# Application state (Jotai atoms + Zustand stores), settings UI, persistence (localStorage / sessionStorage / AppData files), secrets (lichess OAuth token), outbound HTTP (tauri-plugin-http from TS, reqwest from Rust), Tauri capabilities/CSP, notifications/logging, feature toggles, and PostHog telemetry. Repo root for all relative paths below: /tmp/claude-0/-home-user-Chess/095b4a5e-49ff-5938-b919-e82f3408c9dc/scratchpad/upstream/en-croissant (v0.15.1, Cargo.toml:3 / package.json:3).

## Summary
SETTINGS/STATE. Every user setting is a Jotai atom declared in src/state/atoms.ts with `atomWithStorage(key, default, storage?, options?)` from jotai/utils. When the 3rd arg is omitted (the overwhelming majority, e.g. atoms.ts:163-217) jotai's default storage is used: plain JSON in the WebView's `localStorage` under the given key, with no schema validation. Variants: `createJSONStorage(() => sessionStorage)` for per-session UI state (tabs atoms.ts:58-74, database-conversion-state :254, database-view :267); `createZodStorage(schema, localStorage|sessionStorage)` (src/state/utils.ts:29-59) which `schema.parse`s on read, writes back the normalized value if it differs, and on failure logs `warn` via @tauri-apps/plugin-log and resets to the default; `createAsyncZodStorage(schema, AsyncStringStorage)` (utils.ts:61-95, safeParse, 4-space pretty JSON) used only by `enginesAtom` (atoms.ts:153-159) which is file-backed at `<enginesDir>/engines.json` through `enginesFileStorage` (atoms.ts:130-151, @tauri-apps/plugin-fs readTextFile/writeTextFile) and wrapped in `unwrap()` so it reads as `undefined` until loaded. `fileStorage` (utils.ts:12-27, BaseDirectory.AppData) is exported but unused anywhere. There is NO migration framework: migration is (a) key rename with suffix (`"show-coordinates-v2"` atoms.ts:192), (b) tolerant zod schemas (`.nullish()`, `.default(() => crypto.randomUUID())` in src/utils/engines.ts:37-70; `zodArray` atoms.ts:36-45 drops invalid array items instead of failing), (c) reset-to-default on parse failure, (d) keybinds additive merge of missing defaults (src/state/keybinds.ts:62-88), (e) Help > "Clear saved data" doing `localStorage.clear(); sessionStorage.clear()` (src/routes/__root.tsx:284-297). `getOnInit: true` is set on atoms that are read outside React via `getDefaultStore().get()` before mount (telemetryEnabledAtom atoms.ts:215, dir atoms :98-109) because without it jotai returns the default until the atom mounts. src/utils/directories.ts:4-14 even bypasses the atom and reads `localStorage.getItem("databases-dir")` directly. Zustand is used for the per-tab game tree (src/state/store/tree.ts:518-520, `persist` with `createDebouncedSessionStorage()` from src/state/store/debouncedStorage.ts: 300 ms batched writes to sessionStorage, flushed on beforeunload/pagehide) and the database view (src/state/store/database.ts:92-200, sessionStorage). Per-tab Jotai state uses `atomFamily` + the `tabValue()` helper (atoms.ts:310-329). A component-local persisted atom precedent exists: `reportSettingsAtom = atomWithStorage("report-settings", {...})` in src/components/panels/analysis/ReportModal.tsx:13-19.

SETTINGS UI. src/components/settings/SettingsPage.tsx is a single page: `SettingCategory` union (:78-87), `SettingItem {id, category, title, description, keywords?, render}` (:89-96), a `useMemo` array of items (:184-711) whose `render` returns `<SettingsSwitch atom={...}/>` (src/components/settings/SettingsSwitch.tsx, takes `PrimitiveAtom<boolean>`), an inline Mantine `<Select>`, or a custom component; `categoryInfo` (:715-767) with `t("Settings.<Cat>")`/`t("Settings.<Cat>.Desc")` and a @tabler icon; vertical `Tabs.List` (:877-905) and one `Tabs.Panel` per category (:909-1023) calling `renderCategorySettings(cat)`. Search (:769-780) matches title/description/category/id/keywords. `TelemetrySwitch` (:128-151) is the precedent for a toggle with side effects. i18n keys are flat dotted strings (`keySeparator: false`, i18next.config.ts:27) living under `"translation"` in src/translation/<locale>.json; new keys must be extracted with `pnpm i18n:extract` and CI's `pnpm lint:ci` runs `i18next-cli extract --ci` (package.json:15, .github/workflows/test.yml:33) which fails if the JSON is stale; `removeUnusedKeys: true` and `preservePatterns` (i18next.config.ts:33-41) matter for dynamically-built keys.

SECRETS. The only secret today is the lichess OAuth token. Flow: src/components/home/Accounts.tsx:91-97 stages an alias in sessionStorage then calls `commands.authenticate(username)`; Rust src-tauri/src/oauth.rs:61-78 builds a PKCE auth URL (client_id "org.encroissant.app", scope `preference:read`, redirect to a random `127.0.0.1` port served by axum) and opens the browser; the callback (oauth.rs:86-109) checks CSRF, exchanges the code (with `.unwrap()`, i.e. a panic path), and `app.emit("access_token", token)` to the whole webview; Accounts.tsx:99-110 `listen<string>("access_token")` → `addLichessSession(p, {accessToken: token, username, account})` → `setSessions` on `sessionsAtom = atomWithStorage<Session[]>("sessions", [])` (atoms.ts:213) → default storage → plaintext JSON in `localStorage["sessions"]`. The token is then used as `Authorization: Bearer` from TS (src/utils/lichess/api.tsx:219,337,358,375) and passed to Rust `download_file(token: Option<String>)` (src-tauri/src/fs.rs:39-43). Chess.com has no auth (public API only; `ChessComSession {username, stats}`). There is no encryption, no OS keychain, no tauri-plugin-store/stronghold and no `keyring` crate (Cargo.toml:15-68; `security-framework` in Cargo.lock is only a TLS transitive dep of native-tls/rustls-native-certs/rustls-platform-verifier).

HTTP, STREAMING, ERRORS, FLAGS, TELEMETRY. All frontend HTTP uses `import { fetch } from "@tauri-apps/plugin-http"` (5 files: src/utils/lichess/api.tsx:5, chess.com/api.tsx:5, chessdb/api.ts:1, engines.ts:1, db.ts:3) with `apiHeaders()` from src/utils/http.ts:17-22 adding a `User-Agent: EnCroissant/<version> (<repo>)` built by `initUserAgent()` at startup (src/App.tsx:157). No wrapper client, ad hoc `if (!res.ok) throw` or `notifications.show`. plugin-http requests are gated by src-tauri/capabilities/main.json:42-53 (`http:default` with an `allow` list of 7 URL globs); a new host like api.anthropic.com MUST be added there, and a user-configurable base URL cannot be allow-listed at runtime. CSP is `null` (src-tauri/tauri.conf.json:84), so the webview's native fetch is not restricted (posthog-js already talks to app.posthog.com this way, outside the allow-list). Rust uses `reqwest 0.12.5` with `features = ["stream","blocking","json"]` (Cargo.toml:23); `download_file` (fs.rs:21-86) is the precedent: Bearer header, `res.bytes_stream()`, per-chunk progress via `update_progress` → `ProgressEvent` (tauri-specta event, main.rs:173-180). Nothing in TS consumes a streamed body (no ReadableStream/getReader/TextDecoder/EventSource anywhere) and `tauri::ipc::Channel` is unused. Errors: Rust `Error` enum (src-tauri/src/error.rs:4-95, thiserror, boxed sources) serializes as a display string (:187-194) and reaches TS as `Result<T, string>` from tauri-specta bindings; `unwrap()` (src/utils/unwrap.tsx:7-19) logs, shows a red Mantine notification and throws. `<Notifications/>` is mounted in src/App.tsx:286; the pattern is `notifications.show({title, message, color: "red", icon: <IconX/>})` with mostly untranslated English strings (only src/components/boards/Board.tsx:206-210 uses `t()`). There is no feature-flag system; the nearest patterns are boolean `atomWithStorage` + `SettingsSwitch` (the "anarchy" category holds `forcedEnPassantAtom`), build-time `import.meta.env.VITE_PLATFORM` (vite.config.ts:51-53) and Rust `#[cfg(debug_assertions)]`. Telemetry: posthog-js is always initialised (src/App.tsx:167-173; autocapture/pageview/pageleave/session recording all off) and the ONLY event is `posthog.capture("app_started", {version})` gated by `telemetryEnabledAtom` ("telemetry-enabled", default `true`, `getOnInit: true`, atoms.ts:215-217); the settings switch (SettingsPage.tsx:128-151) calls `posthog.opt_in_capturing()`/`opt_out_capturing()`.

## Key files
- `src/state/atoms.ts` — All global Jotai state incl. every persisted setting; storage helpers for engines file; per-tab atom families
  - zodArray helper :36-45 (drops invalid items)
  - tabsAtom 'tabs' sessionStorage+zod :58-62
  - activeTabAtom 'activeTab' sessionStorage :64-68
  - expandedDirectoriesAtom 'expanded-directories' sessionStorage+zod :70-74
  - currentTabAtom :76-95
  - storedDocumentDirAtom 'document-dir' getOnInit :98
  - storedDatabasesDirAtom 'databases-dir' getOnInit :101
  - storedEnginesDirAtom 'engines-dir' getOnInit :104
  - storedPuzzlesDirAtom 'puzzles-dir' getOnInit :107
  - enginesFileStorage (AsyncStringStorage over <enginesDir>/engines.json) :130-151
  - enginesAtom = unwrap(atomWithStorage('engines/engines.json', [], createAsyncZodStorage(zodArray(engineSchema), enginesFileStorage))) :153-159
  - tableViewAtom 'table-view' :163
  - fontSizeAtom 'font-size' :165-168
  - moveNotationTypeAtom key 'letters' :170
  - moveMethodAtom 'move-method' :171
  - spellCheckAtom 'spell-check' :172
  - moveInputAtom 'move-input' :173
  - showDestsAtom 'show-dests' :174
  - moveHighlightAtom 'move-highlight' :175
  - snapArrowsAtom 'snap-dests' :176
  - showArrowsAtom 'show-arrows' :177
  - showConsecutiveArrowsAtom 'show-consecutive-arrows' :178
  - showVariationArrowsAtom 'show-variation-arrows' :179
  - eraseDrawablesOnClickAtom 'erase-drawables-on-click' :180-183
  - autoPromoteAtom 'auto-promote' :184
  - autoSaveAtom 'auto-save' :185
  - previewBoardOnHoverAtom 'preview-board-on-hover' :186
  - flipBoardAfterMoveAtom 'flip-board-after-move' :187
  - enableBoardScrollAtom 'board-scroll' :188
  - materialDisplayAtom 'material-display' :189
  - forcedEnPassantAtom 'forced-ep' :190
  - showCoordinatesAtom 'show-coordinates-v2' getOnInit :191-196
  - soundCollectionAtom 'sound-collection' getOnInit :197-204
  - soundVolumeAtom 'sound-volume' getOnInit :206-208
  - pieceSetAtom 'piece-set' :210
  - boardImageAtom 'board-image' :211
  - primaryColorAtom 'mantine-primary-color' :212
  - sessionsAtom 'sessions' (Session[] incl. lichess accessToken, plaintext) :213
  - nativeBarAtom 'native-bar' :214
  - telemetryEnabledAtom 'telemetry-enabled' default true getOnInit :215-217
  - recentFilesAtom 'recent-files' :230 + addRecentFileAtom :232-237
  - referenceDbAtom 'reference-database' :241
  - selectedPuzzleDbAtom 'puzzle-db' :243
  - databaseConversionStateAtom 'database-conversion-state' sessionStorage :254-265
  - selectedDatabaseAtom 'database-view' sessionStorage :267-271
  - gameInputColorAtom 'game-input-color' :277
  - gamePlayer1SettingsAtom/gamePlayer2SettingsAtom 'game-player1-settings'/'game-player2-settings' :286-294
  - gameSameTimeControlAtom 'game-same-time-control' :296
  - gameOpeningBookPathAtom 'game-opening-book-path' :298
  - gameOpeningBookEnabledAtom 'game-opening-book-enabled' :303
  - gameOpeningBookMaxPlyAtom 'game-opening-book-max-ply' :308
  - tabValue() per-tab helper :310-329
  - hidePuzzleRatingAtom 'hide-puzzle-rating' :332
  - progressivePuzzlesAtom 'progressive-puzzles' :333
  - jumpToNextPuzzleAtom 'puzzle-jump-immediately' :334
  - trackPuzzleTimeAtom 'track-puzzle-time' :335
  - puzzleRatingRangeAtom 'puzzle-ratings' :336-339
  - puzzleThemeAtom 'puzzle-theme' :341
  - coverageMinGamesAtom 'coverage-min-games' :343
  - reportTypeAtom (non-persisted 'CP'|'WDL') :350
  - per-tab families: threat :356, evalOpen :359, evalBarDisplay :362, invisible :365, showComments :368, showVariations :371, tabFamily 'info' :374, reportModalOpen :377, localOptions :380, dbType :414, dbTab :419, analysisTab default 'engines' :422, practiceTab :425, expandedEngines :428, pgnOptions :433, currentPuzzle :443
  - lichessOptionsAtom 'lichess-all-options' zod localStorage getOnInit :392-403
  - masterOptionsAtom 'lichess-master-options' zod localStorage getOnInit :405-412
  - currentDetachedEngineAtom 'detached-engine' :431
  - deckAtomFamily `deck-${file}-${game}` zod localStorage :481-493
  - practiceAutoDifficultyAtom 'practice-auto-difficulty' :536-539
  - engineMovesFamily / engineProgressFamily :544-553
  - bestMovesFamily / firstEngineWithLinesFamily :556-634
  - tabEngineSettingsFamily :636-661
  - allEnabledAtom / enableAllAtom :663-695
- `src/state/utils.ts` — Storage adapters: AppData file storage (unused), sync and async zod-validated storage wrappers
  - fileStorage: AsyncStringStorage (BaseDirectory.AppData; EXPORTED BUT UNUSED) :12-27
  - createZodStorage<Value>(schema, storage: SyncStringStorage): SyncStorage<Value> :29-59 (parse, write-back normalized, warn+reset on failure)
  - createAsyncZodStorage<Input,Output>(schema, storage: AsyncStringStorage): AsyncStorage<Output> :61-95 (safeParse, JSON.stringify(value,null,4))
- `src/state/store/debouncedStorage.ts` — Zustand PersistStorage that batches sessionStorage writes (300 ms), flushes on beforeunload/pagehide; used by the per-tab tree store
  - DEBOUNCE_MS = 300 :3
  - pendingWrites Map :4
  - flush() :9-19
  - bindFlushHandlers() :32-50
  - createDebouncedSessionStorage<S>(delay = DEBOUNCE_MS): PersistStorage<S> :52-74
- `src/state/store/tree.ts` — Zustand per-tab game tree store (where move comments/annotations live); persisted per tab id in sessionStorage
  - TreeStoreState interface :26+ (addAnalysis used by ReportModal)
  - persist(stateCreator, { name: id, storage: createDebouncedSessionStorage<TreeStoreState>() }) :518-520
- `src/state/store/database.ts` — Zustand database-view store persisted in sessionStorage
  - activeDatabaseViewStore = createStore()(persist(..., { name: 'database-view', storage: createJSONStorage(() => sessionStorage) })) :92-200
  - useActiveDatabaseViewStore :202-206
- `src/state/keybinds.ts` — Persisted keymap with additive-default migration storage
  - keys: KeyMap defaults :14-58 (GENERATE_REPORT: f12 :55)
  - keyMapAtom = atomWithStorage('keybinds', keys, defaultStorage(keys, localStorage)) :60
  - defaultStorage(defaults, storage): SyncStorage<KeyMap> :62-88 (fills missing keys from defaults on read)
- `src/components/settings/SettingsPage.tsx` — The whole settings page: category union, item registry, search, tabs, panels; TelemetrySwitch
  - type SettingCategory :78-87
  - interface SettingItem :89-96
  - SettingRow({title, description, children, highlight?}) :98-126
  - TelemetrySwitch (posthog.opt_in_capturing/opt_out_capturing) :128-151
  - Page() default export :153-975
  - useLoaderData({ from: '/settings' }) -> dirs, version :160-168
  - settings: SettingItem[] = useMemo(...) :184-652 (deps array :628-651)
  - platform-conditional item via import.meta.env.VITE_PLATFORM :426-453
  - privacy/telemetry item :619-626
  - categoryInfo record :656-708
  - filteredSettings search :710-721
  - renderCategorySettings(category) :769-776
  - Tabs.List :818-846
  - Tabs.Panel per category :850-964
  - version footer :967-969
- `src/components/settings/SettingsSwitch.tsx` — Reusable boolean setting control bound to a PrimitiveAtom<boolean>
  - SettingsSwitch({ atom }: { atom: PrimitiveAtom<boolean> }) :5-20
- `src/components/settings/SettingsNumberInput.tsx` — Reusable numeric setting control bound to a PrimitiveAtom<number>
  - SettingsNumberInput({ atom, min, max?, step? }) :4-25
- `src/components/settings/RepertoireMinGamesSetting.tsx` — Precedent for a composite setting (preset Select + custom NumberInput) backed by one atom
  - RepertoireMinGamesSetting :7-64 (coverageMinGamesAtom)
- `src/routes/settings.tsx` — TanStack route for /settings; loader supplies dirs + app version to the page
  - Route = createFileRoute('/settings')({ component: SettingsPage, loader: async ({context:{loadDirs}}) => ({ dirs, version }) }) :5-11
- `src/utils/session.ts` — Account session types persisted in sessionsAtom (localStorage 'sessions')
  - LichessSession { accessToken?: string; username: string; account: LichessAccount } :4-8
  - ChessComSession { username: string; stats: ChessComStats } :10-13
  - Session { lichess?: LichessSession; chessCom?: ChessComSession; player?: string; updatedAt: number } :15-20
- `src/components/home/Accounts.tsx` — Where the lichess OAuth token is received from Rust and written into sessionsAtom
  - addLichessSession(alias, session: LichessSession) :51-63
  - onLichessAuthentication(token) :81-89 (reads/removes sessionStorage 'lichess_player_alias')
  - addLichess(player, username, withLogin) -> commands.authenticate(username) :91-97
  - listen<string>('access_token', ...) :99-110
- `src/components/common/AccountCards.tsx` — Reads lichessSession.accessToken for account refresh / download
  - token={lichessSession.accessToken} :217
  - getLichessAccount({ token, username }) :228-231
  - re-persists accessToken on reload :241
- `src/components/panels/database/DatabasePanel.tsx` — Picks the first session that has a lichess accessToken for explorer calls
  - explorerToken = sessions.find((s) => s.lichess?.accessToken)?.lichess?.accessToken :122-123
- `src/utils/http.ts` — The only shared HTTP helper: builds User-Agent header
  - APP_NAME/APP_REPO :3-4
  - initUserAgent(): Promise<void> :8-15 (called in App.tsx:157)
  - apiHeaders(extra?: Record<string,string>): Record<string,string> :17-22
- `src/utils/lichess/api.tsx` — All lichess HTTP via plugin-http fetch + apiHeaders; Bearer token usage; notification on failure
  - baseURL/explorerURL/tablebaseURL :30-32
  - LichessAccount type :82-148
  - getLichessAccount({ token?, username? }) :208-236 (notifications.show on !ok :227-232)
  - getBestMoves (lichess cloud-eval as a 'remote engine') :238-287
  - getCloudEvaluation cache Map :289-320
  - getLichessGames/getMasterGames/getPlayerGames with Authorization: Bearer :322-383
  - downloadLichess -> commands.downloadFile(..., token ?? null, ...) :385-406
  - getLichessGame :408-414
  - getTablebaseInfo :416-424
  - getFidePlayer :426-448
- `src/utils/lichess/explorer.ts` — Zod schemas for persisted lichess explorer options (precedent for zod-typed settings)
  - lichessGamesOptionsSchema :28-54
  - LichessGamesOptions = z.infer :56
  - masterOptionsSchema :58-64
  - getLichessGamesQueryParams/getMasterGamesQueryParams :68-120
- `src/utils/chess.com/api.tsx` — Chess.com public API via plugin-http; zod-validated responses; notification pattern
  - baseURL = 'https://api.chess.com' :16
  - ChessComStatsSchema / ChessComStats :31-37
  - getChessComAccount :66-94 (notifications.show :71-76, :85-90)
  - downloadChessCom (events.progressEvent.emit from TS) :102-150
  - getChesscomGame (www.chess.com/callback) :159-244
- `src/utils/unwrap.tsx` — Standard consumer of tauri-specta Result: log + red notification + throw
  - type Result<T,E> = { status: 'ok'; data: T } | { status: 'error'; error: E } :5
  - unwrap<T>(result: Result<T,string>): T :7-19
- `src/App.tsx` — Root providers (MantineProvider, Notifications, Router), startup sequence, PostHog init and the single telemetry capture
  - colorSchemeManager = localStorageColorSchemeManager({ key: 'mantine-color-scheme' }) :55-57
  - router context loadDirs (seeds dir atoms via getDefaultStore) :75-111
  - checkForUpdates :119-134
  - useAppStartup :146-205
  - initUserAgent() :157
  - posthog.init(key, { api_host: 'https://app.posthog.com', autocapture:false, capture_pageview:false, capture_pageleave:false, disable_session_recording:true }) :167-173
  - if (telemetryEnabled) posthog.capture('app_started', { version }) :175-177
  - <Notifications /> :286
- `src/index.tsx` — i18next init with bundled JSON resources and localStorage language detection
  - i18n.use(LanguageDetector).use(initReactI18next).init({ resources, detection: { order:['localStorage'], caches:['localStorage'] }, fallbackLng:'en-US', returnEmptyString:false }) :27-55
- `i18next.config.ts` — i18next-cli extraction config enforced in CI
  - locales :4-21
  - extract.keySeparator:false, nsSeparator:false :27-28
  - primaryLanguage 'en-US', defaultValue null, removeUnusedKeys true :31-33
  - preservePatterns ['Files.FileType.*','SideBar.*','TimeControl.*','Annotate.*','GoMode.*','Errors.*'] :34-41
  - types output src/i18next.d.ts (not present in clone) :44-48
- `src/translation/en-US.json` — Primary language file; flat dotted keys under 'translation'
  - 'Common.Off'/'Common.On' :206-207
  - 'Errors.*' :313-326
  - 'Settings.Board'/'Settings.Board.Desc' :521-522
  - 'Settings.Privacy', '.Desc', '.Telemetry', '.Telemetry.Desc' :573-576
  - 'Settings.Repertoire' :577, '.Desc' :585
- `src/routes/__root.tsx` — App menu incl. Clear saved data and Open logs notification; native title bar switching
  - Clear saved data: localStorage.clear(); sessionStorage.clear(); location.reload() :284-297
  - notifications.show({ title: 'Logs', message }) :303-306
  - VITE_PLATFORM / isNative decorations :320-330
- `src/components/ErrorComponent.tsx` — Router defaultErrorComponent: shows error + stack, copy button, report-issue links
  - ErrorComponent({ error }: { error: unknown }) :5-56
- `src/utils/engines.ts` — Zod engine schema (richest persisted schema precedent) and plugin-http fetch to encroissant.org
  - goModeSchema :10-26
  - engineSettingsSchema / EngineSettings :28-35
  - localEngineSchema (id: z.string().default(() => crypto.randomUUID()), many .nullish()) :37-51
  - remoteEngineSchema type z.enum(['chessdb','lichess']) :55-65
  - engineSchema = z.union([...]) / Engine :69-70
  - useDefaultEngines (useSWR + fetch) :95-113
- `src/utils/directories.ts` — Reads dir settings straight from localStorage (outside Jotai) and ensures directories exist
  - getStoredDirectory(key): string|null (localStorage.getItem + JSON.parse) :4-14
  - getDatabasesDir/getDocumentDir/getEnginesDir/getPuzzlesDir :23-61
- `src/components/panels/analysis/ReportModal.tsx` — 'Generate report' modal: persisted report settings atom + commands.analyzeGame + addAnalysis into tree store
  - reportSettingsAtom = atomWithStorage('report-settings', { novelty, reversed, variations, goMode, engine }) :13-19
  - useForm({ initialValues: reportSettings, validate }) :49-59
  - analyze(): commands.analyzeGame(`report_${tab}`, engine.path, goMode, { annotateNovelties, fen, referenceDb, reversed, moves }, engineSettings).then(addAnalysis(analysis.data, { showVariations })) :72-104
  - Modal title t('Board.Analysis.GenerateReport') :107-111
- `src-tauri/src/oauth.rs` — Lichess OAuth2 PKCE flow in Rust; token emitted to webview as a global event
  - create_client (ClientId 'org.encroissant.app', https://lichess.org/oauth, /api/token) :17-24
  - AuthState { csrf_token, pkce, client, socket_addr } :34-57
  - #[tauri::command] authenticate(username, state, app) :59-78 (scope 'preference:read', opener.open_url, spawns axum)
  - authorize callback: CSRF check, exchange_code(...).unwrap(), app.emit('access_token', access_token) :86-109
  - run_server :111-121
- `src-tauri/src/fs.rs` — Rust reqwest download with Bearer token and streamed progress; the precedent for Rust-side HTTP
  - download_file(id, url, path, app, state, token: Option<String>, finalize: Option<bool>, total_size: Option<u32>) -> Result<(), Error> :21-86
  - Client::new() + Authorization: Bearer header :35-43
  - res.bytes_stream() loop with update_progress :51-63
  - archive_kind/unzip_file/unpack_tar_gz :88-145
  - set_file_as_executable :147-159, file_exists :161-165, get_file_metadata :172-182
- `src-tauri/src/error.rs` — Single app Error enum (thiserror); serialized as string to the frontend
  - enum Error { Io, Zip, ParseInt, Tauri, TauriOpener, Reqwest(Box<reqwest::Error>), ChessPosition, ..., AnalysisCancelled } :4-95
  - impl From<reqwest::Error> for Error :127-131
  - impl serde::Serialize for Error (serialize_str(to_string())) :187-194
  - impl specta::Type for Error (String) :196-203
- `src-tauri/src/main.rs` — Tauri builder, plugin registration, tauri-specta command/event registry, AppState
  - pub struct AppState { ..., engine_processes, analysis_cancel_flags: DashMap<String, Arc<AtomicBool>>, auth: AuthState, game_manager, progress_state } :76-96
  - tauri_specta::collect_commands!(...) :111-172
  - tauri_specta::collect_events!(BestMovesPayload, DatabaseProgress, ProgressEvent, GameMoveEvent, ClockUpdateEvent, GameOverEvent) :173-180
  - #[cfg(debug_assertions)] export bindings to ../src/bindings/generated.ts :182-188
  - log targets (Stdout + LogDir en-croissant.log in release, LevelFilter::Info) :190-208
  - plugins: window_state, process, log, opener, http, fs, dialog, os, cli, updater :201-241
  - manage(AppState::default()) :247
- `src-tauri/tauri.conf.json` — App config: identifier, updater endpoint, asset protocol, CSP null
  - identifier 'org.encroissant.app' :50
  - plugins.updater.endpoints ['https://www.encroissant.org/updates'] :61-66
  - app.security.assetProtocol.scope ['**'] :78-83
  - app.security.csp: null :84
- `src-tauri/capabilities/main.json` — Permissions for the main window incl. the plugin-http URL allow-list
  - permissions list :8-54 (core:window:*, opener:default + allow-open-path '**', dialog, os, cli, updater, process, log, fs:default, fs:write-all, window-state, fs:scope-appdata-recursive '**')
  - { identifier: 'http:default', allow: ['https://www.encroissant.org/**','https://api.chess.com/**','https://www.chess.com/**','https://lichess.org/**','https://explorer.lichess.org/**','https://tablebase.lichess.org/**','https://www.chessdb.cn/**'] } :42-53
- `src-tauri/capabilities/migrated.json` — Legacy v1-migrated capability
  - local: true, windows ['main'], permissions ['core:default','process:default'] :4-11
- `src-tauri/Cargo.toml` — Rust deps: reqwest, oauth2, axum, tauri plugins; no keyring/stronghold/store
  - reqwest = { version = '0.12.5', features = ['stream','blocking','json'] } :23
  - oauth2 '4.4.2' :49, axum '0.6.20' :50
  - tauri-plugin-http '2.5.7' :56, tauri-plugin-fs '2.4.5' :57, tauri-plugin-log '2.8.0' :63
  - specta / tauri-specta rc.20 :44-45
- `package.json` — Frontend deps and scripts (lint:ci enforces i18n extraction)
  - scripts: lint:ci = 'tsgo --noEmit && oxfmt --check && oxlint --format=github && i18next-cli extract --ci' :15
  - i18n:extract/status/types/sync :17-20
  - @tauri-apps/plugin-http ^2.5.7 :40
  - @mantine/notifications ^8.3.14 :31
  - posthog-js ^1.342.1 :68
  - jotai ^2.18.1 :64, zustand ^5.0.11 :87, zod ^3.22.4 :86, swr ^2.4.0 :82
  - react-markdown ^10.1.0, remark-gfm, rehype-raw :77-81 (markdown rendering available)
  - vitest ^4.1.0 :107
- `vite.config.ts` — Build config; React Compiler enabled; VITE_PLATFORM define
  - babel({ presets: [reactCompilerPreset()] }) :19-21
  - define 'import.meta.env.VITE_PLATFORM' = os.platform() :51-53
  - test.environment 'jsdom' :48-50
- `.editorconfig` — Indentation convention: 2 spaces for .tsx/.jsx, 4 spaces for .ts, 2 for json
  - [*.{jsx,tsx}] indent_size = 2
  - [*.ts] indent_size = 4
- `CONTRIBUTING.md` — Upstream PR process
  - check issues/Discord first
  - branch off master
  - pnpm format, pnpm lint:fix, pnpm i18n:extract before PR
  - pnpm build and manually verify

## Data model
// src/utils/session.ts:4-20 (persisted verbatim in localStorage["sessions"] via sessionsAtom, atoms.ts:213, NO zod)
export type LichessSession = { accessToken?: string; username: string; account: LichessAccount; };
export type ChessComSession = { username: string; stats: ChessComStats; };
export type Session = { lichess?: LichessSession; chessCom?: ChessComSession; player?: string; updatedAt: number; };

// src/state/atoms.ts — declaration forms actually used
export const telemetryEnabledAtom = atomWithStorage<boolean>("telemetry-enabled", true, undefined, { getOnInit: true });   // :215-217
export const sessionsAtom = atomWithStorage<Session[]>("sessions", []);                                                     // :213
export const lichessOptionsAtom = atomWithStorage<LichessGamesOptions>("lichess-all-options", {...}, createZodStorage(lichessGamesOptionsSchema, localStorage), { getOnInit: true }); // :392-403
export const enginesAtom = unwrap(atomWithStorage<Engine[]>("engines/engines.json", [], createAsyncZodStorage(zodArray(engineSchema), enginesFileStorage) as AsyncStorage<Engine[]>)); // :153-159
export const tabsAtom = atomWithStorage<Tab[]>("tabs", [firstTab], createZodStorage(z.array(tabSchema), sessionStorage)); // :58-62

// src/state/utils.ts:29-32, 61-64
export function createZodStorage<Value>(schema: z.ZodType<Value>, storage: SyncStringStorage): SyncStorage<Value>
export function createAsyncZodStorage<Input, Output>(schema: z.ZodType<Output, z.ZodTypeDef, Input>, storage: AsyncStringStorage): AsyncStorage<Output>
export const fileStorage: AsyncStringStorage  // :13, baseDir: BaseDirectory.AppData, unused

// src/state/store/debouncedStorage.ts:52
export function createDebouncedSessionStorage<S>(delay = DEBOUNCE_MS): PersistStorage<S>

// src/components/settings/SettingsPage.tsx:78-96
type SettingCategory = "board" | "inputs" | "anarchy" | "appearance" | "sound" | "keybinds" | "directories" | "repertoire" | "privacy";
interface SettingItem { id: string; category: SettingCategory; title: string; description: string; keywords?: string[]; render: () => React.ReactNode; }
// SettingsSwitch.tsx:5  ({ atom }: { atom: PrimitiveAtom<boolean> })
// SettingsNumberInput.tsx:4-14 ({ atom: PrimitiveAtom<number>; min: number; max?: number; step?: number })

// src/components/panels/analysis/ReportModal.tsx:13-19 (component-local persisted atom, default jotai storage, no zod)
const reportSettingsAtom = atomWithStorage("report-settings", { novelty: true, reversed: true, variations: true, goMode: { t: "Time", c: 500 } as Exclude<GoMode, { t: "Infinite" }>, engine: "" });

// src/utils/http.ts:17-22
export function apiHeaders(extra?: Record<string, string>): Record<string, string>  // => { "User-Agent": userAgent, ...extra }

// src/utils/unwrap.tsx:5-7
type Result<T, E> = { status: "ok"; data: T } | { status: "error"; error: E };
export function unwrap<T>(result: Result<T, string>): T

// src/bindings/generated.ts (tauri-specta generated)
async analyzeGame(id: string, engine: string, goMode: GoMode, options: AnalysisOptions, uciOptions: EngineOption[]) : Promise<Result<MoveAnalysis[], string>>   // :24
async authenticate(username: string) : Promise<Result<null, string>>                                                                                          // :289
async downloadFile(id: string, url: string, path: string, token: string | null, finalize: boolean | null, totalSize: number | null) : Promise<Result<null, string>> // :305

// src-tauri/src/fs.rs:21-32
#[tauri::command] #[specta::specta]
pub async fn download_file(id: String, url: String, path: PathBuf, app: tauri::AppHandle, state: tauri::State<'_, AppState>, token: Option<String>, finalize: Option<bool>, total_size: Option<u32>) -> Result<(), Error>

// src-tauri/src/error.rs:4-22 (excerpt)
#[derive(Debug, thiserror::Error)]
pub enum Error { #[error(transparent)] Io(Box<std::io::Error>), ..., #[error(transparent)] Reqwest(Box<reqwest::Error>), ..., #[error("Analysis cancelled")] AnalysisCancelled }
impl serde::Serialize for Error { ... serializer.serialize_str(self.to_string().as_ref()) }   // :187-194

// src-tauri/src/oauth.rs:34-40
pub struct AuthState { pub csrf_token: CsrfToken, pub pkce: Arc<(PkceCodeChallenge, String)>, pub client: Arc<BasicClient>, pub socket_addr: SocketAddr }

// src-tauri/capabilities/main.json:42-53
{ "identifier": "http:default", "allow": [ "https://www.encroissant.org/**", "https://api.chess.com/**", "https://www.chess.com/**", "https://lichess.org/**", "https://explorer.lichess.org/**", "https://tablebase.lichess.org/**", "https://www.chessdb.cn/**" ] }

// src-tauri/tauri.conf.json:77-85
"security": { "assetProtocol": { "scope": ["**"], "enable": true }, "csp": null }

// src/App.tsx:167-177
posthog.init("phc_kgEBtifs0EgWlrl4ROYEbnsQ1b7BS2W5BKLNyXe7f8z", { api_host: "https://app.posthog.com", autocapture: false, capture_pageview: false, capture_pageleave: false, disable_session_recording: true });
if (telemetryEnabled) { posthog.capture("app_started", { version: await getVersion() }); }

// src/translation/en-US.json shape (flat keys under "translation"):
{ "translation": { "Settings.Privacy": "Privacy", "Settings.Privacy.Desc": "Privacy and data collection settings", "Settings.Privacy.Telemetry": "Anonymous Telemetry", "Settings.Privacy.Telemetry.Desc": "Help improve En Croissant by sending anonymous usage data", ... } }

## Extension points
- **src/state/atoms.ts:161-217 ("// Settings" block, next to telemetryEnabledAtom)**: Declare the commentary config as atomWithStorage with createZodStorage(commentarySettingsSchema, localStorage) and { getOnInit: true } (so non-React services can getDefaultStore().get() it at call time, exactly like telemetryEnabledAtom). Keep the feature toggle (`commentaryEnabledAtom: atomWithStorage<boolean>`) separate from provider config (provider, model, baseUrl) so SettingsSwitch can bind to it. Use explicit prefixed keys (e.g. "commentary-enabled", "commentary-provider"). Put the zod schema next to the provider types in a new src/utils/commentary/ module the way lichessGamesOptionsSchema lives in src/utils/lichess/explorer.ts:28-54.
  - risk: Default jotai storage is plaintext localStorage; putting the API key in the same atom persists it in cleartext (same as the lichess token today). Jotai's atomWithStorage replaces the whole stored object (no deep merge), so adding fields later leaves existing users with `undefined` unless the zod schema has `.default()`/`.catch()` for each field (see engineSchema engines.ts:37-51). Key names in this file are not always descriptive (moveNotationTypeAtom key is "letters", atoms.ts:170); "database-view" is used by both selectedDatabaseAtom (atoms.ts:267) and the zustand store (database.ts:196) — avoid collisions.
- **src/state/utils.ts:12-27 fileStorage (unused) + createAsyncZodStorage :61-95, and atoms.ts:130-159 enginesFileStorage/enginesAtom**: For anything larger than a toggle (a per-game commentary cache, or provider config you want in a human-editable file), replicate the enginesAtom pattern: `unwrap(atomWithStorage("commentary/config.json", default, createAsyncZodStorage(schema, fileStorage)))`. fileStorage already targets BaseDirectory.AppData and `fs:scope-appdata-recursive` is granted (capabilities/main.json:34-41).
  - risk: unwrap() makes the atom read `undefined` until the async read resolves — every consumer must handle it (ReportModal.tsx:41 does `engines ?? []`). The maintainers may ask why fileStorage was dormant; it is exported but has zero call sites.
- **src-tauri/src/main.rs:111-172 collect_commands! (+ a new src-tauri/src/secrets.rs) and src-tauri/src/error.rs:4-95**: If the API key should NOT live in localStorage, add Rust commands `set_secret(service, key)` / `get_secret(service)` / `delete_secret(service)` backed by the `keyring` crate (identifier "org.encroissant.app", tauri.conf.json:50), register them with `#[tauri::command] #[specta::specta]` + collect_commands!, add an `Error::Keyring(Box<keyring::Error>)` variant and `From` impl, and keep only a boolean `commentary-api-key-set` marker in Jotai. Bindings regenerate automatically in debug builds (main.rs:182-188).
  - risk: New native dependency (keyring → secret-service/dbus on Linux, Credential Manager on Windows, Keychain on macOS) increases build surface and may be pushed back on by upstream; AppImage/Flatpak Linux builds may lack a secret service at runtime, so a localStorage fallback with a visible warning is needed. There is currently zero keychain code to copy from.
- **src-tauri/capabilities/main.json:42-53 http:default allow list**: To call a provider from TS with `fetch` from @tauri-apps/plugin-http, append "https://api.anthropic.com/**" (and any other fixed provider hosts) to the `allow` array. No tauri.conf.json change is required (CSP is null, plugin scope lives only here).
  - risk: The list is static JSON compiled into the app: a user-configurable `baseUrl` (self-hosted proxy, Ollama on localhost, OpenAI-compatible endpoints) cannot be allow-listed at runtime; the choices are a fixed provider set, a very broad glob (`https://**`, which maintainers will likely reject), or doing the request in Rust via reqwest where capabilities do not apply. Note plugin-http is the only CORS-free path from TS; the webview's native fetch is unrestricted by CSP but subject to CORS.
- **src-tauri/src/fs.rs:21-86 download_file (pattern) + main.rs:173-180 collect_events! + src-tauri/src/progress.rs update_progress**: A Rust-side `commentary.rs` command can POST with reqwest (Client already in use, `features=["stream","json"]` Cargo.toml:23), read `res.bytes_stream()` like fs.rs:53-63, parse SSE lines, and push deltas to the webview either via a tauri-specta event (`#[derive(tauri_specta::Event)]` struct added to collect_events!, consumed as `events.xxx.listen` in TS) or via a `tauri::ipc::Channel<String>` argument (unused in the codebase today but standard Tauri 2). Cancellation: copy the `analysis_cancel_flags: DashMap<String, Arc<AtomicBool>>` pattern (main.rs:92, chess.rs cancel_analysis).
  - risk: Adds a second HTTP path (Rust) alongside the TS plugin-http convention used by every other API; keep the TS provider interface thin so either transport fits. Error mapping must go through error.rs (`From<reqwest::Error>` exists :127-131); never `unwrap()` the way oauth.rs:97-103 does.
- **src/utils/http.ts:17-22 apiHeaders + a new src/utils/commentary/provider*.ts, following src/utils/lichess/api.tsx:208-236**: TS provider client: `fetch(url, { method: "POST", headers: apiHeaders({ "x-api-key": key, "anthropic-version": "...", "content-type": "application/json" }), body })` using `import { fetch } from "@tauri-apps/plugin-http"`. Validate the response with zod `safeParse` exactly like ChessComStatsSchema (chess.com/api.tsx:31-37, :80-92). For streaming, `response.body?.getReader()` + TextDecoder to parse `text/event-stream` (no existing precedent in repo; see open_questions).
  - risk: apiHeaders currently only sets User-Agent; do not log headers (lichess/api.tsx:226 logs status+url only — keep that discipline). `http:default` includes fetch-cancel so AbortController should work, but nothing in the repo exercises it.
- **src/components/settings/SettingsPage.tsx:78-87 SettingCategory, :184-652 settings array, :628-651 useMemo deps, :656-708 categoryInfo, :818-846 Tabs.List, :850-964 Tabs.Panel**: Add a category (e.g. "commentary") to SettingCategory; add items `{ id: "commentary-enabled", category: "commentary", title: t("Settings.Commentary.Enabled"), description: t("Settings.Commentary.Enabled.Desc"), keywords: ["ai","llm","commentary","claude"], render: () => <SettingsSwitch atom={commentaryEnabledAtom} /> }`; for provider/model use an inline `<Select>` (pattern :166-176) and for the API key a dedicated component (like RepertoireMinGamesSetting) wrapping Mantine `PasswordInput` with a "Test connection" button; add `categoryInfo.commentary` with an icon (e.g. IconSparkles/IconMessage from @tabler/icons-react, `size="1rem"`), a `<Tabs.Tab value="commentary">` and a `<Tabs.Panel value="commentary">` that calls renderCategorySettings("commentary"). Add the strings to src/translation/en-US.json (e.g. "Settings.Commentary", "Settings.Commentary.Desc") and run `pnpm i18n:extract`.
  - risk: If an item's render reads an atom value from the page scope (as the Select items do) it must be added to the deps array at :628-651 or it will render stale; SettingsSwitch avoids this by reading the atom inside. The page is 975 lines in one file — maintainers may prefer a sub-component for a big new section. All 16 locale files will get null entries from extraction; CI (`i18next-cli extract --ci`) fails if you forget to run it.
- **src/components/settings/SettingsPage.tsx:128-151 TelemetrySwitch**: Model an "Enable AI commentary" switch on this: a local component that binds the atom and performs side effects on toggle (e.g. clearing cached commentary or validating the key).
  - risk: None structural; just keep the Switch props (onLabel/offLabel t("Common.On"/"Common.Off"), size="lg") consistent with SettingsSwitch.
- **src/components/panels/analysis/ReportModal.tsx:13-19 reportSettingsAtom and :72-104 analyze()**: Add `commentary: boolean` to reportSettingsAtom's default and a `<Checkbox {...form.getInputProps("commentary", { type: "checkbox" })}>` next to the existing ones (:160-176); after `addAnalysis(analysis.data, ...)` resolves, kick off commentary generation over the resulting MoveAnalysis[] gated on `commentaryEnabledAtom` and provider configured.
  - risk: reportSettingsAtom uses default storage with no zod: for existing users the stored object lacks the new key so `form.values.commentary` is `undefined` (falsy) — acceptable only if the default is false; otherwise switch the atom to createZodStorage with `.default()`. The modal closes before analysis finishes (:75), so progress/notifications must be out-of-band.
- **src/utils/unwrap.tsx:7-19 and the notifications.show pattern (lichess/api.tsx:227-232, chess.com/api.tsx:71-76)**: Surface provider failures (401 bad key, 429 rate limit, network) via `notifications.show({ title: t(...), message, color: "red", icon: <IconX /> })` after `error(...)` from @tauri-apps/plugin-log; use `unwrap()` for any new Rust commands.
  - risk: Existing notification strings are untranslated English; a feature meant for upstream should use t() like Board.tsx:206-210. There is no notification de-duplication (no `id`), so a streaming loop that errors per move could spam toasts.
- **src/App.tsx:146-205 useAppStartup (telemetry gate) and SettingsPage.tsx:141-143 opt in/out**: Any new posthog.capture (e.g. "commentary_generated" with provider name and move count only) must be wrapped in `if (store.get(telemetryEnabledAtom))` / `useAtomValue(telemetryEnabledAtom)`. Never include PGN, FEN, prompts, completions or key material.
  - risk: Only one event exists in the entire app; the maintainer may not want more. posthog.init runs even when telemetry is off, relying on posthog's own persisted opt-out flag — do not assume `posthog.capture` is a no-op when the atom is false.
- **src/state/keybinds.ts:14-58 keys map (GENERATE_REPORT: f12 at :55)**: Add e.g. `GENERATE_COMMENTARY: { name: "Generate commentary", keys: "shift+f12" }`; defaultStorage (:62-88) merges new keys into existing users' stored keymaps automatically.
  - risk: Keybind `name` strings are not i18n'd (they are rendered raw at SettingsPage.tsx:925).
- **i18next.config.ts:34-41 preservePatterns**: If commentary templates (offline fallback) use dynamically-built keys like t(`Commentary.${kind}`), add "Commentary.*" to preservePatterns or `removeUnusedKeys: true` will delete them on the next extract.
  - risk: Pattern list is small and curated; explain in the PR.
- **src/routes/__root.tsx:284-297 Clear saved data**: No change needed, but document that this wipes the stored API key (if kept in localStorage) along with lichess tokens.
  - risk: If the key moves to the OS keychain it will NOT be cleared here, which may surprise users; add a delete-secret call to this handler.

## Conventions
- State: global/settings state = Jotai `atomWithStorage` in src/state/atoms.ts; per-tab UI state = `atomFamily((_tab: string) => atom(...))` + `tabValue(family)` (atoms.ts:310-329); heavy mutable models (game tree, database view) = Zustand `createStore` + `persist` + immer `produce` (src/state/store/tree.ts, database.ts). Components read with `useAtom`/`useAtomValue`/`useSetAtom`; non-React code uses `getDefaultStore().get()` (App.tsx:80-101, :164-165).
- Persistence tiers: localStorage (default) for user preferences; sessionStorage for per-session UI (tabs, database view, per-tab tree via debounced storage); AppData files only for engines.json. No tauri-plugin-store.
- Typed persistence: zod schemas colocated with their domain types (`lichessGamesOptionsSchema` explorer.ts:28, `engineSchema` engines.ts:69, `tabSchema` tabs.ts:58, `ChessComStatsSchema` chess.com/api.tsx:31); types derived via `z.infer`/`z.output`; tolerant schemas with `.nullish()`, `.default(() => ...)`, `.catch()`; invalid stored data is reset to default with a `warn()` log, never thrown.
- Reading settings before React mounts: pass `{ getOnInit: true }` (telemetryEnabledAtom atoms.ts:215, dir atoms :98-109, sound atoms :197-208).
- Settings UI: one `SettingItem` per setting in SettingsPage.tsx with i18n title `Settings.<Category>.<Name>` and description `Settings.<Category>.<Name>.Desc`; boolean → `<SettingsSwitch atom>`; enum → inline Mantine `<Select allowDeselect={false}>`; numbers → `<SettingsNumberInput atom min max step>`; complex → dedicated component in src/components/settings/. Each category needs entries in SettingCategory, categoryInfo, Tabs.List and Tabs.Panel.
- i18n: `const { t } = useTranslation()` and `t("Flat.Dotted.Key")`; keys are flat (keySeparator false); all 16 locale JSON files under src/translation/ with a top-level "translation" object, sorted; run `pnpm i18n:extract` after adding keys (CI enforces with `i18next-cli extract --ci`); `<Trans i18nKey components={{...}}>` for rich text (ErrorComponent.tsx:41-52).
- HTTP from TS: always `import { fetch } from "@tauri-apps/plugin-http"` (never the global fetch), always pass `headers: apiHeaders(extra?)` from src/utils/http.ts, check `res.ok`, parse JSON and validate with zod `safeParse` where the shape matters; SWR (`useSWR`/`useSWRImmutable`) for component-level async data (engines.ts:95-113, db.ts:119-134).
- HTTP from Rust: `reqwest::Client::new()` per call (fs.rs:35), Bearer via HeaderMap, `bytes_stream()` for progress; errors through the single `Error` enum with `Box<...>` sources and `From` impls (error.rs).
- Tauri commands: `#[tauri::command] #[specta::specta] pub async fn ...(..., app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<T, Error>`; registered in `tauri_specta::collect_commands!` (main.rs:111-172); TS bindings auto-generated into src/bindings/generated.ts on debug builds and consumed as `commands.xxx()` returning `Result<T,string>`, unwrapped with `unwrap()` (src/utils/unwrap.tsx). Events: `#[derive(tauri_specta::Event)]` + `collect_events!` (main.rs:173-180), consumed as `events.progressEvent.emit/listen` (chess.com/api.tsx:139-149).
- Errors/notifications: log first with `@tauri-apps/plugin-log` (`error`, `warn`, `info`), then `notifications.show({ title, message, color: "red", icon: <IconX /> })` from @mantine/notifications; informational toasts omit color (routes/__root.tsx:303-306). Router-level failures fall to ErrorComponent (App.tsx:77).
- Logging: Rust `log` crate (info!/warn!) → stdout + app log dir `en-croissant.log` at Info level in release (main.rs:190-208); TS `attachConsole()` forwards Rust logs to devtools in dev (App.tsx:159).
- Forms/modals: `@mantine/form` `useForm({ initialValues, validate })`, `form.getInputProps(...)`, `<Modal opened onClose title={t(...)}>` wrapping `<form onSubmit={form.onSubmit(...)}>` (ReportModal.tsx:49-183).
- Icons: `@tabler/icons-react` with `size="1rem"` (SettingsPage.tsx categoryInfo, Tabs.Tab leftSection).
- Style: oxfmt formatting, oxlint + `tsgo --noEmit` type-check; .editorconfig: 2-space indent for .tsx, 4-space for .ts, LF, final newline; path alias `@/` → src/ (tsconfig.json, vite.config.ts:43-47); React Compiler enabled (vite.config.ts:19-21) but code still uses explicit `useMemo`/`memo` (ReportModal.tsx:187).
- Testing: vitest + jsdom, pure-function tests in src/utils/tests/*.test.ts (chess, chessops, misc, score, store); no component or Rust tests observed. `pnpm test` runs in CI after `pnpm lint:ci` (.github/workflows/test.yml:33-36).
- Feature gating: no flag framework; boolean `atomWithStorage` + SettingsSwitch is the de facto toggle; platform gating via `import.meta.env.VITE_PLATFORM` (vite define); dev-only via `#[cfg(debug_assertions)]`.
- Telemetry: PostHog with everything automatic disabled; a single explicit `posthog.capture` gated on `telemetryEnabledAtom`; opt-in/out toggled via `posthog.opt_in_capturing()/opt_out_capturing()`.
- Contribution process (CONTRIBUTING.md): check issues/Discord first, branch off `master`, `pnpm format && pnpm lint:fix && pnpm i18n:extract`, `pnpm build` and manually verify, then PR. License GPL-3.0-only (Cargo.toml:6).

## Gotchas
- Secrets are plaintext today: `sessionsAtom` (atoms.ts:213) persists `LichessSession.accessToken` as JSON in the WebView's localStorage with no encryption and no zod. There is no OS keychain, no tauri-plugin-store, no stronghold and no `keyring` crate anywhere (Cargo.toml:15-68; `security-framework` in Cargo.lock is only from native-tls/rustls-native-certs/rustls-platform-verifier). An LLM API key stored the same way inherits this; the lichess token is low-value (scope `preference:read`, oauth.rs:71) whereas an API key is billable.
- The lichess token is broadcast with `app.emit("access_token", ...)` (oauth.rs:106) — a global Tauri event any listener in the webview can receive; and `exchange_code(...).request_async(...).await.unwrap()` (oauth.rs:97-103) panics the axum handler on failure. Do not copy this pattern for an LLM key.
- `atomWithStorage` without `getOnInit: true` returns the default value to `getDefaultStore().get()` until the atom is mounted by a React component; any service code that reads provider config/API key outside React must set `getOnInit: true` (precedent telemetryEnabledAtom atoms.ts:215-217). src/utils/directories.ts:4-14 works around this by reading localStorage directly.
- Jotai default storage replaces the stored object wholesale — no deep merge with new defaults. `reportSettingsAtom` (ReportModal.tsx:13-19) and all untyped atoms will yield `undefined` for newly-added fields on existing installs. Use createZodStorage with `.default()` on every field (engineSchema style) for any object-shaped setting.
- The plugin-http allow-list (capabilities/main.json:42-53) is compile-time JSON. api.anthropic.com is not in it; adding it is mandatory for TS-side calls, and a user-editable base URL cannot be honoured without either a broad glob or routing through Rust reqwest (capabilities do not gate Rust code). The generated schema it references (`../gen/schemas/desktop-schema.json`, main.json:2) is not in the clone.
- CSP is `null` (tauri.conf.json:84): the webview's native `fetch`/XHR reaches any host (posthog-js already does this to app.posthog.com which is NOT in the allow-list). So the capability list is not an exfiltration boundary; it only constrains plugin-http. Native fetch is subject to CORS, plugin-http is not.
- No streaming consumer exists in TS (grep for ReadableStream/getReader/TextDecoder/EventSource is empty) and `tauri::ipc::Channel` is unused in Rust; the only streaming is `bytes_stream()` accumulating into memory in fs.rs:51-63 with progress emitted through `update_progress`. Streaming LLM output has no in-repo precedent on either side.
- The sessionStorage key "database-view" is used by both `selectedDatabaseAtom` (atoms.ts:267-271, raw JSON) and the zustand `activeDatabaseViewStore` (database.ts:196, `{state, version}` envelope) — they would clobber each other if both are live. Pick unique, prefixed keys for new atoms.
- Atom keys are historical and sometimes misleading (`moveNotationTypeAtom` key is "letters" atoms.ts:170; `snapArrowsAtom` key "snap-dests" :176; key rename precedent "show-coordinates-v2" :192 leaves the old key orphaned). There is no key registry; collisions are only caught by reading atoms.ts.
- `enginesAtom` is `unwrap()`ped (atoms.ts:153): it is `undefined` until the engines.json read resolves; every consumer guards with `engines ?? []` (ReportModal.tsx:41, bestMovesFamily atoms.ts:561-562). Any file-backed commentary store will behave the same.
- Help > Clear saved data (routes/__root.tsx:284-297) calls `localStorage.clear(); sessionStorage.clear(); location.reload()` — wipes every setting including tokens; a keychain-stored key would NOT be cleared there unless you add it.
- SettingsPage's `settings` useMemo has a hand-maintained deps array (:628-651). An item whose `render` closes over a page-scope atom value (the Select pattern) that is not listed there renders stale. `SettingsSwitch`/`SettingsNumberInput` read their atom internally and avoid the problem.
- CI runs `i18next-cli extract --ci` (package.json:15, test.yml:33): forgetting `pnpm i18n:extract` after adding `t()` keys fails the build; `removeUnusedKeys: true` deletes keys not literally present in code, so dynamically-composed keys need a `preservePatterns` entry (i18next.config.ts:34-41). i18next types output (src/i18next.d.ts) is not committed, so `t()` keys are not type-checked.
- Most `notifications.show` calls use hardcoded English (chess.com/api.tsx, lichess/api.tsx, unwrap.tsx); only Board.tsx:206-210 is translated. Upstream-quality code should use `t()`.
- posthog.init runs unconditionally (App.tsx:167-173) and the default for `telemetry-enabled` is `true` (opt-out model). The startup path only gates the single `app_started` capture by the atom and relies on posthog's own persisted opt-out flag set by the settings switch; any new capture must check the atom itself.
- Indentation differs by extension (.editorconfig): 4 spaces in .ts (atoms.ts, utils.ts) but 2 spaces in .tsx (SettingsPage.tsx, api.tsx). oxfmt enforces; don't hand-format against it.
- Rust `Error` serializes to a bare display string (error.rs:187-194); the frontend cannot distinguish error kinds (e.g. 401 vs 429 from a provider) unless you encode them in the message or add a structured error type.
- `reqwest` is version 0.12.5 while `oauth2 4.4.2` pulls its own `reqwest::async_http_client` (oauth.rs:4) — adding provider code should reuse the existing reqwest, not introduce another HTTP crate.
- `http:default` in Tauri 2 (from library knowledge, not verifiable here) bundles allow-fetch, allow-fetch-send, allow-fetch-read-body and allow-fetch-cancel, so no extra permission identifiers are needed beyond the URL scope entry.

## Open questions
- Does @tauri-apps/plugin-http 2.5.7's `fetch` expose an incrementally-filled `ReadableStream` body (so `response.body.getReader()` can parse SSE as chunks arrive)? From library knowledge it does (fetch_read_body over a Tauri Channel), but node_modules is absent in this clone so it could not be verified against the installed version; confirm in node_modules/@tauri-apps/plugin-http/dist-js/index.js before committing to TS-side streaming.
- Is `selectedDatabaseAtom` ("database-view", atoms.ts:267) still referenced anywhere, and does it actually collide with the zustand `database-view` persist key (database.ts:196)? Not grepped for usages.
- Will upstream accept a new native crate (`keyring`) for secret storage, or do they prefer localStorage parity with the lichess token? CONTRIBUTING.md says to ask on issues/Discord first; no prior discussion was checked.
- Exact Tauri 2 composition of the `http:default` permission set and whether a wildcard like `https://**` would be accepted by maintainers for user-configurable base URLs — the generated schema (src-tauri/gen/schemas/desktop-schema.json) is gitignored and not present.
- Where each platform's WebView stores localStorage on disk (WebKitGTK / WebView2 / WKWebView) and whether it is world-readable — relevant to the security story for an API key but not verifiable from source.
- Does the Rust side have unit tests or a rustfmt/clippy config? Only the frontend test setup (vitest, src/utils/tests) was inspected; `.oxfmtrc.json`/`.oxlintrc.json` ignore src-tauri/**.
- Is `fileStorage` (src/state/utils.ts:12-27) intentionally kept for future use or dead code? It has no call sites; using it for a commentary cache would be the first use.
- Would the maintainer accept any additional PostHog event at all? The app currently emits exactly one event (`app_started`); product policy on telemetry is undocumented beyond the single privacy switch.
- How the report output (`MoveAnalysis[]` from `commands.analyzeGame`, consumed by `addAnalysis` in src/state/store/tree.ts) stores per-move comments in the tree — this is the natural place to attach commentary text but is outside this subsystem and was not read in depth.
- Whether `i18next-cli extract` with `defaultValue: null` writes `null` or the key string for new keys in en-US.json (affects whether a PR must hand-edit en-US.json before CI passes); inferred from config only.
