# Game metadata, clocks, player accounts, home dashboards (/accounts), local game database (SQLite .db3 + mmap search index), repertoire/practice, tabs — i.e. the non-engine context a commentary system can draw on in En Croissant v0.15.1 (upstream HEAD 23f8314).

## Summary
A loaded game lives in a per-tab zustand store (`createTreeStore(id)` in src/state/store/tree.ts:95-526, provided via `TreeStateProvider` in src/components/common/TreeStateContext.tsx and persisted to sessionStorage under the tab id with a 300 ms debounce, src/state/store/debouncedStorage.ts). Its shape is `TreeState { root: TreeNode; headers: GameHeaders; position: number[]; dirty; report: ReportState }` (src/utils/treeReducer.ts:8-14). Per-move data is on `TreeNode` (fen, move, san, children, score, depth, halfMoves, shapes, annotations, comment, clock?) — `clock` is seconds remaining for the side that just moved, filled either from `[%clk]` PGN comments by `innerParsePGN` (src/utils/chess.ts:399-401, via chessops `parseComment`, which also fills `score` from `[%eval]` and `shapes` from `[%csl]/[%cal]`) or from the play-mode clock (`createNode({clock})` divides ms by 1000, treeReducer.ts:127; Board.tsx:227-230, BoardGame.tsx:205-226). Game-level metadata is `GameHeaders` (treeReducer.ts:138-160): event/site/date/round/white/black/white_elo/black_elo/result/time_control/white_time_control/black_time_control/eco/variant/other/start/orientation; `getPgnHeaders` (chess.ts:478-535) only maps a fixed set of tags and throws every other tag (ECO, UTCTime, Opening, Termination, WhiteRatingDiff, WhiteTimeControl…) into `headers.other: Record<string,string>`. `headers.eco`/`headers.time` are populated only when a game is opened from a database, because `createTab` (src/utils/tabs.ts:96-142) overwrites `tree.headers` with the `NormalizedGame` record passed as `headers` (GameCard.tsx:40-55, GameTable.tsx:214-228, panels/database/GamesTable.tsx:41-57). Derived helpers already exist: `getClockInfo` (src/utils/clock.ts:64-143; parses `time_control` "moves/seconds+inc[:…]" and derives both players' remaining time from the current and parent nodes), `getTimeControl(website, tc)` speed classification (src/utils/timeControl.ts), `getOpening(root, position)` -> `commands.getOpeningFromFens` (chess.ts:333-351; name only, from built-in ECO TSVs in src-tauri/src/opening.rs), `getGamePhases` (src/utils/phase.ts:77-97, Lichess Divider port -> middlegamePly/endgamePly), `getGameStats` (chess.ts:541-612: per-colour ACPL, harmonic-mean accuracy, annotation counts), `getMaterialDiff` (chess.ts:622-654), and `getAccuracy/getCPLoss/getWinChance/getAnnotation` (src/utils/score.ts).

Player identity and aggregates: accounts are `Session[]` in `sessionsAtom` (localStorage key "sessions", src/state/atoms.ts:213) holding a Lichess `LichessAccount` (perfs per speed with rating/rd/prog/games, src/utils/lichess/api.tsx:74-148; plus optional OAuth `accessToken` stored in plaintext) and/or chess.com `ChessComStats` (chess_bullet/blitz/rapid/daily with last.rating and record win/loss/draw, src/utils/chess.com/api.tsx:18-37), grouped under an alias `player`. Each account's games are bulk-downloaded to `<username>_lichess.pgn` / `<username>_chesscom.pgn` (api.tsx `downloadLichess` 385-406 / `downloadChessCom` 102-150) and converted by `commands.convertPgn` (src-tauri/src/db/mod.rs:500-615) into a .db3 titled "<username> Lichess" / "<username> Chess.com" (AccountCard.tsx:104-136). The Rust `Importer` (mod.rs:321-498) keeps only White/Black/WhiteElo/BlackElo/TimeControl/ECO/Round/Date|UTCDate/UTCTime/Site/Event/Result/FEN plus the encoded move blob, into which comments and NAGs are embedded verbatim (encoding.rs `encode_comment`), so `[%clk]`/`[%eval]` comments that were in the source PGN round-trip through `decode_game_to_movetext` (encoding.rs:316-324) -> `NormalizedGame.moves` -> `parsePGN`. Per-player aggregates come from `commands.getPlayersGameInfo(file, playerId)` (mod.rs:1378-1529) which returns `PlayerGameInfo { site_stats_data: SiteStatsData[] }` with one `StatsData { date, is_player_white, player_elo, result: Won|Drawn|Lost, time_control, opening }` per game (opening name detected from the first 55 plies; games with a FEN tag, missing elo/result/date/site are skipped). The home dashboard (src/components/home/Databases.tsx:78-120) resolves the player id with `query_players(db.file, { name: db.username })`, calls `getPlayersGameInfo`, merges all accounts' `site_stats_data`, and renders `PersonalPlayerCard` with OverviewPanel (totals + per-month counts), RatingsPanel (max elo per day timeline) and OpeningsPanel (`aggregateOpenings` -> `{name, games, won, draw, lost}` per colour, OpeningsPanel.tsx:28-59). Position-level history is a separate path: `searchPosition(LocalOptions, tabId)` (src/utils/db.ts:168-191 -> `commands.searchPosition`, src-tauri/src/db/search.rs:237-462) filters the mmap index by `player1`(white id)/`player2`(black id)/`wanted_result`/date range and returns `[PositionStats{move,white,draw,black}[], NormalizedGame[] (top 500 by elo)]`; repertoire coverage (src/utils/repertoire.ts:20-169) and the board's Database tab (panels/database/DatabasePanel.tsx) both use it. The Lichess opening explorer per-player endpoint is also wired (`getLichessGames(fen, {player, color,...}, token)` lichess/api.tsx:322-346; needs the OAuth token, DatabasePanel.tsx:122-124).

Tabs: `tabsAtom` (sessionStorage "tabs", zod-validated) holds `Tab { name, value(id), type: "new"|"play"|"analysis"|"puzzles", gameOrigin }` where `gameOrigin` is `{kind:"none"} | {kind:"file"|"temp_file", file: FileMetadata, gameNumber} | {kind:"database", database: path, gameId}` (src/utils/tabs.ts:37-66). Everything else per-tab is either (a) in the tree store (persisted), or (b) a jotai `atomFamily((_tab: string) => atom(...))` wrapped by `tabValue(family)` which resolves the *active* tab (atoms.ts:310-329; e.g. `currentTabSelectedAtom` for the top-right panel tab, `currentAnalysisTabAtom` for engines/report/logs, `currentReportModalOpenAtom`, `currentLocalOptionsAtom`, `engineMovesFamily({tab, engine})`). These are in-memory only. BoardsPage (src/components/tabs/BoardsPage.tsx:291-347) mounts `TreeStateProvider id={tab.value}` + `BoardAnalysis`/`BoardGame`/`Puzzles` per tab with `keepMounted={false}`. The "Generate report" flow is ReportPanel -> ReportModal (`commands.analyzeGame("report_"+tab, …, {fen, moves: getMainLine(root), annotateNovelties, referenceDb, reversed})` -> `MoveAnalysis[]` -> `store.addAnalysis(analysis, {showVariations})` (ReportModal.tsx:72-104; tree.ts:693-809 writes node.score, annotations, "N" novelty and best-line variations), with `report.inProgress` on the tree state and keybind F12 (BoardAnalysis.tsx:151-159). Repertoires are ordinary PGN files whose `.info` sidecar has `metadata.type === "repertoire"` (src/components/files/file.ts), with the user's colour in `headers.orientation` and the drill start path in `headers.start` (written as `[Orientation]`/`[Start "[0,0]"]` tags, chess.ts:187-191); the tree itself is the repertoire, and the FSRS deck is `deckAtomFamily({file, game})` in localStorage `deck-${file}-${game}` (atoms.ts:481-493, src/components/files/opening.ts). `findFen(fen, root)` (treeReducer.ts:46-54) is the existing primitive to test whether a game position is inside a repertoire tree.

## Key files
- `src/utils/treeReducer.ts` — Core per-game data model: TreeState, TreeNode, GameHeaders, ReportState, iterators, defaultTree/createNode
  - TreeState (8-14)
  - TreeNode (16-28)
  - treeIterator (35-44)
  - findFen (46-54)
  - treeIteratorMainLine (56-65)
  - countMainPly (67-75)
  - defaultTree (77-108)
  - createNode (110-136; clock ms -> s at 127)
  - GameHeaders (138-160)
  - getGameName (162-170)
  - getNodeAtPath (172-181)
  - getTreeStructureHash (183-194)
  - ReportState (196-198)
- `src/utils/chess.ts` — PGN parse/serialize, headers mapping, opening lookup, game stats, material
  - makeClk (37-46)
  - getMoveText (48-127; emits [%eval]/[%clk]/[%csl]/[%cal] + comment inside one {})
  - getMainLine (139-141)
  - getVariationLine (148-170)
  - headersToPGN (172-212)
  - getPGN (218-306)
  - getOpening (333-351)
  - innerParsePGN (353-458; parseComment -> score/shapes/clock/comment at 367-404; strips [%timestamp])
  - parsePGN (460-476)
  - getPgnHeaders (478-535; only maps Black/White/BlackElo/WhiteElo/Date/Site/Event/Result/FEN/Round/Start/Orientation/TimeControl/Variant, rest -> other)
  - getGameStats (541-612)
  - getMaterialDiff (622-654)
- `src/utils/clock.ts` — Time-control parsing and per-side remaining-time derivation from TreeNode.clock
  - TimeControlField (26-30)
  - parseTimeControl (39-62, module-private, handles 'moves/seconds+inc' fields separated by ':')
  - getClockInfo (64-143)
  - calculateProgress (5-24)
- `src/components/boards/Clock.tsx` — Clock widget reading store headers/root/position/currentNode and getClockInfo
  - Clock (10-74)
  - formatClock (76-92)
- `src/utils/timeControl.ts` — Speed classification of a time_control string per website
  - getTimeControl(website, timeControl) (1-23): Chess.com -> daily/bullet/blitz/rapid; else '-' -> correspondence, else ultra_bullet/bullet/blitz/rapid/classical
- `src/utils/chess.com/api.tsx` — chess.com account stats, bulk archive download, single-game import via TCN
  - ChessComStatsSchema/ChessComStats (18-37)
  - ChessComGames schema (49-64; only .pgn is used)
  - getChessComAccount (66-94)
  - downloadChessCom (102-150; writes <player>_chesscom.pgn from archive pgn fields)
  - getChesscomGame (159-244; callback endpoint, moveList TCN + pgnHeaders -> PGN with NO comments/clocks)
  - getStats (246-273)
- `src/utils/chess.com/tcn.ts` — TCN move decoding
  - decodeTCN (6-36)
- `src/utils/lichess/api.tsx` — Lichess account, explorer (incl. per-player), cloud eval, bulk/single game export, FIDE lookup
  - LichessAccount (82-148)
  - PositionGames/PositionData (150-206)
  - convertToNormalized (168-190; fetches each explorer game PGN)
  - getLichessAccount (208-236)
  - getBestMoves cloud eval (238-287)
  - getLichessGames (322-346; /lichess or /player explorer by options.player)
  - getMasterGames (348-367)
  - getPlayerGames (369-383)
  - downloadLichess (385-406; /api/games/user/{p}?perfType=...&rated=true&sort=dateAsc[&since])
  - getLichessGame (408-414; https://lichess.org/game/export/{id} no params)
  - getFidePlayer (426-448)
- `src/utils/lichess/explorer.ts` — Zod schemas for explorer options
  - lichessGamesOptionsSchema/LichessGamesOptions (28-56; player, color, speeds, ratings, since/until)
  - masterOptionsSchema (58-66)
  - getLichessGamesQueryParams (68-101)
- `src/utils/session.ts` — Account session types
  - LichessSession (4-8)
  - ChessComSession (10-13)
  - Session (15-20)
- `src/state/atoms.ts` — All jotai state: tabs, sessions, settings, per-tab atom families, practice/deck
  - tabsAtom (58-62)
  - activeTabAtom (64-68)
  - currentTabAtom (76-95)
  - sessionsAtom (213)
  - recentFilesAtom/RecentFile (221-237)
  - referenceDbAtom (241)
  - selectedDatabaseAtom (267-271)
  - tabValue() helper (310-329)
  - coverageMinGamesAtom (343)
  - reportTypeAtom (350)
  - currentTabSelectedAtom/tabFamily (374-375)
  - currentReportModalOpenAtom (377-378)
  - currentLocalOptionsAtom (380-390)
  - lichessOptionsAtom (392-403)
  - currentDbTypeAtom/currentDbTabAtom (414-420)
  - currentAnalysisTabAtom (422-423)
  - currentPracticeTabAtom (425-426)
  - deckAtomFamily/PracticeData (471-493)
  - PracticeState/practiceStateAtom (495-513)
  - engineMovesFamily (544-548)
  - bestMovesFamily (556-601)
  - tabEngineSettingsFamily (636-661)
- `src/state/store/tree.ts` — Per-tab zustand tree store (actions, persistence, addAnalysis)
  - TreeStoreState (26-91)
  - createTreeStore (95-526; persist with createDebouncedSessionStorage when id given, 516-523)
  - makeMove (528-609; updates headers.result on mate/stalemate/3-fold/50-move)
  - setHeaders (453-463)
  - setComment (443-452)
  - setScore (478-487)
  - addAnalysis action (488-494) and impl (693-809)
  - setReportInProgress (496-502)
- `src/state/store/debouncedStorage.ts` — 300ms debounced sessionStorage persist for tree stores, flushed on beforeunload/pagehide
  - createDebouncedSessionStorage (52-74)
- `src/state/store/database.ts` — Database page view store (selected db, games/players/tournaments queries); required context for PersonalPlayerCard
  - DatabaseViewStore (7-41; players.activeTab: 'overview'|'openings')
  - activeDatabaseViewStore (92-200; persisted sessionStorage 'database-view')
  - useActiveDatabaseViewStore (202-206)
- `src/utils/db.ts` — Frontend wrappers for DB commands
  - SuccessDatabaseInfo (19)
  - Speed (28-35, unused by NormalizedGame)
  - query_games (44-69)
  - query_players (71-88)
  - getDatabases (90-97; lists *.db3 in databases dir via commands.getDbInfo)
  - Opening (150-155)
  - searchPosition(options: LocalOptions, tab) (168-191; color->player1/player2)
- `src/utils/tabs.ts` — Tab schema, createTab, saveToFile
  - gameOriginSchema/GameOrigin (37-65)
  - tabSchema/Tab (58-66)
  - getTabFile/getTabGameNumber (68-82)
  - createTab (96-142; parsePGN -> sessionStorage.setItem(id, {version:0,state}) and headers override at 117-121)
  - saveToFile (150-240; database origin -> commands.writeDbGame, file -> commands.writeGame)
- `src/components/tabs/BoardsPage.tsx` — Tab bar + per-tab TreeStateProvider/Mosaic mounting
  - closeTab (47-74; killEngines + abortGame)
  - duplicateTab (111-133; copies sessionStorage state)
  - TabSwitch (291-347)
- `src/components/tabs/ImportModal.tsx` — Import by PGN/file, chess.com or lichess link, FEN
  - handleSubmit (74-199; link import -> gameOrigin none at 164-174)
- `src/components/tabs/NewTabHome.tsx` — New-tab home: cards + recent files (repertoire due badge)
  - openRecentFile (138-178; sets tabFamily(id) to 'practice' for repertoires)
- `src/components/tabs/CreateRepertoireModal.tsx` — Creates a repertoire PGN with Orientation header and .info type
  - handleCreate (30-88)
- `src/components/boards/BoardAnalysis.tsx` — Analysis tab layout: top-right Tabs (practice/analysis/database/annotate/info), hotkeys incl. GENERATE_REPORT
  - GENERATE_REPORT handler (151-159)
  - Tabs.List/Panels (213-249)
  - autosave effect (82-86)
- `src/components/panels/analysis/AnalysisPanel.tsx` — Vertical tabs engines/report/logs
  - Tabs.List (101-107)
  - report panel (248-258)
- `src/components/panels/analysis/ReportModal.tsx` — Generate-report form and analyzeGame call
  - reportSettingsAtom (13-19, localStorage 'report-settings')
  - analyze (72-104)
- `src/components/panels/analysis/ReportPanel.tsx` — Report UI: ProgressButton, accuracy cards, EvalChart, annotation counts
  - ReportPanel (19-96)
  - GameStats (100-168)
  - AccuracyCard (170-191)
- `src/components/common/EvalChart.tsx` — Mainline eval chart with phase reference lines
  - getData (105-128)
  - phases via getGamePhases (150-153)
- `src/utils/phase.ts` — Game phase detection (Lichess Divider port)
  - GamePhases (72-75)
  - getGamePhases(boards: Chess[]) (77-97)
- `src/utils/score.ts` — Eval maths used by report
  - INITIAL_SCORE (7-13)
  - formatScore (17-32)
  - getWinChance (34-36)
  - normalizeScore (38-47)
  - getAccuracy (60-67)
  - getCPLoss (69-73)
  - getAnnotation (75-119)
- `src/utils/annotation.ts` — Annotation glyphs and NAG table
  - Annotation (12-38)
  - NAG_INFO (40-73)
  - ANNOTATION_INFO (83-190; includes '⊕' Time Trouble nag 138)
  - isBasicAnnotation (192-196)
- `src/components/panels/annotation/AnnotationEditor.tsx` — Tiptap markdown editor writing node.comment via setComment
  - AnnotationEditor (14-99; contentType markdown, onUpdate -> setComment(editor.getMarkdown()))
- `src/components/common/Comment.tsx` — Renders node.comment as markdown (react-markdown + rehypeRaw + remarkGfm)
  - Comment (12-35)
- `src/components/common/CompleteMoveCell.tsx` — Per-move cell in notation; renders comment and annotations
  - props comment/annotations/scoreText (64-82)
  - comment render (200)
- `src/components/common/GameInfo.tsx` — Header editor (players, elo, site link detection, date, result)
  - GameInfo (19-272; site URL detection 163-167; elo 0 == Unrated 217)
- `src/components/panels/info/InfoPanel.tsx` — Info tab: DB origin card, game selector for multi-game files, GameInfo, FEN/PGN inputs
  - InfoPanel (40-90)
  - DatabaseInfo (92-128)
  - GameSelectorAccordion (130-274)
- `src/components/home/AccountsPage.tsx` — /accounts route: Accounts + Databases
  - AccountsPage (7-23)
- `src/components/home/Accounts.tsx` — Add Lichess/chess.com accounts (OAuth via commands.authenticate + 'access_token' event)
  - addChessComSession/addLichessSession (37-63)
  - onLichessAuthentication (81-89)
  - AccountModal (143-240)
- `src/components/common/AccountCards.tsx` — Groups sessions by player alias; builds rating stat cards from perfs/stats
  - AccountCards (22-62)
  - LichessOrChessCom (176-300; db filename `${username}_lichess.db3` / `_chesscom.db3`)
- `src/components/home/AccountCard.tsx` — Download games + convert to db3 for an account
  - convert (104-136; title `${username} Lichess|Chess.com`)
  - getLastGameDate (160-177)
  - download onClick (212-245)
- `src/components/home/Databases.tsx` — Resolves per-account DBs and player ids, loads PlayerGameInfo, renders PersonalPlayerCard
  - isDatabaseFromSession (39-46; filename.includes(username))
  - personalDatabases SWR (78-84)
  - personalInfo SWR (86-120; query_players by name -> getPlayersGameInfo)
  - merge of site_stats_data (204-212)
- `src/components/home/PersonalCard.tsx` — Overview/Ratings/Openings tabs for a player (needs DatabaseViewStateContext)
  - PersonalPlayerCard (25-119)
- `src/components/home/PersonalCardPanels/OpeningsPanel.tsx` — Per-colour opening win/draw/loss aggregation and open-opening-in-tab
  - OpeningStats (20-26)
  - aggregateOpenings (28-59, module-private)
  - OpeningsPanel filters (74-89)
  - OpeningDetail createTab with getOpeningFromName (189-222)
- `src/components/home/PersonalCardPanels/OverviewPanel.tsx` — Totals and per-month counts
  - extractGameStats (63-81)
  - OverviewPanel (83-139)
- `src/components/home/PersonalCardPanels/RatingsPanel.tsx` — Rating timeline from StatsData.player_elo
  - RatingsPanel (43-218; ratingData 109-118)
- `src/components/home/PersonalCardPanels/WebsiteAccountSelector.tsx` — Website ('Chess.com'|'Lichess'|'All websites') and account selectors from sessions
  - WebsiteAccountSelector (207-277)
- `src/components/home/PersonalCardPanels/TimeControlSelector.tsx` — Speed selector values per website
  - LICHESS_TIME_CONTROLS/CHESSCOM_TIME_CONTROLS (92-106)
- `src/components/databases/PlayerCard.tsx` — DB page wrapper: getPlayersGameInfo SWR -> PersonalPlayerCard isDatabase
  - PlayerCard (8-33)
- `src/components/databases/PlayerSearchInput.tsx` — Name -> player id autocomplete via query_players/commands.getPlayer
  - PlayerSearchInput (8-69)
- `src/components/panels/database/DatabasePanel.tsx` — Board Database tab: local/lichess/masters stats and games for current FEN
  - LocalOptions (58-67)
  - fetchOpening (73-108)
  - DatabasePanel (110-288; clearGames on reference change 200-203)
- `src/components/panels/database/options/LocalOptionsPanel.tsx` — UI for LocalOptions (player, color, dates, result, exact/partial)
  - LocalOptionsPanel (23+)
- `src/utils/repertoire.ts` — Repertoire coverage vs reference DB using searchPosition; gap finders; tree stats
  - PositionMove (7-18)
  - computeTreeCoverage (20-169; tab id 'coverage-calc')
  - findNextGap (171-212)
  - findBiggestGap (214-274)
  - getStats (290, proxy-memoize)
- `src/components/files/opening.ts` — FSRS deck built from repertoire tree
  - positionSchema/Position (12-22)
  - buildFromTree (24-48)
  - getStats (58-81)
  - getCardForReview (83-95)
  - updateCardPerformance (97-115)
  - syncDeck (117-146)
- `src/components/panels/practice/PracticePanel.tsx` — Train tab (FSRS drill) + Build tab
  - deck sync effect (96-123)
  - newPractice (135-184; findFen + goToMove + setPracticePath)
- `src/components/panels/practice/RepertoireInfo.tsx` — Build tab: DB move stats at current node, coverage, gaps, presets
  - RepertoireInfo (50-357)
  - MovesView (381-677)
- `src/components/files/file.ts` — PGN file metadata (.info sidecar with type/tags)
  - FileType (9)
  - fileMetadataSchema/FileMetadata (18-27)
  - readFileMetadata (34-59)
  - processEntriesRecursively (68-93)
- `src/utils/files.ts` — openFile/createFile helpers
  - openFile (23-103)
  - createFile (105-135)
- `src/utils/puzzles.ts` — Puzzle DB listing and Puzzle type
  - Completion (7)
  - Puzzle (9-20)
  - getPuzzleDatabases (28-35)
- `src/bindings/generated.ts` — tauri-specta generated commands/types
  - getPlayersGameInfo (115)
  - getOpeningFromFens/Name (99-107)
  - analyzeGame (24)
  - getGames (329)
  - searchPosition (337)
  - getPlayers (345)
  - NormalizedGame (516)
  - PlayerGameInfo/SiteStatsData/StatsData (522,549,551)
  - GameOutcome (507)
  - GameQuery (509)
  - PositionStats (527)
  - Score/ScoreValue (534-547)
  - Player (520)
  - DatabaseInfo (494)
  - MoveAnalysis (515)
  - AnalysisOptions (490)
- `src/bindings/index.ts` — Frontend overrides: ScoreValue adds dtz; DatabaseInfo success/error union
  - DatabaseInfo (16-29)
- `src-tauri/src/db/mod.rs` — SQLite DB: PGN import, queries, player stats, PGN export, write-back
  - TempGame (211-228)
  - Importer Visitor (321-498; header() 330-384 whitelist; comment() 480-483)
  - convert_pgn (500-615)
  - get_games (913-1113)
  - normalize_games (1115-1161; moves = decoded movetext + result)
  - get_player (1182-1195)
  - PlayerGameInfo/GameOutcome/SiteStatsData/StatsData (1322-1370)
  - get_players_game_info (1378-1529)
  - PgnGame::write (1638-1709)
  - export_to_pgn (1711-1768)
  - write_db_game (1788-1860)
- `src-tauri/src/db/schema.rs` — Diesel schema (Games columns incl. TimeControl, ECO, UTCTime; unused Comments table)
  - games table (43-85)
  - comments table (87-99)
- `src-tauri/src/db/models.rs` — Game/Player/NormalizedGame/Outcome structs
  - Game (45-65)
  - Outcome (120-145)
  - NormalizedGame (147-177)
- `src-tauri/src/db/encoding.rs` — Move blob encoding with inline comments/NAGs/variations
  - markers (7-10)
  - encode_comment (22-28)
  - DecodedGameNode (102-108)
  - render comment as {…} (316-324)
  - decode_game_to_movetext (341-345)
- `src-tauri/src/db/search.rs` — Position search over mmap index with player/result/date filters; line_cache
  - PositionStats (156-163)
  - search_position (237-462; filters 346-378; single db_cache 265-298)
  - is_position_in_db (464+)
- `src-tauri/src/opening.rs` — Built-in ECO openings table (name + setup) from data/*.tsv
  - get_opening_from_fen (50-53)
  - get_opening_from_name (57-63)
  - get_opening_from_fens (67-74)
  - get_opening_from_setup (76-82)
  - search_opening_name (86-115)
  - OPENINGS lazy_static (117-169; eco field private/unused)
- `src-tauri/src/chess.rs` — Engine analysis (report) command and novelty detection against reference DB
  - MoveAnalysis (448-453)
  - AnalysisOptions (455-463)
  - analyze_game (476-634; novelty via is_position_in_db 615-629)
- `src-tauri/src/pgn.rs` — Multi-game PGN file read/write/delete by index
  - count_pgn_games (156-183)
  - read_games (187-209)
  - write_game (245-279)
- `src-tauri/src/main.rs` — AppState and command registry
  - AppState (78-96: line_cache, db_cache: Mutex<Option<MmapSearchIndex>>, pgn_offsets, engine_processes…)
  - collect_commands! (111+)
- `src/components/settings/SettingsPage.tsx` — Settings list driven by SettingItem[] with category + render
  - SettingCategory (78)
  - SettingItem (89)
  - settings useMemo (184+)
- `src/components/settings/RepertoireMinGamesSetting.tsx` — Example of a custom setting component bound to an atomWithStorage atom
  - RepertoireMinGamesSetting (7-64)
- `i18next.config.ts` — i18n extraction config (flat keys, removeUnusedKeys, preservePatterns)
  - extract options
- `src/utils/tests/store.test.ts` — Vitest conventions for tree store (full TreeState literals incl. report)
  - treeE4D5 (14-62)

## Data model
// src/utils/treeReducer.ts:8-28,138-160,196-198
export interface TreeState { root: TreeNode; headers: GameHeaders; position: number[]; dirty: boolean; report: ReportState; }
export interface TreeNode { fen: string; move: Move | null; san: string | null; children: TreeNode[]; score: Score | null; depth: number | null; halfMoves: number; shapes: DrawShape[]; annotations: Annotation[]; comment: string; clock?: number; }   // clock = seconds remaining for the mover, after the move
export type GameHeaders = { id: number; fen: string; event: string; site: string; date?: string | null; time?: string | null; round?: string | null; white: string; white_elo?: number | null; black: string; black_elo?: number | null; result: Outcome; time_control?: string | null; white_time_control?: string | null; black_time_control?: string | null; eco?: string | null; variant?: string | null; other?: Record<string, string>; /* Repertoire headers */ start?: number[]; orientation?: "white" | "black"; };
export interface ReportState { inProgress: boolean; }
// src/state/store/tree.ts:26-91 — TreeStoreState extends TreeState with actions (goTo*, makeMove({payload, changePosition?, mainline?, clock?, changeHeaders?}), appendMove({payload, clock?}), makeMoves, deleteMove, promoteVariation, setStart, setAnnotation, setComment, setHeaders, setResult, setShapes, setScore, addAnalysis(analysis: {best: BestMoves[]; novelty; is_sacrifice}[], options?: {showVariations}), setReportInProgress, practicePath, setPracticePath, setState, reset, save)

// src/bindings/generated.ts
export type Score = { value: ScoreValue; wdl: [number, number, number] | null }
export type ScoreValue = { type: "cp"; value: number } | { type: "mate"; value: number }   // index.ts adds { type: "dtz"; value: number }
export type Outcome = "1-0" | "0-1" | "1/2-1/2" | "*"
export type BestMoves = { nodes: number; depth: number; score: Score; uciMoves: string[]; sanMoves: string[]; multipv: number; nps: number }
export type MoveAnalysis = { best: BestMoves[]; novelty: boolean; is_sacrifice: boolean }
export type AnalysisOptions = { fen: string; moves: string[]; annotateNovelties: boolean; referenceDb: string | null; reversed: boolean }
export type NormalizedGame = { id: number; fen: string; event: string; event_id: number; site: string; site_id: number; date?: string | null; time?: string | null; round?: string | null; white: string; white_id: number; white_elo?: number | null; black: string; black_id: number; black_elo?: number | null; result: Outcome; time_control?: string | null; eco?: string | null; ply_count?: number | null; moves: string }   // moves = movetext incl. {comments} (+ [%clk]/[%eval] if they were in the source PGN) + result token
export type Player = { id: number; name: string | null; elo: number | null }
export type PlayerGameInfo = { site_stats_data: SiteStatsData[] }
export type SiteStatsData = { site: string; player: string; data: StatsData[] }   // site: "Lichess" (normalized from https://lichess.org/… at mod.rs:1453-1459) or raw e.g. "Chess.com"; player = DB player name
export type StatsData = { date: string; is_player_white: boolean; player_elo: number; result: GameOutcome; time_control: string; opening: string }   // no game id, no opponent
export type GameOutcome = "Won" | "Drawn" | "Lost"
export type GameQuery = { options?: QueryOptions<GameSort> | null; player1?: number | null; player2?: number | null; tournament_id?: number | null; start_date?: string | null; end_date?: string | null; range1?: [number, number] | null; range2?: [number, number] | null; sides?: Sides | null; outcome?: string | null; position?: PositionQueryJs | null; wanted_result?: string | null }
export type PositionQueryJs = { fen: string; type_: string }
export type PositionStats = { move: string; white: number; draw: number; black: number }   // move "*" = games ending here
export type DatabaseInfo = { title: string; description: string; player_count: number; event_count: number; game_count: number; storage_size: bigint; filename: string; indexed: boolean }  // index.ts: | & {type:"success"; file: string; downloadLink?; filter?} | {type:"error"; file; filename; error; indexed}
export type Puzzle = { id: number; fen: string; moves: string; rating: number; rating_deviation: number; popularity: number; nb_plays: number }
export type Token = { type: "ParenOpen" } | { type: "ParenClose" } | { type: "Comment"; value: string } | { type: "San"; value: string } | { type: "Header"; value: { tag: string; value: string } } | { type: "Nag"; value: string } | { type: "Outcome"; value: string }

// src/utils/tabs.ts:37-66
gameOrigin = {kind:"none"} | {kind:"file", file: FileMetadata, gameNumber: number} | {kind:"temp_file", file, gameNumber} | {kind:"database", database: string, gameId: number}
export type Tab = { name: string; value: string; type: "new" | "play" | "analysis" | "puzzles"; gameOrigin: GameOrigin }
// src/components/files/file.ts:7-27
FileType = "repertoire" | "game" | "tournament" | "puzzle" | "other"; FileMetadata = { type: "file"; name: string; path: string; numGames: number; metadata: { type: FileType; tags: string[] }; lastModified: number }
// src/state/atoms.ts:221-226
export type RecentFile = { name: string; path: string; type: "game" | "repertoire" | "tournament" | "puzzle" | "other"; lastOpened: number };

// src/utils/session.ts
export type LichessSession = { accessToken?: string; username: string; account: LichessAccount; };
export type ChessComSession = { username: string; stats: ChessComStats; };
export type Session = { lichess?: LichessSession; chessCom?: ChessComSession; player?: string; updatedAt: number; };
// src/utils/lichess/api.tsx:74-148 (untyped JSON cast)
type LichessPerf = { games: number; rating: number; rd: number; prog: number; prov: boolean };
export type LichessAccount = { id; username; perfs?: { chess960?, atomic?, racingKings?, ultraBullet?, blitz?, kingOfTheHill?, bullet?, correspondence?, horde?, puzzle?, classical?, rapid?: LichessPerf; storm?: {runs; score} }; createdAt; disabled; tosViolation; profile: { country; location; bio; firstName; lastName; fideRating; uscfRating; ecfRating; links }; seenAt; patron; verified; playTime: {total; tv}; title; url; playing; completionRate; count: { all; rated; ai; draw; drawH; loss; lossH; win; winH; bookmark; playing; import; me }; streaming; followable; following; blocking; followsYou };
// src/utils/chess.com/api.tsx:18-37 (zod)
ChessComPerf = { last: { rating; date; rd }, record: { win; loss; draw } }; ChessComStats = { chess_daily?, chess_rapid?, chess_blitz?, chess_bullet?: ChessComPerf }

// src/components/panels/database/DatabasePanel.tsx:58-67
export type LocalOptions = { path: string | null; fen: string; type: "exact" | "partial"; player: number | null; color: "white" | "black"; start_date?: string; end_date?: string; result: "any" | "whitewon" | "draw" | "blackwon"; };
// src/utils/lichess/explorer.ts:28-56
LichessGamesOptions = { variant?; speeds?: ("ultraBullet"|"bullet"|"blitz"|"rapid"|"classical"|"correspondence")[]; ratings?: (0|1000|…|2500)[]; since?: Date; until?: Date; moves?; topGames?; recentGames?; player?: string; color: "white"|"black" }
// src/utils/lichess/api.tsx:192-206
type PositionData = { white: number; black: number; draws: number; moves: { uci; san; averageRating; white; black; draws }[]; recentGames?: PositionGames; topGames?: PositionGames };

// src/utils/clock.ts:26-37
export type TimeControlField = { seconds: number; increment?: number; moves?: number; };  // ms
type ClockInfo = { progress: number; value: number | undefined; };  // value in seconds

// src/components/files/opening.ts:12-22 and src/state/atoms.ts:476-479
export type Position = { fen: string; answer: string; card: Card /* ts-fsrs */ };
export type PracticeData = { positions: Position[]; logs: (ReviewLog & { fen: string })[]; };

// src/components/home/PersonalCardPanels/OpeningsPanel.tsx:20-26 (module-private)
type OpeningStats = { name: string; games: number; won: number; draw: number; lost: number; };
// src/utils/repertoire.ts:7-18
export type PositionMove = { san; games; totalGames; frequency; white; draw; black; inRepertoire: boolean; coverage: number; childIndex: number; };

// Rust (src-tauri/src/db/mod.rs:211-228) — what survives DB import
pub struct TempGame { event_name, site_name, date, time, round, white_name, white_elo, black_name, black_elo, result, time_control, eco, fen: Option<String>, moves: Vec<u8> /* encoded moves + inline comments/NAGs/variations */, position: Chess, material_count }

## Extension points
- **src/components/panels/analysis/ReportModal.tsx:82-103 (analyze().then(analysis => addAnalysis(analysis.data, …)))**: The only place the raw `MoveAnalysis[]` (all multipv `BestMoves` with uciMoves/sanMoves/score/depth, novelty, is_sacrifice) is available; `addAnalysis` keeps only best[0].score, annotations and up to 10-ply variations for ?!/?/??. Hook commentary generation here: pass `analysis.data` + `store.getState()` (root/headers) + player context into a commentary service, then write results (see TreeState/TreeNode points). Add a 'commentary' checkbox to `reportSettingsAtom` (13-19) alongside novelty/reversed/variations.
  - risk: ReportModal is memo'd and form-driven; `setInProgress(false)` in `.finally` ends the ProgressButton — a second async phase (LLM) needs its own progress state or the button will show 'Report generated' before commentary exists. `moves` passed are `getMainLine(root)` only (variations not analysed).
- **src/utils/treeReducer.ts:8-14 TreeState + :77-108 defaultTree + :196-198 ReportState; src/state/store/tree.ts:496-502 setReportInProgress**: Add a game-level field next to `report` (e.g. `commentary: { status; narrative; perMove: Record<path, …> }`) with an action like `setReportInProgress`. It is automatically persisted per tab (zustand persist to sessionStorage) and survives tab switches/duplicates (BoardsPage.tsx:111-133 copies sessionStorage).
  - risk: Not written to PGN or DB unless you also change `getPGN`/`getMoveText` (chess.ts:48-127, 218-306). `defaultTree()` and every literal `TreeState` in src/utils/tests/store.test.ts:14-62 must gain the new field. `setState(tree)` (InfoPanel.tsx:179-181 game switching, ImportModal) replaces the whole state so the field must be in `parsePGN` output too (it is, via defaultTree).
- **src/utils/treeReducer.ts:16-28 TreeNode (`clock?: number` precedent at 27) + chess.ts:353-458 innerParsePGN + chess.ts:81-125 getMoveText**: Per-move commentary can either (a) be a new optional TreeNode field (`commentary?: string`) persisted only in session state, or (b) be written into `node.comment` (markdown) via `store.setComment`/direct produce, which makes it render for free in notation (CompleteMoveCell.tsx:200 -> Comment.tsx markdown) and persist through `getMoveText` into PGN `{…}` and through `saveToFile` -> `commands.writeDbGame` (mod.rs:1788-1860, comments re-encoded by Importer).
  - risk: (b) clobbers/merges with user comments and `[%eval]`/`[%clk]` are emitted into the same `{}` block; chess.com `[%timestamp]` is stripped on parse. (a) is invisible to PGN export and to `getGameStats`. Either way `dirty` flips true and autosave (BoardAnalysis.tsx:82-86, `autoSaveAtom` default true) will immediately write files/DB for persistent origins.
- **src/components/panels/analysis/AnalysisPanel.tsx:101-107 (Tabs.List) and :248-269 (panels); src/state/atoms.ts:422-423 currentAnalysisTabAtom**: Add `<Tabs.Tab value="commentary">` + panel; `currentAnalysisTabAtom` is an untyped string atomFamily so no type change. Alternatively add a top-level tab in BoardAnalysis.tsx:213-249 (`currentTabSelectedAtom`) plus a keybind in src/state/keybinds.ts (GENERATE_REPORT at 55 is the precedent).
  - risk: Panels are `keepMounted={false}` (AnalysisPanel uses display:none, BoardAnalysis unmounts) — keep state in atoms/store, not component state. Translation keys must be added to src/translation/en-US.json and extracted (`pnpm i18n:extract`, removeUnusedKeys: true).
- **src/components/home/Databases.tsx:78-120 + src/utils/db.ts:71-97 + src/bindings/generated.ts:115 getPlayersGameInfo**: Recipe to get 'the user's history' from the board page: `sessions = get(sessionsAtom)`; `getDatabases()` filtered by `db.type === 'success' && db.filename.includes(username)` (or title `${username} Lichess|Chess.com`); `query_players(db.file, {name: username, options:{pageSize:1, sort:'id', direction:'asc', skipCount:false}})` -> `player.id`; `commands.getPlayersGameInfo(db.file, player.id)` -> StatsData[]; reuse/export `aggregateOpenings` (OpeningsPanel.tsx:28-59, currently module-private) to get win rates per opening name, and compare against `getOpening(root, position)` for the loaded game. Determine the user's colour by matching `headers.white`/`headers.black` to session usernames (no helper exists; `headers.orientation` is only set for repertoire/puzzle/play tabs).
  - risk: `getPlayersGameInfo` scans every game of the player with parallel opening detection (seconds on large DBs, emits `databaseProgress`), so cache with SWR (`['player-game-info', file, id]` key already used in PlayerCard.tsx:10-16). `StatsData` has no game ids/opponents. Name matching is exact (`query_players` name filter) and case-sensitive on the PGN tag.
- **src/utils/db.ts:168-191 searchPosition + src-tauri/src/db/search.rs:237-462 + src/utils/repertoire.ts:38-72 getDbMoves (usage example)**: 'You have lost 3 games in this line': call `searchPosition({path: <player db3>, fen: node.fen, type:'exact', player: playerId, color: userColor, result: 'any'}, tabId)`; the `PositionStats[]` give, for every continuation played from this position, white/draw/black counts restricted to the player's games (entry `move === '*'` = games that ended here), and `NormalizedGame[]` are sample games (top 500 by elo) you can open with `createTab({… gameOrigin:{kind:'database', database: path, gameId}})`. `wanted_result` can narrow to losses. Also `commands.analyzeGame` novelty detection already answers 'first time this position appears in the reference DB' (chess.rs:615-629).
  - risk: The Rust `db_cache` is a single `Mutex<Option<MmapSearchIndex>>` loaded for whichever file was searched first (search.rs:265-298 never compares the cached index's file); `line_cache` is keyed by (query, file) but computed from the loaded index. DatabasePanel.tsx:200-203 calls `commands.clearGames()` only when the reference DB changes. Querying the player's personal DB while another reference DB is the active index may return results from the wrong database — verify `clear_games` and coordinate with `referenceDbAtom`. First search on a DB generates a search index (slow). Exact-FEN search needs the mainline only (variations ignored by `iter_mainline_move_bytes`).
- **src/utils/lichess/api.tsx:322-346 getLichessGames with `options.player` + src/components/panels/database/options/LichessOptionsPanel.tsx:142-153**: Lichess 'player explorer' (`explorer.lichess.org/player?fen&player&color&speeds…`) gives the user's own move statistics for the current FEN without a local DB; token from `sessions.find(s => s.lichess?.accessToken)` (DatabasePanel.tsx:122-124).
  - risk: Requires Lichess OAuth login (panel shows ExplorerAuthRequired otherwise); network-bound; `convertToNormalized` fetches each sample game individually.
- **src/utils/clock.ts:64-143 getClockInfo + src/utils/treeReducer.ts TreeNode.clock**: Time-pressure commentary: for mainline node i with `clock`, time spent = parent-of-parent (same colour) `.clock` − node.clock + increment (increment via `parseTimeControl(headers.time_control)[0].increment`, currently module-private at clock.ts:39 — export it). `getTimeControl(website, tc)` (timeControl.ts) classifies speed; `headers.site` tells website ('Chess.com' vs 'https://lichess.org/…', see GameInfo.tsx:163-167). Annotation '⊕' (Time Trouble, nag 138) already exists in ANNOTATION_INFO for tagging.
  - risk: `clock` is absent for chess.com link imports (TCN path), for play-mode games without clocks, and possibly for Lichess bulk downloads (see open questions). `clock: 0` is dropped (createNode `clock ? … : undefined`). `white_time_control`/`black_time_control` are written to PGN (chess.ts:196-201) but never parsed back (land in `other`).
- **src/utils/chess.ts:333-351 getOpening / src-tauri/src/opening.rs:67-82 + src/utils/phase.ts:77-97 getGamePhases + src/utils/chess.ts:541-612 getGameStats + src/utils/score.ts**: Deterministic narrative inputs already computed: opening name for any path (longest-matching FEN in built-in ECO table), phase boundaries (ply index of middlegame/endgame as EvalChart.tsx:150-159 does), per-colour ACPL/accuracy/annotation counts, per-move win-chance delta (`getWinChance(normalizeScore(prev))−…`, the same thresholds `getAnnotation` uses: >20 ??, >10 ?, >5 ?!), material (`getMaterialDiff(fen)`), `headers.white_elo/black_elo` for rating gap, `headers.result`.
  - risk: ECO code is not exposed (Rust `Opening._eco` unused; `headers.eco` only for DB games or `headers.other.ECO` for PGN imports). `getGamePhases` needs `Chess[]` built via `positionFromFen` for every mainline node (EvalChart recomputes each render).
- **src/utils/treeReducer.ts:46-54 findFen + src/state/atoms.ts:230 recentFilesAtom + src/components/files/file.ts:34-59 readFileMetadata + commands.readGames**: Repertoire-aware commentary ('you left your prep at move 9; your repertoire has Nf3'): enumerate repertoire files (recentFilesAtom entries with type 'repertoire', or walk the documents dir with processEntriesRecursively and filter metadata.type), `parsePGN(readGames(path,0,0)[0])`, keep those whose `headers.orientation` equals the user's colour, then for each mainline node of the game call `findFen(node.fen, repRoot)`; the first miss after a hit is the deviation, and `getNodeAtPath(repRoot, path).children[0].san` is the prepared move. FSRS stats for that position come from `deckAtomFamily({file, game:0}).positions.find(p => p.fen === fen).card`.
  - risk: findFen compares full FEN strings (including move counters) — transpositions with different halfmove/fullmove numbers will not match; strip counters (`stripClock` chess.ts:656-658 exists for this). Parsing every repertoire on each game load is heavy; cache by file `lastModified`.
- **src/components/settings/SettingsPage.tsx:78-98 SettingCategory/SettingItem + :184+ settings array; src/components/settings/RepertoireMinGamesSetting.tsx; src/state/atoms.ts:161-217 settings atoms**: Add a 'Commentary' category with items: provider select, API key input, model, offline fallback toggle; back them with `atomWithStorage` atoms in atoms.ts (precedent: `sessionsAtom` stores the Lichess `accessToken` in localStorage at 213). Render via a small component like RepertoireMinGamesSetting bound to the atom.
  - risk: localStorage is plaintext; no keyring/stronghold plugin exists in src-tauri/Cargo.toml (only reqwest, tauri plugins). Outbound HTTP from the webview uses `@tauri-apps/plugin-http` fetch with `apiHeaders()` (src/utils/http.ts, not read) and Tauri capabilities may need the LLM host allow-listed (not verified). Telemetry (posthog, `telemetryEnabledAtom` default true) must not capture prompts.
- **src/components/tabs/BoardsPage.tsx:47-74 closeTab (killEngines + abortGame)**: If commentary generation is a long-running per-tab job keyed by tab id (like `report_${tab}`), cancel it here (and in `commands.cancelAnalysis` style if implemented in Rust).
  - risk: No generic per-tab teardown hook exists; adding one touches BoardsPage.
- **src-tauri/src/main.rs:111+ collect_commands! + #[tauri::command] #[specta::specta] pattern (e.g. mod.rs:1378-1385)**: If any of the commentary pipeline lands in Rust (e.g. a deterministic feature extractor over the move blob, or a provider proxy to keep keys out of the webview), add a command, derive `specta::Type` on structs, register in collect_commands!, and regenerate src/bindings/generated.ts.
  - risk: Bindings are generated artifacts; mismatched specta versions (^2.0.0-rc.20) can break generation. Maintainers keep frontend-heavy logic in TS (score/annotation/phase are TS).

## Conventions
- State: jotai atoms in src/state/atoms.ts (`atomWithStorage` for settings, `atomFamily((_tab: string) => atom(...))` + `tabValue(family)` for per-tab UI state, e.g. atoms.ts:356-444); game data in zustand store via `useStore(store, selector)` with `store = useContext(TreeStateContext)!` (every panel, e.g. Clock.tsx:21-25); immer `produce` inside `set` for mutations (tree.ts).
- Persistence tiers: tabs/activeTab/tree stores/database-view -> sessionStorage (zod-validated via createZodStorage, src/state/utils.ts); settings/sessions/recent files/decks -> localStorage; engines -> engines/engines.json file storage (atoms.ts:130-159).
- Tauri commands via generated `commands.*` returning `Result<T, string>`; unwrap with `unwrap(await commands.x())` (src/utils/unwrap.tsx) or branch on `.status === 'ok'`; events via `events.*.listen` (progressEvent, databaseProgress, bestMovesPayload).
- Data fetching with SWR (`useSWR`/`useSWRImmutable`) keyed by arrays, e.g. `['player-game-info', file, id]` (PlayerCard.tsx:10-16), `['personalInfo', name, databases]` (Databases.tsx:90).
- i18n: every UI string is `t("Namespace.Key")` with flat dotted keys (keySeparator:false) in src/translation/<locale>.json; add to en-US.json and run `pnpm i18n:extract` (i18next-cli, removeUnusedKeys:true, preservePatterns for Annotate.*/Errors.* etc.). Example keys: Board.Analysis.GenerateReport, Board.Tabs.Analysis, Annotate.Blunder.
- Validation: zod schemas colocated with types (tabs.ts, explorer.ts, engines.ts, chess.com/api.tsx); external JSON parsed with `safeParse` and user notified via `notifications.show` + `error()` from @tauri-apps/plugin-log.
- UI: Mantine v8 components, @tabler/icons-react, `match()` from ts-pattern for discriminated unions, `memo()` on heavy components with custom equality (ReportPanel GameStats, CompleteMoveCell).
- Testing: vitest (`pnpm test`) with tests in src/utils/tests/*.test.ts using full literal TreeState fixtures (store.test.ts) and pure-function tests (chess.test.ts, score.test.ts); Rust unit tests inline `#[cfg(test)]` (opening.rs:171-182, encoding.rs:384+).
- Formatting/lint: oxfmt + oxlint + tsgo (`pnpm lint`, `pnpm format`); observed indentation is 4 spaces in src/utils + src/state + src/bindings and 2 spaces in src/components (follow the file you touch). CONTRIBUTING.md: build with `pnpm build`, run format/lint:fix/i18n:extract before PR, discuss on issues/Discord first.
- Chess types: chessops (`Move`, `parseUci`, `makeSan`, `positionFromFen` helper in src/utils/chessops.ts); FEN strings as canonical position keys; tree paths as `number[]` child indices; `halfMoves % 2 === 1` means White just moved.
- Header semantics: `white_elo === 0` means unrated ('-'), undefined means unknown; `headers.site` for online games is the game URL (lichess) or 'Chess.com'; dates 'YYYY.MM.DD'.
- Settings UI: SettingsPage builds `SettingItem[]` ({title, description, category, render}) with reusable SettingsSwitch/SettingsNumberInput bound to atoms.

## Gotchas
- getPgnHeaders (chess.ts:490-533) does not map ECO, UTCTime/Time, Opening, Termination, WhiteRatingDiff/BlackRatingDiff, WhiteTimeControl/BlackTimeControl — they end up in `headers.other`; `headers.eco`/`headers.time` are only set for games opened from a DB (createTab overrides headers with the NormalizedGame). `white_time_control`/`black_time_control` round-trip via `other` only, so getClockInfo loses per-side TC after reload.
- createTab with `headers` replaces ALL parsed headers (tabs.ts:117-121); the NormalizedGame object (with white_id, moves, ply_count…) is stored verbatim as GameHeaders in sessionStorage.
- TreeNode.clock unit: createNode expects milliseconds and divides by 1000 (treeReducer.ts:127) while innerParsePGN assigns parseComment's seconds directly (chess.ts:400); value 0 is dropped (`clock ? … : undefined`). Semantics = remaining time of the side that just moved.
- chess.com link import (getChesscomGame, api.tsx:159-244) rebuilds the PGN from TCN + pgnHeaders: no clocks, no comments, no evals. Bulk chess.com archive import uses the API `pgn` field (clock comments survive if present) but ignores time_control/rated/rules/ratings JSON fields.
- DB import whitelist (mod.rs:330-384): only White/Black/WhiteElo/BlackElo/TimeControl/ECO/Round/Date|UTCDate/UTCTime/Site/Event/Result/FEN are stored; Opening/Termination/RatingDiff/Variant/Link are lost. Comments/NAGs/variations are embedded in the Moves blob (not the unused `Comments` table) and rendered back by decode_game_to_movetext.
- get_players_game_info excludes games with a FEN tag (`games::fen.is_null()`), without elo for the player, without result/date/site; opening detection stops at ply 55; StatsData lacks game id/opponent/opponent elo; `site` is 'Lichess' only when the Site tag starts with https://lichess.org/.
- Rust `db_cache` holds one mmap search index regardless of which .db3 `search_position` is called with (search.rs:265-298 checks only `is_none()`); `commands.clearGames()` is called by DatabasePanel when the reference DB changes. Searching a different DB (e.g. the player's own) without clearing may use the wrong index. `line_cache` results are kept until clearGames. First search generates the index (slow); `new_request` semaphore allows 2 concurrent.
- addAnalysis (tree.ts:693-809) does not store `BestMoves` lines, `depth`, or multipv on nodes — only score, annotations, 'N', and inserted variation nodes for ?!/?/??; `TreeNode.depth` stays null. The raw MoveAnalysis[] exists only inside ReportModal.analyze().
- `report.inProgress` lives in the persisted TreeState; `setInProgress(false)` runs in `.finally` of analyzeGame — an added async stage must manage its own flag.
- Per-tab atoms created with `tabValue()` throw 'No tab selected' when no current tab and always target the ACTIVE tab; background work for another tab must use the atomFamily with an explicit tab id (EvalListener uses `activeTab`).
- Tabs panels are keepMounted={false}; TreeStateProvider recreates the store from sessionStorage on remount; sessionStorage writes are debounced 300 ms (closeTab reads `sessionStorage.getItem(value)` directly, BoardsPage.tsx:51, so a very recent change may not be flushed).
- Autosave (autoSaveAtom default true) writes to file/DB whenever `dirty` is true for persistent origins (BoardAnalysis.tsx:82-86) — any programmatic node mutation (setComment/setScore/addAnalysis) triggers disk/DB writes immediately; DB write-back (write_db_game) re-imports the PGN through the header whitelist, dropping `other` tags.
- Session username vs PGN name linking is by exact name: Databases.tsx matches DB files by `filename.includes(username)` and players by `query_players({name})`; there is no helper to find which side of a loaded game is the user; `headers.orientation` is user colour only for repertoire/puzzle/play tabs.
- PersonalPlayerCard requires `DatabaseViewStateContext` (PersonalCard.tsx:37-39) — reusing it on the board page needs `<DatabaseViewStateContext.Provider value={activeDatabaseViewStore}>`; `players.activeTab` is typed 'overview'|'openings' but the card also uses 'ratings' (cast).
- WebsiteAccountSelector only matches `s.player === playerName` (not bare usernames) while Databases/Accounts fall back to usernames — selectors can be empty for sessions without an alias.
- Lichess `accessToken` is persisted in plaintext in localStorage (`sessionsAtom`); an API key stored the same way is equally exposed. posthog telemetry is on by default.
- Explorer (lichess all/masters) in DatabasePanel requires a Lichess token (`missingExplorerToken`), so Lichess per-player stats are only available to logged-in users.
- Opening lookup returns a name only; `getOpeningFromFens` returns Error::NoOpeningFound → `getOpening` returns '' (chess.ts:345-350). ECO code not exposed by any command.
- `headers.result` is auto-updated by makeMove on checkmate/stalemate/insufficient material/3-fold/50-move when changeHeaders (tree.ts:559-572) — a commentary 'game over' detector should rely on chessops `pos.isEnd()` rather than headers.
- Translation keys are pruned by i18next-cli (`removeUnusedKeys: true`) unless matched by preservePatterns — dynamically built keys (`t(\`Annotate.${key}\`)`) must be covered by a pattern.

## Open questions
- Lichess bulk download (`downloadLichess`, lichess/api.tsx:392-394) requests `/api/games/user/{p}?perfType=…&rated=true&sort=dateAsc` with no `clocks`/`evals`/`opening` parameters; `commands.downloadFile` (src-tauri/src/fs.rs:23-32) was only skimmed and does not visibly add them. Per Lichess API docs those default to false for this endpoint (true for `/game/export/{id}` used by link import), which would mean bulk-imported Lichess games have NO [%clk]/[%eval] comments — needs verification against the API docs / a sample DB.
- Whether chess.com monthly-archive `pgn` strings contain `[%clk …]` comments (expected from chess.com behaviour, suggested by the `[%timestamp]` strip at chess.ts:404, but not verifiable from code).
- `commands.clearGames` / Rust `clear_games` (mod.rs:1899-1905) body not read — assumed to reset `db_cache` and `line_cache`. Confirms whether searching a second .db3 is safe.
- `src/utils/http.ts` `apiHeaders()` and Tauri capability/CSP config (src-tauri/capabilities, tauri.conf.json) were not read — need to confirm an LLM API host can be reached from the webview via @tauri-apps/plugin-http, or whether a Rust-side proxy command is required.
- No secure secret storage found (Cargo.toml has no keyring/stronghold/tauri-plugin-store); only pattern is plaintext atomWithStorage. Decide where an API key lives.
- How en-US.json values are populated given `defaultValue: null` in i18next.config.ts (likely `t('Key', { defaultValue })` or manual edit of en-US.json) — not confirmed.
- `StatsData` per-game `time_control` for chess.com daily games is '1/86400'-style; `getTimeControl` handles it, but Lichess correspondence '-' vs chess.com is distinguished only by the `website` string passed in — confirm what `SiteStatsData.site` is for chess.com DBs ('Chess.com' from the PGN Site tag, assumed).
- Whether `src/components/files/FilesPage.tsx`/DirectoryTree expose a reusable list of repertoire files (only `recentFilesAtom` and `processEntriesRecursively` were found).
- `Score.wdl` producers: addAnalysis copies engine `best[0].score` (wdl may be null unless the engine reports WDL); EvalChart interprets wdl as permille. Not verified which engines populate it.
- Puzzle tabs: `puzzles.ts` Puzzle has `themes?`/`completion`/`timeSpent` in the frontend type but the DB `Puzzle` lacks them — how per-puzzle time/completion is tracked (Puzzles.tsx) was not read in full.
- `ProgressButton`/`useProgress` (src/components/common/ProgressButton.tsx, src/hooks/useProgress.ts) semantics for a two-stage job were not read.
- Rust `is_position_in_db` (search.rs:464+) uses the same single db_cache; novelty detection during a report therefore depends on which index is loaded.
