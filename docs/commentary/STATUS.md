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
| 9 | Game narrative, takeaways and key moments | `commentary/09-game-narrative` | https://github.com/erg-sde/en-croissant/pull/11 |
| 12 | Optional LLM narration, validated | `commentary/12-llm-narration` | https://github.com/erg-sde/en-croissant/pull/12 |
| 10 | Explain this position, threat lines | `commentary/10-explain-position` | https://github.com/erg-sde/en-croissant/pull/13 |
| 11 | Export commentary to PGN | `commentary/11-export-commentary` | https://github.com/erg-sde/en-croissant/pull/14 |
| 13 | Bindings-drift test (partial) | `commentary/13-bindings-drift` | https://github.com/erg-sde/en-croissant/pull/15 |

Each PR description states its slice, what it changes, the invariant rows it establishes, how it was
tested and what is deferred. Departures from `PLAN.md` are in `DEVIATIONS.md`, slice by slice.

## Slices in progress

None. Order of the stack: 0-9, 12, 10, 11, 13 (slice 12 was done before 10 and 11 at the owner's
request). Merge in that order. What remains of the plan: slice 13's cargo CI job (blocked on the
red upstream baseline, see question 1) and its evaluation harness (needs a game corpus), and slice
14 ("later" items).

## Trying narration (slice 12)

1. `git switch commentary/13-bindings-drift` (the top of the stack), `pnpm dev`.
2. Settings ▸ Commentary ▸ "Narrate with a language model": read and accept the consent modal.
3. In "Language model", keep Anthropic and `claude-opus-5-5`, paste the key into "API key", press
   Save (the key goes to `<app data>/commentary/secrets.json` and is never shown again), then
   "Test connection". For a local server instead: provider "OpenAI-compatible server", server
   address `http://localhost:11434` (Ollama), the model name, no key.
4. Open a game, Generate report with "Explain moves" and "Narrate with …" ticked. The offline
   commentary appears first; narrated sentences replace it move by move as chunks arrive. A failure
   shows an orange alert in the report panel and keeps the offline text.

Not yet verified against the live API (no key in the session); "Test connection" is the first check.

## Tests added

| Suite | Baseline | Now | Added |
|---|---|---|---|
| `pnpm test` (vitest) | 40 | 132 | 92 |
| `cargo test` | 52 (44 pass, 8 fail) | 143 (129 pass, 8 fail, 6 ignored) | 91 |

The 8 failing Rust tests fail on the untouched baseline (`chess::tests::eval_*`,
`db::search::tests::get_move_after_exact_match_test`); upstream CI runs no cargo step. No slice adds
a failure. Six Rust tests are `#[ignore]`: five need a UCI engine (`EC_TEST_ENGINE`; all pass against
Stockfish) and one writes the frontend fixtures.

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
6. Slice 12: review the consent modal's text, and choose the default model (prefilled
   `claude-opus-5-5`; a cheaper model may be enough for rephrasing).

For upstream (to record under "Questions for upstream" in `DEVIATIONS.md` when the first slice is
proposed): whether a discussion should precede the slice PRs (CONTRIBUTING.md asks for one; PLAN.md
section 13 drafts it).

## Resume

```bash
git clone git@github.com:erg-sde/en-croissant.git && cd en-croissant
git remote add upstream https://github.com/franciscoBSalgueiro/en-croissant.git
git config core.autocrlf false && git config core.eol lf
git switch commentary/13-bindings-drift
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

Every slice through 13 is open as a PR; nothing is in progress.
