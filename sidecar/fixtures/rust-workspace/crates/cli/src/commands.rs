use app_core::version;

pub fn run(n: u32) {
    let shift = |x: u32| x + version();
    shift(n);
}
