# Project tooling, conventions, i18n, testing, CI, licensing and release process of En Croissant v0.15.1 (the contribution quality bar). Clone root: /tmp/claude-0/-home-user-Chess/095b4a5e-49ff-5938-b919-e82f3408c9dc/scratchpad/upstream/en-croissant (shallow, 1 commit). All paths below are relative to that root.

## Summary
TOOLCHAIN AND GATES. Frontend is pnpm 10 + Vite 8 (rolldown) + React 19 with the React Compiler enabled (vite.config.ts:14-22 wires tanstackRouter file-based routing, @vitejs/plugin-react and @rolldown/plugin-babel with reactCompilerPreset), typechecked by `tsgo --noEmit` (TypeScript native preview 7.0.0-dev, package.json:99) under `strict: true`, `target: es2015`, alias `@/*` -> `src/*` and typed CSS modules (tsconfig.json). Linting is oxlint 1.55 with plugins eslint/import/react/react-perf/oxc/typescript/vitest and NO custom rules or categories (.oxlintrc.json:3-5, i.e. oxlint defaults), formatting is oxfmt 0.40 with only ignore patterns (.oxfmtrc.json) plus .editorconfig (4-space .ts, 2-space .tsx/.json, LF). Tests are vitest 4.1 with a global jsdom environment and no setup files (vite.config.ts:48-50). The exact pre-PR commands (CONTRIBUTING.md:64-70 + package.json:6-20) are: `pnpm format` (oxfmt), `pnpm lint:fix` (oxlint --fix), `pnpm lint` (tsgo --noEmit && oxlint), `pnpm test` (vitest run), `pnpm i18n:extract` (i18next-cli extract) if any translation key changed, and `pnpm build` (tauri build --no-bundle) to manually verify the feature. CI (.github/workflows/test.yml) runs on every push and every PR to any branch and executes exactly `pnpm install --frozen-lockfile`, `pnpm lint:ci` = `tsgo --noEmit && oxfmt --check && oxlint --format=github && i18next-cli extract --ci`, then `pnpm test`. CI never compiles, tests, fmt-checks or clippy-checks the Rust crate (the only Rust build is release.yml on `v*` tags), although src-tauri/src has `#[cfg(test)]` modules in 6 files (chess.rs:710, opening.rs, db/{mod,encoding,search,search_index}.rs). There is no PR template, no CODEOWNERS, no CLA/DCO, no commit-lint; the single visible commit is "Add Chinese translations for loading errors and settings (#902)" (imperative, capitalized, no conventional-commit prefix, PR number appended = squash merge).

I18N. i18next with react-i18next; resources are 16 statically imported JSON files (src/index.tsx:8-48), one namespace wrapper `{"translation": {...}}`, flat dotted PascalCase keys (`keySeparator: false`, `nsSeparator: false`, i18next.config.ts:27-28), 610 keys in en-US. Naming is `Area.SubArea.Thing` with `.Desc` suffix for descriptions (64 keys), `_one/_other` plural suffixes and `{{var}}` interpolation; top prefixes: Board (147), Settings (110), Common (57), Databases (54), Home (44), Menu (28), Annotate (24), Engines (24). Adding keys = call `t("New.Key")` in code, then run `pnpm i18n:extract`, which rewrites src/translation/{{language}}.json for ALL 16 locales with `sort: true`, `removeUnusedKeys: true`, `defaultValue: null`, `primaryLanguage: "en-US"`, and preserves only dynamically built key families listed in `preservePatterns` (Files.FileType.*, SideBar.*, TimeControl.*, Annotate.*, GoMode.*, Errors.*; i18next.config.ts:34-41). Runtime falls back to en-US (`fallbackLng: "en-US"`, `returnEmptyString: false`, src/index.tsx:53-54) and the committed locales are NOT complete (de-DE/nb-NO/tr-TR miss 14 en-US keys, zh-TW misses 16, en-GB has 342 strings identical to en-US), so contributors only need to author en-US; other languages are filled by separate translation PRs like #902. Typed keys are configured (`types` block writes src/i18next.d.ts and src/types/resources.d.ts) but neither file is committed and `i18n:types` is not part of lint, so `t()` is untyped and typos are not compile errors.

STYLE AND ARCHITECTURE RULES. State is split: jotai `atomWithStorage` atoms in src/state/atoms.ts for app-wide persisted preferences (kebab-case storage keys like "telemetry-enabled", "report-settings", optionally zod-validated via `createZodStorage`/`createAsyncZodStorage` in src/state/utils.ts), feature-local persisted atoms may be colocated in the component (ReportModal.tsx:13-19), and a zustand vanilla store (`createTreeStore`, src/state/store/tree.ts:95) built with immer `produce` and `persist` to a debounced sessionStorage holds the per-tab game tree, provided via React context `TreeStateContext` and read with `useStore(store, selector)`. Errors cross the Tauri boundary as tauri-specta `Result<T, string>` discriminated unions (`{status:"ok",data}|{status:"error",error}`; src/bindings/generated.ts:650) and are surfaced with `unwrap()` (src/utils/unwrap.tsx) which logs via @tauri-apps/plugin-log and shows a red Mantine notification; @badrap/result is used in exactly one file (src/utils/files.ts) and is not the house pattern. ts-pattern `match()` is used for exhaustive branching (23 files), zod for persisted data and route search params (src/routes/engines.tsx:5-7), Mantine 8 for all UI (133 files) with CSS modules (30 `*.module.css`) and @tabler icons, `const { t } = useTranslation()` in components (74 files), HTTP via `fetch` from @tauri-apps/plugin-http plus `apiHeaders()` (User-Agent) from src/utils/http.ts, with outbound hosts allow-listed in src-tauri/capabilities/main.json:42-53. Rust side: commands are `#[tauri::command] #[specta::specta]` functions registered in `collect_commands!` (src-tauri/src/main.rs:111-172), events in `collect_events!` (173-180), errors are one thiserror enum with boxed transparent variants serialized as strings (src-tauri/src/error.rs), and TypeScript bindings are regenerated into src/bindings/generated.ts only in debug builds (main.rs:182-188) and committed.

LICENSE, RELEASE, TELEMETRY. LICENSE is GPL-3.0; Cargo.toml:6 declares `license = "GPL-3.0-only"`; README:60 says GPL-3.0; package.json has no license field; no SPDX headers, no CLA, no DCO, so contributions are inbound=outbound GPL-3.0-only with authorship retained. Releases are cut by pushing a `v*` tag (release.yml:3-6): a 4-way matrix (macOS arm64 dmg, macOS x86_64 dmg, ubuntu-24.04 default bundles, windows nsis) runs tauri-apps/tauri-action@v0 with TAURI_SIGNING_PRIVATE_KEY secrets and creates a DRAFT GitHub release "v__VERSION__"; version lives in package.json:3 and src-tauri/Cargo.toml:3 (tauri.conf.json has no version key). The updater plugin is configured with a minisign pubkey and endpoint https://www.encroissant.org/updates (tauri.conf.json:61-66, `createUpdaterArtifacts: "v1Compatible"` line 40) and the app checks for updates at startup (src/App.tsx:119-134,162) and from Help menu (src/routes/__root.tsx:117-130). Telemetry is PostHog, initialized unconditionally with a hardcoded project key at startup (src/App.tsx:167-173; autocapture/pageview/session-recording off), gated by `telemetryEnabledAtom` whose default is TRUE (src/state/atoms.ts:215), i.e. opt-OUT, sending a single `app_started` event with version (App.tsx:175-177); the Settings > Privacy switch (src/components/settings/SettingsPage.tsx:128-151, 618-626) calls posthog.opt_in_capturing/opt_out_capturing.

## Key files
- `package.json` — Scripts and dependency manifest; defines the exact lint/format/test/i18n commands and the CI gate `lint:ci`.
  - "version": "0.15.1" :3
  - scripts.format = "oxfmt" :12
  - scripts.test = "vitest run" :13
  - scripts.lint = "tsgo --noEmit && oxlint" :14
  - scripts["lint:ci"] = "tsgo --noEmit && oxfmt --check && oxlint --format=github && i18next-cli extract --ci" :15
  - scripts["lint:fix"] = "oxlint --fix" :16
  - scripts["i18n:extract"|"i18n:status"|"i18n:types"|"i18n:sync"] :17-20
  - deps: @badrap/result :23, ts-pattern :84, zod ^3.22 :86, zustand ^5 :87, jotai ^2.18 :64, immer :63, swr :82, posthog-js :68, @tauri-apps/plugin-http :40, react-markdown :77
  - devDeps: @typescript/native-preview (tsgo) :99, oxlint ^1.55 :104, oxfmt ^0.40 :103, oxlint-tsgolint :105 (unused by scripts), i18next-cli ^1.42 :101, vitest ^4.1 :107, jsdom ^28 :102, babel-plugin-react-compiler :100
- `.github/workflows/test.yml` — The only PR gate: lint:ci + vitest on ubuntu; no Rust build.
  - on: push branches '*' / pull_request branches '*' / workflow_dispatch :2-9
  - pnpm/action-setup@v4 version 10 :18-21
  - pnpm install --frozen-lockfile :30
  - run: pnpm lint:ci :33
  - run: pnpm test :36
- `.github/workflows/release.yml` — Tag-driven multi-platform build and draft GitHub release via tauri-action with updater signing.
  - on: push tags 'v*' :3-6
  - matrix (macos arm64/x86_64 dmg, ubuntu-24.04, windows nsis) :12-23
  - apt pins libwebkit2gtk-4.1 2.44.0-2 :33-42
  - tauri-apps/tauri-action@v0 :70
  - TAURI_SIGNING_PRIVATE_KEY / _PASSWORD secrets :74-75
  - releaseName "v__VERSION__", releaseDraft: true, prerelease: false :79-82
- `CONTRIBUTING.md` — Contributor process: fork, branch off master, commands, PR checklist.
  - check issues / Discord first :17-19
  - Commands list (pnpm i/dev/test/format/lint:fix/i18n:extract/build) :34-62
  - Submitting a PR: build + manual check, format then lint:fix, i18n:extract if keys changed :64-70
- `i18next.config.ts` — i18next-cli config: locales, extraction rules, preservePatterns, (uncommitted) type generation.
  - locales[16] :4-21
  - extract.input src/**/*.{ts,tsx} :23
  - extract.output src/translation/{{language}}.json :24
  - keySeparator:false, nsSeparator:false :27-28
  - defaultNS "translation", mergeNamespaces true :29-30
  - primaryLanguage "en-US", defaultValue null, removeUnusedKeys true :31-33
  - preservePatterns [Files.FileType.*, SideBar.*, TimeControl.*, Annotate.*, GoMode.*, Errors.*] :34-41
  - sort: true :42
  - types.output src/i18next.d.ts, resourcesFile src/types/resources.d.ts :44-48 (not committed)
- `src/index.tsx` — App bootstrap: i18n init with static resources, dayjs plugin, immer setAutoFreeze(false), React root.
  - i18n.use(LanguageDetector).use(initReactI18next).init :27-55
  - detection order/caches localStorage :49-52
  - fallbackLng "en-US" :53
  - returnEmptyString: false :54
  - setAutoFreeze(false) :59
- `src/translation/en-US.json` — Primary locale; 610 flat keys under the single "translation" wrapper.
  - "translation" wrapper :2
  - Annotate.* :3-26
  - Board.Analysis.* :35-65 (GenerateReport :45, GeneratingReport :46, Report :53, ReportGenerated :54, ShowVariations(.Desc) :57-58)
  - Settings.Privacy(.Desc|.Telemetry|.Telemetry.Desc) :573-576
- `vite.config.ts` — Vite/vitest config: router codegen, React Compiler, @ alias, jsdom test env, VITE_PLATFORM define.
  - tanstackRouter({target:"react"}) :15-17
  - react() + babel({presets:[reactCompilerPreset()]}) :18-21
  - resolve.alias "@" -> ./src :43-47
  - test.environment "jsdom" :48-50
  - define VITE_PLATFORM :51-53
- `tsconfig.json` — TS config: strict, es2015 target, bundler resolution, @/* paths, css-modules plugin, excludes src-tauri.
  - strict: true :8
  - paths @/* :17-19
  - plugins typescript-plugin-css-modules :20
  - include src/**/* :22
- `.oxlintrc.json` — oxlint config: default categories/rules, plugin list, ignores generated files.
  - plugins [eslint, import, react, react-perf, oxc, typescript, vitest] :3
  - categories {} / rules {} :4-5
  - ignorePatterns incl src/bindings/generated.ts, src/routeTree.gen.ts :39-45
- `.oxfmtrc.json` — oxfmt config (ignore patterns only).
  - ignorePatterns :3-9
- `.editorconfig` — Indentation rules: .ts 4 spaces, .tsx/.jsx 2 spaces, .json 2 spaces, LF, final newline.
  - [*.{jsx,tsx}] indent_size 2 :9-10
  - [*.ts] indent_size 4 :12-13
- `src/utils/tests/score.test.ts` — Representative pure-function test style for the scoring/annotation logic a deterministic commentary fallback would build on.
  - import { expect, test } from "vitest" :1
  - formatScore/getWinChance/getAccuracy/getCPLoss/getAnnotation tests :4-53
- `src/utils/tests/store.test.ts` — Zustand tree-store tests using createTreeStore + full TreeState literals; 22 tests incl. addAnalysis.
  - createTreeStore() :6
  - beforeEach store.setState(defaultTree()) :8-10
  - treeE4D5 fixture :14-60
  - test("should handle addAnalysis") :415
  - test("should handle setComment") :317
- `src/utils/tests/chess.test.ts` — NAG/annotation consistency + priority tests.
  - NAGs are consistent :5-11
  - hasMorePriority :13-22
- `src-tauri/tauri.conf.json` — Bundle, updater, CSP, identifier.
  - bundle.publisher "Francisco Salgueiro" :26
  - bundle.resources ../sound/ :36-38
  - createUpdaterArtifacts "v1Compatible" :40
  - productName/mainBinaryName "en-croissant" :48-49
  - identifier "org.encroissant.app" :50
  - plugins.updater.pubkey/endpoints https://www.encroissant.org/updates :61-66
  - security.csp null :84
- `src-tauri/Cargo.toml` — Rust crate manifest: license, version, deps (reqwest, tauri-specta, shakmaty).
  - version "0.15.1" :3
  - license = "GPL-3.0-only" :6
  - reqwest 0.12 (stream, blocking, json) :23
  - specta / tauri-specta ^2.0.0-rc.20 :44-45
  - tauri-plugin-http 2.5.7 :56
  - vampirc-uci git fork :39-42
- `src-tauri/capabilities/main.json` — Tauri v2 capability: fs/opener/http scopes; http allow-list of outbound hosts.
  - http:default allow [encroissant.org, api.chess.com, www.chess.com, lichess.org, explorer.lichess.org, tablebase.lichess.org, chessdb.cn] :42-53
  - fs:scope-appdata-recursive ** :34-41
- `src-tauri/src/main.rs` — Tauri entry: tauri-specta command/event registration, debug-only TS bindings export, plugin wiring.
  - tauri_specta::Builder::new().commands(collect_commands!(...)) :110-172 (analyze_game :114, cancel_analysis :115, get_progress :169)
  - .events(collect_events!(BestMovesPayload, DatabaseProgress, ProgressEvent, ...)) :173-180
  - #[cfg(debug_assertions)] specta_builder.export(Typescript..., "../src/bindings/generated.ts") :182-188
  - .invoke_handler(specta_builder.invoke_handler()) :210
  - .plugin(tauri_plugin_http::init()) :212
- `src-tauri/src/error.rs` — Single thiserror Error enum; serialized to string for the frontend; specta Type = string.
  - pub enum Error :5-95 (Reqwest :22, AnalysisCancelled :94)
  - impl serde::Serialize for Error :187-194
  - impl Type for Error (String) :196-203
- `src/App.tsx` — Root component: router creation with loadDirs context, startup sequence (splash, updater, posthog, CLI args), Mantine theme/providers.
  - createRouter({routeTree, defaultErrorComponent, context:{loadDirs}}) :75-111
  - checkForUpdates (plugin-updater check/downloadAndInstall/relaunch) :119-134
  - useAppStartup :146-205
  - posthog.init("phc_...", {autocapture:false,...}) :167-173
  - if (telemetryEnabled) posthog.capture("app_started") :175-177
  - App(): DndProvider > MantineProvider > ContextMenuProvider > Notifications + RouterProvider :276-291
- `src/routes/__root.tsx` — Root layout: native/custom menu bar, AppShell with SideBar and Outlet, drag-drop PGN open.
  - createRootRouteWithContext<{loadDirs}>()({component: RootLayout}) :82-86
  - RootLayout :88-391
  - menuActions (File/View/Help) :226-316
  - AppShell + <Outlet/> :354-390
- `src/routes/index.tsx` — "/" route -> BoardsPage with loadDirs loader.
  - createFileRoute("/")({component: BoardsPage, loader}) :4-7
- `src/routes/settings.tsx` — "/settings" route -> SettingsPage; loader returns dirs + version.
  - createFileRoute("/settings") :5-11
- `src/routes/engines.tsx` — Example of zod-validated search params on a route.
  - searchSchema = z.object({selected: z.number().optional()}) :5-7
  - validateSearch: searchSchema :11
- `src/state/atoms.ts` — All app-wide jotai atoms (persisted via atomWithStorage, some zod-validated).
  - zodArray helper :36-45
  - tabsAtom (sessionStorage, zod) :58-62
  - activeTabAtom :64-68
  - currentTabAtom :76-95
  - stored*DirAtom (getOnInit) :98-109
  - sessionsAtom (Lichess accessToken in localStorage) :213
  - telemetryEnabledAtom default true :215-217
  - recentFilesAtom/addRecentFileAtom :230-232
- `src/state/utils.ts` — Storage adapters: AppData file storage and zod-validating sync/async storages for atomWithStorage.
  - fileStorage (BaseDirectory.AppData) :12-27
  - createZodStorage :29-59
  - createAsyncZodStorage :61-95
- `src/state/store/tree.ts` — Per-tab zustand game-tree store (immer + persist to debounced sessionStorage); holds annotations, comments, scores, report state.
  - interface TreeStoreState extends TreeState :26-91
  - setAnnotation/setComment :63-64
  - addAnalysis signature :74-81
  - setReportInProgress :83
  - createTreeStore(id?, initialTree?) :95
  - addAnalysis action :488-494
  - function addAnalysis(state, analysis, options) :693+ (annotation via getAnnotation :722-730, variation insertion :734-770)
- `src/utils/treeReducer.ts` — Tree data model and iterators.
  - interface TreeState :8-14
  - interface TreeNode :16-28
  - treeIterator :35
  - defaultTree, createNode, getNodeAtPath, treeIteratorMainLine (imported by tree.ts:16-24)
- `src/utils/score.ts` — Pure scoring helpers (win chance, accuracy, CP loss, annotation classification).
  - INITIAL_SCORE :7
  - formatScore :17
  - getWinChance :34
  - normalizeScore :38
  - getAccuracy :60
  - getCPLoss :69
  - getAnnotation(prevprev, prev, next, color, prevMoves, is_sacrifice?, move?) :75-83 (thresholds 20/10/5 win% :87-95, !!/!/!? :97-115)
- `src/utils/chess.ts` — PGN serialization/parsing, main line, game stats.
  - getMoveText (comment -> "{...}" :117-120) :48
  - getMainLine :139
  - getPGN :218
  - parsePGN :460
  - getGameStats(root) -> {whiteCPL, blackCPL, whiteAccuracy, blackAccuracy, whiteAnnotations, blackAnnotations} :541-612
- `src/utils/annotation.ts` — Annotation glyph vocabulary, NAG mapping, display info with i18n translationKey.
  - type Annotation :12-38
  - NAG_INFO :40-73
  - type AnnotationInfo {group?, name, translationKey?, color?, nag} :75-81
  - ANNOTATION_INFO: Record<Annotation, AnnotationInfo> :83+
- `src/utils/unwrap.tsx` — House error-surfacing helper for specta Results.
  - unwrap<T>(result: Result<T,string>): T (logs, notifications.show red, throws) :7-19
- `src/utils/http.ts` — User-Agent header helper for outbound HTTP.
  - initUserAgent :8
  - apiHeaders(extra?) :17-22
- `src/utils/lichess/api.tsx` — Reference implementation of an external HTTP client in the house style.
  - import { fetch } from "@tauri-apps/plugin-http" :5
  - import { match, P } from "ts-pattern" :10
  - apiHeaders usage :20
  - baseURL/explorerURL/tablebaseURL :30-32
- `src/bindings/generated.ts` — tauri-specta generated commands/events/types (committed, lint/format-ignored).
  - commands.analyzeGame(id, engine, goMode, options: AnalysisOptions, uciOptions): Promise<Result<MoveAnalysis[], string>> :22-30
  - commands.cancelAnalysis :31
  - type AnalysisOptions / BestMoves / MoveAnalysis / ProgressEvent / GoMode (see data_model)
  - export type Result<T,E> :650
- `src/bindings/index.ts` — Re-exports generated bindings and widens Score/BestMoves/DatabaseInfo types.
  - export * from "./generated" :9
  - ScoreValue adds {type:"dtz"} :10
  - DatabaseInfo union :16-29
- `src/components/panels/analysis/AnalysisPanel.tsx` — Analysis side panel with vertical Tabs: engines / report / logs.
  - <Tabs orientation="vertical" placement="right" keepMounted={false}> :90-99
  - Tabs.Tab engines/report/logs :101-107
  - Tabs.Panel value="report" -> <ReportPanel/> :248-258
- `src/components/panels/analysis/ReportModal.tsx` — "Generate report" modal: mantine form, colocated persisted settings atom, calls commands.analyzeGame then store.addAnalysis.
  - reportSettingsAtom = atomWithStorage("report-settings", {novelty, reversed, variations, goMode, engine}) :13-19
  - ReportModal props {tab, initialFen, moves, reportingMode, closeReportingMode, setInProgress} :21-35
  - useForm validate engine/novelty :49-59
  - analyze(): commands.analyzeGame(`report_${tab}`, ...) .then(addAnalysis(analysis.data, {showVariations})) :72-104
  - export default memo(ReportModal) :187
- `src/components/panels/analysis/ReportPanel.tsx` — Report tab body: ProgressButton to open ReportModal, EvalChart, per-colour stats from getGameStats.
  - useStore(store, s => s.report.inProgress / s.setReportInProgress) :28-29
  - stats = getGameStats(root) :31
  - handleCancel -> commands.cancelAnalysis(`report_${activeTab}`) :33-35
  - t(`Annotate.${translationKey}`) :140
- `src/components/settings/SettingsPage.tsx` — Settings page: flat searchable list of SettingItem grouped by SettingCategory; telemetry switch lives here.
  - type SettingCategory = "board"|"inputs"|"anarchy"|"appearance"|"sound"|"keybinds"|"directories"|"repertoire"|"privacy" :78-87
  - interface SettingItem {id, category, title, description, keywords?, render} :89-96
  - SettingRow({title, description, children, highlight}) :98-126
  - TelemetrySwitch (posthog.opt_in/out_capturing) :128-151
  - settings: SettingItem[] = useMemo([...]) :184+
  - telemetry item category "privacy" :618-626
  - category filtering :713, 770
- `src/components/settings/SettingsSwitch.tsx` — Generic boolean-atom switch used by settings rows.
  - SettingsSwitch({atom}: {atom: PrimitiveAtom<boolean>}) :5-20
- `src/hooks/useProgress.ts` — Hook consuming Rust ProgressEvent stream by id.
  - useProgress(id) -> {progress, finished, isActive, clear, item} :4-42
  - events.progressEvent.listen :16
- `src/components/common/ProgressButton.tsx` — Action button with progress/cancel used by report generation.
  - Props {id, initInstalled, onClick, onCancel?, leftIcon?, labels{completed,action,inProgress,finalizing?}, disabled?, redoable?, inProgress, setInProgress} :7-23
- `src/components/common/TreeStateContext.tsx` — Context provider creating one tree store per board tab.
  - TreeStateContext = createContext<TreeStore|null>(null) :5
  - TreeStateProvider({id, initial, children}) :7-19
- `src/components/panels/annotation/AnnotationPanel.tsx` — Per-move annotation glyph buttons + comment editor (AnnotationEditor) reading ANNOTATION_INFO.translationKey.
  - SymbolButton with t(`Annotate.${translationKey}`) :22-47
  - BASIC/ADVANTAGE/EXTRA glyph groups :51-53
  - AnnotationPanel :55+
- `src/state/store/debouncedStorage.ts` — Debounced sessionStorage PersistStorage for zustand persist (300 ms).
  - DEBOUNCE_MS :3
  - flush/scheduleFlush :9-30
- `src/utils/misc.ts` — Small utils incl. mean/harmonicMean; contains a stale eslint-disable comment.
  - useThrottledEffect :72-83 (eslint-disable-next-line react-hooks/exhaustive-deps :81)
  - mean :85
  - harmonicMean :89
- `LICENSE` — GNU GPL v3 full text.
  - GNU GENERAL PUBLIC LICENSE Version 3, 29 June 2007 :1-2
- `README.md` — Project overview, build-from-source, license statement.
  - Building from source (pnpm install; pnpm build) :35-48
  - "licensed under GPL-3.0 License" :60

## Data model
// src/utils/treeReducer.ts:8-28
export interface TreeState {
    root: TreeNode;
    headers: GameHeaders;
    position: number[];
    dirty: boolean;
    report: ReportState;          // tests show shape { inProgress: boolean } (src/utils/tests/store.test.ts:57-59)
}
export interface TreeNode {
    fen: string;
    move: Move | null;
    san: string | null;
    children: TreeNode[];
    score: Score | null;
    depth: number | null;
    halfMoves: number;
    shapes: DrawShape[];
    annotations: Annotation[];
    comment: string;              // serialized into PGN "{...}" by getMoveText (src/utils/chess.ts:117-120)
    clock?: number;
}

// src/state/store/tree.ts:63-83 (subset of TreeStoreState extends TreeState)
setAnnotation: (payload: Annotation) => void;
setComment: (payload: string) => void;
addAnalysis: (
    analysis: { best: BestMoves[]; novelty: boolean; is_sacrifice: boolean }[],
    options?: { showVariations: boolean },
) => void;
setReportInProgress: (value: boolean) => void;

// src/bindings/generated.ts (tauri-specta, committed)
export type AnalysisOptions = { fen: string; moves: string[]; annotateNovelties: boolean; referenceDb: string | null; reversed: boolean }
export type BestMoves = { nodes: number; depth: number; score: Score; uciMoves: string[]; sanMoves: string[]; multipv: number; nps: number }
export type MoveAnalysis = { best: BestMoves[]; novelty: boolean; is_sacrifice: boolean }
export type ProgressEvent = { id: string; progress: number; finished: boolean }
export type GoMode = { t: "PlayersTime"; c: PlayersTime } | { t: "Depth"; c: number } | { t: "Time"; c: number } | { t: "Nodes"; c: number } | { t: "Infinite" }
export type EngineOption = { name: string; value: string }
export type Result<T, E> = { status: "ok"; data: T } | { status: "error"; error: E }   // :650 (mirrored in src/utils/unwrap.tsx:5)
commands.analyzeGame(id: string, engine: string, goMode: GoMode, options: AnalysisOptions, uciOptions: EngineOption[]): Promise<Result<MoveAnalysis[], string>>   // :22
// src/bindings/index.ts:10-14 widens: ScoreValue = ScoreValueT | { type: "dtz"; value: number }; Score = Omit<ScoreT,"value"> & { value: ScoreValue }; BestMoves = Omit<BestMovesT,"score"> & { score: Score }
// ScoreValue (from tests): { type: "cp" | "mate"; value: number }

// src/utils/annotation.ts:12-38, 75-81, 83
export type Annotation = "" | "!" | "!!" | "?" | "??" | "!?" | "?!" | "+-" | "±" | "⩲" | "=" | "∞" | "⩱" | "∓" | "-+" | "N" | "↑↑" | "↑" | "→" | "⇆" | "=∞" | "⊕" | "∆" | "□" | "⨀" | "⊗";
type AnnotationInfo = { group?: string; name: string; translationKey?: string; color?: MantineColor; nag: number };
export const ANNOTATION_INFO: Record<Annotation, AnnotationInfo>;   // e.g. "??": { group:"basic", name:"Blunder", translationKey:"Blunder", color:"red", nag:4 } -> i18n key "Annotate.Blunder"

// src/utils/score.ts:75-83
export function getAnnotation(prevprev: ScoreValue | null, prev: ScoreValue | null, next: ScoreValue, color: Color, prevMoves: BestMoves[], is_sacrifice?: boolean, move?: string): Annotation
// src/utils/chess.ts:541 return shape
getGameStats(root: TreeNode): { whiteCPL: number; blackCPL: number; whiteAccuracy: number; blackAccuracy: number; whiteAnnotations: Record<"??"|"?"|"?!"|"!!"|"!"|"!?", number>; blackAnnotations: same }

// src/components/settings/SettingsPage.tsx:78-96
type SettingCategory = "board" | "inputs" | "anarchy" | "appearance" | "sound" | "keybinds" | "directories" | "repertoire" | "privacy";
interface SettingItem { id: string; category: SettingCategory; title: string; description: string; keywords?: string[]; render: () => React.ReactNode; }

// src/components/panels/analysis/ReportModal.tsx:13-19 (feature-local persisted settings precedent)
const reportSettingsAtom = atomWithStorage("report-settings", { novelty: true, reversed: true, variations: true, goMode: { t: "Time", c: 500 } as Exclude<GoMode, { t: "Infinite" }>, engine: "" });

// src/state/atoms.ts:215
export const telemetryEnabledAtom = atomWithStorage<boolean>("telemetry-enabled", true, undefined, { getOnInit: true });
// src/utils/session.ts:4-8 (precedent: secret token persisted in localStorage via sessionsAtom, atoms.ts:213)
export type LichessSession = { accessToken?: string; username: string; account: LichessAccount };

// src/translation/*.json shape (i18next.config.ts: keySeparator:false, mergeNamespaces:true)
{ "translation": { "Board.Analysis.GenerateReport": "Generate report", "Board.Analysis.ShowVariations.Desc": "Add best continuation lines for mistakes", "Files.GameCountSuffix_one": "{{number}} game", ... } }   // 610 flat keys in en-US

// src-tauri/src/error.rs:4-5, 187-203
#[derive(Debug, thiserror::Error)] pub enum Error { #[error(transparent)] Reqwest(Box<reqwest::Error>), ..., #[error("Analysis cancelled")] AnalysisCancelled }
impl serde::Serialize for Error { /* serialize_str(self.to_string()) */ }   impl specta::Type for Error { /* String */ }

## Extension points
- **src/components/panels/analysis/ReportModal.tsx:72-104 (analyze) and src/state/store/tree.ts:488-494 / 693+ (addAnalysis)**: The report pipeline is: commands.analyzeGame(`report_${tab}`, enginePath, goMode, AnalysisOptions, uciOptions) -> MoveAnalysis[] -> store.addAnalysis(data, {showVariations}) which writes node.score, node.annotations and inserts best-line variations. A commentary stage hooks in right after addAnalysis: iterate the main line with the same MoveAnalysis[] (best lines + scores, novelty, is_sacrifice), produce per-move text and call store.setComment / write node.comment (string) so it is persisted by the existing tree persist + getPGN. Add a 'commentary' checkbox to the ReportModal form (mantine useForm, persisted in reportSettingsAtom) in the style of the existing novelty/reversed/variations checkboxes (:160-176).
  - risk: addAnalysis is a pure function over a Draft<TreeState>; keep the commentary step a separate action so store.test.ts's 'should handle addAnalysis' (store.test.ts:415) stays untouched. Overwriting a user's existing node.comment would be destructive; append or guard on empty. The report id `report_${tab}` and cancelAnalysis (ReportPanel.tsx:33-35) only cancel the engine pass, not a later LLM pass.
- **src/components/panels/analysis/AnalysisPanel.tsx:101-107 and :248-258; src/components/panels/analysis/ReportPanel.tsx**: Game-level narrative and per-move explanation UI belong in the existing 'report' Tabs.Panel (ReportPanel) or as a 4th Tabs.Tab; ReportPanel already computes getGameStats(root) (chess.ts:541) which gives per-colour CPL/accuracy/annotation counts for a deterministic narrative. Per-move text can also be shown in the annotation panel (src/components/panels/annotation/AnnotationPanel.tsx) which already renders node.comment via AnnotationEditor.
  - risk: Tabs use keepMounted={false} and display toggling; long-running LLM state must live in the tree store or an atom, not component state, or it is lost when switching tabs.
- **src/components/settings/SettingsPage.tsx:78-96 (SettingCategory/SettingItem), :184+ (settings array), :618-626 (privacy/telemetry item); src/state/atoms.ts**: Add a new SettingCategory (e.g. "commentary") or reuse "privacy", then push SettingItem entries {id, category, title: t("Settings.Commentary.Provider"), description: t("Settings.Commentary.Provider.Desc"), keywords, render} for provider select, API key input (Mantine PasswordInput), model, and an explicit enable switch via <SettingsSwitch atom={...}/>. Persist with atomWithStorage in src/state/atoms.ts using kebab-case keys ("commentary-provider", "commentary-api-key") and a zod schema through createZodStorage (src/state/utils.ts:29) for the structured config. The category also needs a label/description pair like Settings.Privacy / Settings.Privacy.Desc (en-US.json:573-574) and the section render code around SettingsPage.tsx:702-713, 770, 844, 958.
  - risk: API keys would sit in plaintext localStorage like the Lichess accessToken (sessionsAtom, atoms.ts:213) — consistent with precedent but a reviewer may ask for at least a warning; there is no keychain/stronghold dependency. Default for any provider that sends game data off-device should be OFF (telemetry's default-on at atoms.ts:215 is already the app's only privacy-sensitive default).
- **src-tauri/capabilities/main.json:42-53 (http:default allow-list) and src/utils/lichess/api.tsx:5,20 (fetch from @tauri-apps/plugin-http + apiHeaders)**: Frontend-side provider client: add "https://api.anthropic.com/**" (and any other provider host) to the http allow-list, then implement the client with `fetch` from "@tauri-apps/plugin-http" and `apiHeaders({ "x-api-key": ..., "anthropic-version": ... })` from src/utils/http.ts:17, returning the house Result-like discriminated union or throwing and letting the caller use notifications.show. Keep the client dependency-free (no @anthropic-ai/sdk) to match the project's raw-fetch style for Lichess/chess.com/chessdb.
  - risk: Capability changes require a Rust rebuild and are security-reviewed; a wildcard host for 'custom OpenAI-compatible base URL' would be pushed back on. Alternative with no capability change: a Rust command using the already-present reqwest (Cargo.toml:23), registered in collect_commands! (main.rs:111-172) with a new Error variant (error.rs), which also gives cancellation/streaming via ProgressEvent — but CI does not compile Rust, so Rust code is only validated locally and at release.
- **src/utils/score.ts:75-115 (getAnnotation thresholds), src/utils/annotation.ts:83 (ANNOTATION_INFO.translationKey), src/translation/en-US.json Annotate.* (:3-26)**: The offline deterministic fallback should reuse getWinChance/getCPLoss/getAnnotation and the Annotate.* vocabulary (translationKey -> t(`Annotate.${key}`), already whitelisted by preservePatterns 'Annotate.*') and add new flat keys such as "Commentary.Blunder.Intro" with {{move}}/{{bestMove}}/{{cp}} interpolation; since keys are built dynamically they must be added to i18next.config.ts preservePatterns (e.g. "Commentary.*") or they will be deleted by `pnpm i18n:extract` (removeUnusedKeys: true).
  - risk: Any change to getAnnotation thresholds breaks score.test.ts expectations (:35-53) and chess.test.ts NAG consistency; add new pure functions beside them rather than changing them. Natural-language templates in 16 locales are heavy; the project only requires en-US, others fall back (index.tsx:53).
- **src/utils/tests/*.test.ts (vitest, jsdom, `import { expect, test } from "vitest"`)**: Put the commentary core (feature extraction from MoveAnalysis[], template rendering, prompt building, response parsing/validation with zod) in src/utils/commentary/*.ts with zero Tauri/Mantine imports, and add src/utils/tests/commentary.test.ts in the flat `test("should ...")` style using literal TreeState/MoveAnalysis fixtures like store.test.ts:14-60. Zod-validate LLM output the way persisted data is validated (createZodStorage pattern).
  - risk: There is no mocking infrastructure and no component testing; modules importing @tauri-apps/* (e.g. src/utils/unwrap.tsx, plugin-log) cannot be imported in tests without vi.mock. CI runs `vitest run` on every PR, so tests must be deterministic (no network).
- **i18next.config.ts:34-41 (preservePatterns) + `pnpm i18n:extract`; CI `i18next-cli extract --ci` (package.json:15)**: Every user-visible string must go through t("Area.Sub.Key") with PascalCase dotted keys and a `.Desc` sibling for descriptions; run `pnpm i18n:extract` and commit the regenerated 16 locale files (sorted) — CI fails if extraction output differs from the committed files.
  - risk: Extraction rewrites all locale files (sort + prune), producing wide diffs; dynamic keys silently vanish unless whitelisted. Keys are untyped (src/i18next.d.ts not committed) so a typo falls back to the raw key string at runtime.
- **src/hooks/useProgress.ts and src/components/common/ProgressButton.tsx (Props :7-23); Rust ProgressEvent in collect_events! (main.rs:173-180)**: If commentary generation runs in Rust, emit ProgressEvent {id: `commentary_${tab}`, progress, finished} and reuse ProgressButton/useProgress for the UI; if it runs in TS, mirror the same {inProgress} flag pattern as TreeState.report.inProgress via a store field/atom.
  - risk: ProgressButton couples 'finished' to setInProgress(false) (:42-46); a TS-only implementation must drive inProgress manually in a finally block like ReportModal.tsx:103.
- **src/utils/chess.ts:117-120 (getMoveText writes node.comment inside '{...}') and :460 parsePGN**: Storing commentary in TreeNode.comment makes it round-trip through PGN save/load and the AnnotationEditor for free; a game-level narrative can be stored in root.comment (root node has comment: "" per store.test.ts:55).
  - risk: Comments containing '{' or '}' break PGN; sanitize LLM output. Adding a new TreeNode field instead would require updating createNode/defaultTree, parsePGN, getPGN and every literal fixture in store.test.ts.
- **src-tauri/src/main.rs:182-188 (debug-only bindings export) and src/bindings/generated.ts (committed)**: Any new Rust command/event must be added to collect_commands!/collect_events!, then `pnpm dev` (debug build) run once to regenerate src/bindings/generated.ts, which is committed and excluded from lint/format.
  - risk: generated.ts drift is not detected by CI (no Rust job); forgetting to commit it leaves the frontend calling a nonexistent command at runtime only.

## Conventions
- Pre-PR command sequence (CONTRIBUTING.md:64-70, package.json:6-20): `pnpm build` (manual check), `pnpm format` (oxfmt), `pnpm lint:fix` (oxlint --fix), `pnpm i18n:extract` if keys changed; CI enforces `pnpm lint:ci` = `tsgo --noEmit && oxfmt --check && oxlint --format=github && i18next-cli extract --ci` and `pnpm test` = `vitest run` (test.yml:33-36) on every push and PR to any branch.
- Formatting: oxfmt defaults; .editorconfig gives 4-space indent in .ts and 2-space in .tsx/.json (observed: src/state/atoms.ts 4-space, src/App.tsx 2-space); ~100-char wrap (src/utils/tests/score.test.ts:25-27); double quotes, semicolons, trailing commas; LF.
- Linting: oxlint default (correctness) categories with plugins eslint/import/react/react-perf/oxc/typescript/vitest (.oxlintrc.json:3); generated files ignored; only 2 disable comments in the tree (routeTree.gen.ts:1, misc.ts:81 — stale react-hooks reference), 4 `@ts-expect-error`, 10 `as any` outside generated.ts — keep new code free of suppressions.
- Typechecking uses tsgo (TypeScript native preview) with strict mode; path alias `@/` for all intra-src imports (e.g. `import { commands } from "@/bindings"`), relative imports only for siblings.
- React: function components, default-exported PascalCase in PascalCase files (`export default memo(ReportModal)`), React Compiler enabled (vite.config.ts:19-21) but code still uses memo/useMemo/useCallback explicitly; `"use no memo"` directive is the opt-out (src/components/Sidebar.tsx:1).
- State: jotai `atomWithStorage` with kebab-case storage keys for persisted preferences (src/state/atoms.ts; atoms suffixed `Atom`), zod-validated via createZodStorage/createAsyncZodStorage (src/state/utils.ts:29,61); feature-local persisted settings may be colocated (ReportModal.tsx:13 reportSettingsAtom); zustand vanilla store + immer `produce` + `persist` (debounced sessionStorage) for the per-tab game tree, accessed as `const store = useContext(TreeStateContext)!; const x = useStore(store, (s) => s.x)` (ReportPanel.tsx:24-29).
- Errors: Tauri commands return specta `Result<T, string>`; callers either branch on `status === "ok"` (ReportModal.tsx:97) or call `unwrap()` (src/utils/unwrap.tsx) which logs with @tauri-apps/plugin-log `error()` and shows `notifications.show({ title: "Error", message, color: "red", icon: <IconX/> })`; @badrap/result appears in one file only (src/utils/files.ts) and is not the norm. Rust: one `thiserror` enum with `Box`ed `#[error(transparent)]` variants, manual `From` impls, `Serialize` as string (src-tauri/src/error.rs).
- Branching: ts-pattern `match(x).with(...).otherwise(...)`/`.exhaustive()` (23 files, e.g. src/routes/__root.tsx:46, src/components/common/GoModeInput.tsx:38).
- Validation: zod schemas suffixed `Schema` with `z.infer` types (src/utils/engines.ts:35,69), used for persisted data and TanStack Router `validateSearch` (src/routes/engines.tsx:5-11).
- UI: Mantine 8 components only (133 files), `@mantine/form` useForm for forms (ReportModal.tsx:49), `@mantine/notifications`, `@tabler/icons-react` icons sized like `size="0.875rem"`, CSS modules `X.module.css` imported as `classes` with plain lowercase/camelCase class names (AnalysisPanel.module.css `.label`), `clsx` imported as `cx`.
- i18n: `const { t } = useTranslation()` per component; keys are flat PascalCase dotted `Area.Sub.Key`, descriptions `Key.Desc`, plurals `Key_one`/`Key_other`, interpolation `{{name}}`; occasional `t(key, { defaultValue })` (6 uses, __root.tsx:166-168); dynamic key families must be listed in i18next.config.ts preservePatterns; only en-US must be complete (fallbackLng en-US).
- HTTP: `fetch` from `@tauri-apps/plugin-http` with `apiHeaders()` User-Agent (src/utils/lichess/api.tsx:5,20; src/utils/http.ts:17); hosts must be allow-listed in src-tauri/capabilities/main.json:42-53; no axios/SDK clients.
- Logging: `info/warn/error` from `@tauri-apps/plugin-log` (App.tsx:15, state/utils.ts:2), never console.log.
- Routing: TanStack Router file routes in src/routes/*.tsx exporting `Route = createFileRoute(path)({ component, loader?, validateSearch? })`, generated src/routeTree.gen.ts committed; pages live in src/components/<area>/<Area>Page.tsx.
- Tests: vitest, `import { expect, test } from "vitest"` (no globals, no describe), flat `test("should ...")` names, files in src/utils/tests/<module>.test.ts importing `../<module>` or `@/...`; literal fixture objects; store tests reset with `beforeEach(() => store.setState(defaultTree()))`; 40 tests total, pure logic only, no component/DOM tests despite jsdom env. Rust `#[cfg(test)] mod tests { use super::*; ... }` with helper fns (src-tauri/src/chess.rs:710-755), not run in CI.
- Rust: tauri-specta commands registered in `collect_commands!` (main.rs:111-172), events in `collect_events!`; bindings exported in debug builds to src/bindings/generated.ts and committed; no rustfmt.toml/clippy.toml (defaults).
- Commit/PR style (one visible commit, #902): imperative capitalized subject without type prefix, optional one-sentence body, PR number appended by squash merge; branch off `master`; no PR template/CODEOWNERS/CLA/DCO; translation-only PRs are accepted separately.
- Licensing: GPL-3.0-only (Cargo.toml:6), no SPDX headers, no copyright assignment; third-party deps are permissive (MIT/Apache) and GPL-compatible.
- Telemetry: PostHog, default ON (opt-out) with a single `app_started` event; toggle under Settings > Privacy; no other analytics calls exist in src.

## Gotchas
- `pnpm i18n:extract` rewrites all 16 locale files (sort: true, removeUnusedKeys: true, i18next.config.ts:32-42); CI's `i18next-cli extract --ci` fails if the committed files differ from extraction output, so extraction output must be committed. Template-literal keys (t(`Commentary.${x}`)) are invisible to the extractor and get pruned unless added to preservePatterns (:34-41).
- There are no typed i18n keys (src/i18next.d.ts and src/types/resources.d.ts are configured at i18next.config.ts:44-48 but not committed); a mistyped key renders as the raw key string. `returnEmptyString: false` (src/index.tsx:54) means empty translations fall back to en-US.
- `oxfmt --check` is a CI gate; always run `pnpm format` (indentation differs between .ts (4) and .tsx (2) per .editorconfig).
- CI never builds or tests Rust (test.yml only runs pnpm lint:ci/test); Rust changes and src/bindings/generated.ts drift are only caught locally (`pnpm dev` regenerates bindings in debug builds, main.rs:182-188) or at tag-release time.
- Vitest uses jsdom for everything but there are no mocks: any module importing @tauri-apps/* or @mantine/notifications (e.g. src/utils/unwrap.tsx) is untestable without vi.mock; keep commentary logic in Tauri-free pure modules.
- Outbound HTTP from the webview is restricted by the Tauri capability allow-list (src-tauri/capabilities/main.json:42-53); calling api.anthropic.com requires editing that file (Rust rebuild) or routing through a Rust reqwest command. CSP is null (tauri.conf.json:84).
- Secrets precedent is weak: Lichess accessToken is kept in localStorage via sessionsAtom (src/state/atoms.ts:213, src/utils/session.ts:5); there is no keychain/stronghold plugin. Help > Clear saved data wipes localStorage/sessionStorage (src/routes/__root.tsx:284-296).
- Telemetry default is opt-OUT (`telemetryEnabledAtom` default true, atoms.ts:215) and posthog.init runs even when disabled (App.tsx:167-173, no opt_out_capturing_by_default); an LLM feature that ships game data should be explicit opt-in to avoid a privacy regression reviewers will notice.
- React Compiler is active; code that mutates objects in place (ReportModal.tsx:143-145 mutates form.values.goMode) is an existing smell — do not copy it.
- Tree store is persisted to sessionStorage only (debouncedStorage.ts:15) and tabs/activeTab to sessionStorage (atoms.ts:58-68): generated commentary stored in node.comment survives tab switches but not app restarts unless the PGN is saved (dirty flag / AutoSave setting).
- PGN comments are emitted raw inside `{...}` (src/utils/chess.ts:117-120); braces in generated text must be sanitized.
- src/bindings/generated.ts and src/routeTree.gen.ts are ignored by lint/format and must not be hand-edited; src/bindings/index.ts is where types are widened (ScoreValue adds "dtz").
- `tsgo` (TypeScript 7 native preview, pinned dev build 7.0.0-dev.20260306.1) is required for `pnpm lint`/`build-vite`; behaviour can differ from tsc.
- Vite 8 (rolldown) + @rolldown/plugin-babel: adding Babel/Vite plugins is non-standard here.
- Release artifacts are signed with the maintainer's minisign key and served from encroissant.org/updates (tauri.conf.json:61-66); a fork must replace pubkey/endpoints (or drop the updater), change `identifier` org.encroissant.app (AppData dir is used by fileStorage, state/utils.ts:12) and set its own TAURI_SIGNING_PRIVATE_KEY secrets, otherwise fork builds will try to update to upstream releases.
- No CHANGELOG; release notes are the generic 'See the assets to download…' (release.yml:80); releases are created as drafts and published manually.
- A stale `eslint-disable-next-line react-hooks/exhaustive-deps` at src/utils/misc.ts:81 suggests the project migrated from biome/eslint to oxlint; the react-hooks plugin is not in .oxlintrc.json, so hook-deps rules may not be enforced.
- The Linux release job pins exact apt versions of libwebkit2gtk 2.44.0-2 (release.yml:33-42); not a contributor concern but it breaks when Ubuntu updates.

## Open questions
- Exact failure semantics of `i18next-cli extract --ci` for the pinned version 1.42.12 (pnpm-lock.yaml:2581): assumed to exit non-zero when extraction would modify any locale file; could not be run (tooling must not be executed in the clone).
- Whether `i18next-cli extract` with `defaultValue: null` adds new keys to non-primary locales or omits them: empirical evidence (secondary locales have fewer keys than en-US and zero null values, e.g. de-DE 596/610) suggests omission, i.e. contributors only author en-US, but this was not verified by running the tool.
- Whether oxfmt reads .editorconfig to produce the observed 4-space .ts / 2-space .tsx indentation; .oxfmtrc.json sets nothing but ignores. Run `pnpm format` rather than hand-formatting.
- Which oxlint rules are active under `categories: {}` with oxlint 1.55 defaults (and whether react-hooks rules are included); could not execute the linter.
- Commit message conventions beyond the single visible commit (shallow clone; GitHub API access to franciscoBSalgueiro/en-croissant was refused in this session, so the last 40 upstream commits could not be sampled). Squash-merge is inferred from the '(#902)' suffix.
- Maintainers' stance on LLM/third-party-API features and on new network hosts in capabilities; CONTRIBUTING.md:17 says to check issues/Discord first because 'perhaps there is a reason that feature isn't implemented'.
- Whether a user-configurable provider base URL is acceptable given the host allow-list in src-tauri/capabilities/main.json (a wildcard would likely be rejected); a Rust-side reqwest command may be the only clean way to support arbitrary OpenAI-compatible endpoints.
- Whether src/i18next.d.ts typing is intentionally not committed (it would make t() keys type-checked and would catch key typos at `tsgo --noEmit`).
- Rust formatting/lint expectations (no rustfmt.toml/clippy.toml, no CI job): assume `cargo fmt` defaults and clippy-clean, but unverified.
- Whether posthog-js performs any network request on `posthog.init` when telemetry is disabled (App.tsx:167-173 does not pass opt_out_capturing_by_default); relevant if the architect wants to position the LLM feature's privacy story against existing behaviour.
- ReportState type definition (TreeState.report) was only observed via test fixtures `{ inProgress: boolean }` (store.test.ts:57-59); the declaration site in src/utils/treeReducer.ts was not read.
