# Deviations from PLAN.md

Every place where the implementation departs from `PLAN.md`, with the reason, recorded in the same PR
as the change. Citation drift against the baseline (`BASELINE.md`) is recorded here too.

## Citation drift

None. The baseline commit is the commit the plan was verified against (`BASELINE.md`).

## Deviations

### Tooling: `docs/` excluded from oxfmt and oxlint (slice 0)

`docs/**/*` is added to `ignorePatterns` in `.oxfmtrc.json` and `.oxlintrc.json`. oxfmt formats
Markdown and reflows the fenced code blocks in `PLAN.md`, destroying the aligned type listings, and
`research/maintainer-report-comments-branch-analysis.ts` is a reference copy of upstream code, not
project source. Without the exclusion `pnpm lint:ci` (`oxfmt --check`) fails on the planning docs.

### Slice 1: items without a consumer yet are deferred to the slice that uses them

`cargo clippy -- -D warnings` rejects never-read fields and never-constructed variants, so slice 1
ships only what it uses. Each item lands, unchanged in name and meaning, with its first consumer:

| Plan item (§5.3) | Lands with |
|---|---|
| `JobPhase`, `JobHandle.phase`, `JobHandle.started` | the first job that is not an engine pass (slice 10 `explain_position`) |
| `JobRegistry::subscribe` | the first command that joins a job another command registered (slice 12 `narrate_game`); in slice 1 the single stage uses the receiver `ProgressGuard::start` returns |
| `ProgressStage::Commentary` | the first commentary progress event (slice 8) |
| `ProgressGuard::handed_off` | `AnalysisOptions.finalize` |

### Slice 1: `ProgressGuard` is generic over a `ProgressSink`

The plan's guard holds the progress store and the `AppHandle` directly. It takes a `ProgressSink`
(implemented by `AppProgress { store, app }`) so its drop semantics are unit-tested without a Tauri
runtime. Behaviour is as planned: `finished()`/`cancelled()` report their status, any other exit
reports `Failed`, and the job is unregistered in every case.

### Slice 1: `MultiPvCollector` and two read-loop behaviours

`search_once` delegates line bookkeeping to a private `MultiPvCollector` so the exactness and
irregular-sequence cases are tested with scripted UCI lines instead of a fake engine process. Two
behaviours differ from the old inline loop, both in the direction of the plan's I3/I10:

- A `multipv 1` line starts a new set. Previously a set left incomplete (for example by a skipped
  bound line) blocked later sets of the same search until a mixed-depth set flushed it.
- End of engine output during a search returns `Error::EngineDisconnected`. Previously an empty
  `MoveAnalysis` was pushed and the next position failed with an I/O error, or, for the last
  position, the report succeeded with a missing evaluation.

`search_once` rejects `GoMode::Infinite` with the new `Error::InvalidGoMode`, which the plan
introduces for `explain_position`; it is added in slice 1 because `search_once` is introduced here.

## Questions for upstream

Decisions that need the upstream maintainer's input before a slice can be proposed upstream. The fork
keeps the plan's behaviour behind its settings flags until they are answered.

None yet.
