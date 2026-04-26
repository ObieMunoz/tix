use std::process::ExitCode;

use clap::Parser;
use tix_git::cli::{Cli, Command, ConfigAction, TicketAction};
use tix_git::commands::{handle, init, stub, uninstall};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { dry_run, force } => handle(init::run(dry_run, force)),
        Command::Uninstall { dry_run, purge } => handle(uninstall::run(dry_run, purge)),
        Command::Start { .. } => stub("start"),
        Command::SetTicket { .. } => stub("set-ticket"),
        Command::ClearTicket => stub("clear-ticket"),
        Command::Show => stub("show"),
        Command::Protect { .. } => stub("protect"),
        Command::Unprotect { .. } => stub("unprotect"),
        Command::Config { action } => match action {
            ConfigAction::Get { .. } => stub("config get"),
            ConfigAction::Set { .. } => stub("config set"),
            ConfigAction::List { .. } => stub("config list"),
        },
        Command::Doctor { .. } => stub("doctor"),
        Command::Pr => stub("pr"),
        Command::Ticket { action } => match action {
            None => stub("ticket"),
            Some(TicketAction::Open) => stub("ticket open"),
        },
        Command::Hook { .. } => stub("hook"),
    }
}
