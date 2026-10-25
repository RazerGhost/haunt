//! Running a program with a time limit: its output read as it comes (so a full pipe never stalls it), and the program
//! stopped when it takes longer than allowed. For git's reads, the host tools (`gh`, `glab`, `tea`) and the like.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
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

fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut out);
        }
        out
    })
}

/// Waits for `child` until `limit`, checking again after a tenth of the time it has taken so far (so a quick program
/// is seen finishing within a fraction of a millisecond, and the wait never adds more than about a tenth); stops it
/// when the time is up.
fn wait(child: &mut Child, limit: Duration) -> Result<ExitStatus, Failure> {
    let started = Instant::now();
    loop {
        let pause = (started.elapsed() / 10).clamp(Duration::from_micros(200), Duration::from_millis(50));
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::TimedOut);
            }
            Ok(None) => std::thread::sleep(pause),
            Err(error) => return Err(Failure::Other(error.to_string())),
        }
    }
}

/// Runs `command` (its stdin is the caller's to set) with its stdout and stderr captured, for at most `limit`.
pub fn run(mut command: Command, limit: Duration) -> Result<Finished, Failure> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::startup::hide_console(&mut command);
    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => Failure::NotFound,
        _ => Failure::Other(error.to_string()),
    })?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = wait(&mut child, limit);
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(Finished { status: status?, stdout, stderr })
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
}
