//! Engine lines as facts: the exact evaluation with a win chance, what the line does (mate,
//! material or only an evaluation), where it is worth cutting, and the motifs along the way.

use shakmaty::{uci::UciMove, Chess, Color, Move, Position};
use vampirc_uci::uci::ScoreValue;

use super::{
    destination,
    geometry::{trapped, value},
    motifs::{back_rank, discovered_attacks, fork, passed_pawn, pins_and_skewers, promotion},
    structure::material,
    threats::mate_threats,
    types::{EvalValue, ExactEval, LineCut, LineFacts, LineOrigin, LineOutcome, Motif},
};
use crate::chess::BestMoves;

/// Lines are never shown longer than this many plies; the full PV stays available.
pub const MAX_SHOWN_PLIES: usize = 6;
const MIN_SHOWN_PLIES: usize = 2;
/// Centipawn ceiling for win chances; a mate counts as this, as in the frontend's
/// `normalizeScore`.
const CP_CEILING: i32 = 1000;

/// The frontend's `getWinChance`: 0 to 100 for the side the centipawns favour.
pub fn win_chance(cp: i32) -> f32 {
    let cp = cp.clamp(-CP_CEILING, CP_CEILING) as f32;
    50.0 + 50.0 * (2.0 / (1.0 + (-0.003_682_08 * cp).exp()) - 1.0)
}

/// Centipawns from `side`'s point of view, with mates at the ceiling.
fn cp_for(value: EvalValue, side: Color) -> i32 {
    let sign = if side == Color::White { 1 } else { -1 };
    match value {
        EvalValue::Cp(cp) => (cp * sign).clamp(-CP_CEILING, CP_CEILING),
        EvalValue::Mate(n) => CP_CEILING * n.signum() * sign,
    }
}

/// The number of moves in which `side` mates, if the evaluation is a forced mate for it.
pub fn mate_for(value: EvalValue, side: Color) -> Option<u8> {
    match (value, side) {
        (EvalValue::Mate(n), Color::White) if n > 0 => Some(clamp_u8(n)),
        (EvalValue::Mate(n), Color::Black) if n < 0 => Some(clamp_u8(-n)),
        _ => None,
    }
}

fn clamp_u8(n: i32) -> u8 {
    n.clamp(0, u8::MAX as i32) as u8
}

pub fn eval_value(line: &BestMoves) -> EvalValue {
    match line.score.value {
        ScoreValue::Cp(cp) => EvalValue::Cp(cp),
        ScoreValue::Mate(n) => EvalValue::Mate(n),
    }
}

impl ExactEval {
    /// The evaluation of an engine line. Lines reaching the facts come from `search_once`, which
    /// drops bound lines, so the value is exact. `win` is from `mover`'s point of view.
    pub fn from_line(line: &BestMoves, mover: Color) -> ExactEval {
        let value = eval_value(line);
        ExactEval {
            value,
            depth: line.depth,
            win: win_chance(cp_for(value, mover)),
        }
    }

    /// The result of a game-over position reached by `mover`'s move: checkmate is `Mate(0)` with
    /// a win chance of 100, any other end is a draw at 50.
    pub fn terminal(after: &Chess) -> ExactEval {
        if after.is_checkmate() {
            ExactEval {
                value: EvalValue::Mate(0),
                depth: 0,
                win: 100.0,
            }
        } else {
            ExactEval {
                value: EvalValue::Cp(0),
                depth: 0,
                win: 50.0,
            }
        }
    }
}

/// The PV replayed from `start`, stopping at the first move that does not parse.
fn replay(line: &BestMoves, start: &Chess) -> (Vec<Move>, Vec<Chess>) {
    let mut moves = Vec::new();
    let mut positions = vec![start.clone()];
    for uci in &line.uci_moves {
        let current = &positions[positions.len() - 1];
        let Some(m) = uci
            .parse::<UciMove>()
            .ok()
            .and_then(|u| u.to_move(current).ok())
        else {
            break;
        };
        let mut next = current.clone();
        next.play_unchecked(&m);
        moves.push(m);
        positions.push(next);
    }
    (moves, positions)
}

/// Cut a line that has no earlier reason to stop.
fn length_cut(plies: usize) -> (usize, LineCut) {
    if plies > MAX_SHOWN_PLIES {
        (MAX_SHOWN_PLIES, LineCut::MaxPlies)
    } else {
        (plies, LineCut::EngineEnd)
    }
}

/// The first ply (within the shown range) after which the side that starts the line is at least a
/// pawn up and the next move is not a capture that could change that.
fn cash_in(moves: &[Move], positions: &[Chess]) -> Option<(i32, Option<shakmaty::Role>, usize)> {
    let side = positions[0].turn();
    let balance = |pos: &Chess| material(pos.board(), side) - material(pos.board(), !side);
    let start = balance(&positions[0]);
    (1..=moves.len().min(MAX_SHOWN_PLIES)).find_map(|ply| {
        let won = balance(&positions[ply]) - start;
        let settled = moves.get(ply).is_none_or(|next| !next.is_capture());
        (won >= 1 && settled).then(|| {
            let piece = moves[..ply]
                .iter()
                .step_by(2)
                .filter_map(Move::capture)
                .max_by_key(|&role| value(role));
            (won, piece, ply)
        })
    })
}

/// After a forcing start (captures and checks), the line is cut before its first quiet move.
fn forcing_cut(moves: &[Move], positions: &[Chess]) -> Option<usize> {
    let forcing = |ply: usize| moves[ply].is_capture() || positions[ply + 1].is_check();
    if moves.is_empty() || !forcing(0) {
        return None;
    }
    (1..moves.len()).find(|&ply| !forcing(ply))
}

/// Motifs created by the moves of the first `shown` plies, each found on the position the move
/// leads to, without duplicates.
fn line_motifs(moves: &[Move], positions: &[Chess], shown: usize, mates: bool) -> Vec<Motif> {
    let mut found: Vec<Motif> = Vec::new();
    let mut add = |motif: Motif| {
        if !found.contains(&motif) {
            found.push(motif);
        }
    };
    for ply in 1..=shown {
        let (mv, previous, current) = (&moves[ply - 1], &positions[ply - 1], &positions[ply]);
        let mover = previous.turn();
        let to = destination(mv, mover);
        fork(current, to).into_iter().for_each(&mut add);
        discovered_attacks(previous, mv, current)
            .into_iter()
            .for_each(&mut add);
        pins_and_skewers(current, mover)
            .into_iter()
            .filter(|m| matches!(m, Motif::Pin { attacker, .. } | Motif::Skewer { attacker, .. } if *attacker == to.to_string()))
            .for_each(&mut add);
        let attacked = current.board().attacks_from(to) & current.board().by_color(!mover);
        attacked
            .into_iter()
            .filter_map(|sq| trapped(current, sq))
            .for_each(&mut add);
        if mv.role() == shakmaty::Role::Pawn {
            passed_pawn(current.board(), to)
                .into_iter()
                .for_each(&mut add);
        }
        promotion(mv).into_iter().for_each(&mut add);
        mate_threats(current).into_iter().for_each(&mut add);
    }
    // A back-rank weakness explains a mate only when the mating move lands on that rank.
    if mates && shown > 0 {
        let last = &positions[shown];
        let mated = last.turn();
        let mating_move = &moves[shown - 1];
        if destination(mating_move, !mated).rank() == mated.relative_rank(shakmaty::Rank::First) {
            back_rank(last, mated).into_iter().for_each(&mut add);
        }
    }
    found
}

/// An engine line starting from `start`, as a fact. `mover` is the side whose move the facts
/// describe; win chances are from its point of view.
pub fn line_facts(
    id: u8,
    origin: LineOrigin,
    line: &BestMoves,
    start: &Chess,
    mover: Color,
) -> LineFacts {
    let (moves, positions) = replay(line, start);
    let plies = moves.len();
    let eval = ExactEval::from_line(line, mover);

    let (outcome, shown, cut_reason) = match eval.value {
        EvalValue::Mate(n) => {
            let mate_at = positions.iter().position(Chess::is_checkmate);
            let (shown, cut) = match mate_at {
                Some(ply) if ply <= MAX_SHOWN_PLIES => (ply, LineCut::Mate),
                _ => length_cut(plies),
            };
            let outcome = LineOutcome::Mate {
                mate_in: clamp_u8(n.abs()),
                pv_reaches_mate: mate_at.is_some(),
            };
            (outcome, shown, cut)
        }
        EvalValue::Cp(cp) => match cash_in(&moves, &positions) {
            Some((pawns, piece, ply)) => (
                LineOutcome::Material {
                    pawns,
                    piece: piece.map(Into::into),
                    at_ply: clamp_u8(ply as i32),
                },
                ply,
                LineCut::MaterialRealized,
            ),
            None => {
                let (shown, cut) = match forcing_cut(&moves, &positions) {
                    Some(ply) => (ply, LineCut::Quiet),
                    None => length_cut(plies),
                };
                (LineOutcome::Eval { cp }, shown, cut)
            }
        },
    };
    let shown = shown.max(MIN_SHOWN_PLIES).min(plies);
    let motifs = line_motifs(
        &moves,
        &positions,
        shown,
        matches!(outcome, LineOutcome::Mate { .. }),
    );

    LineFacts {
        id,
        origin,
        multipv: line.multipv,
        eval,
        pv_san: line.san_moves.clone(),
        pv_uci: line.uci_moves.clone(),
        shown_plies: clamp_u8(shown as i32),
        cut_reason,
        outcome,
        motifs,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{line, pos};
    use super::*;
    use crate::commentary::facts::types::PieceKind;

    #[test]
    fn win_chance_matches_the_frontend() {
        assert_eq!(win_chance(0), 50.0);
        assert!((win_chance(100) - 59.1).abs() < 0.05);
        assert!((win_chance(-500) - 13.69).abs() < 0.05);
        assert_eq!(win_chance(5000), win_chance(1000));
    }

    #[test]
    fn material_line_is_cut_where_the_piece_is_won() {
        let start = pos("4k3/8/8/3n4/8/8/8/R3K3 w - - 0 1");
        let l = line(&start, "a1a5 e8e7 a5d5 e7e6", EvalValue::Cp(900));
        let facts = line_facts(0, LineOrigin::After, &l, &start, Color::White);
        assert_eq!(
            facts.outcome,
            LineOutcome::Material {
                pawns: 3,
                piece: Some(PieceKind::Knight),
                at_ply: 3,
            }
        );
        assert_eq!(facts.cut_reason, LineCut::MaterialRealized);
        assert_eq!(facts.shown_plies, 3);
    }

    #[test]
    fn mate_outcome_comes_from_the_eval_not_the_pv_length() {
        // Mate(3) but the PV stops after four plies, before the mate.
        let start = pos("6k1/5ppp/8/8/8/8/5PPP/3RR1K1 w - - 0 1");
        let l = line(&start, "d1d7 g8f8 e1e7 h7h6", EvalValue::Mate(3));
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert_eq!(
            facts.outcome,
            LineOutcome::Mate {
                mate_in: 3,
                pv_reaches_mate: false,
            }
        );
        assert_eq!(facts.cut_reason, LineCut::EngineEnd);
        assert_eq!(facts.shown_plies, 4);
    }

    #[test]
    fn a_mating_pv_is_cut_at_the_mate() {
        let start = pos("6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1");
        let l = line(&start, "d1d8", EvalValue::Mate(1));
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert_eq!(facts.cut_reason, LineCut::Mate);
        // Never below two plies, but never more than the PV has.
        assert_eq!(facts.shown_plies, 1);
        assert!(facts
            .motifs
            .contains(&Motif::BackRank { king: "g8".into() }));
    }

    #[test]
    fn a_mate_off_the_back_rank_is_not_a_back_rank_motif() {
        let start = pos("r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4");
        let l = line(&start, "h5f7", EvalValue::Mate(1));
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert_eq!(facts.cut_reason, LineCut::Mate);
        assert!(!facts
            .motifs
            .iter()
            .any(|m| matches!(m, Motif::BackRank { .. })));
    }

    #[test]
    fn a_forcing_line_is_cut_before_its_first_quiet_move() {
        let start = pos("r3k3/8/8/8/8/8/8/R3K2R w - - 0 1");
        let l = line(&start, "a1a8 e8e7 h1h7 e7d6 e1d2", EvalValue::Cp(950));
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert!(matches!(facts.outcome, LineOutcome::Material { .. }));

        // An even trade: the capture and the recapture are shown, the quiet king move is not.
        let start = pos("3rk3/8/8/8/8/8/8/3RK3 w - - 0 1");
        let l = line(&start, "d1d8 e8d8 e1e2 d8e7", EvalValue::Cp(0));
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert_eq!(facts.outcome, LineOutcome::Eval { cp: 0 });
        assert_eq!((facts.shown_plies, facts.cut_reason), (2, LineCut::Quiet));
    }

    #[test]
    fn quiet_lines_are_cut_at_six_plies_and_never_below_two() {
        let start = pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let l = line(
            &start,
            "e2e4 e7e5 g1f3 b8c6 f1b5 a7a6 b5a4",
            EvalValue::Cp(30),
        );
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert_eq!(
            (facts.shown_plies, facts.cut_reason),
            (6, LineCut::MaxPlies)
        );

        let l = line(&start, "e2e4", EvalValue::Cp(30));
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert_eq!(
            (facts.shown_plies, facts.cut_reason),
            (1, LineCut::EngineEnd)
        );

        let l = line(&start, "e2e4 e7e5 g1f3", EvalValue::Cp(30));
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert_eq!(
            (facts.shown_plies, facts.cut_reason),
            (3, LineCut::EngineEnd)
        );
    }

    #[test]
    fn line_motifs_find_the_fork() {
        let start = pos("q3k3/8/8/1N6/8/8/8/4K3 w - - 0 1");
        let l = line(&start, "b5c7 e8d7 c7a8", EvalValue::Cp(900));
        let facts = line_facts(0, LineOrigin::Before, &l, &start, Color::White);
        assert!(facts
            .motifs
            .iter()
            .any(|m| matches!(m, Motif::Fork { from, is_check: true, .. } if from == "c7")));
    }
}
