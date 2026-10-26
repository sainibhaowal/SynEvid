//! autopsy binary entrypoint.

fn main() {
    // Ensure sufficient stack space across platforms (Windows MSVC defaults to 1MB,
    // which can overflow in debug builds when parsing large command ASTs).
    let builder = std::thread::Builder::new().stack_size(8 * 1024 * 1024);
    let handler = builder
        .spawn(|| autopsy_cli::run_from_env())
        .expect("failed to spawn main thread");
    let exit_code = handler.join().unwrap_or(3);
    std::process::exit(exit_code);
}
