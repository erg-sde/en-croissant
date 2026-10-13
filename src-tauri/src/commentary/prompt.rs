//! The narration prompt. The system prompt is a constant and the user message is a pure function of
//! the `NarrationRequest`, so nothing a user typed into a game (names, headers, comments) can reach
//! a provider: the request type has no field for it.

use serde_json::{json, Value};

use super::{
    facts::{CommentaryKind, FactKind, FactVocabulary, MoveRef, Perspective, PositionRef, Side},
    narrate::{NarrationFact, NarrationGame, NarrationItem, NarrationRequest},
    provider::NarratorConfig,
    wire::{cited_id, refs_of},
};

pub const SYSTEM_PROMPT: &str = r#"You turn verified chess facts into short coaching sentences for a game report. Each fact comes with a draft sentence that is already correct. Rephrase and connect the drafts into natural prose; never add information.

Rules:
1. Nothing exists outside the facts you are given. Use only the moves, pieces, squares, numbers and sides listed in each fact's vocabulary.
2. Never re-judge a move. The classification and kind are final.
3. Never pad. If the facts say little, write little. Stay within each item's word budget.
4. Never claim what a player intended or felt, and never mention an engine, a computer or evaluations in centipawns.
5. Never invent a number.

How to write a sentence:
- Write a template. Every move, piece, square and number comes from a slot, written as a placeholder: {{m0}} {{m1}} are slots.moves[0] and [1], {{p0}} {{p1}} are slots.pieces[0] and [1] (rendered as "the knight on f3"), {{s0}} {{s1}} are slots.squares[0] and [1], {{n0}} {{n1}} are slots.numbers[0] and [1], {{side}} is slots.side ("White"/"Black"), and {{pieces}} is the list slots.pieceList. Never write a square, a move, a piece name or a digit as literal text.
- Slots hold refs, never values. Each fact lists its refs, for example "line:2/moves/1" for the second move of fact line:2, with the value it stands for. Use only refs of facts the sentence cites, put moves refs in "moves", pieces refs in "pieces" and "pieceList", and so on, and use every slot you fill. Leave unused arrays empty and set "side" to "none" when unused.
- Every relation you state must be declared as a claim: attacks, forks, pins, hanging (is undefended and can be taken), winsMaterial, mates, better, threatens. A sentence with no relation has no claims. Each claim has every field; the kind decides which count:
  - attacks: pieces = [attacker ref, target ref]; forks: pieces = [attacker ref, target refs...]; pins: pieces = [attacker ref, pinned ref]; hanging: pieces = [the piece's ref].
  - winsMaterial: side wins "number" pawns along line "line"; mates: side mates in "number" moves along line "line"; threatens: side threatens line "line"; better: side is better at "at", which is "before", "after" or "line:<id>:<ply>".
  - "side" is always the side doing it. Fields a kind does not use: pieces empty, number 0, line 0, at "". Line numbers are the N of "line:N" fact ids.
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

/// A fact as the model sees it, under the id sentences cite it with, with its refs.
fn fact_json(id: &str, fact: &NarrationFact) -> Value {
    json!({
        "id": id,
        "kind": fact.kind,
        "draft": fact.text_hint,
        "refs": refs_of(id, &fact.vocabulary),
        "sides": fact.vocabulary.sides,
    })
}

/// A move; `cite` gives the id each of its facts is cited with.
fn item_json(
    config: &NarratorConfig,
    item: &NarrationItem,
    cite: impl Fn(&NarrationFact) -> String,
) -> Value {
    json!({
        "index": item.index,
        "moveNumber": item.move_number,
        "san": item.san,
        "side": item.side,
        "fenBefore": item.fen_before,
        "classification": item.classification,
        "kind": item.kind,
        "wordBudget": word_limit(config, item),
        "facts": item.facts.iter().map(|f| fact_json(&cite(f), f)).collect::<Vec<_>>(),
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
            "task": "Write the sentences for each move in \"items\". Return one entry per item, with its index.",
            "items": items
                .iter()
                .map(|i| item_json(config, i, |f| f.id.clone()))
                .collect::<Vec<_>>(),
        }),
        ChunkContent::Narrative(game) => json!({
            "task": "Write the game narrative and its takeaways.",
            "wordBudget": config.budget.narrative_words,
            "keyMomentWordBudget": config.budget.key_moment_words,
            "result": game.result,
            "gameFacts": game.facts.iter().map(|f| fact_json(&f.id, f)).collect::<Vec<_>>(),
            "turningPoints": game
                .turning_points
                .iter()
                .map(|i| item_json(config, i, |f| cited_id(i, f)))
                .collect::<Vec<_>>(),
        }),
    };
    json!({
        "lang": request.lang,
        "perspective": request.perspective,
        "request": task,
    })
    .to_string()
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
