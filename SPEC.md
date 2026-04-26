# Spec: tix

## Objective

A Rust CLI tool plus globally-installed git hooks that helps individual developers — and eventually teams — work safely against corporate git workflow rules without manual ceremony.

**Primary user:** the author (single-developer first), then a small team (≤ ~20 engineers).

**Problem it solves:** corporate git workflows demand ticket-prefixed commits, protected branches, naming conventions, and PR hygiene. Doing all of this by hand is tedious and easy to forget. This tool automates the mechanical parts and blocks the dangerous ones.

**Form factor:** single static binary (`tix`) installed via `cargo install` (later: Homebrew tap). Ships a set of git hooks installed once globally via `git config --global core.hooksPath`. Hooks are thin shims that invoke `tix hook <name>`.

## Tech Stack

- **Language:** Rust, 2024 edition.
- **CLI:** `clap` v4 with derive macros.
- **Config & state serialization:** `serde` + `toml` (config) + `serde_json` (state, for atomic writes).
- **Errors:** `anyhow` for the binary's top-level, `thiserror` for any library-style modules.
- **Regex:** `regex` crate.
- **Filesystem paths:** `dirs` for cross-platform config home.
- **Time:** `chrono` for ISO-8601 timestamps.
- **Subprocess:** stdlib `std::process::Command`. We **shell out to `git`** for all git operations rather than using `gix` — keeps the dep surface tiny and makes behavior identical to whatever `git` the user has installed.
- **Testing:** stdlib + `assert_cmd` + `predicates` + `tempfile` for ephemeral git repos.

### Crate vs binary name

The crate name `tix` is taken on crates.io by an unrelated 12-day-old toy project. We sidestep this by:

- **Crate name:** `tix-git` (free on crates.io, but we won't publish there in v1).
- **Binary name:** `tix` — set via `[[bin]]` in `Cargo.toml`.
- **Distribution:** `cargo install --git https://github.com/ObieMunoz/tix` only. No crates.io publication. (Homebrew tap deferred to v2.)

```toml
[package]
name = "tix-git"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "tix"
path = "src/main.rs"
```

## Commands

### Build / dev

```
Build (debug):       cargo build
Build (release):     cargo build --release
Run tests:           cargo test
Lint:                cargo clippy --all-targets -- -D warnings
Format check:        cargo fmt --check
Format apply:        cargo fmt
Install locally:     cargo install --path .
Install from git:    cargo install --git https://github.com/ObieMunoz/tix
```

### `tix` CLI surface

```
tix init                                  # install global hooks, scaffold global config
tix start <TICKET> [DESCRIPTION] [--base <BRANCH>]  # pull base, create branch, register ticket
tix set-ticket <TICKET>                   # set/update ticket for current branch (offers retroactive amend)
tix clear-ticket                          # set current branch to "no-ticket" mode
tix show                                  # show current branch, ticket, protected status, base, config sources
tix protect <BRANCH> [--global|--repo]    # add protected branch
tix unprotect <BRANCH> [--global|--repo]  # remove protected branch
tix config get|set|list <KEY> [VALUE]     # read/write config values
tix doctor                                # run diagnostic checks
tix pr                                    # open PR creation flow for current branch
tix ticket [open]                         # open the current branch's ticket URL in browser
tix hook <NAME> [HOOK_ARGS...]            # internal: invoked by installed git hooks
```

`hook` is the only subcommand users don't type directly — git invokes it via the hook shims.

## Project Structure

```
tix/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── SPEC.md                          # this file
├── src/
│   ├── main.rs                      # entrypoint, dispatch
│   ├── cli.rs                       # clap definitions
│   ├── config/
│   │   ├── mod.rs                   # layered config resolver (global + repo)
│   │   ├── global.rs                # ~/.config/tix/config.toml
│   │   ├── repo.rs                  # <repo>/.tix.toml
│   │   └── state.rs                 # <repo>/.git/tix/state.json
│   ├── git/
│   │   ├── mod.rs
│   │   └── shell.rs                 # `git` subprocess wrapper
│   ├── hooks/
│   │   ├── mod.rs
│   │   ├── prepare_commit_msg.rs    # ticket prefix logic
│   │   ├── pre_commit.rs            # protected-branch + ticket-presence check
│   │   └── pre_push.rs              # protected-branch + stale-base check
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── init.rs
│   │   ├── start.rs
│   │   ├── set_ticket.rs            # includes retroactive-amend logic
│   │   ├── clear_ticket.rs
│   │   ├── show.rs
│   │   ├── protect.rs
│   │   ├── doctor.rs
│   │   ├── pr.rs
│   │   ├── ticket.rs
│   │   └── config_cmd.rs
│   └── util/
│       ├── mod.rs
│       ├── prompt.rs                # interactive y/n + line input
│       └── ticket.rs                # parsing/validation helpers
└── tests/
    ├── common/
    │   └── mod.rs                   # tempdir + git init helpers
    └── integration/
        ├── commit_prefix.rs
        ├── protected_branches.rs
        ├── retroactive_amend.rs
        ├── start_command.rs
        ├── stale_base.rs
        ├── two_tier_config.rs
        └── doctor.rs
```

## Code Style

Standard Rust idioms (`rustfmt` defaults, clippy clean). Errors propagate with `?`; the binary's top frames produce user-facing messages with `anyhow::Context`. Public types in `src/config/` use `thiserror` so callers can match on error variants.

Example (illustrative — final code may differ):

```rust
use anyhow::{Context, Result};

pub fn prefix_message(message: &str, ticket: &str, pattern: &Regex) -> String {
    if pattern.is_match(message.split_whitespace().next().unwrap_or("")) {
        // Already prefixed with some ticket — leave alone.
        return message.to_string();
    }
    format!("{ticket} {message}")
}

pub fn load_repo_config(repo_root: &Path) -> Result<Option<RepoConfig>> {
    let path = repo_root.join(".tix.toml");
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let config: RepoConfig = toml::from_str(&contents)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(config))
}
```

Conventions:
- Module names: `snake_case`. Types: `UpperCamelCase`. Functions/vars: `snake_case`.
- One concept per file. `mod.rs` is dispatch only.
- Output to stderr for diagnostics, stdout for results consumed by other tools.
- Color via `anstream` only when stdout/stderr is a TTY.

## Configuration & State

Three layers, resolved at runtime by merging in this order (last wins):

1. **Built-in defaults** (compiled in).
2. **Global config:** `~/.config/tix/config.toml` — user-wide preferences.
3. **Repo config:** `<repo>/.tix.toml` — committed in the repo, encodes team rules. Overrides global.

State (per-clone, never committed) lives at `<repo>/.git/tix/state.json`.

### Default global config

```toml
[ticket]
pattern = '^[A-Z]+-\d+$'              # POD-12345, DPT-3589, etc.
prefix_format = "{ticket} {message}"  # single space, no colon

[branches]
protected = ["main", "master", "develop", "release/*"]
default_base = "main"
start_prefix = "feature"              # branch prefix used by `tix start`
naming_pattern = '^(feature|bugfix|hotfix|chore)/[A-Z]+-\d+(-.+)?$'
naming_enforcement = "warn"           # "warn" | "block" | "off"

[push]
stale_warn_threshold = 50             # commits behind base; 0 disables

[integrations]
ticket_url_template = ""              # e.g., "https://company.atlassian.net/browse/{ticket}"
pr_provider = "github"                # "github" | "gitlab" | "bitbucket"
pr_command = "auto"                   # "auto" uses gh/glab if present; "url" prints URL only
```

### State file shape

```json
{
  "version": 1,
  "branches": {
    "feature/POD-1234-fix-thing": {
      "ticket": "POD-1234",
      "set_at": "2026-04-26T12:34:56Z",
      "amended_through": "abc123def456"
    },
    "hotfix/scratch": {
      "ticket": null,
      "set_at": "2026-04-26T13:00:00Z"
    }
  }
}
```

`ticket: null` is the "no-ticket" sentinel — distinct from the branch being absent (never seen before). Writes use atomic temp-file + rename.

## Feature Behavior

### 1. Ticket prefix on commits (`prepare-commit-msg` hook)

- Trigger: any commit on a branch whose state has a non-null `ticket`.
- Idempotency: if the message's first whitespace-delimited token already matches the configured ticket pattern (any ticket, not just the current one), do nothing. This honors the "leave a different ticket alone" rule.
- Skip: merge commits (detected via the hook's source argument).
- Apply to: `git commit -m`, interactive `git commit`, `--amend`, `revert` commits.
- Format: `{ticket} {message}` (configurable).

### 2. Per-branch ticket prompting (`pre-commit` hook + commands)

- On first commit/push to a branch with no state entry: hook prompts on TTY for a ticket. Options: enter ticket, enter blank for "no-ticket" mode, or `Ctrl-C` to cancel.
- "No-ticket" mode: state stores `ticket: null`. Hook never re-prompts.
- Explicit override: `tix set-ticket POD-1234` updates the state and triggers the retroactive-amend flow.
- Branch state persists per clone, in `.git/tix/state.json`.
- Branch lookup: when checking out a branch we've seen before, we use the existing entry. No fuzzy/related-ticket lookup in v1.

### 3. Protected branches (`pre-commit` and `pre-push` hooks)

- Default protected: `main`, `master`, `develop`, plus glob `release/*`.
- `pre-commit` blocks any direct commit on a protected branch.
- `pre-push` blocks any push that updates a protected branch's ref.
- Glob support: `*` matches any number of non-`/` chars; full segment globs like `release/*`. No `**`.
- Protections are configurable globally and per-repo. Repo overrides global.
- Error message includes the offending branch and the override hint (`--no-verify` for commits, branch deletion not blocked).

### 4. Retroactive amend

Triggered by `tix set-ticket POD-1234` when the branch has commits that:

- Don't already have a ticket prefix matching the pattern, **and**
- Are not present on the configured remote (computed via `git for-each-ref` + `merge-base`).

Flow:

1. Compute the list of unpushed, unprefixed commits reachable from `HEAD`.
2. Show the list with short SHA + subject.
3. Prompt: `amend N commits to prefix POD-1234? [y/N]`.
4. On `y`: run `git rebase` with an exec script that rewrites each subject. Record the resulting `HEAD` SHA in state as `amended_through`.
5. Hard-stop with a clear error if any candidate commit is found on the remote, unless `--force` is passed (and even then, require an extra confirmation since this would later require force-push).

### 5. `tix start <TICKET> [DESCRIPTION] [--base <BRANCH>]`

1. Validate ticket against `ticket.pattern`.
2. Resolve base branch: `--base` flag if given, else `branches.default_base` config (default `main`).
3. `git fetch origin <base>`.
4. Verify working tree is clean (block with clear error if not).
5. Create branch named `feature/<TICKET>[-<slug(description)>]` off `origin/<base>` (prefix configurable, default `feature`).
6. Register the ticket in state.
7. Print a confirmation including the new branch name and the base it was branched from.

`<DESCRIPTION>` is slugified: lowercased, non-alphanumerics → `-`, collapsed, trimmed, capped at 40 chars.

### 6. Branch-name convention check

- Configurable regex (`branches.naming_pattern`).
- Enforcement modes: `warn` (default), `block`, `off`.
- Checked at:
  - **`tix start`**: always produces a compliant name; no check needed.
  - **`pre-commit`**: if the current branch name doesn't match and mode is `warn`, print a one-line warning. If `block`, refuse the commit with a rename hint.
  - **`pre-push`**: same logic, applied per pushed ref.
- We deliberately don't hook `post-checkout` in v1 — too noisy and can't prevent the creation anyway.

### 7. Stale-base warning (`pre-push` hook)

- Compute `git rev-list --count <branch>..origin/<default_base>` after a `git fetch` (best-effort; skip if fetch fails — we don't want network issues to block pushes).
- If count > `push.stale_warn_threshold` (default 50, 0 disables): print a yellow warning with the rebase hint. Warning only — never blocks.

### 8. Two-tier config

- `tix init` scaffolds `~/.config/tix/config.toml` with defaults.
- A repo opts in to team rules by checking in `.tix.toml` at the repo root. The tool reads it automatically — no per-clone setup required beyond having `tix` installed.
- `tix show` displays the resolved (merged) config and labels each value's source.

### 9. `tix doctor`

Checks, in order:

- `git --version` available and ≥ 2.30.
- `core.hooksPath` is set globally and points to our managed hooks dir.
- All three hook shims exist, are executable, and reference a discoverable `tix` binary.
- Global config file parses.
- If inside a repo: repo config file parses (if present), state file parses (if present), `default_base` exists as a remote ref.
- Optional: GPG/SSH signing keys present if `commit.gpgsign` is enabled (informational, not required).

Each check prints `OK`/`WARN`/`FAIL` with a one-line remediation hint.

### 10. PR creation helper (`tix pr`)

- Detects provider from `origin` URL (github.com / gitlab.com / *.bitbucket.org), or uses `integrations.pr_provider`.
- If `pr_command = "auto"` and the provider's CLI is on `PATH` (`gh` for GitHub, `glab` for GitLab), shell out to it and pass the current branch's ticket as the PR title prefix.
- Otherwise print the provider's PR-creation URL with the branch pre-filled.
- Never opens a PR without explicit user action — `tix pr` is the action.

### 11. Open ticket in browser (`tix ticket [open]`)

- Uses `integrations.ticket_url_template` (e.g., `https://company.atlassian.net/browse/{ticket}`).
- Reads the current branch's ticket from state. Errors clearly if no template is configured or the branch has no ticket.
- `tix ticket` prints the URL; `tix ticket open` calls the platform's open command (`open` on macOS, `xdg-open` on Linux).

## Testing Strategy

- **Unit tests** colocated with code via `#[cfg(test)] mod tests` for pure functions: prefix logic, glob matching, slugification, regex parsing, config merging.
- **Integration tests** in `tests/integration/`. Each test:
  - Creates a temp directory via `tempfile::TempDir`.
  - Initializes a real git repo.
  - Sets `core.hooksPath` to our hook dir for that test.
  - Invokes the `tix` binary via `assert_cmd` and asserts on output, exit code, and resulting git/file state.
- **Coverage targets:** every hook path, every CLI subcommand, both config layers, the retroactive-amend success and unpushed-only-rejection paths.
- **No mocking** of git itself — tests run the real `git` binary against real (temp) repos. Slower but catches actual behavior.
- **CI matrix:** macOS + Ubuntu, stable Rust. (Windows deferred — git hooks on Windows have enough quirks to be a separate effort.)

## Boundaries

### Always do

- Validate any ticket value against the configured pattern before storing or applying it.
- Write state files atomically (temp file + `rename`).
- Print actionable error messages — every failure should suggest a next step.
- Run `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test` before any commit.
- Honor `--no-verify` silently — we cannot prevent it and shouldn't try.
- Treat `.git/tix/` as ours; treat the rest of `.git/` as git's.

### Ask first

- Adding any new dependency, especially anything network-touching or > ~100KB compiled.
- Changing the on-disk shape of `config.toml`, `.tix.toml`, or `state.json` after v1 ships.
- Changing the CLI surface (renaming subcommands, removing flags).
- Anything that would invoke `git rebase`, `git reset --hard`, or `git push --force`.
- Modifying the user's `~/.gitconfig` — only `tix init` does this, and only for `core.hooksPath`.

### Never do

- Force-push without explicit user consent + an extra confirmation prompt.
- Modify commits that exist on a remote branch.
- Skip git hooks via `--no-verify` from inside `tix` itself.
- Edit files inside `.git/` other than `.git/tix/`.
- Commit or transmit the user's ticket data anywhere off-machine.
- Block pushes for purely advisory reasons (stale-base, naming-warn) — only `block` mode and protected-branch rules block.

## Success Criteria

- All v1 features pass integration tests on macOS and Linux.
- `tix init` on a fresh machine takes < 5 seconds and requires no manual file edits.
- Cold-start CLI invocation < 100 ms on M-series Mac for `show`, `set-ticket`, and the hook subcommands.
- Stripped release binary < 5 MB.
- Hooks gracefully no-op (exit 0 with a stderr note) if the `tix` binary is missing — uninstalling the tool must not break git.
- Retroactive amend never touches commits that exist on the remote without `--force` plus confirmation.
- `tix doctor` correctly identifies a misconfigured installation (verified via tests that intentionally break each invariant).

## Resolved Decisions

- **Windows:** out of scope for v1. macOS + Linux only.
- **Branch-name enforcement default:** `warn`. Repos can opt into `block` per-repo via `.tix.toml`.
- **`tix start --base <BRANCH>`:** supported in v1.
- **Distribution:** `cargo install --git https://github.com/ObieMunoz/tix` only. No crates.io publication. Homebrew tap deferred to v2.
- **Cuts (parked v2):** pre-push secret scanning, large-file warning.

## v2+ Parking Lot

Not in scope for v1, but on the radar:

- Pre-push secret scanning (regex-based: AWS keys, GitHub tokens, JWT-shaped strings, `BEGIN PRIVATE KEY`, `.env` patterns).
- Large-file warning before commit/push.
- WIP/fixup detection on push to non-personal branches.
- `--no-verify` audit log (local-only).
- Branch cleanup (`tix cleanup` deletes locals whose remote branch is gone).
- Subject-line linting (length, conventional commits).
- Trailers helper (`Co-authored-by:`, `Signed-off-by:`).
- Live Jira/Linear API integration to fetch ticket titles.
- Activity log per ticket (commit timestamps for standup/EOW reports).
- `tix self-update`.
- Windows support.
