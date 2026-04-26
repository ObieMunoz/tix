use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::{Result, anyhow};

pub fn confirm(question: &str, default: bool) -> Result<bool> {
    require_tty()?;
    let suffix = if default { "[Y/n]" } else { "[y/N]" };
    print!("{question} {suffix} ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    match trimmed.as_str() {
        "" => Ok(default),
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        other => Err(anyhow!("invalid response: {other:?}")),
    }
}

pub fn line(question: &str) -> Result<String> {
    require_tty()?;
    print!("{question} ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn require_tty() -> Result<()> {
    if io::stdin().is_terminal() {
        Ok(())
    } else {
        Err(anyhow!(
            "cannot prompt: stdin is not a terminal (use `tix set-ticket` or `tix clear-ticket`)"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_errors_without_tty() {
        let err = confirm("ok?", true).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("not a terminal"), "expected TTY error: {msg}");
    }

    #[test]
    fn line_errors_without_tty() {
        let err = line("name?").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("not a terminal"), "expected TTY error: {msg}");
    }

    #[test]
    fn tty_error_includes_command_hint() {
        let err = confirm("ok?", true).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("tix set-ticket") || msg.contains("tix clear-ticket"),
            "expected command hint: {msg}"
        );
    }
}
