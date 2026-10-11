# "Generate report" / game analysis pipeline (Rust `analyze_game` command → TS `addAnalysis` store action → ReportPanel/EvalChart presentation), plus adjacent annotation/comment/opening-name/progress plumbing

## Summary
End-to-end flow. The Report tab lives in the right-hand analysis panel (src/components/panels/analysis/AnalysisPanel.tsx:103 `<Tabs.Tab value="report">`, panel at :248-258 renders `<ReportPanel />`). ReportPanel (src/components/panels/analysis/ReportPanel.tsx) owns a ProgressButton with id `report_${activeTab}` (:56-71), and mounts ReportModal with `initialFen={root.fen}` and `moves={getMainLine(root)}` (:47-54; getMainLine = mainline UCI moves, src/utils/chess.ts:139-141). The modal can also be opened by the F12 keybind (src/components/boards/BoardAnalysis.tsx:151-159; default in src/state/keybinds.ts:55) which switches to the analysis tab, sets `currentAnalysisTabAtom` to "report" and sets `currentReportModalOpenAtom` (src/state/atoms.ts:377-378). ReportModal (src/components/panels/analysis/ReportModal.tsx) persists its form in `reportSettingsAtom = atomWithStorage("report-settings", {novelty:true, reversed:true, variations:true, goMode:{t:"Time",c:500}, engine:""})` (:13-19), validates that a local engine is chosen and that a reference DB exists if novelty is on (:49-59), then calls `commands.analyzeGame(\`report_${tab}\`, engine.path, goMode, {annotateNovelties, fen: initialFen, referenceDb, reversed, moves}, engineSettings)` (:82-95) and, on `status === "ok"`, `addAnalysis(analysis.data, {showVariations})` (:96-102); `.finally(() => setInProgress(false))` (:103). Nothing is streamed back except a progress percentage; the full `MoveAnalysis[]` arrives in one promise resolution.

Rust side (src-tauri/src/chess.rs:474-634 `analyze_game`). It registers a cancel `AtomicBool` under `state.analysis_cancel_flags[id]` (:485-488), spawns a fresh `EngineProcess` (:493), replays `options.moves` from `options.fen` building `fens: Vec<(Fen, Vec<String>, bool)>` starting with the root position (:500) and, for every move whose resulting position is not game-over, pushes (fen-after-move, moves[0..=i], is_sacrifice) (:502-522) where `is_sacrifice = naive_eval(previous_pos) > -naive_eval(current_pos) + 100` (:513-518; naive_eval = best capture-only quiescence material from side-to-move, :636-708, piece values P90/N300/B300/R500/Q1000 :654-663). If `reversed` the list is analyzed last-to-first (:524-526) and un-reversed afterwards (:601-604). For each position it emits progress `i/len*100` via `update_progress` (:537-543), forces `MultiPV=2` on top of the user's UCI options (:545-557), calls `set_options`/`go(go_mode)` (:559-566) and reads UCI `info` lines until `bestmove`, keeping only the last complete multipv set at the deepest depth as `current_analysis.best` (:569-597). After the loop it does novelty detection: for positions in order, the first one for which `!is_position_in_db(reference_db, GameQuery.position(exact fen))` gets `novelty = true` and the search stops (:606-630; `Error::MissingReferenceDatabase` if annotate_novelties with no DB, :627). Finally `update_progress(..., 100.0, true)` (:631) and `Ok(analysis)` (:633). Scores from `parse_uci_attrs` are always converted to White's perspective (:239-241 invert when Black to move).

Frontend write-back (src/state/store/tree.ts:693-809 `addAnalysis(state, analysis, options)`, invoked by the `addAnalysis` store action at :488-494 which also sets `state.dirty = true`). It walks only the mainline `cur = root → cur.children[0]` with index i (:702-707, :805-807). For each node with a non-terminal position and non-empty `best`: `cur.score = analysis[i].best[0].score` (:709; i.e. node.score is the eval of the position AFTER the node's move, from White's POV); the move annotation is computed by `getAnnotation(prevprev=analysis[i-2].best[0].score.value, prev=analysis[i-1].best[0].score.value, next=analysis[i].best[0].score.value, color = cur.halfMoves%2===1?"white":"black", prevMoves=analysis[i-1].best, analysis[i].is_sacrifice, cur.san)` (:710-730; src/utils/score.ts:75-119). If annotation is "??"/"?"/"?!" and `showVariations`, the engine's best PV from the previous position (`analysis[i-1].best[0].uciMoves`, up to 10 plies) is appended as a sibling variation on `parent.children` unless a child with that move already exists (:734-795). `novelty` adds the "N" annotation (:797-799). Annotations are de-duplicated with a Set and sorted by NAG number (:800-803). Presentation: ReportPanel computes `getGameStats(root)` (src/utils/chess.ts:541-612) on every root change → per-color ACPL (mean of `getCPLoss`), accuracy (harmonicMean of `getAccuracy`), and counts of the six basic annotations; renders two AccuracyCards (ReportPanel.tsx:73-86, 170-191), the EvalChart (src/components/common/EvalChart.tsx; Mantine AreaChart over `2/(1+exp(-0.004*cp))-1` :55-74 with annotation-coloured dots :324-338, middlegame/endgame reference lines from `getGamePhases` :150-159, WDL mode, click-to-navigate :140-145), and a GameStats grid whose counts are clickable → `goToAnnotation(symbol, color)` which cycles the mainline to the next node carrying that annotation (tree.ts:158-184). No natural-language text is produced anywhere; `TreeNode.comment` is only written by the user via the tiptap AnnotationEditor (`setComment`, tree.ts:443-452). There is no LLM/AI code in the repo (grep for openai|anthropic|llm|gpt|ollama only hits "allMoves"/"fullmoves").

## Key files
- `src/components/panels/analysis/ReportModal.tsx` — Report settings form; invokes analyze_game and feeds result into the tree store
  - reportSettingsAtom :13-19 (atomWithStorage "report-settings")
  - ReportModal props :21-35 (tab, initialFen, moves, reportingMode, closeReportingMode, setInProgress)
  - analyze() :72-104 (commands.analyzeGame call :82-95, addAnalysis :98-100)
- `src/components/panels/analysis/ReportPanel.tsx` — Report tab UI: progress button, accuracy cards, eval chart, annotation counts
  - ReportPanel :19-96
  - stats = getGameStats(root) :31
  - handleCancel → commands.cancelAnalysis(`report_${activeTab}`) :33-35
  - ProgressButton id `report_${activeTab}` :56-71
  - GameStats :100-168 (goToAnnotation :126,:149)
  - AccuracyCard :170-191
- `src-tauri/src/chess.rs` — Engine process wrapper, live best-moves streaming, and the batch game analysis command
  - EngineProcess :39-153
  - BestMoves :154-167
  - BestMovesPayload (Event) :169-178
  - invert_score :180-191
  - parse_uci_attrs :193-242
  - EngineOptions :244-250
  - get_best_moves :319-446 (emits BestMovesPayload :402-410, :428-436)
  - MoveAnalysis :448-453
  - AnalysisOptions :455-463
  - cancel_analysis :465-472
  - analyze_game :474-634
  - count_material :636-652
  - piece_value :654-663
  - qsearch :665-696
  - naive_eval :698-708
  - tests :711-773
- `src/utils/score.ts` — Win-chance, accuracy, CP-loss and annotation classification formulas
  - INITIAL_SCORE :7-13 (cp 15)
  - CP_CEILING :15 (1000)
  - formatScore :17-32
  - getWinChance :34-36
  - normalizeScore :38-47
  - getAccuracy :60-67
  - getCPLoss :69-73
  - getAnnotation :75-119
- `src/state/store/tree.ts` — Zustand+immer tree store; all mutations of the game tree including report write-back
  - TreeStoreState :26-91 (goToAnnotation :42, setAnnotation :63, setComment :64, setScore :68, addAnalysis :74-81, setReportInProgress :83)
  - createTreeStore :95-526 (persist per-tab :516-523)
  - goToAnnotation impl :158-184
  - setAnnotation impl :422-442
  - setComment impl :443-452
  - setScore impl :478-487
  - addAnalysis action :488-494
  - setReportInProgress :496-502
  - function addAnalysis :693-809
- `src/utils/chess.ts` — Tree ↔ PGN helpers, mainline extraction, opening lookup, game statistics
  - BestMoves/MoveAnalysis local interfaces :21-34 (duplicate of bindings)
  - getMoveText :48-127 (PGN comment/eval/clk/csl/cal serialization :81-125)
  - getMainLine :139-141
  - getVariationLine :148-170
  - getPGN :218-306
  - getOpening :333-351
  - innerParsePGN :353-458 (comment parsing :367-404)
  - getGameStats :541-612
  - getMaterialDiff :622-654
- `src/utils/annotation.ts` — Annotation glyph vocabulary, NAG mapping, display metadata
  - Annotation union :12-38
  - NAG_INFO :40-73
  - ANNOTATION_INFO :83-190 (basic group !!,!,!?,?!,?,?? :85-126; N :175; ⊗ Miss :189)
  - isBasicAnnotation :192-196
- `src/utils/phase.ts` — Lichess Divider port: middlegame/endgame ply detection
  - GamePhases :72-75
  - getGamePhases(boards: Chess[]) :77-97
- `src/hooks/useProgress.ts` — Subscribes to progress-event for a given id
  - useProgress(id) :4-42 (getProgress :8, events.progressEvent.listen :16-24, clear :30-33)
- `src-tauri/src/progress.rs` — Progress store + event emission used by analyze_game
  - ProgressItem :8-13
  - ProgressEvent (Event) :15-20
  - update_progress :24-47
  - get_progress :49-53
  - clear_progress :55-59
- `src/bindings/generated.ts` — tauri-specta generated commands/events/types (do not hand-edit; regenerated from Rust)
  - commands.analyzeGame :24
  - commands.cancelAnalysis :32
  - commands.getOpeningFromFens :99
  - commands.getProgress/clearProgress :449-454
  - events :468-482 (progressEvent → "progress-event" :481)
  - AnalysisOptions :490
  - BestMoves :491
  - BestMovesPayload :492
  - EngineOption :499
  - GoMode :514
  - MoveAnalysis :515
  - ProgressEvent/ProgressItem :528-529
  - Score/ScoreValue :534-547
- `src/bindings/index.ts` — Hand-written overrides on top of generated bindings
  - ScoreValue adds {type:"dtz"} :10
  - Score/BestMoves re-typed :11-14
- `src/components/common/EvalChart.tsx` — Report eval/WDL chart with annotation dots and phase lines
  - DataPoint :33-44
  - getYValue :55-74
  - getData :105-128
  - phases via getGamePhases :150-159
  - CustomTooltip :298-322
  - CustomDot :324-338
- `src/utils/treeReducer.ts` — Core tree data model
  - TreeState :8-14
  - TreeNode :16-28
  - treeIteratorMainLine :56-65
  - defaultTree :77-108
  - createNode :110-136
  - GameHeaders :138-160 (eco :154)
  - ReportState :196-198
- `src-tauri/src/opening.rs` — Opening name lookup from embedded lichess ECO TSVs
  - TSV_DATA a-e.tsv :32-38, FISCHER_RANDOM_DATA :40
  - get_opening_from_fen :48-53
  - get_opening_from_fens :65-74 (last-to-first exact Setup match)
  - get_opening_from_setup :76-82
  - search_opening_name :84-115
  - OPENINGS lazy_static :117-169 (_eco field unused)
- `src/components/common/OpeningName.tsx` — Displays opening name for current path
  - OpeningName :7-24 (getOpening(root, position) :14); used in GameNotation.tsx:157 and databases/GamePreview.tsx:59
- `src/components/boards/EvalListener.tsx` — Consumes live BestMovesPayload events into per-engine atoms and writes current node score
  - EngineListener :98-180 (events.bestMovesPayload.listen :136-165, setEngineVariation map key `${fen}:${moves}` :147-156, setScore(ev[0].score) :158-162)
- `src/components/panels/analysis/BestMoves.tsx` — Per-engine live analysis accordion (the analysis-by-engine view)
  - BestMovesComponent :68 (reads engineMovesFamily :80, engineProgressFamily :81, tabEngineSettingsFamily :84)
  - AnalysisRow rendering per PV :304-317
- `src/components/panels/analysis/AnalysisRow.tsx` — One PV line (score bubble + SAN moves + copy)
  - AnalysisRow props {engine, score, moves, halfMoves, threat, fen, orientation} :21-37
- `src/state/atoms.ts` — Jotai global/per-tab atoms incl. settings persisted to localStorage
  - sessionsAtom (lichess token storage precedent) :213
  - referenceDbAtom :241
  - tabValue helper :310-329
  - reportTypeAtom :350
  - currentReportModalOpenAtom :377-378
  - currentAnalysisTabAtom :423
  - engineMovesFamily :544-548
  - engineProgressFamily :550-553
  - bestMovesFamily :556-600
  - tabEngineSettingsFamily :636-661
- `src/components/settings/SettingsPage.tsx` — Settings page with categorized SettingItem list
  - SettingCategory union :78-87
  - SettingItem :89-96
  - SettingRow :98-126
  - settings: SettingItem[] :184+
  - renderCategorySettings :769
- `src/components/common/Comment.tsx` — Renders node.comment as Markdown (react-markdown + rehype-raw + remark-gfm)
  - Comment :12-35
- `src/components/panels/annotation/AnnotationEditor.tsx` — Tiptap markdown editor bound to current node comment
  - useEditor onUpdate → setComment(editor.getMarkdown()) :24-44
- `src-tauri/src/main.rs` — Command/event registration and AppState
  - AppState :78-96 (analysis_cancel_flags :92, progress_state :95)
  - collect_commands! :111-171 (analyze_game :114, cancel_analysis :115)
  - collect_events! :173-177 (BestMovesPayload, ProgressEvent)
- `src-tauri/src/error.rs` — thiserror Error enum serialized as string to TS
  - NoMovesFound :57-58
  - MissingReferenceDatabase :60-61
  - NoOpeningFound :63-64
  - AnalysisCancelled :93-94
  - impl Serialize (to_string) :187-194
- `src-tauri/capabilities/main.json` — Tauri http plugin URL allowlist
  - http:default allow list :42-53
- `src/utils/lichess/api.tsx` — Precedent for authenticated external HTTP via @tauri-apps/plugin-http fetch
  - import { fetch } from "@tauri-apps/plugin-http" :5
  - apiHeaders({ Authorization: `Bearer ${token}` }) :217-219
- `src/utils/tests/score.test.ts` — Unit tests for the scoring/annotation formulas
  - getAnnotation threshold tests :35-53
  - getAccuracy :23-28
  - getCPLoss :30-33
- `src/utils/tests/store.test.ts` — Store action tests incl. addAnalysis
  - should handle addAnalysis :415-489

## Data model
Rust (src-tauri/src/chess.rs):
```rust
#[derive(Clone, Serialize, Debug, Derivative, Type)] #[derivative(Default)]
pub struct BestMoves { nodes: u32, depth: u32, score: Score, #[serde(rename="uciMoves")] uci_moves: Vec<String>, #[serde(rename="sanMoves")] san_moves: Vec<String>, #[derivative(Default(value="1"))] multipv: u16, nps: u32 }   // :156-167
#[derive(Serialize, Debug, Default, Type)]
pub struct MoveAnalysis { best: Vec<BestMoves>, novelty: bool, is_sacrifice: bool }   // :448-453  (NO rename_all → TS field is `is_sacrifice`)
#[derive(Deserialize, Debug, Default, Type)] #[serde(rename_all = "camelCase")]
pub struct AnalysisOptions { pub fen: String, pub moves: Vec<String>, pub annotate_novelties: bool, pub reference_db: Option<PathBuf>, pub reversed: bool }   // :455-463
pub async fn analyze_game(id: String, engine: String, go_mode: GoMode, options: AnalysisOptions, uci_options: Vec<EngineOption>, state: State<AppState>, app: AppHandle) -> Result<Vec<MoveAnalysis>, Error>   // :474-484
pub async fn cancel_analysis(id: String, state) -> Result<(), Error>   // :465-472
#[derive(Serialize, Debug, Clone, Type, Event)] #[serde(rename_all="camelCase")]
pub struct BestMovesPayload { pub best_lines: Vec<BestMoves>, pub engine: String, pub tab: String, pub fen: String, pub moves: Vec<String>, pub progress: f64 }   // :169-178
```
src-tauri/src/engine/types.rs: `pub struct EngineOption { pub name: String, pub value: String }` :5-8; `#[serde(tag="t", content="c")] pub enum GoMode { PlayersTime(PlayersTime), Depth(u32), Time(u32), Nodes(u32), Infinite }` :10-18.
src-tauri/src/progress.rs: `pub struct ProgressItem { pub id: String, pub progress: f32, pub finished: bool }` :8-13; `#[derive(Event)] pub struct ProgressEvent { pub id: String, pub progress: f32, pub finished: bool }` :15-20.

Generated TS (src/bindings/generated.ts):
```ts
analyzeGame(id: string, engine: string, goMode: GoMode, options: AnalysisOptions, uciOptions: EngineOption[]) : Promise<Result<MoveAnalysis[], string>>   // :24  (invokes "analyze_game")
cancelAnalysis(id: string) : Promise<Result<null, string>>   // :32
getOpeningFromFens(fens: string[]) : Promise<Result<string, string>>   // :99
getProgress(id: string) : Promise<ProgressItem | null>; clearProgress(id: string): Promise<void>   // :449-454
events.progressEvent  → "progress-event";  events.bestMovesPayload → "best-moves-payload"   // :468-482
export type AnalysisOptions = { fen: string; moves: string[]; annotateNovelties: boolean; referenceDb: string | null; reversed: boolean }   // :490
export type BestMoves = { nodes: number; depth: number; score: Score; uciMoves: string[]; sanMoves: string[]; multipv: number; nps: number }   // :491
export type MoveAnalysis = { best: BestMoves[]; novelty: boolean; is_sacrifice: boolean }   // :515
export type GoMode = { t: "PlayersTime"; c: PlayersTime } | { t: "Depth"; c: number } | { t: "Time"; c: number } | { t: "Nodes"; c: number } | { t: "Infinite" }   // :514
export type EngineOption = { name: string; value: string }   // :499
export type ProgressEvent = { id: string; progress: number; finished: boolean }   // :528
export type Score = { value: ScoreValue; wdl: [number, number, number] | null }   // :534-538 (wdl in permille; EvalChart divides by 10)
export type ScoreValue = { type: "cp"; value: number } | { type: "mate"; value: number }   // :539-547
```
src/bindings/index.ts overrides: `export type ScoreValue = ScoreValueT | { type: "dtz"; value: number }` :10; `Score`, `BestMoves` re-typed with it :11-14.

Tree model (src/utils/treeReducer.ts):
```ts
export interface TreeState { root: TreeNode; headers: GameHeaders; position: number[]; dirty: boolean; report: ReportState }   // :8-14
export interface TreeNode { fen: string; move: Move | null; san: string | null; children: TreeNode[]; score: Score | null; depth: number | null; halfMoves: number; shapes: DrawShape[]; annotations: Annotation[]; comment: string; clock?: number }   // :16-28
export interface ReportState { inProgress: boolean }   // :196-198
GameHeaders { ..., eco?: string | null, variant?, other?: Record<string,string>, start?: number[], orientation? }   // :138-160
```
Store action signature (src/state/store/tree.ts:74-81): `addAnalysis: (analysis: { best: BestMoves[]; novelty: boolean; is_sacrifice: boolean }[], options?: { showVariations: boolean }) => void`; `setReportInProgress: (value: boolean) => void` :83; `goToAnnotation: (annotation: Annotation, color: "white" | "black") => void` :42; `setComment: (payload: string) => void` :64; `setAnnotation: (payload: Annotation) => void` :63; `setScore: (score: Score) => void` :68.

Annotations (src/utils/annotation.ts:12-38): `type Annotation = "" | "!" | "!!" | "?" | "??" | "!?" | "?!" | "+-" | "±" | "⩲" | "=" | "∞" | "⩱" | "∓" | "-+" | "N" | "↑↑" | "↑" | "→" | "⇆" | "=∞" | "⊕" | "∆" | "□" | "⨀" | "⊗"`; `ANNOTATION_INFO: Record<Annotation, { group?: string; name: string; translationKey?: string; color?: MantineColor; nag: number }>` :75-190 (basic: "!!" Brilliant cyan nag3, "!" Good teal nag1, "!?" Interesting lime nag5, "?!" Dubious yellow nag6, "?" Mistake orange nag2, "??" Blunder red nag4; "N" Novelty nag146; "⊗" Miss red nag9).

Report-derived stats (src/utils/chess.ts:541-612) `getGameStats(root)` returns `{ whiteCPL, blackCPL, whiteAccuracy, blackAccuracy, whiteAnnotations, blackAnnotations }` where the annotation maps are `{"??","?","?!","!!","!","!?"} → number`.
Phases (src/utils/phase.ts:72-75): `type GamePhases = { middlegamePly: number | null; endgamePly: number | null }` (indices into mainline nodes excluding root).
Report settings (ReportModal.tsx:13-19): `{ novelty: boolean; reversed: boolean; variations: boolean; goMode: Exclude<GoMode, {t:"Infinite"}>; engine: string }`.
Engine (src/utils/engines.ts:37-53): `LocalEngine = { type:"local"; id; name; version; path; image?; elo?; downloadSize?; downloadLink?; loaded?; go?: GoMode; enabled?; settings?: {name: string; value: string|number|boolean|null}[] }`.

## Extension points
- **src/state/store/tree.ts:693-809 (function addAnalysis) and the action at :488-494**: This is the single place where per-move report facts (prev/next score, best lines `analysis[i-1].best`, is_sacrifice, novelty, played SAN, classification) are all in scope at once. A commentary generator can be invoked here (or immediately after, in ReportModal.tsx:96-102 with the same `analysis.data`) to produce a per-move explanation. The deterministic fallback has everything it needs already: `getAnnotation` inputs, winChance delta, `prevMoves[0].sanMoves` (best move + PV), `prevMoves[1]` (second-best), `is_sacrifice`, `novelty`, `cur.san`, `color`, and the node FEN for tactical probing with chessops.
  - risk: addAnalysis runs inside an immer `produce` and is synchronous; anything async (LLM) must happen outside the reducer and write back via a new action (e.g. `setCommentary(path, text)`) — do not block the store. Writing into `TreeNode.comment` would collide with user comments and with PGN import/export (comment is serialized in `{}` by getMoveText chess.ts:117-119 and overwritten by the tiptap editor on edit, AnnotationEditor.tsx:39-41). Maintainers will likely want a separate field or a clearly delimited block in the comment, plus a decision about PGN serialization.
- **src/utils/treeReducer.ts:16-28 (TreeNode) + src/state/store/tree.ts persist :516-523 + src/utils/chess.ts getMoveText :48-127 / innerParsePGN :353-458**: Add an optional per-node field (e.g. `commentary?: { text: string; source: "llm"|"heuristic"; model?: string }`) and extend getMoveText/innerParsePGN to round-trip it (e.g. as a dedicated PGN comment or a `[%xxx ...]` markup, the way `[%eval]`/`[%clk]`/`[%csl]` are handled at :84-115 and parsed via chessops `parseComment` at :367-404).
  - risk: TreeNode is spread/compared with `toStrictEqual` across store tests (store.test.ts:462-488) — every test fixture (treeE4D5 etc.) that constructs nodes will need the new optional field or the tests will fail. `createNode` (:110-136) and `defaultTree` (:77-108) must stay consistent. Per-tab state is persisted to sessionStorage (createDebouncedSessionStorage) so large text per node increases storage size.
- **src/utils/score.ts:75-119 (getAnnotation) and tests src/utils/tests/score.test.ts:35-53**: Reuse (do not duplicate) the thresholds: winChanceDiff >20 → "??", >10 → "?", >5 → "?!"; only-move check `winChance(best)-winChance(second) > 10 && move === prevMoves[0].sanMoves[0]`; `!!` if also is_sacrifice; `!` if best-line winChance beats prevprev by >5; `!?` if is_sacrifice && nextCP > -200. A commentary module can import getWinChance/normalizeScore/getAccuracy/getCPLoss and the same classification to generate text that is consistent with the glyph the report already shows. If richer categories are wanted (e.g. chess.com's Best/Excellent/Good/Book/Great/Miss), add a separate pure classifier in src/utils/ with its own tests rather than changing getAnnotation's return type (Annotation union is used by NAG export).
  - risk: getAnnotation's contract (NAG glyphs, PGN NAG export at chess.ts:70-77, tests) is relied on; changing thresholds changes every user's reports. `move === prevMoves[0].sanMoves[0]` relies on SAN string equality between shakmaty SanPlus (Rust) and chessops makeSan.
- **src/components/panels/analysis/ReportPanel.tsx:55-93 (Stack layout) and src/components/panels/analysis/AnalysisPanel.tsx:101-107 (Tabs.List)**: Game-level narrative can be a new Paper below `<GameStats />` (or above EvalChart) in ReportPanel, fed from the store (e.g. `s.report.narrative`) — `ReportState` (treeReducer.ts:196-198) currently only holds `inProgress` and is the natural place for game-level outputs (`report: { inProgress, narrative?, generatedAt?, provider? }`). Per-move explanation UI for the current node can be a card in the same panel keyed on `s.position`, mirroring how Board.tsx reads `currentNode.annotations[0]` (:342-349) for the AnnotationHint.
  - risk: ReportPanel re-computes `getGameStats(root)` on every root change (:31) — adding heavy work there will lag navigation; memoize by tree hash (`getTreeStructureHash`, treeReducer.ts:183-194) or compute once in the write-back step. `ReportState` is part of `TreeState` which is persisted per tab and spread in tests (defaultTree :104-106).
- **src/components/panels/analysis/BestMoves.tsx:304-317 (AnalysisRow per PV) / AnalysisRow.tsx:21-37, fed by src/components/boards/EvalListener.tsx:136-165 and atoms engineMovesFamily (atoms.ts:544-548)**: For the analysis-by-engine view, an 'Explain this position' affordance can read the current position's lines via `engineMovesFamily({tab, engine}).get(`${fen}:${moves.join(",")}`)` (same key the listener writes at :149) or the aggregated `bestMovesFamily({fen, gameMoves})` (atoms.ts:556-600 gives `{pv: string[]; winChance}` per engine filtered to within 10 win% of best). Threat mode uses `swapMove(fen)` keys (:150-153, BestMoves.tsx:313).
  - risk: Live `setScore(ev[0].score)` (EvalListener.tsx:158-162) overwrites the report's `node.score` for whichever node is current while engines run, so any commentary that trusts `node.score` after a report must tolerate it changing; and engine output arrives continuously (rate-limited 5/s in Rust, chess.rs:365,386) so commentary must be debounced/triggered manually, not reactive to every payload.
- **src-tauri/src/chess.rs:474-634 (analyze_game) and :545-557 (MultiPV=2), :606-630 (novelty loop)**: If the deterministic fallback needs more engine facts per move (e.g. MultiPV>2 to describe 'the only move', a short search of the played move's refutation, or evaluating the threat by null-move), the batch loop is the place: everything per position is already in `current_analysis.best`. Additional fields on `MoveAnalysis` (e.g. `threat: Option<BestMoves>`) flow automatically into TS via specta after regenerating bindings (tauri-specta collect_commands! in main.rs:111-171). Per-move partial results could also be streamed by emitting a new `Event` struct (pattern: BestMovesPayload chess.rs:169-178 + collect_events! main.rs:173-177) so commentary can start before the whole game finishes.
  - risk: Report time scales linearly with positions × go_mode (default 500 ms/position); doubling engine work per move will be felt. MoveAnalysis lacks `#[serde(rename_all="camelCase")]` so new snake_case fields leak as-is (existing `is_sacrifice`). `reversed` processing order means per-position streaming would arrive last-to-first by default. `fens` skips game-over positions (:514), so indices are 'non-terminal positions' not plies.
- **src/state/atoms.ts (atomWithStorage pattern e.g. referenceDbAtom :241, sessionsAtom :213) + src/components/settings/SettingsPage.tsx:78-96 (SettingCategory/SettingItem), :98-126 (SettingRow), :769 (renderCategorySettings)**: Add an atomWithStorage for provider config (provider enum, API key, model, enable flag) and a new `SettingCategory` (e.g. "commentary"/"ai") with SettingItem entries rendered through SettingRow; the lichess `accessToken` in `sessionsAtom` is the existing precedent for keeping a secret in localStorage.
  - risk: Secrets in localStorage are plaintext; maintainers may prefer the Rust side (tauri store/keychain) — no such mechanism exists today. SettingCategory is a closed union; the page's search/keywords and category rendering must be updated together. All UI strings must go through `t()` with flat keys and `pnpm i18n:extract` (i18next.config.ts; removeUnusedKeys: true).
- **HTTP egress: src-tauri/capabilities/main.json:42-53 (http:default allow list) + src/utils/lichess/api.tsx:5,217-219 (plugin-http fetch + apiHeaders) + src/utils/http.ts:17-22; alternatively Rust reqwest 0.12.5 (src-tauri/Cargo.toml:9)**: An LLM provider client in TS must add its host (e.g. the provider API base URL) to the capability allow list, then call `fetch` from `@tauri-apps/plugin-http` with `apiHeaders({...})`. A Rust command using reqwest avoids the capability list and keeps the key out of the webview, at the cost of specta types and a command registration in main.rs.
  - risk: Capability allow list is an explicit security surface; upstream will review new hosts. Streaming responses via plugin-http is possible but no precedent exists in the repo. The claude-api skill (available in this session) should be consulted for exact model ids/params before writing provider code.
- **src/components/common/Comment.tsx:12-35 and CompleteMoveCell.tsx:200 / GameNotation.tsx:112-114 (rendering), currentShowCommentsAtom (atoms.ts:368-369)**: If commentary is rendered inline in the notation, reuse Comment (Markdown) and the existing show/hide comments toggle; if it must be visually distinct from user comments, add a sibling component gated by its own per-tab atom via `tabValue(atomFamily(...))` (atoms.ts:310-329).
  - risk: GameNotation has two layouts (tree and TableNotation :281+) that both special-case comments; a new inline block must be handled in both. Long text per move will bloat the notation panel.
- **src/utils/chess.ts:333-351 getOpening + src-tauri/src/opening.rs:65-74 get_opening_from_fens**: Game narrative can name the opening by calling `getOpening(root, lastMainlinePosition)` (it walks fens along the path and asks Rust for the deepest known position). There is no per-move 'book' flag; to say 'still in book' a commentary module can call `commands.getOpeningFromFen(node.fen)` per node (opening.rs:48-53) and treat the last node that matches as the end of book; ECO code is parsed but discarded (`_eco`, opening.rs:13,147).
  - risk: Exact `Setup` equality; FRC entries included. One IPC call per node if done naively — batch via getOpeningFromFens or add a Rust command returning the last matching index.

## Conventions
- State: global/per-tab UI state in jotai (src/state/atoms.ts) with `atomWithStorage` for persisted settings (localStorage keys like "report-settings", "reference-database") and `tabValue(atomFamily(...))` for per-tab values (atoms.ts:310-329); the game tree lives in a zustand store created per tab (`createTreeStore(id, initial)`, tree.ts:95) accessed through `TreeStateContext` + `useStore(store, selector)` (e.g. ReportPanel.tsx:24-29), mutated with immer `produce` inside actions, every mutation sets `state.dirty = true` (tree.ts:481,491).
- Rust↔TS boundary: every command is `#[tauri::command] #[specta::specta]`, every exposed type derives `specta::Type` (+ `serde`), events derive `tauri_specta::Event` (chess.rs:169-178, progress.rs:15-20); registration in main.rs collect_commands!/collect_events! (:111-177); src/bindings/generated.ts is generated (ignored by oxfmt, .oxfmtrc.json) and hand-tweaked only through src/bindings/index.ts. Commands return `Result<T, Error>`; Error serializes to a string (error.rs:187-194) so TS gets `Result<T, string>` and callers check `res.status === "ok"` (ReportModal.tsx:97) or `unwrap(...)` (src/utils/unwrap.tsx; chess.ts:461).
- Long-running Rust work reports progress via `update_progress(&state.progress_state, &app, id, pct, finished)` (progress.rs:24-47) and is cancelled through an `AtomicBool` in `AppState.analysis_cancel_flags` keyed by the same id (chess.rs:465-472, 531-536); the frontend pairs `useProgress(id)` with `ProgressButton` (ProgressButton.tsx:37-54) and an id convention like `report_${tab}`.
- i18n: react-i18next with flat dotted keys (`t("Board.Analysis.GenerateReport")`), keySeparator/nsSeparator disabled, keys extracted with `pnpm i18n:extract` into src/translation/*.json (16 locales, sorted, removeUnusedKeys true, dynamic `Annotate.*` keys preserved via preservePatterns) — i18next.config.ts; CI runs `i18next-cli extract --ci` (package.json lint:ci).
- Tooling: pnpm; `pnpm lint` = tsgo --noEmit + oxlint; `pnpm format` = oxfmt; tests = vitest (`pnpm test`, jsdom) living in src/utils/tests/*.test.ts using `test()`/`expect()` and whole-state `toStrictEqual` fixtures (store.test.ts:415-489); Rust unit tests inline under `#[cfg(test)] mod tests` (chess.rs:711-773, opening.rs:171-182). Formatting observed: double quotes, semicolons, trailing commas; components under src/components use 2-space indent, src/utils and src/state use 4-space indent (oxfmt-managed).
- UI: Mantine v8 components (Paper, Stack, Group, Grid, Modal, Select, Checkbox, NumberInput), @mantine/form `useForm` with `validate` for modals (ReportModal.tsx:49-59), @mantine/charts AreaChart for charts, @tabler/icons-react icons, CSS modules (*.module.css) for styling, `memo` with custom equality for list cells (ReportPanel.tsx:100-168, CompleteMoveCell).
- Chess logic in TS uses chessops (`positionFromFen`, `parseUci`, `makeSan`, `makeFen`) via src/utils/chessops.ts helpers; in Rust uses shakmaty (`Chess`, `Fen`, `UciMove`, `SanPlus`) and vampirc-uci (fork) for UCI parsing. Engine evaluation is always normalized to White's perspective in Rust (chess.rs:239-241) and flipped per mover with `normalizeScore(score, color)` in TS (score.ts:38-47); mates clamp to ±CP_CEILING=1000.
- Annotations are NAG glyph strings (`Annotation` union) with metadata in ANNOTATION_INFO; basic six are exclusive by `group: "basic"` when set by the user (setAnnotation tree.ts:431-438) and exported as literal glyphs or `$N` NAGs in PGN (chess.ts:70-77). Comments are Markdown strings edited by tiptap (`editor.getMarkdown()`) and rendered with react-markdown.
- Opening data: embedded lichess ECO TSVs (src-tauri/data/{a..e}.tsv, frc.tsv) compiled in with include_bytes! and parsed at first use (opening.rs:117-169).

## Gotchas
- Index semantics: `analysis[i]` corresponds to the i-th non-terminal position in mainline order (root is index 0), not to ply i: game-over positions are skipped when building `fens` (chess.rs:514 `if !chess.is_game_over()`), so a final checkmate/stalemate node gets no `score`/annotation (EvalChart special-cases unscored leaves at EvalChart.tsx:63-72). `node.score` set by addAnalysis is the evaluation of the position AFTER that node's move, from White's POV (tree.ts:709).
- The report only covers the mainline from `root` (ReportPanel.tsx:50 getMainLine; addAnalysis walks `children[0]` only, tree.ts:702-807) regardless of the currently selected variation; variations are never scored.
- Re-running a report is additive: annotations are appended then Set-deduped (tree.ts:732, 800) but a previously added "?" is not removed if the new result is "" or "??" (unlike `setAnnotation` which replaces same-group glyphs), and best-line variations added earlier are not removed (only a same-move child prevents duplication, :746-756).
- Live engine analysis overwrites report scores: EvalListener calls `setScore(ev[0].score)` on the current node whenever the first engine reports (EvalListener.tsx:158-162), so browsing a reported game with engines enabled mutates `node.score` and therefore `getGameStats` (ACPL/accuracy) and the chart. `TreeNode.depth` exists but addAnalysis never sets it (always null).
- `MoveAnalysis` has no `#[serde(rename_all = "camelCase")]` so the TS field is `is_sacrifice` while `AnalysisOptions` fields are camelCase (`annotateNovelties`, `referenceDb`); `BestMoves` uses explicit `#[serde(rename)]` for uciMoves/sanMoves. src/utils/chess.ts:21-34 re-declares `BestMoves`/`MoveAnalysis` interfaces that drift from the bindings (missing `nodes`).
- `is_sacrifice` is a crude static-exchange heuristic (capture-only quiescence over material, P=90 N=B=300 R=500 Q=1000, checkmate=-10000; chess.rs:636-708) evaluated on the position before vs after the move with a >100 threshold (:518); it fires for any material-losing move including plain blunders, which is why `!!`/`!?` additionally require the engine's only-move/eval conditions (score.ts:97-116).
- Classification thresholds are win-chance based (lichess formula `50 + 50*(2/(1+exp(-0.00368208*cp))-1)`, score.ts:34-36): >20% drop ⇒ "??", >10 ⇒ "?", >5 ⇒ "?!" (score.ts:87-95). `!!`/`!` require MultiPV≥2 (forced to exactly 2 in analyze_game chess.rs:545-557, capped to legal-move count in set_options :104) and SAN equality with `prevMoves[0].sanMoves[0]` (:101). Accuracy = clamp(103.1668*exp(-0.04354*(winBefore-winAfter))-3.1669+1, 0, 100) (score.ts:60-67); game accuracy = harmonic mean (misc.ts:89-92, with `Math.max(1, b)` guard), ACPL = plain mean of max(0, prevCP-nextCP) with mates clamped to ±1000 (score.ts:69-73). Root with no score uses INITIAL_SCORE cp +15 (chess.ts:571).
- Novelty: only the FIRST position not found in the reference DB is flagged (chess.rs:617-628 `novelty_found`), checked in forward order for every position including the root; it requires a reference database (`referenceDbAtom`) else `Error::MissingReferenceDatabase` fails the whole report. The UI text ("Add a comment to the first position...", en-US.json:39) is misleading: the implementation adds the "N" NAG annotation (tree.ts:797-799), never a comment. is_position_in_db may trigger building a search index on first use (db/search.rs:494-509) — slow.
- Progress: `update_progress` emits one `progress-event` per position with `progress = i/len*100` and `finished=false`, and a final `100, true` (chess.rs:537-543, 631); the analysis result itself is not streamed. Cancellation resolves the promise with `status: "error"` ("Analysis cancelled") and nothing is written; ReportModal only acts on `status === "ok"` and never surfaces the error to the user (ReportModal.tsx:96-103). `useProgress` first fetches stored progress (`getProgress`) so a finished item persists until `clearProgress` (useProgress.ts:7-13, 30-33; ProgressButton clears on cancel only).
- `reversed: true` (default) analyzes from the last position backwards (chess.rs:524-526) to exploit engine hash reuse; any per-position streaming or early commentary would naturally arrive last-to-first.
- `goToAnnotation` (tree.ts:158-184) loops forever (`while (true)`) if no node of that colour carries the annotation; callers guard with count > 0 (ReportPanel.tsx:125,148).
- Opening lookup is an exact `Setup` equality against positions reached by replaying each TSV PGN (opening.rs:76-82, 140-150); only the deepest matching fen along the path wins (:65-74). ECO code is read into `_eco` but never exposed; `GameHeaders.eco` (treeReducer.ts:154) is parsed from PGN but no component renders it. There is no 'book move' classification anywhere.
- Comments: `node.comment` is Markdown produced by tiptap (`getMarkdown()`), rendered through react-markdown with rehype-raw (HTML allowed) (Comment.tsx), exported inside PGN `{}` together with `[%eval]`/`[%clk]`/`[%csl]`/`[%cal]` (chess.ts:81-125), and on import `[%timestamp N]` is stripped (chess.ts:404). Any machine-generated commentary stored there will be editable/overwritable by the user and indistinguishable from their notes.
- Persistence: tree store is persisted per tab to sessionStorage via zustand `persist` with a debounced storage (tree.ts:516-523); files are saved as PGN via getPGN — new TreeNode fields are not saved to disk unless encoded in PGN.
- HTTP from the webview is restricted by the Tauri capability allow list (src-tauri/capabilities/main.json:42-53: encroissant.org, chess.com, lichess.org, explorer/tablebase.lichess.org, chessdb.cn); any LLM endpoint must be added there or called from Rust (reqwest available).
- Per-position engine budget defaults to 500 ms (`goMode {t:"Time", c:500}`) and MultiPV is clamped to 2, so the data available for commentary is: best and second-best line (PV up to engine depth, SAN + UCI), their scores (cp/mate, optional WDL permille), depth/nodes/nps — nothing about threats, hanging pieces, or the played move's own PV unless the played move happens to be one of the two lines.

## Open questions
- Exact fields of the vampirc-uci fork's `Score` struct (chess.rs:373-374 reads `score.lower_bound`/`upper_bound`, but the generated TS `Score` exposes only `value` and `wdl`): the fork (Cargo.toml git rev 0d2b70d1) is not vendored in the clone, so whether bound flags are serde-skipped could not be verified.
- Whether shakmaty 0.27 `Setup` equality used in `get_opening_from_setup` (opening.rs:79) includes halfmove/fullmove counters (which would make transpositions with different move counts miss the opening table) — shakmaty source not available offline.
- How `createDebouncedSessionStorage` (src/state/store/debouncedStorage.ts, not read) partializes or size-limits persisted tree state; relevant if commentary text is stored on nodes.
- The file-save path (`userSaveFile` in BoardAnalysis.tsx, not read) was assumed to serialize via `getPGN` only; confirm no JSON sidecar exists that could carry extra per-node fields.
- No tests exist for `getGameStats` or `getGamePhases` (src/utils/tests/chess.test.ts has none); baseline behaviour would have to be pinned before refactoring.
- Whether maintainers would accept storing an API key in localStorage (precedent: lichess accessToken in `sessionsAtom`, atoms.ts:213) versus a Rust-side secure store — no keychain/secure-storage dependency exists in Cargo.toml.
- Is `TreeNode.depth` intentionally unused by the report (never set in addAnalysis) — it may be reserved for the live engine; no writer was found in tree.ts.
