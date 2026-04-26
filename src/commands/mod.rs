use std::process::ExitCode;

pub mod clear_ticket;
pub mod config_cmd;
pub mod doctor;
pub mod init;
pub mod set_ticket;
pub mod show;
pub mod uninstall;

pub fn stub(name: &str) -> ExitCode {
    eprintln!("tix {name}: not yet implemented");
    ExitCode::from(1)
}

pub fn handle(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}
