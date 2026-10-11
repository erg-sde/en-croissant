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
| `cargo test`, `cargo clippy -- -D warnings` | see `STATUS.md` |
