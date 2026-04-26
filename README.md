# tix

Corporate git workflow assistant — ticket-prefix discipline and branch protection in one Rust binary.

`tix` enforces a few small things every team needs and nothing else:

- Every commit is prefixed with a ticket ID (`POD-1234 fix login`).
- Direct commits and pushes to `main`, `master`, `develop`, `release/*` are blocked.
- New branches are created off the latest base with a consistent name (`feature/POD-1234-fix-login`).
- Pushing a stale branch warns; opening a PR shells out to `gh`/`glab` or prints the URL.

It's offline, has no telemetry, and uninstalls cleanly.

## Install

```sh
cargo install --git https://github.com/ObieMunoz/tix
tix init
```

`tix init` installs three hook shims to `~/.config/tix/hooks/` and points `git`'s global `core.hooksPath` at them. It refuses to overwrite an existing `core.hooksPath` without `--force`. Use `--dry-run` to preview the actions.

## Quick start

```sh
# 1. Install
cargo install --git https://github.com/ObieMunoz/tix
tix init

# 2. Start a branch off the latest main
tix start POD-1234 fix-login
# → feature/POD-1234-fix-login

# 3. Edit, commit — message gets prefixed automatically
git commit -m "drop legacy session token"
# → "POD-1234 drop legacy session token"

# 4. Push and open a PR
git push -u origin HEAD
tix pr  # → opens gh/glab or prints the compare URL
```

If you already have unprefixed commits on a branch:

```sh
tix set-ticket POD-1234
# previews the rewrite, prompts, then rebases. Refuses if any commit
# is already on a remote ref unless --force.
```

For a branch that shouldn't have a ticket:

```sh
tix clear-ticket
```

## What it does

| Feature | How |
|---|---|
| **Ticket prefix on commits** | `prepare-commit-msg` rewrites the first real line of the message to `<TICKET> <subject>`. Idempotent. Skips merge/squash. Leaves a different ticket alone. |
| **First-time prompt** | `pre-commit` prompts on a branch with no state entry. Empty input → no-ticket mode. Non-TTY refuses with a clear hint to run `tix set-ticket` / `tix clear-ticket`. |
| **Protected branches** | `pre-commit` and `pre-push` block direct work on `main`, `master`, `develop`, `release/*`. Glob patterns are single-segment (no `**`). Branch deletion is allowed. `--no-verify` bypasses (see Limitations). |
| **Retroactive amend** | `tix set-ticket POD-1234` rewrites the subject of every unprefixed unpushed commit reachable from `HEAD` via cherry-pick. Body is preserved. Refuses to rewrite remote-side commits unless `--force`. |
| **`tix start`** | Validates ticket, refuses on a dirty tree, fetches `origin/<base>`, creates `<start_prefix>/<TICKET>[-<slug>]`, registers the ticket on the new branch. |
| **Branch-naming check** | Per `branches.naming_enforcement`: `warn` (default) prints to stderr, `block` refuses, `off` skips. |
| **Stale-base warning** | `pre-push` warns when the pushed branch is more than `push.stale_warn_threshold` (default 50) commits behind `origin/<default_base>`. Best-effort; no network → no warning. Never blocks. |
| **`tix pr`** | Detects provider from `origin` (github / gitlab / bitbucket), shells to `gh`/`glab` if installed and `pr_command = auto`, else prints the compare/MR URL. |
| **`tix ticket [open]`** | Substitutes `{ticket}` in `integrations.ticket_url_template` and prints (or opens) the URL. |

## Commands

| Command | Purpose |
|---|---|
| `tix init [--dry-run] [--force]` | Install global hooks, scaffold `~/.config/tix/config.toml`. |
| `tix uninstall [--dry-run] [--purge]` | Remove the managed shims, unset `core.hooksPath` (only if it points at our dir). `--purge` also drops the config file. |
| `tix start <TICKET> [DESCRIPTION] [--base <BRANCH>]` | Create a feature branch off the latest base. |
| `tix set-ticket <TICKET> [--force] [--yes]` | Set/replace ticket on the current branch; offers retroactive amend. |
| `tix clear-ticket` | Put the current branch in no-ticket mode. |
| `tix show` | Print branch, ticket, protected status, base, config sources. |
| `tix protect <BRANCH> [--global \| --repo]` | Add a pattern to the protected list (defaults preserved). |
| `tix unprotect <BRANCH> [--global \| --repo]` | Remove a pattern. |
| `tix config get \| set \| list` | Read / write / list config values. |
| `tix doctor [--verbose]` | Diagnostic checks across the install. |
| `tix pr` | Open a PR for the current branch. |
| `tix ticket [open]` | Print or open the current branch's ticket URL. |

## Configuration

Three layers, last-wins:

1. Built-in defaults (compiled in).
2. Global config: `~/.config/tix/config.toml` (or `$XDG_CONFIG_HOME/tix/config.toml`).
3. Repo config: `<repo>/.tix.toml` — committable, encodes team rules.

Per-clone state (never committed) lives at `<repo>/.git/tix/state.json`.

### Config keys

| Key | Type | Default | Effect |
|---|---|---|---|
| `ticket.pattern` | regex | `^[A-Z]+-\d+$` | What counts as a ticket. |
| `ticket.prefix_format` | string | `{ticket} {message}` | Format used when prefixing. |
| `branches.protected` | list | `["main", "master", "develop", "release/*"]` | Glob patterns blocked from direct commits/pushes. |
| `branches.default_base` | string | `main` | Base branch for `tix start` and the stale-base check. |
| `branches.start_prefix` | string | `feature` | Prefix used in branch names by `tix start`. |
| `branches.naming_pattern` | regex | `^(feature\|bugfix\|hotfix\|chore)/[A-Z]+-\d+(-.+)?$` | Naming check pattern. |
| `branches.naming_enforcement` | enum | `warn` | `warn` / `block` / `off`. |
| `push.stale_warn_threshold` | u32 | `50` | Commits behind base before warning; `0` disables. |
| `integrations.ticket_url_template` | string | `""` | URL template with `{ticket}` (e.g. `https://company.atlassian.net/browse/{ticket}`). |
| `integrations.pr_provider` | string | `github` | Fallback when origin host doesn't match a known provider. |
| `integrations.pr_command` | string | `auto` | `auto` shells to `gh`/`glab` if present; anything else prints the URL. |

### Useful `tix config` invocations

```sh
tix config get branches.default_base
tix config set branches.default_base develop
tix config set integrations.ticket_url_template "https://example.atlassian.net/browse/{ticket}"
tix config set branches.protected --append trunk           # add to the list (idempotent)
tix config set branches.protected --remove trunk           # remove from the list
tix config set branches.start_prefix bug --repo            # team rule, committed in .tix.toml
tix config list                                            # resolved view, with sources
tix config list --global                                   # only what's in ~/.config/tix/config.toml
```

## Uninstall

```sh
tix uninstall            # removes shims + unsets core.hooksPath; keeps config.toml
tix uninstall --purge    # also removes ~/.config/tix/
```

`tix uninstall` only unsets `core.hooksPath` if it points at our managed dir — if you've redirected hooks elsewhere, that setting is left alone. Per-clone state in `<repo>/.git/tix/state.json` is left untouched (the tool can't enumerate clones).

If `tix` is uninstalled but the shims linger, they're a silent no-op — every shim short-circuits when `tix` isn't on PATH.

## Limitations

- **Client-side enforcement.** Hooks bypass with `--no-verify`. Pair with server-side branch protection (GitHub / GitLab settings) for hard guarantees.
- **Single-segment globs.** `release/*` matches `release/1.0` but not `release/1.0/rc1`. No `**`.
- **Unix-only.** macOS and Linux. The hook shims are POSIX shell.
- **Offline.** `tix doctor` checks origin, but no command makes network requests other than `git fetch`.

## Development

```sh
cargo build              # debug build
cargo test               # unit + integration tests (uses real git in temp dirs)
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Tests are end-to-end where possible: each integration test spins up a temp `$HOME`, `$XDG_CONFIG_HOME`, `GIT_CONFIG_GLOBAL`, and a fresh git repo (sometimes with a bare-repo origin). Hook tests exercise the shim → tix → hook → message-rewrite chain via real `git commit` / `git push` with the cargo-built binary on `PATH`.

### Hacking on tix while tix is installed

The repo ships its own `.tix.toml` that turns off branch protection and naming enforcement for this repo only:

```toml
[branches]
protected = []
naming_enforcement = "off"
```

Without it, the maintainer (and any contributor with `tix` installed) couldn't commit to `main` or push release tags without `--no-verify` — the very tool would fight its own dev workflow. Global defaults still apply to every other repo on your machine.

On a **fresh clone**, the per-clone state in `.git/tix/state.json` doesn't exist yet, so the first `git commit` on `main` will prompt for a ticket. Run this once and the prompt won't return:

```sh
tix clear-ticket
```

This puts `main` into "no-ticket mode" for your clone — the entry persists, so subsequent commits skip the prompt. (Per-clone state is intentionally not committed; each contributor decides their own ticket-vs-no-ticket choice.)

## Notes on the SPEC

Two deliberate deviations from `SPEC.md`, documented in commits:

- **Shim form.** The literal SPEC shim (`exec command -v tix > /dev/null && exec tix hook ...`) exits 1 when `tix` is absent — it would block every commit, not silent-no-op. We use `command -v tix > /dev/null 2>&1 || exit 0; exec tix hook ...` instead, which exits 0 when missing.
- **Config dir.** SPEC says `~/.config/tix/config.toml`. `dirs::config_dir()` returns `~/Library/Application Support` on macOS; we honor the SPEC's stated path on every platform via `$XDG_CONFIG_HOME` (or `~/.config/tix`).

## License

MIT. See [LICENSE](LICENSE).
