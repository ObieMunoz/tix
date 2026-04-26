# tix v0.1.0 — Todo

> Concise checklist. Full task detail (acceptance, verification, dependencies) lives in `plan.md`. Check items off as completed.

## Phase 0: Bootstrap
- [x] 0.1 Rename `corporate-git-assist/` → `tix/`; init repo, add `origin = github.com/ObieMunoz/tix`, fetch + hard reset to bring in remote LICENSE + .gitignore
- [x] 0.2 `cargo init` skeleton: `Cargo.toml` (crate `tix-git`, binary `tix`, edition 2024, deps), `src/main.rs` stub
- [x] 0.3 First commit (SPEC + tasks + Cargo skeleton + Cargo.lock); push to `origin/main` — commit `20327e2`
- [x] **Checkpoint A:** clean build, our commit on `main` ahead of remote initial commit

## Phase 1: Foundation
- [x] 1.1 Config system (defaults + global + repo merge, source tracking)
- [x] 1.2 State system (`.git/tix/state.json`, atomic writes, versioned)
- [x] 1.3 Git subprocess wrapper (`repo_root`, `current_branch`, `fetch`, `rev_list_count`, etc.)
- [x] 1.4 Utilities (ticket validate/extract, slugify, glob, prompt)
- [x] 1.5 CLI scaffold (clap, all 12 subcommands as stubs)
- [x] **Checkpoint B:** all unit + integration tests pass; clippy/fmt clean

## Phase 2: Bootstrap commands
- [x] 2.1 `tix init` — install global hooks + scaffold config (idempotent, `--dry-run`)
- [x] 2.1a `tix uninstall` — remove managed shims + unset hooksPath (`--purge`, `--dry-run`)
- [x] 2.2 `tix doctor` — diagnostic checks
- [x] 2.3 `tix show` — current branch/ticket/protected/base/sources
- [ ] 2.4 `tix config get|set|list` — read/write config (e.g., `default_base`, `protected`)
- [ ] **Checkpoint C:** install + configure + introspect works on fresh env

## Phase 3: Ticket workflow
- [ ] 3.1 `tix set-ticket <TICKET>` (no amend yet)
- [ ] 3.2 `tix clear-ticket`
- [ ] 3.3 `prepare-commit-msg` hook (prefix, idempotent, skip merges)
- [ ] 3.4 `pre-commit` hook (first-time prompt; non-TTY safe)
- [ ] 3.5 Retroactive amend in `set-ticket` (unpushed-only; `--force` for pushed)
- [ ] **Checkpoint D:** end-to-end commit flow works

## Phase 4: Branch protection + lifecycle
- [ ] 4.1 Protected branches in `pre-commit` + `pre-push` hooks
- [ ] 4.2 `tix protect` / `tix unprotect` (`--global` / `--repo`)
- [ ] 4.3 `tix start <TICKET> [DESCRIPTION] [--base <BRANCH>]`
- [ ] 4.4 Branch-naming convention check (warn / block / off)
- [ ] **Checkpoint E:** protections + `start` work

## Phase 5: Push-time helpers
- [ ] 5.1 Stale-base warning in `pre-push` (best-effort, never blocks)
- [ ] 5.2 `tix pr` — detect provider, shell to `gh`/`glab` or print URL
- [ ] 5.3 `tix ticket [open]` — print or open ticket URL
- [ ] **Checkpoint F:** push helpers work

## Phase 6: Polish + ship
- [ ] 6.1 `README.md` (install, quick-start, config reference, uninstall)
- [ ] 6.2 Fresh-machine smoke test (`scripts/smoke.sh`)
- [ ] 6.3 CI workflow (fmt + clippy + test + smoke; macOS + Linux matrix)
- [ ] 6.4 Tag v0.1.0
- [ ] **Checkpoint G:** v0.1.0 shipped — `cargo install --git ...` works on fresh machine
