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
    /// What the mover threatens: the exact best line of the position after the move with the turn
    /// passed back, when it was searched.
    pub threat_line: Option<&'a BestMoves>,
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

    let threat = match (ctx.threat_line, threats::null_move(&after)) {
        (Some(line), Some(passed)) if !ctx.terminal => {
            let id = clamp_u8(lines.len());
            lines.push(line_facts(id, LineOrigin::Threat, line, &passed, mover));
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
        threat,
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
            threat_line: None,
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
            threat_line: None,
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
    fn a_searched_threat_becomes_a_threat_line() {
        // 3. Bc4 threatens Qxf7#.
        let before = pos("r1bqkbnr/pppp1ppp/2n5/4p2Q/4P3/8/PPPP1PPP/RNB1KBNR w KQkq - 2 3");
        let mv = uci(&before, "f1c4");
        let mut after = before.clone();
        after.play_unchecked(&mv);
        let passed = threats::null_move(&after).unwrap();
        let parent = [line(&before, "f1c4 g7g6", EvalValue::Cp(40))];
        let own = [line(&after, "g7g6 h5f3", EvalValue::Cp(40))];
        let threat = line(&passed, "h5f7", EvalValue::Mate(1));
        let facts = extract_move_facts(&FactsContext {
            index: 5,
            before: &before,
            mv: &mv,
            parent_lines: &parent,
            own_lines: &own,
            opening: None,
            prev: None,
            time: None,
            terminal: false,
            novelty: false,
            threat_line: Some(&threat),
        })
        .unwrap();
        let id = facts.threat.expect("a threat line") as usize;
        assert_eq!(facts.lines[id].origin, LineOrigin::Threat);
        assert_eq!(facts.lines[id].pv_san, ["Qxf7#"]);
        assert_eq!(
            facts.lines[id].outcome,
            LineOutcome::Mate {
                mate_in: 1,
                pv_reaches_mate: true,
            }
        );
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
            threat_line: None,
        });
        assert!(facts.is_none());
    }

    /// Facts for the frontend's commentary tests, from scripted (not engine) lines. Regenerate with
    /// `cargo test write_frontend_fixtures -- --ignored` after a change to the fact types.
    #[test]
    #[ignore = "writes src/utils/tests/commentary/fixtures/moveFacts.json"]
    fn write_frontend_fixtures() {
        struct Case<'a> {
            name: &'a str,
            fen: &'a str,
            played: &'a str,
            parent: &'a [(&'a str, EvalValue)],
            own: Option<(&'a str, EvalValue)>,
            /// The mover's threat, searched from the position after the move with the turn passed.
            threat: Option<(&'a str, EvalValue)>,
            index: u32,
            prev: Option<(EvalValue, EvalValue, Color)>,
            novelty: bool,
            time: Option<TimeFacts>,
            opening: Option<(&'a str, &'a str)>,
        }
        let cases = [
            Case {
                name: "mateAllowed",
                fen: BEFORE_NF6,
                played: "g8f6",
                parent: &[
                    ("g7g6 h5f3 g8f6", EvalValue::Cp(-30)),
                    ("d8e7 g1f3 g8f6", EvalValue::Cp(-10)),
                ],
                own: Some(("h5f7", EvalValue::Mate(1))),
                index: 6,
                prev: None,
                novelty: false,
                time: None,
                opening: None,
                threat: None,
            },
            Case {
                name: "checkmate",
                fen: "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
                played: "h5f7",
                parent: &[
                    ("h5f7", EvalValue::Mate(1)),
                    ("d2d3 d7d5", EvalValue::Cp(-40)),
                ],
                own: None,
                index: 7,
                prev: None,
                novelty: false,
                time: None,
                opening: None,
                threat: None,
            },
            Case {
                name: "queenLost",
                fen: "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
                played: "d8g5",
                parent: &[
                    ("g8f6 b1c3 f8b4", EvalValue::Cp(-20)),
                    ("d7d6 d2d4 c8g4", EvalValue::Cp(-35)),
                ],
                own: Some(("f3g5 g8f6 d2d3", EvalValue::Cp(900))),
                index: 4,
                prev: None,
                novelty: false,
                time: Some(TimeFacts {
                    remaining_s: 12.0,
                    spent_s: Some(3.0),
                    under_pressure: true,
                }),
                opening: None,
                threat: None,
            },
            Case {
                name: "onlyMove",
                fen: "4k3/8/8/3n4/8/8/8/R3K3 w - - 0 1",
                played: "a1a5",
                parent: &[
                    ("a1a5 e8e7 a5d5", EvalValue::Cp(900)),
                    ("a1a7 d5c3 e1d2", EvalValue::Cp(-250)),
                ],
                own: Some(("e8e7 a5d5 e7e6", EvalValue::Cp(900))),
                index: 1,
                prev: None,
                novelty: false,
                time: None,
                opening: None,
                threat: None,
            },
            Case {
                name: "clearlyBest",
                fen: "4k3/8/8/3n4/8/8/8/R3K3 w - - 0 1",
                played: "a1a5",
                parent: &[
                    ("a1a5 e8e7 a5d5", EvalValue::Cp(900)),
                    ("a1a7 d5c3 e1d2", EvalValue::Cp(250)),
                ],
                own: Some(("e8e7 a5d5 e7e6", EvalValue::Cp(900))),
                index: 1,
                prev: None,
                novelty: false,
                time: None,
                opening: None,
                threat: None,
            },
            Case {
                name: "sacrifice",
                fen: "r1bqk1nr/pppp1ppp/2n5/2b1p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
                played: "c4f7",
                parent: &[
                    ("c4f7 e8f7 f3g5 f7e8", EvalValue::Cp(60)),
                    ("e1g1 g8f6 d2d3", EvalValue::Cp(20)),
                ],
                own: Some(("e8f7 f3g5 f7e8 d1f3", EvalValue::Cp(60))),
                index: 7,
                prev: None,
                novelty: false,
                time: None,
                opening: None,
                threat: None,
            },
            Case {
                name: "missedPunishment",
                fen: "4k3/8/8/3n4/8/8/8/R3K3 w - - 0 1",
                played: "e1d2",
                parent: &[
                    ("a1a5 e8e7 a5d5", EvalValue::Cp(900)),
                    ("a1a7 d5c3 e1d2", EvalValue::Cp(250)),
                ],
                own: Some(("e8e7 a1a7 e7e6", EvalValue::Cp(50))),
                index: 3,
                prev: Some((EvalValue::Cp(0), EvalValue::Cp(900), Color::Black)),
                novelty: false,
                time: None,
                opening: None,
                threat: None,
            },
            Case {
                name: "openingNovelty",
                fen: "rnbqkb1r/pppp1ppp/4pn2/8/2PP4/8/PP2PPPP/RNBQKBNR w KQkq - 0 3",
                played: "b1c3",
                parent: &[
                    ("b1c3 f8b4 e2e3", EvalValue::Cp(30)),
                    ("g1f3 d7d5 b1c3", EvalValue::Cp(25)),
                ],
                own: Some(("f8b4 e2e3 e8g8", EvalValue::Cp(30))),
                index: 5,
                prev: None,
                novelty: true,
                time: None,
                opening: Some(("E20", "Nimzo-Indian Defense")),
                threat: None,
            },
            Case {
                name: "threat",
                fen: "r1bqkbnr/pppp1ppp/2n5/4p2Q/4P3/8/PPPP1PPP/RNB1KBNR w KQkq - 2 3",
                played: "f1c4",
                parent: &[
                    ("f1c4 g7g6 h5f3", EvalValue::Cp(40)),
                    ("f1b5 g8f6 h5e2", EvalValue::Cp(5)),
                ],
                own: Some(("g7g6 h5f3 g8f6", EvalValue::Cp(40))),
                index: 5,
                prev: None,
                novelty: false,
                time: None,
                opening: None,
                threat: Some(("h5f7", EvalValue::Mate(1))),
            },
        ];

        let mut out = serde_json::Map::new();
        for case in cases {
            let before = pos(case.fen);
            let mv = uci(&before, case.played);
            let mut after = before.clone();
            after.play_unchecked(&mv);
            let parent: Vec<BestMoves> = case
                .parent
                .iter()
                .enumerate()
                .map(|(k, (u, v))| {
                    let mut l = line(&before, u, *v);
                    l.multipv = k as u16 + 1;
                    l
                })
                .collect();
            let own: Vec<BestMoves> = case.own.iter().map(|(u, v)| line(&after, u, *v)).collect();
            let threat = case
                .threat
                .map(|(u, v)| line(&threats::null_move(&after).unwrap(), u, v));
            let prev = case.prev.map(|(b, a, mover)| {
                let ev = |v: EvalValue| {
                    ExactEval::from_line(
                        &line(&pos("4k3/8/8/8/8/8/8/4K3 w - - 0 1"), "e1e2", v),
                        mover,
                    )
                };
                let (before, after) = (ev(b), ev(a));
                EvalFacts {
                    win_delta: Some(before.win - after.win),
                    before: Some(before),
                    after,
                }
            });
            let opening = case.opening.map(|(eco, name)| OpeningMatch {
                eco: eco.into(),
                name: name.into(),
                pgn: None,
                index: case.index,
            });
            let facts = extract_move_facts(&FactsContext {
                index: case.index,
                before: &before,
                mv: &mv,
                parent_lines: &parent,
                own_lines: &own,
                opening: opening.as_ref(),
                prev: prev.as_ref(),
                time: case.time,
                terminal: after.is_game_over(),
                novelty: case.novelty,
                threat_line: threat.as_ref(),
            })
            .unwrap();
            out.insert(case.name.into(), serde_json::to_value(facts).unwrap());
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/utils/tests/commentary/fixtures");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("moveFacts.json"),
            serde_json::to_string_pretty(&out).unwrap() + "\n",
        )
        .unwrap();

        // A whole game: 1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6?? 4. Qxf7#, with two scripted lines per
        // position, as the report's post-pass computes it.
        let moves = ["e2e4", "e7e5", "d1h5", "b8c6", "f1c4", "g8f6", "h5f7"];
        let lines: [&[(&str, EvalValue)]; 8] = [
            &[
                ("e2e4 e7e5", EvalValue::Cp(30)),
                ("d2d4 d7d5", EvalValue::Cp(25)),
            ],
            &[
                ("e7e5 g1f3", EvalValue::Cp(30)),
                ("c7c5 g1f3", EvalValue::Cp(35)),
            ],
            &[
                ("g1f3 b8c6", EvalValue::Cp(35)),
                ("d1h5 b8c6", EvalValue::Cp(0)),
            ],
            &[
                ("b8c6 f1c4", EvalValue::Cp(0)),
                ("g7g6 h5f3", EvalValue::Cp(-10)),
            ],
            &[
                ("f1c4 g7g6", EvalValue::Cp(0)),
                ("h5f3 g8f6", EvalValue::Cp(-10)),
            ],
            &[
                ("g7g6 h5f3", EvalValue::Cp(-30)),
                ("d8e7 g1f3", EvalValue::Cp(-10)),
            ],
            &[("h5f7", EvalValue::Mate(1))],
            &[],
        ];
        let mut positions = vec![Chess::default()];
        let mut played = Vec::new();
        for m in moves {
            let before = positions.last().unwrap().clone();
            let mv = uci(&before, m);
            let mut after = before.clone();
            after.play_unchecked(&mv);
            played.push(mv);
            positions.push(after);
        }
        let best: Vec<Vec<BestMoves>> = lines
            .iter()
            .zip(&positions)
            .map(|(ls, p)| {
                ls.iter()
                    .enumerate()
                    .map(|(k, (u, v))| {
                        let mut l = line(p, u, *v);
                        l.multipv = k as u16 + 1;
                        l
                    })
                    .collect()
            })
            .collect();
        let fens: Vec<String> = positions
            .iter()
            .map(|p| {
                shakmaty::fen::Fen::from_position(p.clone(), shakmaty::EnPassantMode::Legal)
                    .to_string()
            })
            .collect();
        let opening = crate::opening::get_opening_match(fens.clone()).unwrap();
        let mut game_moves = vec![serde_json::Value::Null];
        let mut prev: Option<EvalFacts> = None;
        for i in 1..positions.len() {
            let facts = extract_move_facts(&FactsContext {
                index: i as u32,
                before: &positions[i - 1],
                mv: &played[i - 1],
                parent_lines: &best[i - 1],
                own_lines: &best[i],
                opening: opening.as_ref(),
                prev: prev.as_ref(),
                time: None,
                terminal: positions[i].is_game_over(),
                novelty: false,
                threat_line: None,
            });
            prev = facts.as_ref().map(|f| f.eval.clone());
            game_moves.push(serde_json::to_value(facts).unwrap());
        }
        let game = serde_json::json!({
            "scholarsMate": { "result": "1-0", "fens": fens, "moves": game_moves }
        });
        std::fs::write(
            dir.join("gameFacts.json"),
            serde_json::to_string_pretty(&game).unwrap() + "\n",
        )
        .unwrap();
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
