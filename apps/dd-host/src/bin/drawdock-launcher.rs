#![windows_subsystem = "windows"]

#[path = "../host_environment.rs"]
mod host_environment;
#[path = "../launcher/mod.rs"]
mod launcher;

fn main() -> std::process::ExitCode {
    if let Err(error) = launcher::run() {
        launcher::show_error(&error);
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
