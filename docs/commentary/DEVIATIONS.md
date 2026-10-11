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

### Slice 2: raw tab-state writes go through the guarded flush

The plan asks for a try/catch on the four raw `sessionStorage.setItem` calls (tabs.ts:123,
ImportModal.tsx:112/165/187). They call `writeSessionState` in `debouncedStorage.ts` instead, which
queues the entry and flushes immediately through the same per-key try/catch as the debounced writes.
A write that fails therefore stays in the pending map that `getItem` reads first, so the new tab still
opens with its game instead of an empty board, and the write is retried on the next flush.

### Slice 2: report errors are shown inline from component state

The inline Alert's message lives in `ReportPanel` state, not in `ReportState`. The plan's persistent
report failure field is `ReportState.failure: NarrationFailure` (slice 12), which describes provider
failures; engine-pass errors are plain strings and have no typed field to live in yet. The toast is
always shown; the Alert is shown while the panel is mounted.

### Slice 6: clock fields of `AnalysisOptions` land with their reader

The plan adds `AnalysisOptions.clocks`, `increment_s` and `initial_s` in slice 6. Nothing in Rust
reads them before `facts/time.rs` (slice 7b), and an unread field fails `clippy -D warnings`, so they
are added there. The TypeScript snapshot already captures what they will carry: each ply's `clock`
and the time control's `incrementS`/`initialS` (camelCase, the form they take across IPC).

### Slice 6: report positions are a named struct built by a pure function

`analyze_game`'s `fens: Vec<(Fen, Vec<String>, bool, bool)>` is a private `ReportPosition { fen,
moves, is_sacrifice, terminal }` built by `report_positions(fen, moves)`, so FEN alignment
(index 0 normalized, the game-over entry kept) is unit-tested without an engine. Positions are
computed before the engine is spawned, so a bad FEN or illegal move fails without starting one.

### Slice 6: `treeRegistry` parks writes, not narration chunks

The plan's `treeRegistry.parked` is a `Map<tabId, NarrationChunk[]>` drained by
`CommentaryListener`. Slice 6 already needs parking for the engine result itself (a report that
finishes while its board tab is unmounted), so `parked` holds `ParkedWrite = (store) => void`
closures applied in order on the next registration; slice 12 parks chunk applications through the
same queue. `activeRuns` arrives with the progress reconciliation in slice 8. `discard(tab)` is
added so a closed tab's parked writes are dropped.

### Slice 6: `addAnalysis` checks positions, not SAN, and stops at the first mismatch

`addAnalysis(analysis, glyphs, { showVariations, runId })` writes entry `i` only while the node and
its parent are the positions in `analysis[i].fen` and `analysis[i - 1].fen` (`samePosition`). The
plan also compares `node.san` with the snapshot's SAN; that is implied (two different moves from the
same parent cannot reach the same position), so the store does not need the snapshot. Because the
walk follows the current mainline, the first mismatch ends it. `beginReport(snapshot)` and
`endReport(runId)` (glossary names) set and clear the run; `ReportState.stage` arrives with its
first reader in slice 8. `ReportModal` takes the root and headers from the store at submit time
instead of render-time props.

## Questions for upstream

Decisions that need the upstream maintainer's input before a slice can be proposed upstream. The fork
keeps the plan's behaviour behind its settings flags until they are answered.

None yet.
