//! Tactical and structural patterns, each reported as pieces on squares. Detection is geometric
//! and static: a pin is a line, not a judgement that the pin matters.

use shakmaty::{attacks, Bitboard, Board, Chess, Color, Move, Position, Role, Square};

use super::{
    geometry::{see, value},
    types::{Motif, PieceKind, Target},
};

/// Whether the piece of `owner` on `square` is worth attacking for a piece of `attacker` value:
/// the king, anything more valuable, or a piece that loses material on its square.
fn is_target(board: &Board, square: Square, attacker: Role) -> bool {
    match board.piece_at(square) {
        Some(piece) if piece.role == Role::King => true,
        Some(piece) => {
            (attacker != Role::King && value(piece.role) > value(attacker))
                || see(board, square, piece.color) < 0
        }
        None => false,
    }
}

/// A fork by the piece standing on `square` (normally the piece that just moved): two or more
/// enemy targets attacked at once. The king always counts as a target.
pub fn fork(pos: &Chess, square: Square) -> Option<Motif> {
    let board = pos.board();
    let piece = board.piece_at(square)?;
    let attacked = attacks::attacks(square, piece, board.occupied()) & board.by_color(!piece.color);
    let targets: Vec<Target> = attacked
        .into_iter()
        .filter(|&target| is_target(board, target, piece.role))
        .filter_map(|target| {
            Some(Target {
                square: target.to_string(),
                piece: PieceKind::from(board.role_at(target)?),
            })
        })
        .collect();
    (targets.len() >= 2).then(|| Motif::Fork {
        from: square.to_string(),
        attacker: PieceKind::from(piece.role),
        targets,
        is_check: pos.is_check(),
    })
}

/// Pins and skewers by the sliders of `side`: an enemy piece is the only piece between a slider
/// and another enemy piece on the slider's line. A cheaper piece in front is pinned (absolutely
/// when the king is behind); a more valuable piece or the king in front is skewered.
pub fn pins_and_skewers(pos: &Chess, side: Color) -> Vec<Motif> {
    let board = pos.board();
    let mut found = Vec::new();
    for from in board.by_color(side) & board.sliders() {
        let Some(piece) = board.piece_at(from) else {
            continue;
        };
        let lines = attacks::attacks(from, piece, Bitboard::EMPTY);
        for behind in board.by_color(!side) & lines {
            let blockers = attacks::between(from, behind) & board.occupied();
            let Some(front) = blockers.first() else {
                continue;
            };
            if blockers.count() != 1 || board.color_at(front) != Some(!side) {
                continue;
            }
            let (Some(front_role), Some(behind_role)) =
                (board.role_at(front), board.role_at(behind))
            else {
                continue;
            };
            if behind_role == Role::King {
                found.push(Motif::Pin {
                    attacker: from.to_string(),
                    pinned: front.to_string(),
                    behind: behind.to_string(),
                    absolute: true,
                });
            } else if front_role == Role::King || value(front_role) > value(behind_role) {
                found.push(Motif::Skewer {
                    attacker: from.to_string(),
                    front: front.to_string(),
                    behind: behind.to_string(),
                });
            } else if value(front_role) < value(behind_role) {
                found.push(Motif::Pin {
                    attacker: from.to_string(),
                    pinned: front.to_string(),
                    behind: behind.to_string(),
                    absolute: false,
                });
            }
        }
    }
    found
}

/// Attacks opened by `mv`: a slider of the mover now reaches a target through the square the
/// moving piece left, and did not before.
pub fn discovered_attacks(before: &Chess, mv: &Move, after: &Chess) -> Vec<Motif> {
    let Some(moved_from) = mv.from() else {
        return Vec::new();
    };
    let mover = before.turn();
    let board = after.board();
    let mut found = Vec::new();
    for revealed in (board.by_color(mover) & board.sliders()).without(mv.to()) {
        let Some(role) = board.role_at(revealed) else {
            continue;
        };
        let newly = board.attacks_from(revealed) & !before.board().attacks_from(revealed);
        for target in newly & board.by_color(!mover) {
            if attacks::between(revealed, target).contains(moved_from)
                && is_target(board, target, role)
            {
                found.push(Motif::DiscoveredAttack {
                    moved_from: moved_from.to_string(),
                    revealed: revealed.to_string(),
                    target: target.to_string(),
                    is_check: board.role_at(target) == Some(Role::King),
                });
            }
        }
    }
    found
}

/// `side`'s king on its back rank with no square to step off it, while the opponent has a rook or
/// queen.
pub fn back_rank(pos: &Chess, side: Color) -> Option<Motif> {
    let board = pos.board();
    let king = board.king_of(side)?;
    let back_rank = side.relative_rank(shakmaty::Rank::First);
    if king.rank() != back_rank || (board.by_color(!side) & board.rooks_and_queens()).is_empty() {
        return None;
    }
    let forward = attacks::king_attacks(king) & !Bitboard::from_rank(back_rank);
    let has_luft = forward.into_iter().any(|square| {
        !board.by_color(side).contains(square)
            && board.attacks_to(square, !side, board.occupied()).is_empty()
    });
    (!has_luft).then(|| Motif::BackRank {
        king: king.to_string(),
    })
}

/// The pawn on `square` if no enemy pawn stands ahead of it on its own or an adjacent file.
pub fn passed_pawn(board: &Board, square: Square) -> Option<Motif> {
    let pawn = board.piece_at(square)?;
    if pawn.role != Role::Pawn {
        return None;
    }
    let ahead = |other: Square| match pawn.color {
        Color::White => other.rank() > square.rank(),
        Color::Black => other.rank() < square.rank(),
    };
    let blocked = (board.pawns() & board.by_color(!pawn.color))
        .into_iter()
        .any(|other| other.file().distance(square.file()) <= 1 && ahead(other));
    (!blocked).then(|| Motif::PassedPawn {
        square: square.to_string(),
    })
}

pub fn promotion(mv: &Move) -> Option<Motif> {
    mv.promotion().map(|role| Motif::Promotion {
        square: mv.to().to_string(),
        to: PieceKind::from(role),
    })
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{after, pos, sq};
    use super::*;

    fn target(square: &str, piece: PieceKind) -> Target {
        Target {
            square: square.into(),
            piece,
        }
    }

    #[test]
    fn knight_fork_after_nxf7() {
        let p = pos("r1b1kbnr/pppp1Npp/8/6q1/2BnP3/8/PPPP1PPP/RNBQK2R b KQkq - 0 5");
        assert_eq!(
            fork(&p, sq("f7")),
            Some(Motif::Fork {
                from: "f7".into(),
                attacker: PieceKind::Knight,
                targets: vec![
                    target("g5", PieceKind::Queen),
                    target("h8", PieceKind::Rook)
                ],
                is_check: false,
            })
        );
    }

    #[test]
    fn royal_fork_counts_the_king() {
        let (_, _, p) = after("q3k3/8/8/1N6/8/8/8/4K3 w - - 0 1", "b5c7");
        assert_eq!(
            fork(&p, sq("c7")),
            Some(Motif::Fork {
                from: "c7".into(),
                attacker: PieceKind::Knight,
                targets: vec![
                    target("a8", PieceKind::Queen),
                    target("e8", PieceKind::King)
                ],
                is_check: true,
            })
        );
    }

    #[test]
    fn no_fork_on_a_single_target() {
        let p = pos("4k3/8/8/8/8/2n5/8/R3K3 b - - 0 1");
        assert_eq!(fork(&p, sq("c3")), None);
    }

    #[test]
    fn absolute_pin() {
        let p = pos("3k4/8/3n4/8/4B3/8/8/3RK3 b - - 0 1");
        assert_eq!(
            pins_and_skewers(&p, Color::White),
            vec![Motif::Pin {
                attacker: "d1".into(),
                pinned: "d6".into(),
                behind: "d8".into(),
                absolute: true,
            }]
        );
    }

    #[test]
    fn skewer_of_the_king() {
        let p = pos("q7/8/8/k7/8/8/8/R3K3 b - - 0 1");
        assert_eq!(
            pins_and_skewers(&p, Color::White),
            vec![Motif::Skewer {
                attacker: "a1".into(),
                front: "a5".into(),
                behind: "a8".into(),
            }]
        );
    }

    #[test]
    fn relative_pin() {
        let p = pos("4k3/8/8/1q6/8/3n4/8/5B1K b - - 0 1");
        assert_eq!(
            pins_and_skewers(&p, Color::White),
            vec![Motif::Pin {
                attacker: "f1".into(),
                pinned: "d3".into(),
                behind: "b5".into(),
                absolute: false,
            }]
        );
    }

    #[test]
    fn discovered_check() {
        let (before, mv, after) = after("4k3/8/8/8/8/8/4N3/4R1K1 w - - 0 1", "e2c3");
        assert_eq!(
            discovered_attacks(&before, &mv, &after),
            vec![Motif::DiscoveredAttack {
                moved_from: "e2".into(),
                revealed: "e1".into(),
                target: "e8".into(),
                is_check: true,
            }]
        );
    }

    #[test]
    fn back_rank_weakness() {
        let p = pos("6k1/5ppp/8/8/8/8/5PPP/3R2K1 b - - 0 1");
        assert_eq!(
            back_rank(&p, Color::Black),
            Some(Motif::BackRank { king: "g8".into() })
        );
        // White's king has the same shelter but Black has no rook or queen.
        assert_eq!(back_rank(&p, Color::White), None);
        // With luft on h6 there is no weakness.
        let luft = pos("6k1/5pp1/7p/8/8/8/5PPP/3R2K1 b - - 0 1");
        assert_eq!(back_rank(&luft, Color::Black), None);
    }

    #[test]
    fn passed_pawn_and_promotion() {
        let p = pos("4k3/8/8/3P4/8/8/5p2/4K3 w - - 0 1");
        assert_eq!(
            passed_pawn(p.board(), sq("d5")),
            Some(Motif::PassedPawn {
                square: "d5".into()
            })
        );
        let blocked = pos("4k3/2p5/8/3P4/8/8/8/4K3 w - - 0 1");
        assert_eq!(passed_pawn(blocked.board(), sq("d5")), None);

        let (_, mv, _) = after("4k3/8/8/8/8/8/5p2/K7 b - - 0 1", "f2f1q");
        assert_eq!(
            promotion(&mv),
            Some(Motif::Promotion {
                square: "f1".into(),
                to: PieceKind::Queen,
            })
        );
    }
}
