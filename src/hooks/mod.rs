use std::process::ExitCode;

pub mod pre_commit;
pub mod pre_push;
pub mod prepare_commit_msg;

pub fn dispatch(name: &str, args: &[String]) -> ExitCode {
    let result = match name {
        "prepare-commit-msg" => prepare_commit_msg::run(args),
        "pre-commit" => pre_commit::run(),
        "pre-push" => pre_push::run(args),
        other => {
            eprintln!("error: unknown hook `{other}`");
            return ExitCode::from(1);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}
