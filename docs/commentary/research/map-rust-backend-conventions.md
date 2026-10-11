# Rust backend conventions (src-tauri): command/event/error/progress/state structure, tauri-specta bindings generation, long-running work and cancellation, shakmaty usage, opening identification, HTTP conventions, DB persistence of derived data, Rust test conventions. Upstream snapshot: commit 23f8314 (2026-09-07), v0.15.1, repo root = /tmp/claude-0/-home-user-Chess/095b4a5e-49ff-5938-b919-e82f3408c9dc/scratchpad/upstream/en-croissant

## Summary
The backend is a single Rust binary (src-tauri/src/main.rs) with flat modules declared at main.rs:6-19 (chess, db, engine, error, game, fs, lexer, oauth, opening, pgn, progress, puzzle, sound). Every IPC entry point is a free function annotated `#[tauri::command] #[specta::specta]` returning `Result<T, crate::error::Error>` (the single exception is `close_splashscreen` main.rs:98-107 which returns `Result<(), String>`). Commands are registered in one `tauri_specta::Builder::new().commands(tauri_specta::collect_commands!(...))` list at main.rs:110-172 and typed events in `.events(tauri_specta::collect_events!(...))` at main.rs:173-180. The builder's `invoke_handler()` is passed to `tauri::Builder` (main.rs:210) and `mount_events(app)` is called in setup (main.rs:222). Shared mutable state is one `AppState` struct (main.rs:76-96; `#[derive(Derivative)] #[derivative(Default)]`) made of `DashMap`s, `Mutex<Option<..>>`, an `Arc<Semaphore>` and sub-managers (`GameManager`, `ProgressStore`, `AuthState`), registered via `.manage(AppState::default())` (main.rs:247) and injected into commands as `state: tauri::State<'_, AppState>`; the `tauri::AppHandle` is injected as `app` for emitting events. A second managed type exists (`sound::SoundServerPort`, main.rs:231/234, read in sound.rs:140-143) showing that separate `app.manage(T)` types are acceptable too.

TypeScript bindings are NOT produced by build.rs (build.rs:1-3 is only `tauri_build::build()`). They are written at runtime when a debug build starts: main.rs:182-188 `#[cfg(debug_assertions)] specta_builder.export(Typescript::default().bigint(BigIntExportBehavior::BigInt), "../src/bindings/generated.ts")`. The file src/bindings/generated.ts is checked into git (git ls-files shows it; src/bindings/index.ts:9 re-exports it and overrides `Score`/`BestMoves`/`DatabaseInfo`). So the recipe is: edit Rust, run `pnpm dev` (tauri dev → debug cargo run) once, commit the regenerated generated.ts. CI (.github/workflows/test.yml) only runs `pnpm lint:ci` (tsgo --noEmit, oxfmt --check, oxlint, i18next-cli extract --ci) and `pnpm test` (vitest); it never runs cargo build or cargo test, so a stale generated.ts is only caught indirectly if TS references a missing command.

Errors: `error::Error` (error.rs:4-95) is a thiserror enum with boxed `#[error(transparent)]` wrappers (Io, Zip, ParseInt, Tauri, TauriOpener, Reqwest, ChessPosition, IllegalUciMove, ParseUciMove, Fen, ParseSan, IllegalSan, Diesel, R2d2, SystemTime) plus unit/message variants (NoStdin … AnalysisCancelled). It implements `serde::Serialize` as its Display string (error.rs:187-194) and `specta::Type` as a primitive string (error.rs:196-203). Hence every `Result<T, Error>` command becomes `Promise<Result<T, string>>` in TS where `Result` is tauri-specta's own discriminated union `{ status: "ok"; data: T } | { status: "error"; error: E }` (generated.ts:650-652) produced by a try/catch wrapper around `TAURI_INVOKE` (generated.ts:8-15). `@badrap/result` is NOT involved in bindings (it is only used by frontend-only code, src/utils/files.ts:1). Frontend consumers either branch on `.status` (ReportModal.tsx:97) or call `unwrap()` from src/utils/unwrap.tsx:7-19 which logs via plugin-log, shows a red Mantine notification with the error string, and throws.

Long-running work runs inline inside `async fn` commands on Tauri's tokio runtime: blocking Diesel queries and rayon `par_iter` CPU work are executed directly in the command (db/mod.rs:1424, db/search.rs:422/544); there is no `spawn_blocking` anywhere. Engine analysis (`analyze_game`, chess.rs:473-688) loops over plies sequentially, reporting progress through `progress::update_progress` (progress.rs:24-47: stores a `ProgressItem` in `AppState.progress_state` and emits the typed `ProgressEvent`), and is cancelled cooperatively via `AppState.analysis_cancel_flags: DashMap<String, Arc<AtomicBool>>` (main.rs:92) set by `cancel_analysis` (chess.rs:464-470) and polled per ply (chess.rs:513-517), returning `Err(Error::AnalysisCancelled)`. Detached background work uses `tokio::spawn` (game.rs:643 game loop, game.rs:1182 engine-move task, engine/process.rs:55 stderr drain) or `tauri::async_runtime::spawn` (oauth.rs:76, sound.rs:122), with shutdown via `tokio::sync::watch` + `JoinHandle::abort` (game.rs:634-635, 1218-1226, 1316). External HTTP to chess sites is done in the FRONTEND with `@tauri-apps/plugin-http` `fetch` under a URL allowlist in src-tauri/capabilities/main.json; Rust `reqwest` (0.12.5, features stream/blocking/json) is used only for file downloads with streaming progress (fs.rs:21-86) and by oauth2. Games are stored in SQLite (`Games.Moves` BLOB with an in-house encoding that already carries PGN comments and NAGs, db/encoding.rs:7-36) or as PGN files edited in place (pgn.rs); derived data precedent is the `<db>.ecsi` rkyv/memmap search-index sidecar (search_index.rs:316-318).

## Key files
- `src-tauri/src/main.rs` — Binary entry: module list, AppState, specta builder (command + event registration), TS export, plugin setup, exit cleanup
  - mod declarations main.rs:6-19
  - pub struct AppState main.rs:76-96 (connection_pool, line_cache, db_cache, new_request: Arc<Semaphore::new(2)>, search_collisions, pgn_offsets, engine_processes: DashMap<(String,String), Arc<tokio::sync::Mutex<EngineProcess>>>, analysis_cancel_flags: DashMap<String, Arc<AtomicBool>>, auth: AuthState, game_manager: GameManager, progress_state: ProgressStore)
  - close_splashscreen main.rs:98-107 (only command returning Result<(), String>)
  - fn main main.rs:109-260; collect_commands! main.rs:111-172; collect_events! main.rs:173-180
  - #[cfg(debug_assertions)] specta_builder.export(Typescript::default().bigint(BigIntExportBehavior::BigInt), "../src/bindings/generated.ts") main.rs:182-188
  - .invoke_handler(specta_builder.invoke_handler()) main.rs:210; specta_builder.mount_events(app) main.rs:222; .manage(AppState::default()) main.rs:247
  - tauri_plugin_log level Info main.rs:204-209; tauri_plugin_http::init main.rs:212
  - ExitRequested → kill engines main.rs:250-259
  - is_bmi2_compatible main.rs:262-270, memory_size main.rs:272-277 (sync, non-Result commands)
- `src-tauri/build.rs` — Only tauri_build::build(); does NOT generate bindings
  - fn main build.rs:1-3
- `src-tauri/src/error.rs` — Single crate Error enum; Serialize-as-string; specta Type = string
  - pub enum Error error.rs:4-95
  - transparent boxed variants error.rs:6-49 (Io, Zip, ParseInt, Tauri, TauriOpener, Reqwest, ChessPosition, IllegalUciMove, ParseUciMove, Fen, ParseSan, IllegalSan, Diesel, R2d2, SystemTime)
  - message variants error.rs:51-94 (NoStdin, NoStdout, NoMovesFound, MissingReferenceDatabase, NoOpeningFound, NoPuzzles, NotDistinctPlayers, GameNotFound(String), GameNotInProgress, NotHumanTurn, NotEngineTurn, InvalidColor(String), EngineNotInitialized, EngineDisconnected, AnalysisCancelled)
  - manual From impls error.rs:97-185
  - impl serde::Serialize for Error error.rs:187-194 (serialize_str(self.to_string()))
  - impl specta::Type for Error error.rs:196-203 (DataType::Primitive(PrimitiveType::String))
- `src-tauri/src/progress.rs` — Generic id-keyed progress store + typed ProgressEvent; reused by downloads and analysis
  - pub struct ProgressItem progress.rs:8-13
  - pub struct ProgressEvent (derive Clone, Debug, Serialize, Type, Event) progress.rs:15-20
  - pub type ProgressStore = DashMap<String, ProgressItem> progress.rs:22
  - pub fn update_progress(store, app: &tauri::AppHandle, id: String, progress: f32, finished: bool) -> Result<(), Error> progress.rs:24-47
  - get_progress progress.rs:49-53 (sync, returns Option<ProgressItem>)
  - clear_progress progress.rs:55-59
- `src-tauri/src/chess.rs` — Engine driving: live best-moves streaming, whole-game analysis (the Generate Report backend), naive material eval used for is_sacrifice
  - pub struct EngineProcess chess.rs:39-50; EngineProcess::new/set_options/go/stop/kill chess.rs:52-153
  - pub struct BestMoves chess.rs:157-171
  - pub struct BestMovesPayload (Event) chess.rs:173-182
  - fn invert_score chess.rs:184-195; fn parse_uci_attrs chess.rs:197-246 (score normalised to White POV at 224-226)
  - pub struct EngineOptions chess.rs:248-255
  - kill_engines/kill_engine/stop_engine/get_engine_logs chess.rs:257-321
  - get_best_moves chess.rs:323-446 (governor RateLimiter 5/s at 363)
  - pub struct MoveAnalysis chess.rs:448-453; pub struct AnalysisOptions chess.rs:455-463
  - cancel_analysis chess.rs:464-471
  - analyze_game chess.rs:473-688 (fens vector 489-510; cancel poll 513-517; update_progress 519-525; MultiPV forced to 2 at 527-539; novelty via is_position_in_db 655-676)
  - fn count_material chess.rs:616-633; fn piece_value chess.rs:635-644; fn qsearch chess.rs:646-679; fn naive_eval chess.rs:681-692 (all private)
  - #[cfg(test)] mod tests chess.rs:710-779 (9 FEN-based eval tests)
  - pub struct EngineConfig chess.rs:781-785; get_engine_config chess.rs:787-809
- `src-tauri/src/engine/process.rs` — UCI child-process wrapper (tokio::process), logs, stderr drain task
  - pub enum EngineLog process.rs:19-24 (#[serde(tag="type", content="value", rename_all="camelCase")])
  - pub type EngineReader = Lines<BufReader<ChildStdout>> process.rs:26
  - pub struct BaseEngine process.rs:28-34; spawn process.rs:37-69 (tokio::spawn stderr drain at 55-61); init_uci 91-97; send 99-104; wait_for 106-120; set_option 122-128; set_position 130-138; go 140-143; stop/quit 145-151; wait_for_bestmove 153-162; kill_sync 164-166
  - impl Drop (start_kill) process.rs:169-173
- `src-tauri/src/engine/types.rs` — Shared engine option/go-mode types exported to TS
  - pub struct EngineOption types.rs:4-8
  - pub enum GoMode (#[serde(tag="t", content="c")]) types.rs:10-18; to_uci_string 20-35
  - pub struct PlayersTime types.rs:37-43
- `src-tauri/src/engine/uci.rs` — FEN/UCI helpers reused everywhere
  - pub fn parse_fen_to_position(fen:&str)->Result<Chess,Error> uci.rs:5-13 (CastlingMode::detect + ignore_too_much_material)
  - pub fn apply_uci_moves uci.rs:15-22
  - pub fn parse_fen_and_apply_moves uci.rs:24-28
  - pub fn normalize_uci_moves_for_fen uci.rs:30-49
- `src-tauri/src/game.rs` — Newest subsystem (play vs engine): best example of current conventions — camelCase serde, tagged enums, typed events, tokio::spawn loop, watch shutdown, DashMap<Arc<RwLock<..>>> state
  - pub type GameId = String game.rs:32
  - pub enum PlayerConfig game.rs:34-47; pub struct TimeControl 49-54; pub struct GameConfig 56-67; pub struct OpeningBookConfig 69-75
  - pub enum GameStatus 81-86; pub enum GameResult 88-94; GameEndReason 96-103; DrawReason 105-113
  - pub struct GameMove 115-124; pub struct GameState 126-140
  - Events: GameMoveEvent 142-150, ClockUpdateEvent 152-158, GameOverEvent 160-166
  - struct GameController 176-192; apply_move 306-363; position_key 294-297 (4-field FEN key); check_game_end 448-490 (is_checkmate/is_stalemate/is_insufficient_material/halfmoves>=100/repetition map)
  - pub struct GameManager { games: DashMap<GameId, Arc<RwLock<GameController>>> } 554-556; start_game 565-651 (tokio::spawn(game_loop) at 643)
  - fn spawn_engine_task 1174-1185; async fn game_loop 1204-1332 (select!, abort at 1226/1316)
  - fn try_polyglot_book_move 1334-1366 (polyglot_book_rs)
  - commands start_game/get_game_state/make_game_move/take_back_game_move/resign_game/abort_game/get_game_engine_logs 1506-1576
- `src-tauri/src/opening.rs` — Opening name lookup by exact Setup equality from embedded lichess TSVs; fuzzy name search
  - struct Opening { _eco, name, setup: Setup, pgn: Option<String> } opening.rs:11-17 (eco never returned)
  - pub struct OutOpening { name, fen } opening.rs:19-23
  - const TSV_DATA include_bytes a..e.tsv opening.rs:32-38; FISCHER_RANDOM_DATA frc.tsv opening.rs:40
  - get_opening_from_fen(fen:&str)->Result<String,Error> opening.rs:48-53
  - get_opening_from_name opening.rs:55-63 (returns pgn string)
  - get_opening_from_fens(Vec<String>) opening.rs:65-74 (iterates .rev(), first hit)
  - pub fn get_opening_from_setup(Setup)->Result<String,Error> opening.rs:76-82 (o.setup == setup)
  - search_opening_name opening.rs:84-115 (strsim sorensen_dice/jaro_winkler > 0.8, top 15)
  - lazy_static OPENINGS opening.rs:117-169 (replays SAN with San::parse + to_move + play_unchecked; into_setup(EnPassantMode::Legal))
  - #[cfg(test)] test_get_opening opening.rs:171-182
- `src-tauri/src/puzzle.rs` — Small module template: private cache behind once_cell Lazy<Mutex>, diesel queries, camelCase output struct
  - struct PuzzleCache puzzle.rs:13-87 (static PUZZLE_CACHE: Lazy<Mutex<PuzzleCache>> at 97)
  - get_puzzle puzzle.rs:89-102
  - pub struct PuzzleDatabaseInfo (#[serde(rename_all="camelCase")]) puzzle.rs:104-112
  - get_puzzle_db_info/delete_puzzle_database/get_puzzle_themes/get_themes_for_puzzle puzzle.rs:114-165
- `src-tauri/src/fs.rs` — Only Rust-side HTTP client usage: reqwest streaming download with progress; archive extraction
  - download_file(id, url, path, app, state, token: Option<String>, finalize: Option<bool>, total_size: Option<u32>) fs.rs:21-86 (Client::new, Bearer header 39-43, bytes_stream 53-63, update_progress 61/82)
  - unzip_file fs.rs:116-145; set_file_as_executable 147-159; file_exists 161-165; FileMetadata/get_file_metadata 167-182
- `src-tauri/src/oauth.rs` — Lichess OAuth via local axum server; raw app.emit("access_token")
  - pub struct AuthState oauth.rs:34-40; impl Default 42-57
  - authenticate oauth.rs:59-78 (tauri::async_runtime::spawn(run_server))
  - authorize oauth.rs:86-109 (app.emit("access_token", ..) at 106)
- `src-tauri/src/lexer.rs` — pgn_reader Visitor → Token stream for the frontend PGN importer (tagged enum example)
  - pub enum Token (#[serde(tag="type", content="value")]) lexer.rs:11-21
  - impl Visitor for Lexer lexer.rs:23-64 (san/header/nag/begin_variation/end_variation/comment/outcome)
  - lex_pgn lexer.rs:66-76
- `src-tauri/src/pgn.rs` — PGN file storage: count/read/delete/write games in place with byte offsets cached in AppState.pgn_offsets
  - struct PgnParser pgn.rs:11-142
  - count_pgn_games pgn.rs:154-183; read_games 185-209; delete_game 211-234; write_game 243-279
- `src-tauri/src/db/mod.rs` — SQLite game databases: r2d2 pool per path, PGN import (Visitor), queries, stats with rayon, export, write-back; in-memory SQLite tests
  - const DATABASE_VERSION = "1.0.0" db/mod.rs:64; CREATE_TABLES_SQL include_str!("create.sql") 70
  - ConnectionOptions/JournalMode + CustomizeConnection db/mod.rs:105-146
  - fn get_db_or_create(state, db_path, options) -> PooledConnection db/mod.rs:148-171 (Pool max_size 16, cached in AppState.connection_pool)
  - fn set_info_value db/mod.rs:181-193 (Info KV upsert)
  - pub struct TempGame db/mod.rs:210-228; insert_to_db 230-287
  - struct Importer (pgn_reader::Visitor) db/mod.rs:289-497 (san 400-431 encode_move; begin/end_variation 433-466; comment 468-471 encode_comment; nag 473-475)
  - convert_pgn db/mod.rs:500-614 (raw app.emit("convert_progress", (usize,u32,Option<String>)) at 568-577)
  - pub fn generate_search_index db/mod.rs:617-698
  - pub struct DatabaseInfo 700-710; get_db_info 723-770
  - pub struct GameQuery 870-895; QueryResponse 908-911; get_games 913-1113; fn normalize_games 1115-1162
  - pub struct DatabaseProgress (Event) db/mod.rs:1372-1376
  - get_players_game_info db/mod.rs:1378-1529 (rayon par_iter 1424; DatabaseProgress emit 1482-1487; opening via get_opening_from_setup 1461-1470)
  - export_to_pgn 1711-1768 (decode_game_to_movetext)
  - write_db_game(file, game_id, pgn) 1788-1860 (re-imports PGN incl. comments and UPDATEs the row)
  - clear_games 1899-1903; preload_reference_db 1906-1934
  - #[cfg(test)] mod tests db/mod.rs:1936-2205 (setup_test_db :memory: + CREATE_TABLES_SQL at 2007-2012)
- `src-tauri/src/db/models.rs` — Diesel models + NormalizedGame exported to TS
  - pub struct Puzzle models.rs:9-19
  - pub struct Player 21-27; NewPlayer 29-34
  - pub struct Game models.rs:41-65 (moves: Vec<u8>)
  - pub struct NewGame<'a> 67-88
  - pub struct Info { name, value: Option<String> } 114-118
  - pub enum Outcome (serde renames "1-0" etc.) 120-131
  - pub struct NormalizedGame models.rs:147-177 (#[specta(optional)] on Option fields)
- `src-tauri/src/db/schema.rs` — diesel::table! definitions with #[sql_name] mapping to PascalCase SQL names; includes an unused `comments` table
  - puzzles/themes/puzzle_themes schema.rs:1-29
  - players schema.rs:31-41; games schema.rs:43-85 (Moves -> Binary)
  - comments (ID, GameID, Ply, Comment) schema.rs:87-99 — NOT in create.sql, no usages
  - events 101-109; info 111-119; sites 121-129; joinable!/allow_tables 131-134
- `src-tauri/src/db/create.sql` — Schema for a new game DB (created once in convert_pgn when file does not exist)
  - Info(Name UNIQUE, Value) create.sql:1-4
  - Games(... ECO TEXT, PlyCount, FEN, Moves BLOB, PawnHome) create.sql:22-46
  - no Comments table; no migration table
- `src-tauri/src/db/encoding.rs` — Moves BLOB encoding: 1 byte per move = index into legal_moves(); escape markers for variations/comments/NAGs; decoder to movetext
  - VARIATION_START_MARKER=255, VARIATION_END_MARKER=254, COMMENT_MARKER=253, NAG_MARKER=252 encoding.rs:7-10
  - pub fn encode_move(m:&Move, chess:&Chess)->Result<u8,Error> encoding.rs:12-15
  - pub fn decode_move(byte:u8, chess:&Chess)->Option<Move> encoding.rs:17-20
  - pub fn encode_comment(comment:&str, output:&mut Vec<u8>) encoding.rs:22-28 (u16 LE length-prefixed chunks)
  - pub fn iter_mainline_move_bytes encoding.rs:98-100
  - pub enum DecodedGameNode 103-109; pub fn decode_game 125; pub fn decode_game_to_movetext 341
  - #[cfg(test)] encoding.rs:348 (7 tests)
- `src-tauri/src/db/search.rs` — Position search over mmap index: semaphore + per-query collision lock + cache; raw search_progress emit
  - pub struct PositionQueryJs { fen, type_ } search.rs:85-88
  - fn is_contained(container: Bitboard, subset: Bitboard) search.rs:152-154
  - pub struct PositionStats search.rs:156-163
  - pub struct ProgressPayload (Clone, Serialize only — no Type) search.rs:228-233
  - search_position search.rs:235-458 (collision lock 245-253; line_cache 256-258; Semaphore permit 263; lazy index generation 267-277; app.emit("search_progress") 336-343; par_iter 422)
  - pub async fn is_position_in_db(file: PathBuf, query: GameQuery, state: tauri::State<'_, AppState>) -> Result<bool, Error> search.rs:464-556
  - #[cfg(test)] search.rs:561 (9 tests)
- `src-tauri/src/db/search_index.rs` — Precedent for persisted derived data next to a DB: rkyv-serialized, memmapped sidecar file with magic+version header
  - const MAGIC = b"ECSI", VERSION = 4 search_index.rs:12-13
  - pub struct SearchGameEntry search_index.rs:76-91
  - pub struct SearchIndex { entries } 93-95; write_to 126
  - pub struct MmapSearchIndex 215; open 224-240; par_iter 263-268
  - pub fn get_index_path(db_path:&Path)->PathBuf = with_extension("ecsi") search_index.rs:316-318
  - #[cfg(test)] search_index.rs:320 (4 tests)
- `src-tauri/Cargo.toml` — Dependency inventory (no [dev-dependencies], no workspace, edition 2021)
  - tauri 2.10.2; tokio 1.33 full (lock 1.36.0); reqwest 0.12.5 features [stream, blocking, json] (Cargo.toml:21); futures-util 0.3; shakmaty 0.27.1; pgn-reader 0.26.0; rayon 1.6; dashmap 6; diesel 2 sqlite+r2d2; specta ^2.0.0-rc.20 (lock rc.22); tauri-specta ^2.0.0-rc.20 (lock rc.21) features [derive, typescript]; specta-typescript 0.0.9; thiserror 2; log 0.4; derivative; once_cell; lazy_static; governor; vampirc-uci (fork with specta feature); polyglot-book-rs 0.1.0; rkyv 0.8; memmap2; axum 0.6; oauth2 4.4; tauri-plugin-http 2.5.7
  - no dev-dependencies section (grep confirmed)
- `src/bindings/generated.ts` — Checked-in tauri-specta output (687 lines): commands object, events proxy, types, Result type
  - export const commands generated.ts:7-463 (per-command try/catch wrapper 8-15; non-Result command memorySize 72-74)
  - export const events = __makeEvents__<{bestMovesPayload, clockUpdateEvent, databaseProgress, gameMoveEvent, gameOverEvent, progressEvent}>({... "best-moves-payload", "progress-event" ...}) generated.ts:468-483
  - types generated.ts:490-630 (e.g. MoveAnalysis 515 snake_case; GameConfig 503 with `initialMoves?: string[]`; ClockUpdateEvent 493 bigint)
  - export type Result<T,E> = {status:"ok";data:T}|{status:"error";error:E} generated.ts:650-652
  - function __makeEvents__ generated.ts:654-687 (listen/once/emit via @tauri-apps/api/event)
- `src/bindings/index.ts` — Public bindings facade: re-exports generated and widens Score/BestMoves/DatabaseInfo
  - export * from "./generated" index.ts:9
  - export type ScoreValue = ScoreValueT | { type: "dtz"; value: number } index.ts:10
  - export type DatabaseInfo = success|error union index.ts:16-29
- `src/hooks/useProgress.ts` — Frontend consumer of ProgressEvent/getProgress/clearProgress keyed by id
  - export function useProgress(id) useProgress.ts:4-42
- `src/components/panels/analysis/ReportModal.tsx` — The Generate Report frontend entry: calls commands.analyzeGame and feeds MoveAnalysis[] into the tree store
  - reportSettingsAtom ReportModal.tsx:13-19
  - function analyze() ReportModal.tsx:72-104 (id `report_${tab}`; analysis.status === "ok" → addAnalysis)
- `src/components/panels/analysis/ReportPanel.tsx` — Cancel wiring and progress button for the report
  - handleCancel → commands.cancelAnalysis(`report_${activeTab}`) ReportPanel.tsx:33-35
  - <ReportModal initialFen={root.fen} moves={getMainLine(root)}> ReportPanel.tsx:47-54
  - <ProgressButton id={`report_${activeTab}`} onCancel={handleCancel}> ReportPanel.tsx:56-70
- `src/state/store/tree.ts` — Where backend analysis becomes UI state (score, annotation, best-line variations)
  - addAnalysis signature tree.ts:74-81 ({best: BestMoves[]; novelty; is_sacrifice}[])
  - function addAnalysis tree.ts:693+ (sets cur.score, getAnnotation(...) with is_sacrifice, inserts up to 10-ply PV variation)
- `src-tauri/capabilities/main.json` — Tauri v2 permission allowlist incl. http:default URL scope for frontend fetch
  - http:default allow: encroissant.org, api.chess.com, www.chess.com, lichess.org, explorer.lichess.org, tablebase.lichess.org, www.chessdb.cn main.json:44-55
- `.github/workflows/test.yml` — CI: frontend lint + vitest only; no cargo test
  - pnpm lint:ci; pnpm test (test.yml:31-35)

## Data model
// error.rs:4-95 (abridged)
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)] Io(Box<std::io::Error>),
    #[error(transparent)] Reqwest(Box<reqwest::Error>),
    #[error(transparent)] ChessPosition(Box<shakmaty::PositionError<Chess>>),
    #[error(transparent)] IllegalUciMove(Box<shakmaty::uci::IllegalUciMoveError>),
    #[error(transparent)] ParseUciMove(Box<shakmaty::uci::ParseUciMoveError>),
    #[error(transparent)] Fen(Box<shakmaty::fen::ParseFenError>),
    #[error(transparent)] ParseSan(Box<shakmaty::san::ParseSanError>),
    #[error(transparent)] IllegalSan(Box<shakmaty::san::SanError>),
    #[error(transparent)] Diesel(Box<diesel::result::Error>),
    #[error(transparent)] Tauri(Box<tauri::Error>),
    /* Zip, ParseInt, TauriOpener, R2d2, SystemTime likewise */
    #[error("No moves found")] NoMovesFound,
    #[error("Missing reference database")] MissingReferenceDatabase,
    #[error("No opening found")] NoOpeningFound,
    #[error("Game not found: {0}")] GameNotFound(String),
    #[error("Engine disconnected")] EngineDisconnected,
    #[error("Analysis cancelled")] AnalysisCancelled,
    /* ... */
}
impl serde::Serialize for Error { /* serialize_str(self.to_string()) */ }          // error.rs:187-194
impl specta::Type for Error { /* DataType::Primitive(PrimitiveType::String) */ }   // error.rs:196-203

// progress.rs:8-22
#[derive(Clone, Debug, Serialize, Type)]        pub struct ProgressItem  { pub id: String, pub progress: f32, pub finished: bool }
#[derive(Clone, Debug, Serialize, Type, Event)] pub struct ProgressEvent { pub id: String, pub progress: f32, pub finished: bool }
pub type ProgressStore = DashMap<String, ProgressItem>;
pub fn update_progress(store: &ProgressStore, app: &tauri::AppHandle, id: String, progress: f32, finished: bool) -> Result<(), Error>

// main.rs:76-96
#[derive(Derivative)] #[derivative(Default)]
pub struct AppState {
    connection_pool: DashMap<String, diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::SqliteConnection>>>,
    line_cache: DashMap<(GameQuery, PathBuf), (Vec<PositionStats>, Vec<NormalizedGame>)>,
    db_cache: Mutex<Option<MmapSearchIndex>>,
    #[derivative(Default(value = "Arc::new(Semaphore::new(2))"))] new_request: Arc<Semaphore>,
    #[derivative(Default(value = "DashMap::new()"))] search_collisions: DashMap<(GameQuery, PathBuf), Arc<tokio::sync::Mutex<()>>>,
    pgn_offsets: DashMap<String, Vec<u64>>,
    engine_processes: DashMap<(String, String), Arc<tokio::sync::Mutex<EngineProcess>>>,
    analysis_cancel_flags: DashMap<String, Arc<AtomicBool>>,
    auth: AuthState,
    game_manager: GameManager,
    progress_state: ProgressStore,
}

// chess.rs:157-182, 448-463
#[derive(Clone, Serialize, Debug, Derivative, Type)] #[derivative(Default)]
pub struct BestMoves { nodes: u32, depth: u32, score: Score /* vampirc_uci::uci::Score{value: ScoreValue::Cp(i32)|Mate(i8), wdl: Option<(u16,u16,u16)>, lower_bound, upper_bound} */, #[serde(rename="uciMoves")] uci_moves: Vec<String>, #[serde(rename="sanMoves")] san_moves: Vec<String>, #[derivative(Default(value="1"))] multipv: u16, nps: u32 }
#[derive(Serialize, Debug, Clone, Type, Event)] #[serde(rename_all = "camelCase")]
pub struct BestMovesPayload { pub best_lines: Vec<BestMoves>, pub engine: String, pub tab: String, pub fen: String, pub moves: Vec<String>, pub progress: f64 }
#[derive(Serialize, Debug, Default, Type)]
pub struct MoveAnalysis { best: Vec<BestMoves>, novelty: bool, is_sacrifice: bool }   // TS: { best: BestMoves[]; novelty: boolean; is_sacrifice: boolean } (snake_case!)
#[derive(Deserialize, Debug, Default, Type)] #[serde(rename_all = "camelCase")]
pub struct AnalysisOptions { pub fen: String, pub moves: Vec<String> /* UCI mainline */, pub annotate_novelties: bool, pub reference_db: Option<PathBuf>, pub reversed: bool }
// analyze_game(id: String, engine: String /*path*/, go_mode: GoMode, options: AnalysisOptions, uci_options: Vec<EngineOption>, state, app) -> Result<Vec<MoveAnalysis>, Error>
//   analysis[i] corresponds to fens[i] where fens[0] = start position and fens[i] = position after moves[..i] (game-over positions are skipped, chess.rs:489-510). Score in BestMoves is White-POV (chess.rs:224-226).

// engine/types.rs:4-18
#[derive(Deserialize, Serialize, Debug, Clone, Type, PartialEq, Eq)] pub struct EngineOption { pub name: String, pub value: String }
#[derive(Deserialize, Serialize, Debug, Clone, Type, PartialEq, Eq)] #[serde(tag = "t", content = "c")]
pub enum GoMode { PlayersTime(PlayersTime), Depth(u32), Time(u32), Nodes(u32), Infinite }   // TS: { t: "Depth"; c: number } | ...

// opening.rs:11-23
struct Opening { _eco: String, name: String, setup: Setup, pgn: Option<String> }
#[derive(Debug, Clone, Type, Serialize)] pub struct OutOpening { name: String, fen: String }
// get_opening_from_fen(fen: &str) -> Result<String, Error>; get_opening_from_fens(fens: Vec<String>) -> Result<String, Error>; pub fn get_opening_from_setup(setup: Setup) -> Result<String, Error>

// game.rs:142-166 (event template)
#[derive(Clone, Debug, Serialize, Type, Event)] #[serde(rename_all = "camelCase")]
pub struct GameMoveEvent { pub game_id: GameId, pub moves: Vec<GameMove>, pub fen: String, pub white_time: Option<u64>, pub black_time: Option<u64> }  // u64 → bigint in TS

// db/models.rs:41-65 (SQLite Games row)
pub struct Game { pub id: i32, pub event_id: i32, pub site_id: i32, pub date: Option<String>, pub time: Option<String>, pub round: Option<String>, pub white_id: i32, pub white_elo: Option<i32>, pub black_id: i32, pub black_elo: Option<i32>, pub white_material: i32, pub black_material: i32, pub result: Option<String>, pub time_control: Option<String>, pub eco: Option<String>, pub ply_count: Option<i32>, pub fen: Option<String>, pub moves: Vec<u8>, pub pawn_home: i32 }
// db/models.rs:114-118  pub struct Info { pub name: String, pub value: Option<String> }   (KV table; set_info_value upsert db/mod.rs:181-193)
// db/schema.rs:87-99    comments (id, game_id, ply: Nullable<Integer>, comment: Nullable<Text>)  -- declared, never created or used
// db/encoding.rs:7-28   Moves BLOB: byte = index into chess.legal_moves(); 255/254 variation start/end; 253 + u16 LE len + utf8 = comment; 252 + u16 LE len + utf8 = NAG

// generated.ts:650-652
export type Result<T, E> = { status: "ok"; data: T } | { status: "error"; error: E };
// generated.ts:8-15 wrapper:  try { return { status: "ok", data: await TAURI_INVOKE("cmd", { a, b }) } } catch (e) { if (e instanceof Error) throw e; else return { status: "error", error: e as any } }

// Frontend tree node that holds per-move text today (src/utils/treeReducer.ts:16-28)
export interface TreeNode { fen: string; move: Move | null; san: string | null; children: TreeNode[]; score: Score | null; depth: number | null; halfMoves: number; shapes: DrawShape[]; annotations: Annotation[]; comment: string; clock?: number; }

## Extension points
- **src-tauri/src/main.rs:6-19 (mod list), main.rs:39-72 (use block), main.rs:111-172 collect_commands!, main.rs:173-180 collect_events!**: Add `mod commentary;` (or `mod facts;`), import the new `#[tauri::command] #[specta::specta]` fns and `#[derive(Event)]` structs, append them to collect_commands!/collect_events!. Then run `pnpm dev` once in debug so main.rs:182-188 rewrites src/bindings/generated.ts; commit that file. Order in collect_commands! = order in generated.ts.
  - risk: Forgetting to commit generated.ts breaks `tsgo --noEmit` in CI for any TS that references the new command; no CI step regenerates or diffs bindings. Non-camelCase struct fields leak snake_case into TS (see MoveAnalysis).
- **src-tauri/src/chess.rs:473-688 analyze_game and chess.rs:448-453 MoveAnalysis**: Either (a) extend `MoveAnalysis` with new fields (e.g. `facts`, `classification`) computed per ply inside the existing loop — the loop already has `fens[i]` (Fen), the UCI move list, `current_analysis.best` (MultiPV=2 lines with White-POV scores), `is_sacrifice`; or (b) keep analyze_game untouched and add a new command `explain_game(fen, moves, analysis: Vec<MoveAnalysis>) -> Vec<MoveCommentary>` that re-derives positions with `engine::parse_fen_and_apply_moves` (engine/uci.rs:24) and consumes the engine output. Option (b) keeps the engine loop pure and lets the LLM/offline step run after the report.
  - risk: (a) changes the TS shape consumed by addAnalysis (src/state/store/tree.ts:74-81, 693+) and the frontend `getAnnotation` heuristic that currently owns move classification; (b) needs MoveAnalysis to become Deserialize + Type-for-input (it is Serialize-only today). `naive_eval`/`qsearch`/`piece_value`/`count_material` are private (chess.rs:616-692) and would need `pub(crate)`.
- **src-tauri/src/progress.rs:24-47 update_progress + AppState.progress_state (main.rs:95) and AppState.analysis_cancel_flags (main.rs:92) + chess.rs:464-470 cancel_analysis**: A commentary pipeline command takes an `id: String` from the frontend (convention: `${purpose}_${tab}`, e.g. `report_${tab}` in ReportModal.tsx:84), calls `update_progress(&state.progress_state, &app, id.clone(), pct, false)` per step and `(…, 100.0, true)` at the end; frontend uses `useProgress(id)` (src/hooks/useProgress.ts) / `ProgressButton`. For cancellation either reuse `analysis_cancel_flags` with the same id (so the existing `commands.cancelAnalysis` works) or add a new `DashMap<String, Arc<AtomicBool>>` field to AppState and poll it between LLM calls/plies.
  - risk: Cancellation is cooperative only (no task abort); an in-flight reqwest call will not be interrupted unless you `tokio::select!` it against the flag or use a CancellationToken (tokio-util is not a dependency). `AnalysisCancelled` surfaces to TS as the string "Analysis cancelled" in `Result.error`.
- **src-tauri/src/error.rs:4-95 and :97-185**: Add variants such as `#[error("LLM provider error: {0}")] LlmProvider(String)`, `#[error("No API key configured")] MissingApiKey`, `#[error("Commentary cancelled")]`; add a `From<serde_json::Error>` impl if parsing provider JSON (none exists today). Messages are user-facing because `unwrap()` (src/utils/unwrap.tsx) shows them verbatim in a notification.
  - risk: Errors are stringly typed on the TS side (Type = string), so the frontend cannot discriminate variants except by message text; keep messages stable and i18n-neutral. Avoid leaking API keys or full provider bodies into messages (they get logged by plugin-log).
- **src-tauri/src/opening.rs:76-82 get_opening_from_setup and :11-17 Opening._eco**: A Rust facts extractor can call `crate::opening::get_opening_from_setup(pos.clone().into_setup(EnPassantMode::Legal))` per ply to find where the game left book (pattern already used in db/mod.rs:1461-1470: collect setups, reverse, find_map). To return ECO, add a `pub fn get_opening_info_from_setup(Setup) -> Option<OpeningInfo{eco,name,pgn}>` and rename `_eco` → `eco`.
  - risk: Match is exact `Setup` equality (opening.rs:79) which includes castling rights, ep square and — because shakmaty `Setup` derives PartialEq over all fields — the halfmove/fullmove counters, so transpositions reached at a different move number do not match; a commentary feature probably wants a counter-insensitive comparison (compare board+turn+castling only). Changing get_opening_from_fen's TS return type (string today, generated.ts:91) would break callers; add a new command instead.
- **src-tauri/src/fs.rs:21-86 download_file (reqwest pattern) vs src/utils/lichess/api.tsx:5 + src-tauri/capabilities/main.json:44-55 (frontend fetch pattern)**: Rust-side LLM client: `reqwest::Client` with `.json()` body and `res.bytes_stream()` + `futures_util::StreamExt` for SSE streaming, errors via `Error::Reqwest` (From impl exists), API key passed per call as a command argument like `token: Option<String>` (fs.rs:29, 39-43), streamed tokens emitted as a typed Event `CommentaryChunkEvent { id, ply, delta }`. Frontend-side alternative: `import { fetch } from "@tauri-apps/plugin-http"` plus adding `https://api.anthropic.com/**` (and any other provider) to the `http:default` allow list in capabilities/main.json, headers via `apiHeaders()` (src/utils/http.ts:17-22).
  - risk: Repo convention for third-party chess APIs is frontend fetch; Rust-side HTTP is only used for binary downloads and OAuth, so maintainers may ask why the provider call lives in Rust. Conversely frontend fetch exposes the key to the webview and streaming support of plugin-http fetch is unverified. There is no secret store on either side: lichess tokens live in localStorage via `sessionsAtom = atomWithStorage<Session[]>("sessions", [])` (src/state/atoms.ts:213).
- **src-tauri/src/db/encoding.rs:22-28 encode_comment + db/mod.rs:468-471 Importer::comment + db/mod.rs:1788-1860 write_db_game; src/utils/treeReducer.ts:16-28 TreeNode.comment; src-tauri/src/pgn.rs:243-279 write_game**: Zero-schema persistence: write generated commentary into `TreeNode.comment` (and NAG-style `annotations`) on the frontend; saving a file goes through write_game (PGN text, comments preserved) and saving to a DB goes through write_db_game which re-imports the PGN and encodes `{comments}` with COMMENT_MARKER into `Games.Moves`. Export round-trips via decode_game_to_movetext (db/mod.rs:1748-1757).
  - risk: Comments are free text in the mainline byte stream; there is no structured per-ply metadata (e.g. provider, model, confidence). If structured storage is wanted, the unused `comments` diesel table (schema.rs:87-99) is a tempting but dead declaration — it is not in create.sql and there is no migration mechanism (CREATE_TABLES_SQL only runs when the DB file does not exist, db/mod.rs:530-535; Info.Version is written but never read for upgrades).
- **src-tauri/src/db/search_index.rs:316-318 get_index_path + :12-13 MAGIC/VERSION, db/mod.rs:617-698 generate_search_index**: Precedent for a derived-data sidecar file keyed to a DB path (`<db>.ecsi`, rkyv + memmap, versioned header, regenerated lazily when `MmapSearchIndex::is_valid` fails). A commentary cache (e.g. `<db>.commentary.sqlite` or JSON keyed by game id + engine settings hash) could follow the same shape.
  - risk: Sidecar files are invisible to the DB's own integrity story (deleting a DB via delete_database db/mod.rs:1531-1545 must also remove the sidecar — check whether it removes .ecsi today).
- **src-tauri/src/engine/process.rs:28-162 BaseEngine and chess.rs:39-153 EngineProcess**: If commentary wants extra engine probes (e.g. evaluate the position after the threat move, or evaluate the refutation of the played move), reuse `BaseEngine::spawn` + `init_uci` + `set_position(fen, moves)` + `go(&GoMode::Depth(n))` + `wait_for_bestmove`, or drive `EngineProcess::set_options` + `go` and parse lines with `vampirc_uci::parse_one` → `parse_uci_attrs` (chess.rs:197-246).
  - risk: Engine processes are per (tab, engine) in `AppState.engine_processes`; spawning additional engines per commentary run multiplies CPU and must be killed on cancel (analyze_game calls `proc.kill()` chess.rs:514). `parse_uci_attrs` is private.
- **src/state/store/tree.ts:693+ addAnalysis and src/utils/annotation.ts (Annotation type, getAnnotation)**: Today Rust returns only scores/PV/novelty/is_sacrifice and the FRONTEND classifies moves (??/?/?!/!/!!). A clean architecture would move classification + facts to a Rust `facts` module and return a typed `MoveFacts { classification, cp_loss, best_move_san, threat_san, motifs: Vec<Motif>, … }` consumed by both the offline template renderer and the LLM prompt builder.
  - risk: Must stay consistent with existing getAnnotation thresholds or the UI annotations change for existing users; the frontend tests in src/utils/tests/store.test.ts:415 cover addAnalysis.

## Conventions
- Command shape: `#[tauri::command]` then `#[specta::specta]`, `pub async fn name(args.., app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<T, Error>` (chess.rs:473-482, fs.rs:21-32, game.rs:1506-1516). `app`/`state`/`window` params are omitted from the TS signature; remaining params become positional TS args in Rust order and are sent as a camelCased object (generated.ts:155-157 `convertPgn(files, dbPath, timestamp, title, description)` ↔ db/mod.rs:502-510). `&str`, `String`, `PathBuf` params all become `string`.
- Sync commands are allowed when there is no State borrow across await: `get_progress`/`clear_progress` (progress.rs:49-59), `clear_games` (db/mod.rs:1899-1903), `memory_size` (main.rs:272-277); a non-Result return produces a plain `Promise<T>` (generated.ts:72-74).
- All fallible commands return the crate `Error` (never String except close_splashscreen main.rs:100); new foreign error types get a boxed transparent variant + manual `From` impl (error.rs:97-185); domain failures get unit variants with `#[error("human text")]`.
- Types exported to TS: `#[derive(Serialize/Deserialize, Debug, Clone, Type)]`; newer code adds `#[serde(rename_all = "camelCase")]` (game.rs:49-51, 56-58, chess.rs:171-172, puzzle.rs:104-105); older types do not (MoveAnalysis, NormalizedGame, DatabaseInfo → snake_case in TS). Enums are internally/adjacently tagged: `#[serde(tag = "type", rename_all = "camelCase")]` (game.rs:35, 89), `#[serde(tag = "type", content = "value")]` (lexer.rs:12, engine/process.rs:20), `#[serde(tag = "t", content = "c")]` (engine/types.rs:11). `#[specta(optional)]` on Option fields yields `field?: T | null` (models.rs:155-175); `#[serde(default)]` yields `field?: T` (game.rs:63-64 → generated.ts:503).
- Events: `#[derive(Clone, Debug, Serialize, Type, tauri_specta::Event)]` struct, registered in collect_events! (main.rs:173-180), emitted with `MyEvent { .. }.emit(&app)?` (progress.rs:39-44, game.rs:683-690; `use tauri_specta::Event;` trait import). Event name = kebab-case of struct name (generated.ts:476-482). Events are app-global, so payloads carry a discriminator (`id`, `tab`, `game_id`) and the frontend filters (useProgress.ts:17, BoardGame.tsx:463-480). Legacy raw `app.emit("snake_name", payload)` still exists (db/mod.rs:568, search.rs:336, oauth.rs:106) but is not the pattern to copy.
- Progress: id-keyed, float percent, `finished` flag; `update_progress(&state.progress_state, &app, id, pct, finished)` (progress.rs:24); ids are chosen by the frontend (`report_${tab}`, download ids). Cancellation: cooperative `Arc<AtomicBool>` in a DashMap keyed by the same id (main.rs:92, chess.rs:464-470, 513-517).
- State: one `AppState` with `DashMap` for concurrent maps, `std::sync::Mutex` for rarely-held caches (never across await), `tokio::sync::Mutex`/`RwLock` inside `Arc` for per-entity async state (engine_processes, GameManager.games), `Arc<Semaphore>` for throttling heavy work (main.rs:85-86, search.rs:263), `derivative` for non-trivial Defaults. Module-private caches use `once_cell::sync::Lazy<Mutex<..>>` (puzzle.rs:97) or `lazy_static!` (opening.rs:117).
- Concurrency: no spawn_blocking; blocking Diesel/IO and rayon `par_iter` run directly inside async commands (db/mod.rs:1424, search.rs:422). Fire-and-forget tasks use `tokio::spawn` (game.rs:643, 1182) or `tauri::async_runtime::spawn` (oauth.rs:76, sound.rs:122); shutdown via `watch` channel + `JoinHandle::abort` (game.rs:1218-1226). UI update rate limited with `governor` (chess.rs:363).
- Chess position handling: parse with `engine::parse_fen_to_position` / `parse_fen_and_apply_moves` (engine/uci.rs) which auto-detects Chess960 and tolerates too-much-material; moves are exchanged as UCI strings and converted with `UciMove::from_ascii(..)?.to_move(&pos)?`; SAN produced with `SanPlus::from_move_and_play_unchecked`; positions advanced with `play_unchecked`; FENs produced with `Fen::from_position(pos, EnPassantMode::Legal)`. Scores are normalised to White POV in Rust (chess.rs:224-226).
- Logging: `log::{info, warn, error}` macros routed through tauri-plugin-log at Info level (main.rs:204-209); a few stray `println!` remain (chess.rs:802, db/mod.rs:1527, oauth.rs:93) — do not add more.
- DB access: `get_db_or_create(&state, path, ConnectionOptions::default())` returns a pooled Diesel SqliteConnection (db/mod.rs:148-171); schema via `diesel::table!` with `#[sql_name]` PascalCase columns (schema.rs); KV metadata in `Info` through `set_info_value` (db/mod.rs:181-193); raw SQL via `batch_execute`/`sql_query` where Diesel DSL is awkward.
- Testing: inline `#[cfg(test)] mod tests { use super::*; … }` at the bottom of the module; plain `#[test]` (no tokio::test, no dev-dependencies, no tests/ dir); fixtures are FEN strings via a local helper (`fn pos(fen: &str) -> Chess` chess.rs:716-719) or PGN raw strings fed to the Visitor (db/mod.rs:1960-1980); DB tests use `SqliteConnection::establish(":memory:")` + CREATE_TABLES_SQL (db/mod.rs:2007-2012). 36 `#[test]` total across opening.rs(1), chess.rs(9), db/encoding.rs(7), db/search.rs(9), db/mod.rs(6), db/search_index.rs(4). Run locally with `cargo test` in src-tauri; CI does not run it.
- Formatting: default rustfmt (no rustfmt.toml/clippy.toml/rust-toolchain file); `.editorconfig` = 4-space, LF. Frontend: oxfmt + oxlint (`pnpm format`, `pnpm lint:fix`), i18n keys must be extracted (`pnpm i18n:extract`) per CONTRIBUTING.md:48-68.
- Frontend consumption of commands: `commands.xxx(...)` → branch on `result.status === "ok"` (ReportModal.tsx:97, Puzzles.tsx:107) or `unwrap(result)` (src/utils/unwrap.tsx) which notifies + throws; events via `events.xxxEvent.listen(({ payload }) => …)` returning a Promise<UnlistenFn> cleaned up in useEffect (useProgress.ts:15-28).
- External HTTP: frontend `import { fetch } from "@tauri-apps/plugin-http"` (lichess/api.tsx:5, chess.com/api.tsx:5, chessdb/api.ts:1, engines.ts:1, db.ts:3) with `apiHeaders()` User-Agent (src/utils/http.ts) and a URL allowlist in capabilities/main.json; Rust reqwest only in fs.rs download_file (streaming, Bearer token param) and oauth2.

## Gotchas
- generated.ts is produced at RUNTIME of a debug build (main.rs:182-188), not by build.rs or a pnpm script; you must launch `pnpm dev` (or `cargo run` in debug) after changing commands/types, and commit the result. Release builds never write it. CI never checks it is in sync.
- `BigIntExportBehavior::BigInt` (main.rs:185) maps u64/usize/i64 to TS `bigint` (ClockUpdateEvent.whiteTime, OpeningBookConfig.maxPly, TimeControl, DatabaseInfo.storage_size — generated.ts:493, 494, 517, 552). Use u32/i32/f32/f64 for numbers the UI should treat as `number`.
- Errors cross IPC as plain strings (error.rs:187-203); the TS side cannot pattern-match variants. `Error::AnalysisCancelled` therefore arrives as `{status:"error", error:"Analysis cancelled"}` and callers just ignore non-ok (ReportModal.tsx:97-103).
- Tauri rule: an `async` command that borrows `tauri::State` must return a `Result` (all async State-using commands here do); the sync `get_progress` returns `Option<ProgressItem>` without Result.
- Mixed casing already shipped: `MoveAnalysis.is_sacrifice`, `NormalizedGame.white_elo`, `DatabaseInfo.player_count` are snake_case in TS. New types should use `#[serde(rename_all = "camelCase")]`; do not rename existing fields (frontend code depends on them).
- `AnalysisOptions.moves` are UCI strings of the MAINLINE only (ReportPanel.tsx:50 getMainLine); `analyze_game` output is index-aligned with the `fens` vector that drops positions where `is_game_over()` (chess.rs:489-510), so `analysis.len()` can be `moves.len()` (not +1) when the game ends in mate/stalemate; addAnalysis walks the tree in parallel (tree.ts:705-707).
- MultiPV is forced to 2 in analyze_game (chess.rs:527-539) regardless of user engine settings; `real_multipv` is clamped to the number of legal moves (chess.rs:104) and there is an `assert_eq!` on it (chess.rs:580) that can panic the command if an engine misbehaves.
- `is_sacrifice` is a Rust heuristic (prev naive_eval > cur naive_eval + 100, chess.rs:501-507) but move classification (??, ?, !) is a frontend heuristic (getAnnotation in src/utils/annotation.ts called from tree.ts:721-729) — logic is split across the IPC boundary.
- Opening lookup is exact `Setup` equality (opening.rs:79); because shakmaty `Setup` includes halfmove/fullmove counters, a position reached by transposition at a different move number will not be recognised. ECO codes are loaded but never exposed (`_eco`, opening.rs:13).
- The diesel `comments` table in schema.rs:87-99 does not exist in create.sql and has no users — do not assume per-ply comment storage exists in the DB. The only per-move text storage is the COMMENT_MARKER escape inside the `Games.Moves` BLOB (encoding.rs:22-28) and `TreeNode.comment` on the frontend.
- No DB migration framework: create.sql runs only when the file is new (db/mod.rs:528-535); `Info.Version` ("1.0.0") is written but never consulted. Adding a table to existing user databases needs hand-rolled `CREATE TABLE IF NOT EXISTS`.
- Blocking is the norm inside async commands (Diesel, rayon, file IO); heavy new work will block a tokio worker thread exactly like existing commands. If you add real async HTTP with streaming, do not hold `std::sync::Mutex` guards (e.g. `state.db_cache`) across `.await`.
- Cancellation flags are only polled between plies (chess.rs:513); a long single `go` cannot be interrupted except by killing the engine. `cancel_analysis` on an unknown id is a silent no-op (chess.rs:466-468).
- `get_engine_config` prints the whole config with `println!` (chess.rs:802) and `get_players_game_info` prints timings (db/mod.rs:1527) — pre-existing noise, not a convention.
- Raw `app.emit("convert_progress", (usize, u32, Option<String>))` (db/mod.rs:568) and `app.emit("search_progress", ProgressPayload)` (search.rs:336) bypass specta typing (`ProgressPayload` has no `Type` derive, search.rs:228-233) and are listened to with hand-written generics in TS (App.tsx:223, DatabaseLoader.tsx:17); new code should use the typed Event path.
- Frontend HTTP is sandboxed by capabilities/main.json `http:default` allow list; a new provider host (e.g. api.anthropic.com) silently fails until added there. Rust reqwest is not subject to that allowlist.
- No secrets store exists: lichess access tokens live in localStorage (`sessionsAtom`, atoms.ts:213). An LLM API key stored the same way is readable from the webview; a Rust-side keyring is not present in Cargo.toml.
- Rust tests are not run in CI (test.yml runs only pnpm lint:ci and vitest). Reviewers will not see Rust test failures automatically.
- `tauri::State<'_, AppState>` is `Clone` (used at chess.rs:663 `state.clone()`) but `AppState` itself is not; pass `&state`/`State` into helpers (pattern: `generate_search_index(&file, &state)` db/mod.rs:618).

## Open questions
- shakmaty 0.27.1 API surface beyond what the repo uses could not be verified locally (no cargo registry source on this machine; no network use attempted). The repo uses: Fen/Setup/CastlingMode::detect/ignore_too_much_material, UciMove, San/SanPlus, Position::{legal_moves, play_unchecked, turn, board, is_checkmate, is_stalemate, is_insufficient_material, is_game_over, halfmoves}, Move::{is_capture, capture, is_promotion, role}, Board::{material, from_ascii_board_fen}, Bitboard ops, into_setup(EnPassantMode::Legal), shakmaty::Outcome. NOT used anywhere: `shakmaty::attacks::*`, `Position::checkers()/king_attackers()/is_check()`, `Board::attacks_to()/attacks_from()`, pin detection, SEE (shakmaty has no SEE; the in-repo substitute is the private capture-only `qsearch`/`naive_eval` in chess.rs:646-692). Verify availability/names of attacks/checkers/king_attackers/attacks_to in shakmaty 0.27.1 docs before designing the motif extractor.
- Whether `@tauri-apps/plugin-http` 2.5.7 `fetch` supports streaming (ReadableStream body / SSE) in the webview was not verified; Rust `reqwest::Response::bytes_stream` + `futures_util::StreamExt` is proven in fs.rs:53-63.
- Whether shakmaty `Setup`'s PartialEq really includes halfmoves/fullmoves (which would make opening.rs:79 counter-sensitive) is inferred from the struct definition in memory, not from local source; confirm against docs.rs.
- No maintainer guidance on Rust style/tests exists (CONTRIBUTING.md only covers pnpm commands); unknown whether maintainers would accept a cargo-test CI job or new heavy dependencies (e.g. an Anthropic SDK crate vs raw reqwest).
- `delete_database` (db/mod.rs:1531-1545) — not read in full; unknown whether it removes the `.ecsi` sidecar, which matters if commentary adopts the sidecar pattern.
- Where/whether `Info.Version` is ever read for compatibility checks was not exhaustively grepped (only writes at db/mod.rs:532 observed).
- The exact frontend thresholds in `getAnnotation` (src/utils/annotation.ts) were not read; a Rust-side classifier must mirror or replace them deliberately.
