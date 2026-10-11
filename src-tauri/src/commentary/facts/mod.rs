//! Facts: what was counted about a move, from positions and engine lines. Each extractor is a
//! pure function of positions; nothing here touches an engine, a database or app state.

pub mod criticality;
pub mod geometry;
pub mod motifs;
pub mod refutation;
pub mod structure;
pub mod threats;
pub mod time;
pub mod types;

#[cfg(test)]
mod fixtures;

use shakmaty::{san::SanPlus, uci::UciMove, CastlingMode, Chess, Color, Move, Position, Square};

pub use types::*;

use self::{
    criticality::{criticality, CriticalityInput},
    geometry::capture_see,
    refutation::line_facts,
    structure::{board_delta, board_facts},
};
use crate::{chess::BestMoves, opening::OpeningMatch};

/// Where the moving piece ends up. For castling that is the king's square; shakmaty encodes
/// castling as the king moving onto its rook.
pub fn destination(mv: &Move, mover: Color) -> Square {
    mv.castling_side()
        .map(|side| side.king_to(mover))
        .unwrap_or(mv.to())
}

/// Everything the extractors read for one mainline move.
pub struct FactsContext<'a> {
    /// Index of the position after the move.
    pub index: u32,
    pub before: &'a Chess,
    pub mv: &'a Move,
    /// The parent position's exact lines, best first.
    pub parent_lines: &'a [BestMoves],
    /// The exact lines of the position after the move (empty when it is game over).
    pub own_lines: &'a [BestMoves],
    /// The last named opening position of the game.
    pub opening: Option<&'a OpeningMatch>,
    /// The facts of the opponent's previous move.
    pub prev: Option<&'a EvalFacts>,
    pub time: Option<TimeFacts>,
    pub terminal: bool,
    pub novelty: bool,
}

fn uci_of(mv: &Move) -> String {
    UciMove::from_move(mv, CastlingMode::Standard).to_string()
}

fn played_move_facts(ctx: &FactsContext, after: &Chess, rank: Option<u16>) -> PlayedMoveFacts {
    let mover = ctx.before.turn();
    PlayedMoveFacts {
        san: SanPlus::from_move(ctx.before.clone(), ctx.mv).to_string(),
        uci: uci_of(ctx.mv),
        piece: ctx.mv.role().into(),
        from: ctx.mv.from().map(|sq| sq.to_string()).unwrap_or_default(),
        to: destination(ctx.mv, mover).to_string(),
        rank,
        is_capture: ctx.mv.is_capture(),
        captured: ctx.mv.capture().map(Into::into),
        gives_check: after.is_check(),
        gives_mate: after.is_checkmate(),
        is_castle: ctx.mv.is_castle(),
        promotion: ctx.mv.promotion().map(Into::into),
        see: capture_see(ctx.before, ctx.mv),
    }
}

/// The rank of the move played among the parent's lines (1 is the engine's first choice).
fn played_rank(ctx: &FactsContext) -> Option<u16> {
    ctx.parent_lines
        .iter()
        .position(|line| {
            line.uci_moves
                .first()
                .and_then(|u| u.parse::<UciMove>().ok())
                .and_then(|u| u.to_move(ctx.before).ok())
                .is_some_and(|m| &m == ctx.mv)
        })
        .map(|k| k as u16 + 1)
}

fn clamp_u8(n: usize) -> u8 {
    n.min(u8::MAX as usize) as u8
}

/// The facts of one mainline move. `None` when the position after the move has no exact line to
/// judge it by: no fact, no sentence.
pub fn extract_move_facts(ctx: &FactsContext) -> Option<MoveFacts> {
    let mover = ctx.before.turn();
    let mut after = ctx.before.clone();
    after.play_unchecked(ctx.mv);

    let after_eval = if ctx.terminal {
        ExactEval::terminal(&after)
    } else {
        ExactEval::from_line(ctx.own_lines.first()?, mover)
    };
    let before_eval = ctx
        .parent_lines
        .first()
        .map(|line| ExactEval::from_line(line, mover));
    let eval = EvalFacts {
        win_delta: before_eval.as_ref().map(|b| b.win - after_eval.win),
        before: before_eval,
        after: after_eval,
    };

    let mut lines: Vec<LineFacts> = ctx
        .parent_lines
        .iter()
        .enumerate()
        .map(|(k, line)| line_facts(clamp_u8(k), LineOrigin::Before, line, ctx.before, mover))
        .collect();
    let parent_count = lines.len();
    let refutation = match ctx.own_lines.first() {
        Some(line) if !ctx.terminal => {
            let id = clamp_u8(lines.len());
            lines.push(line_facts(id, LineOrigin::After, line, &after, mover));
            Some(id)
        }
        _ => None,
    };

    let rank = played_rank(ctx);
    let before_board = board_facts(ctx.before);
    let after_board = board_facts(&after);
    let delta = board_delta(ctx.before, &after, &before_board, &after_board);
    let criticality = criticality(&CriticalityInput {
        before: ctx.before,
        mv: ctx.mv,
        after: &after,
        rank,
        parent: &lines[..parent_count],
        refutation: refutation.map(|id| &lines[id as usize]),
        eval: &eval,
        prev: ctx.prev,
        terminal: ctx.terminal,
    });

    Some(MoveFacts {
        schema_version: FACTS_SCHEMA_VERSION,
        index: ctx.index,
        fen_before: shakmaty::fen::Fen::from_position(
            ctx.before.clone(),
            shakmaty::EnPassantMode::Legal,
        )
        .to_string(),
        fen_after: shakmaty::fen::Fen::from_position(after.clone(), shakmaty::EnPassantMode::Legal)
            .to_string(),
        mover: mover.into(),
        terminal: ctx.terminal,
        played: played_move_facts(ctx, &after, rank),
        eval,
        best: (rank != Some(1) && parent_count > 0).then_some(0),
        second: (parent_count >= 2).then_some(1),
        refutation,
        threat: None,
        lines,
        criticality,
        before: before_board,
        after: after_board,
        delta,
        opening: ctx
            .opening
            .filter(|opening| opening.index == ctx.index)
            .map(|opening| OpeningFacts {
                eco: opening.eco.clone(),
                name: opening.name.clone(),
                last_named_index: opening.index,
            }),
        novelty: ctx.novelty,
        time: ctx.time.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::fixtures::{line, pos, uci};
    use super::*;

    /// 1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6?? 4. Qxf7#: the facts of 3...Nf6 and 4. Qxf7#.
    const BEFORE_NF6: &str = "r1bqkbnr/pppp1ppp/2n5/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 3 3";

    #[test]
    fn a_blunder_has_its_refutation_and_the_better_move() {
        let before = pos(BEFORE_NF6);
        let mv = uci(&before, "g8f6");
        let mut after = before.clone();
        after.play_unchecked(&mv);
        let parent = [
            line(&before, "g7g6 h5f3 g8f6", EvalValue::Cp(-30)),
            line(&before, "d8e7 g1f3 g8f6", EvalValue::Cp(-10)),
        ];
        let own = [line(&after, "h5f7", EvalValue::Mate(1))];
        let facts = extract_move_facts(&FactsContext {
            index: 6,
            before: &before,
            mv: &mv,
            parent_lines: &parent,
            own_lines: &own,
            opening: None,
            prev: None,
            time: None,
            terminal: false,
            novelty: false,
        })
        .unwrap();

        assert_eq!(facts.mover, Side::Black);
        assert_eq!(facts.played.san, "Nf6");
        assert_eq!(facts.played.rank, None);
        assert_eq!(
            (facts.best, facts.second, facts.refutation),
            (Some(0), Some(1), Some(2))
        );
        let refutation = &facts.lines[2];
        assert_eq!(refutation.origin, LineOrigin::After);
        assert_eq!(
            refutation.outcome,
            LineOutcome::Mate {
                mate_in: 1,
                pv_reaches_mate: true,
            }
        );
        assert!(facts.eval.win_delta.unwrap() > 40.0);
        assert!(facts
            .criticality
            .contains(&Criticality::MateCreated { mate_in: 1 }));
    }

    #[test]
    fn the_mating_move_is_terminal() {
        let before = pos("r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4");
        let mv = uci(&before, "h5f7");
        let parent = [line(&before, "h5f7", EvalValue::Mate(1))];
        let facts = extract_move_facts(&FactsContext {
            index: 7,
            before: &before,
            mv: &mv,
            parent_lines: &parent,
            own_lines: &[],
            opening: None,
            prev: None,
            time: None,
            terminal: true,
            novelty: false,
        })
        .unwrap();
        assert!(facts.terminal && facts.played.gives_mate);
        assert_eq!(facts.played.san, "Qxf7#");
        assert_eq!(facts.eval.after.win, 100.0);
        assert_eq!(facts.refutation, None);
        assert_eq!(facts.played.rank, Some(1));
        assert_eq!(facts.best, None);
    }

    #[test]
    fn no_facts_without_an_exact_line_after_the_move() {
        let before = pos(BEFORE_NF6);
        let mv = uci(&before, "g8f6");
        let facts = extract_move_facts(&FactsContext {
            index: 6,
            before: &before,
            mv: &mv,
            parent_lines: &[],
            own_lines: &[],
            opening: None,
            prev: None,
            time: None,
            terminal: false,
            novelty: false,
        });
        assert!(facts.is_none());
    }

    #[test]
    fn castling_destination_is_the_king_square() {
        let before = pos("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        assert_eq!(
            destination(&uci(&before, "e1g1"), Color::White).to_string(),
            "g1"
        );
        assert_eq!(
            destination(&uci(&before, "e1c1"), Color::White).to_string(),
            "c1"
        );
    }
}
