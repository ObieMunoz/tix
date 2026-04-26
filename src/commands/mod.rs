use std::process::ExitCode;

pub mod clear_ticket;
pub mod config_cmd;
pub mod doctor;
pub mod init;
pub mod pr;
pub mod protect;
pub mod set_ticket;
pub mod show;
pub mod start;
pub mod ticket;
pub mod uninstall;

pub fn handle(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}
