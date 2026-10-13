//! What the model writes, and how it becomes `NarratedSentence`s.
//!
//! Structured outputs compile the response schema into a grammar with size limits, so the wire
//! shape is small and flat: no unions, no nullable fields, and board references written as *refs*
//! instead of copied values. A ref names one entry of a cited fact's vocabulary,
//! `"<fact id>/<moves|pieces|squares|numbers>/<n>"` (for example `"line:2/moves/1"`), and is looked
//! up here in the request, so the model cannot garble a move, a square or a number while copying it.
//! A ref that does not resolve leaves its slot empty and the frontend validator drops the sentence;
//! a claim missing a piece it needs is dropped, which only makes the sentence easier to reject.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    facts::{FactVocabulary, MoveRef, NumberRef, PieceRef, PositionRef, Side},
    narrate::{
        Claim, NarratedGame, NarratedSentence, NarratedTakeaway, NarrationFact, NarrationGame,
        NarrationItem, Slots,
    },
};

/// The facts a chunk's sentences may cite, by the id they are cited with.
pub struct RefTable<'a> {
    facts: Vec<(String, &'a FactVocabulary)>,
}

impl<'a> RefTable<'a> {
    /// The facts of one move, cited by their own ids.
    pub fn for_item(item: &'a NarrationItem) -> Self {
        Self {
            facts: item
                .facts
                .iter()
                .map(|f| (f.id.clone(), &f.vocabulary))
                .collect(),
        }
    }

    /// Game facts, and the turning points' move facts cited as `"<index>/<id>"`.
    pub fn for_game(game: &'a NarrationGame) -> Self {
        let mut facts: Vec<(String, &FactVocabulary)> = game
            .facts
            .iter()
            .map(|f| (f.id.clone(), &f.vocabulary))
            .collect();
        for item in &game.turning_points {
            for fact in &item.facts {
                facts.push((cited_id(item, fact), &fact.vocabulary));
            }
        }
        Self { facts }
    }

    fn entry(&self, reference: &str, kind: &str) -> Option<(&'a FactVocabulary, usize)> {
        let mut parts = reference.rsplitn(3, '/');
        let n: usize = parts.next()?.trim().parse().ok()?;
        if parts.next()? != kind {
            return None;
        }
        let fact = parts.next()?;
        self.facts
            .iter()
            .find(|(id, _)| id == fact)
            .map(|(_, vocabulary)| (*vocabulary, n))
    }

    fn move_ref(&self, r: &str) -> Option<MoveRef> {
        let (v, n) = self.entry(r, "moves")?;
        v.moves.get(n).cloned()
    }

    fn piece(&self, r: &str) -> Option<PieceRef> {
        let (v, n) = self.entry(r, "pieces")?;
        v.pieces.get(n).cloned()
    }

    fn square(&self, r: &str) -> Option<String> {
        let (v, n) = self.entry(r, "squares")?;
        v.squares.get(n).cloned()
    }

    fn number(&self, r: &str) -> Option<NumberRef> {
        let (v, n) = self.entry(r, "numbers")?;
        v.numbers.get(n).cloned()
    }
}

/// How a turning point's fact is cited from the narrative.
pub fn cited_id(item: &NarrationItem, fact: &NarrationFact) -> String {
    format!("{}/{}", item.index, fact.id)
}

/// Every ref a fact offers, with the value it stands for, as shown to the model.
pub fn refs_of(id: &str, vocabulary: &FactVocabulary) -> Value {
    let mut refs = serde_json::Map::new();
    for (n, m) in vocabulary.moves.iter().enumerate() {
        refs.insert(format!("{id}/moves/{n}"), json!(m));
    }
    for (n, p) in vocabulary.pieces.iter().enumerate() {
        refs.insert(format!("{id}/pieces/{n}"), json!(p));
    }
    for (n, s) in vocabulary.squares.iter().enumerate() {
        refs.insert(format!("{id}/squares/{n}"), json!(s));
    }
    for (n, x) in vocabulary.numbers.iter().enumerate() {
        refs.insert(format!("{id}/numbers/{n}"), json!(x));
    }
    Value::Object(refs)
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum WireSide {
    White,
    Black,
    None,
}

impl WireSide {
    fn side(self) -> Option<Side> {
        match self {
            WireSide::White => Some(Side::White),
            WireSide::Black => Some(Side::Black),
            WireSide::None => None,
        }
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct WireSlots {
    #[serde(default)]
    moves: Vec<String>,
    #[serde(default)]
    pieces: Vec<String>,
    #[serde(default)]
    squares: Vec<String>,
    #[serde(default)]
    numbers: Vec<String>,
    side: Option<WireSide>,
    #[serde(default)]
    piece_list: Vec<String>,
}

fn get(refs: &[String], n: usize) -> Option<&str> {
    refs.get(n).map(String::as_str)
}

impl WireSlots {
    fn resolve(self, table: &RefTable) -> Slots {
        Slots {
            m0: get(&self.moves, 0).and_then(|r| table.move_ref(r)),
            m1: get(&self.moves, 1).and_then(|r| table.move_ref(r)),
            p0: get(&self.pieces, 0).and_then(|r| table.piece(r)),
            p1: get(&self.pieces, 1).and_then(|r| table.piece(r)),
            s0: get(&self.squares, 0).and_then(|r| table.square(r)),
            s1: get(&self.squares, 1).and_then(|r| table.square(r)),
            n0: get(&self.numbers, 0).and_then(|r| table.number(r)),
            n1: get(&self.numbers, 1).and_then(|r| table.number(r)),
            side: self.side.and_then(WireSide::side),
            pieces: self
                .piece_list
                .iter()
                .filter_map(|r| table.piece(r))
                .collect(),
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
    pieces: Vec<String>,
    #[serde(default)]
    number: i32,
    #[serde(default)]
    line: u8,
    /// "before", "after", or "line:<id>:<ply>".
    #[serde(default)]
    at: String,
}

fn position(at: &str) -> Option<PositionRef> {
    match at.trim() {
        "before" => Some(PositionRef::Before),
        "after" => Some(PositionRef::After),
        other => {
            let mut parts = other.strip_prefix("line:")?.split(':');
            let id = parts.next()?.parse().ok()?;
            let ply = parts.next()?.parse().ok()?;
            Some(PositionRef::Line { id, ply })
        }
    }
}

impl WireClaim {
    /// The typed claim, or `None` when a piece or position it needs does not resolve.
    fn resolve(self, table: &RefTable) -> Option<Claim> {
        let pieces: Option<Vec<PieceRef>> = self.pieces.iter().map(|r| table.piece(r)).collect();
        let mut pieces = pieces?.into_iter();
        let (side, line, number) = (self.side, self.line, self.number);
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
            ClaimKind::Better => Claim::Better {
                side,
                at: position(&self.at)?,
            },
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

impl WireSentence {
    fn resolve(self, table: &RefTable) -> NarratedSentence {
        NarratedSentence {
            template: self.template,
            slots: self.slots.resolve(table),
            claims: self
                .claims
                .into_iter()
                .filter_map(|c| c.resolve(table))
                .collect(),
            fact_ids: self.fact_ids,
        }
    }
}

fn resolve_all(sentences: Vec<WireSentence>, table: &RefTable) -> Vec<NarratedSentence> {
    sentences.into_iter().map(|s| s.resolve(table)).collect()
}

#[derive(Deserialize, Debug, Clone)]
struct WireMoveItem {
    index: u32,
    #[serde(default)]
    sentences: Vec<WireSentence>,
}

/// The answer to a move chunk.
#[derive(Deserialize, Debug, Clone)]
pub struct MovesResponse {
    #[serde(default)]
    items: Vec<WireMoveItem>,
}

impl MovesResponse {
    /// `(index, sentences)` for each move that was asked for, refs resolved against its facts.
    pub fn resolve(self, items: &[NarrationItem]) -> Vec<(&NarrationItem, Vec<NarratedSentence>)> {
        self.items
            .into_iter()
            .filter_map(|answer| {
                let item = items.iter().find(|i| i.index == answer.index)?;
                let table = RefTable::for_item(item);
                Some((item, resolve_all(answer.sentences, &table)))
            })
            .collect()
    }
}

#[derive(Deserialize, Debug, Clone)]
struct WireTakeaway {
    #[serde(default)]
    indices: Vec<u32>,
    #[serde(default)]
    sentences: Vec<WireSentence>,
}

/// The answer to the narrative request.
#[derive(Deserialize, Debug, Clone)]
pub struct NarrativeResponse {
    #[serde(default)]
    sentences: Vec<WireSentence>,
    #[serde(default)]
    takeaways: Vec<WireTakeaway>,
}

impl NarrativeResponse {
    /// The narrative with refs resolved, or `None` when nothing was written.
    pub fn resolve(self, game: &NarrationGame) -> Option<NarratedGame> {
        let table = RefTable::for_game(game);
        let narrated = NarratedGame {
            sentences: resolve_all(self.sentences, &table),
            takeaways: self
                .takeaways
                .into_iter()
                .map(|t| NarratedTakeaway {
                    indices: t.indices,
                    sentences: resolve_all(t.sentences, &table),
                })
                .collect(),
        };
        (!narrated.sentences.is_empty() || !narrated.takeaways.is_empty()).then_some(narrated)
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

fn strings() -> Value {
    json!({ "type": "array", "items": { "type": "string" } })
}

fn array(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

fn sentence_schema() -> Value {
    closed(json!({
        "template": { "type": "string" },
        "slots": closed(json!({
            "moves": strings(),
            "pieces": strings(),
            "squares": strings(),
            "numbers": strings(),
            "side": { "type": "string", "enum": ["white", "black", "none"] },
            "pieceList": strings(),
        })),
        "claims": array(closed(json!({
            "kind": { "type": "string", "enum": [
                "attacks", "forks", "pins", "hanging", "winsMaterial", "mates", "better", "threatens",
            ] },
            "side": { "type": "string", "enum": ["white", "black"] },
            "pieces": strings(),
            "number": { "type": "integer" },
            "line": { "type": "integer" },
            "at": { "type": "string" },
        }))),
        "factIds": strings(),
    }))
}

/// The schema of a move chunk's answer: no unions, no nullable fields, no constraints.
pub fn moves_schema() -> Value {
    closed(json!({
        "items": array(closed(json!({
            "index": { "type": "integer" },
            "sentences": array(sentence_schema()),
        }))),
    }))
}

/// The schema of the narrative's answer.
pub fn narrative_schema() -> Value {
    let sentences = array(sentence_schema());
    closed(json!({
        "sentences": sentences,
        "takeaways": array(closed(json!({
            "indices": array(json!({ "type": "integer" })),
            "sentences": sentences,
        }))),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commentary::{
        facts::{CommentaryKind, FactKind, PieceKind},
        narrate::NarrationFact,
    };

    /// Every object is closed and requires all of its properties; nothing is a union, nullable or
    /// constrained.
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
    fn schemas_are_small_flat_and_closed() {
        let mut problems = vec![];
        check(&moves_schema(), "moves", &mut problems);
        check(&narrative_schema(), "narrative", &mut problems);
        assert_eq!(problems, Vec::<String>::new());
        // A guard against regrowth: the old schema that the API rejected was ~25 KB.
        assert!(moves_schema().to_string().len() < 2500);
        assert!(narrative_schema().to_string().len() < 5000);
    }

    fn item() -> NarrationItem {
        let rc8 = MoveRef {
            san: "Rc8".into(),
            side: Side::Black,
            at: PositionRef::Before,
        };
        let d4 = MoveRef {
            san: "d4".into(),
            side: Side::White,
            at: PositionRef::Line { id: 2, ply: 0 },
        };
        let knight = PieceRef {
            piece: PieceKind::Knight,
            square: "e5".into(),
            at: PositionRef::Line { id: 2, ply: 0 },
        };
        NarrationItem {
            index: 23,
            fen_before: "8/8/8/8/8/8/8/8 b - - 0 12".into(),
            move_number: 12,
            san: "Rc8".into(),
            side: Side::Black,
            classification: "?".into(),
            kind: CommentaryKind::Mistake,
            budget_words: 60,
            facts: vec![NarrationFact {
                id: "line:2".into(),
                kind: FactKind::Line,
                vocabulary: FactVocabulary {
                    moves: vec![rc8, d4],
                    pieces: vec![knight],
                    squares: vec!["e5".into()],
                    numbers: vec![NumberRef {
                        value: 3.0,
                        unit: crate::commentary::facts::NumberUnit::Pawns,
                    }],
                    sides: vec![Side::White],
                },
                text_hint: "Rc8 allows d4, winning the knight.".into(),
            }],
        }
    }

    #[test]
    fn refs_resolve_to_the_values_of_the_cited_fact() {
        let items = [item()];
        let response: MovesResponse = serde_json::from_value(json!({
            "items": [
                { "index": 23, "sentences": [{
                    "template": "{{m0}} allows {{m1}}, which wins {{p0}}.",
                    "slots": {
                        "moves": ["line:2/moves/0", "line:2/moves/1"],
                        "pieces": ["line:2/pieces/0"],
                        "squares": [], "numbers": ["line:2/numbers/9"], "side": "none", "pieceList": []
                    },
                    "claims": [
                        { "kind": "winsMaterial", "side": "white", "pieces": [], "number": 3, "line": 2, "at": "" },
                        { "kind": "forks", "side": "white", "pieces": ["line:2/pieces/7"], "number": 0, "line": 0, "at": "" },
                        { "kind": "better", "side": "white", "pieces": [], "number": 0, "line": 0, "at": "line:2:0" }
                    ],
                    "factIds": ["line:2"]
                }]},
                { "index": 99, "sentences": [] }
            ]
        }))
        .unwrap();
        let resolved = response.resolve(&items);
        assert_eq!(
            resolved.len(),
            1,
            "an index that was not asked for is dropped"
        );
        let sentence = &resolved[0].1[0];
        assert_eq!(sentence.slots.m1.as_ref().unwrap().san, "d4");
        assert_eq!(sentence.slots.p0.as_ref().unwrap().square, "e5");
        assert_eq!(
            sentence.slots.n0, None,
            "an out-of-range ref resolves to nothing"
        );
        assert_eq!(
            sentence.claims,
            vec![
                Claim::WinsMaterial {
                    side: Side::White,
                    pawns: 3,
                    line: 2
                },
                Claim::Better {
                    side: Side::White,
                    at: PositionRef::Line { id: 2, ply: 0 }
                },
            ],
            "the fork whose piece does not resolve is dropped"
        );
    }

    #[test]
    fn the_narrative_resolves_turning_point_refs_by_their_cited_id() {
        let game = NarrationGame {
            facts: vec![],
            result: "0-1".into(),
            turning_points: vec![item()],
        };
        let response: NarrativeResponse = serde_json::from_value(json!({
            "sentences": [{
                "template": "{{m0}} decided the game.",
                "slots": { "moves": ["23/line:2/moves/0"], "pieces": [], "squares": [], "numbers": [], "side": "black", "pieceList": [] },
                "claims": [], "factIds": ["23/line:2"]
            }],
            "takeaways": []
        }))
        .unwrap();
        let narrated = response.resolve(&game).unwrap();
        assert_eq!(narrated.sentences[0].slots.m0.as_ref().unwrap().san, "Rc8");
        assert_eq!(narrated.sentences[0].slots.side, Some(Side::Black));
        let empty: NarrativeResponse =
            serde_json::from_value(json!({ "sentences": [], "takeaways": [] })).unwrap();
        assert!(empty.resolve(&game).is_none());
    }

    #[test]
    fn refs_shown_to_the_model_name_every_vocabulary_entry() {
        let item = item();
        let refs = refs_of("line:2", &item.facts[0].vocabulary);
        let keys: Vec<&String> = refs.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            [
                "line:2/moves/0",
                "line:2/moves/1",
                "line:2/numbers/0",
                "line:2/pieces/0",
                "line:2/squares/0"
            ]
        );
    }
}
