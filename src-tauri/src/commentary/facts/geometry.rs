//! Static board measurements: piece values, attackers and defenders, the static exchange on a
//! square, hanging and trapped pieces. Nothing here searches; every result is a function of one
//! position.

use shakmaty::{Bitboard, Board, Chess, Color, Move, Position, Role, Square};

use super::types::{Motif, PieceKind, Side};

/// Material value in pawns: P1 N3 B3 R5 Q9. The king has no material value.
pub fn value(role: Role) -> i32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}

/// The king's weight inside an exchange: more than any material, so a sequence never ends with
/// the king capturing onto a defended square.
const KING_IN_EXCHANGE: i32 = 100;

fn exchange_value(role: Role) -> i32 {
    match role {
        Role::King => KING_IN_EXCHANGE,
        role => value(role),
    }
}

const ROLES_BY_VALUE: [Role; 6] = [
    Role::Pawn,
    Role::Knight,
    Role::Bishop,
    Role::Rook,
    Role::Queen,
    Role::King,
];

fn least_valuable(board: &Board, attackers: Bitboard) -> Option<(Square, Role)> {
    ROLES_BY_VALUE.iter().find_map(|&role| {
        (attackers & board.by_role(role))
            .first()
            .map(|sq| (sq, role))
    })
}

/// Pieces of `color` attacking `square`, including those behind a friendly slider on the same
/// line (x-ray), so a battery counts in full.
pub fn attackers(board: &Board, square: Square, color: Color) -> Bitboard {
    let mut occupied = board.occupied();
    let mut found = Bitboard::EMPTY;
    loop {
        let direct = board.attacks_to(square, color, occupied) & occupied & !found;
        if direct.is_empty() {
            return found;
        }
        found |= direct;
        // Look through the sliders just found for pieces lined up behind them.
        occupied &= !(direct & board.sliders());
    }
}

/// The material `capturer` nets by capturing the piece on `square` with its least valuable
/// attacker, after which each side recaptures only while that pays. `None` when the square is
/// empty, `capturer` does not attack it, or the only capture is the king onto a defended square.
fn exchange(board: &Board, square: Square, capturer: Color) -> Option<i32> {
    let target = board.role_at(square)?;
    let mut occupied = board.occupied();
    let (mut from, mut role) = least_valuable(board, board.attacks_to(square, capturer, occupied))?;
    if role == Role::King && board.attacks_to(square, !capturer, occupied).any() {
        return None;
    }

    // gains[d] is the material balance for the side making capture d if the sequence stopped
    // after it; the backward pass lets each side stop when continuing loses.
    let mut gains = vec![exchange_value(target)];
    let mut side = capturer;
    loop {
        occupied = occupied.without(from);
        side = !side;
        let Some(next) = least_valuable(board, board.attacks_to(square, side, occupied) & occupied)
        else {
            break;
        };
        gains.push(exchange_value(role) - gains[gains.len() - 1]);
        (from, role) = next;
    }
    while gains.len() > 1 {
        let last = gains.pop().unwrap_or_default();
        let previous = gains.len() - 1;
        gains[previous] = -(-gains[previous]).max(last);
    }
    Some(gains[0])
}

/// Static exchange evaluation on `square`, in pawns, from `side`'s point of view.
///
/// - If the piece on `square` belongs to the opponent, `side` captures first (with its least
///   valuable attacker) and the result can be negative: a bishop taking a pawn defended by a pawn
///   is -2.
/// - If the piece belongs to `side`, the opponent captures only if that pays, so the result is 0
///   or negative: a queen left en prise to a knight is -9.
///
/// Pins and checks are ignored, as in any static exchange; callers that need a legal capture check
/// for one (see [`hanging`]).
pub fn see(board: &Board, square: Square, side: Color) -> i32 {
    match board.color_at(square) {
        None => 0,
        Some(owner) if owner == side => -exchange(board, square, !side).unwrap_or(0).max(0),
        Some(_) => exchange(board, square, side).unwrap_or(0),
    }
}

/// The net result, mover's point of view, of the exchange a capture starts: the captured piece
/// plus what the opponent then wins on the destination square. `None` for a quiet move.
pub fn capture_see(before: &Chess, mv: &Move) -> Option<i32> {
    let captured = mv.capture()?;
    let mut after = before.clone();
    after.play_unchecked(mv);
    Some(value(captured) + see(after.board(), mv.to(), before.turn()))
}

/// Whether `capturer` has a legal capture on `square` in `pos`, passing the move to it if needed.
fn can_capture(pos: &Chess, square: Square, capturer: Color) -> bool {
    let to_move = if pos.turn() == capturer {
        pos.clone()
    } else {
        match pos.clone().swap_turn() {
            Ok(swapped) => swapped,
            Err(_) => return false,
        }
    };
    to_move
        .legal_moves()
        .iter()
        .any(|m| m.is_capture() && m.to() == square)
}

/// Pieces of either side (kings excluded) that lose material to the best capture on their square
/// and that the opponent can legally capture, in square order.
pub fn hanging(pos: &Chess) -> Vec<Motif> {
    let board = pos.board();
    (board.occupied() & !board.kings())
        .into_iter()
        .filter_map(|square| {
            let piece = board.piece_at(square)?;
            let see = see(board, square, piece.color);
            (see < 0 && can_capture(pos, square, !piece.color)).then(|| Motif::Hanging {
                square: square.to_string(),
                piece: PieceKind::from(piece.role),
                side: Side::from(piece.color),
                see,
            })
        })
        .collect()
}

/// The piece on `square` if it is trapped: it stands where it loses material and every legal
/// move it has lands where it loses material too (counting what it captures on the way).
pub fn trapped(pos: &Chess, square: Square) -> Option<Motif> {
    let board = pos.board();
    let piece = board.piece_at(square)?;
    if matches!(piece.role, Role::Pawn | Role::King) || see(board, square, piece.color) >= 0 {
        return None;
    }
    let owner_to_move = if pos.turn() == piece.color {
        pos.clone()
    } else {
        pos.clone().swap_turn().ok()?
    };
    let escapes = owner_to_move.legal_moves().iter().any(|m| {
        m.from() == Some(square)
            && capture_see(&owner_to_move, m).unwrap_or_else(|| {
                let mut after = owner_to_move.clone();
                after.play_unchecked(m);
                see(after.board(), m.to(), piece.color)
            }) >= 0
    });
    (!escapes).then(|| Motif::TrappedPiece {
        square: square.to_string(),
        piece: PieceKind::from(piece.role),
    })
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{pos, sq, uci};
    use super::*;

    #[test]
    fn see_pawn_takes_undefended_bishop() {
        let p = pos("4k3/8/8/3b4/4P3/8/8/4K3 w - - 0 1");
        assert_eq!(see(p.board(), sq("d5"), Color::White), 3);
        assert_eq!(capture_see(&p, &uci(&p, "e4d5")), Some(3));
    }

    #[test]
    fn see_bishop_takes_defended_pawn() {
        let p = pos("4k3/8/2p5/3p4/8/8/6B1/4K3 w - - 0 1");
        assert_eq!(see(p.board(), sq("d5"), Color::White), -2);
        assert_eq!(capture_see(&p, &uci(&p, "g2d5")), Some(-2));
    }

    #[test]
    fn see_queen_left_en_prise_to_a_knight() {
        let p = pos("4k3/8/8/2n5/8/3Q4/8/4K3 b - - 0 1");
        assert_eq!(see(p.board(), sq("d3"), Color::White), -9);
        assert_eq!(see(p.board(), sq("d3"), Color::Black), 9);
    }

    #[test]
    fn see_declines_a_capture_that_loses() {
        // Black's rook would take a pawn defended by a pawn; the owner loses nothing.
        let p = pos("4k3/3r4/8/8/3P4/2P5/8/4K3 w - - 0 1");
        assert_eq!(see(p.board(), sq("d4"), Color::White), 0);
        assert_eq!(see(p.board(), sq("d4"), Color::Black), -4);
    }

    #[test]
    fn see_never_lets_the_king_capture_onto_a_defended_square() {
        let p = pos("8/8/8/3k4/3P4/4P3/8/4K3 b - - 0 1");
        assert_eq!(see(p.board(), sq("d4"), Color::Black), 0);
    }

    #[test]
    fn hanging_counts_an_x_ray_defender() {
        let defended = pos("3r2k1/3r4/8/8/3N4/8/3R4/3R2K1 w - - 0 1");
        assert_eq!(
            attackers(defended.board(), sq("d4"), Color::White),
            Bitboard::from(sq("d2")) | Bitboard::from(sq("d1"))
        );
        assert!(hanging(&defended).is_empty());

        let undefended = pos("3r2k1/3r4/8/8/3N4/8/3R4/6K1 w - - 0 1");
        assert_eq!(
            hanging(&undefended),
            vec![Motif::Hanging {
                square: "d4".into(),
                piece: PieceKind::Knight,
                side: Side::White,
                see: -3,
            }]
        );
    }

    #[test]
    fn a_pinned_attacker_does_not_make_a_piece_hanging() {
        // The only attacker of the bishop, the knight on d6, is pinned to its king.
        let p = pos("3k4/8/3n4/8/4B3/8/8/3RK3 b - - 0 1");
        assert_eq!(see(p.board(), sq("e4"), Color::White), -3);
        // The pinned knight itself is undefended against the rook and is reported; the bishop is
        // not.
        assert_eq!(
            hanging(&p),
            vec![Motif::Hanging {
                square: "d6".into(),
                piece: PieceKind::Knight,
                side: Side::Black,
                see: -3,
            }]
        );
    }

    #[test]
    fn trapped_bishop_on_a7() {
        let p = pos("r3k3/B1p5/1p6/8/8/8/8/4K3 w - - 0 1");
        assert_eq!(
            trapped(&p, sq("a7")),
            Some(Motif::TrappedPiece {
                square: "a7".into(),
                piece: PieceKind::Bishop,
            })
        );
        // Without the c7 pawn, Bxb6 is a safe way out.
        let free = pos("r3k3/B7/1p6/8/8/8/8/4K3 w - - 0 1");
        assert_eq!(trapped(&free, sq("a7")), None);
    }
}
