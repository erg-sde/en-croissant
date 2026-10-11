# Status

Session of 2026-10-11. Baseline: upstream `23f83142` (v0.15.1), see `BASELINE.md`. Every slice is a
branch stacked on the previous one, each with a pull request against the fork's `master`
(`erg-sde/en-croissant`). Merge them in order; GitHub shows each PR's diff against `master`, so a PR
also lists the commits of the slices below it until those merge.

## Slices completed

| # | Slice | Branch | PR |
|---|---|---|---|
| 0 | Planning documents | `commentary/00-docs` | https://github.com/erg-sde/en-croissant/pull/1 |
| 1 | Report robustness: exact scores, cancellable searches, terminal progress | `commentary/01-report-robustness` | https://github.com/erg-sde/en-croissant/pull/2 |
| 2 | Report errors surfaced, store guards | `commentary/02-report-errors` | https://github.com/erg-sde/en-croissant/pull/3 |
| 3 | PGN comment hygiene | `commentary/03-pgn-comment-hygiene` | https://github.com/erg-sde/en-croissant/pull/4 |
| 4 | Render comments without raw HTML | `commentary/04-safe-comment-rendering` | https://github.com/erg-sde/en-croissant/pull/5 |
| 5 | Opening match with ECO and ply | `commentary/05-opening-match` | https://github.com/erg-sde/en-croissant/pull/6 |
| 6 | Self-describing analysis | `commentary/06-self-describing-analysis` | https://github.com/erg-sde/en-croissant/pull/7 |
| 7a | Facts: geometry and motifs | `commentary/07a-facts-geometry-motifs` | https://github.com/erg-sde/en-croissant/pull/8 |
| 7b | Facts: lines, criticality, structure, time, extraction | `commentary/07b-facts-lines-extraction` | https://github.com/erg-sde/en-croissant/pull/9 |
| 8 | Deterministic move explanations | `commentary/08-deterministic-commentary` | https://github.com/erg-sde/en-croissant/pull/10 |

Each PR description states its slice, what it changes, the invariant rows it establishes, how it was
tested and what is deferred. Departures from `PLAN.md` are in `DEVIATIONS.md`, slice by slice.

## Slices in progress

None. The next slice is 9 (game narrative: `gameFacts.ts`, `narrateGame`, takeaways,
`GameNarrative.tsx`, `report.narrative`), branched from `commentary/08-deterministic-commentary`.
Slice 12 (LLM narration) is not to be started without the owner: it needs an Anthropic key and a
consent review.

## Tests added

| Suite | Baseline | Now | Added |
|---|---|---|---|
| `pnpm test` (vitest) | 40 | 105 | 65 |
| `cargo test` | 52 (44 pass, 8 fail) | 115 (107 pass with the ignored tests, 8 fail) | 63 |

The 8 failing Rust tests fail on the untouched baseline (`chess::tests::eval_*`,
`db::search::tests::get_move_after_exact_match_test`); upstream CI runs no cargo step. No slice adds
a failure. Five Rust tests are `#[ignore]`: four need a UCI engine (`EC_TEST_ENGINE`) and one writes
the frontend fixtures.

## Environment notes

- Windows checkout with `core.autocrlf=false` and `core.eol=lf` (oxfmt and `.editorconfig` need LF).
- pnpm 10 (as in CI); pnpm 11+ ignores `onlyBuiltDependencies` in `pnpm-workspace.yaml`.
- Rust 1.99 with the MSVC build tools (Visual Studio Build Tools 2022, C++ workload). Run cargo from
  PowerShell or cmd: Git Bash's `link` shadows the MSVC linker.
- `dist/` must exist (it can be empty) for `cargo test`, because `tauri::generate_context!` checks
  `frontendDist`.
- `cargo fmt` reformats two upstream files (`db/search_index.rs`, `sound.rs`) and clippy reports four
  upstream lints; the slices leave both alone (`BASELINE.md`).
- Bindings were regenerated with `pnpm dev` (debug build) for every slice that changes Rust types.
  The webview starts on this machine.

## Open questions

For the owner:

1. Upstream baseline: should a separate PR fix the 8 failing Rust tests, the 4 clippy lints and the
   2 unformatted files, and add a cargo CI job (PLAN.md slice 13 offers the job)? Until then, "clippy
   clean" can only mean "no lint in touched files".
2. Slice 3 writes `[%` in user comments as `[ %` on export, so text cannot be re-read as a PGN
   command. A comment holding an unknown command from another tool is exported broken up too. Keep,
   or limit the replacement to generated commentary?
3. Slice 1 rejects `GoMode::Infinite` in the report (`Error::InvalidGoMode`); the report UI never
   offers it, so this only guards other callers.
4. Slice 8 wording: generated sentences avoid intent ("puts the bishop on f7 en prise" rather than
   "sacrifices"), because the same fact describes a blunder. Confirm the tone before the templates
   are translated.
5. The PR descriptions say which UI paths were not clicked through in the running app this session
   (slice 8's card, modal options and settings rows); a manual pass before merging slice 8 is
   advised.

For upstream (to record under "Questions for upstream" in `DEVIATIONS.md` when the first slice is
proposed): whether a discussion should precede the slice PRs (CONTRIBUTING.md asks for one; PLAN.md
section 13 drafts it).

## Resume

```bash
git clone git@github.com:erg-sde/en-croissant.git && cd en-croissant
git remote add upstream https://github.com/franciscoBSalgueiro/en-croissant.git
git config core.autocrlf false && git config core.eol lf
git switch commentary/08-deterministic-commentary
pnpm i --frozen-lockfile
pnpm lint:ci && pnpm test
```

Rust (PowerShell, from `src-tauri`):

```powershell
New-Item -ItemType Directory -Force ..\dist | Out-Null
cargo test
$env:EC_TEST_ENGINE = "<path to stockfish>"; cargo test -- --include-ignored --skip write_frontend_fixtures
cargo clippy --all-targets -- -D warnings
```

Start slice 9:

```bash
git switch -c commentary/09-game-narrative commentary/08-deterministic-commentary
```
