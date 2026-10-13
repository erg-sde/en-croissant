# Baseline

All commentary work is based on upstream `franciscoBSalgueiro/en-croissant` `master` at:

| Field | Value |
|---|---|
| Commit | `23f83142fcbd4d3af60a524855defe8c9a5703fe` |
| Subject | Add Chinese translations for loading errors and settings (#902) |
| Committer date | 2026-09-07 23:52:50 +0100 |
| Version | 0.15.1 (`package.json`, `src-tauri/Cargo.toml`) |
| Checked | 2026-10-11, `git fetch upstream`: `upstream/master` still points at this commit |

This is the same commit the research maps and `PLAN.md` were written against, so every `file:line`
citation in the plan applies as written. Any later rebase onto a newer upstream must re-verify the
citations a slice relies on and record drift in `DEVIATIONS.md`.

Remotes: `origin` = `erg-sde/en-croissant` (the fork, PR target), `upstream` =
`franciscoBSalgueiro/en-croissant`.

## Baseline checks

Run on the baseline commit before any change:

| Check | Result |
|---|---|
| `pnpm i --frozen-lockfile` (pnpm 10, as in `.github/workflows/test.yml`) | ok |
| `pnpm lint` | 0 errors, 56 warnings (pre-existing) |
| `pnpm test` | 5 files, 40 tests passed |
| `cargo test` (Rust 1.99.0) | 44 passed, 8 failed (pre-existing, see below) |
| `cargo fmt --check` | 2 files would be reformatted (`db/search_index.rs`, `sound.rs`) |
| `cargo clippy -- -D warnings` | 4 errors in upstream code (`db/mod.rs:631`, `db/mod.rs:714`, `db/search_index.rs:182`, `fs.rs:23`) |

Upstream CI (`.github/workflows/test.yml`) runs no cargo step, so these were not visible upstream.
The commentary slices leave them alone: each slice must add no new test failure, no new clippy
lint and no unformatted file, and only files a slice touches are formatted. The failing tests are
`chess::tests::eval_*` (7 tests asserting `naive_eval` values) and
`db::search::tests::get_move_after_exact_match_test`; fixing them belongs in a separate PR.

Environment notes for this Windows checkout: `core.autocrlf=false` and `core.eol=lf` are set in the
repository config because `.editorconfig` and oxfmt require LF; `dist/` must exist (it can be empty)
for `cargo test` because `tauri::generate_context!` checks `frontendDist`.
