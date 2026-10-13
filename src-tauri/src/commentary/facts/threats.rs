//! What the side that just moved threatens: the position with the turn passed back, and the
//! moves that would mate from it.

use shakmaty::{fen::Fen, Chess, EnPassantMode, Position};

use super::types::Motif;

/// `pos` with the turn passed to the other side and no en passant square: the setup an engine is
/// asked about to find the mover's threat. `None` when the side to move is in check, since there
/// is no null move out of check.
pub fn null_move(pos: &Chess) -> Option<Chess> {
    pos.clone().swap_turn().ok()
}

/// [`null_move`] as a FEN for the engine.
#[allow(dead_code)] // Used by the threat search (slice 10).
pub fn null_move_fen(pos: &Chess) -> Option<String> {
    null_move(pos).map(|p| Fen::from_position(p, EnPassantMode::Legal).to_string())
}

/// Mates in one the side that just moved would have if the opponent passed.
pub fn mate_threats(pos: &Chess) -> Vec<Motif> {
    let Some(passed) = null_move(pos) else {
        return Vec::new();
    };
    passed
        .legal_moves()
        .iter()
        .filter(|m| {
            let mut after = passed.clone();
            after.play_unchecked(m);
            after.is_checkmate()
        })
        .filter_map(|m| {
            Some(Motif::MateThreat {
                from: m.from()?.to_string(),
                to: m.to().to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::pos;
    use super::*;

    #[test]
    fn scholars_mate_threat() {
        let p = pos("r1bqkbnr/pppp1ppp/2n5/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 3 3");
        assert_eq!(
            mate_threats(&p),
            vec![Motif::MateThreat {
                from: "h5".into(),
                to: "f7".into(),
            }]
        );
    }

    #[test]
    fn null_move_passes_the_turn_and_clears_en_passant() {
        let p = pos("rnbqkbnr/ppp1pppp/8/8/3pP3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 3");
        assert_eq!(
            null_move_fen(&p).as_deref(),
            Some("rnbqkbnr/ppp1pppp/8/8/3pP3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 3")
        );
    }

    #[test]
    fn no_null_move_out_of_check() {
        let p = pos("4k3/8/8/8/8/8/4q3/4K3 w - - 0 1");
        assert!(null_move(&p).is_none());
        assert!(mate_threats(&p).is_empty());
    }
}
