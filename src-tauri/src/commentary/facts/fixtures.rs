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
