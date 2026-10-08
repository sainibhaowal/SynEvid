//! autopsy binary entrypoint.

fn main() {
    let exit_code = autopsy_cli::run_from_env();
    std::process::exit(exit_code);
}
