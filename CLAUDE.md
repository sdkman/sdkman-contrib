# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`sdkman-contrib` is a Rust crate with one binary, `contrib`. It checks and applies community contributions of candidates and versions, written as YAML requests under `requests/`, to the State API. The design is in [`specs/001-community-contributions.md`](specs/001-community-contributions.md), amended by [`specs/002-review-follow-ups.md`](specs/002-review-follow-ups.md). Read both before changing behaviour.

## Layout

- `src/main.rs` - the `contrib` CLI
- `src/lib.rs` - the library the CLI and the tests share
- `tests/*.rs` - integration tests; shared helpers in `tests/support/`
- `.github/workflows/` - the GitHub workflows; `pull-requests.yml` holds no secrets
- `.github/rulesets/main.json`, `.github/rulesets/release-tags.json` - the rulesets of `main` and of the `v*` tags
- `scripts/protect-main.sh` - applies and checks every ruleset in `.github/rulesets/`

## Boundaries

- Test servers listen on loopback only (`127.0.0.1`). No test reaches a real host, including `state.sdkman.io`.
- Tests that need git build a temporary repository. They never touch this repository.
- A test may push only to a temporary bare repository it created. Never push, switch branches or rewrite Git history from a script.

## Verification gate

Run these commands in this order. Work is done only when every command exits with status 0:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
nix run nixpkgs#actionlint -- -ignore 'unexpected key "queue"'
nix run nixpkgs#shellcheck -- scripts/protect-main.sh
```
