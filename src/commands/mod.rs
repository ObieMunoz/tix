use std::process::ExitCode;

pub fn stub(name: &str) -> ExitCode {
    eprintln!("tix {name}: not yet implemented");
    ExitCode::from(1)
}
