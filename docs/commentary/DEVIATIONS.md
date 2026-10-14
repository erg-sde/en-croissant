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

### Slice 7a: the facts module is compiled but not yet called

Slice 7a adds the extractors and their tests; slice 7b calls them from `analyze_game`. Until then
`mod commentary` carries `#[allow(dead_code, unused_imports)]` with a comment saying so, because
`clippy -D warnings` rejects unused functions. Slice 7b removes the attribute. The vocabulary types
(`FactVocabulary`, `MoveRef`, `PieceRef`, `NumberRef`, `PositionRef`, `NumberUnit`) and `FactKind`,
`CommentaryKind`, `Perspective` are defined in slice 8, where TypeScript first uses them; Rust does
not construct them, so they are exported through the specta builder's type list there.

### Slice 7a: `see` is defined by the plan's own fixtures

The plan describes `see(board, square, side)` as "from the capturing side's POV" and also lists
fixtures in which the owner of the piece is the point of view ("queen left en prise to a knight
→ −9"). `geometry::see` satisfies all three fixtures with one rule: if `side` owns the piece, the
opponent captures only when that pays (result ≤ 0); if the opponent owns it, `side` captures first
and the result may be negative. `capture_see(before, mv)` gives `PlayedMoveFacts.see` (captured
value plus `see` on the destination after the move), so a capture is judged with the piece that
actually moved. A king never captures onto a defended square inside an exchange.

### Slice 7b: extraction interface details

- `extract_move_facts(ctx) -> Option<MoveFacts>` (the plan shows `-> MoveFacts`): `None` when the
  position after the move has no exact line, which is the plan's own rule ("a position whose exact
  set is empty gets `facts: None`").
- `FactsContext` takes the computed `time: Option<TimeFacts>` (from `time::time_facts` over a
  `Clocks` view of `AnalysisOptions`) instead of a raw clock input, and has no `threat_line` yet:
  the threat search that fills it is slice 10. Until then `LineOrigin::Threat` and
  `threats::null_move_fen` carry a commented `#[allow(dead_code)]`, and `MoveFacts.threat` is
  always `None`.
- `AnalysisOptions.clocks`, `increment_s` and `initial_s` arrive here with their reader (deferred
  from slice 6), together with `with_facts`.
- `commentary/mod.rs` does not re-export `FACTS_SCHEMA_VERSION`: nothing outside `facts` uses it
  yet and the unused import would fail clippy.

### Slice 7b: readings of under-specified extractor rules

- **`MateFound`**: the plan's condition "`before` is not a mate for the mover" cannot hold when the
  played move is the parent's mating line, because `EvalFacts.before` is that same parent eval. The
  extractor reads "before" as the evaluation before the opponent's previous move (`prev.before`):
  the mover's mate is new if it was not there one move earlier. `mate_in` counts the mover's moves
  including the move played (the played line's own `Mate(n)` when it is among the parent's lines,
  otherwise the after-position's `n + 1`). The mating move itself (terminal) gets no mate-transition
  fact; it is narrated as `Commentary.Mate`.
- **Material cash-in** is searched only within the first `MAX_SHOWN_PLIES` (6) plies, so the shown
  part of a line always contains the capture its outcome names.
- **Line motifs** (per shown ply, on the position the move leads to): a fork by the moved piece,
  discovered attacks, a pin or skewer by the moved piece, enemy pieces it attacks that are trapped,
  a passed pawn it creates, a promotion, mate-in-one threats; for a mating line also a back-rank
  weakness at the end. Duplicates are dropped.
- **`OpeningFacts`** is attached only to the move whose position is the last named one
  (`MoveFacts.index == OpeningMatch.index`).
- **Cancellation** is checked between positions of the facts pass, as planned. There is no unit
  test for it: the pass lives inside `analyze_game`, which needs a Tauri runtime; the token and
  guard semantics it relies on are tested in slice 1.

### Slice 8: what lands with the model-written text instead

- `CommentarySentence` has no `claims` field, and `Claim`/`Slots` are not defined yet: deterministic
  sentences make no claims, and the types arrive with the model's response schema (slice 12).
- Validation rules implemented: 1 (citations), 2 (references within the cited facts' vocabulary),
  3 (pieces on their squares, moves legal for their side), 4 (no literal square, digit or piece word
  in the template, applied to deterministic templates too, which makes a translator's mistake a
  dropped sentence rather than an unchecked one) and 8 (forbidden tokens, word budget). Rules 5-7
  (claim entailment and the relation, magnitude and motif lexicons) only check model-written text
  and arrive with it.
- No `ProgressStage::Commentary`, `ReportState.stage` or `treeRegistry.activeRuns` yet: the
  deterministic stage runs in milliseconds inside the report's completion handler, so there is no
  second stage to show or reconcile. They arrive with narration (slice 12).

### Slice 8: interface details

- `narrateMove` returns `SentenceDraft { template, sentence }`: the translated template is needed by
  rule 4 and is not stored.
- `buildMoveCommentary` (new module `report.ts`) chains extended glyphs, selection, narration and
  validation into a `MoveCommentary`, so the whole per-move pipeline is unit-tested; `ReportModal`
  calls it for each move. `fingerprint.ts` hashes the inputs with FNV-1a.
- `applyReport(analysis, glyphs, options, commentary)` takes per-move `{ extended, commentary }`
  results; the plan's separate `snapshot` and `records` arguments are covered by `options.runId` and
  the position checks of slice 6. A move without commentary in a re-run loses its stale record.
- `restoreCommentary` (validate.ts) re-brands a stored record after `moveCommentarySchema` accepts
  it; session storage keeps no brand.
- `createZodStorage` takes a schema whose input type is `unknown`, so a schema with defaults (the
  commentary settings) can back an atom. Existing callers are unaffected.
- The frontend tests run on `MoveFacts` written by the Rust extractor (an ignored test,
  `write_frontend_fixtures`), so both sides share one set of fixtures.

### Slice 8: selection and wording choices

- Kind precedence: a terminal mate, then ⊗ (a missed punishment is the more specific verdict), then
  the mistake glyphs, then the plan's order. `Miss` weighs 80, like the other criticality facts
  (the plan's weight list does not name it).
- After the mandatory facts, a mate, an opening or a novelty is said on its own (an opening may add
  the novelty); the parent's second line is added only to verdicts about the alternatives (only
  move, clearly best, sacrifice); a mate transition is not added when a chosen line already shows
  the mate. When the move played was the engine's second choice, that line is never called "the
  alternative".
- Templates make no claim of intent: a SEE-defined `Sacrifice` is "puts the bishop on f7 en prise",
  because the same fact describes a blunder. Mate templates name the mating side, so they stay true
  whichever side mates. A material outcome without a single piece uses the outcome key
  `MaterialGain`. Lines are cited through `Commentary.<Kind>.<Slot>.<Outcome>` with a
  `Commentary.Any.<Slot>.<Outcome>` fallback, so not every kind needs every combination.
- A back-rank motif is listed only when the mating move lands on that rank (a fix to slice 7b's line
  motifs; `Qxf7#` was described as a back-rank mate).

### Slice 8: deferred presentation

Arrows are computed and stored on each record but not drawn yet (the `commentaryArrowsFamily` toggle
and the Board layer), and the inline notation, the table segment and `TOGGLE_COMMENTARY` are not part
of this slice. The card's move chips play the cited line only while the report's facts are in memory
(after a reload the text and chips remain, without the "What this is based on" list).

### Slice 9: game narrative details

- `GameFacts.accuracy` values are `number | null` (a side with no analysed move has none). The
  accuracy numbers use the `winPercent` unit; there is no separate percent unit in the shared
  `NumberUnit`.
- Swings are the three largest win-chance changes of at least five points. `decisive` is the first
  move after the last position in which the eventual loser was still above 50%, and exists only for
  a decisive result (`1-0`, `0-1`).
- Narrative validation is `validateNarrative`: citations are `game:<kind>:<n>` or the plan's
  `<index>/<kind>:<n>`; a move reference must come from a cited fact or be the move played at a cited
  index, and is checked against the position it was played in. Each narrative sentence may have up
  to 220 words.
- Takeaways carry no counts ("more than once") so that no number is written that no fact holds;
  takeaway (d) says "in time trouble" rather than naming a seconds threshold. Takeaway (a) groups the
  motif facts selected for the opponent's replies.
- `applyReport` takes the narrative as a fifth argument and replaces `report.narrative` on every run.
  Slice 12 writes the model-written narrative through `applyCommentary` instead.
- `buildMoveCommentary` also returns the move's `FactSelection`, which the takeaways aggregate.
- A second fixture, `gameFacts.json`, holds the facts of a whole miniature from the same Rust writer.

### Slice 12: the prompt is written from scratch, not adapted from kibitz

PLAN.md section 6.4 says the prompt is adapted from kibitz and carries its copyright notice. The
kibitz source was not available in this session, so `prompt.rs` holds a prompt written from the
plan's own description (role, the five prohibitions, slot templates, claims, ordering, perspective,
language, budgets). It carries no third-party notice because it copies no third-party text. If
upstream prefers the kibitz wording, it can replace `SYSTEM_PROMPT` with attribution.

### Slice 12: deferred or narrowed

- **Repair round.** `NarratorConfig.repair_rounds` exists and is sent, but no repair request is made:
  a failing narrative sentence falls back to its draft. The validator's findings are recorded in
  `provenance.validation`, which is what a repair round would send back.
- **Preview request.** The consent modal lists what is sent; the "Preview request" link that renders
  the exact JSON for the current game is not built.
- **`ReportState.stage`.** Not added. The narration state is `report.narration` (`running`,
  `completed`, `failed`, `cancelled`, plus the failure), shown by `NarrationStatus` with a cancel
  button. `EvalChart`'s overlay still follows `inProgress`, which ends with the engine pass.
- **Mount reconciliation of stale progress.** `treeRegistry.hasRun(tab)` is there for it; the
  `ProgressButton` change is not.
- **Language selector.** `language` stays `null` (the UI language) in this slice; the setting row
  is not built.

### Slice 12: interface details

- `NarrationFailure` gains `MissingBaseUrl`: the OpenAI-compatible adapter has no default server.
  `RateLimit.retry_after_ms` is `u32` (milliseconds; `u64` would cross IPC as a `bigint`).
- `Redacted` masks every `sk-` token (a superset of the plan's `sk-ant-` pattern), `Bearer` tokens
  and the stored secret, then cuts to 200 characters. No regex crate: a small scanner does it.
- The Anthropic adapter sends `fallbacks: "default"` and the `server-side-fallback-2026-07-01` beta
  header only for models that have a server-side fallback (Opus 5 and later, Fable 5, Sonnet 5.5);
  other models such as Haiku would reject them. `Served.input_tokens` includes cache reads and
  writes.
- The OpenAI-compatible adapter retries once with `json_object` (schema appended to the system
  prompt) after any 400 to the `json_schema` request, and treats `finish_reason: "length"` as
  `MalformedResponse` and a `refusal` field as `Refused`.
- One response schema serves both chunk kinds: `{ items, narrative }` with `narrative` nullable.
  Move chunks ignore a returned narrative; the narrative chunk ignores items. Rust keeps only moves
  that were asked for and copies `fen_before` and `san` from the request item.
- The schema has not been exercised against the live API in this session (no key). Its shape is
  checked by a test (closed objects, every property required, no length or range constraints).
  Structured-output complexity limits could still reject it; "Test connection" will show that as a
  `Provider` failure with the API's message.
- `narrate_game` reports progress under the same job id as the report, stage `commentary`. A
  provider that cannot be built (no key, insecure URL) returns `Failed` with no request sent.
- `test_narrator` sends a fixed one-move request (1.e4 with one fact, plus a one-fact narrative) and
  reports the first failure, or `null` when both chunks parse.
- Secrets: `FileStore` writes `<app_data_dir>/commentary/secrets.json` (0600 on Unix, created with
  that mode), removes the file when the last key is cleared, and trims whitespace. The hint is the
  last four characters only for keys of eight characters or more. Help ▸ Clear saved data clears
  both providers' keys.
- TS: `SentenceDraft` gains optional `claims` and `problems`; rules 5 to 7 run only for drafts that
  carry `claims`, i.e. model-written ones. `renderSlots` (in `segmentize.ts`) renders a template from
  its slots and reports unknown, empty and unused placeholders, which fail as `literalInTemplate`.
  Lexicons for rules 6 and 7 cover en, de, ru and zh; `unknownMotif` is English only.
- `mergeNarration` (validate.ts) appends, after the kept model sentences, the deterministic
  sentences that share a fact with a dropped sentence and are not already covered.
- `applyCommentary(runId, writes, narrative)` checks `runId`, the node's SAN and its parent's
  position (`samePosition`) per write; `setNarration` replaces `setGameNarrative`. The listener
  turns chunks into records with the run context kept in `treeRegistry` (the last four runs, in
  memory; after a reload chunks are ignored and the drafts stay).
- The model setting is free text, prefilled `claude-opus-5-5`.
- After the first live runs (16 of 71 move sentences passed):
  - **Response format.** The API rejected the first schema (67 union-typed parameters) and then a flattened one (grammar too large). The model now writes refs into the cited facts' vocabulary (`line:2/moves/1`), resolved in Rust (`wire.rs`), with one small schema per request kind and no unions or nullable fields. A plain square or SAN that one of the chunk's facts holds is accepted where a ref was expected; the validator still requires it to come from a cited fact.
  - **Unused slots.** A filled slot that the template does not use no longer fails rule 4, since it puts nothing on screen. A placeholder with no value still fails.
  - **Piece names.** `{{q0}}`/`{{q1}}` render a piece slot by name only ("the {{q0}}" → "the queen"), so the model has a slot for the wording the drafts use.
  - **`sideBetter`.** The English lexicon now matches "White/Black/you is/stands … better", "advantage" and "the upper hand", not "the better move".
  - **Failure details** name what was found ("square", "digit", "piece word queen") instead of echoing the template, so they can be logged.
  - **Logging.** Narration logs counts per chunk, tokens, the outcome, and validation failures by rule. It never logs a key or the model's text.

### Slice 10: explain position and threat lines

- **Order.** Slice 10 is stacked on slice 12 (done first at the owner's request); it does not depend
  on it.
- **`parse_uci_attrs` stays private.** `EngineProcess::{new,set_options,go,stop,kill}` and
  `search_once` are `pub(crate)`; `explain_position` reads lines only through `search_once`, so I3's
  "only reader of UCI lines" holds without exposing the parser.
- **Threat candidates.** Moves with `OnlyMove`, `ClearlyBest`, `MateFound`, `Sacrifice` or `Miss`,
  or a static mate threat (`threats::mate_threats`) after the move, ordered by `win_delta` ascending,
  at most 12. "Never for ?!/?/??" is enforced in Rust as "never when the move lost 5 points of win
  chance or more", since glyphs are derived in the frontend; this also excludes most `Miss` moves.
  Threat lines are searched after the facts pass, and the facts of those moves are recomputed with
  the line. They use the report's `go_mode` with MultiPV 1.
- **Threat lines in the frontend.** `lineStart` replays a threat line from `fen_after` with the
  turn passed back (it used the plain `fen_after`, where the threat moves are illegal), and its
  first move is numbered as the mover's next move ("3. Bc4 threatens 4.Qxf7#"). New templates
  `Commentary.Any.Threat.{Mate,Material,MaterialGain,Eval}`.
- **`explain_position`** takes no `AppHandle` and emits no progress: it registers its job under the
  given id (so a new request cancels the previous one) and unregisters on every exit; the engine is
  quit explicitly and killed on drop. Engine options other than `MultiPV` and `Threads` are passed
  through. The opening, previous-move and clock facts are not available on demand.
- **Explain button.** `ExplainPosition` sits at the top of the engine stack in the analysis panel
  (visible with any number of engines). The engine is the report's engine, else the first local
  engine. The glyph comes from `explainGlyph` (the report's `getAnnotation` thresholds on the two
  searched lines) and the selection uses the `allMoves` detail. "Keep" calls a new
  `keepCommentary(path, fenBefore, record)` store action, which works on any line, not only the
  mainline (`applyCommentary` is mainline- and run-bound). "Keep" sets `dirty`; exporting the text
  is slice 11's flag.
- **Model badge.** The card now shows the model that answered for model-written records (deferred
  from slice 12).
- **Hotkey** `EXPLAIN_POSITION` ("e") is active while the analysis panel is shown.

### Slice 11: export commentary to PGN

- `getPGN` takes a `commentary` function (node → `PgnSafeText | null`) rather than a per-node value,
  so the recursion passes one option; `getMoveText` takes the node's `PgnSafeText`. `null` (the
  default) exports nothing generated; `copyVariationPgn` never passes it.
- The export flag is read with `readCommentarySettings(localStorage)` in `copyPgn` (store) and
  `saveToFile` (tabs.ts): importing `commentarySettingsAtom` there would make `atoms.ts` and
  `tabs.ts` import each other. The storage key is shared through `COMMENTARY_SETTINGS_KEY`.
- The provenance sentence uses the app version and source recorded with the narrative ("Commentary
  by En Croissant 0.15.1, deterministic[, model]."), in English: it is metadata, like the tag.
  Takeaways are not exported, only the narrative and each move's sentences.
- `[Annotator "En Croissant"]` is added through `headers.other` only when something generated is
  exported (a narrative or any node with commentary).

### Slice 13: only the bindings-drift test

- `main.rs` builds the specta builder in `specta_builder()` and the exporter in
  `bindings_exporter()`; `bindings_are_up_to_date` exports to a temporary file and compares it with
  the committed `src/bindings/generated.ts` (line endings normalized).
- **Deferred: the cargo CI job.** On the untouched baseline `cargo test` has 8 failing tests,
  `cargo clippy -- -D warnings` 4 lints and `cargo fmt --check` 2 files, so a job added now would be
  red from its first run. It should follow the baseline fix (the open question in `STATUS.md`).
- **Deferred: `scripts/commentary-eval/`.** The claim-precision harness needs the dev-only export
  of `{ facts, commentary }` per game and a corpus of at least 50 real games, plus LLM runs for the
  model-written half; none of these exist in this session.

### Slice 14 (fork): a verdict for every move

Not in PLAN.md; added at the owner's request so that stepping through a reviewed game says something
on every move, as chess.com's Game Review does.

- New `CommentaryKind`s `book`, `best`, `excellent`, `good`, `inaccuracy`, `error` ("Mistake") and
  `blunder`. A move without fuller commentary at the chosen detail level gets a one-line verdict:
  its glyph's grade, else "book" up to the last named opening position, else "best" for the engine's
  first choice, else "excellent" (at most 2 points of win chance lost) or "good" (at most 5). A move
  that is not the engine's choice adds "The engine preferred …", citing the best line.
- Verdicts cite the `evalDelta` fact and are validated like any sentence. They are not sent for
  narration and do not take part in the game narrative's selections.
- The detail setting still decides which moves get full explanations; verdicts cover every other
  analysed move whatever the setting.

## Questions for upstream

Decisions that need the upstream maintainer's input before a slice can be proposed upstream. The fork
keeps the plan's behaviour behind its settings flags until they are answered.

- **Consent text.** The consent modal's wording (Settings ▸ Commentary ▸ "Narrate with a language
  model") needs review before narration ships beyond the fork.
- **Default model.** The plan prefills `claude-opus-5-5`. A cheaper default may suit a feature that
  rephrases short sentences; the setting is free text either way.
