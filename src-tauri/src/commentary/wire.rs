//! What the model writes, and how it becomes `NarratedSentence`s.
//!
//! Structured outputs limit how many parameters may be nullable or unions, so the wire shape has
//! neither: slots are arrays (`{{m0}}` is `moves[0]`), a claim is one flat object whose `kind` says
//! which fields count, and a position is `{ t, id, ply }` with `id`/`ply` 0 unless `t` is "line".
//! The conversion here turns that into the typed `Slots`/`Claim` the validator checks; a claim
//! missing the pieces its kind needs is dropped, which only makes the sentence easier to reject.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    facts::{MoveRef, NumberRef, PieceKind, PieceRef, PositionRef, Side},
    narrate::{Claim, NarratedGame, NarratedMove, NarratedSentence, NarratedTakeaway, Slots},
};

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum PositionKind {
    Before,
    After,
    Line,
}

#[derive(Deserialize, Debug, Clone, Copy)]
struct WirePosition {
    t: PositionKind,
    #[serde(default)]
    id: u8,
    #[serde(default)]
    ply: u8,
}

impl From<WirePosition> for PositionRef {
    fn from(p: WirePosition) -> Self {
        match p.t {
            PositionKind::Before => PositionRef::Before,
            PositionKind::After => PositionRef::After,
            PositionKind::Line => PositionRef::Line {
                id: p.id,
                ply: p.ply,
            },
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
struct WireMove {
    san: String,
    side: Side,
    at: WirePosition,
}

impl From<WireMove> for MoveRef {
    fn from(m: WireMove) -> Self {
        MoveRef {
            san: m.san,
            side: m.side,
            at: m.at.into(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
struct WirePiece {
    piece: PieceKind,
    square: String,
    at: WirePosition,
}

impl From<WirePiece> for PieceRef {
    fn from(p: WirePiece) -> Self {
        PieceRef {
            piece: p.piece,
            square: p.square,
            at: p.at.into(),
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum WireSide {
    White,
    Black,
    None,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct WireSlots {
    #[serde(default)]
    moves: Vec<WireMove>,
    #[serde(default)]
    pieces: Vec<WirePiece>,
    #[serde(default)]
    squares: Vec<String>,
    #[serde(default)]
    numbers: Vec<NumberRef>,
    side: Option<WireSide>,
    #[serde(default)]
    piece_list: Vec<WirePiece>,
}

impl From<WireSlots> for Slots {
    fn from(w: WireSlots) -> Self {
        let mut moves = w.moves.into_iter().map(MoveRef::from);
        let mut pieces = w.pieces.into_iter().map(PieceRef::from);
        let mut squares = w.squares.into_iter();
        let mut numbers = w.numbers.into_iter();
        Slots {
            m0: moves.next(),
            m1: moves.next(),
            p0: pieces.next(),
            p1: pieces.next(),
            s0: squares.next(),
            s1: squares.next(),
            n0: numbers.next(),
            n1: numbers.next(),
            side: match w.side {
                Some(WireSide::White) => Some(Side::White),
                Some(WireSide::Black) => Some(Side::Black),
                _ => None,
            },
            pieces: w.piece_list.into_iter().map(PieceRef::from).collect(),
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ClaimKind {
    Attacks,
    Forks,
    Pins,
    Hanging,
    WinsMaterial,
    Mates,
    Better,
    Threatens,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct WireClaim {
    kind: ClaimKind,
    side: Side,
    #[serde(default)]
    pieces: Vec<WirePiece>,
    #[serde(default)]
    number: i32,
    #[serde(default)]
    line: u8,
    at: Option<WirePosition>,
}

impl WireClaim {
    /// The typed claim, or `None` when the pieces its kind needs are missing.
    fn into_claim(self) -> Option<Claim> {
        let side = self.side;
        let line = self.line;
        let number = self.number;
        let at = self.at.map(PositionRef::from).unwrap_or(PositionRef::After);
        let mut pieces = self.pieces.into_iter().map(PieceRef::from);
        Some(match self.kind {
            ClaimKind::Attacks => Claim::Attacks {
                attacker: pieces.next()?,
                target: pieces.next()?,
            },
            ClaimKind::Forks => Claim::Forks {
                attacker: pieces.next()?,
                targets: pieces.collect(),
            },
            ClaimKind::Pins => Claim::Pins {
                attacker: pieces.next()?,
                pinned: pieces.next()?,
            },
            ClaimKind::Hanging => Claim::Hanging {
                piece: pieces.next()?,
            },
            ClaimKind::WinsMaterial => Claim::WinsMaterial {
                side,
                pawns: number,
                line,
            },
            ClaimKind::Mates => Claim::Mates {
                side,
                mate_in: number.clamp(0, u8::MAX as i32) as u8,
                line,
            },
            ClaimKind::Better => Claim::Better { side, at },
            ClaimKind::Threatens => Claim::Threatens { side, line },
        })
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct WireSentence {
    template: String,
    #[serde(default)]
    slots: WireSlots,
    #[serde(default)]
    claims: Vec<WireClaim>,
    #[serde(default)]
    fact_ids: Vec<String>,
}

impl From<WireSentence> for NarratedSentence {
    fn from(w: WireSentence) -> Self {
        NarratedSentence {
            template: w.template,
            slots: w.slots.into(),
            claims: w
                .claims
                .into_iter()
                .filter_map(WireClaim::into_claim)
                .collect(),
            fact_ids: w.fact_ids,
        }
    }
}

fn sentences(w: Vec<WireSentence>) -> Vec<NarratedSentence> {
    w.into_iter().map(Into::into).collect()
}

#[derive(Deserialize, Debug, Clone)]
struct WireMoveItem {
    index: u32,
    #[serde(default)]
    sentences: Vec<WireSentence>,
}

#[derive(Deserialize, Debug, Clone)]
struct WireTakeaway {
    #[serde(default)]
    indices: Vec<u32>,
    #[serde(default)]
    sentences: Vec<WireSentence>,
}

#[derive(Deserialize, Debug, Clone)]
struct WireGame {
    #[serde(default)]
    sentences: Vec<WireSentence>,
    #[serde(default)]
    takeaways: Vec<WireTakeaway>,
}

/// One answer: the moves and, for the narrative request, the game summary.
#[derive(Deserialize, Debug, Clone)]
pub struct NarrationResponse {
    #[serde(default)]
    items: Vec<WireMoveItem>,
    #[serde(default)]
    narrative: Option<WireGame>,
}

impl NarrationResponse {
    /// The narrated moves (positions not yet filled in) and the narrative, if one was written.
    pub fn into_narrated(self) -> (Vec<NarratedMove>, Option<NarratedGame>) {
        let moves = self
            .items
            .into_iter()
            .map(|item| NarratedMove {
                index: item.index,
                fen_before: String::new(),
                san: String::new(),
                sentences: sentences(item.sentences),
            })
            .collect();
        let narrative = self
            .narrative
            .map(|game| NarratedGame {
                sentences: sentences(game.sentences),
                takeaways: game
                    .takeaways
                    .into_iter()
                    .map(|t| NarratedTakeaway {
                        indices: t.indices,
                        sentences: sentences(t.sentences),
                    })
                    .collect(),
            })
            .filter(|game| !game.sentences.is_empty() || !game.takeaways.is_empty());
        (moves, narrative)
    }
}

fn closed(properties: Value) -> Value {
    let required: Vec<String> = properties
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn string_enum(values: &[&str]) -> Value {
    json!({ "type": "string", "enum": values })
}

fn array(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

fn position_schema() -> Value {
    closed(json!({
        "t": string_enum(&["before", "after", "line"]),
        "id": { "type": "integer" },
        "ply": { "type": "integer" },
    }))
}

fn piece_schema() -> Value {
    closed(json!({
        "piece": string_enum(&["pawn", "knight", "bishop", "rook", "queen", "king"]),
        "square": { "type": "string" },
        "at": position_schema(),
    }))
}

fn sentence_schema() -> Value {
    let side = string_enum(&["white", "black"]);
    closed(json!({
        "template": { "type": "string" },
        "slots": closed(json!({
            "moves": array(closed(json!({
                "san": { "type": "string" },
                "side": side,
                "at": position_schema(),
            }))),
            "pieces": array(piece_schema()),
            "squares": array(json!({ "type": "string" })),
            "numbers": array(closed(json!({
                "value": { "type": "number" },
                "unit": string_enum(&["pawns", "mateIn", "winPercent", "move", "eval", "seconds"]),
            }))),
            "side": string_enum(&["white", "black", "none"]),
            "pieceList": array(piece_schema()),
        })),
        "claims": array(closed(json!({
            "kind": string_enum(&[
                "attacks", "forks", "pins", "hanging", "winsMaterial", "mates", "better", "threatens",
            ]),
            "side": side,
            "pieces": array(piece_schema()),
            "number": { "type": "integer" },
            "line": { "type": "integer" },
            "at": position_schema(),
        }))),
        "factIds": array(json!({ "type": "string" })),
    }))
}

/// The response schema: closed objects with every property required, no unions or nullable
/// fields, no recursion and no numeric or length constraints, as structured outputs require.
pub fn narration_schema() -> Value {
    let sentences = array(sentence_schema());
    closed(json!({
        "items": array(closed(json!({
            "index": { "type": "integer" },
            "sentences": sentences,
        }))),
        "narrative": closed(json!({
            "sentences": sentences,
            "takeaways": array(closed(json!({
                "indices": array(json!({ "type": "integer" })),
                "sentences": sentences,
            }))),
        })),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every object in the schema is closed and requires all of its properties, and nothing is a
    /// union or nullable.
    fn check(schema: &Value, path: &str, problems: &mut Vec<String>) {
        match schema {
            Value::Object(map) => {
                if map.get("type") == Some(&json!("object")) {
                    let keys = map["properties"].as_object().unwrap().len();
                    let required = map["required"].as_array().unwrap().len();
                    if map.get("additionalProperties") != Some(&json!(false)) {
                        problems.push(format!("{path}: not closed"));
                    }
                    if keys != required {
                        problems.push(format!("{path}: not every property is required"));
                    }
                }
                if map.get("type").is_some_and(Value::is_array) {
                    problems.push(format!("{path}: type array"));
                }
                for banned in [
                    "anyOf",
                    "oneOf",
                    "allOf",
                    "minLength",
                    "maxLength",
                    "minimum",
                    "maximum",
                    "$ref",
                ] {
                    if map.contains_key(banned) {
                        problems.push(format!("{path}: uses {banned}"));
                    }
                }
                for (key, value) in map {
                    check(value, &format!("{path}/{key}"), problems);
                }
            }
            Value::Array(values) => {
                for (i, value) in values.iter().enumerate() {
                    check(value, &format!("{path}/{i}"), problems);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn schema_has_no_unions_and_every_object_is_closed() {
        let mut problems = vec![];
        check(&narration_schema(), "", &mut problems);
        assert_eq!(problems, Vec::<String>::new());
    }

    fn piece(piece: &str, square: &str, at: Value) -> Value {
        json!({ "piece": piece, "square": square, "at": at })
    }

    #[test]
    fn an_answer_becomes_typed_slots_and_claims() {
        let before = json!({ "t": "before", "id": 0, "ply": 0 });
        let line = json!({ "t": "line", "id": 2, "ply": 1 });
        let after = json!({ "t": "after", "id": 0, "ply": 0 });
        let response: NarrationResponse = serde_json::from_value(json!({
            "items": [{ "index": 23, "sentences": [{
                "template": "{{m0}} allows {{m1}}, and {{p0}} is lost.",
                "slots": {
                    "moves": [
                        { "san": "Rc8", "side": "black", "at": before },
                        { "san": "d4", "side": "white", "at": line },
                    ],
                    "pieces": [piece("knight", "e5", after.clone())],
                    "squares": [], "numbers": [], "side": "none", "pieceList": []
                },
                "claims": [
                    { "kind": "winsMaterial", "side": "white", "pieces": [], "number": 3, "line": 2, "at": after },
                    { "kind": "forks", "side": "white", "pieces": [], "number": 0, "line": 0, "at": after },
                ],
                "factIds": ["line:2"]
            }]}],
            "narrative": { "sentences": [], "takeaways": [] }
        }))
        .unwrap();
        let (moves, narrative) = response.into_narrated();
        assert!(narrative.is_none(), "an empty narrative counts as none");
        let sentence = &moves[0].sentences[0];
        assert_eq!(
            sentence.slots.m1,
            Some(MoveRef {
                san: "d4".into(),
                side: Side::White,
                at: PositionRef::Line { id: 2, ply: 1 },
            })
        );
        assert_eq!(sentence.slots.p0.as_ref().unwrap().square, "e5");
        assert_eq!(sentence.slots.side, None);
        // The fork has no attacker and is dropped; the material claim stays.
        assert_eq!(
            sentence.claims,
            vec![Claim::WinsMaterial {
                side: Side::White,
                pawns: 3,
                line: 2
            }]
        );
    }

    #[test]
    fn claims_take_their_pieces_in_order() {
        let at = json!({ "t": "after", "id": 0, "ply": 0 });
        let claim: WireClaim = serde_json::from_value(json!({
            "kind": "forks", "side": "white",
            "pieces": [piece("knight", "c7", at.clone()), piece("king", "e8", at.clone()), piece("rook", "a8", at.clone())],
            "number": 0, "line": 0, "at": at
        }))
        .unwrap();
        let Some(Claim::Forks { attacker, targets }) = claim.into_claim() else {
            panic!("expected a fork");
        };
        assert_eq!(attacker.square, "c7");
        assert_eq!(
            targets
                .iter()
                .map(|t| t.square.as_str())
                .collect::<Vec<_>>(),
            ["e8", "a8"]
        );
    }
}
