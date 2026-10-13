//! The fact set: everything counted about one mainline move. Commentary text is a function of these
//! values and nothing else. Squares are algebraic strings and pieces use [`PieceKind`] because
//! shakmaty's types implement neither `Serialize` nor `specta::Type`.

use serde::{Deserialize, Serialize};
use shakmaty::{Color, Role};
use specta::Type;

/// Bumped whenever a fact's shape or meaning changes, so stored commentary built from older facts
/// can be recognized and regenerated.
pub const FACTS_SCHEMA_VERSION: u16 = 1;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    White,
    Black,
}

impl From<Color> for Side {
    fn from(color: Color) -> Self {
        match color {
            Color::White => Side::White,
            Color::Black => Side::Black,
        }
    }
}

/// Matches chessops' `Role` strings.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl From<Role> for PieceKind {
    fn from(role: Role) -> Self {
        match role {
            Role::Pawn => PieceKind::Pawn,
            Role::Knight => PieceKind::Knight,
            Role::Bishop => PieceKind::Bishop,
            Role::Rook => PieceKind::Rook,
            Role::Queen => PieceKind::Queen,
            Role::King => PieceKind::King,
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct ByColor<T> {
    pub white: T,
    pub black: T,
}

/// An evaluation from White's point of view. `Mate(n)`: n > 0 means White mates in n moves, n < 0
/// means Black mates in |n| (UCI moves, not plies).
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum EvalValue {
    Cp(i32),
    Mate(i32),
}

/// An exact evaluation (never a search bound) with the mover's win chance, 0 to 100.
#[derive(Serialize, Debug, Clone, PartialEq, Type)]
pub struct ExactEval {
    pub value: EvalValue,
    pub depth: u32,
    pub win: f32,
}

#[derive(Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct EvalFacts {
    /// The parent position's best line.
    pub before: Option<ExactEval>,
    /// This position's best line, or the game result for a game-over position.
    pub after: ExactEval,
    /// `before.win - after.win` from the mover's point of view.
    pub win_delta: Option<f32>,
}

/// Why a line was cut where it was.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum LineCut {
    Mate,
    MaterialRealized,
    Quiet,
    MaxPlies,
    EngineEnd,
}

/// What a line does: mate (from its evaluation, never from its length), material won along its
/// PV, or only an evaluation.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LineOutcome {
    #[serde(rename_all = "camelCase")]
    Mate { mate_in: u8, pv_reaches_mate: bool },
    /// Pawn units won by the side that plays the line's first move.
    #[serde(rename_all = "camelCase")]
    Material {
        pawns: i32,
        piece: Option<PieceKind>,
        at_ply: u8,
    },
    /// White's point of view.
    Eval { cp: i32 },
}

/// Which position a line starts from.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum LineOrigin {
    /// The parent position (the alternatives to the move played).
    Before,
    /// The position after the move (the opponent's best reply).
    After,
    /// The position after the move with the turn passed back (what the mover threatens).
    Threat,
}

#[derive(Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct LineFacts {
    /// Index into `MoveFacts.lines`.
    pub id: u8,
    pub origin: LineOrigin,
    pub multipv: u16,
    pub eval: ExactEval,
    pub pv_san: Vec<String>,
    pub pv_uci: Vec<String>,
    /// Plies worth showing: the line is cut where its consequence is realized, 2 to 6 plies.
    pub shown_plies: u8,
    pub cut_reason: LineCut,
    pub outcome: LineOutcome,
    /// Motifs found on the positions after each shown ply.
    pub motifs: Vec<Motif>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct Target {
    pub square: String,
    pub piece: PieceKind,
}

/// A board pattern, described by pieces and squares.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Motif {
    /// One piece attacks two or more targets: the king, more valuable pieces, or hanging pieces.
    #[serde(rename_all = "camelCase")]
    Fork {
        from: String,
        attacker: PieceKind,
        targets: Vec<Target>,
        is_check: bool,
    },
    /// `pinned` cannot move off the line without exposing `behind`; `absolute` when `behind` is
    /// the king.
    Pin {
        attacker: String,
        pinned: String,
        behind: String,
        absolute: bool,
    },
    /// The more valuable `front` is attacked and `behind` falls when it moves.
    Skewer {
        attacker: String,
        front: String,
        behind: String,
    },
    /// A move from `moved_from` opened the line of the slider on `revealed` to `target`.
    #[serde(rename_all = "camelCase")]
    DiscoveredAttack {
        moved_from: String,
        revealed: String,
        target: String,
        is_check: bool,
    },
    /// A piece that loses material to the best capture on its square (`see` < 0, owner's point of
    /// view) and that can legally be captured.
    Hanging {
        square: String,
        piece: PieceKind,
        side: Side,
        see: i32,
    },
    /// A piece in a bad spot with no move to a square that is not.
    TrappedPiece {
        square: String,
        piece: PieceKind,
    },
    /// The king is on its back rank with no escape square, facing a rook or queen.
    BackRank {
        king: String,
    },
    /// A move from `from` to `to` would mate if the opponent passed.
    MateThreat {
        from: String,
        to: String,
    },
    PassedPawn {
        square: String,
    },
    Promotion {
        square: String,
        to: PieceKind,
    },
}

/// What makes the move played stand out among the alternatives. Zero or more per move, in a fixed
/// extractor order.
#[derive(Serialize, Debug, Clone, PartialEq, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Criticality {
    /// The engine's first choice was played, the second choice is more than 10 points of win
    /// chance worse, and it no longer holds (`second_win` < 50 or it gets mated).
    #[serde(rename_all = "camelCase")]
    OnlyMove { margin: f32, second_win: f32 },
    /// As `OnlyMove`, but the second choice still holds.
    ClearlyBest { margin: f32 },
    /// The move starts a forced mate the position before did not have for the mover.
    #[serde(rename_all = "camelCase")]
    MateFound {
        mate_in: u8,
        alternatives_also_mate: bool,
    },
    /// The move allows a forced mate against the mover.
    #[serde(rename_all = "camelCase")]
    MateCreated { mate_in: u8 },
    /// The mover had a forced mate and the move lets it go.
    #[serde(rename_all = "camelCase")]
    MateLost { was_mate_in: u8 },
    /// The mover keeps a forced mate but a longer one.
    MateDelayed { from: u8, to: u8 },
    /// The opponent's previous move gave away `gift` points of win chance and this move gave at
    /// least half of it back.
    #[serde(rename_all = "camelCase")]
    Miss { gift: f32, given_back: f32 },
    /// The move leaves a piece where the opponent wins at least two pawns' worth by exchange.
    #[serde(rename_all = "camelCase")]
    Sacrifice {
        piece: PieceKind,
        square: String,
        see: i32,
        holds: bool,
        taken_in_line: bool,
    },
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayedMoveFacts {
    pub san: String,
    pub uci: String,
    pub piece: PieceKind,
    pub from: String,
    pub to: String,
    /// 1 is the engine's first choice; `None` if the move is not among the parent's lines.
    pub rank: Option<u16>,
    pub is_capture: bool,
    pub captured: Option<PieceKind>,
    pub gives_check: bool,
    pub gives_mate: bool,
    pub is_castle: bool,
    pub promotion: Option<PieceKind>,
    /// Net result of the exchange the capture starts, mover's point of view; `None` for a quiet
    /// move.
    pub see: Option<i32>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct PawnFacts {
    pub isolated: Vec<String>,
    pub doubled: Vec<String>,
    pub passed: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoardFacts {
    /// Pawn units, P1 N3 B3 R5 Q9.
    pub material: ByColor<i32>,
    /// `Motif::Hanging` only.
    pub hanging: Vec<Motif>,
    pub in_check: bool,
    /// Enemy pieces attacking each king's zone.
    pub king_zone_attackers: ByColor<u8>,
    pub pawns: ByColor<PawnFacts>,
    /// Files with no pawn of either colour, as letters.
    pub open_files: Vec<char>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct AttackFact {
    pub from: String,
    pub to: String,
    pub attacker: PieceKind,
    pub target: PieceKind,
    pub defended: bool,
}

/// What the move changed on the board.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoardDelta {
    /// Mover's point of view, pawn units.
    pub material_change: i32,
    /// Enemy pieces the mover attacks after the move and did not before.
    pub newly_attacked: Vec<AttackFact>,
    /// The mover's own pieces that are hanging after the move and were not before.
    pub newly_hanging: Vec<Motif>,
    /// Change in the number of the mover's pieces attacking the enemy king's zone.
    pub king_zone_delta: i8,
}

/// The last position of the game found in the opening table.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpeningFacts {
    pub eco: String,
    pub name: String,
    pub last_named_index: u32,
}

#[derive(Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct TimeFacts {
    pub remaining_s: f32,
    pub spent_s: Option<f32>,
    pub under_pressure: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveFacts {
    pub schema_version: u16,
    /// Index into the report's positions (the position after the move). The frontend joins facts
    /// to tree nodes by `(fen_before, played.san)`, never by this index.
    pub index: u32,
    pub fen_before: String,
    pub fen_after: String,
    pub mover: Side,
    /// `fen_after` is checkmate or stalemate: no line was searched from it.
    pub terminal: bool,
    pub played: PlayedMoveFacts,
    pub eval: EvalFacts,
    /// The parent's lines, the best line after the move, and the threat line when searched.
    pub lines: Vec<LineFacts>,
    /// Index in `lines` of the parent's best line when it differs from the move played.
    pub best: Option<u8>,
    /// Index in `lines` of the parent's second line.
    pub second: Option<u8>,
    /// Index in `lines` of the opponent's best reply.
    pub refutation: Option<u8>,
    /// Index in `lines` of the threat line.
    pub threat: Option<u8>,
    pub criticality: Vec<Criticality>,
    pub before: BoardFacts,
    pub after: BoardFacts,
    pub delta: BoardDelta,
    pub opening: Option<OpeningFacts>,
    pub novelty: bool,
    pub time: Option<TimeFacts>,
}

/// The position a board reference is evaluated in: before the move, after it, or after `ply`
/// plies of line `id`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum PositionRef {
    Before,
    After,
    Line { id: u8, ply: u8 },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum NumberUnit {
    Pawns,
    MateIn,
    WinPercent,
    Move,
    /// Rendered through the frontend's `formatScore`.
    Eval,
    Seconds,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct MoveRef {
    pub san: String,
    pub side: Side,
    pub at: PositionRef,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct PieceRef {
    pub piece: PieceKind,
    pub square: String,
    pub at: PositionRef,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct NumberRef {
    pub value: f32,
    pub unit: NumberUnit,
}

/// The board references a fact permits. A sentence citing the fact may name only these; the
/// frontend computes it from the fact and the validator checks sentences against it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, Type)]
pub struct FactVocabulary {
    pub moves: Vec<MoveRef>,
    pub pieces: Vec<PieceRef>,
    pub squares: Vec<String>,
    pub numbers: Vec<NumberRef>,
    pub sides: Vec<Side>,
}

/// The families of facts a sentence can cite.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum FactKind {
    Line,
    Hanging,
    Attack,
    Motif,
    Criticality,
    EvalDelta,
    Opening,
    Novelty,
    Time,
    Game,
}

/// Why a move's commentary exists.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum CommentaryKind {
    Mistake,
    EvalDrift,
    Miss,
    BetterMove,
    OnlyMove,
    ClearlyBest,
    Sacrifice,
    MateFound,
    Threat,
    Opening,
    Novelty,
    Phase,
    Mate,
    Positional,
}

/// Whose side "you" is in commentary; `Both` is third person.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum Perspective {
    White,
    Black,
    Both,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn wire_format_is_camel_case_like_the_bindings() {
        let only = Criticality::OnlyMove {
            margin: 12.5,
            second_win: 30.0,
        };
        assert_eq!(
            serde_json::to_value(only).unwrap(),
            json!({ "kind": "onlyMove", "margin": 12.5, "secondWin": 30.0 })
        );
        let discovered = Motif::DiscoveredAttack {
            moved_from: "e2".into(),
            revealed: "e1".into(),
            target: "e8".into(),
            is_check: true,
        };
        assert_eq!(
            serde_json::to_value(discovered).unwrap(),
            json!({ "kind": "discoveredAttack", "movedFrom": "e2", "revealed": "e1", "target": "e8", "isCheck": true })
        );
        let outcome = LineOutcome::Material {
            pawns: 3,
            piece: Some(PieceKind::Knight),
            at_ply: 3,
        };
        assert_eq!(
            serde_json::to_value(outcome).unwrap(),
            json!({ "kind": "material", "pawns": 3, "piece": "knight", "atPly": 3 })
        );
        assert_eq!(
            serde_json::to_value(EvalValue::Mate(-2)).unwrap(),
            json!({ "type": "mate", "value": -2 })
        );
    }
}
