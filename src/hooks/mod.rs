use std::process::ExitCode;

pub mod pre_commit;
pub mod prepare_commit_msg;

pub fn dispatch(name: &str, args: &[String]) -> ExitCode {
    let result = match name {
        "prepare-commit-msg" => prepare_commit_msg::run(args),
        "pre-commit" => pre_commit::run(),
        // pre-push remains a no-op stub until Task 4.1 implements it —
        // must exit 0 so pushes are not blocked while the shim exists.
        "pre-push" => return ExitCode::SUCCESS,
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
