use std::process::ExitCode;

use clap::Parser;
use tix_git::cli::{Cli, Command, ConfigAction, TicketAction};
use tix_git::commands::{
    clear_ticket, config_cmd, doctor, handle, init, pr, protect, set_ticket, show, start, ticket,
    uninstall,
};
use tix_git::hooks;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { dry_run, force } => handle(init::run(dry_run, force)),
        Command::Uninstall { dry_run, purge } => handle(uninstall::run(dry_run, purge)),
        Command::Start {
            ticket,
            description,
            base,
        } => handle(start::run(&ticket, description.as_deref(), base.as_deref())),
        Command::SetTicket { ticket, force, yes } => handle(set_ticket::run(&ticket, force, yes)),
        Command::ClearTicket => handle(clear_ticket::run()),
        Command::Show => handle(show::run()),
        Command::Protect { branch, scope } => handle(protect::protect(&branch, scope)),
        Command::Unprotect { branch, scope } => handle(protect::unprotect(&branch, scope)),
        Command::Config { action } => match action {
            ConfigAction::Get { key } => handle(config_cmd::get(&key)),
            ConfigAction::Set {
                key,
                value,
                scope,
                append,
                remove,
            } => handle(config_cmd::set(&key, value, scope, append, remove)),
            ConfigAction::List { global, repo, .. } => {
                let scope = if global {
                    config_cmd::ListScope::Global
                } else if repo {
                    config_cmd::ListScope::Repo
                } else {
                    config_cmd::ListScope::All
                };
                handle(config_cmd::list(scope))
            }
        },
        Command::Doctor { verbose } => match doctor::run(verbose) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(1)
            }
        },
        Command::Pr => handle(pr::run()),
        Command::Ticket { action } => match action {
            None => handle(ticket::run(false)),
            Some(TicketAction::Open) => handle(ticket::run(true)),
        },
        Command::Hook { name, args } => hooks::dispatch(&name, &args),
    }
}
