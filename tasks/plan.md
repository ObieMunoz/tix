# Implementation Plan: tix

> Source of truth: `SPEC.md`. This plan operationalizes the spec into ordered, verifiable tasks. Phases produce vertical slices — each leaves the system in a working, demoable state.

## Overview

`tix` is a Rust CLI + git-hook bundle that automates ticket-prefix discipline and protects branches in corporate git workflows. v1 ships seven user-facing features (ticket prefix, first-time prompt, protected branches, retroactive amend, `tix start`, branch-name check, stale-base warning, PR helper, ticket browser) plus install (`init`) and diagnostics (`doctor`). All git work shells out to `git`; per-clone state lives in `.git/tix/state.json`; config is layered (defaults → global → repo).

## Architecture Decisions (locked)

- **Crate `tix-git`, binary `tix`** — sidesteps the squatted `tix` crate. Distributed via `cargo install --git`.
- **Shell out to `git`** — no `gix` dep. Smaller binary, identical to user's `git` behavior.
- **One Rust binary, hooks are 3-line shell shims** — shims call `tix hook <name>`. Trivial to ship, easy to no-op if `tix` is uninstalled.
- **Config layering: defaults → global TOML → repo TOML** — last wins. Repo file is committed; global is per-user.
- **State in `.git/tix/state.json`** — per clone, never committed, atomic writes via temp+rename.
- **Real git in tests** — no mocking. Each integration test spins up a temp repo.

## Dependency Graph

```
                    ┌─────────────────┐
                    │  Cargo + crates │  (Phase 0)
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────────┐
        │                    │                        │
   ┌────▼─────┐        ┌─────▼─────┐           ┌──────▼──────┐
   │  Config  │        │   State   │           │ Git wrapper │  (Phase 1)
   │  loader  │        │   R/W     │           │ (subprocess)│
   └────┬─────┘        └─────┬─────┘           └──────┬──────┘
        │                    │                        │
        └────────────┬───────┴────────────────┬───────┘
                     │                        │
                ┌────▼────┐              ┌────▼────┐
                │  Utils  │              │   CLI   │  (Phase 1)
                │ (slug,  │              │ scaffold│
                │ regex,  │              │  (clap) │
                │ prompt, │              └────┬────┘
                │ glob)   │                   │
                └────┬────┘                   │
                     └──────────┬─────────────┘
                                │
        ┌───────────────────────┼─────────────────────────┐
        │                       │                         │
  ┌─────▼─────┐           ┌─────▼─────┐            ┌──────▼──────┐
  │ tix init  │           │ tix show  │            │ tix doctor  │  (Phase 2)
  │ (writes   │           │ (read     │            │ (verify)    │
  │  hooks +  │           │  state +  │            │             │
  │  config)  │           │  config)  │            │             │
  └─────┬─────┘           └───────────┘            └─────────────┘
        │
        └─────────────────────┬───────────────────────────────────┐
                              │                                   │
                    ┌─────────▼──────────┐               ┌────────▼────────┐
                    │ set/clear-ticket   │               │ prepare-commit- │  (Phase 3)
                    │ (state writes)     │               │ msg hook        │
                    └─────────┬──────────┘               │ (uses state)    │
                              │                          └────────┬────────┘
                              │                                   │
                    ┌─────────▼──────────┐               ┌────────▼────────┐
                    │ retroactive amend  │               │ pre-commit:     │
                    │ (set-ticket flow + │               │ first-time      │
                    │  git rebase)       │               │ prompt          │
                    └────────────────────┘               └─────────────────┘
                              │                                   │
                              └─────────┬─────────────────────────┘
                                        │
              ┌─────────────────────────┼──────────────────────────┐
              │                         │                          │
        ┌─────▼──────┐            ┌─────▼─────┐              ┌─────▼──────┐
        │ protected  │            │ tix start │              │ branch-    │  (Phase 4)
        │ branches   │            │ (creates  │              │ naming     │
        │ in hooks   │            │  branch)  │              │ check      │
        └────────────┘            └───────────┘              └────────────┘
                                        │
              ┌─────────────────────────┼──────────────────────────┐
              │                         │                          │
        ┌─────▼──────┐            ┌─────▼─────┐              ┌─────▼──────┐
        │ stale-base │            │  tix pr   │              │ tix ticket │  (Phase 5)
        │ in pre-push│            │           │              │ open       │
        └────────────┘            └───────────┘              └────────────┘
                                        │
                              ┌─────────▼──────────┐
                              │ Polish: README,    │   (Phase 6)
                              │ tix config, smoke, │
                              │ v0.1.0 tag         │
                              └────────────────────┘
```

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| `tix init` modifies the user's global git config (`core.hooksPath`) — affects every repo on the machine | High | Confirm-by-default; print exact `git config` command; show how to revert; idempotent re-runs. |
| Retroactive amend on commits that turn out to be already pushed → forces force-push later | High | Hard-stop unless `--force`; second confirmation; fetch latest remote before checking. Show full SHA list before any rebase. |
| `--no-verify` bypass means client-side enforcement is advisory | Medium | Documented as known limitation; pair with server-side branch protection (out of scope). |
| Hooks invoked in non-TTY contexts (CI, GUI clients, IDE git integrations) cannot prompt | Medium | Detect `isatty()`; if no TTY and branch has no state, fail closed with a clear error directing user to `tix set-ticket`/`tix clear-ticket`. Never hang waiting for input. |
| Glob/regex parsing errors in user-supplied config → tool refuses to run | Medium | Fail loud with exact line/file; `tix doctor` flags unparsable config without breaking other commands. |
| Hooks remain after `tix` uninstall and break every commit | High | Shims `exec command -v tix > /dev/null && exec tix hook <name> "$@"` — silent no-op when binary missing. |
| Atomic state writes lost on crash mid-write | Low | Write to `state.json.tmp` + `rename` in same dir; tests cover the path. |
| User's existing `core.hooksPath` is already set to something else | Medium | `tix init` detects, refuses to overwrite without `--force`; offers to merge by symlinking. |
| Git rebase during retroactive amend leaves repo in mid-rebase state on failure | High | Wrap in `git rebase --abort` on any failure; integration test simulates a failure mid-rebase. |
| Branch name with shell-unsafe chars in `tix start` slug | Low | Slugifier strips non-alphanumeric to `-`; explicit allowlist. |

## Parallelization Notes

For solo development, phases unroll sequentially — but within a phase several tasks are independent and can be reordered freely:

- **Phase 1 (foundation):** Tasks 1.1, 1.2, 1.3, 1.4 are independent. 1.5 (CLI scaffold) is independent of all but easier to write last.
- **Phase 2:** 2.1 (init) gates 2.2 (doctor verifies what init produces). 2.3 (show) is independent.
- **Phase 3:** 3.1 → 3.2 (independent of 3.1 actually) → 3.3 (depends on state from 3.1) → 3.4 (depends on state + prompt util) → 3.5 (depends on 3.1).
- **Phase 4:** 4.1 → 4.2 (depends on 4.1's protected-branch logic) → 4.3 (independent) → 4.4 (depends on hook plumbing from 4.1).
- **Phase 5:** 5.1, 5.2, 5.3 are mutually independent.

If multiple agents are available, the safe parallelization fronts are:
- Phase 1 tasks (4 independent libraries).
- Phase 5 tasks (3 independent push-time helpers).
- Phase 6 README + smoke test can begin in parallel with Phase 5.

## Phases & Tasks

---

### Phase 0: Bootstrap

> Context: the GitHub repo `https://github.com/ObieMunoz/tix.git` already exists with `LICENSE` (MIT) and `.gitignore` committed on `main`. The local working dir `corporate-git-assist/` contains only `SPEC.md` and `tasks/` (no `.git`). Phase 0 reconciles the two so we end up with a single `tix/` folder containing remote files + our local files, tracking `origin/main`.

#### Task 0.1: Rename folder and link to remote

**Description:** Rename `corporate-git-assist/` → `tix/`, init a git repo, add the existing GitHub remote, and pull in the remote's `main` (LICENSE + .gitignore) alongside our local SPEC + tasks.

**Acceptance criteria:**
- [ ] `/Users/obiemunoz/Development/MISC_PROJECTS/tix/` exists; `corporate-git-assist/` does not.
- [ ] `git remote -v` shows `origin → https://github.com/ObieMunoz/tix.git`.
- [ ] Local working tree contains: `SPEC.md`, `tasks/plan.md`, `tasks/todo.md`, `LICENSE`, `.gitignore`.
- [ ] `git status` shows `SPEC.md` and `tasks/` as untracked (or staged); `LICENSE` and `.gitignore` as tracked from remote.
- [ ] `git log --oneline` shows the remote's initial commit on `main`.

**Verification:**
- [ ] `ls /Users/obiemunoz/Development/MISC_PROJECTS/tix/{SPEC.md,LICENSE,.gitignore,tasks/plan.md}` succeeds.
- [ ] `git -C /Users/obiemunoz/Development/MISC_PROJECTS/tix log -1 --format=%s` returns the remote's first commit subject.

**Dependencies:** None.

**Files touched:** Directory rename + git metadata only.

**Estimated scope:** XS.

**Note:** Recommended sequence — `mv corporate-git-assist tix && cd tix && git init -b main && git remote add origin https://github.com/ObieMunoz/tix.git && git fetch origin && git reset --soft origin/main`. The `--soft` reset adopts remote's history without disturbing our local untracked files.

---

#### Task 0.2: Initialize Cargo project

**Description:** Create the Rust project skeleton with the crate-name/binary-name split, declared dependencies, and edition 2024. (LICENSE + .gitignore already exist from the remote.)

**Acceptance criteria:**
- [ ] `Cargo.toml` exists with `name = "tix-git"`, `version = "0.1.0"`, `edition = "2024"`, `[[bin]] name = "tix"`, `license = "MIT"`.
- [ ] All required deps declared: `clap` (derive), `serde`, `toml`, `serde_json`, `anyhow`, `thiserror`, `regex`, `dirs`, `chrono`, `anstream`; dev-deps: `assert_cmd`, `predicates`, `tempfile`.
- [ ] `src/main.rs` exists with a `fn main() -> anyhow::Result<()>` stub.
- [ ] `.gitignore` already excludes `target/` (verify; add if missing). `Cargo.lock` is **committed** (binary crate).

**Verification:**
- [ ] `cargo build` succeeds.
- [ ] `cargo run -- --help` prints clap's auto-help.

**Dependencies:** 0.1.

**Files touched:** `Cargo.toml`, `src/main.rs`, possibly `.gitignore` (additive).

**Estimated scope:** S.

---

#### Task 0.3: First commit and push to `main`

**Description:** Stage and commit SPEC.md + tasks/ + Cargo skeleton on top of the remote's existing `main`. Push to `origin/main`.

**Acceptance criteria:**
- [ ] One new commit on `main` adds `SPEC.md`, `tasks/plan.md`, `tasks/todo.md`, `Cargo.toml`, `Cargo.lock`, `src/main.rs`.
- [ ] `git push origin main` succeeds (fast-forward).
- [ ] Commit message follows a clean convention (e.g., `chore: scaffold Rust project (spec, plan, cargo skeleton)`).

**Verification:**
- [ ] `git log --oneline` shows our commit on top of the remote initial commit.
- [ ] GitHub web UI shows the new files on `main`.

**Dependencies:** 0.2.

**Files touched:** Git metadata + push only (content already created in 0.1/0.2).

**Estimated scope:** XS.

---

### Checkpoint A: Project bootstrapped

- [ ] `cargo build` clean
- [ ] `cargo test` runs (no tests yet, exits 0)
- [ ] `git log --oneline` shows our scaffold commit on top of the remote initial commit
- [ ] `origin/main` is up to date with our local `main`
- [ ] Human review: directory rename done? deps look right? remote tracking correct?

---

### Phase 1: Foundation libraries (no user-facing behavior)

#### Task 1.1: Config system

**Description:** Implement layered config loading: built-in defaults merged with `~/.config/tix/config.toml` then `<repo>/.tix.toml`. Define the `Config` struct(s) matching the default schema in SPEC §"Default global config." Track each value's source for `tix show`.

**Acceptance criteria:**
- [ ] `Config` struct with `ticket`, `branches`, `push`, `integrations` sections — all optional in TOML, filled from defaults.
- [ ] `Config::load(repo_root: Option<&Path>) -> Result<Config>` performs the three-layer merge.
- [ ] Each field knows its provenance (one of `Default | Global | Repo`) accessible via `Config::source(field) -> Source`.
- [ ] Bad TOML produces a clear error with file path.

**Verification:**
- [ ] Unit tests: defaults-only, global-only, global+repo merge, repo overrides global, malformed TOML, missing field uses default.

**Dependencies:** 0.2.

**Files touched:** `src/config/mod.rs`, `src/config/global.rs`, `src/config/repo.rs`.

**Estimated scope:** M.

---

#### Task 1.2: State system

**Description:** Implement `.git/tix/state.json` reader/writer. Atomic writes via temp+rename. Schema versioned. Per-branch entries with `ticket`, `set_at`, optional `amended_through`.

**Acceptance criteria:**
- [ ] `State::load(git_dir: &Path) -> Result<State>` (returns empty state if file absent).
- [ ] `State::save(git_dir: &Path) -> Result<()>` writes atomically.
- [ ] `State::set_branch(name, entry)`, `State::get_branch(name) -> Option<&BranchEntry>`, `State::clear_branch(name)`.
- [ ] `BranchEntry { ticket: Option<String>, set_at: DateTime<Utc>, amended_through: Option<String> }`.
- [ ] Version field; reject unknown future versions with clear error.
- [ ] Bad JSON produces a clear error with file path; offers `tix doctor` hint.

**Verification:**
- [ ] Unit tests: empty load, round-trip save/load, atomic write (interrupt mid-write doesn't corrupt), version mismatch.

**Dependencies:** 0.2.

**Files touched:** `src/config/state.rs` (or `src/state/mod.rs` — TBD during implementation).

**Estimated scope:** S.

---

#### Task 1.3: Git subprocess wrapper

**Description:** Thin wrapper around `std::process::Command::new("git")` exposing the operations `tix` needs. Each function returns `Result<T>` with stderr captured into errors.

**Acceptance criteria:**
- [ ] Functions implemented: `repo_root()`, `git_dir()`, `current_branch()`, `is_clean()`, `fetch(remote, branch)`, `rev_list_count(range)`, `for_each_ref(pattern)`, `commit_subject(sha)`, `merge_base(a, b)`, `is_commit_on_remote(sha, remote_branch)`, `current_commit()`, `set_global_config(key, value)`, `get_global_config(key)`.
- [ ] Operations execute against the current working dir unless a `repo_root` override is passed.
- [ ] Errors include the failing command + stderr.

**Verification:**
- [ ] Integration tests using `tempfile::TempDir` + real git: each function exercised against a fresh repo.

**Dependencies:** 0.2.

**Files touched:** `src/git/mod.rs`, `src/git/shell.rs`, `tests/common/mod.rs` (helpers).

**Estimated scope:** M.

---

#### Task 1.4: Utility modules

**Description:** Pure helper functions used across commands and hooks.

**Acceptance criteria:**
- [ ] `util::ticket::validate(s, pattern: &Regex) -> Result<()>` — passes/fails with clear message.
- [ ] `util::ticket::extract_prefix(message, pattern) -> Option<&str>` — returns first token if it matches.
- [ ] `util::slug::slugify(s, max_len: usize) -> String` — lowercase, non-alphanumerics → `-`, collapse, trim, cap.
- [ ] `util::glob::matches(pattern, branch) -> bool` — single-segment `*` (no `**`).
- [ ] `util::prompt::confirm(question, default: bool) -> Result<bool>` — TTY-only; non-TTY returns `Err`.
- [ ] `util::prompt::line(question) -> Result<String>` — TTY-only; non-TTY returns `Err`.

**Verification:**
- [ ] Unit tests covering: valid/invalid tickets, slug edge cases (unicode, leading/trailing dashes, > max_len), glob wildcards (`release/*` matches `release/1.0` but not `release/1.0/rc1`), prompt non-TTY error path.

**Dependencies:** 0.2.

**Files touched:** `src/util/mod.rs`, `src/util/ticket.rs`, `src/util/slug.rs`, `src/util/glob.rs`, `src/util/prompt.rs`.

**Estimated scope:** S.

---

#### Task 1.5: CLI scaffold (clap definitions)

**Description:** Define the entire CLI surface in clap derive form. Every subcommand exists as a stub that prints `not yet implemented` and exits 1. This locks the surface and lets us write integration tests against it.

**Acceptance criteria:**
- [ ] All 11 subcommands from SPEC §"`tix` CLI surface" parse correctly.
- [ ] `tix --help` shows all subcommands.
- [ ] `tix <subcommand> --help` shows subcommand-specific help.
- [ ] Unknown subcommand → clean clap error.
- [ ] `tix hook <name>` accepts trailing args (`HOOK_ARGS...`).

**Verification:**
- [ ] `cargo run -- --help` works.
- [ ] `cargo run -- start POD-1234 --base develop fix-thing` parses without error (even if stub).
- [ ] Integration test: every subcommand invocation exits with the stub message.

**Dependencies:** 0.2.

**Files touched:** `src/cli.rs`, `src/main.rs`, `src/commands/mod.rs` (stubs).

**Estimated scope:** S.

---

### Checkpoint B: Foundation ready

- [ ] All Phase 1 unit + integration tests pass on macOS and Linux
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] CLI surface accepts every subcommand
- [ ] Human review: function signatures look right? glob semantics correct?

---

### Phase 2: Bootstrap commands (install, status, diagnostics)

#### Task 2.1: `tix init`

**Description:** Set up the global git hooks directory and scaffold the global config file. Idempotent. Refuses to overwrite an existing non-tix `core.hooksPath` without `--force`.

**Acceptance criteria:**
- [ ] Creates `~/.config/tix/hooks/` containing three executable shims: `prepare-commit-msg`, `pre-commit`, `pre-push`. Each shim is `#!/bin/sh\nexec command -v tix > /dev/null && exec tix hook <name> "$@"` — silent no-op if `tix` absent.
- [ ] Sets `git config --global core.hooksPath ~/.config/tix/hooks`.
- [ ] If `core.hooksPath` is already set to something else: refuse, print existing value, suggest `--force`.
- [ ] Scaffolds `~/.config/tix/config.toml` with default contents (only if absent).
- [ ] Prints what it did and a "next steps" message.
- [ ] `--dry-run` flag: print what would happen without doing it.

**Verification:**
- [ ] Integration test: temp `$HOME` + temp `$XDG_CONFIG_HOME`, run `tix init`, assert file existence + git global config + hook executability.
- [ ] Test re-run is idempotent (no errors, no changes).
- [ ] Test refusal when `core.hooksPath` pre-set to a non-tix dir.
- [ ] Manual: run `tix init` on dev machine, then `git commit` in any repo — hook fires.

**Dependencies:** 1.1, 1.3, 1.4.

**Files touched:** `src/commands/init.rs`.

**Estimated scope:** M.

---

#### Task 2.2: `tix doctor`

**Description:** Run the diagnostic checks listed in SPEC §9. Each check prints `OK`/`WARN`/`FAIL` with one-line remediation.

**Acceptance criteria:**
- [ ] Checks: git version ≥ 2.30; `core.hooksPath` matches our managed dir; all three hook shims present + executable + reference `tix`; global config parses; (if in repo) repo config parses, state file parses, `default_base` resolvable as remote ref; (if `commit.gpgsign=true`) signing key configured.
- [ ] Exit code 0 if all OK or only WARN; exit 1 if any FAIL.
- [ ] `--verbose` flag dumps additional detail per check.

**Verification:**
- [ ] Integration tests with intentionally-broken state for each invariant: missing hook, wrong hooksPath, malformed config, missing default_base remote.
- [ ] Healthy install reports all OK.

**Dependencies:** 1.1, 1.2, 1.3, 2.1.

**Files touched:** `src/commands/doctor.rs`.

**Estimated scope:** M.

---

#### Task 2.3: `tix show`

**Description:** Print current branch, ticket, protected status, base branch, and config sources for each major value.

**Acceptance criteria:**
- [ ] Output sections: `Branch`, `Ticket`, `Protected branches`, `Base`, `Config sources`.
- [ ] When run outside a git repo: graceful error, no panic.
- [ ] When branch has no state entry: `Ticket: (not set — first commit will prompt)`.
- [ ] When ticket is null: `Ticket: (no-ticket mode)`.

**Verification:**
- [ ] Integration tests: outside repo, fresh repo, branch with ticket, branch with no-ticket, repo with `.tix.toml` overriding globals.

**Dependencies:** 1.1, 1.2, 1.3.

**Files touched:** `src/commands/show.rs`.

**Estimated scope:** S.

---

#### Task 2.4: `tix config get | set | list`

**Description:** Read/write config values from the CLI without hand-editing TOML. Moved from Phase 6 to Phase 2 so values like `branches.default_base`, `branches.protected`, and `integrations.ticket_url_template` are configurable from day one.

**Acceptance criteria:**
- [ ] `tix config get <KEY>` — prints the resolved value and its source (default/global/repo). Dotted notation: `branches.default_base`, `integrations.ticket_url_template`.
- [ ] `tix config set <KEY> <VALUE> [--global|--repo]` — writes to the specified scope (default `--global`). `--repo` requires being inside a git repo and writes `.tix.toml` (creates if absent). Type-aware: scalars (string/int/bool); list values via `--append <V>` and `--remove <V>` for `branches.protected`-style lists.
- [ ] `tix config list [--global|--repo|--all]` — prints all keys with values + sources. Default is `--all` (the resolved view).
- [ ] Invalid key (unknown field) → clear error listing known keys.
- [ ] Invalid value type → clear error showing expected type.
- [ ] Re-validates the file after write (round-trip parse) to catch corruption early.

**Verification:**
- [ ] Integration tests: get/set/list across both scopes; type errors; list-append + list-remove for `branches.protected`; setting `branches.default_base = "release/2026.04"` works and `tix show` reflects it.

**Dependencies:** 1.1.

**Files touched:** `src/commands/config_cmd.rs`.

**Estimated scope:** S.

---

### Checkpoint C: Install + configure + introspect

- [ ] On a fresh test environment: install binary → `tix init` → `tix doctor` reports OK → `tix show` reports current state in any repo
- [ ] `tix config set branches.default_base develop` (and `--repo` variant) updates the resolved value visible in `tix show`
- [ ] Hooks exist on disk and are executable
- [ ] `git commit` in any repo invokes `tix hook prepare-commit-msg` (verified via stub printing — actual hook logic comes in Phase 3)
- [ ] Human review: install UX, doctor checks, and config ergonomics are right?

---

### Phase 3: Ticket workflow (the marquee feature)

#### Task 3.1: `tix set-ticket <TICKET>` (without retroactive amend)

**Description:** Validate ticket, persist to state for current branch. The amend flow comes in 3.5.

**Acceptance criteria:**
- [ ] Validates ticket against `ticket.pattern` config.
- [ ] Updates current branch's state entry (creates if missing).
- [ ] Refuses outside a git repo with clear message.
- [ ] Prints confirmation showing branch + ticket + previous value if changed.

**Verification:**
- [ ] Integration test: valid ticket persists; invalid ticket rejected; state file updated atomically.

**Dependencies:** 1.1, 1.2, 1.3, 1.4.

**Files touched:** `src/commands/set_ticket.rs`.

**Estimated scope:** S.

---

#### Task 3.2: `tix clear-ticket`

**Description:** Set current branch's ticket to `null` ("no-ticket" mode). The branch entry still exists so the first-time prompt won't re-fire.

**Acceptance criteria:**
- [ ] Persists `ticket: null` for current branch.
- [ ] Prints confirmation.

**Verification:**
- [ ] Integration test: after `clear-ticket`, `tix show` reports no-ticket mode; commits proceed without prefix.

**Dependencies:** 1.2, 1.3.

**Files touched:** `src/commands/clear_ticket.rs`.

**Estimated scope:** XS.

---

#### Task 3.3: `prepare-commit-msg` hook (ticket prefix)

**Description:** Implement the marquee feature: prepend the current branch's ticket to the commit message. Idempotent. Skips merge commits.

**Acceptance criteria:**
- [ ] Hook reads commit message file (arg 1) and source type (arg 2).
- [ ] If source is `merge` or `squash`: no-op.
- [ ] If branch has no state entry: no-op (the prompt happens in `pre-commit`, 3.4).
- [ ] If branch state has `ticket: null`: no-op.
- [ ] If first whitespace-delimited token of message already matches `ticket.pattern`: no-op (idempotent, leaves a different ticket alone).
- [ ] Otherwise: rewrite message file as `{ticket} {message}` per `prefix_format`.
- [ ] Applies to `commit -m`, interactive `commit`, `--amend`, `revert`.
- [ ] Hook exits 0 on success; non-zero only on hard errors (state corrupt, etc.) — never on policy decisions.

**Verification:**
- [ ] Integration tests: first commit prefixed; subsequent commits prefixed; already-prefixed message untouched; different-ticket-prefix untouched; merge commit untouched; amend prefixed; revert prefixed.

**Dependencies:** 1.1, 1.2, 1.3, 1.4, 3.1.

**Files touched:** `src/hooks/prepare_commit_msg.rs`, `src/hooks/mod.rs`.

**Estimated scope:** M.

---

#### Task 3.4: `pre-commit` hook (first-time prompt)

**Description:** When committing on a branch with no state entry, prompt for a ticket (or empty for no-ticket mode). Persists choice.

**Acceptance criteria:**
- [ ] If branch already has state entry: no-op.
- [ ] If non-TTY and no entry: fail with clear error directing user to `tix set-ticket` or `tix clear-ticket`. Never hang.
- [ ] If TTY: prompt `Ticket for branch '<name>' (blank for no-ticket): `.
- [ ] Empty input → store `ticket: null`. Non-empty → validate against pattern, retry up to 3 times on invalid, then fail.
- [ ] Hook exits 0 on persisted choice; non-zero on cancel (Ctrl-C) or repeated invalid input.

**Verification:**
- [ ] Integration tests: TTY prompt persists ticket; empty input persists null; non-TTY fails clean; invalid → retry → eventually fails.

**Dependencies:** 1.1, 1.2, 1.3, 1.4.

**Files touched:** `src/hooks/pre_commit.rs`.

**Estimated scope:** M.

---

#### Task 3.5: Retroactive amend in `tix set-ticket`

**Description:** When `set-ticket` is called and the branch has unpushed unprefixed commits, offer to rewrite their subjects. Hard-stop if any are pushed.

**Acceptance criteria:**
- [ ] Computes unpushed commits via `git rev-list <branch> ^origin/<base>` (after best-effort `git fetch`).
- [ ] Filters to commits whose subject's first token does NOT match `ticket.pattern`.
- [ ] If any candidate is reachable from any remote ref: refuse without `--force`. With `--force`, require an extra confirmation explaining force-push consequences.
- [ ] Shows preview: short SHA + current subject + new subject. Prompts `amend N commits? [y/N]`.
- [ ] On confirm: runs `git rebase` rewriting each subject to `{ticket} {subject}`. Records new HEAD SHA in state as `amended_through`.
- [ ] On any rebase failure: runs `git rebase --abort` and reports clearly.

**Verification:**
- [ ] Integration tests: amends N local commits; refuses when commit is on remote; aborts cleanly on simulated rebase conflict; `--force` path with confirmation.
- [ ] Manual smoke: real branch with 3 unprefixed commits.

**Dependencies:** 1.2, 1.3, 1.4, 3.1.

**Files touched:** `src/commands/set_ticket.rs` (extended).

**Estimated scope:** M-L (highest-risk task in v1).

---

### Checkpoint D: End-to-end commit flow works

- [ ] Fresh branch → edit file → `git commit -m "msg"` → prompted for ticket → commit lands as `POD-1234 msg`
- [ ] Subsequent commits auto-prefix
- [ ] `tix clear-ticket` → commits without prefix
- [ ] No-ticket commits + `tix set-ticket POD-1234` → preview → amends prior commits
- [ ] `--no-verify` bypass works as expected (bypass hook)
- [ ] Human review: prompt UX is right? Amend safety is sufficient?

---

### Phase 4: Branch protection + lifecycle

#### Task 4.1: Protected branches in hooks

**Description:** Add protected-branch checks to `pre-commit` and `pre-push` hooks. Block direct commits and pushes.

**Acceptance criteria:**
- [ ] `pre-commit`: if `current_branch()` matches any pattern in `branches.protected` (literal or glob): exit non-zero with clear error including branch + protection source (default/global/repo) + override hint (`--no-verify`).
- [ ] `pre-push`: reads stdin (git's pre-push protocol); for each ref being pushed, blocks if local ref name matches a protected pattern.
- [ ] Glob support: `release/*` matches `release/1.0` but not `release/1.0/rc1` (single segment).
- [ ] Branch deletion is allowed (no block).

**Verification:**
- [ ] Integration tests: commit on `main` blocked; commit on `release/1.0` blocked (glob); commit on `feature/x` allowed; push to `main` blocked; deleting `release/old` allowed.

**Dependencies:** 1.1, 1.3, 1.4, 3.3, 3.4.

**Files touched:** `src/hooks/pre_commit.rs` (extended), `src/hooks/pre_push.rs`.

**Estimated scope:** S.

---

#### Task 4.2: `tix protect` / `tix unprotect`

**Description:** Add or remove a branch pattern from the protected list, in either global or repo config.

**Acceptance criteria:**
- [ ] `tix protect <BRANCH> [--global|--repo]` — defaults to `--global`. Appends to list if not already present. Persists.
- [ ] `tix unprotect <BRANCH> [--global|--repo]` — removes from list. No-op + warning if absent.
- [ ] `--repo` requires being inside a git repo; updates `.tix.toml` (creates if absent).
- [ ] Prints the resulting list.

**Verification:**
- [ ] Integration tests: protect → list updated; unprotect → list updated; idempotent re-add; repo scope writes `.tix.toml`.

**Dependencies:** 1.1, 4.1.

**Files touched:** `src/commands/protect.rs`.

**Estimated scope:** S.

---

#### Task 4.3: `tix start <TICKET> [DESCRIPTION] [--base <BRANCH>]`

**Description:** Create a properly-named branch off the latest base, register the ticket.

**Acceptance criteria:**
- [ ] Validates ticket against pattern.
- [ ] Resolves base: `--base` flag → `branches.default_base` config → `main`.
- [ ] Refuses with clear error if working tree is dirty.
- [ ] Runs `git fetch origin <base>` (errors if fetch fails — branching off stale base would be misleading).
- [ ] Creates branch `feature/<TICKET>[-<slug(description)>]` off `origin/<base>`. Branch prefix configurable (default `feature`).
- [ ] Registers ticket in state for the new branch.
- [ ] Prints `Started <new-branch> off <base> @ <short-sha> with ticket <TICKET>`.

**Verification:**
- [ ] Integration tests: clean repo + happy path; with description (slugified); with `--base develop`; dirty repo refused; invalid ticket refused; collision (branch already exists) handled cleanly.

**Dependencies:** 1.1, 1.2, 1.3, 1.4.

**Files touched:** `src/commands/start.rs`.

**Estimated scope:** M.

---

#### Task 4.4: Branch-naming convention check

**Description:** Check current branch name against `branches.naming_pattern` in `pre-commit` and `pre-push`. `warn` (default) prints a one-line yellow message; `block` refuses; `off` skips.

**Acceptance criteria:**
- [ ] Reads `branches.naming_enforcement` mode.
- [ ] `pre-commit`: applies to current branch.
- [ ] `pre-push`: applies per pushed ref.
- [ ] `warn` mode: stderr warning only; commit/push proceeds.
- [ ] `block` mode: stderr error + non-zero exit + suggestion to `git branch -m <new-name>`.
- [ ] `off` mode: no check.

**Verification:**
- [ ] Integration tests: each mode × matching/non-matching branch name.

**Dependencies:** 1.1, 4.1.

**Files touched:** `src/hooks/pre_commit.rs` (extended), `src/hooks/pre_push.rs` (extended).

**Estimated scope:** S.

---

### Checkpoint E: Branch protection + lifecycle complete

- [ ] Cannot directly commit/push to `main`, `develop`, `release/*`
- [ ] `tix start POD-1234 fix-login` creates `feature/POD-1234-fix-login` off latest `main`
- [ ] `tix start POD-1234 --base develop` works
- [ ] Branch named outside convention → warning at commit/push (default) or block (when configured)
- [ ] Human review: protection enforcement matches expectations?

---

### Phase 5: Push-time helpers

#### Task 5.1: Stale-base warning in `pre-push`

**Description:** Best-effort fetch + count of commits behind base. Warn-only.

**Acceptance criteria:**
- [ ] Best-effort `git fetch origin <default_base>`. Skip the rest on fetch failure (no network → no warning, no block).
- [ ] Computes `git rev-list --count <branch>..origin/<default_base>`.
- [ ] If count > `push.stale_warn_threshold` (default 50; 0 disables): print yellow warning with count + rebase hint.
- [ ] Never blocks the push.

**Verification:**
- [ ] Integration test: simulate behind-base by adding commits to base in a temp repo; assert warning fires; threshold of 0 silences it.

**Dependencies:** 1.1, 1.3, 4.1.

**Files touched:** `src/hooks/pre_push.rs` (extended).

**Estimated scope:** S.

---

#### Task 5.2: `tix pr`

**Description:** Open the PR-creation flow for the current branch.

**Acceptance criteria:**
- [ ] Detects provider from `origin` URL: `github.com`, `gitlab.com`, `*.bitbucket.org`. Falls back to `integrations.pr_provider`.
- [ ] If `pr_command = "auto"` and `gh` (GitHub) or `glab` (GitLab) is on PATH: shells out, passing the ticket as the PR title prefix.
- [ ] Otherwise: prints the provider's PR-create URL with branch pre-filled.
- [ ] If branch has no upstream: hint to `git push -u origin <branch>` first.

**Verification:**
- [ ] Integration tests: GitHub URL detection, GitLab URL detection, Bitbucket URL detection, provider override via config, no-upstream hint.
- [ ] Manual: real GitHub repo, `tix pr` opens PR draft via `gh`.

**Dependencies:** 1.1, 1.2, 1.3.

**Files touched:** `src/commands/pr.rs`.

**Estimated scope:** S.

---

#### Task 5.3: `tix ticket [open]`

**Description:** Print or open the current branch's ticket URL using the configured template.

**Acceptance criteria:**
- [ ] Reads `integrations.ticket_url_template`. Errors clearly if empty.
- [ ] Reads current branch's ticket from state. Errors clearly if missing or no-ticket mode.
- [ ] Substitutes `{ticket}` in template.
- [ ] `tix ticket` (no subcommand) → prints URL to stdout.
- [ ] `tix ticket open` → spawns `open` (macOS) or `xdg-open` (Linux).

**Verification:**
- [ ] Integration tests: print path; missing template error; missing ticket error; `open` invocation (mock the binary).

**Dependencies:** 1.1, 1.2.

**Files touched:** `src/commands/ticket.rs`.

**Estimated scope:** XS.

---

### Checkpoint F: Push-time helpers complete

- [ ] Pushing a stale branch → warning printed, push proceeds
- [ ] `tix pr` opens GitHub/GitLab PR draft (or prints URL)
- [ ] `tix ticket open` opens the configured ticket page
- [ ] All Phase 1–5 integration tests pass

---

### Phase 6: Polish + ship v0.1.0

#### Task 6.1: README and quick-start docs

**Description:** Write `README.md` covering: what tix is, install, `tix init`, the workflow it enables, config reference (key descriptions), uninstall.

**Acceptance criteria:**
- [ ] `README.md` exists at repo root.
- [ ] Has install command: `cargo install --git https://github.com/ObieMunoz/tix`.
- [ ] Has 5-minute quick-start: install → init → start → commit → push.
- [ ] Documents every config key with default and effect.
- [ ] Documents the `--no-verify` bypass as a known limitation.

**Verification:**
- [ ] Manual: a fresh reader can install + use `tix` end-to-end following only the README.

**Dependencies:** All v1 features merged.

**Files touched:** `README.md`.

**Estimated scope:** S.

---

#### Task 6.2: Fresh-machine smoke test

**Description:** Run the full install-to-first-commit flow on a clean environment (or simulated clean environment via temp `$HOME`).

**Acceptance criteria:**
- [ ] Script `scripts/smoke.sh` (or equivalent) that: builds tix, installs to a temp prefix, scaffolds temp `$HOME` + `$XDG_CONFIG_HOME`, runs `tix init`, creates a temp git repo, runs `tix start POD-1 test`, commits, asserts the commit message is `POD-1 <msg>`.
- [ ] Script exits 0 on success, prints clear failure on any step.

**Verification:**
- [ ] CI runs the smoke script on macOS + Linux.

**Dependencies:** All v1 features merged.

**Files touched:** `scripts/smoke.sh`, `.github/workflows/ci.yml`.

**Estimated scope:** S.

---

#### Task 6.3: CI workflow

**Description:** GitHub Actions: fmt check, clippy (deny warnings), test, smoke. Matrix: macos-latest + ubuntu-latest, stable Rust.

**Acceptance criteria:**
- [ ] Workflow file at `.github/workflows/ci.yml`.
- [ ] Triggers on `push` to `main` and on PRs.
- [ ] Steps: cache, fmt check, clippy, build, test, smoke script.
- [ ] All steps pass on the v0.1.0 commit.

**Verification:**
- [ ] PR CI green; pushing to main triggers green build.

**Dependencies:** 6.2.

**Files touched:** `.github/workflows/ci.yml`.

**Estimated scope:** S.

---

#### Task 6.4: Tag and release v0.1.0

**Description:** Bump `Cargo.toml` to 0.1.0, tag, push.

**Acceptance criteria:**
- [ ] `Cargo.toml` version is `0.1.0`.
- [ ] Git tag `v0.1.0` exists on the release commit.
- [ ] GitHub release page lists the v0.1.0 tag with a brief changelog.

**Verification:**
- [ ] `cargo install --git https://github.com/ObieMunoz/tix --tag v0.1.0` succeeds on a fresh machine.

**Dependencies:** 6.1, 6.2, 6.3.

**Files touched:** `Cargo.toml`, git tag.

**Estimated scope:** XS.

---

### Checkpoint G: v0.1.0 shipped

- [ ] All checkpoints A–F passed
- [ ] CI green on `main`
- [ ] `cargo install --git https://github.com/ObieMunoz/tix` works on a fresh machine
- [ ] README is complete; the SPEC's success criteria are all met
- [ ] Human review: ready to start using daily?

---

## Resolved Plan Decisions

- **License:** MIT (already on remote).
- **`Cargo.lock`:** committed (binary crate convention).
- **`tix init` vs existing `core.hooksPath`:** refuse without `--force`, print the existing value and suggested override.
- **Cargo workspace:** single-crate. Can convert later if we split off libraries.
- **`branches.start_prefix`:** configurable in v1. Default `feature`. `tix start` reads it; `tix config set branches.start_prefix bug` works.
- **Telemetry:** none. Tool is fully offline; no analytics, no phone-home.
- **GitHub remote:** `https://github.com/ObieMunoz/tix.git` already initialized with LICENSE + .gitignore on `main`. Phase 0 reconciles local + remote.
