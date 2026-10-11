//! Static facts about a position and what a move changed: material, pawn structure, open files,
//! king safety, new attacks and newly hanging pieces.

use shakmaty::{attacks, Bitboard, Board, Chess, Color, File, Position, Square};

use super::{
    geometry::{attackers, hanging, value},
    types::{AttackFact, BoardDelta, BoardFacts, ByColor, Motif, PawnFacts, PieceKind},
};

fn by_color<T>(f: impl Fn(Color) -> T) -> ByColor<T> {
    ByColor {
        white: f(Color::White),
        black: f(Color::Black),
    }
}

/// Material of `color` in pawns, P1 N3 B3 R5 Q9.
pub fn material(board: &Board, color: Color) -> i32 {
    (board.by_color(color) & !board.kings())
        .into_iter()
        .filter_map(|sq| board.role_at(sq))
        .map(value)
        .sum()
}

fn names(squares: impl IntoIterator<Item = Square>) -> Vec<String> {
    squares.into_iter().map(|sq| sq.to_string()).collect()
}

pub fn pawn_facts(board: &Board, color: Color) -> PawnFacts {
    let own = board.pawns() & board.by_color(color);
    let enemy = board.pawns() & board.by_color(!color);
    let on_files = |pawns: Bitboard, file: File, spread: u32| {
        pawns
            .into_iter()
            .any(|sq| sq.file().distance(file) <= spread)
    };
    let ahead = |sq: Square, other: Square| match color {
        Color::White => other.rank() > sq.rank(),
        Color::Black => other.rank() < sq.rank(),
    };
    let isolated = own.into_iter().filter(|&sq| {
        !own.into_iter()
            .any(|other| other.file().distance(sq.file()) == 1)
    });
    let doubled = own.into_iter().filter(|&sq| {
        own.into_iter()
            .any(|other| other != sq && other.file() == sq.file())
    });
    let passed = own.into_iter().filter(|&sq| {
        !on_files(
            Bitboard::from_iter(enemy.into_iter().filter(|&other| ahead(sq, other))),
            sq.file(),
            1,
        )
    });
    PawnFacts {
        isolated: names(isolated),
        doubled: names(doubled),
        passed: names(passed),
    }
}

/// Files with no pawn of either colour.
pub fn open_files(board: &Board) -> Vec<char> {
    File::ALL
        .into_iter()
        .filter(|&file| (board.pawns() & Bitboard::from_file(file)).is_empty())
        .map(File::char)
        .collect()
}

/// The king's square and the squares around it.
fn king_zone(board: &Board, color: Color) -> Bitboard {
    board
        .king_of(color)
        .map(|king| attacks::king_attacks(king).with(king))
        .unwrap_or(Bitboard::EMPTY)
}

/// Pieces of `attacker` that attack the zone of the other side's king.
fn zone_attackers(board: &Board, attacker: Color) -> Bitboard {
    king_zone(board, !attacker)
        .into_iter()
        .fold(Bitboard::EMPTY, |found, sq| {
            found | board.attacks_to(sq, attacker, board.occupied())
        })
}

pub fn board_facts(pos: &Chess) -> BoardFacts {
    let board = pos.board();
    BoardFacts {
        material: by_color(|color| material(board, color)),
        hanging: hanging(pos),
        in_check: pos.is_check(),
        king_zone_attackers: by_color(|king| zone_attackers(board, !king).count() as u8),
        pawns: by_color(|color| pawn_facts(board, color)),
        open_files: open_files(board),
    }
}

/// Enemy pieces (kings excluded) attacked by `mover` in `after` from a square that did not attack
/// them in `before`.
fn newly_attacked(before: &Board, after: &Board, mover: Color) -> Vec<AttackFact> {
    let mut found = Vec::new();
    for target in after.by_color(!mover) & !after.kings() {
        let then = before.attacks_to(target, mover, before.occupied());
        let now = after.attacks_to(target, mover, after.occupied());
        for from in now & !then {
            let (Some(attacker), Some(role)) = (after.role_at(from), after.role_at(target)) else {
                continue;
            };
            found.push(AttackFact {
                from: from.to_string(),
                to: target.to_string(),
                attacker: PieceKind::from(attacker),
                target: PieceKind::from(role),
                defended: attackers(after, target, !mover).any(),
            });
        }
    }
    found
}

fn is_hanging_piece_of(motif: &Motif, side: Color) -> bool {
    matches!(motif, Motif::Hanging { side: s, .. } if *s == side.into())
}

pub fn board_delta(
    before: &Chess,
    after: &Chess,
    facts_before: &BoardFacts,
    facts_after: &BoardFacts,
) -> BoardDelta {
    let mover = before.turn();
    let balance = |pos: &Chess| material(pos.board(), mover) - material(pos.board(), !mover);
    let newly_hanging = facts_after
        .hanging
        .iter()
        .filter(|m| is_hanging_piece_of(m, mover) && !facts_before.hanging.contains(m))
        .cloned()
        .collect();
    let zone_before = zone_attackers(before.board(), mover).count() as i32;
    let zone_after = zone_attackers(after.board(), mover).count() as i32;
    BoardDelta {
        material_change: balance(after) - balance(before),
        newly_attacked: newly_attacked(before.board(), after.board(), mover),
        newly_hanging,
        king_zone_delta: (zone_after - zone_before).clamp(i8::MIN as i32, i8::MAX as i32) as i8,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{after, pos};
    use super::super::types::Side;
    use super::*;

    #[test]
    fn material_counts_pawn_units() {
        let p = pos("4k3/pp6/8/8/8/8/PPP5/RN2K1Q1 w - - 0 1");
        assert_eq!(material(p.board(), Color::White), 3 + 5 + 3 + 9);
        assert_eq!(material(p.board(), Color::Black), 2);
    }

    #[test]
    fn pawn_structure_and_open_files() {
        let p = pos("4k3/6p1/8/4P3/8/2P5/PPP5/4K3 w - - 0 1");
        let white = pawn_facts(p.board(), Color::White);
        assert_eq!(white.isolated, vec!["e5"]);
        assert_eq!(white.doubled, vec!["c2", "c3"]);
        assert_eq!(white.passed, vec!["a2", "b2", "c2", "c3", "e5"]);
        let black = pawn_facts(p.board(), Color::Black);
        assert_eq!(black.isolated, vec!["g7"]);
        assert_eq!(black.passed, vec!["g7"]);
        assert_eq!(open_files(p.board()), vec!['d', 'f', 'h']);
    }

    #[test]
    fn king_zone_attackers_count_distinct_pieces() {
        let p = pos("6k1/5ppp/8/6NQ/8/8/8/4K3 w - - 0 1");
        let facts = board_facts(&p);
        assert_eq!(facts.king_zone_attackers.black, 2);
        assert_eq!(facts.king_zone_attackers.white, 0);
    }

    #[test]
    fn delta_records_new_attacks_and_newly_hanging_pieces() {
        // Ng5 attacks the h7 pawn and steps onto a square the e7 bishop covers.
        let (before, _, after_pos) = after("4k3/4b2p/8/8/8/5N2/8/4K3 w - - 0 1", "f3g5");
        let delta = board_delta(
            &before,
            &after_pos,
            &board_facts(&before),
            &board_facts(&after_pos),
        );
        assert_eq!(delta.material_change, 0);
        assert_eq!(
            delta.newly_attacked,
            vec![AttackFact {
                from: "g5".into(),
                to: "h7".into(),
                attacker: PieceKind::Knight,
                target: PieceKind::Pawn,
                defended: false,
            }]
        );
        assert_eq!(
            delta.newly_hanging,
            vec![Motif::Hanging {
                square: "g5".into(),
                piece: PieceKind::Knight,
                side: Side::White,
                see: -3,
            }]
        );
    }
}
