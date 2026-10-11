//! What makes the move played stand out against the alternatives: only move or clearly best,
//! mate transitions, a missed punishment, a real sacrifice.

use shakmaty::{Chess, Color, Move, Position};

use super::{
    destination,
    geometry::{capture_see, see},
    refutation::mate_for,
    types::{Criticality, EvalFacts, LineFacts, PieceKind},
};

/// Win-chance gap between the first and second line above which the first is clearly best: the
/// strict gap `getAnnotation` uses for `!`.
const BEST_GAP: f32 = 10.0;
/// A second line below this win chance no longer holds.
const HOLDING_WIN: f32 = 50.0;
/// The opponent's previous move counts as a gift when it lost at least this much win chance.
const GIFT: f32 = 10.0;
/// A sacrifice gives up at least this much by exchange.
const SACRIFICE_SEE: i32 = -2;
/// A sacrifice holds when the mover keeps at least this win chance.
const HOLDS_WIN: f32 = 40.0;

pub struct CriticalityInput<'a> {
    pub before: &'a Chess,
    pub mv: &'a Move,
    pub after: &'a Chess,
    /// 1 when the engine's first choice was played.
    pub rank: Option<u16>,
    /// The parent position's lines, best first.
    pub parent: &'a [LineFacts],
    /// The opponent's best reply after the move.
    pub refutation: Option<&'a LineFacts>,
    pub eval: &'a EvalFacts,
    /// The opponent's previous move, for `Miss`.
    pub prev: Option<&'a EvalFacts>,
    pub terminal: bool,
}

/// All criticality facts of a move, in a fixed order.
pub fn criticality(input: &CriticalityInput) -> Vec<Criticality> {
    let mover = input.before.turn();
    let mut found = Vec::new();
    found.extend(best_move(input, mover));
    if !input.terminal {
        found.extend(mates(input, mover));
    }
    found.extend(miss(input));
    found.extend(sacrifices(input, mover));
    found
}

fn best_move(input: &CriticalityInput, mover: Color) -> Option<Criticality> {
    let [best, second, ..] = input.parent else {
        return None;
    };
    let margin = best.eval.win - second.eval.win;
    if input.rank != Some(1) || margin <= BEST_GAP {
        return None;
    }
    let second_win = second.eval.win;
    Some(
        if second_win < HOLDING_WIN || mate_for(second.eval.value, !mover).is_some() {
            Criticality::OnlyMove { margin, second_win }
        } else {
            Criticality::ClearlyBest { margin }
        },
    )
}

fn mates(input: &CriticalityInput, mover: Color) -> Vec<Criticality> {
    let mut found = Vec::new();
    let before = input.eval.before.as_ref().map(|e| e.value);
    let after = input.eval.after.value;
    let before_for = before.and_then(|v| mate_for(v, mover));
    let after_for = mate_for(after, mover);

    // Mate for the mover that was not there before the opponent's previous move.
    let earlier_for = input
        .prev
        .and_then(|p| p.before.as_ref())
        .and_then(|e| mate_for(e.value, mover));
    if let (Some(after_n), None) = (after_for, earlier_for) {
        let played_line = input.rank.and_then(|r| input.parent.get(r as usize - 1));
        let mate_in = played_line
            .and_then(|l| mate_for(l.eval.value, mover))
            .unwrap_or(after_n.saturating_add(1));
        let alternatives_also_mate = input.parent.iter().enumerate().any(|(k, l)| {
            Some(k + 1) != input.rank.map(usize::from) && mate_for(l.eval.value, mover).is_some()
        });
        found.push(Criticality::MateFound {
            mate_in,
            alternatives_also_mate,
        });
    }
    if let Some(mate_in) = mate_for(after, !mover) {
        if before.and_then(|v| mate_for(v, !mover)).is_none() {
            found.push(Criticality::MateCreated { mate_in });
        }
    }
    match (before_for, after_for) {
        (Some(was_mate_in), None) => found.push(Criticality::MateLost { was_mate_in }),
        (Some(from), Some(after_n)) if after_n.saturating_add(1) > from => {
            found.push(Criticality::MateDelayed {
                from,
                to: after_n.saturating_add(1),
            })
        }
        _ => {}
    }
    found
}

fn miss(input: &CriticalityInput) -> Option<Criticality> {
    let gift = input.prev?.win_delta?;
    let given_back = input.eval.win_delta?;
    (gift >= GIFT && given_back >= gift / 2.0).then_some(Criticality::Miss { gift, given_back })
}

/// Whether the opponent's reply line captures on `square` within its first two plies.
fn taken_in_line(refutation: Option<&LineFacts>, square: &str) -> bool {
    refutation.is_some_and(|line| {
        line.pv_uci
            .iter()
            .take(2)
            .any(|uci| uci.get(2..4) == Some(square))
    })
}

/// Pieces the move puts or leaves where the opponent wins at least two pawns' worth by exchange:
/// the moved piece first, then pieces the move newly left en prise.
fn sacrifices(input: &CriticalityInput, mover: Color) -> Vec<Criticality> {
    let after = input.after.board();
    let to = destination(input.mv, mover);
    let holds = input.eval.after.win >= HOLDS_WIN;
    let mut found = Vec::new();
    let mut add = |square: shakmaty::Square, see: i32| {
        if let Some(role) = after.role_at(square) {
            let square = square.to_string();
            found.push(Criticality::Sacrifice {
                piece: PieceKind::from(role),
                taken_in_line: taken_in_line(input.refutation, &square),
                square,
                see,
                holds,
            });
        }
    };

    let moved = capture_see(input.before, input.mv).unwrap_or_else(|| see(after, to, mover));
    if moved <= SACRIFICE_SEE {
        add(to, moved);
    }
    for square in (after.by_color(mover) & !after.kings()).without(to) {
        let now = see(after, square, mover);
        let then = if input.before.board().piece_at(square) == after.piece_at(square) {
            see(input.before.board(), square, mover)
        } else {
            0
        };
        if now <= SACRIFICE_SEE && then > SACRIFICE_SEE {
            add(square, now);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{after, line, pos};
    use super::super::refutation::line_facts;
    use super::super::types::{EvalValue, ExactEval, LineOrigin};
    use super::*;

    fn exact(value: EvalValue, mover: Color) -> ExactEval {
        let start = pos("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
        ExactEval::from_line(&line(&start, "e1e2", value), mover)
    }

    fn eval(before: EvalValue, after: EvalValue, mover: Color) -> EvalFacts {
        let before = exact(before, mover);
        let after = exact(after, mover);
        EvalFacts {
            win_delta: Some(before.win - after.win),
            before: Some(before),
            after,
        }
    }

    fn parent(start: &Chess, lines: &[(&str, EvalValue)]) -> Vec<LineFacts> {
        lines
            .iter()
            .enumerate()
            .map(|(k, (uci, value))| {
                line_facts(
                    k as u8,
                    LineOrigin::Before,
                    &line(start, uci, *value),
                    start,
                    start.turn(),
                )
            })
            .collect()
    }

    fn run(
        fen: &str,
        played: &str,
        rank: Option<u16>,
        lines: &[(&str, EvalValue)],
        eval: &EvalFacts,
        prev: Option<&EvalFacts>,
    ) -> Vec<Criticality> {
        let (before, mv, after_pos) = after(fen, played);
        let parent = parent(&before, lines);
        criticality(&CriticalityInput {
            before: &before,
            mv: &mv,
            after: &after_pos,
            rank,
            parent: &parent,
            refutation: None,
            eval,
            prev,
            terminal: after_pos.is_game_over(),
        })
    }

    const QUIET: &str = "4k3/8/8/8/8/8/R7/4K3 w - - 0 1";

    #[test]
    fn only_move_when_the_alternative_fails() {
        let e = eval(EvalValue::Mate(4), EvalValue::Mate(3), Color::White);
        let found = run(
            QUIET,
            "a2a7",
            Some(1),
            &[("a2a7", EvalValue::Mate(4)), ("a2a3", EvalValue::Cp(-250))],
            &e,
            None,
        );
        assert!(matches!(found[0], Criticality::OnlyMove { second_win, .. } if second_win < 50.0));
    }

    #[test]
    fn clearly_best_when_the_alternative_still_holds() {
        let e = eval(EvalValue::Mate(4), EvalValue::Mate(3), Color::White);
        let found = run(
            QUIET,
            "a2a7",
            Some(1),
            &[("a2a7", EvalValue::Mate(4)), ("a2a3", EvalValue::Cp(250))],
            &e,
            None,
        );
        assert!(matches!(found[0], Criticality::ClearlyBest { margin } if margin > 10.0));
    }

    #[test]
    fn no_best_move_fact_for_the_second_choice() {
        let e = eval(EvalValue::Cp(300), EvalValue::Cp(250), Color::White);
        let found = run(
            QUIET,
            "a2a3",
            Some(2),
            &[("a2a7", EvalValue::Cp(300)), ("a2a3", EvalValue::Cp(250))],
            &e,
            None,
        );
        assert!(found.is_empty());
    }

    #[test]
    fn mate_found_when_both_lines_mate() {
        let e = eval(EvalValue::Mate(3), EvalValue::Mate(2), Color::White);
        let prev = eval(EvalValue::Cp(50), EvalValue::Mate(3), Color::Black);
        let found = run(
            QUIET,
            "a2a7",
            Some(1),
            &[("a2a7", EvalValue::Mate(3)), ("a2a8", EvalValue::Mate(4))],
            &e,
            Some(&prev),
        );
        assert!(found.contains(&Criticality::MateFound {
            mate_in: 3,
            alternatives_also_mate: true,
        }));
    }

    #[test]
    fn mate_created_lost_and_delayed() {
        let created = eval(EvalValue::Cp(20), EvalValue::Mate(-2), Color::White);
        let found = run(QUIET, "a2a3", None, &[], &created, None);
        assert_eq!(found, vec![Criticality::MateCreated { mate_in: 2 }]);

        let lost = eval(EvalValue::Mate(2), EvalValue::Cp(400), Color::White);
        let found = run(QUIET, "a2a3", None, &[], &lost, None);
        assert_eq!(found, vec![Criticality::MateLost { was_mate_in: 2 }]);

        let delayed = eval(EvalValue::Mate(2), EvalValue::Mate(4), Color::White);
        // White was already mating before Black's move, so this is no new mate found.
        let prev = eval(EvalValue::Mate(3), EvalValue::Mate(2), Color::Black);
        let found = run(QUIET, "a2a3", None, &[], &delayed, Some(&prev));
        assert_eq!(found, vec![Criticality::MateDelayed { from: 2, to: 5 }]);
    }

    #[test]
    fn miss_when_half_the_gift_is_given_back() {
        let prev = eval(EvalValue::Cp(0), EvalValue::Cp(400), Color::Black);
        let e = eval(EvalValue::Cp(400), EvalValue::Cp(50), Color::White);
        let found = run(QUIET, "a2a3", None, &[], &e, Some(&prev));
        assert!(
            matches!(found[0], Criticality::Miss { gift, given_back } if gift > 10.0 && given_back >= gift / 2.0)
        );
    }

    #[test]
    fn sacrifice_is_defined_by_the_exchange() {
        // Bxf7+ gives a bishop for a pawn; the king takes back.
        let fen = "r1bqk1nr/pppp1ppp/2n5/2b1p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";
        let e = eval(EvalValue::Cp(20), EvalValue::Cp(-150), Color::White);
        let found = run(fen, "c4f7", None, &[], &e, None);
        assert!(found.contains(&Criticality::Sacrifice {
            piece: PieceKind::Bishop,
            square: "f7".into(),
            see: -2,
            holds: false,
            taken_in_line: false,
        }));
        // Taking a free pawn is no sacrifice, whatever the legacy material heuristic says.
        let fen = "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1";
        let e = eval(EvalValue::Cp(100), EvalValue::Cp(200), Color::White);
        assert!(run(fen, "e4d5", None, &[], &e, None).is_empty());
    }
}
