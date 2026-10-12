//! Helpers shared by the extractors' FEN fixture tests.

use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, FromSetup, Move, Square};

pub fn pos(fen: &str) -> Chess {
    let setup = fen.parse::<Fen>().unwrap().into_setup();
    Chess::from_setup(setup, CastlingMode::Standard).unwrap()
}

pub fn sq(name: &str) -> Square {
    name.parse().unwrap()
}

pub fn uci(pos: &Chess, mv: &str) -> Move {
    mv.parse::<UciMove>().unwrap().to_move(pos).unwrap()
}

/// The position after playing `mv` (UCI) in `fen`.
pub fn after(fen: &str, mv: &str) -> (Chess, Move, Chess) {
    let before = pos(fen);
    let m = uci(&before, mv);
    let mut after = before.clone();
    shakmaty::Position::play_unchecked(&mut after, &m);
    (before, m, after)
}

/// An exact engine line from `start` with the given White-POV evaluation.
pub fn line(start: &Chess, ucis: &str, value: super::types::EvalValue) -> crate::chess::BestMoves {
    use shakmaty::san::SanPlus;
    use vampirc_uci::uci::{Score, ScoreValue};

    let mut pos = start.clone();
    let mut san_moves = Vec::new();
    let uci_moves: Vec<String> = ucis.split_whitespace().map(String::from).collect();
    for u in &uci_moves {
        let m = uci(&pos, u);
        san_moves.push(SanPlus::from_move_and_play_unchecked(&mut pos, &m).to_string());
    }
    crate::chess::BestMoves {
        nodes: 0,
        depth: 20,
        score: Score {
            value: match value {
                super::types::EvalValue::Cp(cp) => ScoreValue::Cp(cp),
                super::types::EvalValue::Mate(n) => ScoreValue::Mate(n),
            },
            wdl: None,
            lower_bound: None,
            upper_bound: None,
        },
        uci_moves,
        san_moves,
        multipv: 1,
        nps: 0,
    }
}
