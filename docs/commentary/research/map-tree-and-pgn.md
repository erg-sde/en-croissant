# Game tree, annotation model, and PGN round-trip (TS tree store + Rust lexer/pgn writer + SQLite move-blob encoding) in en-croissant v0.15.1. Repo root (absolute): /tmp/claude-0/-home-user-Chess/095b4a5e-49ff-5938-b919-e82f3408c9dc/scratchpad/upstream/en-croissant — all paths below are relative to it.

## Summary
The game is an in-memory tree of `TreeNode` (src/utils/treeReducer.ts:16-28) held in a per-tab zustand vanilla store created by `createTreeStore(id, initialTree)` (src/state/store/tree.ts:95-526) and provided through React context `TreeStateContext` (src/components/common/TreeStateContext.tsx:5-19). All mutations go through store actions implemented with immer `produce`; every mutating action sets `state.dirty = true`. There is no undo/redo and no history middleware; the only persistence of in-flight state is zustand `persist` into sessionStorage via a 300 ms debounced adapter (src/state/store/debouncedStorage.ts) keyed by the tab id (tree.ts:516-523). Per-node data is: `fen`, `move` (chessops Move), `san`, `children`, `score: Score | null` (engine eval of the position after the move), `depth` (never written anywhere — always null), `halfMoves`, `shapes: DrawShape[]` (chessground arrows/circles), `annotations: Annotation[]` (NAG glyph strings), `comment: string` (markdown), `clock?: number` (seconds). The root node is a real node (fen = start position, move null) and can carry a comment/score/shapes; a PGN comment before the first move lands on root and is rendered as a game-level paragraph (src/components/common/GameNotation.tsx:112-116).

PGN in: `parsePGN(pgn)` (src/utils/chess.ts:460-476) calls the Rust Tauri command `lex_pgn` (src-tauri/src/lexer.rs:66-76) which uses `pgn_reader` to emit a flat `Token` stream (ParenOpen/ParenClose/Comment(raw string)/San/Header{tag,value}/Nag("$n")/Outcome). The Rust lexer never interprets comment commands; `innerParsePGN` (chess.ts:353-458) does, via chessops `parseComment`: `[%eval]` -> `root.score` (:370-388), `[%csl]/[%cal]` -> `root.shapes` (:390-397), `[%clk]` -> `root.clock` (:399-401), remaining text with `[%timestamp N]` regex-stripped -> `root.comment` (:404). NAG tokens map through `NAG_INFO` (src/utils/annotation.ts:40-73) into `annotations`, sorted by `ANNOTATION_INFO[..].nag` (:423-427). PGN out: `getPGN(root, {headers, glyphs, comments, variations, extraMarkups, path})` (chess.ts:218-306) recursively calls `getMoveText` (chess.ts:48-127), which emits glyph suffix / `$n` NAGs (:70-77) and one `{...}` block containing `[%eval]`, `[%clk]`, `[%csl]`, `[%cal]` (gated by `extraMarkups`, :84-115) followed by the raw `tree.comment` (gated by `comments`, :117-119). Headers come from `headersToPGN` (:172-212) including a passthrough `other: Record<string,string>` for unknown tags.

Saving: `saveToFile` (src/utils/tabs.ts:150-240) serialises the whole tree with all flags true (:171-177) and either calls `commands.writeDbGame` for `gameOrigin.kind === "database"` (:179-183; Rust `write_db_game` src-tauri/src/db/mod.rs:1790-1860 re-imports the PGN through the DB `Importer` visitor and updates the `Games` row incl. the binary `moves` blob, where comments/NAGs/variations are stored verbatim with marker bytes, src-tauri/src/db/encoding.rs:7-36) or `commands.writeGame(filePath, gameNumber, pgn)` (:238; Rust `write_game` src-tauri/src/pgn.rs:243-279 rewrites the whole .pgn file through a tempfile, replacing game n by byte offset). After either, `store.save()` clears `dirty`. Autosave (`autoSaveAtom`, default true, src/state/atoms.ts:185) runs the save in an effect whenever `hasPersistentOrigin && autoSave && dirty` (src/components/boards/BoardAnalysis.tsx:82-86), i.e. after every store mutation including each keystroke of the comment editor. Opening a DB game feeds `NormalizedGame.moves` (decoded movetext incl. `{comments}`, mod.rs:1751-1759 / encoding.rs:316-324) back into `parsePGN` via `createTab` (src/utils/tabs.ts:96-142; src/components/databases/GameCard.tsx:40-54). Reloading another game from a file (`InfoPanel` src/components/panels/info/InfoPanel.tsx:172-195) or pasting PGN (src/components/panels/info/PgnInput.tsx:94-104) replaces the tree with `setState(parsePGN(...))`, so any node field not round-tripped through PGN text is lost.

Editing/rendering comments: the Annotate tab (BoardAnalysis.tsx:225-245) mounts `AnnotationPanel` (symbol toggle buttons calling `setAnnotation`, src/components/panels/annotation/AnnotationPanel.tsx:22-47, 51-53) above `AnnotationEditor`, a Mantine `RichTextEditor` over tiptap with `StarterKit` + `@tiptap/markdown` + `Placeholder`; it loads `currentNode.comment` as `contentType: "markdown"` and writes back `editor.getMarkdown()` through `setComment` on every update (src/components/panels/annotation/AnnotationEditor.tsx:24-44). The move list renders comments with `Comment` (src/components/common/Comment.tsx) = `react-markdown` with `rehypeRaw` (raw HTML allowed) and GFM, inline `<span>` for one-line comments, block otherwise. Engine-report flow: `ReportPanel` (src/components/panels/analysis/ReportPanel.tsx) -> `ReportModal.analyze()` (src/components/panels/analysis/ReportModal.tsx:72-104) -> Rust `analyze_game` (src-tauri/src/chess.rs:474-640, forces MultiPV=2, computes `is_sacrifice` via naive eval drop >100) -> `MoveAnalysis[]` (one per mainline position incl. root, skipping game-over positions) -> `addAnalysis` (tree.ts:693-809) which writes `score`, computes glyphs with `getAnnotation` (src/utils/score.ts:75-119), appends the engine's best line as a sibling variation (up to 10 plies) for ?!/?/?? moves when `showVariations`, and adds "N" for novelties. Live analysis writes only the current node's `score` through `EvalListener` (src/components/boards/EvalListener.tsx:158-162); live multi-PV lines live in the jotai `engineMovesFamily` map (atoms.ts:544-548) and are never persisted into the tree.

## Key files
- `src/utils/treeReducer.ts` — Core data model: TreeState/TreeNode/GameHeaders/ReportState types, tree iterators, defaultTree/createNode factories, getNodeAtPath.
  - interface TreeState :8-14 (root, headers, position:number[], dirty, report)
  - interface TreeNode :16-28
  - type ListNode :30-33
  - treeIterator :35-44 (DFS yields {position,node})
  - findFen :46-54
  - treeIteratorMainLine :56-65
  - countMainPly :67-75
  - defaultTree(fen?) :77-108 (root node + default headers + report{inProgress:false})
  - createNode({fen,move,san,halfMoves,clock}) :110-136 (clock ms->s at :127)
  - type GameHeaders :138-160 (other?: Record<string,string> :156; start?/orientation? repertoire fields :158-159)
  - getGameName :162-170
  - getNodeAtPath :172-181 (returns deepest valid node on bad path)
  - getTreeStructureHash :183-194
  - interface ReportState :196-198
- `src/state/store/tree.ts` — Zustand store (per tab) with all reducer actions; immer produce; persist to sessionStorage when id given.
  - interface TreeStoreState :26-91 (action signatures; setAnnotation :63, setComment :64, setShapes :67, setScore :68, addAnalysis :74-81, setReportInProgress :83, setState :88, reset :89, save :90)
  - createTreeStore(id?, initialTree?) :95-526
  - save (dirty=false) :114-119
  - setFen :121-128
  - goToAnnotation :158-184 (while(true) scan of mainline; loops forever if glyph absent)
  - makeMove/appendMove/makeMoves :186-239
  - deleteMove/promoteVariation/promoteToMainline :367-391
  - copyVariationPgn :392-403 (comments:false, extraMarkups:false)
  - copyPgn :404-414 (all flags true)
  - setAnnotation :422-442 (toggle; evicts same `group`; sorts by nag)
  - setComment :443-452 (current position only)
  - setHeaders :453-463 (resets tree if fen changes)
  - setShapes :471-477 -> setShapes helper :673-691 (toggle one shape, or clear on empty)
  - setScore :478-487
  - addAnalysis :488-494 -> addAnalysis helper :693-809
  - setReportInProgress :496-502
  - clearShapes :504-513
  - persist(stateCreator,{name:id,storage:createDebouncedSessionStorage()}) :516-523
  - makeMove helper :528-609 (dedupes by san :574; sets result on mate/stalemate/3-fold/50-move :559-572)
  - isThreeFoldRepetition :615-633
  - deleteMove helper :640-657
  - promoteVariation helper :659-671
- `src/utils/chess.ts` — PGN serialisation (getPGN/getMoveText/headersToPGN) and parsing (parsePGN/innerParsePGN/getPgnHeaders) plus game stats/material helpers.
  - interface BestMoves :21-28 (TS copy; bindings has nodes too)
  - interface MoveAnalysis :30-34
  - makeClk :37-46
  - getMoveText(tree,{glyphs,comments,extraMarkups,isFirst}) :48-127 (NAG emit :70-77; [%eval] :85-90; [%clk] :92-94; [%csl] :101-107; [%cal] :108-114; raw comment :117-119)
  - getLastMainlinePosition :129-137
  - getMainLine :139-141 (UCI list for analyze_game)
  - getVariationLine :148-170
  - headersToPGN :172-212 (other tags :208-210)
  - defaultPGN :214-216
  - getPGN :218-306 (root comment :250-256; variations :257-275, 285-289; path= mainline-only export)
  - innerParsePGN(tokens,fen,halfMoves) :353-458 (Comment :367-404; ParenOpen variation recursion :405-421; Nag :423-427; San :428-452; Outcome :453-455)
  - parsePGN(pgn, initialFen?) :460-476 (async; IPC lexPgn)
  - getPgnHeaders :478-535 (known tags destructured :490-506; rest -> other :532)
  - getGameStats :541-612 (per-colour glyph counts, ACPL, accuracy from mainline scores)
  - getMaterialDiff :622-654
  - stripClock :656-658
  - hasMorePriority :660-672
- `src/utils/annotation.ts` — Annotation (NAG glyph) union, NAG<->glyph tables, per-glyph metadata (name/translationKey/color/group/nag).
  - addPieceSymbol :5-10
  - type Annotation :12-38 (26 glyph strings incl. "")
  - NAG_INFO Map<"$n",Annotation> :40-73 (lossy: $22/$23->⨀, $32/$33->↑↑, etc.)
  - type AnnotationInfo :75-81 {group?,name,translationKey?,color?,nag}
  - ANNOTATION_INFO :83-190 (groups: basic :85-126, advantage :127-174; extras :175-189)
  - isBasicAnnotation :192-196
- `src/utils/score.ts` — Eval maths used by report: win chance, accuracy, CP loss, glyph classification.
  - INITIAL_SCORE :7-13
  - formatScore(score, precision=2) :17-32 ("+0.50", "+M5")
  - getWinChance :34-36
  - normalizeScore :38-47 (mate->±1000cp, clamp)
  - getAccuracy :60-67
  - getCPLoss :69-73
  - getAnnotation(prevprev, prev, next, color, prevMoves, is_sacrifice?, move?) :75-119 (??>20, ?>10, ?!>5 win% drop; !!/! when played best move and 2nd best was >10 worse)
- `src-tauri/src/lexer.rs` — Rust PGN tokenizer (pgn_reader Visitor) exposed as Tauri command lex_pgn; emits raw comment strings, no command parsing.
  - enum Token :13-21 (#[serde(tag="type",content="value")])
  - impl Visitor for Lexer :23-64 (san :26, header :30, nag :36, begin/end_variation :40-47, comment :49-53 raw bytes lossy-utf8, outcome :59-63)
  - lex_pgn(pgn) :66-76 (single game: reader.read_game)
- `src-tauri/src/pgn.rs` — Byte-offset based multi-game .pgn file reader/writer (count/read/delete/write game n).
  - struct PgnParser :11-16
  - offset_by_index :34-60 (uses AppState.pgn_offsets every 100 games)
  - skip_games :63-107 (header-line heuristic, tracks { } to ignore '[' inside comments)
  - read_game :109-141
  - count_pgn_games :154-183
  - read_games(file,start,end) :185-209
  - delete_game :211-234
  - write_game(file_path, n, pgn) :243-279 (tempfile copy, splice, truncate)
- `src-tauri/src/db/encoding.rs` — SQLite Games.Moves binary encoding: legal-move index bytes plus marker bytes for variations/comments/NAGs; decoder renders movetext with {comments} and glyphs.
  - VARIATION_START_MARKER=255 / VARIATION_END_MARKER=254 / COMMENT_MARKER=253 / NAG_MARKER=252 :7-10
  - encode_move :12-15
  - encode_comment :22-28 (u16 LE length-prefixed chunks, raw bytes)
  - encode_nag :30-36
  - enum DecodedGameNode (Comment(String) :106)
  - render_nodes NAG :290-314 ($1-$6 -> glyph suffix, others '$n'), Comment :316-324, Variation :325-335
  - decode_game_to_movetext :341-345
- `src-tauri/src/db/mod.rs` — DB import visitor and commands; write_db_game re-imports a PGN string to update one game row; get_games decodes movetext into NormalizedGame.moves.
  - impl Visitor for Importer :321 (san :414-447, begin_variation :449-465, end_variation :467-478, comment :480-483 -> encode_comment, nag :485-487 -> encode_nag)
  - NormalizedGame/PgnGame construction with moves: decode_game_to_movetext :1736-1760
  - write_db_game(file, game_id, pgn) :1790-1860 (Importer::new(None); diesel update incl. games::moves :1850)
- `src-tauri/src/db/schema.rs` — Diesel schema; note the declared-but-unused Comments table.
  - games table :43-85 (Moves -> Binary :80-81, FEN :78-79)
  - comments table (ID, GameID, Ply, Comment) :87-99 — no references anywhere else in src-tauri (not in create.sql, not in ops)
- `src-tauri/src/chess.rs` — Engine analysis types and analyze_game command used by Generate report.
  - struct MoveAnalysis { best: Vec<BestMoves>, novelty: bool, is_sacrifice: bool } :448-453
  - struct AnalysisOptions { fen, moves, annotate_novelties, reference_db, reversed } :455-463 (camelCase)
  - cancel_analysis :465-472
  - analyze_game(id, engine, go_mode, options, uci_options) :474-640 (is_sacrifice heuristic :512-519; forces MultiPV=2 :545-557; per-position loop :530-599; novelty via reference DB :615-626)
- `src/components/panels/annotation/AnnotationEditor.tsx` — tiptap/Mantine rich text editor bound to currentNode.comment as markdown.
  - AnnotationEditor :14-99
  - useEditor({extensions:[StarterKit(link autolink), Markdown, Placeholder], content: currentNode.comment||null, contentType:'markdown', onUpdate: setComment(editor.getMarkdown())}, [position.join(',')]) :24-44
  - Toolbar controls :68-94 (Bold, Italic, Underline, Strikethrough, ClearFormatting, H1-H4, Blockquote, Hr, BulletList, OrderedList, Link, Unlink)
- `src/components/panels/annotation/AnnotationPanel.tsx` — Annotate tab: glyph toggle buttons + editor.
  - SymbolButton :22-47 (setAnnotation toggle; tooltip t(`Annotate.${translationKey}`))
  - BASIC/ADVANTAGE/EXTRA glyph groups :51-53
  - AnnotationPanel :55-121 (editor mounted :116-118)
- `src/components/boards/AnnotationHint.tsx` — On-board glyph badge for currentNode.annotations[0] at the destination square.
  - AnnotationHint({square, annotation, orientation}) :7-59
  - glyphToSvg: Record<Annotation, ReactNode> :63-265 (lichess SVG paths)
- `src/components/boards/Board.tsx` — Board; renders AnnotationHint and wires chessground drawable shapes to setShapes.
  - square = match(currentNode) O-O/O-O-O/move.to :367-370
  - AnnotationHint render gated by showComments && annotations.length>0 :430-443
  - drawable.autoShapes = shapes, onChange -> setShapes :581-597 (custom brush 'variation' key 'v' :586-593)
- `src/components/common/GameNotation.tsx` — Move list (inline tree view and table view), root comment, show-comments/variations toggles.
  - GameNotation :49-146 (rootComment render :112-116; result footer :126-138)
  - NotationHeader :148-186 (toggles currentInvisibleAtom, tableViewAtom, currentShowCommentsAtom, currentShowVariationsAtom)
  - RenderVariationTree (memo, recursive) :188-259 (passes annotations/comment props to CompleteMoveCell :212-243)
  - RowItem/VariationItem/CommentItem/Segment types :261-279
  - TableNotation :281-509 (segment builder :291-444; comment rows :450-460; variation rows :462-494)
  - RowSegment :511-575 (scoreText = formatScore(node.score.value,1) when showComments :545,:565)
  - VariationCell :577-593
- `src/components/common/CompleteMoveCell.tsx` — One move cell: number, MoveCell, context menu, transposition link, trailing Comment.
  - getTranspositionMap/getTranspositions :28-57 (WeakMap cache per root)
  - CompleteMoveCell({movePath, halfMoves, move, fen, comment, annotations, showComments, first, targetRef, tableLayout, scoreText}) :59-203
  - context Menu items :151-188 (MarkAsStart repertoire-only, PromoteToMainLine, PromoteVariation, CopyVariationPGN, DeleteMove)
  - comment render `{showComments && !tableLayout && comment && <Comment/>}` :200
  - memo comparator :205-218 (includes comment, annotations)
- `src/components/common/MoveCell.tsx` — Button for a SAN move coloured by first annotation; appends glyphs text.
  - MoveCellProps :9-19
  - visualAnnotation = showComments ? annotations[0] : '' :24; color from ANNOTATION_INFO :26
  - text = san (+piece symbols) + annotations.join('') :66-70
- `src/components/common/Comment.tsx` — Markdown renderer for comments (react-markdown + rehypeRaw + remarkGfm).
  - normalizeTiptapMarkdown (++u++ -> <u>) :8-10
  - Comment :12-35 (inline <span> when single line :14,:27; links target=_blank :26; rehypePlugins=[rehypeRaw, remarkGfm] :29)
- `src/components/panels/analysis/ReportModal.tsx` — Generate report dialog: engine/go-mode/options form; calls analyzeGame then addAnalysis.
  - reportSettingsAtom (atomWithStorage 'report-settings': novelty, reversed, variations, goMode, engine) :13-19
  - ReportModal props :21-35
  - analyze() :72-104 (commands.analyzeGame(`report_${tab}`, path, goMode, {annotateNovelties, fen, referenceDb, reversed, moves}, engineSettings) -> addAnalysis(data,{showVariations}) :82-103)
- `src/components/panels/analysis/ReportPanel.tsx` — Report tab inside the Analysis panel: progress button, accuracy cards, EvalChart, glyph stats table.
  - ReportPanel :19-96 (moves=getMainLine(root) :50; ProgressButton id `report_${activeTab}` :56-71)
  - GameStats :100-168 (click -> goToAnnotation)
  - AccuracyCard :170-191
- `src/components/panels/analysis/AnalysisPanel.tsx` — Analysis side panel with tabs engines/report/logs.
  - Tabs.Tab engines/report/logs :102-106
  - ReportPanel mounted :248-258
- `src/components/boards/BoardAnalysis.tsx` — Analysis board page: autosave, panel tabs (practice/analysis/database/annotate/info).
  - saveFile/userSaveFile -> saveToFile :65-81
  - autosave effect :82-86
  - addGame (append defaultPGN to file) :88-110
  - Tabs.Tab list :215-230; Tabs.Panel annotate -> AnnotationPanel :243-245; analysis -> AnalysisPanel :246-248
- `src/components/boards/EvalListener.tsx` — Subscribes to engine bestMovesPayload events; writes current node score; stores live PVs in jotai.
  - EvalListener :32-96
  - EngineListener :98-245 (setScore(ev[0].score) when first engine with lines :158-162; engineMovesFamily map key `${fen}:${moves.join(',')}` :149)
- `src/utils/tabs.ts` — Tab + GameOrigin zod schemas, createTab (parsePGN -> sessionStorage seed), saveToFile.
  - gameOriginSchema (none | file{file,gameNumber} | temp_file | database{database,gameId}) :37-56
  - tabSchema :58-63; type GameOrigin/Tab :65-66
  - getTabFile/getTabGameNumber/isPersistentGameOrigin :68-87
  - createTab :96-142 (sessionStorage.setItem(id, {version:0,state:tree}) :123)
  - saveToFile({dir,tab,setCurrentTab,store,isUserSave}) :150-240 (PGN flags :171-177; writeDbGame :179-183; writeGame :238; save() :239)
- `src/state/atoms.ts` — Jotai settings/UI atoms; per-tab atom families via tabValue().
  - tabsAtom :58; activeTabAtom :64; currentTabAtom :76
  - tableViewAtom :163; spellCheckAtom :172; autoSaveAtom ('auto-save', true) :185
  - sessionsAtom (atomWithStorage 'sessions' — Lichess accessToken lives in localStorage) :213
  - tabValue(family) helper :310-330
  - showCommentsFamily/currentShowCommentsAtom :368-369; showVariationsFamily :371-372; tabFamily/currentTabSelectedAtom :374-375; reportModalOpenFamily/currentReportModalOpenAtom :377-378
  - engineMovesFamily({tab,engine}) -> Map<string,BestMoves[]> :544-548
  - bestMovesFamily({fen,gameMoves}) :556-578
- `src/state/store/debouncedStorage.ts` — sessionStorage PersistStorage with 300ms debounce and flush on unload.
  - createDebouncedSessionStorage(delay=300) :52-74
- `src/bindings/generated.ts` — tauri-specta generated commands and types (do not edit; regenerated from Rust).
  - commands.analyzeGame :24-26
  - commands.writeGame :297-299
  - commands.getGames :329
  - type AnalysisOptions :490
  - type BestMoves :491
  - type MoveAnalysis :515
  - type NormalizedGame :516
  - type Outcome :519
  - type Score :534-538 / ScoreValue :539-547
  - type Token :553
- `src/utils/tests/store.test.ts` — Vitest store tests; full-state toStrictEqual fixtures with every TreeNode field spelled out.
  - treeE4D5 fixture :14-60
  - should handle setAnnotation :297-315
  - should handle setComment :317-335
  - should handle setShapes :383-410
  - should handle addAnalysis :415+
- `src/utils/phase.ts` — Lichess-derived opening/middlegame/endgame divider (useful for deterministic narrative).
  - type GamePhases {middlegamePly, endgamePly} :72-75
  - getGamePhases(boards: Chess[]) :77+
- `i18next.config.ts` — i18n extraction config: flat dotted keys (keySeparator:false), removeUnusedKeys, preservePatterns incl. Annotate.*.
  - extract.output 'src/translation/{{language}}.json'
  - keySeparator:false / nsSeparator:false
  - preservePatterns ['Files.FileType.*','SideBar.*','TimeControl.*','Annotate.*','GoMode.*','Errors.*']

## Data model
// src/utils/treeReducer.ts:8-14
export interface TreeState {
    root: TreeNode;
    headers: GameHeaders;
    position: number[];      // path of child indices from root to current node
    dirty: boolean;
    report: ReportState;     // { inProgress: boolean } :196-198
}

// src/utils/treeReducer.ts:16-28
export interface TreeNode {
    fen: string;
    move: Move | null;          // chessops Move
    san: string | null;
    children: TreeNode[];       // children[0] is the main line
    score: Score | null;        // eval of position AFTER this move (engine POV = white)
    depth: number | null;       // never assigned anywhere in src (always null)
    halfMoves: number;          // ply count; odd = white just moved
    shapes: DrawShape[];        // chessground DrawShape; code uses orig, dest?, brush
    annotations: Annotation[];  // sorted by ANNOTATION_INFO[a].nag
    comment: string;            // markdown (tiptap getMarkdown()); "" when none
    clock?: number;             // seconds remaining after the move
}

// src/utils/treeReducer.ts:138-160
export type GameHeaders = {
    id: number; fen: string; event: string; site: string;
    date?: string | null; time?: string | null; round?: string | null;
    white: string; white_elo?: number | null; black: string; black_elo?: number | null;
    result: Outcome; time_control?: string | null; white_time_control?: string | null;
    black_time_control?: string | null; eco?: string | null; variant?: string | null;
    other?: Record<string, string>;   // any unrecognised PGN tag, round-trips via headersToPGN :208-210
    start?: number[]; orientation?: "white" | "black";   // repertoire
};

// src/utils/annotation.ts:12-38
export type Annotation = "" | "!" | "!!" | "?" | "??" | "!?" | "?!" | "+-" | "±" | "⩲" | "=" | "∞" | "⩱" | "∓" | "-+" | "N" | "↑↑" | "↑" | "→" | "⇆" | "=∞" | "⊕" | "∆" | "□" | "⨀" | "⊗";
// src/utils/annotation.ts:75-81
type AnnotationInfo = { group?: string; name: string; translationKey?: string; color?: MantineColor; nag: number };
export const ANNOTATION_INFO: Record<Annotation, AnnotationInfo>;   // :83-190
export const NAG_INFO = new Map<string, Annotation>([["$1","!"],["$2","?"],["$3","!!"],["$4","??"],["$5","!?"],["$6","?!"],["$7","□"],["$9","⊗"],["$10","="],["$13","∞"],["$14","⩲"],["$15","⩱"],["$16","±"],["$17","∓"],["$18","+-"],["$19","-+"],["$22","⨀"],["$23","⨀"],["$32","↑↑"],["$33","↑↑"],["$36","↑"],["$37","↑"],["$40","→"],["$41","→"],["$44","=∞"],["$45","=∞"],["$132","⇆"],["$133","⇆"],["$138","⊕"],["$139","⊕"],["$140","∆"],["$146","N"]]);  // :40-73

// src/bindings/generated.ts:534-547
export type Score = { value: ScoreValue; wdl: [number, number, number] | null }
export type ScoreValue = { type: "cp"; value: number } | { type: "mate"; value: number }
// :491
export type BestMoves = { nodes: number; depth: number; score: Score; uciMoves: string[]; sanMoves: string[]; multipv: number; nps: number }
// :515
export type MoveAnalysis = { best: BestMoves[]; novelty: boolean; is_sacrifice: boolean }
// :490
export type AnalysisOptions = { fen: string; moves: string[]; annotateNovelties: boolean; referenceDb: string | null; reversed: boolean }
// :519
export type Outcome = "1-0" | "0-1" | "1/2-1/2" | "*"
// :553
export type Token = { type: "ParenOpen" } | { type: "ParenClose" } | { type: "Comment"; value: string } | { type: "San"; value: string } | { type: "Header"; value: { tag: string; value: string } } | { type: "Nag"; value: string } | { type: "Outcome"; value: string }
// :516
export type NormalizedGame = { id: number; fen: string; event: string; event_id: number; site: string; site_id: number; date?: string | null; time?: string | null; round?: string | null; white: string; white_id: number; white_elo?: number | null; black: string; black_id: number; black_elo?: number | null; result: Outcome; time_control?: string | null; eco?: string | null; ply_count?: number | null; moves: string /* decoded movetext incl. {comments}, NAGs, (variations) */ }

// src-tauri/src/lexer.rs:11-21
#[derive(Serialize, Clone, Type)] #[serde(tag = "type", content = "value")]
pub enum Token { ParenOpen, ParenClose, Comment(String), San(String), Header { tag: String, value: String }, Nag(String), Outcome(String) }

// src-tauri/src/chess.rs:448-463
#[derive(Serialize, Debug, Default, Type)] pub struct MoveAnalysis { best: Vec<BestMoves>, novelty: bool, is_sacrifice: bool }
#[derive(Deserialize, Debug, Default, Type)] #[serde(rename_all = "camelCase")] pub struct AnalysisOptions { pub fen: String, pub moves: Vec<String>, pub annotate_novelties: bool, pub reference_db: Option<PathBuf>, pub reversed: bool }

// src-tauri/src/db/encoding.rs:7-10 (Games.Moves blob markers)
pub const VARIATION_START_MARKER: u8 = 255; pub const VARIATION_END_MARKER: u8 = 254; pub const COMMENT_MARKER: u8 = 253; pub const NAG_MARKER: u8 = 252;
// comment = COMMENT_MARKER, u16 LE len, raw utf8 bytes (chunked at 65535)

// src/utils/tabs.ts:37-66
GameOrigin = { kind: "none" } | { kind: "file"; file: FileMetadata; gameNumber: number } | { kind: "temp_file"; file: FileMetadata; gameNumber: number } | { kind: "database"; database: string; gameId: number }
Tab = { name: string; value: string /* id, also sessionStorage key */; type: "new" | "play" | "analysis" | "puzzles"; gameOrigin: GameOrigin }

// src/components/common/GameNotation.tsx:261-279 (table view segments)
type RowItem = { type: "row"; moveNumber: number; white: TreeNode | null; whitePath: number[]; black: TreeNode | null; blackPath: number[]; splitRow?: boolean };
type VariationItem = { type: "variations"; variations: TreeNode[]; parentPath: number[] };
type CommentItem = { type: "comment"; comment: string };

// PGN comment block emitted by getMoveText (src/utils/chess.ts:82-124), all in ONE {…}:
// "{[%eval +0.35] [%clk 0:05:12] [%csl Ge4,Rd5][%cal Gg1f3]<raw markdown comment>} "
// Parsed by chessops parseComment (chess.ts:368): evaluation -> score, shapes -> shapes, clock -> clock, text (minus [%timestamp N]) -> comment.

## Extension points
- **src/utils/treeReducer.ts:16-28 (interface TreeNode) + defaultTree :83-94 + createNode :123-135**: Add an optional structured field for machine commentary, e.g. `commentary?: { text: string; source: "llm" | "rules"; provider?: string; model?: string; createdAt: number; inputHash: string }` (optional so existing sessionStorage-persisted trees and the store.test.ts fixtures that use toStrictEqual keep working). Keep the human `comment` untouched so user prose and generated prose never clobber each other.
  - risk: Any new node field that is not written into PGN text is lost whenever the tree is rebuilt from PGN: InfoPanel.setPage (src/components/panels/info/InfoPanel.tsx:179-181), PgnInput.updatePgn (:95-102), createTab (src/utils/tabs.ts:116), and DB reopen (GameCard.tsx:47). Fixtures in src/utils/tests/store.test.ts:14-60 spell out every field, so a required field forces touching every fixture.
- **src/utils/chess.ts:367-404 (innerParsePGN Comment branch) and src/utils/chess.ts:81-125 (getMoveText comment block)**: Round-trip commentary through PGN. Option A (zero format risk): write it into the ordinary comment text (prefix/separator), i.e. `comment` is the persistence. Option B (keeps separation): emit a custom command such as `[%cmt source=llm ...]` or `[%commentary ...]` inside the same `{}` block under the `extraMarkups` flag at :115, and in innerParsePGN strip/parse it from `comment.text` exactly like the `[%timestamp]` regex at :404 (chessops leaves unknown `[%x]` commands in `text`). Game-level narrative already has a natural home: a comment before the first move attaches to `tree.root` (:367, root is initially tree.root) and is rendered by GameNotation.tsx:112-116 and emitted by getPGN :250-256.
  - risk: No escaping of `}` anywhere (getMoveText :117-119); LLM text containing `}` corrupts the PGN and the Rust pgn.rs offset scanner (:84-91 tracks braces). Non-standard `[%...]` commands show as literal text in other tools. Maintainers may prefer plain comments; the `[%timestamp]` strip shows they already tolerate vendor commands on input only. Variation-leading comments are dropped (:418-421 keeps only newTree.root.children[0]).
- **src/state/store/tree.ts:443-452 (setComment) and :693-809 (addAnalysis) — add actions in TreeStoreState :26-91**: Add path-addressed and bulk actions, e.g. `setCommentAt(path: number[], comment: string)` and `applyCommentary(entries: { path: number[]; ... }[])` executed inside a single `set(produce(...))` so the whole game is annotated in one state transition (pattern: addAnalysis walks `cur = cur.children[0]` with index `i` aligned to MoveAnalysis[]). Existing setComment only targets `state.position`, which is unusable for background generation while the user navigates.
  - risk: Every produce() sets dirty and, with autoSaveAtom (default true), BoardAnalysis.tsx:82-86 rewrites the entire .pgn file (src-tauri/src/pgn.rs:243-279 copies the whole file to a tempfile) or re-imports into SQLite on each call — per-move writes would be O(moves × file size). No undo exists, so a bulk overwrite of `comment` is irreversible; prefer writing to a separate field or appending.
- **src/components/panels/analysis/ReportModal.tsx:72-104 (analyze → addAnalysis) and reportSettingsAtom :13-19**: After `addAnalysis(analysis.data, …)` resolves, run the commentary generator with `analysis.data` (MoveAnalysis[] aligned to mainline positions, MultiPV=2 best lines with sanMoves/uciMoves, is_sacrifice, novelty) plus the updated tree (scores, glyphs, auto-inserted best-move variations). Add `commentary: boolean` / provider choice to `reportSettingsAtom` (atomWithStorage) and a form checkbox next to :166-176. Reuse `setInProgress` / ProgressButton id `report_${tab}` (ReportPanel.tsx:56-71) or add a second progress stage.
  - risk: analyze_game skips game-over positions (src-tauri/src/chess.rs:512) so `analysis.length` can be shorter than the mainline; addAnalysis bounds on analysis.length (:706). Analysis may be cancelled (`cancel_analysis`) — generator must handle `status: "error"`. Running commentary while the user keeps editing the tree can target stale paths; capture the path list at start and re-validate with getNodeAtPath (which silently returns the deepest valid ancestor, treeReducer.ts:172-181).
- **src/components/common/CompleteMoveCell.tsx:200 and :205-218; src/components/common/GameNotation.tsx:212-243, 335-336, 357, 392, 432, 450-460**: Render generated commentary beneath moves the same way `comment` is: add a prop (or pass the node field) to CompleteMoveCell, render via `<Comment>` with a visual marker (badge/colour), extend the memo comparator, and add a `CommentItem`-like segment type in TableNotation's segment builder. Alternatively (lower churn) add a dedicated Tabs.Tab in BoardAnalysis.tsx:215-248 (keybind pattern: ANNOTATE_TAB 'd', src/state/keybinds.ts:42) that shows commentary for `currentNode` like AnnotationPanel does, plus a game narrative header.
  - risk: RenderVariationTree is memoised by nodePath/depth/first only (:254-258) and reads the node from the store itself, so new per-node fields are fine; but CompleteMoveCell's comparator (:205-218) must include any new prop or updates will not repaint. Comment.tsx uses rehypeRaw (:29) so untrusted LLM output can inject raw HTML into the webview — escape or disable rehypeRaw for generated text.
- **src/state/atoms.ts (atomWithStorage pattern, e.g. :185, :213) + src/components/settings/SettingsPage.tsx (rows with `render: () => <SettingsSwitch atom={…}/>` :193-337)**: Store provider config (`commentaryProviderAtom`, `claudeApiKeyAtom`) with atomWithStorage like every other setting; add a Settings row with a Mantine PasswordInput/TextInput (TextInput already imported :12). Precedent for a credential: `sessionsAtom` (:213) keeps the Lichess `accessToken` (src/utils/session.ts:5) in plain localStorage.
  - risk: No tauri-plugin-store/stronghold/keyring in package.json or src-tauri/Cargo.toml, so a key sits in localStorage unencrypted; maintainers may want an opt-in warning. Outbound HTTP from the webview goes through @tauri-apps/plugin-http whose allowed hosts are declared in src-tauri capabilities (not read here); a Rust-side command with reqwest is the alternative.
- **src/utils/score.ts:75-119 (getAnnotation), src/utils/phase.ts:77 (getGamePhases), src/utils/chess.ts:622-654 (getMaterialDiff), :333-351 (getOpening via commands.getOpeningFromFens), BestMoves.sanMoves/uciMoves, MoveAnalysis.is_sacrifice/novelty, TreeNode.clock, Score.wdl**: The deterministic fallback can be a pure function `describeMove(prevNode, node, prevAnalysis, curAnalysis, color)` composed from these existing primitives: win-chance delta (why it is a mistake), `prevMoves[0].sanMoves` (what the better move was and its PV), material/phase/opening names, mate-in-N, sacrifice/novelty flags, clock deltas. Put it in src/utils/ with vitest tests next to score.test.ts (pure, no Tauri IPC).
  - risk: parsePGN requires Tauri IPC (commands.lexPgn), so tests that need trees must build them with createNode/defaultTree or the store (store.test.ts pattern), not from PGN text.
- **src/utils/treeReducer.ts:156 GameHeaders.other + src/utils/chess.ts:208-210 / :505 (unknown tag passthrough)**: Provenance at game level can ride on standard PGN tags today without code changes: set `headers.other.Annotator = "en-croissant commentary (claude-…)"` via setHeaders; it is emitted and re-parsed. Per-move provenance has no slot (see gotchas).
  - risk: `other` is replaced wholesale by getPgnHeaders on reload; GameInfo UI may not display custom tags; `[Annotator]` conflicts with a human annotator's existing tag.

## Conventions
- State: one zustand vanilla store per tab, created in TreeStateProvider (src/components/common/TreeStateContext.tsx:16) and consumed with `useStore(store, selector)` / `useStoreWithEqualityFn` (CompleteMoveCell.tsx:84-98); all mutations use immer `produce` inside `set(...)` and set `state.dirty = true` (tree.ts:422-494). Pure tree helpers live as module-level functions below the store (makeMove :528, deleteMove :640, setShapes :673, addAnalysis :693).
- UI/settings state is jotai: persisted settings via `atomWithStorage` (atoms.ts:163,172,185,213; ReportModal.tsx:13-19), per-tab UI via `atomFamily` + `tabValue()` (atoms.ts:310-330, 356-378).
- Tauri IPC through tauri-specta generated `commands.*` (src/bindings/generated.ts) returning `{status:"ok",data}|{status:"error",error}`; callers either branch on `status` (ReportModal.tsx:97, chess.ts:346) or use `unwrap()` from src/utils/unwrap.tsx (chess.ts:461). Rust side: `#[tauri::command] #[specta::specta]` async fns returning `Result<T, Error>` (lexer.rs:66-68, pgn.rs:243-250), option structs `#[serde(rename_all = "camelCase")]` + `#[derive(Type)]` (chess.rs:455-463).
- Persisted shapes are zod schemas with `z.infer` types (tabs.ts:37-66, src/components/files/file.ts:7-27, src/utils/engines.ts:53-70).
- Formatting: .editorconfig — 4-space indent for .ts (utils/state), 2-space for .tsx (components), LF, final newline; `pnpm format` = oxfmt, `pnpm lint` = `tsgo --noEmit && oxlint`; generated files src/bindings/generated.ts and src/routeTree.gen.ts are ignored by the formatter (.oxfmtrc.json).
- i18n: react-i18next with FLAT dotted keys (`t("Board.Annotate.WriteHere")`, `t(\`Annotate.${translationKey}\`)`); i18next.config.ts sets keySeparator:false, removeUnusedKeys:true, sort:true, primary en-US; CONTRIBUTING.md requires `pnpm i18n:extract` when adding keys; dynamic key families must be listed in preservePatterns (Annotate.* already is).
- Tests: vitest (`pnpm test`), files in src/utils/tests/*.test.ts, pure functions only (no Tauri); store tests use `createTreeStore()` + `store.setState(fixture)` and `expect(state).toStrictEqual({...fullTreeState})` (store.test.ts:6-10, 297-335). Rust has unit tests inside modules (encoding.rs:384+, db/mod.rs:1978+).
- Components: function components, default export, `memo` with explicit comparators for hot notation cells (CompleteMoveCell.tsx:205-218, GameNotation.tsx:254-258, ReportPanel.tsx:162-167); Mantine v8 components + @tabler/icons-react; hotkeys via react-hotkeys-hook bound to `keyMapAtom` entries (src/state/keybinds.ts).
- Markdown: comments are tiptap markdown (`++underline++` normalised to `<u>` in Comment.tsx:8-10) rendered by react-markdown with rehypeRaw + GFM.
- Annotation semantics: `annotations[0]` is the "visual" glyph (MoveCell.tsx:24, Board.tsx:439, EvalChart.tsx:121-122); arrays are kept sorted by NAG number so basic glyphs ($1-$6) come first.
- PGN export flag conventions: copyPgn/saveToFile use {glyphs, comments, variations, extraMarkups} all true; copyVariationPgn uses comments:false, extraMarkups:false, variations:false with `path` (tree.ts:392-414); PgnInput exposes the four flags as checkboxes (PgnInput.tsx:62-92).

## Gotchas
- BUG src/utils/chess.ts:73-75 — for a basic glyph getMoveText does `moveText += tree.annotations` (the whole array, string-coerced with commas). With a single basic glyph it prints fine; with e.g. ["!","N"] it emits `e4!,N $146`. Any change touching glyph emission should fix this (append `annotation`, not `tree.annotations`).
- innerParsePGN assigns (not appends) `root.comment` (:404) — if two `{}` blocks follow one move, the last wins; eval/clk/shapes from the earlier block are still applied. A comment that precedes the first move of a variation is attached to the temporary variation root and discarded (:418-421).
- No `}` escaping anywhere (getMoveText :117-119). A comment containing `}` breaks the PGN for the Rust lexer and for src-tauri/src/pgn.rs:84-91/122-128 brace tracking used to locate game boundaries, potentially corrupting multi-game files on write_game.
- Autosave is per state transition: with autoSaveAtom=true (default) every AnnotationEditor keystroke → setComment → dirty → saveToFile → full file rewrite via tempfile (pgn.rs:259-276) or SQLite re-import (db/mod.rs:1790-1860). Bulk commentary must be applied in one produce() call.
- No undo/redo: the Edit→Undo/Redo entries in src/routes/__root.tsx:198-203 are Tauri predefined native menu items (text-field undo). The only guard is `dirty` + ConfirmChangesModal (BoardsPage.tsx:52, InfoPanel.tsx:173).
- Trees are rebuilt from PGN text on reload/next game/paste/DB open (InfoPanel.tsx:179-181, PgnInput.tsx:95-102, tabs.ts:116, GameCard.tsx:47). sessionStorage persistence (tree.ts:516-523, tabs.ts:123 writes `{version:0,state}` directly) has no migrate function, so new fields must be optional.
- No provenance anywhere: grep for Annotator/annotator/author/provenance in src returns nothing; `comment` is a bare string; TreeNode has no author/source/timestamp. The SQLite `Comments(ID,GameID,Ply,Comment)` table (src-tauri/src/db/schema.rs:87-99) is declared but never created or referenced. Only `GameHeaders.other` (treeReducer.ts:156) could carry an `[Annotator]` tag today.
- Comment.tsx:29 passes `remarkGfm` inside `rehypePlugins` (should be `remarkPlugins`), and `rehypeRaw` renders raw HTML from comment text — generated text must be sanitised. Links open with target=_blank (:26).
- `TreeNode.depth` is never written (grep: only type/fixtures); `score.wdl` is only non-null when the engine reports WDL. `score` means eval after the node's move from white's POV; `getAnnotation` compares the parent position's MultiPV (analysis[i-1].best) to decide !/!! (score.ts:97-116).
- chessops `parseComment` (chessops@0.14.0 per pnpm-lock.yaml:2189) strips %eval/%clk/%emt/%csl/%cal from `text` and leaves unknown `[%…]` commands in it; the app additionally strips `[%timestamp N]` (chess.ts:403-404). `%emt` is parsed but not stored. NAG_INFO is lossy (several NAG numbers map to one glyph, e.g. $22/$23→⨀); emission uses ANNOTATION_INFO.nag (the first of each pair).
- Custom chessground brush `variation` (key 'v', Board.tsx:586-593) would serialise as `[%cal V…]` (non-standard colour letter) via `shape.brush![0].toUpperCase()` (chess.ts:104,111).
- goToAnnotation (tree.ts:158-184) is a `while(true)` scan that wraps to [] when it runs off the mainline; if no mainline node carries the glyph for that colour it never terminates (callers only invoke it when counts > 0, ReportPanel.tsx:124-128).
- analyze_game (chess.rs:512) omits positions where the game is over and forces MultiPV=2 (:545-557); `MoveAnalysis.best` can be empty for a position if the engine produced no complete multipv set (addAnalysis guards `analysis[i].best.length > 0` :708). `reversed:true` analyses from the end backwards for better hash reuse (:524-526, 601-604).
- parsePGN is async and needs the Tauri runtime (commands.lexPgn), so none of the vitest suites exercise PGN parsing; pure-function tests only.
- PGN export of the root node: getPGN :250 checks `tree.comment !== null` which is always true (comment is a string), so the root `{…}` block is emitted whenever eval/clk/shapes/comment exist on root — root.score is set by addAnalysis for i=0 (tree.ts:706-709) and by live EvalListener when at the start position.
- Translation file names use a dash (src/translation/en-US.json); keys like "Board.Annotate.WriteHere" (:66) are literal flat keys.

## Open questions
- chessops `parseComment` behaviour was not verified from source (no node_modules in the shallow clone); the statement that unknown `[%cmd]` commands remain in `comment.text` and that `%emt` is parsed-but-dropped is from knowledge of chessops 0.14, and the exact `[%eval]` grammar accepted (e.g. leading '+' as emitted by formatScore) should be confirmed against node_modules/chessops/pgn.js before relying on it.
- Tauri HTTP capability allowlist (src-tauri/capabilities/*.json, src-tauri/tauri.conf.json) was not read; whether the webview may call api.anthropic.com via @tauri-apps/plugin-http, or whether a Rust command (reqwest) is needed, is unresolved.
- Whether upstream maintainers would accept a non-standard PGN comment command (e.g. `[%cmt …]`) versus plain comment text, and whether an open issue/PR for commentary exists — not checked (CONTRIBUTING.md asks to check issues first).
- How `remarkGfm` listed under `rehypePlugins` (Comment.tsx:29) actually behaves at runtime (likely a no-op) — cannot run the app from the clone.
- Exact rendering of `GameHeaders.other` tags in the Info panel (GameInfo.tsx not read) — relevant if an `[Annotator]` tag is used for provenance.
- pgn_reader's handling of `}` inside comments / multi-line comments on the Rust lexer side was not tested; only that raw bytes are forwarded (lexer.rs:49-53).
- Whether `write_game` byte-offset splicing stays correct when a comment contains a line starting with `[` (pgn.rs:84,122 treat `[`-prefixed lines as headers only when `!in_comment`, which depends on brace balance) — a wrapped/escaped commentary line could confuse the game-boundary heuristic.
