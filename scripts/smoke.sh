#!/bin/sh
# Fresh-machine smoke test: build tix, isolate $HOME / $XDG_CONFIG_HOME /
# GIT_CONFIG_GLOBAL into temp dirs, run install-to-first-commit end-to-end.
#
# Exits 0 on success; non-zero (with a clear failure line) on any step.
# Cleans up temp dirs via trap.

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

TMPROOT="$(mktemp -d)"
cleanup() { rm -rf "$TMPROOT"; }
trap cleanup EXIT INT TERM

step() { printf '\033[1;34m== %s\033[0m\n' "$1"; }
fail() { printf '\033[1;31mFAIL: %s\033[0m\n' "$1" >&2; exit 1; }

step "build tix (release)"
( cd "$REPO_DIR" && cargo build --release --quiet ) || fail "cargo build"
TIX_BIN="$REPO_DIR/target/release/tix"
[ -x "$TIX_BIN" ] || fail "tix binary not produced at $TIX_BIN"

# Put the binary on PATH so installed hook shims can find it.
SMOKE_BIN="$TMPROOT/bin"
mkdir -p "$SMOKE_BIN"
ln -s "$TIX_BIN" "$SMOKE_BIN/tix"

# Isolated environment.
export HOME="$TMPROOT/home"
export XDG_CONFIG_HOME="$TMPROOT/xdg"
export GIT_CONFIG_GLOBAL="$TMPROOT/gitconfig"
export PATH="$SMOKE_BIN:$PATH"
mkdir -p "$HOME" "$XDG_CONFIG_HOME"
: > "$GIT_CONFIG_GLOBAL"

# Bare origin so `tix start` has a base to fork off.
ORIGIN_DIR="$TMPROOT/origin.git"
git init --bare -b main "$ORIGIN_DIR" >/dev/null
SEED_DIR="$TMPROOT/seed"
git init -b main "$SEED_DIR" >/dev/null
( cd "$SEED_DIR"
  git config user.email "smoke@example.com"
  git config user.name "Smoke Test"
  git config commit.gpgsign false
  git commit --allow-empty -m "initial" >/dev/null
  git remote add origin "$ORIGIN_DIR"
  git push origin main >/dev/null 2>&1
)

step "tix init"
tix init >/dev/null || fail "tix init failed"
[ -x "$XDG_CONFIG_HOME/tix/hooks/prepare-commit-msg" ] || fail "prepare-commit-msg shim missing"
[ -f "$XDG_CONFIG_HOME/tix/config.toml" ] || fail "global config not scaffolded"

step "tix doctor (healthy)"
tix doctor >/dev/null || fail "doctor reported failures on a fresh install"

step "clone + start a feature branch"
WORK_DIR="$TMPROOT/work"
git clone "$ORIGIN_DIR" "$WORK_DIR" >/dev/null 2>&1
cd "$WORK_DIR"
git config user.email "smoke@example.com"
git config user.name "Smoke Test"
git config commit.gpgsign false

tix start POD-1 smoke-test >/dev/null || fail "tix start failed"
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
[ "$BRANCH" = "feature/POD-1-smoke-test" ] || fail "expected feature/POD-1-smoke-test, got $BRANCH"

step "git commit (hook prefixes the subject)"
git commit --allow-empty -m "drop legacy auth" >/dev/null || fail "git commit blocked"
SUBJECT="$(git log -1 --format=%s)"
[ "$SUBJECT" = "POD-1 drop legacy auth" ] || fail "expected 'POD-1 drop legacy auth', got '$SUBJECT'"

step "amend retains the prefix"
git commit --amend --allow-empty -m "drop legacy auth (v2)" >/dev/null || fail "amend blocked"
SUBJECT="$(git log -1 --format=%s)"
[ "$SUBJECT" = "POD-1 drop legacy auth (v2)" ] || fail "amend lost prefix: '$SUBJECT'"

step "protected branch refuses direct commit"
git checkout main >/dev/null 2>&1
if git commit --allow-empty -m "should be blocked" >/dev/null 2>&1; then
  fail "commit on main was not blocked"
fi

step "tix uninstall removes shims + unsets hooksPath"
tix uninstall >/dev/null || fail "uninstall failed"
[ ! -e "$XDG_CONFIG_HOME/tix/hooks/prepare-commit-msg" ] || fail "shim still present"
if grep -q hooksPath "$GIT_CONFIG_GLOBAL"; then
  fail "core.hooksPath still set after uninstall"
fi

printf '\033[1;32mOK: smoke test passed\033[0m\n'
