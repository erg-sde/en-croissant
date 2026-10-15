<!--
Status: approved design, not yet implemented. Baseline: upstream franciscoBSalgueiro/en-croissant
commit 23f83142fcbd4d3af60a524855defe8c9a5703fe (v0.15.1, 2026-09-07). All file:line citations refer to
that commit and were checked mechanically against a clone; re-verify after rebasing onto a newer upstream.
Provider facts (model ids, output_config.format and effort, header version, refusal handling) were checked
against the Anthropic API reference cached 2026-10-06. See README.md in this folder for how the plan was made.
-->

# Facts-first commentary for En Croissant: finishing `report-comments` with a typed fact set, a deterministic narrator, and an optional validated LLM verbalizer

## 1. Thesis

The product is a typed, versioned **fact set per mainline move**, computed deterministically in Rust from positions the report already holds (`fens`, the MultiPV lines from the parent position, the best line from the position after the move) plus shakmaty board predicates, and a typed **game fact set** derived in TypeScript from the same engine output; everything a user reads is a function of those two fact sets and nothing else. The deterministic narrator renders selected facts through i18n templates keyed by what the fact actually says (a line that mates, wins material or merely changes the evaluation gets a different sentence) and is the default; the glyph layer (`getAnnotation`, src/utils/score.ts:75-119) is untouched and derived exactly once by `deriveGlyphs`; the optional LLM only writes connective text around **slots** we render from the cited facts' vocabulary and declares every relation it asserts as a typed **claim** we check against the fact's own fields. Text that can be displayed or exported exists only as branded types (`ValidatedNarration`, `PgnSafeText`) with one minter module each; the brand is a convention enforced by a vitest that scans `src/` for casts to those types outside the minters, and by `moveCommentarySchema.safeParse` on every write and on first read of a rehydrated node. This finishes the maintainer's abandoned `getComment(pos, move): Comment | null` (branch `report-comments`, src/utils/analysis.ts on that branch) with the one thing it lacked, engine-grounded facts about *why*, while honouring his two stated conditions: "I'd rather have no descriptions than bad descriptions" (#230) and no dependence on paid third-party APIs (#704). The operating rule for every stage is **no fact, no sentence**.

## 2. Goals and non-goals

Goals, in priority order:

1. Per-move explanations with reasons: why a move was a mistake (the refutation line cut where it cashes in, naming the piece won and the motif as piece+square), what the better move achieves (its line, cut the same way), what is threatened now, why a move was the only move or a real sacrifice. Never a bare label; never a sentence whose template does not match the line's outcome.
2. A game narrative in the Report tab: the opening (last named position with ECO) and, when a reference database is configured, the move that left known games; turning points; what decided the game; up to three takeaways that are aggregates over selected facts and cite moves.
3. Two surfaces sharing one code path: the "Generate report" batch flow and an on-demand "Explain this position" in the analysis-by-engine view.
4. An always-available offline narrator that is good on its own; an opt-in LLM narrator (Anthropic first, OpenAI-compatible/local second) behind one provider interface, reachable with a local server and **no key** so the feature is never "dependent on paid third-party APIs".
5. Designed for, not built now: player-aware context (own history in this line, rating gap) and Maia-style human-likelihood, as further fact extractors behind the same `MoveFacts` shape. Clock pressure is built now because the data (`TreeNode.clock`) already exists.

Non-goals: chess.com-style labels on every move (#913, #649; the maintainer rejected misleading derived metrics in #749); rating-dependent expected-points classification; changing `getAnnotation` thresholds or NAG semantics; markdown or HTML in generated text; a plugin system; shipping a local LLM; voice; querying the personal database before the single-slot `db_cache` bug (src-tauri/src/main.rs:84, src-tauri/src/db/search.rs:265-298) is fixed.

## 3. Architecture overview

```
 engine (UCI)        Rust  src-tauri/src                                     TypeScript  src (webview)
 ──────────┐  ┌───────────────────────────────────────────────────┐   ┌───────────────────────────────────────────────────┐
           └─►│ analyze_game (chess.rs, exists)                    │   │ GLYPHS  commentary/classify.ts deriveGlyphs (pure) │
              │   fens[i] incl. terminal, MoveAnalysis[i].best     │   │   reproduces tree.ts:708-730 once; feeds the       │
              │   (exact lines only: search_once drops bounds)     │   │   store and the orchestrator                      │
              │        │  post-pass, same positions in hand        │   │        │                                          │
              │        ▼                                            │   │        ▼                                          │
              │ FACTS  commentary::facts::extract_game_facts        │──►│ GAME FACTS  commentary/gameFacts.ts (pure)        │
              │   MoveFacts[i] (schema-versioned, specta-typed)     │IPC│   GameFacts from analysis[], ids game:<kind>:<n>  │
              │   refutation = analysis[i].best[0] (after the move) │   │ SELECTION  commentary/salience.ts  (pure)         │
              │   threat     = null-move search_once (good moves)   │   │   fact ids + vocabulary, ≤3 per move,             │
              │                                                    │   │   mandatory facts per kind, phases via phase.ts   │
              │ NARRATION (opt-in)  commentary::narrate_game        │◄──│        ▼                                          │
              │   NarratorProvider → reqwest; key never leaves Rust │   │ NARRATION  commentary/narrate.ts  (i18n, pure)    │
              │   send_replace cancel token, select! around send()  │──►│   templates keyed Kind.Slot.Outcome, typed refs;  │
              │   NarrationChunk event per ≤12-ply chunk            │evt│   optional NarrationRequest = facts, vocabulary,  │
              │                                                    │   │   drafts; LLM returns slot templates + claims     │
              │ secrets.rs (0600 file, Redacted errors)             │   │        ▼                                          │
              │ jobs.rs (one watch token per id, receiver kept)     │   │ VALIDATION  commentary/validate.ts (chessops)     │
              │                                                    │   │   slots ⊆ vocabulary, claims vs fact fields,      │
              │                                                    │   │   no literals in templates → ValidatedNarration   │
              │                                                    │   │        ▼                                          │
              │                                                    │   │ PERSISTENCE  tree store: node.commentary,         │
              │                                                    │   │   report.narrative (applyReport/applyCommentary   │
              │                                                    │   │   via treeRegistry); PGN export opt-in PgnSafeText│
              │                                                    │   │        ▼                                          │
              │                                                    │   │ PRESENTATION  CommentaryCard, GameNarrative,      │
              │                                                    │   │   CommentaryText (plain text + chips, no HTML);   │
              │                                                    │   │   CommentaryListener mounted in RootLayout        │
              └───────────────────────────────────────────────────┘   └───────────────────────────────────────────────────┘
```

Why the boundary falls where it does. FACTS run in Rust because `analyze_game` already holds every `shakmaty::Chess` position and both candidate lines after its loop (src-tauri/src/chess.rs:500-604) on one `EngineProcess` created at :493; the refutation of move *i* is `analysis[i].best[0]`, the best line from the position after the move, which is exactly what `addAnalysis` writes to `cur.score` (src/state/store/tree.ts:709); facts must be FEN-fixture testable in the existing `#[cfg(test)]` style (chess.rs:711-773); and the same extractor serves `explain_position`. GLYPHS, GAME FACTS, SELECTION, deterministic NARRATION and VALIDATION run in TypeScript because they are bound to i18next (16 locales live in the webview), must be vitest-testable with no Tauri import (the only tests CI runs, .github/workflows/test.yml:30-36), and reuse the bundled chessops, `getAnnotation` and the existing `getGamePhases` (src/utils/phase.ts:77). LLM HTTP runs in Rust because the webview's `http:default` allow list is compile-time JSON (src-tauri/capabilities/main.json:43), a user base URL cannot be allow-listed, the key must not enter the webview, and only a dropped reqwest future really tears down an in-flight request. Persistence and presentation are TypeScript because the tree store is.

## 4. Module layout

```
src-tauri/src/
  chess.rs             (edit) ParsedLine{line,bound} from parse_uci_attrs; search_once drops bound lines; terminal entries in fens;
                              finalize, multipv, with_facts/with_threats, clocks; MoveAnalysis.fen/.terminal/.facts; assert removed;
                              PR 10: pub(crate) on EngineProcess::{new,set_options,go,stop,kill}, parse_uci_attrs, search_once
  progress.rs          (edit) stage/status (#[specta(optional)]), ProgressGuard (sole owner)
  jobs.rs              NEW   JobRegistry (watch token per id, receiver kept), JobHandle, JobPhase; replaces analysis_cancel_flags (PR 1)
  opening.rs           (edit) HashMap<Epd, usize>, eco exposed, get_opening_match
  error.rs / main.rs   (edit) InvalidGoMode; mod jobs; mod commentary; AppState.jobs; registrations
  commentary/          NEW   one module for everything the feature owns
    mod.rs                        FACTS_SCHEMA_VERSION, re-exports
    facts/{mod,types}.rs          extract_game_facts / extract_move_facts; every fact type and the shared enums (§5.1)
    facts/geometry.rs             attackers/defenders with x-ray, see() on the 1/3/3/5/9 scale, hanging (SEE + legal capture), trapped
    facts/motifs.rs               fork (king always a target), pin, skewer, discovered attack, back-rank, passed pawn, promotion
    facts/threats.rs              static mate-in-1 threats (the branch's "threatens checkmate on sq"); null-move setup
    facts/criticality.rs          OnlyMove / ClearlyBest, MateFound/Created/Lost/Delayed, Miss, Sacrifice (SEE-defined)
    facts/structure.rs            material (P1 N3 B3 R5 Q9), pawns, open files, king zone, BoardDelta
    facts/refutation.rs           PV walk: LineOutcome (mate from eval, material from the PV), LineCut, shown_plies
    facts/time.rs                 TimeFacts from AnalysisOptions.clocks / increment_s / initial_s
    explain.rs                    explain_position on its own EngineProcess (never engine_processes)
    narrate.rs / prompt.rs        narrate_game: chunking, select! cancel, NarrationChunk; system prompt (kibitz-derived, attributed) + NARRATION_SCHEMA
    provider/{mod,anthropic,openai_compat}.rs   NarratorProvider, NarratorConfig, NarrationFailure, Redacted; the two adapters
    secrets.rs                    SecretStore trait, FileStore (0600 JSON in app_data_dir), set/clear/status commands
src/
  utils/commentary/    NEW   pure modules: no Tauri, no Mantine
    types.ts                      records, brands, zod schemas
    classify.ts                   deriveGlyphs (PR 6); getExtendedAnnotation (⊗, □, ⊕) (PR 8)
    gameFacts.ts                  GameFacts from analysis[] + phases + opening match (PR 9)
    salience.ts                   enumerateFacts, vocabularyOf, selectFacts
    narrate.ts / segmentize.ts    i18n templates → sentences with typed refs (sentinel interpolation, Intl.ListFormat, number spans)
    validate.ts / pgnSafe.ts      the only minters of ValidatedNarration / PgnSafeText
    snapshot.ts / fingerprint.ts / settings.ts (incl. resolvePerspective) / errors.ts
  utils/tests/commentary/*.test.ts  NEW   (+ brands.test.ts: no `as ValidatedNarration`/`as PgnSafeText` outside the minters)
  state/store/treeRegistry.ts  NEW   live TreeStore per tab id + parked NarrationChunk queue per (tab, runId) + activeRuns
  state/store/tree.ts          (edit) applyReport, applyCommentary, setGameNarrative, beginReport/endReport, appendComment; addAnalysis uses deriveGlyphs
  utils/treeReducer.ts         (edit) TreeNode.commentary?; ReportState.stage?/runId?/narrative?/failure?
  utils/chess.ts, utils/score.ts (edit) glyph fix, PgnSafeText slot, getOpeningMatch(); exhaustive normalizeScore; getGameStats gap reset
  utils/clock.ts               (edit) export parseTimeControl
  state/atoms.ts, state/keybinds.ts (edit) reportSettingsAtom (moved here, exported), commentarySettingsAtom, commentaryFactsFamily,
                                     showCommentaryFamily, commentaryArrowsFamily; EXPLAIN_POSITION, TOGGLE_COMMENTARY
  components/common/CommentaryText.tsx NEW; Comment.tsx (edit, no rehypeRaw); TreeStateContext.tsx (edit, registry)
  components/common/CommentaryListener.tsx NEW   mounted once in RootLayout (src/routes/__root.tsx:88); applies/parks NarrationChunk
  components/panels/analysis/{CommentaryCard,GameNarrative,ExplainButton}.tsx NEW; {ReportModal,ReportPanel,AnalysisPanel}.tsx (edit)
  components/settings/CommentarySettings.tsx NEW; SettingsPage.tsx (edit, category "commentary")
  components/tabs/BoardsPage.tsx (edit) closeTab cancels report_${value}
  translation/en-US.json, i18next.config.ts (edit) Commentary.*, Settings.Commentary.*, Errors.Commentary.*; preservePattern "Commentary.*"
```

Naming discipline: `facts` holds what was *counted*; `salience` alone decides what matters; `narrate` alone produces words; `validate` alone deletes words and mints `ValidatedNarration`; `pgnSafe` alone mints `PgnSafeText`; `commentary` is the one user-facing noun across module, directory, node field, atoms, settings category and i18n prefix. Stage-internal types take the stage noun (`Narration*`, `NarratorConfig`); user-visible records are `MoveCommentary` and `GameNarrative`. There is no Rust phase module: phases are a presentation grouping, `getGamePhases` already exists in TS, and a port would need a permanent equality test.

## 5. Data model

### 5.1 Rust facts (`src-tauri/src/commentary/facts/types.rs`, NEW)

shakmaty 0.27.1 implements neither `serde::Serialize` nor `specta::Type` for `Square`/`Role`/`Color` (its Cargo.toml `[features]` lists only `alloc`, `std`, `nohash-hasher`, `variant`), so facts use their own enums and `String` squares. Every type below derives `Serialize, Debug, Clone, Type` with `#[serde(rename_all = "camelCase")]` (the newer-code convention, src-tauri/src/game.rs:49-58); the enums and vocabulary types also derive `Deserialize` because they return over IPC inside `NarrationRequest` and `NarratedSentence`; tagged enums use `#[serde(tag = "kind")]`. `FactKind`, `CommentaryKind` and `Perspective` are defined here once and imported by TS from `@/bindings`, so Rust and TS cannot drift.

```rust
pub const FACTS_SCHEMA_VERSION: u16 = 1;

pub enum Side { White, Black }
pub enum PieceKind { Pawn, Knight, Bishop, Rook, Queen, King }   // matches chessops Role strings
pub struct ByColor<T> { pub white: T, pub black: T }             // ours; shakmaty's ByColor has no Serialize

#[serde(tag = "type", content = "value")]
pub enum EvalValue { Cp(i32), Mate(i32) }   // White POV; Mate(n): n > 0 White mates in n moves, n < 0 Black mates in |n| (UCI moves, not plies)

pub struct ExactEval { pub value: EvalValue, pub depth: u32, pub win: f32 }
// win = mover-POV win chance 0..100 (getWinChance, score.ts:34-36; mate clamped to ±1000 like normalizeScore :38-47).
// Built by ExactEval::from_line(&BestMoves, mover) over lines returned by search_once, which drops bound lines (§7),
// or by ExactEval::terminal(outcome) for checkmate/stalemate positions (depth 0, win 100 / 50 / 0).

pub struct EvalFacts {
    pub before: Option<ExactEval>,    // parent best[0]           (analysis[i-1].best[0])
    pub after: ExactEval,             // this position's best[0]  (analysis[i].best[0]) or terminal
    pub win_delta: Option<f32>,       // before.win - after.win, mover POV; None when before is None
}

pub enum LineCut { Mate, MaterialRealized, Quiet, MaxPlies, EngineEnd }
pub enum LineOutcome {
    Mate { mate_in: u8, pv_reaches_mate: bool },             // from the line's eval, never from PV length
    Material { pawns: i32, piece: Option<PieceKind>, at_ply: u8 },   // P1 N3 B3 R5 Q9, mover POV of the line's first move
    Eval { cp: i32 },                                        // White POV, line eval
}
pub enum LineOrigin { Before, After, Threat }   // parent position / position after the move / null-move search

pub struct LineFacts {
    pub id: u8,                       // index into MoveFacts.lines; references below are indices, so they cannot dangle
    pub origin: LineOrigin, pub multipv: u16, pub eval: ExactEval,
    pub pv_san: Vec<String>, pub pv_uci: Vec<String>,   // full line
    pub shown_plies: u8,              // cash-in truncation, 2..=6
    pub cut_reason: LineCut, pub outcome: LineOutcome,
    pub motifs: Vec<Motif>,           // detected on positions after each shown ply
}

pub struct Target { pub square: String, pub piece: PieceKind }
pub enum Motif {
    Fork { from: String, attacker: PieceKind, targets: Vec<Target>, is_check: bool },   // the king is always a valid target
    Pin { attacker: String, pinned: String, behind: String, absolute: bool },
    Skewer { attacker: String, front: String, behind: String },
    DiscoveredAttack { moved_from: String, revealed: String, target: String, is_check: bool },
    Hanging { square: String, piece: PieceKind, side: Side, see: i32 },   // see < 0 and a legal capture exists
    TrappedPiece { square: String, piece: PieceKind },
    BackRank { king: String },
    MateThreat { from: String, to: String },
    PassedPawn { square: String },
    Promotion { square: String, to: PieceKind },
}

pub enum Criticality {
    OnlyMove { margin: f32, second_win: f32 },            // rank 1 played, win(best) - win(second) > 10, second_win < 50 or second mates the mover
    ClearlyBest { margin: f32 },                          // rank 1 played, gap > 10, but the second line still holds (second_win >= 50)
    MateFound { mate_in: u8, alternatives_also_mate: bool },   // before: cp or no mate for the mover; the played line mates
    MateCreated { mate_in: u8 },                          // cp → mate against the mover (lila MateCreated)
    MateLost { was_mate_in: u8 },                         // mate for the mover → cp or mate against
    MateDelayed { from: u8, to: u8 },
    Miss { gift: f32, given_back: f32 },                  // opponent's win_delta at i-1, mover's at i
    Sacrifice { piece: PieceKind, square: String, see: i32, holds: bool, taken_in_line: bool },   // SEE-defined (§6.1)
}

pub struct PlayedMoveFacts {
    pub san: String, pub uci: String,
    pub piece: PieceKind, pub from: String, pub to: String,
    pub rank: Option<u16>,            // 1 = engine best; None if outside MultiPV
    pub is_capture: bool, pub captured: Option<PieceKind>,
    pub gives_check: bool, pub gives_mate: bool, pub is_castle: bool, pub promotion: Option<PieceKind>,
    pub see: Option<i32>,             // see(to) after the move, pawn units, mover POV; None for non-captures
}

pub struct PawnFacts { pub isolated: Vec<String>, pub doubled: Vec<String>, pub passed: Vec<String> }
pub struct BoardFacts {
    pub material: ByColor<i32>,       // pawn units P1 N3 B3 R5 Q9 (Board::material, board.rs:314)
    pub hanging: Vec<Motif>,          // Motif::Hanging only
    pub in_check: bool, pub king_zone_attackers: ByColor<u8>,
    pub pawns: ByColor<PawnFacts>, pub open_files: Vec<char>,
}
pub struct AttackFact { pub from: String, pub to: String, pub attacker: PieceKind, pub target: PieceKind, pub defended: bool }
pub struct BoardDelta {
    pub material_change: i32,             // mover POV, pawn units
    pub newly_attacked: Vec<AttackFact>,  // the branch's "This move attacks the X on sq"
    pub newly_hanging: Vec<Motif>,        // the mover's own pieces only
    pub king_zone_delta: i8,
}
pub struct OpeningFacts { pub eco: String, pub name: String, pub last_named_index: u32 }   // last position found in the ECO table
pub struct TimeFacts { pub remaining_s: f32, pub spent_s: Option<f32>, pub under_pressure: bool }

// Vocabulary: one shape serves as a fact's allowed board references (computed in TS from the fact)
// and as the pool the LLM's slots are drawn from. The validator requires slots ⊆ vocabulary.
#[serde(tag = "t")]
pub enum PositionRef { Before, After, Line { id: u8, ply: u8 } }
pub enum NumberUnit { Pawns, MateIn, WinPercent, Move, Eval, Seconds }   // Eval renders through formatScore
pub struct MoveRef { pub san: String, pub side: Side, pub at: PositionRef }
pub struct PieceRef { pub piece: PieceKind, pub square: String, pub at: PositionRef }
pub struct NumberRef { pub value: f32, pub unit: NumberUnit }
pub struct FactVocabulary { pub moves: Vec<MoveRef>, pub pieces: Vec<PieceRef>, pub squares: Vec<String>, pub numbers: Vec<NumberRef>, pub sides: Vec<Side> }

pub enum FactKind { Line, Hanging, Attack, Motif, Criticality, EvalDelta, Opening, Novelty, Time, Game }
pub enum CommentaryKind {
    Mistake, EvalDrift, Miss, BetterMove, OnlyMove, ClearlyBest, Sacrifice, MateFound, Threat,
    Opening, Novelty, Phase, Mate, Positional,
}
pub enum Perspective { White, Black, Both }

pub struct MoveFacts {
    pub schema_version: u16,
    pub index: u32,                   // index into fens/analysis (position AFTER the move); root = 0 has no facts.
                                      // Not TreeNode.halfMoves; the TS join key is (fen_before, san), never index.
    pub fen_before: String, pub fen_after: String, pub mover: Side,
    pub terminal: bool,               // fen_after is checkmate/stalemate; lines is empty, after is synthesised
    pub played: PlayedMoveFacts, pub eval: EvalFacts,
    pub lines: Vec<LineFacts>,        // Before lines (parent MultiPV), the After best line, optional Threat line
    pub best: Option<u8>,             // lines index of the parent best when it differs from the played move
    pub second: Option<u8>,           // lines index of the parent second line
    pub refutation: Option<u8>,       // lines index of the After best line (opponent's best reply)
    pub threat: Option<u8>,           // lines index of the Threat line, when searched
    pub criticality: Vec<Criticality>,// zero or more, fixed extractor order
    pub before: BoardFacts, pub after: BoardFacts, pub delta: BoardDelta,
    pub opening: Option<OpeningFacts>, pub novelty: bool, pub time: Option<TimeFacts>,
}
```

### 5.2 Edits to existing Rust types (`src-tauri/src/chess.rs`)

```rust
pub struct BestMoves { /* :154-167 UNCHANGED; fields become pub(crate) so commentary::facts can read them.
                          Serialized shape is identical, so src/bindings/index.ts:12-14, the chessdb and lichess
                          adapters (src/utils/chessdb/api.ts:31-66, src/utils/lichess/api.tsx:238-287) and the
                          addAnalysis fixture (src/utils/tests/store.test.ts:415-460) keep type-checking. */ }

enum Bound { Exact, Lower, Upper }                  // NEW, private to chess.rs (PR 1)
struct ParsedLine { line: BestMoves, bound: Bound } // NEW, private: parse_uci_attrs returns it; bound comes from the fork's
                                                    // Score.lower_bound/upper_bound (scratchpad/vampirc-fork/uci.rs:807-820)

#[derive(Serialize, Debug, Default, Type)]           // no rename_all added; is_sacrifice stays as shipped
pub struct MoveAnalysis {
    best: Vec<BestMoves>, novelty: bool, is_sacrifice: bool,
    fen: String,                                               // NEW: fens[i].0 as Fen::from_position(.., EnPassantMode::Legal) for every i, index 0 included
    terminal: bool,                                            // NEW: game-over position, best is empty, no search was run
    #[specta(optional)] facts: Option<crate::commentary::facts::MoveFacts>,   // NEW: Some when with_facts && i > 0
}

#[derive(Deserialize, Debug, Default, Type)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisOptions {
    pub fen: String, pub moves: Vec<String>, pub annotate_novelties: bool,
    pub reference_db: Option<PathBuf>, pub reversed: bool,
    #[serde(default)] pub multipv: Option<u16>,     // NEW, default 2, clamped to 2..=4
    #[serde(default)] pub with_facts: bool,         // NEW
    #[serde(default)] pub with_threats: bool,       // NEW: null-move lines for ≤12 good moves
    #[serde(default)] pub finalize: Option<bool>,   // NEW, fs.rs:30-33 precedent
    #[serde(default)] pub clocks: Option<Vec<Option<f32>>>,   // NEW: seconds remaining after moves[k], aligned to moves
    #[serde(default)] pub increment_s: Option<f32>, // NEW: from parseTimeControl(headers.time_control)
    #[serde(default)] pub initial_s: Option<f32>,   // NEW: first field's seconds
}
```

`#[serde(default)]` alone makes specta emit an optional key (precedent: `GameConfig.initial_moves`, game.rs:63-64 → generated.ts:503 `initialMoves?: string[]`), so ReportModal's existing call keeps type-checking and the new fields are passed only where used. TS shape changes, listed so PR 6 cannot fail CI: `MoveAnalysis` gains `fen: string`, `terminal: boolean`, `facts?: MoveFacts | null`; `addAnalysis`'s input type (tree.ts:74-81) gains `fen: string` and the two fixture entries in store.test.ts:418-456 gain `fen`.

### 5.3 Rust provider, job and progress types

```rust
// commentary/provider/mod.rs  (all: Serialize, Deserialize, Debug, Clone, Type, camelCase)
pub struct NarratorConfig {           // crosses IPC; has NO key field (type-level test, §12)
    pub provider: ProviderKind,
    pub model: String,                // default "claude-opus-5-5"
    pub base_url: Option<String>,     // None = provider default; https required unless localhost/127.0.0.1/[::1]
    pub language: String,             // BCP-47 from i18next, e.g. "pt-PT"
    pub perspective: Perspective,     // resolved in TS (§9); never "auto" past the IPC boundary
    pub budget: WordBudget,
    pub repair_rounds: u8,            // 0 for move chunks, 1 for the narrative chunk
}
pub enum ProviderKind { Anthropic, OpenaiCompatible }
pub struct WordBudget { pub move_words: u16, pub key_moment_words: u16, pub narrative_words: u16 }  // 60 / 140 / 220

/// Serialize and Debug run redact(): masks sk-ant-[A-Za-z0-9_-]+, "Bearer …", the stored secret; caps at 200 chars.
#[derive(Clone)] pub struct Redacted(String);

#[serde(tag = "kind", content = "detail")]
pub enum NarrationFailure {
    NoKey,                                   // Anthropic only; openaiCompatible sends without Authorization when no secret exists
    InsecureBaseUrl,
    Auth { status: u16 },
    RateLimit { retry_after_ms: Option<u64> },
    Network { message: Redacted },           // connect, read and the 120 s per-request timeout
    Provider { status: u16, message: Redacted },
    MalformedResponse { message: Redacted },
    Refused { category: Option<String> },    // stop_reason == "refusal", stop_details.category
    Cancelled,
}
#[serde(tag = "status")]
pub enum NarrationRun { Completed { chunks: u32 }, Cancelled { chunks_done: u32 }, Failed { chunks_done: u32, failure: NarrationFailure } }
pub struct SecretStatus { pub configured: bool, pub hint: Option<String> }   // hint = last 4 chars
pub struct Served { pub model: String, pub input_tokens: u32, pub output_tokens: u32 }   // from the response, carried in NarrationChunk

// jobs.rs (top level, PR 1)
pub enum JobPhase { Engine, Narration, Explain }
pub struct JobHandle {
    pub cancel: tokio::sync::watch::Sender<bool>,
    keep: tokio::sync::watch::Receiver<bool>,   // held so the channel never closes; send_replace(true) always stores
    pub phase: JobPhase, pub started: Instant,
}
pub struct JobRegistry(DashMap<String, JobHandle>);   // AppState.jobs; insert(id) cancels and replaces an existing handle
// JobRegistry::subscribe(id) -> watch::Receiver<bool>; every stage calls it, never stores the Sender's initial receiver

// progress.rs (ProgressItem and ProgressEvent both)
pub enum ProgressStage { Engine, Commentary }
pub enum ProgressStatus { Running, Finished, Cancelled, Failed }
pub struct ProgressEvent {
    pub id: String, pub progress: f32, pub finished: bool,
    #[specta(optional)] pub stage: Option<ProgressStage>,
    #[specta(optional)] pub status: Option<ProgressStatus>,
}
pub struct ProgressGuard<'a> { /* store, app, id, jobs, stage, done: bool; Drop emits (stage, 100, true, Failed) and removes the job unless finished()/cancelled()/handed_off() was called */ }
```

`#[specta(optional)]` (precedent src-tauri/src/db/models.rs:155-174 → generated.ts:516 `date?: string | null`) emits `stage?: ProgressStage | null`, so the TS-side `events.progressEvent.emit({ finished, id, progress })` calls at src/utils/chess.com/api.tsx:139-149 keep type-checking.

### 5.4 TypeScript records (`src/utils/treeReducer.ts` edit, `src/utils/commentary/types.ts` NEW)

```ts
// treeReducer.ts
export interface TreeNode {
    fen: string; move: Move | null; san: string | null; children: TreeNode[];
    score: Score | null; depth: number | null; halfMoves: number;
    shapes: DrawShape[]; annotations: Annotation[]; comment: string; clock?: number;
    commentary?: MoveCommentary;          // NEW: never assigned by createNode/defaultTree
}
export interface ReportState {
    inProgress: boolean;
    stage?: "engine" | "commentary";      // NEW, optional: defaultTree() still yields { inProgress: false }
    runId?: string;                       // NEW: snapshot identity of the current run
    narrative?: GameNarrative;            // NEW
    failure?: NarrationFailure | null;    // NEW, from bindings
}

// commentary/types.ts
import type { FactKind, CommentaryKind, Perspective, FactVocabulary, Claim, Slots } from "@/bindings";
export const COMMENTARY_SCHEMA_VERSION = 1 as const;
export type FactId = string;              // "<kind>:<ordinal>" within a move, e.g. "line:2", "hanging:1"; "game:<kind>:<ordinal>" for GameFacts;
                                          // "<index>/<kind>:<ordinal>" when a move fact is cited from the narrative

export interface CommentarySentence { text: string; factIds: FactId[]; refs: FactVocabulary; claims: Claim[] }

declare const validated: unique symbol;
export type ValidatedNarration = { sentences: CommentarySentence[] } & { readonly [validated]: true }; // minted only in validate.ts
declare const pgnSafe: unique symbol;
export type PgnSafeText = string & { readonly [pgnSafe]: true };                                   // minted only in pgnSafe.ts

export interface Provenance {
    source: "deterministic" | "llm";
    provider?: "anthropic" | "openaiCompatible";
    model?: string;                       // the response's model field, not the configured one (server-side fallbacks)
    usage?: { inputTokens: number; outputTokens: number };
    generatedAt: number; appVersion: string; factsSchemaVersion: number;
    engine?: { goMode: GoMode; depth: number };
    validation?: ValidationResult;        // present for source === "llm"
}
export interface MoveCommentary {
    schemaVersion: typeof COMMENTARY_SCHEMA_VERSION;
    kind: CommentaryKind; perspective: Perspective;
    narration: ValidatedNarration;        // the only text field; plain text, no { } [% < >
    arrows: { orig: Key; dest?: Key; brush: "green" | "red" | "blue" | "yellow" }[];
    fingerprint: string; provenance: Provenance;
}
export interface GameFacts {              // gameFacts.ts, pure over analysis[] + facts; ids game:<kind>:<ordinal>
    opening: { eco: string | null; name: string | null; lastNamedIndex: number | null };
    noveltyIndex: number | null;          // from MoveAnalysis.novelty; null when no reference DB was used
    phases: { middlegamePly: number | null; endgamePly: number | null };   // getGamePhases (src/utils/phase.ts:77)
    swings: { index: number; san: string; side: Side; winDelta: number; kind: CommentaryKind }[];   // top 3 by |winDelta|
    decisive: number | null;              // index after which the loser's win chance never recovers past 50
    accuracy: ByColor<number>;            // harmonicMean of getAccuracy over consecutive scored entries; gaps break the chain
    result: Outcome; terminal: { index: number; san: string; mate: boolean } | null;
}
export interface GameNarrative {
    schemaVersion: typeof COMMENTARY_SCHEMA_VERSION;
    facts: GameFacts;
    takeaways: { narration: ValidatedNarration; indices: number[] }[];   // 0..3, aggregates over selected facts (§6.3)
    narration: ValidatedNarration; fingerprint: string; provenance: Provenance;
}

// salience.ts
export interface SelectedFact { id: FactId; kind: FactKind; weight: number; vocabulary: FactVocabulary }
export interface FactSelection {
    index: number; classification: Annotation; extended: ("⊗" | "□" | "⊕")[]; kind: CommentaryKind;
    facts: SelectedFact[]; budgetWords: number;
}

// validate.ts
export type ValidationRule =
    | "unknownFact" | "uncitedFact" | "slotNotInFacts" | "pieceNotOnBoard" | "moveNotLegalForSide" | "wrongSide"
    | "literalInTemplate" | "claimUnsupported" | "claimContradicted" | "relationWithoutClaim"
    | "magnitude" | "sideBetter" | "unknownMotif" | "forbiddenToken" | "budget";
export interface ValidationFailure { rule: ValidationRule; sentence: number; detail: string }
export interface ValidationResult { checked: number; dropped: number; failures: ValidationFailure[] }

// settings.ts
export const commentarySettingsSchema = z.object({
    explain: z.boolean().default(true),
    detail: z.enum(["keyMoments", "mistakes", "allMoves"]).default("keyMoments"),
    extendedGlyphs: z.boolean().default(true),
    threatLines: z.boolean().default(false),
    llmEnabled: z.boolean().default(false),
    consentVersion: z.number().default(0),
    provider: z.enum(["anthropic", "openaiCompatible"]).default("anthropic"),
    model: z.string().default("claude-opus-5-5"),
    baseUrl: z.string().url().nullable().default(null),
    language: z.string().nullable().default(null),       // null = UI language
    perspective: z.enum(["auto", "white", "black", "both"]).default("auto"),
    exportToPgn: z.boolean().default(false),
});
export type CommentarySettings = z.infer<typeof commentarySettingsSchema>;
export function resolvePerspective(headers: GameHeaders, sessions: Session[], setting: CommentarySettings["perspective"]): Perspective;
```

Where each lives. `MoveFacts` crosses IPC inside `MoveAnalysis.facts` or `PositionExplanation` and is held per tab in the non-persisted jotai map `commentaryFactsFamily(tab): Map<lineKey, MoveFacts>` like `engineMovesFamily` (src/state/atoms.ts:544), bounded to two runs. `MoveCommentary` is node-resident and persisted by the existing `persist` (tree.ts:516-523); `GameNarrative` rides on `ReportState`. `schemaVersion` plus `provenance.factsSchemaVersion` let `applyCommentary` drop records of an older shape; `moveCommentarySchema.safeParse` runs in `applyCommentary` and on the first read of a rehydrated node, because sessionStorage JSON carries no brand. Optional fields that `createNode`/`defaultTree` never assign cost zero edits to the `toStrictEqual` fixtures (src/utils/tests/store.test.ts:14-62), and no required field is added to `TreeState`, which is why a `revision` counter was rejected.

## 6. Pipeline stages in detail

### 6.1 FACTS (Rust, pure, inside the `analyze_game` post-pass and `explain_position`)

Inputs: `fens: Vec<(Fen, Vec<String>, bool, bool)>` (chess.rs:500-522 plus a `terminal` flag; game-over positions are now pushed too, with no search), the un-reversed `analysis` (after :601-604), optional threat lines, `multipv`, `clocks`. Output: `MoveFacts` for `i ≥ 1`. Purity: `extract_move_facts(ctx: &FactsContext) -> MoveFacts`, with `FactsContext { before: &Chess, mv: &Move, parent_lines: &[BestMoves], own_lines: &[BestMoves], threat_line: Option<&BestMoves>, opening: Option<&OpeningMatch>, prev: Option<&EvalFacts>, clock: Option<TimeInput>, terminal: bool }`, touches no engine, DB or state; only `threats.rs` builds the swapped setup the engine is then asked about. API names verified in scratchpad/shakmaty-src/shakmaty-0.27.1 (lichess-puzzler and lila material below is used as published algorithm descriptions only; no code is ported):

- Positions: `parse_fen_and_apply_moves` (src-tauri/src/engine/uci.rs:24) replays once; `before.clone()` then `play_unchecked(&m)` gives `after`. Move flags from `Move::capture/is_capture/is_castle/promotion/role/from/to` (types.rs:60-143); check and mate from `Position::is_check`/`is_checkmate` (position.rs:560/:565) on `after` (`gives_check` exists only under the `variant` feature, position.rs:665, and is not used).
- Material and SEE (`geometry.rs`): one table `value(PieceKind) = P1 N3 B3 R5 Q9`, king excluded from sums and treated as infinite as a target; `BoardFacts.material` from `Board::material()` (board.rs:314). `see(board, square, side) -> i32` is a classic swap-list static exchange on one square: attackers from `Board::attacks_to(sq, attacker, occupied)` (board.rs:298), least-valuable attacker first, x-ray re-query after each capture by removing the capturing piece from `occupied` and re-asking the sliders (`rooks_and_queens()`/`bishops_and_queens()`, board.rs:207/:212), result in pawn units from the capturing side's POV. The existing capture-only `qsearch` (chess.rs:665-696, 90/300/500/1000 scale) is kept only for the legacy `is_sacrifice` flag; no commentary number comes from it.
- Hanging = `see(square, enemy) < 0` **and** `legal_moves()` contains a capture to that square (so absolute pins and checks are respected); `BoardDelta.newly_hanging` lists the mover's own pieces only. Trapped per the published tagger definition (in a bad spot, no legal move to a square that is not).
- Pins/skewers: for each enemy slider, `attacks::ray`/`attacks::between` (attacks.rs:171/:194) with exactly one blocker of the pinned colour; `absolute` when the far piece is `king_of(side)` (board.rs:219). Discovered attack: the moved piece left `between(revealed, target)` and the revealed slider now attacks `target`. Fork: `attacks::attacks(to, piece, occupied)` (attacks.rs:141) ∩ enemy pieces that are the king, higher value, or hanging, ≥ 2; `is_check = after.is_check()`. Pawn structure from `by_piece`/`by_color` bitboards (board.rs:286/:276) and file masks; king-zone attackers = enemy `attacks_to` over `king_attacks(king_sq)` (attacks.rs:53).
- Engine lines: every parent line becomes a `LineFacts { origin: Before }`; `analysis[i].best[0]` becomes the `After` line and is the **refutation**; `played.rank` matches the played UCI against `parent_lines[k].uci_moves[0]`. Lines reach the extractor only through `search_once`, which drops bound lines, so every `ExactEval` is exact by construction; a position whose exact set is empty gets `facts: None` and no sentence.
- Line outcome (`refutation.rs`): `LineOutcome::Mate { mate_in: |n|, pv_reaches_mate }` whenever the line's eval is `Mate(n)` (|n| moves by the mating side counted from the position the line starts from; `pv_reaches_mate` = the replayed PV ends in checkmate); otherwise walk the PV with the 1/3/3/5/9 table: `Material { pawns, piece, at_ply }` at the first ply where the delta from the line's start (POV of the side that plays the line's first move) is ≥ 1 and the next PV move is not a capture (or the PV ends); otherwise `Eval { cp }`. `shown_plies`: `Mate` cuts where the PV mates (or at `MaxPlies` with the mate number carried by the outcome), `MaterialRealized` at `at_ply`, `Quiet` at the first non-capture non-check after a forcing sequence, `MaxPlies` at 6, `EngineEnd` if the PV is shorter; always ≥ 2 plies; the full `pv_san` lets the UI expand.
- `Criticality` (`criticality.rs`): with `played.rank == Some(1)` and `win(best) − win(second) > 10` (the strict gap score.ts:98-101 uses for `!`): `OnlyMove { margin, second_win }` when `second_win < 50` or the second line's eval is mate against the mover, else `ClearlyBest { margin }`. `MateFound { mate_in, alternatives_also_mate }` when `before` is not a mate for the mover and the played move's own Before line (or `after`) is mate for the mover; `MateCreated`/`MateLost`/`MateDelayed` from `EvalValue::Mate` transitions against the mover (lila `Advice.scala` semantics), never through the ±1000 clamp. `Miss { gift, given_back }` when the opponent's `win_delta` at *i−1* was ≥ 10 and the mover's at *i* is ≥ `gift / 2`. `Sacrifice { piece, square, see, holds, taken_in_line }` when, on the board after the move, the moved piece or a piece the move newly left en prise has `see ≤ −2` for the opponent (a capture's destination square counts), `holds = eval.after.win ≥ 40`, `taken_in_line` = the After line captures on that square within 2 plies; the legacy `is_sacrifice` flag stays a glyph input only and never produces a sentence.
- Threat lines (`with_threats`): after the main loop the still-open `EngineProcess` runs `search_once` (MultiPV 1, same `go_mode`) on the side-swapped setup of `fen_after` (turn flipped, ep square cleared, skipped when in check) for at most 12 indices chosen in Rust among moves where the mover's idea is the point: `OnlyMove`/`ClearlyBest`/`MateFound`/`Sacrifice`/`Miss` present or a `MateThreat` motif present, ordered by `win_delta` ascending (best moves first); never for `?!/?/??`, where the opponent's threat is the refutation line. ≤ 6 s extra at the 500 ms default. The result is a `LineFacts { origin: Threat }` and never touches `node.score` (unlike the live threat toggle, src/components/boards/EvalListener.tsx:58-71/:158-162).
- Opening: `opening::get_opening_match(&fens)` keyed by `shakmaty::fen::Epd` (fen.rs:541-563) yields `eco`, `name`, `last_named_index`. The table's lines have a median length of 9 plies (p90 16, max 36 over 3,641 rows in src-tauri/data/{a..e}.tsv), so this is "the last named position", never "theory ends"; "left known games" is `MoveAnalysis.novelty` (chess.rs:615-629) and is copied to `MoveFacts.novelty`. Phases are not a Rust fact (§4).
- Time (`time.rs`): `clocks[k]` is the mover's remaining seconds after move `k+1`; `spent_s = previous same-colour clock − remaining + increment_s` when both exist; `under_pressure = remaining_s < 20 || remaining_s < 0.1 × initial_s`. Absent clocks → `time: None`.
- Terminal positions: the final move's `MoveFacts` has `terminal: true`, `played.gives_mate` from `after.is_checkmate()`, `eval.after = ExactEval::terminal(outcome)`, `lines` empty except the parent lines; narrate.ts renders `Commentary.Mate` for it and `GameFacts.terminal` cites it.

### 6.2 GLYPHS and SELECTION (TS, pure, `classify.ts`, `salience.ts`)

`deriveGlyphs(analysis, snapshot): (Annotation | null)[]` (classify.ts, PR 6) reproduces tree.ts:708-730 once: `color = snapshot.plies[i].halfMoves % 2 ? "white" : "black"`, `prevprev`/`prev`/`cur` scores, `prevMoves = analysis[i-1].best`, `is_sacrifice`, `san`, and the `best.length > 0 && !terminal` skips; the refactored `addAnalysis(state, analysis, glyphs, options)` consumes its output, so the store, the PGN NAG and the selection all read the same glyph. `getExtendedAnnotation(facts, glyph)` appends `□` when `OnlyMove` is present and (`margin ≥ 20` or the second line is mate against the mover), `⊗` when `Miss` is present, `⊕` when `time.under_pressure` and the glyph is `?`/`??`; the three reuse the reserved NAGs 7, 9 and 138 (src/utils/annotation.ts:185-189) through the existing additive glyph path, only when `extendedGlyphs` is on, so `getAnnotation`'s outputs stay byte-identical.

`selectFacts(facts, glyph, extended, detail, phases)`: `enumerateFacts(facts)` assigns ids in a fixed order (lines in `lines[]` order, `after.hanging`, `delta.newly_attacked`, `delta.newly_hanging`, refutation motifs, `criticality`, `evalDelta`, `opening`, `novelty`, `time`) and `vocabularyOf(fact)` derives each fact's `FactVocabulary` (a line's plies as `MoveRef`s at `{ t: "line", id, ply }`, its outcome as `NumberRef`s, a motif's squares and pieces at the motif's position, a criticality's margin, the opening's move number, the time fact's seconds). The kind is decided first: `??/?/?!` → `evalDrift` when `played.rank === 1`, else `mistake`; `⊗` → `miss`; `!!` → `sacrifice` when a `Sacrifice` fact exists, else `clearlyBest`/`onlyMove` by criticality; `!`/`□` → `onlyMove` or `clearlyBest`; `!?` → `sacrifice` when present, else `mateFound` when present, else `positional`; terminal mate → `mate`; last named position → `opening`; novelty → `novelty`. Output: `FactSelection` with ≤ 3 facts. Weights, higher wins: mate in refutation 100; material cash-in in refutation 90; `Sacrifice` 85; `OnlyMove`/`ClearlyBest`/`MateFound` 80; mate transitions 78; `second` line 75; newly hanging piece 70 only when the refutation PV captures that square within `shown_plies` (otherwise it is listed in "What this is based on" and never narrated); motif on the refutation 65; better-move line 60; threat line with non-positional outcome 55; novelty 50; last named position 50; Eval-outcome lines 45; king-zone delta ≥ 2 40; passed pawn/promotion 35; phase boundary 30; time 20; newly attacked defended piece 15; positional notes 10. Mandatory by kind: `mistake` → `refutation` + `best`; `evalDrift` → `refutation` only; `sacrifice` → `Sacrifice` + `second` + (`OnlyMove` | `ClearlyBest` when present); `onlyMove`/`clearlyBest` → the criticality + `second` (+ `threat` when present); `mateFound` → `MateFound` + the mating line; `miss` → `Miss`; `opening` → `opening`; `novelty` → `novelty`; `mate` → the played move. Triggers: `keyMoments` = glyph ∈ {??, ?, ?!, !!, !, !?, ⊗, □} ∪ {last named position, novelty, phase boundaries, mate transitions, terminal mate}; `mistakes` = ?!/?/??/⊗ only; `allMoves` adds a `positional` sentence only when a fact of weight ≥ 35 exists, otherwise nothing.

### 6.3 NARRATION, deterministic (TS, `narrate.ts` + `segmentize.ts`, i18n)

`narrateMove(selection, facts, t)` composes at most three sentences; a line fact is rendered by `Commentary.<Kind>.<Slot>.<Outcome>` where `Slot ∈ {Line, Best, Second, Threat}` and `Outcome ∈ {Mate, Material, Eval}` (`match(outcome).with(...).exhaustive()`, so an unhandled `LineOutcome` is a type error); criticality, motif, opening, novelty, time and mate facts have one key each (`Commentary.Sacrifice`, `Commentary.OnlyMove`, `Commentary.ClearlyBest`, `Commentary.MateFound`, `Commentary.Motif.Fork`, `Commentary.Opening`, `Commentary.Novelty`, `Commentary.Time`, `Commentary.Mate`). Interpolations: `{{san}}`, `{{best}}`, `{{secondSan}}`, `{{line}}`, `{{piece}}`, `{{square}}`, `{{list}}`, `{{pawns}}`, `{{mateIn}}`, `{{eval}}`, `{{margin}}`, `{{seconds}}`, `{{side}}`, `{{move}}`. Piece names come from `Commentary.Piece.<kind>`; sides from new sentence-case keys `Commentary.Side.White/Black` (the existing `Common.WHITE`/`Common.BLACK` are the all-caps labels, src/translation/en-US.json:230/:176); `{{eval}}` renders through `formatScore` (score.ts:17-32); `{{list}}` through `Intl.ListFormat(lang)` (tsconfig `lib: ["esnext"]`) with every item recorded as a ref. `segmentize()` interpolates each chess value as an opaque sentinel (`\u0001k\u0002`), calls `t()`, splits the result, substitutes the real token and records it in `refs` together with its formatted span, so `"12,5"` in de-DE is one recorded span; a translator can reorder but cannot produce an unrecorded square, SAN or digit, so every deterministic sentence passes the validator by construction in every locale. Example en-US keys: `Commentary.Mistake.Line.Material` "{{san}} allows {{line}}, winning the {{piece}} ({{pawns}} pawns' worth)."; `Commentary.Mistake.Line.Eval` "{{san}} allows {{line}}; the engine prefers {{side}} ({{eval}})."; `Commentary.Mistake.Line.Mate` "{{san}} allows {{line}} and mate in {{mateIn}}."; `Commentary.Mistake.Best.Eval` "Better was {{best}}, keeping the position at {{eval}}."; `Commentary.EvalDrift.Line.Eval` "The engine's assessment fell after {{san}}: {{line}} ({{eval}})."; `Commentary.OnlyMove` "{{san}} was the only move: every alternative loses at least {{margin}}% winning chances."; `Commentary.OnlyMove.Second.Material` "Nothing else works: {{secondSan}} runs into {{line}}, losing the {{piece}}."; `Commentary.ClearlyBest.Second.Eval` "The alternative {{secondSan}} is clearly worse: {{line}} ({{eval}})."; `Commentary.Sacrifice` "{{san}} offers the {{piece}} on {{square}} ({{pawns}} pawns' worth by exchange)."; `Commentary.MateFound` "{{san}} starts a forced mate in {{mateIn}}."; `Commentary.Motif.Fork` "The {{piece}} on {{square}} forks {{list}}."; `Commentary.Opening` "The game follows the {{name}} ({{eco}}) through move {{move}}."; `Commentary.Novelty` "{{san}} leaves the games in your reference database."; `Commentary.Time` "Played with {{seconds}} s on the clock."; `Commentary.Mate` "{{san}} is checkmate." Keys are dynamic, so `"Commentary.*"` joins `preservePatterns` (i18next.config.ts:34); only en-US must be complete (`fallbackLng`, src/index.tsx:53). Arrows derive from selected facts (refutation's first two plies red, better move green, hanging or forked squares yellow).

`narrateGame(gameFacts, selections, t)` renders from `GameFacts` only: opening (last named position), novelty move when present, phase plies, the three swings with their kinds, `decisive`, per-colour accuracy, the terminal move. Takeaways are aggregates with minimum support, each citing indices: (a) a motif kind that appears in ≥ 2 selected refutation facts ("Twice material was lost to a {{motif}}: {{list}}"); (b) ≥ 2 `Miss` facts ("{{n}} chances to punish a mistake went unused: {{list}}"); (c) the phase containing `decisive` ("The game was decided in the {{phase}} with {{san}}"); (d) ≥ 2 mistakes with `under_pressure` ("Both {{list}} were played with under {{seconds}} s left"). Fewer than three are emitted rather than padded; the narrative's numbers come from `GameFacts`' vocabulary, so it validates like any other text.

### 6.4 NARRATION, LLM (Rust transport, typed request, slot-templated response)

Contract. The orchestrator assembles a typed `NarrationRequest { lang, perspective, game: NarrationGame, items: NarrationItem[] }` where each `NarrationItem` carries `fen_before`, the kind, the glyph, the budget and `facts[]` as `NarrationFact { id, kind, vocabulary, textHint }` (`textHint` is the deterministic draft sentence that cited that fact): facts, their vocabulary and drafts only; never player names, PGN headers, user comments, file names. `NarrationGame` carries `GameFacts` as `NarrationFact`s with `game:` ids plus the turning-point items. Rust (`prompt.rs`) builds the system prompt and user JSON, chunks items into requests of ≤ 12 plies plus one narrative request, sends them sequentially and emits `NarrationChunk` per chunk.

The model does not write board references: it writes a **template** per sentence whose placeholders name fixed slots, chooses slot values from the cited facts' vocabulary, and declares the relations it asserts as **claims**. We render the sentence through the same `segmentize` substitution as deterministic drafts, so a piece word, square, SAN or number can only appear in displayed text if we put it there from a fact.

```rust
pub struct Slots { pub m0: Option<MoveRef>, pub m1: Option<MoveRef>, pub p0: Option<PieceRef>, pub p1: Option<PieceRef>,
                   pub s0: Option<String>, pub s1: Option<String>, pub n0: Option<NumberRef>, pub n1: Option<NumberRef>,
                   pub side: Option<Side>, pub pieces: Vec<PieceRef> }     // fixed names: non-recursive, closed schema
#[serde(tag = "kind")]
pub enum Claim {
    Attacks { attacker: PieceRef, target: PieceRef },
    Forks { attacker: PieceRef, targets: Vec<PieceRef> },
    Pins { attacker: PieceRef, pinned: PieceRef },
    Hanging { piece: PieceRef },
    WinsMaterial { side: Side, pawns: i32, line: u8 },
    Mates { side: Side, mate_in: u8, line: u8 },
    Better { side: Side, at: PositionRef },
    Threatens { side: Side, line: u8 },
}
pub struct NarratedSentence { pub template: String, pub slots: Slots, pub claims: Vec<Claim>, pub fact_ids: Vec<String> }
```

Prompt shape (adapted from kibitz, GPL-3.0-or-later; `prompt.rs` carries its copyright notice and attribution in a header comment): role = "you turn verified chess facts into short coaching sentences; each fact comes with a draft sentence; rephrase and connect, never add"; the five prohibitions (nothing outside each fact's vocabulary exists; never re-judge the classification; never pad; never assert intent, attribute lines to the engine; never invent a number); write templates with `{{m0}}`-style placeholders and never a square, move, piece word or digit as literal text; every relation (attacks, forks, pins, hangs, wins material, mates, is better, threatens) must be a claim; address the player as "you" only when `perspective` is a side, else "White"/"Black"; write in `lang`; order per item: verdict → how it is punished or what the better move achieves → at most one positional reason; budgets 60/140/220. Output is JSON constrained with `output_config.format`:

```json
{ "items": [ { "index": 23, "sentences": [ { "template": "{{m0}} allows {{m1}}, and the {{p0}} is lost.",
     "slots": { "m0": {"san":"Rc8","side":"black","at":{"t":"before"}}, "m1": {"san":"d4","side":"white","at":{"t":"line","id":2,"ply":1}},
                "p0": {"piece":"knight","square":"e5","at":{"t":"after"}}, "p1": null, "s0": null, "s1": null, "n0": null, "n1": null, "side": null, "pieces": [] },
     "claims": [ { "kind": "winsMaterial", "side": "white", "pawns": 3, "line": 2 } ],
     "factIds": ["line:2"] } ] } ],
  "narrative": { "sentences": [ { "template": "...", "slots": {...}, "claims": [], "factIds": ["game:swing:0"] } ],
                 "takeaways": [ { "indices": [23], "sentences": [ ... ] } ] } }
```

Every object in `NARRATION_SCHEMA` sets `additionalProperties: false` with a full `required` list (nullable slots), the schema is non-recursive and uses no `minLength`/`maximum` (unsupported by structured outputs). Anthropic request: `POST {base_url | https://api.anthropic.com}/v1/messages`; headers `content-type: application/json`, `x-api-key`, `anthropic-version: 2023-06-01`, `anthropic-beta: server-side-fallback-2026-07-01`; body `{ model: "claude-opus-5-5", max_tokens: 16000, fallbacks: "default", system: [{ type: "text", text: SYSTEM, cache_control: { type: "ephemeral" } }], messages: [{ role: "user", content: <json> }], output_config: { effort: "low", format: { type: "json_schema", schema: NARRATION_SCHEMA } } }`. No `thinking` field is sent (on `claude-opus-5-5` thinking is always on and `effort` is the depth control; `low` is explicit because the model's default is `medium`; thinking tokens count against `max_tokens`, hence the 16000 non-streaming default); no prefill and no forced `tool_choice` (both rejected on this model). `stop_reason` is checked before `content`: `refusal` → `Refused { category: stop_details.category }`, `max_tokens` → `MalformedResponse`. The response's `model` and `usage` are returned as `Served` in each `NarrationChunk` and land in `Provenance`. 429 honours `retry-after` once, then fails the chunk; 401/403 → `Auth`, 5xx/529 → `Provider`, connect/read/120 s timeout (`Client::builder().timeout(Duration::from_secs(120))`) → `Network`. The OpenAI-compatible adapter posts to `{base_url}/v1/chat/completions` with the same text and `response_format: { type: "json_schema", json_schema: { name, schema, strict: true } }`, degrading to `json_object` plus the schema in the prompt on a 400; it attaches `Authorization: Bearer` only when a secret is stored and otherwise sends no auth header, so Ollama/LM Studio/llama-server work with no key; serde enforces the shape either way. One repair round (the violation list sent back) runs for the narrative chunk only.

### 6.5 VALIDATION (TS, pure, `validate.ts`)

`validateSentences(sentences, facts, selection): { narration: ValidatedNarration; result: ValidationResult }` is the only code that may mint `ValidatedNarration`. Positions: `fenBefore`, `fenAfter`, every `LineFacts` replayed with chessops `parseSan`/`play`. Deterministic sentences arrive with their recorded refs; LLM sentences are rendered from `template` + `slots` by `segmentize`, which records the refs. Checks, locale-independent first:

1. `unknownFact`/`uncitedFact`: every sentence cites ≥ 1 fact id that exists for its index (or a `game:` / `<index>/` id for the narrative).
2. `slotNotInFacts`: every slot value ∈ ∪ vocabulary(cited facts) member-wise (moves by `san`+`side`+`at`, pieces by `piece`+`square`+`at`, squares, numbers by `value`+`unit`, sides), plus the played move; `line` indices in claims must be cited lines.
3. `pieceNotOnBoard`, `moveNotLegalForSide`, `wrongSide`: each piece slot stands on its square at `at`; each move slot is legal for the declared side at `at` (`positionFromFen` + `parseSan`). The vocabulary is board-derived, so a failure here is a facts bug and is logged as such.
4. `literalInTemplate`: the template text (before substitution) contains no `[a-h][1-8]` (not word-bounded, so `Sf3` and figurine forms are caught), no digit run, and no piece word from `Commentary.Piece.*` of the request language where that table exists; the only chess content is slots. Unused slot placeholders or placeholders without a value fail the same rule.
5. `claimUnsupported`/`claimContradicted`: each claim must be entailed by a cited fact's fields: `Forks` ↔ `Motif::Fork { from, targets }` (attacker square and every target present), `Attacks` ↔ an `AttackFact` or fork target, `Pins` ↔ `Motif::Pin`, `Hanging` ↔ `Motif::Hanging` with the same side, `WinsMaterial` ↔ `LineOutcome::Material` of that line with the same sign for that side, `Mates` ↔ `LineOutcome::Mate` with the same `mate_in` and mating side, `Better` ↔ the sign of the eval at `at` (|cp| > 100 or mate), `Threatens` ↔ a cited `Threat` line. A sentence inverting a relation ("the knight forks the pawn") carries a claim whose attacker is not the fact's attacker and fails here.
6. `relationWithoutClaim`: a relation word (attack/fork/pin/hang/win/mate/better/threat; English lexicon plus small verb tables for de-DE, ru-RU, zh-CN) in a template with no claim of that kind; without a table the rule is skipped and 1-5 carry the guarantee.
7. `magnitude`/`sideBetter`: "winning/decisive/crushing" only if |cp| > 300 or mate; "roughly equal" only if |cp| ≤ 100; "White/Black is better" must carry a `Better` claim (same lexicon policy). `unknownMotif`: motif words only if a cited fact of that kind exists; "outpost/weak square/hole" never.
8. `forbiddenToken`: "centipawn", "cp", `{`, `}`, `[%`, `<`, `>`, URLs, markdown emphasis. `budget`: `Intl.Segmenter(lang, { granularity: "word" })` word-like segments of the rendered sentence ≤ budget × 1.2.

Failing sentences are dropped; if every sentence of an item fails, the deterministic draft stays; the `ValidationResult` is stored in `provenance.validation`. Deterministic output is validated in tests in every locale that has a `Commentary.*` table.

## 7. IPC contract

All new commands follow `#[tauri::command] #[specta::specta]`, return `Result<T, Error>` for programmer/IO failures only, and are appended to `collect_commands!` (src-tauri/src/main.rs:111-172); events to `collect_events!` (:173-180).

```rust
// chess.rs (signature unchanged; MultiPV = options.multipv.unwrap_or(2); terminal fens pushed with no search;
// MoveAnalysis.fen/.terminal filled; facts/threat post-pass; ProgressGuard on every exit)
pub async fn analyze_game(id: String, engine: String, go_mode: GoMode, options: AnalysisOptions,
    uci_options: Vec<EngineOption>, state: tauri::State<'_, AppState>, app: tauri::AppHandle)
    -> Result<Vec<MoveAnalysis>, Error>;
pub async fn cancel_analysis(id: String, state: tauri::State<'_, AppState>) -> Result<(), Error>; // send_replace(true) on any job id
// pub(crate) async fn search_once(proc, reader, go_mode, cancel_rx) -> Result<Vec<BestMoves>, Error>: the read loop at :569-597
// extracted; skips lines whose ParsedLine.bound != Exact exactly like :373-377; returns the last complete exact MultiPV set;
// used per position, for threat lines and by explain_position. PR 10 promotes EngineProcess::{new,set_options,go,stop,kill}
// (:53/:82/:129/:137/:143, private today) and parse_uci_attrs (:193) to pub(crate) so commentary::explain can call them.

// commentary/explain.rs NEW
#[derive(Deserialize, Debug, Type)] #[serde(rename_all = "camelCase")]
pub struct ExplainOptions { pub fen: String, pub moves: Vec<String>, pub multipv: u16, pub with_threat: bool, pub threads: Option<u16> }
#[derive(Serialize, Debug, Type)] #[serde(rename_all = "camelCase")]
pub struct PositionExplanation { pub parent_lines: Vec<BestMoves>, pub lines: Vec<BestMoves>, pub threat: Option<BestMoves>, pub facts: Option<MoveFacts> }
pub async fn explain_position(id: String, engine: String, go_mode: GoMode, options: ExplainOptions,
    uci_options: Vec<EngineOption>, state: tauri::State<'_, AppState>) -> Result<PositionExplanation, Error>;
// rejects GoMode::Infinite with Error::InvalidGoMode; spawns its own EngineProcess (never engine_processes);
// search_once on moves[..n-1], on the position, optionally the null-move threat; kills the process on every exit.
// Takes no caller-supplied lines: BestMoves is Serialize-only (:154) and live lines may be bound or dtz.

// opening.rs NEW
#[derive(Serialize, Debug, Clone, Type)] #[serde(rename_all = "camelCase")]
pub struct OpeningMatch { pub eco: String, pub name: String, pub pgn: Option<String>, pub index: u32 }
pub fn get_opening_match(fens: Vec<String>) -> Result<Option<OpeningMatch>, Error>;

// commentary/narrate.rs NEW (request types: Deserialize, Debug, Type; result types: Serialize, Debug, Clone, Type; all camelCase)
pub struct NarrationFact { pub id: String, pub kind: FactKind, pub vocabulary: FactVocabulary, pub text_hint: String }
pub struct NarrationItem { pub index: u32, pub fen_before: String, pub move_number: u32, pub san: String, pub side: Side,
    pub classification: String, pub kind: CommentaryKind, pub budget_words: u16, pub facts: Vec<NarrationFact> }
pub struct NarrationGame { pub facts: Vec<NarrationFact>, pub result: String, pub turning_points: Vec<NarrationItem> }
pub struct NarrationRequest { pub lang: String, pub perspective: Perspective, pub game: NarrationGame, pub items: Vec<NarrationItem> }
pub struct NarratedMove { pub index: u32, pub fen_before: String, pub san: String, pub sentences: Vec<NarratedSentence> }   // fen_before copied by Rust from the item
pub struct NarratedTakeaway { pub indices: Vec<u32>, pub sentences: Vec<NarratedSentence> }
pub struct NarratedGame { pub sentences: Vec<NarratedSentence>, pub takeaways: Vec<NarratedTakeaway> }
#[derive(Event)]
pub struct NarrationChunk { pub id: String, pub run_id: String, pub chunk: u32, pub chunks: u32,
    pub items: Vec<NarratedMove>, pub narrative: Option<NarratedGame>, pub served: Served }
pub async fn narrate_game(id: String, run_id: String, request: NarrationRequest, config: NarratorConfig,
    state: tauri::State<'_, AppState>, app: tauri::AppHandle) -> Result<NarrationRun, Error>;
pub async fn test_narrator(config: NarratorConfig, app: tauri::AppHandle) -> Result<Option<NarrationFailure>, Error>;

// commentary/secrets.rs NEW (webview → Rust only; the secret is never returned)
pub async fn set_commentary_secret(provider: ProviderKind, secret: String, app: tauri::AppHandle) -> Result<(), Error>;
pub async fn clear_commentary_secret(provider: ProviderKind, app: tauri::AppHandle) -> Result<(), Error>;
pub async fn get_commentary_secret_status(provider: ProviderKind, app: tauri::AppHandle) -> Result<SecretStatus, Error>;
```

How errors become typed. `crate::error::Error` keeps serialising to a bare string (src-tauri/src/error.rs:187-203): changing that would alter every `Result<_, string>` binding, and whether tauri-specta rc.21 emits a union for a tagged enum in `Result`'s `E` position is unverified (`DatabaseInfo`, db/mod.rs:700, is a plain struct). Provider failures are therefore *data*: `narrate_game` returns `Ok(NarrationRun::Failed { .. })`, `test_narrator` returns `Ok(Some(failure))`, and a 429 can never discard annotations. TS maps `NarrationFailure.kind` with ts-pattern `.exhaustive()` to `Errors.Commentary.*` (already in `preservePatterns`) and renders through `t()`; `Cancelled` is silent; `detail` appears only in the inline Alert's expandable section, never in a toast title. Transport `Error`s re-thrown by the generated wrapper (src/bindings/generated.ts:28) are caught and shown as `Errors.Unknown`.

Progress and cancellation end to end. `AppState.jobs: JobRegistry` replaces `analysis_cancel_flags` (main.rs:92; its only users are chess.rs:467-472 and :485-535/:632, a small diff). `analyze_game` calls `jobs.insert(id, JobPhase::Engine)`, which cancels and replaces any handle under the same id (today a second run orphans the first flag, chess.rs:486-488); the handle keeps a `watch::Receiver` so the channel never closes and `cancel_analysis(id)` uses `send_replace(true)`, which stores the value whether or not a stage is currently awaiting it (`watch::channel` precedent: src-tauri/src/game.rs:633). `search_once`, the facts post-pass (checked between positions) and every provider call `subscribe()` from the stored handle; `search_once` runs `tokio::select! { line = reader.next_line() => …, _ = rx.changed() => { proc.stop(); drain to bestmove; cancelled } }`, so a `Depth 30` report cancels within one `info` line instead of one `go`; each provider call runs `tokio::select! { r = client.post(url).json(&body).send() => r, _ = rx.changed() => Err(Cancelled) }`; dropping the reqwest future aborts the TCP request. The `ProgressGuard` created at the top of each command emits `(stage, 100, finished: true, status)` and removes the job on every exit. There is no handoff phase and no watchdog: when `finalize == Some(false)` the guard emits `(Engine, 100, finished: false, Running)` and removes the job; TS owns the gap (select, draft, validate, milliseconds); `narrate_game` inserts its own job under the same id and emits `(Commentary, 0, Running)` first; on any non-handoff exit of the orchestrator TS calls `commands.clearProgress(id)` and emits a terminal `progressEvent` itself (precedent: src/utils/chess.com/api.tsx:139-149). A webview reload mid-gap leaves `(Engine, 100, Running)` in the ProgressStore; the mount reconciliation treats that item as stale when `treeRegistry.activeRuns` holds no run for the tab (a reload empties it) and clears it. On the TS side `applyReport` runs in `try/finally`; the `catch` branch calls `commands.cancelAnalysis(id)` and `clearProgress(id)`, and the orchestrator checks `report.runId` and `!cancelled` before issuing `narrateGame`. `ProgressButton` treats `status === "cancelled" | "failed"` as `clear()` + `setInProgress(false)` without `labels.completed`; `labels.finalizing` (src/components/common/ProgressButton.tsx:61) shows "Explaining moves…" at 100. `closeTab` (src/components/tabs/BoardsPage.tsx:68-70) additionally calls `cancelAnalysis(\`report_${value}\`)`, which also closes #893's orphan engine.

Bindings. After any Rust change run `pnpm dev` once (the debug build writes src/bindings/generated.ts, main.rs:182-188) and commit the file in the same PR; src/bindings/index.ts is untouched because `BestMoves`' serialized shape is unchanged. The builder construction moves into `fn specta_builder() -> tauri_specta::Builder` and a `#[test]` exports it to a temp file and diffs it against the committed file, so drift fails `cargo test`.

## 8. Surfaces and UX

Report flow. The modal (src/components/panels/analysis/ReportModal.tsx:160-177) gains, under the existing checkboxes: "Explain moves" (`explain`, default on; subtitle "offline, deterministic"), a detail Select (`keyMoments` / `mistakes` / `allMoves`), "Threat lines (good moves only)" (`threatLines`, "+ up to 12 searches"), and "Narrate with {provider or host}" (visible when `llmEnabled && consentVersion ≥ 1 && (secretStatus.configured || (provider === "openaiCompatible" && baseUrl !== null))`). `reportSettingsAtom` moves from ReportModal.tsx:13 to src/state/atoms.ts and is exported (it stores an engine *id*). Submitting captures `takeMainlineSnapshot(root, headers)` = `{ rootFen, runId: crypto.randomUUID(), plies: [{ fen, san, halfMoves, clock }], increment_s, initial_s }` via `treeIteratorMainLine` (src/utils/treeReducer.ts:56) and `parseTimeControl` (src/utils/clock.ts:39, exported); `beginReport(snapshot)` stores `runId` and `stage: "engine"`. When the engine promise resolves, the orchestrator writes `MoveFacts` into `commentaryFactsFamily`, runs `deriveGlyphs(analysis, snapshot)` once, then `getExtendedAnnotation` + `selectFacts` + `narrateMove` + `validateSentences` per selected index and `gameFacts` + `narrateGame`, then one store action `applyReport(analysis, glyphs, snapshot, records, narrative)` does, inside a single `produce`: the refactored `addAnalysis` (consuming the same `glyphs`), extended glyphs, `node.commentary = record`, and `report.narrative`. A write lands on a node only where `samePosition(node.fen, analysis[i].fen) && node.san === snapshot.plies[i].san && samePosition(parent.fen, analysis[i-1].fen)`, with `samePosition(a, b) = makeFen(positionFromFen(a).toSetup()) === makeFen(positionFromFen(b).toSetup())` (chessops canonical form on both sides; `root.fen` is the untrimmed-counter user string, treeReducer.ts:84, while Rust emits `Fen::from_position`), never string equality. The user sees reasons the moment the engine finishes. If narration is on, `NarrationChunk`s upgrade sentences in place: each `NarratedMove` is rendered, validated, merged (dropped sentences keep the draft sentence with the same `factIds`) and written through `applyCommentary` with the same checks; the card badge flips from "Offline" to "{served.model}" per move.

ReportPanel layout (ReportPanel.tsx:56-93): ProgressButton → `GameNarrative` Paper (narrative, takeaways, then "Key moments" rows: move number, SAN coloured by `ANNOTATION_INFO`, first sentence; click → `goToMove(path)`) → AccuracyCards → EvalChart (its `LoadingOverlay`, src/components/common/EvalChart.tsx:172, follows `stage === "engine"` only) → GameStats → `CommentaryCard` for the current node. The card shows the kind label (`Commentary.Kind.*`), the perspective badge ("you are Black" / "third person"), the text via `CommentaryText` (declared `refs.moves` render as chips that call `makeMoves` with the cited line prefix, as `AnalysisRow` does at src/components/panels/analysis/AnalysisRow.tsx:203; a truncated line offers "show full line" from `cut_reason`/`pv_san`), an "Arrows" toggle (`commentaryArrowsFamily(tab)`, consumed by Board.tsx beside the `bestMovesFamily` arrows at :114; never written to `node.shapes`), a collapsible "What this is based on" list of all enumerated facts, and "Add to my notes" (`appendComment(path, text)`).

Analysis-by-engine on-demand explain. `ExplainButton` sits in the `Stack` at src/components/panels/analysis/AnalysisPanel.tsx:156, above the engine Accordion, so it is visible with one engine (the `EngineSummary` Paper at :131-155 renders only when `loadedEngines.length > 1`). Click or hotkey `EXPLAIN_POSITION` (`{ name: "Explain current position", keys: "e" }`; `e` is unused in src/state/keybinds.ts:14-58 and `defaultStorage` merges new keys at :62-88) resolves the engine as `engines.find(e => e.id === reportSettings.engine)?.path ?? first loaded LocalEngine`, calls `explainPosition(\`explain_${tab}\`, enginePath, { t: "Time", c: 1000 }, { fen: root.fen, moves: getVariationLine(root, position), multipv: 3, withThreat: true, threads: 1 })`, runs the same `getExtendedAnnotation`/`selectFacts`/`narrateMove`/`validateSentences` with the glyph from `getAnnotation` over the returned parent/own lines, and shows the same `CommentaryCard` under the engine list with a "Keep" button that persists the record to that node through `applyCommentary` (setting `dirty` only when `exportToPgn` is on). It always searches on its own process (≈ 3 s) and never hijacks the live `(tab, engine)` process (chess.rs:334-355); reusing `engineMovesFamily` lines was rejected because facts are computed in Rust and `BestMoves` cannot be received over IPC without a new input type (§7). This answers #637 within the maintainer's remark in #72 that live analysis "would be better".

Inline notation. `CompleteMoveCell` (src/components/common/CompleteMoveCell.tsx:200) renders, when `showCommentaryFamily(tab)` is on (hotkey `TOGGLE_COMMENTARY: "shift+c"`, free), a one-line `CommentaryText` with the first sentence and a "…" that focuses the card; `TableNotation` gets a `CommentaryItem` segment beside `CommentItem` (GameNotation.tsx:275-279); the memo comparator (CompleteMoveCell.tsx:205-218) gains `prev.commentary === next.commentary`.

Tab and route switch: the board tab unmounts (BoardsPage.tsx:187 `keepMounted={false}`), `BoardsPage` itself is only the `/` route (src/routes/index.tsx:4-7), and the store is recreated from sessionStorage with `commentary` intact; `useProgress` restores the bar from Rust (src/hooks/useProgress.ts:7-13); `TreeStateProvider` (src/components/common/TreeStateContext.tsx:16) registers the live store in `treeRegistry` on mount and unregisters on unmount; `CommentaryListener` is mounted once in `RootLayout` (src/routes/__root.tsx:88, the always-mounted shell around `<Outlet/>`), resolves the payload's store at event time, applies the chunk, or appends it to `treeRegistry.parked: Map<tabId, NarrationChunk[]>` keyed by `(tab, run_id)`; the next registration of that tab drains its queue in order. Navigation computes nothing (the card reads `currentNode().commentary`). Re-run: `applyReport` replaces `commentary` whenever `fingerprint` differs and the narrative always, never `comment`, user annotations or `shapes`.

## 9. Settings, secrets, privacy

Atoms (src/state/atoms.ts, "Settings" block near `telemetryEnabledAtom` :215): `commentarySettingsAtom = atomWithStorage("commentary-settings", defaults, createZodStorage(commentarySettingsSchema, localStorage), { getOnInit: true })`; zod `.default()` on every field keeps later additions safe for existing installs (the `engineSchema` precedent, src/utils/engines.ts:69) and `getOnInit` lets `CommentaryListener` read it outside React. Per-tab UI atoms `showCommentaryFamily`, `commentaryArrowsFamily` via `tabValue` (:310). `perspective: "auto"` is resolved in TS before anything crosses IPC: `resolvePerspective` lower-cases `headers.white`/`headers.black` and matches them against every `Session`'s `lichess.username`, `chessCom.username` and `player` alias (`sessionsAtom`, atoms.ts:213; src/utils/session.ts:4-20); exactly one side matching → that side, otherwise `both` (third person). `headers.orientation` is not used: `getPgnHeaders` sets it to `"white"` for every parsed PGN (src/utils/chess.ts:529).

Settings page: new `SettingCategory` `"commentary"` (src/components/settings/SettingsPage.tsx:78), `categoryInfo.commentary` with `IconMessage2`, a tab and panel, and rows: explain, detail, extended glyphs, threat lines, a `TelemetrySwitch`-style switch (:128) that opens the consent modal before enabling the LLM, and `CommentarySettings` (provider Select, model TextInput prefilled `claude-opus-5-5`, base URL, Mantine `PasswordInput` with Save/Clear calling `set/clear_commentary_secret`, status "Key configured (…abcd)" or, for `openaiCompatible` with a base URL and no key, "Local server, no key", "Test connection" calling `test_narrator`, perspective, language, `exportToPgn`). All strings are `Settings.Commentary.*`.

Where the key lives and why. The key is a billable credential, unlike the `preference:read` lichess token kept in plaintext localStorage (`sessionsAtom`, atoms.ts:213) that #898 already criticises. It is written once over IPC into `<app_data_dir>/commentary/secrets.json` created with mode `0o600` on Unix, read only inside `commentary::secrets` when a request is built, and never returned (`SecretStatus` only). `NarratorConfig` has no key field; `Redacted` wraps every provider detail so a future `log::error!("{:?}", failure)` cannot leak it; `base_url` is validated in Rust (`https:` unless the host is `localhost`, `127.0.0.1` or `[::1]`, else `InsecureBaseUrl`), so a mistyped `http://` proxy never receives the key in cleartext. This is isolation, not a keychain: `fs:scope-appdata-recursive **` (capabilities/main.json:35) means compromised webview code could still read the file; `SecretStore` is a trait so the fork can add a `keyring` store behind a cargo feature. Help ▸ "Clear saved data" (src/routes/__root.tsx:284) also calls `clear_commentary_secret`. `provider/*.rs` never logs bodies or formats `x-api-key`/`Authorization`.

Opt-in flow and what is sent. The consent modal lists exactly the `NarrationRequest` contents (FENs, SAN moves, engine evaluations and lines, detected motifs, opening name, game result, draft sentences, UI language, the resolved perspective) and what is never sent (player names, PGN headers, your comments, file paths, database names), plus "requests go directly from this computer to {host}; nothing goes through En Croissant's servers" and, for Anthropic, "the API may serve a request with a different Claude model than the one configured; the model that answered is shown on each card"; accepting sets `consentVersion = 1`; a "Preview request" link renders the exact JSON for the current game. The default for everyone is the deterministic path; "Generate report" never contacts a provider implicitly; PostHog is untouched. `baseUrl` applies to both adapters and the OpenAI-compatible adapter needs no key, so Ollama/LM Studio/llama-server make the feature usable with no paid API at all (#704).

## 10. Persistence and PGN round-trip

Node-resident vs side maps. `TreeNode.commentary` is node-resident because the node object is the only identity that survives `promoteVariation` (tree.ts:659-671), `deleteMove` (:640-657) and `makeMove` dedupe/unshift (:574-595) with zero fix-up code, and an optional field never assigned by `createNode`/`defaultTree` costs zero fixture edits. `MoveFacts` (≈ 1-2 KiB per index) stay in `commentaryFactsFamily`, keyed by line key `${root.fen}:${uci path}`, bounded to two runs, not persisted, regenerable. `report.narrative` (≈ 2-4 KiB) is persisted with `ReportState`.

sessionStorage budget. A `MoveCommentary` with refs is ≈ 0.8-1.5 KiB, so a fully commented 80-ply game adds ≈ 70-120 KiB on top of today's ≈ 21-32 KiB tree, against a ≈ 5 MB area shared by every tab ever opened (closed tabs are never removed, BoardsPage.tsx:47-72). The robustness PR makes `flush()` (src/state/store/debouncedStorage.ts:9-19) try/catch per key, keep the failing key pending, `warn()` once and continue. No `partialize` in v1: zustand runs it on every `set()`, including 5/s live `setScore`, so a deep walk there would be a regression; the hook point (tree.ts:518-521) is documented for dropping `refs` if quota ever bites.

PGN. Export is explicit and off by default. `getPGN`/`getMoveText` gain `commentary: PgnSafeText | null` per node (`saveToFile`, src/utils/tabs.ts:171, and `copyPgn` pass `exportToPgn ? toPgnSafe(...) : null`; `copyVariationPgn` never). `toPgnSafe` replaces `{`/`}` with `(`/`)`, breaks `[%` into `[ %`, strips control characters and returns the brand; the same function now guards the user `comment` (the existing hazard at src/utils/chess.ts:117-118). Replacement, not escaping: src-tauri/src/pgn.rs:87-90 toggles `in_comment` on every `}` and pgn_reader ends a comment at the first `}`, so a `\}` escape would corrupt multi-game files. When on, the `{}` block becomes `[%eval …] [%clk …] [%csl …][%cal …]<user comment> <commentary text>`; the narrative goes into the root comment the same way, preceded by one provenance sentence ("Commentary by En Croissant <version>, <source>[, <model>]"), which survives both save paths because `write_db_game` re-imports comments through `encode_comment` (src-tauri/src/db/encoding.rs:22-28). The standard `[Annotator "…"]` tag is set too through `headers.other` (chess.ts:208-210), with the stated limitation that it survives only for file-origin games: `Importer::header` (src-tauri/src/db/mod.rs:330-384) whitelists White/Black/*Elo/TimeControl/ECO/Round/Date|UTCDate/UTCTime/Site/Event/Result/FEN and drops it on DB write-back. On re-import (`innerParsePGN`, chess.ts:367-404) the text is an ordinary comment, interoperable with every tool; the structured record is not round-tripped by design, and the setting says so. A custom `[%ec …]` trailer was rejected: it survives chessops `parseComment` only as literal text, pollutes other tools, and autosave would write it because `extraMarkups` is always true in `saveToFile`.

User comment vs generated text. `comment` is written only by the user (editor) or by the explicit "Add to my notes"; `applyReport`/`applyCommentary` write only `commentary`. Regeneration compares fingerprints and replaces stale `commentary`; it never touches `comment`, user annotations or `shapes`.

## 11. Robustness

| Invariant | Construction that enforces it | Proving test |
|---|---|---|
| I1 No undeclared or unverified board reference | LLM text is rendered from `template` + `slots`; `slotNotInFacts` (slots ⊆ vocabulary of cited facts), `literalInTemplate`, `pieceNotOnBoard`/`moveNotLegalForSide`; declared relations are `claims` checked against the cited fact's fields (`claimUnsupported`/`claimContradicted`); only validate.ts mints `ValidatedNarration` | seeded outputs (knight slot on d5 where a bishop stands; a template containing "Bxe5"; a fork claim with attacker and target swapped; "wins a pawn" claim on an Eval line; "centipawn"; an undeclared square; a slot outside the vocabulary) each dropped with the named rule |
| I2 No verdict without its reason | mandatory facts per kind in `salience.ts`; `evalDrift` exists so a `?` on the engine's own first choice has a defined reason | `mistake` without `refutation`+`best` and `onlyMove` without `second` are unrepresentable; rank-1 `?` yields `evalDrift` with the After line |
| I3 Exactness | `search_once` is the only reader of UCI lines and drops `ParsedLine.bound != Exact`; `ExactEval::from_line` only sees its output; `explain_position` takes no caller lines | scripted `lowerbound`-only set → `best: []`, `facts: None`, no sentence; a mixed set keeps the last exact one |
| I4 Score kind | `EvalValue` has two variants; facts never read `node.score`; `normalizeScore` exhaustive (score.ts:38-47; `dtz` reachable via src/bindings/index.ts:10) | chessdb `dtz` line yields no delta |
| I5 PGN safety | `PgnSafeText` brand via replacement; `getMoveText` accepts only the brand | property test over `}`, `[%`, `<script>`; Rust `pgn.rs` brace tracker `#[test]` fed the output |
| I6 Render safety | generated text only through `CommentaryText`; `Comment.tsx` drops `rehypeRaw`, moves `remarkGfm` to `remarkPlugins`, handles `++u++` with a remark plugin (no new dependency) | DOM vitest: `<script async src>`, `<b ondblclick>`, `[x](javascript:1)` yield no script, no handler, empty href |
| I7 Target integrity | `MoveAnalysis.fen` for every index; writes require `samePosition(node.fen, fen)`, `node.san`, `samePosition(parent.fen, …)` and `report.runId` to match; `getNodeAtPath`'s deepest-ancestor fallback (treeReducer.ts:172-181) never relied upon | promote a variation and delete a move mid-run → records land correctly or are dropped; a root FEN without counters and a header FEN with a non-legal ep square both still receive move 1 |
| I8 Live store only, no lost chunks | `treeRegistry` + per-tab parked queue; `CommentaryListener` in `RootLayout`; no click-time closure writes (ReportModal.tsx:44-45 today) | run completes with no board tab mounted (Settings route) → chunks parked; remount applies them once, in order |
| I9 Cancel everywhere | one `watch` token per id with its receiver kept; `send_replace`; selected inside the engine read loop, between post-pass positions and around every `send()`; `insert` cancels and replaces; `ProgressGuard` terminal event on every exit | mock server that never answers: cancel returns < 100 ms and the socket closes; cancel during the facts post-pass stops within one position; a second run under the same id cancels the first |
| I10 No panic path | the `assert_eq!` at chess.rs:586 is provably unreachable (:577/:579 make `len == real_multipv` hold) and is deleted | scripted irregular MultiPV sequence completes with the previous exact set |
| I11 Secret containment | key only in `secrets.rs`; `NarratorConfig` has no key field (type-level `Expect<Equal<>>` test); `Redacted`; `InsecureBaseUrl`; openaiCompatible without a secret sends no `Authorization` | `format!("{:?}", failure)` built from a body containing `sk-ant-…` has no key; `http://evil` rejected; `http://localhost:11434` + no secret → request without `Authorization` |
| I12 Prompt closure | the prompt is a pure function of `NarrationRequest` | Rust snapshot: a PGN comment reading "ignore previous instructions" yields a byte-identical request |
| I13 Field separation | `setComment` writes `comment`; `applyCommentary` writes `commentary`; editor sync uses `setContent(..., { contentType: "markdown", emitUpdate: false })` when `!editor.isFocused` (AnnotationEditor.tsx:24-44) | `setComment` leaves `commentary` untouched and vice versa |
| I14 Bounded persistence | per-key try/catch in `flush()` and on the raw writes (tabs.ts:123, ImportModal.tsx:112/165/187) | thrown `QuotaExceededError` keeps other keys flushing |
| I15 One glyph source | `deriveGlyphs` is the only caller of `getAnnotation` in the report path; `addAnalysis` consumes its output | refactored `addAnalysis` reproduces store.test.ts:415-460 byte-for-byte; `deriveGlyphs` equals the annotations written |

Other guards: `db_cache` is untouched in v1 (the per-path fix precedes the player-aware slice only); `explain_position` passes `Threads = threads.unwrap_or(1)` and kills its process via `Drop` (src-tauri/src/engine/process.rs:169-173) plus explicit `kill()`; provider failures are chunk-level and drafts stay; `report.inProgress` is reconciled on mount against `getProgress(id)` and `treeRegistry.activeRuns`, so a persisted `true` with no job behind it clears; `moveCommentarySchema.safeParse` on rehydration drops malformed or older records.

## 12. Testing and evaluation

Rust (`cargo test` in src-tauri; the fork adds a `cargo fmt --check` / `clippy` / `cargo test` job and offers it upstream as a one-file workflow PR). `#[cfg(test)]` modules per extractor with one FEN + expected fact list each, in the chess.rs:711-773 style: `geometry.rs` (`see`: pawn takes bishop → 3; bishop takes defended pawn → −2; queen left en prise to a knight → −9; hanging with x-ray defence; a pinned "attacker" does not make a piece hanging; trapped bishop on a7), `motifs.rs` (fork after `5.Nxf7` in `r1b1kbnr/pppp1Npp/8/6q1/2BnP3/8/PPPP1PPP/RNBQK2R b KQkq - 0 5`; royal fork `1.Nc7+` from `q3k3/8/8/1N6/8/8/8/4K3 w - - 0 1` with `is_check` and the king among `targets`; absolute pin, skewer, discovered check, back-rank), `threats.rs` (Qxf7# threat in `r1bqkbnr/pppp1ppp/2n5/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 3 3`), `refutation.rs` (cut at the capture ply with `piece` named; mate outcome from a `Mate(3)` eval whose PV has 4 plies → `pv_reaches_mate: false`; never below 2 plies), `criticality.rs` (OnlyMove vs ClearlyBest on `Mate(4)` vs `+2.5`; MateFound when both lines mate; Sacrifice absent when `is_sacrifice` fires on a declined capture), `time.rs`, `opening.rs` (the D35 transposition `1.d4 Nf6 2.c4 e6 3.Nc3 d5` matches with `eco` and `index`), `chess.rs` (bound filtering keeps the last exact set, `MoveAnalysis.fen` alignment including index 0 and the terminal entry, `search_once` rejects `Infinite`), `provider` (status→failure mapping, `Redacted` never leaks, no-key local request), `prompt.rs` (closure snapshot), `jobs.rs`/`progress.rs` (`ProgressGuard` drop semantics, cancel-and-replace, `send_replace` with no stage awaiting), and the bindings-drift test.

vitest (CI): `classify.test.ts` (`deriveGlyphs` equals the store fixture; ⊗/□/⊕ thresholds; `getAnnotation` unchanged on the existing score.test.ts cases); `salience.test.ts` (deterministic ids and selection, cap of 3, mandatory facts per kind incl. `evalDrift`, hanging selected only when the PV captures it, `allMoves` emits nothing under weight 35, every vocabulary square exists on its board); `gameFacts.test.ts` (accuracy chain breaks on gaps; decisive index; takeaway support); `narrate.test.ts` (snapshot per locale over ten `FactSelection` fixtures; outcome-keyed templates exhaustive; no raw key, brace or markup leak; de-DE, ru-RU and zh-CN included, every deterministic sentence in those locales must pass `validateSentences`, de-DE "12,5" recorded as one span); `validate.test.ts` (the seeded corpus of I1 plus a positive corpus from real outputs; slot rendering; claim entailment); `pgnSafe.test.ts` (property tests, the glyph fix); `snapshot.test.ts` (`samePosition` over short FENs and ep variants); `brands.test.ts` (no `as ValidatedNarration`/`as PgnSafeText` outside validate.ts/pgnSafe.ts); `store.test.ts` additions (I7, I8 with no provider mounted, I13, I15, `getGameStats` gap); `errors.test.ts`; a type-level test for I11.

Evaluation (fork, `scripts/commentary-eval/`): an ACT-Eval-style claim-precision measurement without an LLM judge. A dev-only menu action exports `{ facts, commentary }` per game; the script runs over ≥ 50 real games, treats each slot and claim as an atomic claim, and reports claim precision (verified / total) for deterministic and LLM text separately, drop rate by rule with and without `textHint`, the share of `evalDrift` items, fact coverage, words per move and cost per game from `usage`. Targets before enabling LLM narration by default in the fork: deterministic precision 100% by construction, LLM pre-validation ≥ 95%, post-validation 100%, drop rate < 10%. A 100-sentence human spot-check sheet rates usefulness, because fluency and faithfulness are distinct properties. The §12 fixtures grow from the games this harness exports before PR 7b/8 ship.

What CI runs: upstream unchanged (`pnpm lint:ci` incl. `i18next-cli extract --ci`, `pnpm test`); every TS module above is Tauri-free. Fork CI adds the cargo job.

## 13. Delivery plan

Open a discussion first (CONTRIBUTING.md asks for it) referencing #108, #230, #559, #637, #893, #913, #928 and the `report-comments` branch, framed as "finishing report-comments with engine facts, deterministic first". Then, in order:

| # | PR title | Content | Size | Destination |
|---|---|---|---|---|
| 1 | Report robustness: exact scores, cancellable searches, terminal progress | `ParsedLine`/`Bound` private in chess.rs, bound filter in the extracted read loop, delete the dead assert; `jobs.rs` (top level) watch token with kept receiver replacing `analysis_cancel_flags` (two call sites), cancel-and-replace, `stop` inside the read loop; `ProgressGuard` in progress.rs; `stage`/`status` with `#[specta(optional)]`; `closeTab` cancels `report_${tab}` (#893) | ~380 LOC | upstream |
| 2 | Report errors surfaced, store guards | ReportModal `status === "error"` + `.catch` + `t()` toast and inline Alert; tree.ts:714/:718 `best[0]` guards; `getGameStats` resets its `prevScore` chain across unscored nodes (chess.ts:593-596) + fixture; `flush()` and raw-write try/catch; `normalizeScore` exhaustive | ~180 LOC | upstream |
| 3 | PGN comment hygiene | `getMoveText` glyph fix (chess.ts:73-75); `PgnSafeText` replacement for user comments; tests | ~100 LOC | upstream |
| 4 | Render comments without raw HTML | Comment.tsx: `remarkPlugins`, `++u++` plugin, no `rehypeRaw`; DOM vitest (#898) | ~120 LOC | upstream |
| 5 | Opening match with ECO and ply | `HashMap<Epd, usize>`, `eco` exposed, `get_opening_match`, `getOpeningMatch()`; `get_opening_from_fens` kept as a wrapper | ~180 LOC | upstream |
| 6 | Self-describing analysis | `MoveAnalysis.fen`/`.terminal` (terminal entries pushed, no search), `AnalysisOptions.multipv`/`clocks`/`increment_s`/`initial_s`, `takeMainlineSnapshot`, `samePosition`, `classify.ts` `deriveGlyphs` + refactored `addAnalysis(analysis, glyphs, options)` whose input gains `fen` (store.test.ts:418-456 entries gain `fen`), `runId`, `treeRegistry`, `reportSettingsAtom` moved to atoms.ts, `parseTimeControl` exported | ~320 LOC | upstream |
| 7a | Facts: geometry and motifs | `commentary/facts/{types,geometry,motifs,threats}.rs` incl. `see` + fixtures | ~550 LOC | upstream |
| 7b | Facts: lines, criticality, structure, time, extraction | `refutation.rs`, `criticality.rs`, `structure.rs`, `time.rs`, `mod.rs`, `with_facts` post-pass, bindings | ~550 LOC | upstream |
| 8 | Deterministic move explanations | `salience.ts`, `getExtendedAnnotation`, `narrate.ts`, `segmentize.ts`, `validate.ts`, brands + `brands.test.ts`, `TreeNode.commentary`, `applyReport`, `CommentaryText`, `CommentaryCard`, i18n keys + preservePattern, settings rows, tests | ~1,200 LOC | upstream |
| 9 | Game narrative | `gameFacts.ts`, `narrateGame`, takeaway aggregates, `GameNarrative.tsx`, `report.narrative` | ~350 LOC | upstream |
| 10 | Explain this position | `pub(crate)` on `EngineProcess::{new,set_options,go,stop,kill}`, `parse_uci_attrs`, `search_once`; `explain_position`, threat lines (`with_threats` too), `ExplainButton` at AnalysisPanel.tsx:156, "Keep", hotkey | ~450 LOC | upstream |
| 11 | Export commentary to PGN | export flag, root-comment provenance + `[Annotator]`, settings row | ~120 LOC | upstream |
| 12 | LLM narration | `narrate.rs`, `prompt.rs` (attributed), `provider/*` incl. the no-key local path, `secrets.rs`, `NarrationChunk`, `CommentaryListener` in `RootLayout`, slot/claim schema and validation rules 4-6, consent modal, `CommentarySettings` | ~1,500 LOC | fork first; upstream only after an explicit yes |
| 13 | Evaluation harness and cargo CI | `scripts/commentary-eval/`, workflow, bindings-drift test | ~250 LOC | CI parts upstream |
| 14 | Later | `db_cache` per-path fix; `ContextFacts` (rating gap, own history); Maia sidecar as a fact source | — | fork |

Every slice is independently valuable and compiles on its own: 1-6 fix shipped bugs; 7 alone enables #928's overlay; 8 alone closes #108; 12 is removable without touching 1-11. Given the maintainer's bursty cadence (0 merges May-Aug 2026, 35 open PRs) the fork carries all slices from day one and rebases; each PR description carries its rows of the invariant table.

## 14. Why this is strictly better than chess.com's Game Review commentary

1. Reasons, never labels. A recurring forum complaint about chess.com's coach is that it does not say why; here a `?`/`??` cannot be shown without a cited refutation line and a better move (I2), and the sentence says what the line does (mate, which piece is won, or the evaluation) rather than one shape for all.
2. Declared, verified board facts. chess.com's "loses a knight" (it was a bishop) is the failure class the design closes structurally: every piece, square, move and number in LLM text is a slot we render from a counted fact, and every relation it asserts is a typed claim checked against that fact's fields before display (I1); deterministic text is built from the same facts.
3. Human-length refutations: lines are cut where the consequence is realised (2-6 plies), the full line one click away, instead of "five or six-move sequences".
4. Both sides' mistakes and the quiet brilliance: Miss (⊗), only move (□) distinguished from clearly best, real sacrifices (SEE-defined, with whether they hold), mate found/created/lost, the last named opening position with ECO and the move that left your reference database, what you threaten after a good move (null-move line), and time pressure (⊕) from the clocks in the PGN.
5. Cumulative, not per-move only: phases, swings, the decisive moment and takeaways that are aggregates over verified facts and cite moves (DecodeChess's "per-move, not cumulative" criticism).
6. Always available, offline, unlimited, free: no Diamond gate, no one-review-per-day; the deterministic path needs only the engine already in the app and the LLM path works with a local server and no key.
7. i18n-ready from day one: templates are i18next keys with typed slots; en-US ships complete, the other 15 locales fall back to English until translated.
8. Transparent and inspectable: the card shows every fact the text could have used and draws the selected ones; LLM text carries provenance (the model that answered, token usage, validation counts) and is badged.
9. Final-depth, bound-free, rating-independent classification: glyphs from exact lines at the end of each search, thresholds unchanged, no rating-dependent model that disagrees between quick and full review (run-to-run reproducibility is not promised, §15).
10. Your data, your files: works on any PGN or database game, exports as plain PGN comments, and the provider request contains facts only.

## 15. Risks and open questions

- Maintainer appetite: the only LLM statements are "way out of scope" (#230) and "no paid third-party APIs" (#704). PR 12 is droppable; the discussion comes first.
- Template grammar across 16 locales: whole-sentence slots avoid most agreement problems, but piece gender/case in Slavic and Romance locales may need per-piece variants, and LLM-written templates inherit the same risk; snapshots guard key leaks, not awkwardness.
- Fact quality: SEE is static (no quiet defences, no zwischenzug); pins and skewers are geometric; a hanging piece is narrated only when the engine's line takes it, which hides true but unpunished weaknesses. The fixture set must grow from the games the eval harness exports before PR 7b/8 ship.
- Engine nondeterminism: `movetime 500`, shared hash and `reversed` order make facts vary run to run; `evalDrift` is the visible symptom and the harness counts it; fingerprints change and drafts regenerate; reproducibility is not promised.
- Validator lexicons for `relationWithoutClaim`/`magnitude` exist for four locales; elsewhere rules 1-5 carry the guarantee, which means an un-claimed relation word in, say, pl-PL is not caught. Number words ("two pawns") are not numbers; a template naming a true square outside the slots is dropped anyway; the eval harness measures both costs.
- Cost and latency: ≈ 2-6 requests per game at `claude-opus-5-5` with `effort: "low"`; users may pick `claude-haiku-5-5` or a local model. Whether `fallbacks: "default"` and `output_config.format` pass unchanged through user proxies is unknown; serde validation means the design does not depend on either.
- Secrets: a 0600 file is isolation, not a keychain; whether upstream would accept `keyring` later is unknown.
- Thresholds for ⊗ (gift ≥ 10, ≥ half given back), □ (margin ≥ 20 or mate in the second line), `OnlyMove` (second_win < 50), Sacrifice (`see ≤ −2`), cash-in (≥ 1 pawn, next PV move not a capture) and ⊕ (< 20 s or < 10 %) are constants to calibrate on real games before defaulting `extendedGlyphs` on.
- After `exportToPgn` save + reload, generated prose is an ordinary comment the editor can overwrite; accepted for interoperability and stated in the setting.
- Explain always searches (≈ 3 s); a `LineInput` IPC type could add reuse of live lines later.

## 16. Glossary and naming

| Name | Kind | Definition |
|---|---|---|
| `commentary` | Rust module, TS dir, node field, atoms, settings category, i18n prefix | the one user-facing noun for the feature |
| `MoveFacts`, `GameFacts` | Rust struct / TS types | everything counted about one mainline move, and about the game; schema-versioned; the only inputs to prose |
| `LineFacts`, `LineOrigin`, `LineCut`, `LineOutcome` | Rust types | an engine line: origin, where it was cut and why, what it does (mate from its eval, material from its PV, or an evaluation) |
| `ExactEval`, `EvalValue` | Rust types | exact-only evaluation with win chance; two score variants; built only from `search_once` output or a terminal outcome |
| `ParsedLine`, `Bound` | Rust types, private to chess.rs | one parsed UCI line with its bound flag; `search_once` keeps only `Exact` |
| "mate in N" | convention | N moves by the mating side counted from the position the line starts from; always bound to `LineOutcome::Mate.mate_in` or a criticality's `mate_in`, never to a raw eval |
| `Motif`, `Criticality` | Rust enums | board patterns as piece+square facts; only move vs clearly best, mate found/created/lost/delayed, miss, SEE-defined sacrifice |
| `see` | Rust fn | static exchange on one square, 1/3/3/5/9 scale, x-ray aware; the only source of material numbers besides `Board::material` |
| `BoardFacts`, `BoardDelta`, `AttackFact`, `PawnFacts`, `ByColor<T>` | Rust types | static board predicates, what the move changed, per-colour values |
| `OpeningFacts`, `OpeningMatch`, `get_opening_match` | Rust types / command | ECO, name and the last named position, Epd-keyed; "left known games" is `novelty` |
| `TimeFacts` | Rust type | remaining and spent seconds from PGN clocks, time pressure |
| `FactVocabulary`, `MoveRef`, `PieceRef`, `NumberRef`, `PositionRef`, `NumberUnit` | Rust types / TS bindings | the references a fact permits and a sentence's slots are drawn from, with the position each is evaluated at |
| `FactKind`, `CommentaryKind`, `Perspective` | Rust enums / TS bindings | the fact families; why a sentence exists; whose side "you" is (never `auto` past IPC) |
| `Slots`, `Claim`, `NarratedSentence` | Rust types / TS bindings | fixed-name slot values, typed relations, and the LLM's template-plus-slots-plus-claims unit |
| `search_once`, `explain_position`, `ExplainOptions`, `PositionExplanation` | Rust fn / command / types | one cancellable UCI search that drops bound lines; on-demand facts for one position on an isolated process |
| `JobRegistry`, `JobHandle`, `JobPhase`, `ProgressGuard`, `ProgressStage`, `ProgressStatus` | Rust types | one `watch` cancel token per job id with its receiver kept (`send_replace`), cancel-and-replace on insert; RAII terminal progress event; additive optional fields |
| `narrate_game`, `NarrationRequest`, `NarrationGame`, `NarrationItem`, `NarrationFact` | command / types | typed LLM verbalisation request: facts, vocabulary, drafts, `fen_before` |
| `NarratedMove`, `NarratedGame`, `NarratedTakeaway`, `NarrationChunk`, `Served` | types / event | typed results; per-chunk delivery with the model that answered |
| `NarrationRun`, `NarrationFailure`, `Redacted` | Rust types | outcome-as-data; typed failure; a string that redacts itself |
| `NarratorProvider`, `NarratorConfig`, `ProviderKind`, `WordBudget`, `test_narrator` | trait / types / command | provider seam; key-free config; word limits; one probe request |
| `SecretStore`, `set_/clear_/get_commentary_secret_status`, `SecretStatus` | trait / commands / type | key lifecycle; the key never returns to the webview |
| `classify.ts`: `deriveGlyphs`, `getExtendedAnnotation` | TS module | the one derivation of `getAnnotation` glyphs in the report path; ⊗, □ and ⊕ additive to it |
| `gameFacts.ts`: `gameFacts` | TS module | `GameFacts` from `analysis[]`, phases and the opening match, with `game:` ids |
| `salience.ts`: `enumerateFacts`, `vocabularyOf`, `selectFacts`, `FactSelection`, `SelectedFact`, `FactId` | TS module | fact ids in fixed order; allowed vocabulary; what matters by kind, weight, mandatory facts and detail level |
| `narrate.ts`, `segmentize.ts`: `narrateMove`, `narrateGame` | TS modules | outcome-keyed i18n templates to sentences with typed refs and recorded number spans; also renders LLM slot templates |
| `validate.ts`: `validateSentences`, `ValidationRule`, `ValidationResult` | TS module | the only module that drops sentences and mints `ValidatedNarration` |
| `ValidatedNarration`, `PgnSafeText` | TS brands | displayable text; exportable text; one minter each, enforced by `brands.test.ts` and `safeParse` on rehydration |
| `CommentarySentence`, `MoveCommentary`, `GameNarrative`, `Provenance` | TS types | sentence with refs and claims; per-node and per-game records; who wrote it (served model, usage, validation) |
| `commentaryFingerprint`, `takeMainlineSnapshot`, `samePosition`, `resolveSnapshotNode`, `resolvePerspective` | TS fns | staleness key; click-time mainline capture; chessops-canonical position equality; fen/san re-resolution; `auto` → side |
| `applyReport`, `applyCommentary`, `setGameNarrative`, `beginReport`, `endReport`, `appendComment` | store actions | one-`produce` report write with shared glyphs; validated upgrades; narrative; run identity; notes append |
| `treeRegistry`, `CommentaryListener` | TS module / component | live `TreeStore` per tab, per-tab parked chunk queue, `activeRuns`; listener mounted once in `RootLayout` |
| `CommentaryText`, `CommentaryCard`, `GameNarrative.tsx`, `ExplainButton`, `CommentarySettings` | components | plain-text renderer with chips; per-node card with perspective badge; narrative panel; explain entry above the engine list; settings section |
| `reportSettingsAtom`, `commentarySettingsAtom`, `commentaryFactsFamily`, `showCommentaryFamily`, `commentaryArrowsFamily` | atoms | report form (moved to atoms.ts, exported); zod-validated settings; non-persisted facts per tab; per-tab UI toggles |
| `EXPLAIN_POSITION`, `TOGGLE_COMMENTARY` | keybinds | `e` and `shift+c` |
| `Commentary.<Kind>.<Slot>.<Outcome>`, `Commentary.*`, `Settings.Commentary.*`, `Errors.Commentary.*`, `Commentary.Side.*`, `Commentary.Piece.*` | i18n families | outcome-keyed line templates, single-key fact templates, settings strings, failure strings, sentence-case sides, piece names |