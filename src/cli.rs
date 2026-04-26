use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "tix", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Install global hooks and scaffold global config.
    Init {
        /// Print actions without making changes.
        #[arg(long)]
        dry_run: bool,
        /// Overwrite an existing core.hooksPath if set.
        #[arg(long)]
        force: bool,
    },

    /// Create a branch off the latest base and register the ticket.
    Start {
        ticket: String,
        description: Option<String>,
        /// Base branch to fork off (defaults to `branches.default_base`).
        #[arg(long, value_name = "BRANCH")]
        base: Option<String>,
    },

    /// Set the ticket for the current branch (offers retroactive amend).
    #[command(name = "set-ticket")]
    SetTicket {
        ticket: String,
        /// Allow rewriting commits already on the remote.
        #[arg(long)]
        force: bool,
    },

    /// Clear the ticket for the current branch (no-ticket mode).
    #[command(name = "clear-ticket")]
    ClearTicket,

    /// Show current branch, ticket, protected status, base, config sources.
    Show,

    /// Add a branch pattern to the protected list.
    Protect {
        branch: String,
        #[command(flatten)]
        scope: ScopeFlags,
    },

    /// Remove a branch pattern from the protected list.
    Unprotect {
        branch: String,
        #[command(flatten)]
        scope: ScopeFlags,
    },

    /// Read or write config values.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Run diagnostic checks.
    Doctor {
        #[arg(long)]
        verbose: bool,
    },

    /// Open a PR for the current branch.
    Pr,

    /// Print or open the current branch's ticket URL.
    Ticket {
        #[command(subcommand)]
        action: Option<TicketAction>,
    },

    /// Internal: invoked by installed git hooks.
    Hook {
        name: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Print a single value with its source.
    Get { key: String },

    /// Write a value (or list mutation) into a config file.
    Set {
        key: String,
        value: Option<String>,
        #[command(flatten)]
        scope: ScopeFlags,
        /// Append a value to a list-typed key.
        #[arg(long, value_name = "VALUE")]
        append: Option<String>,
        /// Remove a value from a list-typed key.
        #[arg(long, value_name = "VALUE")]
        remove: Option<String>,
    },

    /// Print every key with its value and source.
    List {
        #[arg(long)]
        global: bool,
        #[arg(long)]
        repo: bool,
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum TicketAction {
    /// Open the ticket URL in the default browser.
    Open,
}

#[derive(clap::Args, Debug)]
#[group(multiple = false)]
pub struct ScopeFlags {
    /// Apply to the global config file.
    #[arg(long)]
    pub global: bool,
    /// Apply to the repo config file (`.tix.toml`).
    #[arg(long)]
    pub repo: bool,
}
