//! What the model writes: plain sentences per move, and the game narrative with its lessons.
//!
//! Structured outputs compile the response schema into a grammar with size limits, so the shapes
//! here are small: arrays of strings, no unions, no nullable fields. The frontend checks every move
//! and piece a sentence names against the board before showing it.

use serde::Deserialize;
use serde_json::{json, Value};

use super::narrate::{NarratedGame, NarratedTakeaway, NarrationItem};

/// Sentences with surrounding whitespace removed and empty ones dropped.
fn clean(sentences: Vec<String>) -> Vec<String> {
    sentences
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Deserialize, Debug, Clone)]
struct WireMoveItem {
    index: u32,
    #[serde(default)]
    sentences: Vec<String>,
}

/// The answer to a move chunk.
#[derive(Deserialize, Debug, Clone)]
pub struct MovesResponse {
    #[serde(default)]
    items: Vec<WireMoveItem>,
}

impl MovesResponse {
    /// The sentences of each move that was asked for.
    pub fn resolve(self, items: &[NarrationItem]) -> Vec<(&NarrationItem, Vec<String>)> {
        self.items
            .into_iter()
            .filter_map(|answer| {
                let item = items.iter().find(|i| i.index == answer.index)?;
                Some((item, clean(answer.sentences)))
            })
            .collect()
    }
}

#[derive(Deserialize, Debug, Clone)]
struct WireTakeaway {
    #[serde(default)]
    indices: Vec<u32>,
    #[serde(default)]
    sentences: Vec<String>,
}

/// The answer to the narrative request.
#[derive(Deserialize, Debug, Clone)]
pub struct NarrativeResponse {
    #[serde(default)]
    sentences: Vec<String>,
    #[serde(default)]
    takeaways: Vec<WireTakeaway>,
}

impl NarrativeResponse {
    /// The narrative and its lessons, or `None` when nothing was written.
    pub fn resolve(self, _game: &super::narrate::NarrationGame) -> Option<NarratedGame> {
        let narrated = NarratedGame {
            sentences: clean(self.sentences),
            takeaways: self
                .takeaways
                .into_iter()
                .map(|t| NarratedTakeaway {
                    indices: t.indices,
                    sentences: clean(t.sentences),
                })
                .filter(|t| !t.sentences.is_empty())
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

fn array(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

fn strings() -> Value {
    array(json!({ "type": "string" }))
}

/// The schema of a move chunk's answer.
pub fn moves_schema() -> Value {
    closed(json!({
        "items": array(closed(json!({
            "index": { "type": "integer" },
            "sentences": strings(),
        }))),
    }))
}

/// The schema of the narrative's answer.
pub fn narrative_schema() -> Value {
    closed(json!({
        "sentences": strings(),
        "takeaways": array(closed(json!({
            "indices": array(json!({ "type": "integer" })),
            "sentences": strings(),
        }))),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commentary::{
        facts::{CommentaryKind, Side},
        narrate::NarrationGame,
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
                for banned in ["anyOf", "oneOf", "allOf", "minLength", "maximum", "$ref"] {
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
        assert!(moves_schema().to_string().len() < 1000);
        assert!(narrative_schema().to_string().len() < 1000);
    }

    fn item(index: u32) -> NarrationItem {
        NarrationItem {
            index,
            fen_before: "8/8/8/8/8/8/8/8 b - - 0 12".into(),
            fen_after: "8/8/8/8/8/8/8/8 w - - 1 13".into(),
            move_number: 12,
            san: "Rc8".into(),
            side: Side::Black,
            classification: "?".into(),
            kind: CommentaryKind::Mistake,
            eval_before: Some("+0.3".into()),
            eval_after: "+1.5".into(),
            lines: vec![],
            budget_words: 60,
            facts: vec![],
        }
    }

    #[test]
    fn only_asked_moves_are_kept_and_sentences_cleaned() {
        let items = [item(23)];
        let response: MovesResponse = serde_json::from_value(json!({
            "items": [
                { "index": 23, "sentences": ["  Rc8 drops the knight.  ", "", "Play Nd7 instead."] },
                { "index": 99, "sentences": ["Not asked for."] }
            ]
        }))
        .unwrap();
        let resolved = response.resolve(&items);
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved[0].1,
            ["Rc8 drops the knight.", "Play Nd7 instead."]
        );
    }

    #[test]
    fn an_empty_narrative_is_none() {
        let game = NarrationGame {
            facts: vec![],
            result: "1-0".into(),
            turning_points: vec![],
        };
        let empty: NarrativeResponse = serde_json::from_value(json!({
            "sentences": [" "], "takeaways": [{ "indices": [3], "sentences": [] }]
        }))
        .unwrap();
        assert!(empty.resolve(&game).is_none());
        let written: NarrativeResponse = serde_json::from_value(json!({
            "sentences": ["White won by attacking the king."],
            "takeaways": [{ "indices": [12, 20], "sentences": ["Develop before attacking."] }]
        }))
        .unwrap();
        let narrated = written.resolve(&game).unwrap();
        assert_eq!(narrated.takeaways[0].indices, [12, 20]);
    }
}
