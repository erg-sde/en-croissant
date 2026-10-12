//! The narration prompt. The system prompt is a constant and the user message is a pure function of
//! the `NarrationRequest`, so nothing a user typed into a game (names, headers, comments) can reach
//! a provider: the request type has no field for it.

use serde_json::{json, Value};

use super::{
    facts::{CommentaryKind, FactKind, FactVocabulary, MoveRef, Perspective, PositionRef, Side},
    narrate::{NarrationFact, NarrationGame, NarrationItem, NarrationRequest},
    provider::NarratorConfig,
};

pub const SYSTEM_PROMPT: &str = r#"You turn verified chess facts into short coaching sentences for a game report. Each fact comes with a draft sentence that is already correct. Rephrase and connect the drafts into natural prose; never add information.

Rules:
1. Nothing exists outside the facts you are given. Use only the moves, pieces, squares, numbers and sides listed in each fact's vocabulary.
2. Never re-judge a move. The classification and kind are final.
3. Never pad. If the facts say little, write little. Stay within each item's word budget.
4. Never claim what a player intended or felt, and never mention an engine, a computer or evaluations in centipawns.
5. Never invent a number.

How to write a sentence:
- Write a template. Every move, piece, square, number and side comes from a slot, written as a placeholder: {{m0}} {{m1}} for moves, {{p0}} {{p1}} for pieces, {{s0}} {{s1}} for squares, {{n0}} {{n1}} for numbers, {{side}} for a side, {{pieces}} for a list of pieces. Never write a square, a move, a piece name or a digit as literal text.
- Fill each placeholder you use with a value copied exactly from the vocabulary of a fact the sentence cites. Set unused slots to null and leave "pieces" empty when unused.
- Every relation you state must be declared as a claim: attacks, forks, pins, hangs (is undefended and can be taken), wins material, mates, is better, threatens. A sentence with no relation has no claims.
- List the ids of the facts each sentence relies on in "factIds". Every sentence cites at least one fact of its own item (or a game fact for the narrative).

Per move: first the verdict, then how it is punished or what the better move achieves, then at most one positional reason.

Address the player as "you" only when perspective is "white" or "black" and the move is theirs; otherwise say White and Black. Write in the language given by "lang".

For the game narrative: a few sentences on how the game went (opening, turning points, how it ended) and takeaways that each name the moves (by index) they rest on. Narrative sentences cite game facts ("game:...") or move facts written as "<index>/<id>"."#;

/// What one request covers.
pub enum ChunkContent<'a> {
    Moves(&'a [NarrationItem]),
    Narrative(&'a NarrationGame),
}

fn word_limit(config: &NarratorConfig, item: &NarrationItem) -> u16 {
    if item.budget_words > 0 {
        item.budget_words
    } else {
        config.budget.move_words
    }
}

fn fact_json(fact: &NarrationFact) -> Value {
    json!({
        "id": fact.id,
        "kind": fact.kind,
        "draft": fact.text_hint,
        "vocabulary": fact.vocabulary,
    })
}

fn item_json(config: &NarratorConfig, item: &NarrationItem) -> Value {
    json!({
        "index": item.index,
        "moveNumber": item.move_number,
        "san": item.san,
        "side": item.side,
        "fenBefore": item.fen_before,
        "classification": item.classification,
        "kind": item.kind,
        "wordBudget": word_limit(config, item),
        "facts": item.facts.iter().map(fact_json).collect::<Vec<_>>(),
    })
}

/// The user message for one chunk: JSON built only from the request and the configured budgets.
pub fn user_message(
    request: &NarrationRequest,
    config: &NarratorConfig,
    content: &ChunkContent<'_>,
) -> String {
    let task = match content {
        ChunkContent::Moves(items) => json!({
            "task": "Write the sentences for each move in \"items\". Return one entry per item, with its index, and set \"narrative\" to null.",
            "items": items.iter().map(|i| item_json(config, i)).collect::<Vec<_>>(),
        }),
        ChunkContent::Narrative(game) => json!({
            "task": "Write the game narrative and its takeaways. Return an empty \"items\" list.",
            "wordBudget": config.budget.narrative_words,
            "keyMomentWordBudget": config.budget.key_moment_words,
            "result": game.result,
            "gameFacts": game.facts.iter().map(fact_json).collect::<Vec<_>>(),
            "turningPoints": game.turning_points.iter().map(|i| item_json(config, i)).collect::<Vec<_>>(),
        }),
    };
    json!({
        "lang": request.lang,
        "perspective": request.perspective,
        "request": task,
    })
    .to_string()
}

fn nullable(schema: Value) -> Value {
    json!({ "anyOf": [schema, { "type": "null" }] })
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

fn position_schema() -> Value {
    json!({ "anyOf": [
        closed(json!({ "t": string_enum(&["before"]) })),
        closed(json!({ "t": string_enum(&["after"]) })),
        closed(json!({ "t": string_enum(&["line"]), "id": { "type": "integer" }, "ply": { "type": "integer" } })),
    ]})
}

fn side_schema() -> Value {
    string_enum(&["white", "black"])
}

fn move_schema() -> Value {
    closed(json!({ "san": { "type": "string" }, "side": side_schema(), "at": position_schema() }))
}

fn piece_schema() -> Value {
    closed(json!({
        "piece": string_enum(&["pawn", "knight", "bishop", "rook", "queen", "king"]),
        "square": { "type": "string" },
        "at": position_schema(),
    }))
}

fn number_schema() -> Value {
    closed(json!({
        "value": { "type": "number" },
        "unit": string_enum(&["pawns", "mateIn", "winPercent", "move", "eval", "seconds"]),
    }))
}

fn claim_schema() -> Value {
    let claim = |kind: &str, fields: Value| {
        let mut properties = json!({ "kind": string_enum(&[kind]) });
        for (key, value) in fields.as_object().unwrap() {
            properties[key] = value.clone();
        }
        closed(properties)
    };
    let int = json!({ "type": "integer" });
    json!({ "anyOf": [
        claim("attacks", json!({ "attacker": piece_schema(), "target": piece_schema() })),
        claim("forks", json!({ "attacker": piece_schema(), "targets": { "type": "array", "items": piece_schema() } })),
        claim("pins", json!({ "attacker": piece_schema(), "pinned": piece_schema() })),
        claim("hanging", json!({ "piece": piece_schema() })),
        claim("winsMaterial", json!({ "side": side_schema(), "pawns": int, "line": int })),
        claim("mates", json!({ "side": side_schema(), "mateIn": int, "line": int })),
        claim("better", json!({ "side": side_schema(), "at": position_schema() })),
        claim("threatens", json!({ "side": side_schema(), "line": int })),
    ]})
}

fn sentence_schema() -> Value {
    closed(json!({
        "template": { "type": "string" },
        "slots": closed(json!({
            "m0": nullable(move_schema()),
            "m1": nullable(move_schema()),
            "p0": nullable(piece_schema()),
            "p1": nullable(piece_schema()),
            "s0": nullable(json!({ "type": "string" })),
            "s1": nullable(json!({ "type": "string" })),
            "n0": nullable(number_schema()),
            "n1": nullable(number_schema()),
            "side": nullable(side_schema()),
            "pieces": { "type": "array", "items": piece_schema() },
        })),
        "claims": { "type": "array", "items": claim_schema() },
        "factIds": { "type": "array", "items": { "type": "string" } },
    }))
}

/// The response schema: closed objects with every property required, no recursion and no
/// numeric or length constraints, as structured outputs require.
pub fn narration_schema() -> Value {
    let sentences = json!({ "type": "array", "items": sentence_schema() });
    closed(json!({
        "items": { "type": "array", "items": closed(json!({
            "index": { "type": "integer" },
            "sentences": sentences,
        })) },
        "narrative": nullable(closed(json!({
            "sentences": sentences,
            "takeaways": { "type": "array", "items": closed(json!({
                "indices": { "type": "array", "items": { "type": "integer" } },
                "sentences": sentences,
            })) },
        }))),
    }))
}

/// A fixed one-move game for "Test connection": 1.e4 with one fact, plus a one-fact narrative.
pub fn connection_test_request(config: &NarratorConfig) -> NarrationRequest {
    let e4 = MoveRef {
        san: "e4".into(),
        side: Side::White,
        at: PositionRef::Before,
    };
    let fact = |id: &str, kind, hint: &str| NarrationFact {
        id: id.into(),
        kind,
        vocabulary: FactVocabulary {
            moves: vec![e4.clone()],
            sides: vec![Side::White],
            ..Default::default()
        },
        text_hint: hint.into(),
    };
    let item = NarrationItem {
        index: 1,
        fen_before: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
        move_number: 1,
        san: "e4".into(),
        side: Side::White,
        classification: String::new(),
        kind: CommentaryKind::Opening,
        budget_words: config.budget.move_words,
        facts: vec![fact("line:0", FactKind::Line, "1.e4 was the best move.")],
    };
    NarrationRequest {
        lang: config.language.clone(),
        perspective: Perspective::Both,
        game: NarrationGame {
            facts: vec![fact(
                "game:terminal:0",
                FactKind::Game,
                "The game ended after 1.e4.",
            )],
            result: "*".into(),
            turning_points: vec![],
        },
        items: vec![item],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commentary::provider::{tests::config, ProviderKind};

    /// Every object in the schema is closed and requires all of its properties.
    fn assert_closed(schema: &Value, path: &str, problems: &mut Vec<String>) {
        match schema {
            Value::Object(map) => {
                if map.get("type") == Some(&json!("object")) {
                    let keys: Vec<&String> =
                        map["properties"].as_object().unwrap().keys().collect();
                    let required: Vec<&str> = map["required"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap())
                        .collect();
                    if map.get("additionalProperties") != Some(&json!(false)) {
                        problems.push(format!("{path}: not closed"));
                    }
                    if keys.len() != required.len() {
                        problems.push(format!("{path}: not every property is required"));
                    }
                }
                for banned in ["minLength", "maxLength", "minimum", "maximum", "$ref"] {
                    if map.contains_key(banned) {
                        problems.push(format!("{path}: uses {banned}"));
                    }
                }
                for (key, value) in map {
                    assert_closed(value, &format!("{path}/{key}"), problems);
                }
            }
            Value::Array(values) => {
                for (i, value) in values.iter().enumerate() {
                    assert_closed(value, &format!("{path}/{i}"), problems);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn schema_is_closed_and_unconstrained() {
        let mut problems = vec![];
        assert_closed(&narration_schema(), "", &mut problems);
        assert_eq!(problems, Vec::<String>::new());
    }

    #[test]
    fn a_model_answer_in_the_schema_shape_deserializes() {
        use crate::commentary::narrate::NarratedSentence;
        let sentence: NarratedSentence = serde_json::from_value(json!({
            "template": "{{m0}} allows {{m1}}, and the {{p0}} is lost.",
            "slots": {
                "m0": { "san": "Rc8", "side": "black", "at": { "t": "before" } },
                "m1": { "san": "d4", "side": "white", "at": { "t": "line", "id": 2, "ply": 1 } },
                "p0": { "piece": "knight", "square": "e5", "at": { "t": "after" } },
                "p1": null, "s0": null, "s1": null, "n0": null, "n1": null, "side": null, "pieces": []
            },
            "claims": [{ "kind": "winsMaterial", "side": "white", "pawns": 3, "line": 2 }],
            "factIds": ["line:2"]
        }))
        .unwrap();
        assert_eq!(sentence.claims.len(), 1);
    }

    #[test]
    fn the_user_message_is_a_pure_function_of_the_request() {
        let config = config(ProviderKind::Anthropic, None);
        let request = connection_test_request(&config);
        let content = ChunkContent::Moves(&request.items);
        let first = user_message(&request, &config, &content);
        let second = user_message(
            &request.clone(),
            &config,
            &ChunkContent::Moves(&request.items),
        );
        assert_eq!(first, second);
        // Only request fields reach the message.
        assert!(first.contains("\"draft\":\"1.e4 was the best move.\""));
        assert!(first.contains(&request.items[0].fen_before));
        let narrative = user_message(&request, &config, &ChunkContent::Narrative(&request.game));
        assert!(narrative.contains("game:terminal:0"));
        assert!(!narrative.contains("\"items\":["));
    }
}
