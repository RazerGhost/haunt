//! Running a program with a time limit: its output read as it comes (so a full pipe never stalls it), and the program
//! stopped when it takes longer than allowed. For git's reads and writes, the host tools (`gh`, `glab`, `tea`) and the
//! like. Stopping a program stops everything it started too (ssh, a credential helper, a hook): on Unix each program
//! runs in its own process group for that.

use parking_lot::Mutex;
use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// What a program left when it finished.
pub struct Finished {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub enum Failure {
    /// The program is not installed (not on the PATH).
    NotFound,
    /// It took longer than allowed, and was stopped.
    TimedOut,
    /// It could not start, or could not be waited on.
    Other(String),
}

/// How long the output is still read after the program ended: a process it started that lives on (a hook's, or one
/// a stop could not reach) may hold the pipes open, and must not hold the caller up.
const AFTER_EXIT: Duration = Duration::from_secs(2);

/// A pipe read aside as it comes.
struct Drain {
    out: Arc<Mutex<Vec<u8>>>,
    done: mpsc::Receiver<()>,
}

impl Drain {
    fn new(pipe: Option<impl Read + Send + 'static>) -> Drain {
        let out = Arc::new(Mutex::new(Vec::new()));
        let (sender, done) = mpsc::channel();
        let kept = out.clone();
        std::thread::spawn(move || {
            if let Some(mut pipe) = pipe {
                let mut buffer = [0u8; 8192];
                while let Ok(count @ 1..) = pipe.read(&mut buffer) {
                    kept.lock().extend_from_slice(&buffer[..count]);
                }
            }
            let _ = sender.send(());
        });
        Drain { out, done }
    }

    /// What was read, once the pipe closed or `until` passed.
    fn take(self, until: Instant) -> Vec<u8> {
        let _ = self.done.recv_timeout(until.saturating_duration_since(Instant::now()));
        std::mem::take(&mut *self.out.lock())
    }
}

/// A Windows job object: every process started from a program in it is in it too, so stopping the job stops them all
/// (`taskkill /T` follows parent ids, which a hook's MSYS shell loses when it runs a command).
#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn TerminateJobObject(job: *mut c_void, exit_code: u32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    pub struct Job(*mut c_void);

    // A job handle may be used and closed from any thread.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        /// A new job holding `child` (and what it starts from now on); None when Windows refuses.
        pub fn holding(child: &std::process::Child) -> Option<Job> {
            // SAFETY: a new unnamed job with default security; the handle is owned by the Job and closed on drop.
            let job = Job(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) });
            if job.0.is_null() {
                return None;
            }
            // SAFETY: both handles are open: the job's is owned here, the process's by `child`, which outlives this call.
            let assigned = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) };
            (assigned != 0).then_some(job)
        }

        /// Stops every process in the job.
        pub fn stop(&self) -> bool {
            // SAFETY: the handle is open until the Job is dropped.
            unsafe { TerminateJobObject(self.0, 1) != 0 }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle is owned here and closed once; closing does not stop the processes in the job.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// The jobs of the programs running now, by process id (Windows).
#[cfg(windows)]
fn jobs() -> &'static Mutex<std::collections::HashMap<u32, Arc<job::Job>>> {
    static JOBS: std::sync::OnceLock<Mutex<std::collections::HashMap<u32, Arc<job::Job>>>> = std::sync::OnceLock::new();
    JOBS.get_or_init(Mutex::default)
}

/// Keeps track of what `child` starts, so `kill_tree` stops it all; `release` when it has ended. Right after it is
/// started: what it started before this is not followed on Windows.
pub fn adopt(child: &Child) {
    #[cfg(windows)]
    if let Some(job) = job::Job::holding(child) {
        jobs().lock().insert(child.id(), Arc::new(job));
    }
    #[cfg(not(windows))]
    let _ = child;
}

/// Lets go of a program `adopt`ed, once it has ended (its id may be another program's next).
pub fn release(pid: u32) {
    #[cfg(windows)]
    jobs().lock().remove(&pid);
    #[cfg(not(windows))]
    let _ = pid;
}

/// Stops the program `pid` and every process it started: on Windows its job (`adopt`) and `taskkill /T`, on Unix its
/// process group (the program must have been started in its own group, `own_group`). Whether the stop could be sent.
pub fn kill_tree(pid: u32) -> bool {
    #[cfg(windows)]
    let job = jobs().lock().get(&pid).cloned().is_some_and(|job| job.stop());
    #[cfg(not(windows))]
    let job = false;
    #[cfg(windows)]
    let mut kill = {
        let mut command = Command::new("taskkill");
        command.args(["/T", "/F", "/PID", &pid.to_string()]);
        command
    };
    #[cfg(not(windows))]
    let mut kill = {
        let mut command = Command::new("kill");
        command.args(["-TERM", &format!("-{pid}")]);
        command
    };
    kill.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    crate::startup::hide_console(&mut kill);
    kill.status().is_ok() || job
}

/// Starts the program in a process group of its own on Unix, so `kill_tree` stops what it starts too; nothing on
/// Windows, where the job does that (`adopt`).
pub fn own_group(command: &mut Command) {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    #[cfg(not(unix))]
    let _ = command;
}

/// Waits for `child` until `limit`, checking again after a tenth of the time it has taken so far (so a quick program
/// is seen finishing within a fraction of a millisecond, and the wait never adds more than about a tenth); stops it,
/// with what it started, when the time is up.
fn wait(child: &mut Child, limit: Duration) -> Result<ExitStatus, Failure> {
    let started = Instant::now();
    loop {
        let pause = (started.elapsed() / 10).clamp(Duration::from_micros(200), Duration::from_millis(50));
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() >= limit => {
                kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::TimedOut);
            }
            Ok(None) => std::thread::sleep(pause),
            Err(error) => return Err(Failure::Other(error.to_string())),
        }
    }
}

/// Starts `command` (its stdin is the caller's to set) with its stdout and stderr captured, in its own process group,
/// `adopt`ed: hand it to `finish`.
pub fn start(mut command: Command) -> Result<Child, Failure> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::startup::hide_console(&mut command);
    own_group(&mut command);
    let child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => Failure::NotFound,
        _ => Failure::Other(error.to_string()),
    })?;
    adopt(&child);
    Ok(child)
}

/// Reads what a program `start`ed says and waits for it, for at most `limit`.
pub fn finish(mut child: Child, limit: Duration) -> Result<Finished, Failure> {
    let stdout = Drain::new(child.stdout.take());
    let stderr = Drain::new(child.stderr.take());
    let status = wait(&mut child, limit);
    release(child.id());
    let until = Instant::now() + AFTER_EXIT;
    let stdout = stdout.take(until);
    let stderr = stderr.take(until);
    Ok(Finished { status: status?, stdout, stderr })
}

/// Runs `command` (its stdin is the caller's to set) with its stdout and stderr captured, for at most `limit`.
pub fn run(command: Command, limit: Duration) -> Result<Finished, Failure> {
    finish(start(command)?, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_stops_a_program_that_takes_too_long() {
        let mut quick = Command::new("git");
        quick.arg("--version").stdin(Stdio::null());
        let done = run(quick, Duration::from_secs(20)).unwrap();
        assert!(done.status.success() && String::from_utf8_lossy(&done.stdout).starts_with("git version"));

        // A program that waits on its stdin forever.
        let mut slow = Command::new("git");
        slow.args(["hash-object", "--stdin"]).stdin(Stdio::piped());
        let started = Instant::now();
        assert_eq!(run(slow, Duration::from_millis(300)).err(), Some(Failure::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(5));

        assert_eq!(run(Command::new("haunt-no-such-program"), Duration::from_secs(1)).err(), Some(Failure::NotFound));
    }

    /// A timeout stops what the program started too: here cmd's ping.
    #[cfg(windows)]
    #[test]
    fn a_timeout_stops_the_whole_tree() {
        let mut slow = Command::new("cmd");
        slow.args(["/c", "ping", "-n", "30", "127.0.0.1"]).stdin(Stdio::null());
        let started = Instant::now();
        let child = start(slow).unwrap();
        let cmd = child.id();
        // The ping cmd started, found by its parent.
        let ping = (0..50).find_map(|_| {
            let mut find = Command::new("powershell");
            find.args([
                "-NoProfile",
                "-Command",
                &format!(
                    "(Get-CimInstance Win32_Process -Filter \"ParentProcessId={cmd} and Name='PING.EXE'\").ProcessId"
                ),
            ]);
            let out = find.output().ok()?;
            let found = String::from_utf8_lossy(&out.stdout).trim().parse::<u32>().ok();
            if found.is_none() {
                std::thread::sleep(Duration::from_millis(100));
            }
            found
        });
        let waiting = Instant::now();
        assert_eq!(finish(child, Duration::from_secs(1)).err(), Some(Failure::TimedOut));
        assert!(waiting.elapsed() < Duration::from_secs(10), "waited for the ping: {:?}", waiting.elapsed());
        assert!(started.elapsed() < Duration::from_secs(25));
        let ping = ping.expect("the ping was not found");
        let mut list = Command::new("tasklist");
        list.args(["/FI", &format!("PID eq {ping}"), "/NH"]);
        let listed = String::from_utf8_lossy(&list.output().unwrap().stdout).into_owned();
        assert!(!listed.contains(&ping.to_string()), "the ping still runs: {listed}");
    }
}
