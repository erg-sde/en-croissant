use std::collections::HashMap;

use log::info;
use serde::{Deserialize, Serialize};
use shakmaty::{
    fen::{Epd, Fen},
    san::San,
    CastlingMode, Chess, EnPassantMode, Position, Setup,
};

use lazy_static::lazy_static;
use specta::Type;
use strsim::{jaro_winkler, sorensen_dice};

use crate::error::Error;

#[derive(Debug, Clone)]
struct Opening {
    eco: String,
    name: String,
    setup: Setup,
    pgn: Option<String>,
}

#[derive(Debug, Clone, Type, Serialize)]
pub struct OutOpening {
    name: String,
    fen: String,
}

/// The deepest position of a game found in the opening table.
#[derive(Debug, Clone, Type, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningMatch {
    pub eco: String,
    pub name: String,
    pub pgn: Option<String>,
    /// Index of the matching position in the list that was searched.
    pub index: u32,
}

#[derive(Deserialize)]
struct OpeningRecord {
    eco: String,
    name: String,
    pgn: String,
}

const TSV_DATA: [&[u8]; 5] = [
    include_bytes!("../data/a.tsv"),
    include_bytes!("../data/b.tsv"),
    include_bytes!("../data/c.tsv"),
    include_bytes!("../data/d.tsv"),
    include_bytes!("../data/e.tsv"),
];

const FISCHER_RANDOM_DATA: &[u8] = include_bytes!("../data/frc.tsv");

#[derive(Deserialize)]
struct FischerRandomRecord {
    name: String,
    fen: String,
}

#[tauri::command]
#[specta::specta]
pub fn get_opening_from_fen(fen: &str) -> Result<String, Error> {
    lookup(&epd_of(fen)?)
        .map(|o| o.name.clone())
        .ok_or(Error::NoOpeningFound)
}

#[tauri::command]
#[specta::specta]
pub fn get_opening_from_name(name: &str) -> Result<String, Error> {
    OPENINGS
        .iter()
        .find(|o| o.name == name)
        .map(|o| o.pgn.clone().expect("opening without pgn"))
        .ok_or_else(|| Error::NoOpeningFound)
}

#[tauri::command]
#[specta::specta]
pub fn get_opening_from_fens(fens: Vec<String>) -> Result<String, Error> {
    get_opening_match(fens)?
        .map(|m| m.name)
        .ok_or(Error::NoOpeningFound)
}

/// Finds the last position in `fens` that is in the opening table. Positions are compared
/// without move counters, so transpositions match; FENs that do not parse are skipped.
#[tauri::command]
#[specta::specta]
pub fn get_opening_match(fens: Vec<String>) -> Result<Option<OpeningMatch>, Error> {
    for (index, fen) in fens.iter().enumerate().rev() {
        let Ok(epd) = epd_of(fen) else {
            continue;
        };
        if let Some(opening) = lookup(&epd) {
            return Ok(Some(OpeningMatch {
                eco: opening.eco.clone(),
                name: opening.name.clone(),
                pgn: opening.pgn.clone(),
                index: index as u32,
            }));
        }
    }
    Ok(None)
}

pub fn get_opening_from_setup(setup: Setup) -> Result<String, Error> {
    lookup(&Epd::from_setup(setup))
        .map(|o| o.name.clone())
        .ok_or(Error::NoOpeningFound)
}

fn lookup(epd: &Epd) -> Option<&'static Opening> {
    OPENING_INDEX.get(epd).map(|&i| &OPENINGS[i])
}

/// The counter-free key of a FEN, with the en passant square kept only when a capture is legal,
/// as in the table. A FEN that is not a legal position is keyed as written.
fn epd_of(fen: &str) -> Result<Epd, Error> {
    let setup = fen.parse::<Fen>()?.into_setup();
    let mode = CastlingMode::detect(&setup);
    Ok(match setup.clone().position::<Chess>(mode) {
        Ok(pos) => Epd::from_position(pos, EnPassantMode::Legal),
        Err(_) => Epd::from_setup(setup),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn search_opening_name(query: String) -> Result<Vec<OutOpening>, Error> {
    let lower_query = query.to_lowercase();
    let scores = OPENINGS
        .iter()
        .map(|opening| {
            let lower_name = opening.name.to_lowercase();
            let sorenson_score = sorensen_dice(&lower_query, &lower_name);
            let jaro_score = jaro_winkler(&lower_query, &lower_name);
            let score = sorenson_score.max(jaro_score);
            (opening.clone(), score)
        })
        .collect::<Vec<_>>();
    let mut best_matches = scores
        .into_iter()
        .filter(|(_, score)| *score > 0.8)
        .collect::<Vec<_>>();

    best_matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let best_matches_names = best_matches
        .iter()
        .map(|(o, _)| o.clone())
        .take(15)
        .map(|o| OutOpening {
            name: o.name,
            fen: Fen::from_setup(o.setup.clone()).to_string(),
        })
        .collect();
    Ok(best_matches_names)
}

lazy_static! {
    static ref OPENINGS: Vec<Opening> = {
        info!("Initializing openings table...");

        let mut positions = vec![
            Opening {
                eco: "Extra".to_string(),
                name: "Starting Position".to_string(),
                setup: Setup::default(),
                pgn: None,
            },
            Opening {
                eco: "Extra".to_string(),
                name: "Empty Board".to_string(),
                setup: Setup::empty(),
                pgn: None,
            },
        ];

        for tsv in TSV_DATA {
            let mut rdr = csv::ReaderBuilder::new().delimiter(b'\t').from_reader(tsv);
            for result in rdr.deserialize() {
                let record: OpeningRecord = result.expect("Failed to deserialize opening");
                let mut pos = Chess::default();
                for token in record.pgn.split_whitespace() {
                    if let Ok(san) = token.parse::<San>() {
                        pos.play_unchecked(&san.to_move(&pos).expect("legal move"));
                    }
                }
                positions.push(Opening {
                    eco: record.eco,
                    name: record.name,
                    setup: pos.into_setup(EnPassantMode::Legal),
                    pgn: Some(record.pgn),
                });
            }
        }
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .from_reader(FISCHER_RANDOM_DATA);
        for result in rdr.deserialize() {
            let record: FischerRandomRecord = result.expect("Failed to deserialize opening");
            let fen: Fen = record.fen.parse().expect("Failed to parse fen");
            positions.push(Opening {
                eco: "FRC".to_string(),
                name: record.name,
                setup: fen.into_setup(),
                pgn: None,
            });
        }
        positions
    };

    /// Position key to index in `OPENINGS`. The first entry wins when two share a position, as
    /// in the linear scan this replaces (the standard start is also a Fischer Random setup).
    static ref OPENING_INDEX: HashMap<Epd, usize> = {
        let mut index = HashMap::with_capacity(OPENINGS.len());
        for (i, opening) in OPENINGS.iter().enumerate() {
            index.entry(Epd::from_setup(opening.setup.clone())).or_insert(i);
        }
        index
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_opening() {
        let opening =
            get_opening_from_fen("rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPPKPPP/RNBQ1BNR b kq - 1 2")
                .unwrap();
        assert_eq!(opening, "Bongcloud Attack");
    }

    /// FENs after each move of `sans`, starting with the initial position.
    fn fens(sans: &str) -> Vec<String> {
        let mut pos = Chess::default();
        let mut fens = vec![Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string()];
        for san in sans.split_whitespace() {
            let m = san.parse::<San>().unwrap().to_move(&pos).unwrap();
            pos.play_unchecked(&m);
            fens.push(Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string());
        }
        fens
    }

    #[test]
    fn match_reports_eco_and_index_of_the_last_named_position() {
        let found = get_opening_match(fens("d4 d5 c4 e6 Nc3 Nf6 Bg5 Be7 h4"))
            .unwrap()
            .unwrap();
        assert_eq!(found.eco, "D53");
        assert_eq!(found.index, 8);
    }

    #[test]
    fn match_finds_transpositions() {
        // The table reaches this position through 1. d4 d5 2. c4 e6 3. Nc3 Nf6.
        let found = get_opening_match(fens("d4 Nf6 c4 e6 Nc3 d5"))
            .unwrap()
            .unwrap();
        assert_eq!(found.eco, "D35");
        assert_eq!(found.name, "Queen's Gambit Declined: Normal Defense");
        assert_eq!(found.index, 6);
        assert_eq!(
            get_opening_from_fens(fens("d4 Nf6 c4 e6 Nc3 d5")).unwrap(),
            found.name
        );
    }

    #[test]
    fn match_ignores_an_en_passant_square_without_a_capture() {
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        let found = get_opening_match(vec![fen.to_string()]).unwrap().unwrap();
        assert_eq!(found.eco, "B00");
        assert_eq!(found.index, 0);
    }

    #[test]
    fn match_skips_invalid_fens_and_reports_none() {
        assert!(get_opening_match(vec!["not a fen".to_string()])
            .unwrap()
            .is_none());
        assert!(matches!(
            get_opening_from_fens(vec!["8/8/8/8/8/8/8/K6k w - - 0 1".to_string()]),
            Err(Error::NoOpeningFound)
        ));
    }
}
