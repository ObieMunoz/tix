use std::process::ExitCode;

pub mod prepare_commit_msg;

pub fn dispatch(name: &str, args: &[String]) -> ExitCode {
    match name {
        "prepare-commit-msg" => match prepare_commit_msg::run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(1)
            }
        },
        // pre-commit and pre-push are stubs until Tasks 3.4 / 4.1 — they
        // must exit 0 so commits and pushes are not blocked while their
        // shims exist on disk.
        "pre-commit" | "pre-push" => ExitCode::SUCCESS,
        other => {
            eprintln!("error: unknown hook `{other}`");
            ExitCode::from(1)
        }
    }
}
