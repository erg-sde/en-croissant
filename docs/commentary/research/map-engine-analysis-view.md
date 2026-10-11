# Live engine analysis view ("Analysis" tab → Engines/Report/Logs) and board-page panel layout (react-mosaic + Portals + Mantine Tabs) in En Croissant v0.15.1

## Summary
Layout. Route "/" (src/routes/index.tsx:4-7) renders BoardsPage. BoardsPage (src/components/tabs/BoardsPage.tsx) uses react-mosaic-component (import :8). The mosaic holds only three empty anchor divs `#left`, `#topRight`, `#bottomRight` (`fullLayout` :269-273, `ViewId` :267) and persists the split tree in `windowsStateAtom = atomWithStorage<WindowsState>("windowsState", …)` (:279-289). For an "analysis" tab, `TabSwitch` (:291-347, analysis branch :319-334) renders `<TreeStateProvider id={tab.value}>` → `<Mosaic>` + `<BoardAnalysis/>`. BoardAnalysis (src/components/boards/BoardAnalysis.tsx) does NOT use mosaic itself; it mounts `<EvalListener/>` at :174 (outside any tab, so engines run regardless of the visible tab) and three Mantine `<Portal target="#left|#topRight|#bottomRight">` (:175, :183, :252). `#topRight` holds a `<Paper>` with Mantine `<Tabs value={currentTabSelected} keepMounted={false}>` (:191-249); `Tabs.List grow` lists practice (repertoire only) / analysis / database / annotate / info (:213-231) with `t("Board.Tabs.*")` labels and `Tabs.Panel`s (:232-248) → PracticePanel, InfoPanel, DatabasePanel, AnnotationPanel, AnalysisPanel. The selected tab lives in jotai `currentTabSelectedAtom = tabValue(tabFamily)` (src/state/atoms.ts:374-375, default "info"). AnalysisPanel (src/components/panels/analysis/AnalysisPanel.tsx) nests a vertical `<Tabs orientation="vertical" placement="right" keepMounted={false}>` with sub-tabs engines/report/logs (:101-107) selected via `currentAnalysisTabAtom = tabValue(analysisTabFamily)` (atoms.ts:422-423, default "engines"); the engines panel shows TablebaseInfo (gated by piece count :123-130), an EngineSummary strip when >1 engine (:131-155), and a drag-reorderable Accordion of `<BestMoves>` cards (:157-221); the report panel is `<ReportPanel/>` (:257). Keybinds: `keyMapAtom` (src/state/keybinds.ts:60) with defaults at :14-58 (e.g. `ANALYSIS_TAB: "a"`, `GENERATE_REPORT: "f12"`, `TOGGLE_ALL_ENGINES: "ctrl+a"`); `defaultStorage.getItem` merges new default keys into a user's stored map (:70-74), so adding a key is backwards compatible. BoardAnalysis wires them with `useHotkeys` from @mantine/hooks (:133-170); `GENERATE_REPORT` sets tab "analysis", sub-tab "report" and opens the modal via `currentReportModalOpenAtom` (:151-159).

Engine request path (live). The per-tab zustand store (TreeStateContext, src/components/common/TreeStateContext.tsx; `createTreeStore(id)` in src/state/store/tree.ts:95-526, persisted with a 300 ms debounced sessionStorage, tree.ts:516-525 + debouncedStorage.ts:3) holds `root: TreeNode`, `position: number[]` (path of child indices), `headers`, `dirty`, `report.inProgress`. EvalListener (src/components/boards/EvalListener.tsx) derives `fen = root.fen` and `moves = getVariationLine(root, position)` (UCI list from root, :36-41, chess.ts:148-170), computes `finalFen`, and (when `currentThreatAtom` is on) swaps side to move (:58-71). It renders one `<EngineListener>` per `engines.filter(e => e.loaded)` (:80-95). Each EngineListener (a) subscribes to the tauri-specta event `events.bestMovesPayload.listen` (:136) and accepts a payload only if `payload.engine === engine.id && payload.tab === activeTab && payload.fen === searchingFen && equal(payload.moves, searchingMoves) && settings.enabled && !isGameOver` (:138-145), then inside `startTransition` writes `engineMovesFamily({engine, tab})` Map under key `${searchingFen}:${searchingMoves.join(",")}` (:146-156), sets `engineProgressFamily` (:157), and if this engine is `firstEngineWithLinesFamily` (or none) calls store `setScore(ev[0].score)` which writes `node.score` on the current node and marks `dirty` (:158-162, tree.ts:478-487); (b) runs `useThrottledEffect(…, 50, deps)` (misc.ts:72-83, actually a trailing debounce) that calls the per-engine-type `getBestMoves` chosen by ts-pattern `match(engine.type)` (:181-193): local → `commands.getBestMoves(engine.id, engine.path, tab, goMode, options)` (src/utils/engines.ts:84-93), "chessdb" → src/utils/chessdb/api.ts:31-67, "lichess" → cloud-eval src/utils/lichess/api.tsx:238-287; the returned `[progress, BestMoves[]] | null` seeds the same Map (:208-222). Rust side (src-tauri/src/chess.rs): `get_best_moves` (:319-446) keys processes by `(tab, engine_id)` in `AppState.engine_processes: DashMap<(String,String), Arc<Mutex<EngineProcess>>>` (main.rs:91). If a process exists and `options == process.options && go_mode == process.go_mode && process.running` it returns `Some((last_progress, last_best_moves))` (:338-343); otherwise `stop`, sleep 50 ms, `set_options` (sends UCI setoption for changed extra options, `UCI_Chess960`, `position fen … moves …` via normalized UCI, process.rs:130-138; sets `real_multipv = min(MultiPV, legal_moves)`, :104), `go` and returns `None` (:344-354). On first spawn it loops over stdout lines, parses `info` with vampirc-uci into `BestMoves` (`parse_uci_attrs` :193-242: PV converted to both UCI and SAN via shakmaty `SanPlus`; score inverted to White-relative when Black to move :237-239; lines with lower/upper bound skipped :373-377), buffers one entry per multipv index, and when all `real_multipv` lines are at the same depth ≥ `last_depth` and the 5/s `governor` RateLimiter allows (:365, :386) emits `BestMovesPayload { best_lines, engine: id, tab, fen, moves, progress }` (:402-410); progress = depth/time/nodes ratio or 99.99 for Infinite (:388-401). On `bestmove` it emits the last lines with `progress: 100.0` (:427-438). Multiple engines are therefore multiplexed by a single global event stream filtered client-side by `(engine, tab, fen, moves)`.

Report path. ReportPanel (src/components/panels/analysis/ReportPanel.tsx) shows a `ProgressButton id={`report_${activeTab}`}` (:56-71) driven by `useProgress(id)` which listens to `events.progressEvent` (src/hooks/useProgress.ts:15-28); opening the modal sets `currentReportModalOpenAtom`. ReportModal (ReportModal.tsx) persists settings in `reportSettingsAtom` (:13-19: novelty, reversed, variations, goMode default `{t:"Time", c:500}`, engine id) and on submit calls `commands.analyzeGame(`report_${tab}`, engine.path, goMode, { annotateNovelties, fen: initialFen, referenceDb, reversed, moves: getMainLine(root) }, engineSettings)` (:82-95), then `addAnalysis(analysis.data, { showVariations })` (:98-100). Rust `analyze_game` (chess.rs:474-634) spawns a fresh process, builds `fens: Vec<(Fen, Vec<String>, bool /*is_sacrifice via naive_eval qsearch*/)>` for every non-terminal position (:500-522), forces `MultiPV=2` (:545-557), runs `go` per position until `bestmove`, keeps the deepest complete MultiPV set as `MoveAnalysis.best` (:568-598), reports progress via `update_progress` (:537-543), optionally marks the first novelty against the reference DB (:606-630), and returns `Vec<MoveAnalysis>` (index i = position after i plies of the mainline). `addAnalysis` (tree.ts:693-809) walks the mainline (`cur = cur.children[0]`), sets `cur.score = analysis[i].best[0].score`, computes `getAnnotation(prevprev, prev, cur, color, prevMoves, is_sacrifice, san)` (score.ts:75-119), appends "N" for novelties, and for ?!/?/?? optionally inserts a variation node chain from the previous position's best PV (≤10 plies, :734-795). The raw `MoveAnalysis[]` is not retained anywhere after this.

Display of lines. BestMoves (BestMoves.tsx) receives `{id, engine, fen: rootFen, moves, halfMoves, orientation}` (:58-66), reads `engineMovesFamily`/`engineProgressFamily`/`tabEngineSettingsFamily` (:79-90), replays `moves` on `fen` to get `finalFen` (:123-136), looks up `ev.get(`${searchingFen}:${searchingMoves.join(",")}`)` wrapped in `useDeferredValue(useMemo(...))` (:153-158), and renders `EngineTop` (name, nps, Eval = `formatScore(engineVariations[0].score.value, 1)`, depth, nodes; :325-397) plus one `<AnalysisRow engine score moves={engineVariation.sanMoves} halfMoves threat fen orientation/>` per line (:300-317). AnalysisRow (AnalysisRow.tsx) replays SAN moves with chessops to produce per-move `{fen, san, lastMove, isCheck}` (:47-59), shows a `ScoreBubble` (cp or WDL per `scoreTypeFamily(engine)`, :72-83) and clickable `MoveCell`s that call store `makeMoves({ payload: moves.slice(0, index+1) })` (:201-205) with a hover board preview Portal. The eval bar on the board reads `currentNode.score` (Board.tsx:464 → EvalBar.tsx); arrows come from derived atom `bestMovesFamily({fen: rootFen, gameMoves: moves})` (atoms.ts:556-601, Board.tsx:113-118).

## Key files
- `src/components/tabs/BoardsPage.tsx` — Top-level tab bar + react-mosaic layout; creates per-tab TreeStateProvider and renders BoardAnalysis/BoardGame/Puzzles into mosaic anchors
  - BoardsPage (default export) :30
  - closeTab → commands.killEngines(value) :47-74 (:69)
  - type ViewId = "left"|"topRight"|"bottomRight" :267
  - fullLayout anchors <div id="left|topRight|bottomRight"/> :269-273
  - windowsStateAtom = atomWithStorage<WindowsState>("windowsState", …) :279-289
  - TabSwitch :291-347 (analysis branch :319-334)
- `src/components/boards/BoardAnalysis.tsx` — Analysis-mode page: mounts EvalListener, portals Board/#left, panel Tabs/#topRight, DetachedEval+GameNotation+MoveControls/#bottomRight; wires tab/annotation/report keybinds
  - BoardAnalysis :45
  - useHotkeys tab/report/engine bindings :137-170 (GENERATE_REPORT :151-159, ANALYSIS_TAB :150)
  - <EvalListener/> :174
  - Portal #topRight + <Tabs value={currentTabSelected} keepMounted={false}> :183-251
  - Tabs.List practice/analysis/database/annotate/info :213-231
  - Tabs.Panel list :232-248
  - Portal #bottomRight: DetachedEval, GameNotation, MoveControls :252-277
- `src/components/panels/analysis/AnalysisPanel.tsx` — Analysis tab body: vertical sub-tabs engines/report/logs; tablebase card; multi-engine summary; draggable BestMoves accordion; engine selection popover
  - AnalysisPanel :49 (memo export :342)
  - currentNodeFen/moves/currentNodeHalfMoves selectors :53-66
  - Tabs.List engines/report/logs :101-107
  - TablebaseInfo gate (≤7 pieces or 8 w/ captures|isOp1) :123-130
  - EngineSummary strip :131-155, EngineSummary :294-340
  - Accordion + DragDropContext of <BestMoves …> :157-221
  - reorderEngines :275-292
  - ReportPanel sub-tab :248-258
- `src/components/panels/analysis/BestMoves.tsx` — One engine card: enable toggle, threat toggle, pin (detached), settings collapse, progress bar, lines table
  - arrowColors :51-56
  - BestMovesProps {id, engine, fen, moves, halfMoves, dragHandleProps, orientation} :58-66
  - BestMovesComponent :68
  - settings sync to enginesAtom :92-115
  - searchingFen/searchingMoves via match(threat) :138-151
  - engineVariations = useDeferredValue(useMemo(ev.get(key))) :153-158
  - AnalysisRow mapping :300-317
  - EngineTop (depth/nodes/nps/eval header) :325-397
  - memo comparator :399-408
- `src/components/panels/analysis/AnalysisRow.tsx` — Renders a single PV line with ScoreBubble, clickable SAN cells, hover board preview, copy button
  - AnalysisRow props {engine, score: Score, moves: string[] (SAN), halfMoves, threat, fen, orientation} :21-37
  - SAN replay → moveInfo[{fen,san,lastMove,isCheck}] :47-59
  - analysis-panel-scroll listener :63-68
  - scoreTypeFamily(engine) :72
  - BoardPopover → makeMoves({payload: moves.slice(0,index+1)}) :158-238 (:201-205)
- `src/components/panels/analysis/ScoreBubble.tsx` — Score chip (cp text or WDL bar)
  - ScoreBubble({size, score: Score, evalDisplay?: "cp"|"wdl", setEvalDisplay?}) :6-16
  - WDL branch (permille /10) :17-59
  - cp branch formatScore(score.value) :60-86
- `src/components/panels/analysis/TablebaseInfo.tsx` — Lichess tablebase card for ≤7-piece positions
  - TablebaseInfo({fen, turn}) :12
  - useSWRImmutable(["tablebase", fen], getTablebaseInfo) :16-19
  - sortedMoves :21-35
  - makeMove({payload: parseUci(m.uci)}) on click :72-74
  - OutcomeBadge :100-153
- `src/components/panels/analysis/ReportPanel.tsx` — Report sub-tab: Generate report button (ProgressButton), accuracy cards, EvalChart, annotation stats
  - ReportPanel :19
  - root = useStore(store, s => s.root) :25
  - stats = useMemo(getGameStats(root)) :31
  - handleCancel → commands.cancelAnalysis(`report_${activeTab}`) :33-35
  - <ReportModal tab initialFen moves={getMainLine(root)} …/> :47-54
  - <ProgressButton id={`report_${activeTab}`} …/> :56-71
  - GameStats memo :100-168
  - AccuracyCard :170-191
- `src/components/panels/analysis/ReportModal.tsx` — Report settings form and analyzeGame invocation
  - reportSettingsAtom (novelty, reversed, variations, goMode {t:"Time",c:500}, engine) :13-19
  - ReportModal :21
  - analyze(): commands.analyzeGame(`report_${tab}`, engine.path, goMode, {annotateNovelties, fen, referenceDb, reversed, moves}, engineSettings) :72-104 (call :82-95, addAnalysis :98-100)
- `src/components/boards/EvalListener.tsx` — Headless component that drives engine searches and ingests best-moves events for every loaded engine
  - EvalListener :32
  - moves = useShallow(getVariationLine(root, position)) :38-41
  - searchingFen/searchingMoves (threat swap) :58-71
  - firstEngineWithLinesFamily :73-78
  - EngineListener :98-245
  - events.bestMovesPayload.listen filter :136-145
  - startTransition → setEngineVariation Map + setProgress + setScore :146-163
  - getBestMoves dispatch by engine.type :181-193
  - useThrottledEffect(…, 50, deps) request/stop :195-243
- `src/components/common/DetachedEval.tsx` — Pinned single-engine eval strip above notation (reads same atoms as BestMoves)
  - DetachedEval :25
  - DetachedEvalInner :47-162
  - ev = engineMovesFamily :79
  - engineVariations useDeferredValue :102-107
- `src/components/boards/EvalBar.tsx` — Vertical eval bar from currentNode.score
  - EvalBar({score: Score|null, orientation}) :8
  - getWinChance progress :76-77
- `src/components/boards/Board.tsx` — Chessground board; consumes bestMovesFamily for arrows, currentNode for eval bar/annotation
  - moves = useShallow(getVariationLine) :106-109
  - currentNode = useStore(store, s => s.currentNode()) :111
  - arrows = useAtomValue(bestMovesFamily({fen: rootFen, gameMoves: moves})) :113-118
  - variation arrows :294-312
  - <EvalBar score={currentNode.score}/> :464
  - memo(Board) :634
- `src/utils/engines.ts` — Engine zod schemas/types and local-engine command wrappers
  - requiredEngineSettings = ["MultiPV","Threads","Hash"] :8
  - goModeSchema :10-26
  - engineSettingsSchema / EngineSettings :28-35
  - localEngineSchema / LocalEngine :37-53
  - remoteEngineSchema / RemoteEngine (type "chessdb"|"lichess") :55-67
  - engineSchema / Engine :69-70
  - stopEngine :72-76, killEngine :78-82
  - getBestMoves(engine, tab, goMode, options) → commands.getBestMoves(engine.id, engine.path, tab, goMode, options) :84-93
  - useDefaultEngines (fetch encroissant.org) :95-113
- `src/state/atoms.ts` — All jotai atoms: tabs, settings, per-tab families, engine line storage, derived best-move atoms
  - tabsAtom :58-62, activeTabAtom :64-68, currentTabAtom :76-95
  - enginesAtom (file storage engines/engines.json, unwrap(atomWithStorage)) :153-159
  - tabValue(family) helper :310-329
  - reportTypeAtom :350, scoreTypeFamily :352
  - threatFamily/currentThreatAtom :356-357
  - evalOpenFamily/currentEvalOpenAtom :359-360, evalBarDisplayFamily :362-363
  - showCommentsFamily/currentShowCommentsAtom :368-369
  - tabFamily/currentTabSelectedAtom (default "info") :374-375
  - reportModalOpenFamily/currentReportModalOpenAtom :377-378
  - analysisTabFamily/currentAnalysisTabAtom (default "engines") :422-423
  - expandedEnginesFamily/currentExpandedEnginesAtom :428-429
  - currentDetachedEngineAtom :431
  - engineMovesFamily({tab, engine}) → atom<Map<string, BestMoves[]>> :544-548
  - engineProgressFamily → atom<number> :550-553
  - bestMovesFamily({fen, gameMoves}) → Map<number,{pv,winChance}[]> :556-601
  - firstEngineWithLinesFamily :603-634
  - tabEngineSettingsFamily({tab, engineId, defaultSettings?, defaultGo?}) → {enabled, settings, go, synced} :636-661
  - allEnabledAtom :663-680, enableAllAtom :682-695
  - sessionsAtom (lichess token precedent) :213
- `src/state/keybinds.ts` — Keybind defaults and persisted keyMapAtom with default-merging storage
  - keys map :14-58 (PRACTICE_TAB :39, ANALYSIS_TAB :40, DATABASE_TAB :41, ANNOTATE_TAB :42, INFO_TAB :43, TOGGLE_ALL_ENGINES :53, GENERATE_REPORT :55)
  - keyMapAtom :60
  - defaultStorage merge of new defaults :62-88 (:70-74)
- `src/state/store/tree.ts` — Per-tab zustand game-tree store (navigation, moves, annotations, comments, score, addAnalysis)
  - TreeStoreState :26-91 (currentNode :27, goToMove :34, makeMove :44-50, makeMoves :54, setAnnotation :63, setComment :64, setScore :68, addAnalysis :74-81, setReportInProgress :83)
  - createTreeStore(id?, initialTree?) :95-526 (persist + createDebouncedSessionStorage :516-523)
  - currentNode = getNodeAtPath(root, position) :99
  - setComment :443-452
  - setScore (sets node.score, dirty=true) :478-487
  - addAnalysis action :488-494
  - makeMove impl :528-609
  - addAnalysis impl (mainline walk, getAnnotation, variation insertion) :693-809
- `src/utils/treeReducer.ts` — Tree data types and helpers
  - TreeState :8-14
  - TreeNode :16-28
  - treeIterator :35-44, findFen :46-54, treeIteratorMainLine :56-65, countMainPly :67-75
  - defaultTree :77-108, createNode :110-136
  - GameHeaders :138-160
  - getNodeAtPath :172-181
  - ReportState :196-198
- `src/utils/score.ts` — Score math and move classification used by report
  - INITIAL_SCORE :7-13
  - formatScore :17-32
  - getWinChance :34-36
  - normalizeScore (mate→±1000, mover perspective) :38-47
  - getAccuracy :60-67, getCPLoss :69-73
  - getAnnotation(prevprev, prev, next, color, prevMoves, is_sacrifice?, move?) :75-119
- `src/utils/chess.ts` — PGN/tree utilities: variation line extraction, PGN text with comments/[%eval], stats, opening lookup
  - getMoveText (comments + [%eval] in braces) :48-127
  - getLastMainlinePosition :129-137, getMainLine :139-141
  - uciNormalize :143-146
  - getVariationLine(root, position, includeLastMove) :148-170
  - getPGN :218-306
  - getOpening(root, position) → commands.getOpeningFromFens :333-351
  - parsePGN / innerParsePGN ([%eval] → node.score :370-388) :353-476
  - getGameStats :541-612
  - getMaterialDiff :622-654
- `src/utils/misc.ts` — Small helpers incl. the engine request debounce
  - useThrottledEffect(callback, delay, deps) :72-83
  - mean :85, harmonicMean :89
  - isPrefix :35, takeWhile :47, skipWhile :57
- `src/bindings/generated.ts` — tauri-specta generated commands/events/types (do not edit; regenerated by debug cargo build)
  - commands.getBestMoves(id, engine, tab, goMode, options) → Result<[number, BestMoves[]] | null, string> :16-23
  - commands.analyzeGame(id, engine, goMode, options: AnalysisOptions, uciOptions: EngineOption[]) → Result<MoveAnalysis[], string> :24-31
  - commands.cancelAnalysis(id) :32
  - commands.stopEngine(engine, tab) :40, killEngine :48, killEngines(tab) :56, getEngineLogs(engine, tab) :64, getEngineConfig(path) :123
  - events = __makeEvents__<{bestMovesPayload: BestMovesPayload, progressEvent: ProgressEvent, …}>({bestMovesPayload: "best-moves-payload", progressEvent: "progress-event", …}) :468-483
  - AnalysisOptions :490, BestMoves :491, BestMovesPayload :492, EngineConfig :497, EngineLog :498, EngineOption :499, EngineOptions :500, GoMode :514, MoveAnalysis :515, ProgressEvent :528, Score :534-538, ScoreValue :539-547, UciOptionConfig :559
- `src/bindings/index.ts` — Re-exports generated types and widens Score to include dtz
  - ScoreValue = ScoreValueT | { type: "dtz"; value: number } :10
  - Score = Omit<ScoreT, "value"> & { value: ScoreValue } :11
  - BestMoves = Omit<BestMovesT, "score"> & { score: Score } :12-14
- `src-tauri/src/chess.rs` — Engine process orchestration, UCI info parsing, live get_best_moves streaming, whole-game analyze_game
  - struct EngineProcess {base, last_depth, best_moves, last_best_moves, last_progress, options, go_mode, running, real_multipv, start} :39-50
  - EngineProcess::set_options (UCI_Chess960, MultiPV clamp, setoption diff, position) :82-120
  - struct BestMoves {nodes:u32, depth:u32, score:Score, uci_moves("uciMoves"), san_moves("sanMoves"), multipv:u16, nps:u32} :154-167
  - struct BestMovesPayload {best_lines, engine, tab, fen, moves, progress:f64} (#[serde(rename_all="camelCase")], tauri_specta Event) :169-178
  - invert_score :180-191
  - parse_uci_attrs(attrs, fen, moves) → BestMoves (SAN via SanPlus, invert for Black) :193-242
  - struct EngineOptions {fen, moves, extra_options} :244-251
  - kill_engines :253-272, kill_engine :274-287, stop_engine :288-301, get_engine_logs :303-317
  - get_best_moves(id, engine(path), tab, go_mode, options, app, state) :319-446 (reuse/stop/restart :334-355, RateLimiter 5/s :365, emit :402-410, bestmove emit progress 100 :427-438)
  - struct MoveAnalysis {best, novelty, is_sacrifice} :448-453
  - struct AnalysisOptions {fen, moves, annotate_novelties, reference_db, reversed} :455-463
  - cancel_analysis :465-472
  - analyze_game(id, engine, go_mode, options, uci_options, state, app) :474-634 (fens+is_sacrifice :500-522, MultiPV=2 :545-557, progress :537-543, novelty :606-630)
  - count_material/qsearch/naive_eval :636-713
  - get_engine_config :781
- `src-tauri/src/engine/process.rs` — Generic UCI child-process wrapper with log capture
  - enum EngineLog {Gui(String), Engine(String)} (tag "type", content "value", camelCase) :19-24
  - type EngineReader :26
  - struct BaseEngine {stdin, reader, child, logs} :28-34
  - spawn :37-69, init_uci :91-97, send :99-104, wait_for :106-120, set_option :122-128, set_position (normalize_uci_moves_for_fen) :130-138, go :140-143, stop :145-147, quit :149-151, wait_for_bestmove :153-162, kill_sync :164-166
- `src-tauri/src/engine/types.rs` — GoMode/EngineOption types shared with TS
  - struct EngineOption {name, value} :5-8
  - enum GoMode (#[serde(tag="t", content="c")]) PlayersTime|Depth(u32)|Time(u32)|Nodes(u32)|Infinite :10-18
  - GoMode::to_uci_string :20-35
  - struct PlayersTime :37-43
- `src-tauri/src/engine/uci.rs` — FEN/UCI helpers (shakmaty)
  - parse_fen_to_position :5-13
  - apply_uci_moves :15-22
  - parse_fen_and_apply_moves :24-28
  - normalize_uci_moves_for_fen :30-49
- `src-tauri/src/engine/mod.rs` — Module re-exports
  - pub use process::{BaseEngine, EngineLog, EngineReader}; pub use types::*; pub use uci::*; :5-7
- `src-tauri/src/main.rs` — AppState, command/event registration, TS bindings export
  - struct AppState {…, engine_processes: DashMap<(String,String), Arc<tokio::sync::Mutex<EngineProcess>>>, analysis_cancel_flags, progress_state, …} :76-96
  - tauri_specta::Builder::new().commands(collect_commands!(…get_best_moves, analyze_game, cancel_analysis, stop_engine, kill_engine, kill_engines, get_engine_logs, …)) :110-172
  - .events(collect_events!(BestMovesPayload, DatabaseProgress, ProgressEvent, GameMoveEvent, ClockUpdateEvent, GameOverEvent)) :173-180
  - #[cfg(debug_assertions)] export to ../src/bindings/generated.ts :182-188
  - kill engines on ExitRequested :250-259
- `src/utils/lichess/api.tsx` — External HTTP (tauri plugin-http) to lichess: cloud eval, explorer, tablebase, games
  - fetch from @tauri-apps/plugin-http :5
  - baseURL/explorerURL/tablebaseURL :30-32
  - TablebaseCategory :36-44, TablebaseMove :59-72
  - getBestMoves(_tab, _goMode, options) (cloud-eval → BestMoves[]) :238-287
  - cache Map + getCloudEvaluation(fen, multipv) :289-320
  - getLichessGames :322-346, getMasterGames :348-367
  - getTablebaseInfo(fen) :416-424
- `src/utils/chessdb/api.ts` — chessdb.cn remote engine adapter
  - getBestMoves(_tab, _goMode, options) :31-67
  - chessDBevalToScore (cp/mate/dtz) :69-90
  - cache Map / queryPosition / queryBest :102-156
- `src/utils/http.ts` — User-Agent header helper for all external requests
  - initUserAgent :8-15
  - apiHeaders(extra?) :17-22
- `src/components/panels/database/DatabasePanel.tsx` — Opening explorer (local DB / lichess) — reference pattern for debounced, SWR-cached external fetch keyed by FEN
  - fetchOpening(db: DBType, tab) via match().exhaustive() :73-109
  - fen = useStore(store, s => s.currentNode().fen) :116
  - useDebouncedValue(fen, 50) :117
  - explorerToken from sessionsAtom :120-121
  - useSWR(key | null, fetcher) :166-176
- `src/components/common/Comment.tsx` — Renders node.comment as Markdown (react-markdown + rehype-raw + remark-gfm) inside notation
  - normalizeTiptapMarkdown :8-10
  - Comment({comment}) :12-35
  - memo export :37
- `src/components/panels/annotation/AnnotationEditor.tsx` — Tiptap markdown editor bound to currentNode.comment via setComment
  - root/position/setComment selectors :18-20
  - useEditor(… content: currentNode.comment, contentType: "markdown", onUpdate → setComment(editor.getMarkdown()) …, [position.join(",")]) :24-44
- `src/components/common/GameNotation.tsx` — Move list; shows comments inline when currentShowCommentsAtom
  - currentFen selector + autoscroll :51-74
  - rootComment render :112-115
  - RenderVariationTree memo (node = getNodeAtPath(s.root, nodePath)) :189-264 (comparator :259-263)
  - CommentItem segment type :275-279
- `src/components/common/OpeningName.tsx` — Pattern for recomputing something on navigation
  - useEffect(() => getOpening(root, position)…, [root, position]) :13-15
- `src/components/common/EvalChart.tsx` — Eval graph over mainline node.score with phase dividers
  - getYValue (sigmoid of cp) :55-74
  - getNodes via treeIteratorMainLine :98-103
  - getData :105-128
  - onChartClick → goToMove(movePath) :140-145
  - phases useMemo(getGamePhases) :150
- `src/utils/phase.ts` — Lichess-style opening/middlegame/endgame divider
  - GamePhases {middlegamePly, endgamePly} :72-75
  - getGamePhases(boards: Chess[]) :77-97
- `src/utils/annotation.ts` — Annotation glyph set and NAG metadata
  - type Annotation :12-38
  - NAG_INFO :40-73
  - ANNOTATION_INFO (group, name, translationKey, color, nag) :83-190
  - isBasicAnnotation :192-196
- `src/hooks/useProgress.ts` — Progress event hook used by ProgressButton
  - useProgress(id) → {progress, finished, isActive, clear, item} :4-42
  - events.progressEvent.listen filter by id :15-28
- `src/components/common/ProgressButton.tsx` — Button with embedded progress + cancel, used for Generate report
  - Props :7-23
  - ProgressButton :25-94
- `src/components/panels/analysis/LogsPanel.tsx` — Engine UCI log viewer via useSWR + commands.getEngineLogs
  - useSWR(["logs", engine?.id, activeTab], …getEngineLogs) :19-21
- `src/components/panels/analysis/EngineSettingsForm.tsx` — Per-engine settings form (GoMode, MultiPV, Threads, Hash)
  - type Settings = {enabled, go: GoMode, settings: EngineSettings, synced} :23-28
  - MultiPV via LinesSlider :72-91
- `src/components/settings/SettingsPage.tsx` — Settings page — registry of SettingItem rows per category (template for new provider/API-key settings)
  - type SettingCategory union :78-87
  - interface SettingItem {id, category, title, description, keywords?, render} :89-96
  - SettingRow :98
  - settings: SettingItem[] useMemo :176-640 (privacy telemetry example :619-626)
  - categoryInfo :657-713
  - keybinds table from Object.entries(keyMap) + KeybindInput :922-927
- `src/translation/en-US.json` — Primary flat-key locale file
  - "Board.Analysis.*" keys :35-65
  - "Board.Tabs.Analysis|Annotate|Database|Info|Practice" :169-173
  - "Settings.Keybinds*" :551-553
- `i18next.config.ts` — i18next-cli extraction config (flat keys, 16 locales, removeUnusedKeys, --ci in lint)
  - locales :4-21
  - extract {keySeparator:false, nsSeparator:false, primaryLanguage:"en-US", defaultValue:null, removeUnusedKeys:true, preservePatterns, sort:true} :22-43
- `vite.config.ts` — Build config: React Compiler enabled, vitest jsdom
  - babel({presets:[reactCompilerPreset()]}) :19-21
  - test.environment jsdom :48-50
  - alias @ → ./src :43-47
- `src/state/store/debouncedStorage.ts` — 300 ms debounced sessionStorage persist for tree store
  - DEBOUNCE_MS = 300 :3
  - createDebouncedSessionStorage :52-74
- `src/utils/tabs.ts` — Tab schema/type and createTab/saveToFile
  - tabSchema {name, value, type: "new"|"play"|"analysis"|"puzzles", gameOrigin} :58-63
  - Tab :66
  - createTab :96-142
  - saveToFile (getPGN with comments:true) :150-240
- `src/utils/chessops.ts` — chessops helpers
  - positionFromFen :21-34
  - swapMove :36-45
  - getPiecesCount :93-102, hasCaptures :104-114, isOp1 :116-134
  - parseSanOrUci :136-148

## Data model
// ---- Rust → TS (src/bindings/generated.ts) ----
export type BestMoves = { nodes: number; depth: number; score: Score; uciMoves: string[]; sanMoves: string[]; multipv: number; nps: number }   // :491
export type BestMovesPayload = { bestLines: BestMoves[]; engine: string; tab: string; fen: string; moves: string[]; progress: number }       // :492  (event name "best-moves-payload")
export type Score = { value: ScoreValue; /** The probability of each result (win, draw, loss). */ wdl: [number, number, number] | null }   // :534-538 (wdl in permille, UI divides by 10)
export type ScoreValue = { type: "cp"; value: number } | { type: "mate"; value: number }                                                   // :539-547
export type GoMode = { t: "PlayersTime"; c: PlayersTime } | { t: "Depth"; c: number } | { t: "Time"; c: number } | { t: "Nodes"; c: number } | { t: "Infinite" } // :514
export type EngineOption = { name: string; value: string }                                                                                 // :499
export type EngineOptions = { fen: string; moves: string[]; extraOptions: EngineOption[] }                                                  // :500
export type AnalysisOptions = { fen: string; moves: string[]; annotateNovelties: boolean; referenceDb: string | null; reversed: boolean }   // :490
export type MoveAnalysis = { best: BestMoves[]; novelty: boolean; is_sacrifice: boolean }                                                   // :515 (note snake_case is_sacrifice)
export type ProgressEvent = { id: string; progress: number; finished: boolean }                                                             // :528
export type EngineLog = { type: "gui"; value: string } | { type: "engine"; value: string }                                                  // :498
export type EngineConfig = { name: string; options: UciOptionConfig[] }                                                                     // :497
// commands (generated.ts): getBestMoves(id: string, engine: string /*path*/, tab: string, goMode: GoMode, options: EngineOptions): Promise<Result<[number, BestMoves[]] | null, string>> :16
//                          analyzeGame(id: string, engine: string, goMode: GoMode, options: AnalysisOptions, uciOptions: EngineOption[]): Promise<Result<MoveAnalysis[], string>> :24
//                          cancelAnalysis(id) :32, stopEngine(engine, tab) :40, killEngine(engine, tab) :48, killEngines(tab) :56, getEngineLogs(engine, tab): Result<EngineLog[]> :64, getEngineConfig(path): Result<EngineConfig> :123
// Result<T,E> = { status: "ok"; data: T } | { status: "error"; error: E }  (unwrap() in src/utils/unwrap.tsx:7-19)

// ---- src/bindings/index.ts widening (what app code imports from "@/bindings") ----
export type ScoreValue = ScoreValueT | { type: "dtz"; value: number };          // :10
export type Score = Omit<ScoreT, "value"> & { value: ScoreValue };               // :11
export type BestMoves = Omit<BestMovesT, "score"> & { score: Score };            // :12-14

// ---- Rust source of truth (src-tauri/src/chess.rs) ----
pub struct BestMoves { nodes: u32, depth: u32, score: Score /*vampirc_uci::uci::Score {value: ScoreValue::Cp(i32)|Mate(i8), wdl: Option<(u16,u16,u16)>, lower_bound, upper_bound}*/, #[serde(rename="uciMoves")] uci_moves: Vec<String>, #[serde(rename="sanMoves")] san_moves: Vec<String>, #[derivative(Default(value="1"))] multipv: u16, nps: u32 } // :154-167
#[serde(rename_all="camelCase")] pub struct BestMovesPayload { pub best_lines: Vec<BestMoves>, pub engine: String, pub tab: String, pub fen: String, pub moves: Vec<String>, pub progress: f64 } // :169-178
#[serde(rename_all="camelCase")] pub struct EngineOptions { pub fen: String, pub moves: Vec<String>, pub extra_options: Vec<EngineOption> } // :244-251
pub struct MoveAnalysis { best: Vec<BestMoves>, novelty: bool, is_sacrifice: bool }   // :448-453
#[serde(rename_all="camelCase")] pub struct AnalysisOptions { pub fen: String, pub moves: Vec<String>, pub annotate_novelties: bool, pub reference_db: Option<PathBuf>, pub reversed: bool } // :455-463
#[serde(tag="t", content="c")] pub enum GoMode { PlayersTime(PlayersTime), Depth(u32), Time(u32), Nodes(u32), Infinite } // engine/types.rs:10-18
// Score convention: parse_uci_attrs inverts cp/mate and swaps wdl when side-to-move is Black (chess.rs:237-239, invert_score :180-191) → scores are ALWAYS White-relative.

// ---- Tree store (src/utils/treeReducer.ts) ----
export interface TreeState { root: TreeNode; headers: GameHeaders; position: number[]; dirty: boolean; report: ReportState; }               // :8-14
export interface TreeNode { fen: string; move: Move | null; san: string | null; children: TreeNode[]; score: Score | null; depth: number | null; halfMoves: number; shapes: DrawShape[]; annotations: Annotation[]; comment: string; clock?: number; } // :16-28
export interface ReportState { inProgress: boolean; }                                                                                        // :196-198
export type GameHeaders = { id: number; fen: string; event: string; site: string; date?: string|null; time?: string|null; round?: string|null; white: string; white_elo?: number|null; black: string; black_elo?: number|null; result: Outcome; time_control?: string|null; white_time_control?: string|null; black_time_control?: string|null; eco?: string|null; variant?: string|null; other?: Record<string,string>; start?: number[]; orientation?: "white"|"black"; } // :138-160
// src/state/store/tree.ts TreeStoreState extends TreeState { currentNode: () => TreeNode; getNode(path); goToNext/goToPrevious/goToStart/goToEnd/goToMove(move: number[])/goToBranchStart/…; goToAnnotation(annotation, color); makeMove({payload: string|Move, changePosition?, mainline?, clock?, changeHeaders?}); appendMove; makeMoves({payload: string[], mainline?, changeHeaders?}); deleteMove; promoteVariation; promoteToMainline; copyPgn; copyVariationPgn; setStart; setAnnotation(payload: Annotation); setComment(payload: string); setHeaders; setResult; setShapes; setScore(score: Score); clearShapes; setFen; addAnalysis(analysis: {best: BestMoves[]; novelty: boolean; is_sacrifice: boolean}[], options?: {showVariations: boolean}); setReportInProgress(value: boolean); practicePath: number[]|null; setPracticePath; setState; reset; save; } // :26-91

// ---- Engine config (src/utils/engines.ts) ----
export type EngineSettings = { name: string; value: string | number | boolean | null }[];   // :28-35
LocalEngine = { type: "local"; id: string; name: string; version: string; path: string; image?; elo?; downloadSize?; downloadLink?; loaded?: boolean|null; go?: GoMode|null; enabled?: boolean|null; settings?: EngineSettings|null } // :37-53
RemoteEngine = { type: "chessdb" | "lichess"; id; name; url; image?; loaded?; enabled?; go?; settings? } // :55-67
export type Engine = LocalEngine | RemoteEngine;  // :69-70

// ---- jotai engine atoms (src/state/atoms.ts) ----
engineMovesFamily({ tab: string; engine: string }) → atom<Map<string /* `${fen}:${uciMoves.join(",")}` */, BestMoves[]>>   // :544-548
engineProgressFamily({ tab; engine }) → atom<number>                                                                        // :550-553
tabEngineSettingsFamily({ tab; engineId; defaultSettings?: EngineSettings; defaultGo?: GoMode }) → atom<{ enabled: boolean; settings: EngineSettings; go: GoMode; synced: boolean }> // :636-661 (default enabled:false, go {t:"Infinite"})
bestMovesFamily({ fen: string; gameMoves: string[] }) → derived atom<Map<number /*engine index*/, { pv: string[]; winChance: number }[]>> // :556-601 (lines within 10 winChance of best)
firstEngineWithLinesFamily({ fen; gameMoves }) → derived atom<string | null /*engine id*/>                                   // :603-634
currentThreatAtom (bool per tab) :356-357; currentAnalysisTabAtom (string per tab, "engines") :422-423; currentTabSelectedAtom ("info") :374-375; currentReportModalOpenAtom :377-378; currentDetachedEngineAtom: string|null (localStorage) :431; scoreTypeFamily(engine) "cp"|"wdl" :352; reportTypeAtom "CP"|"WDL" :350
// Per-tab atom pattern: const xFamily = atomFamily((_tab: string) => atom(default)); export const currentXAtom = tabValue(xFamily); (tabValue :310-329 throws "No tab selected" if no currentTabAtom)

// ---- Keybinds (src/state/keybinds.ts) ----
type Keybind = { name: string; keys: string }; type KeyMap = { [key: string]: Keybind }; // :7-12  e.g. ANALYSIS_TAB: { name: "Go to analysis tab", keys: "a" } :40

// ---- Settings registry (src/components/settings/SettingsPage.tsx) ----
type SettingCategory = "board"|"inputs"|"anarchy"|"appearance"|"sound"|"keybinds"|"directories"|"repertoire"|"privacy"; // :78-87
interface SettingItem { id: string; category: SettingCategory; title: string; description: string; keywords?: string[]; render: () => React.ReactNode; } // :89-96

// ---- Score helpers (src/utils/score.ts) ----
getWinChance(cp) = 50 + 50*(2/(1+exp(-0.00368208*cp)) - 1) :34-36 ; normalizeScore(score, color) → mover-relative cp clamped ±1000, mate→±1000 :38-47 ; getAccuracy :60-67 ; getCPLoss :69-73 ; getAnnotation thresholds winChanceDiff >20 "??", >10 "?", >5 "?!"; "!!"/"!"/"!?" need prevMoves[1] (2nd best) and is_sacrifice :75-119

// ---- Lichess tablebase (src/utils/lichess/api.tsx) ----
TablebaseCategory = "win"|"unknown"|"maybe-win"|"cursed-win"|"draw"|"blessed-loss"|"maybe-loss"|"loss" :36-44 ; TablebaseData {checkmate, stalemate, variant_win, variant_loss, insufficient_material, dtz, precise_dtz, dtm, category, moves: TablebaseMove[]} :46-57 ; TablebaseMove {uci, san, zeroing, checkmate, stalemate, …, dtz, precise_dtz, dtm, category} :59-72

## Extension points
- **src/components/boards/BoardAnalysis.tsx:213-248 (Tabs.List / Tabs.Panel) + src/state/atoms.ts:374-375 (tabFamily) + src/state/keybinds.ts:14-58 + BoardAnalysis.tsx:137-170 (useHotkeys) + src/translation/en-US.json:169-173**: Register a new top-level panel: add `<Tabs.Tab value="commentary" leftSection={<IconMessage …/>}>{t("Board.Tabs.Commentary")}</Tabs.Tab>` and `<Tabs.Panel value="commentary" flex={1} style={{overflowY:"hidden"}}><CommentaryPanel/></Tabs.Panel>`; add `COMMENTARY_TAB: { name: "Go to commentary tab", keys: "…" }` to `keys` (auto-merged into existing users' stored keymap by keybinds.ts:70-74) and `[keyMap.COMMENTARY_TAB.keys, () => setCurrentTabSelected("commentary")]` to the useHotkeys list; add `"Board.Tabs.Commentary"` to en-US.json and run `pnpm i18n:extract` (writes all 16 locale files; CI `lint:ci` runs `i18next-cli extract --ci`).
  - risk: Tabs.List uses `grow` with 4–5 tabs already; a 6th tab may crowd narrow layouts (no overflow handling). Panels are `keepMounted={false}` so the panel unmounts on tab switch — any long-running commentary generation must live in a hoisted headless component (like EvalListener at :174) or in atoms, not in panel state. Maintainers may prefer a sub-tab under Analysis rather than a new top-level tab.
- **src/components/panels/analysis/AnalysisPanel.tsx:101-107 (sub-tab list) and :248-269 (panels), src/state/atoms.ts:422-423 (analysisTabFamily)**: Alternative lower-impact registration: add `<Tabs.Tab value="commentary">{t("Board.Analysis.Commentary")}</Tabs.Tab>` next to Report and a `<Tabs.Panel value="commentary" style={{display: tab==="commentary"?"flex":"none", flexDirection:"column", overflow:"hidden"}}><CommentaryPanel/></Tabs.Panel>`; the GENERATE_REPORT hotkey already routes to `setAnalysisTab("report")` (BoardAnalysis.tsx:151-159) and can be mirrored.
  - risk: Sub-tab panels are also unmounted (`keepMounted={false}` :99) and each panel wraps its own ScrollArea; the vertical Tabs.List on the right has limited vertical space for long labels.
- **src/components/panels/analysis/ReportModal.tsx:96-103 (`.then((analysis) => { if (analysis.status === "ok") addAnalysis(analysis.data, …) })`)**: This is the only place the full per-ply `MoveAnalysis[]` (2 lines per position, novelty, is_sacrifice) exists on the frontend. Hook game-level narrative + per-move explanation generation here: after `addAnalysis`, pass `analysis.data` + `store.getState().root` to a commentary service that (a) produces deterministic text from (prev/next score, best vs played PV, annotation from getAnnotation, phase from getGamePhases, material from getMaterialDiff, opening via getOpening) and (b) optionally enriches via LLM; persist results per node (see TreeNode extension) and/or write into `node.comment` via a path-addressed setter.
  - risk: `addAnalysis` only walks the mainline (`cur = cur.children[0]`, tree.ts:806) and `MoveAnalysis` index i = position after i plies with terminal positions skipped (chess.rs:512-520) — index mapping must mirror that. Writing into `node.comment` overwrites user comments (setComment tree.ts:443-452 replaces); PGN export includes comments (chess.ts:117-119, saveToFile tabs.ts:171-177) so generated text would be saved to the user's PGN and autosaved (BoardAnalysis.tsx:82-86) — needs an explicit opt-in or a separate field.
- **src/state/store/tree.ts:693-809 (`addAnalysis` pure function) and src/utils/score.ts:75-119 (`getAnnotation`)**: The deterministic classification tuple `(prevprevScore, prevScore, curScore, color, prevMoves(best 2), is_sacrifice, san)` is already computed per node at tree.ts:710-730; a `classifyMove()`/`explainMove()` helper in src/utils/ can take exactly these inputs so offline commentary stays consistent with the report glyphs. Keep it a pure function with vitest tests in src/utils/tests/ (pattern: score.test.ts).
  - risk: `addAnalysis` runs inside immer `produce` with `setAutoFreeze(false)` (index.tsx:59); it must stay synchronous — no LLM calls here. Changing getAnnotation thresholds would alter existing report behaviour (tests in src/utils/tests/score.test.ts).
- **src/utils/treeReducer.ts:16-28 (`TreeNode`) + :110-136 (`createNode`) + :77-108 (`defaultTree`) + src/utils/chess.ts:48-127 (`getMoveText`) / :353-458 (`innerParsePGN`)**: Add an optional field, e.g. `commentary?: { text: string; source: "offline" | "llm"; model?: string; generatedAt: number }`, on TreeNode and (optionally) a game-level narrative on TreeState/headers; add a store action `setCommentary(path: number[], value)` modelled on `setScore`/`setComment`. Persistence: the whole store is JSON-persisted to sessionStorage per tab (tree.ts:516-523), so the field survives tab switches; for PGN round-trip either emit it inside the `{…}` comment (getMoveText) or leave it ephemeral.
  - risk: Any extra TreeNode field must be optional to keep `createNode`/`parsePGN` and existing sessionStorage blobs valid; `src/utils/tests/store.test.ts` (not read) may assert node shapes. `TreeNode.depth` already exists but is never written anywhere — reusing it for commentary metadata would be confusing.
- **src/components/boards/EvalListener.tsx:146-163 (payload ingest) and src/state/atoms.ts:544-548 (`engineMovesFamily`)**: For live per-position commentary ('what is threatened now', 'why was this a mistake' while browsing), do NOT add a second `events.bestMovesPayload.listen`; read `engineMovesFamily({engine, tab})` for the current key `${root.fen}:${getVariationLine(root, position).join(",")}` (same lookup as BestMoves.tsx:153-158 / DetachedEval.tsx:102-107) and for the parent path `getVariationLine(root, position.slice(0,-1))`, or use `firstEngineWithLinesFamily` to pick the engine. Derive 'what changed' from `getNodeAtPath(root, position.slice(0,-1)).score` vs `currentNode().score` (both White-relative; normalize with normalizeScore(score, color)). Use `currentThreatAtom`-style side-swap (`swapMove(finalFen)`, EvalListener.tsx:58-71) to obtain 'threat' lines — the key for threat results is `${swapMove(finalFen)}:`.
  - risk: Lines arrive up to 5/s per engine; the panel must wrap lookups in `useDeferredValue(useMemo(...))` and avoid subscribing to `s.root` (every `setScore` replaces root and flips `dirty`). The parent position's live lines exist in the Map only if that path was analysed in this tab session; otherwise only `parent.score` (if ever set) is available.
- **src/components/settings/SettingsPage.tsx:78-96 (SettingCategory/SettingItem) + :176-640 (settings array) + :657-713 (categoryInfo) + src/state/atoms.ts (atomWithStorage) + en-US.json "Settings.*" keys**: Add a category (e.g. "commentary" or reuse "privacy") and SettingItems rendering a provider Select, a key TextInput (there is no PasswordInput anywhere in src — none exists to copy), model, and an on/off `SettingsSwitch atom={…}`. Back them with `atomWithStorage<…>("commentary-provider", …)` / `atomWithStorage<string>("anthropic-api-key", "")` (precedent for a secret in localStorage: lichess `accessToken` inside `sessionsAtom = atomWithStorage<Session[]>("sessions", [])`, atoms.ts:213, session.ts:4-8).
  - risk: Secrets in localStorage are readable by any webview script; no stronghold/keyring plugin is present (Cargo.toml). The settings page also has a search index (`keywords`) and a `categoryInfo` record that must include the new category or rendering throws.
- **src/utils/lichess/api.tsx:238-320 (fetch pattern) + src/utils/http.ts:17-22 + src/components/panels/analysis/TablebaseInfo.tsx:16-19 (useSWRImmutable)**: LLM/HTTP client should use `fetch` from `@tauri-apps/plugin-http`, `apiHeaders({ "x-api-key": …, "anthropic-version": … })`, a module-level `Map` cache keyed by (fen|moves|model), and be consumed through `useSWRImmutable(["commentary", key], fetcher)` or `useSWR(key|null, …)` with `useDebouncedValue(fen, 50)` (DatabasePanel.tsx:117) so navigation doesn't fire a request per keystroke. Errors surface via `notifications.show({color:"red", icon:<IconX/>})` + `error()` from @tauri-apps/plugin-log (unwrap.tsx:7-19).
  - risk: Tauri 2 `plugin-http` requires the target host to be allowed in src-tauri/capabilities (not read — see open_questions); external calls per navigation could be costly, so gate LLM calls behind an explicit button/toggle and prefer the offline generator by default. Alternatively implement the call in Rust (reqwest is a dependency) as a `#[tauri::command] #[specta::specta]` registered in main.rs:111-172 — but generated.ts only regenerates in debug builds (main.rs:182-188).
- **src-tauri/src/chess.rs:474-634 (`analyze_game`) and :545-557 (MultiPV forced to 2)**: If richer offline commentary needs more candidate moves per position (e.g. 3–4 lines to say 'better was X, also fine was Y'), raise the forced MultiPV or make it a field of `AnalysisOptions` (add `multipv: Option<u16>` with `#[serde(rename_all="camelCase")]`), then regenerate bindings and pass it from ReportModal.tsx:87-93.
  - risk: Doubles/triples report time; `getAnnotation` only inspects prevMoves[0..1] so extra lines must not change glyph logic; the `assert_eq!(proc.best_moves.len(), proc.real_multipv)` at :586 panics if lines are inconsistent.
- **src/components/common/Comment.tsx:24-32 (Markdown renderer) and src/components/common/GameNotation.tsx:335-336 / :450-455 (comment segments) + currentShowCommentsAtom (atoms.ts:368-369)**: If commentary is stored in `node.comment` it renders automatically as Markdown inline in the notation and in the Annotate tab editor (AnnotationEditor.tsx:37-41), toggleable via the existing show-comments control. A separate `commentary` field could be rendered by extending `CompleteMoveCell`/`GameNotation` segments with a new segment type (`Segment = RowItem | VariationItem | CommentItem`, :275-279).
  - risk: GameNotation memoises `RenderVariationTree` by nodePath/depth/first only (:259-263) and reads nodes via `getNodeAtPath(s.root, nodePath)`; new fields are picked up, but very long generated comments make the notation noisy — a collapsible/short form is needed.
- **src/utils/phase.ts:77-97 (`getGamePhases`), src/utils/chess.ts:622-654 (`getMaterialDiff`), :333-351 (`getOpening` → Rust `get_opening_from_fens`), src/utils/chessops.ts:93-134 (piece counts/captures), src/utils/lichess/api.tsx:416-424 (tablebase)**: Reuse these for a game-level narrative (opening name + phase boundaries + material swings + tablebase verdict in endings) without new engine calls; EvalChart.tsx:98-128 shows the mainline iteration idiom (`treeIteratorMainLine`, `skipWhile`, `takeWhile`).
  - risk: `getOpening` is async (Rust command) and `getGamePhases` needs `Chess[]` built with `positionFromFen` per node (allocation per ply) — memoise on `getTreeStructureHash(root)` (treeReducer.ts:183-194) or on mainline length rather than recomputing on every engine tick.

## Conventions
- Path alias `@/` for all intra-src imports (tsconfig.json paths, vite.config.ts:43-47); `import type { … }` / inline `type` specifiers used consistently (e.g. BestMoves.tsx:33-34).
- Formatting via oxfmt + oxlint (`pnpm format`, `pnpm lint:fix`; CI `pnpm lint:ci` = `tsgo --noEmit && oxfmt --check && oxlint --format=github && i18next-cli extract --ci`, package.json). `.editorconfig`: 2-space indent for *.tsx/*.jsx, 4-space for *.ts, 2 for json; LF; final newline. `.oxlintrc.json` has empty `rules` (default recommended set) and ignores generated.ts/routeTree.gen.ts.
- Tauri commands: `#[tauri::command] #[specta::specta] pub async fn name(args…, state: tauri::State<'_, AppState>, app: tauri::AppHandle) -> Result<T, Error>` (chess.rs:319-329); request structs `#[derive(Deserialize, Type)] #[serde(rename_all = "camelCase")]` (chess.rs:244-251, 455-457); events `#[derive(Serialize, Clone, Type, Event)]` and `.emit(&app)?` (chess.rs:169-178, :402-410); all registered in `collect_commands!`/`collect_events!` (main.rs:111-180). TS bindings are regenerated only in debug builds (main.rs:182-188) — never hand-edit src/bindings/generated.ts.
- Frontend wraps every command result with `unwrap(result)` (src/utils/unwrap.tsx:7-19: logs via @tauri-apps/plugin-log `error`, shows Mantine red notification with IconX, throws) or checks `analysis.status === "ok"` (ReportModal.tsx:97).
- State: app/global/per-tab UI state in jotai (`atomWithStorage` for persisted settings, `createZodStorage`/`createAsyncZodStorage` with zod schemas for validated persistence atoms.ts:58-62, :153-159; per-tab values via `atomFamily((_tab) => atom(default))` + `tabValue(family)` helper atoms.ts:310-329; `atomFamily` with explicit equality fn for object keys :547, :552, :600). Game tree state in a per-tab zustand store created by `createTreeStore(id)` and read with `useStore(store, selector)`; use `useShallow` for derived arrays (AnalysisPanel.tsx:59-62) and `useStoreWithEqualityFn` where needed (CompleteMoveCell.tsx:16); store actions mutate via immer `produce` and set `state.dirty = true` for user-visible edits (tree.ts:443-452, 478-487).
- Performance idioms: `memo()` default exports with custom comparators using `fast-deep-equal` (`equal`) for array props (BestMoves.tsx:399-408, GameNotation.tsx:259-263, ReportPanel.tsx:162-167); `useDeferredValue(useMemo(...))` for engine-driven values (BestMoves.tsx:153-158); `startTransition` around state updates from events (EvalListener.tsx:146, AnalysisPanel.tsx:176); `useThrottledEffect(cb, 50, deps)` (really a debounce) with `JSON.stringify` of array/object deps (EvalListener.tsx:195-243); `useDebouncedValue(fen, 50)` from @mantine/hooks for external fetches (DatabasePanel.tsx:117); `proxy-memoize` `memoize` only for zustand derived selectors (`getStats = memoize((store: TreeStoreState) => …)`, repertoire.ts:290, used as `useStore(store, getStats)` InfoPanel.tsx:42); `useOptimistic` for reorder (AnalysisPanel.tsx:69-72). React Compiler (babel-plugin-react-compiler) is on (vite.config.ts:19-21), so components must be render-pure.
- Data fetching: SWR (`useSWR` / `useSWRImmutable` from "swr/immutable") keyed by arrays/objects or `null` to disable (TablebaseInfo.tsx:16-19, LogsPanel.tsx:19-21, DatabasePanel.tsx:166-176); external HTTP via `fetch` from `@tauri-apps/plugin-http` with `apiHeaders()` User-Agent (lichess/api.tsx:5,20; http.ts:17-22); module-level `Map` caches (lichess/api.tsx:289, chessdb/api.ts:102). Branching on discriminated unions with ts-pattern `match(...).with(...).exhaustive()` (EvalListener.tsx:181-193, DatabasePanel.tsx:73-109, TablebaseInfo.tsx:114-127).
- Chess logic: chessops on the frontend (`positionFromFen` [Chess, null] | [null, error] tuple src/utils/chessops.ts:21-34, `parseUci`/`parseSan`/`makeSan`/`makeFen`), shakmaty in Rust; UCI move lists are normalised for Chess960 (`uciNormalize` chess.ts:143-146, `normalize_uci_moves_for_fen` uci.rs:30-49). Side-to-move parity convention: `node.halfMoves % 2 === 1` ⇒ the move leading to this node was played by White (tree.ts:721, chess.ts:592).
- i18n: `const { t } = useTranslation(); t("Namespace.Key")` with flat dotted keys (keySeparator:false); keys live in src/translation/<locale>.json under `{"translation": {...}}`, sorted; `pnpm i18n:extract` adds/removes keys across all 16 locales (removeUnusedKeys:true) and CI fails on drift; some strings are still hard-coded English (e.g. BestMoves.tsx:187 "Check the opponent's threat", :255 "Invalid position", :264 "Game is over").
- UI: Mantine v8 components (`Paper withBorder`, `Stack`, `Group`, `Tabs`, `Accordion`, `ScrollArea offsetScrollbars`, `Tooltip`, `ActionIcon`), Tabler icons `IconX size="1rem"`, CSS modules `X.module.css` imported as `classes`; keyboard shortcuts via `useHotkeys` from @mantine/hooks with `keyMap.X.keys` (BoardAnalysis.tsx:133-170) — note __root.tsx/InfoPanel also use `react-hotkeys-hook`'s `useHotkeys`.
- Testing: vitest (`pnpm test`, jsdom env) with plain `test`/`expect` in src/utils/tests/*.test.ts (score.test.ts, chess.test.ts, chessops.test.ts, misc.test.ts, store.test.ts); pure util functions are what get tested — keep commentary generation logic in src/utils/ as pure functions.
- Rust style: `thiserror`-based `crate::error::Error`, `log::{info, warn}` logging, `tokio` async, `dashmap::DashMap` for shared registries, `governor` rate limiting, `derivative` for Default; engine logs captured in `BaseEngine.logs` (process.rs:83-89) and exposed via `get_engine_logs`.

## Gotchas
- Scores are always White-relative: Rust `parse_uci_attrs` inverts cp/mate and swaps wdl when Black is to move (chess.rs:237-239, invert_score :180-191). Use `normalizeScore(score.value, color)` (score.ts:38-47) to get mover-relative cp; mate is clamped to ±1000 cp (CP_CEILING) so cp-loss across mate boundaries is lossy.
- Live engine results are keyed by ROOT fen + UCI path from root (`${root.fen}:${getVariationLine(root, position).join(",")}`, EvalListener.tsx:149, BestMoves.tsx:155), not by the current node's FEN. The same position reached by a different move order is a different key, and navigating into a variation triggers a fresh `position fen … moves …` search. Threat-mode results are keyed `${swapMove(finalFen)}:` (empty move list).
- `node.score` is written on EVERY accepted bestMovesPayload (≤5/s per engine) by `setScore` (EvalListener.tsx:158-162 → tree.ts:478-487), which marks `dirty = true` and replaces `root` structurally. Anything subscribed to `s => s.root` (ReportPanel.tsx:25, EvalChart.tsx:50, AnnotationEditor.tsx:18, Board.tsx:104, OpeningName.tsx:10) re-renders at engine tick rate while an engine runs, and autosave fires for file-backed tabs (BoardAnalysis.tsx:82-86). Only the FIRST loaded engine with lines (`firstEngineWithLinesFamily`) writes `node.score`.
- `TreeNode.depth: number | null` exists (treeReducer.ts:22) but is never assigned anywhere (createNode/defaultTree set null, no store action writes it) — there is no record of which depth produced `node.score`.
- The raw `MoveAnalysis[]` from `analyzeGame` is consumed by `addAnalysis` and discarded (ReportModal.tsx:96-103); afterwards only `node.score`, `node.annotations` and optional inserted variation nodes (≤10 plies of PV, tree.ts:770-789) remain. `analyze_game` forces MultiPV=2 (chess.rs:545-557), so only best + second-best are ever available per position from the report.
- `MoveAnalysis` index mapping: entry 0 = start position, entry i = position after mainline ply i; positions where `chess.is_game_over()` are skipped when building `fens` (chess.rs:512-520), and `addAnalysis` walks `cur.children[0]` only (tree.ts:806). `reversed` analysis is un-reversed before returning (:601-604).
- `useThrottledEffect` (misc.ts:72-83) is a trailing debounce (setTimeout cleared on every dep change), not a throttle; deps include `JSON.stringify(...)` strings. `get_best_moves` returns `Some(cached)` only when options AND go_mode are identical and the engine is still running; otherwise it stops, sleeps 50 ms, restarts and returns `None` (chess.rs:334-355) — the first `info` lines after restart may arrive before the frontend effect re-subscribes, which is why the effect also seeds the Map from the command result (EvalListener.tsx:208-222).
- `events.bestMovesPayload` is a single global Tauri event for all tabs/engines; every listener must filter on `payload.tab === activeTab && payload.engine === engine.id && fen && moves` (EvalListener.tsx:138-145). Engine processes are keyed `(tab, engineId)` and killed on tab close via `commands.killEngines(tab)` (BoardsPage.tsx:69).
- `atomFamily` instances are never removed: `engineMovesFamily`/`engineProgressFamily` per (tab, engine) and `bestMovesFamily`/`firstEngineWithLinesFamily` per (fen, gameMoves) grow for the session; the Map inside `engineMovesFamily` also retains every analysed key except threat/non-threat counterpart deletions (EvalListener.tsx:150-154). A commentary cache should follow the same pattern but bound its size.
- Mantine `Tabs keepMounted={false}` at both levels (BoardAnalysis.tsx:196, AnalysisPanel.tsx:99) — panels unmount on switch; `EvalListener` is deliberately mounted outside the Tabs (BoardAnalysis.tsx:174). Any commentary generation that must continue while the user is on another tab must be hoisted similarly or run in atoms/store.
- `src/utils/chess.ts:21-34` declares its own `BestMoves` (no `nodes`) and `MoveAnalysis` interfaces that shadow the binding names; panel code imports `BestMoves`/`Score` from `@/bindings` (which widens `ScoreValue` with `dtz` for chessdb). Always import from `@/bindings` and handle `type: "dtz"` in `match(score.type).exhaustive()` (formatScore score.ts:17-32 does).
- WDL from engines is in permille (0–1000) — `ScoreBubble` divides by 10 (ScoreBubble.tsx:17-28) and `EvalBar` too; `wdl` is `null` unless `UCI_ShowWDL` is enabled (en-US.json:40 "Board.Analysis.EnableWDL").
- Generated bindings (`src/bindings/generated.ts`) are only rewritten when the Rust side is compiled in debug (`#[cfg(debug_assertions)]`, main.rs:182-188); adding a Rust command/type without running `pnpm dev`/`cargo build` leaves the TS side stale, and `lint:ci` type-checks against the committed file.
- `parsePGN` converts `[%eval …]` comments into `node.score` (chess.ts:370-388) and `getMoveText` re-emits `[%eval]`/`[%clk]`/`[%csl]`/`[%cal]` plus `node.comment` inside one `{…}` block (chess.ts:81-125); comments are Markdown (tiptap `getMarkdown()`, AnnotationEditor.tsx:40) rendered with `react-markdown` + `rehype-raw` (Comment.tsx:24-32) — raw HTML in generated text would be rendered.
- `analysis-panel-scroll` custom DOM event (AnalysisPanel.tsx:119-121, AnalysisRow.tsx:63-68) forces re-render of rows to reposition hover popovers; new rows inside the same ScrollArea that show popovers must subscribe to it.
- `immer` `setAutoFreeze(false)` (index.tsx:59) — store state objects are mutable after produce; do not mutate them outside store actions.
- `tabValue()` atoms throw `Error("No tab selected")` when `currentTabAtom` is undefined (atoms.ts:316, :322) — reading `currentThreatAtom` etc. outside a board tab context crashes.
- Only 16 locale JSONs exist and `removeUnusedKeys: true` — a key referenced only via a dynamic template string (e.g. `t(`Annotate.${translationKey}`)`) survives solely because of `preservePatterns` (i18next.config.ts:34-41); new dynamic key families need a preservePattern entry.
- Keybind defaults use `meta` = cmd on macOS else ctrl (keybinds.ts:5) and several single-letter keys (a, b, d, i, p, f, c, x, z, 1–6) are already taken; also ctrl+1..9 / alt+1..9 are used for tab selection (BoardsPage.tsx:163-180).

## Open questions
- Tauri HTTP capability scope: src-tauri/capabilities/*.json and tauri.conf.json were not read; it is unknown which remote hosts `@tauri-apps/plugin-http` `fetch` is allowed to reach (lichess.org, chessdb.cn, encroissant.org work today). Adding api.anthropic.com (or any LLM endpoint) likely requires a capability/scope entry, or a Rust-side reqwest command.
- `src/utils/tests/store.test.ts` was not read — it may construct TreeNode literals or snapshot store state, so adding a TreeNode field could require test updates.
- `src/i18next.d.ts` / `src/types/resources.d.ts` (declared in i18next.config.ts:44-48) are not present in the checkout, so it is unclear whether `t()` keys are type-checked; assume not.
- No existing precedent for a secret/text-input setting in SettingsPage (only switches/selects/sliders/directories) and no `PasswordInput` usage; where the maintainers want API keys stored (localStorage atom vs. a file in AppData via `fileStorage` in src/state/utils.ts:13-27 vs. a Rust keyring) is undetermined.
- Whether a new top-level board tab (BoardAnalysis.tsx:213-231) or an Analysis sub-tab (AnalysisPanel.tsx:101-107) is preferred upstream is a product/maintainer decision; no upstream issue or roadmap was checked.
- Board.tsx lines 135-280 (arrow/shape generation from `bestMovesFamily`, brushes, MEDIUM_BRUSH) were not read in full; if commentary wants to draw 'the idea' arrows on the board, the exact shapes pipeline and brush names need confirming (brushes seen: arrowColors strong/pale names BestMoves.tsx:51-56, "variation" Board.tsx:302).
- Whether live-analysis lines for the PREVIOUS node are reliably present in `engineMovesFamily` when the user steps forward (they are only if that path was searched in this tab and not evicted) — may need an explicit short 'evaluate parent' request or rely on `parent.score`.
- The vampirc-uci fork (Cargo.toml) exposes `Score {value, wdl, lower_bound, upper_bound}`; only `value`/`wdl` appear in the TS `Score` type — confirm the specta export if commentary wants bound flags.
- `analyze_game` is a fresh process per report and blocks until done; no per-move streaming of `MoveAnalysis` exists (only `progress-event`), so incremental commentary during the report would need a new event (e.g. `MoveAnalysisPayload`) emitted inside the loop at chess.rs:598.
- BoardGame.tsx (play mode) renders its own `#topRight` content (BoardGame.tsx:608-640) without the analysis Tabs; whether commentary should also appear in play/puzzle tabs was not scoped.
