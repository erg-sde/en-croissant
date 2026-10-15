//! The narration prompt. The system prompt is a constant and the user message is a pure function of
//! the `NarrationRequest`, so nothing a user typed into a game (names, headers, comments) can reach
//! a provider: the request type has no field for it.

use serde_json::{json, Value};

use super::{
    facts::{CommentaryKind, FactKind, Perspective, Side},
    narrate::{
        LineRole, NarrationFact, NarrationGame, NarrationItem, NarrationLine, NarrationRequest,
    },
    provider::NarratorConfig,
};

pub const SYSTEM_PROMPT: &str = r##"You are a chess coach reviewing a game for the player. For each move you get the position before and after it (FEN), the move played, the engine's evaluation before and after it (from White's point of view, in pawns; "#3" is mate in 3 for White, "#-3" for Black), the engine's lines, the move's grade, and verified notes about tactics found in those lines.

The engine lines are:
- best: the engine's best move in the position before, and how play continues.
- second: its second choice there.
- played: the line that starts with the move that was played, when the engine considered it.
- reply: the opponent's best answer after the move played.
- threat: what the move played threatens if the opponent ignored it.

For each move, explain in plain, natural sentences:
1. Why the move is good or bad: the concrete idea. What does it attack, defend, allow, win or give up? Use the reply and threat lines to show the consequence, and add the positional reason when it matters: development, the center, king safety, pawn structure, piece activity, open files, weak squares.
2. For a mistake or an inaccuracy, what to play instead and the idea or plan behind it, from the best line.
3. For a good move, what it achieves or prevents.

Accuracy rules, which are checked:
- Name moves in standard algebraic notation (Nf3, exd5, O-O, e8=Q) and only moves that appear in the lines given, the move played, or legal moves in the positions shown. Never invent a variation.
- Write "the knight on f3" style only when that piece stands on that square in the position you are talking about.
- Do not contradict the evaluation or the grade. Do not quote evaluation numbers or mention an engine, a computer or centipawns; say "winning", "a clear advantage", "roughly equal" and so on instead.
- No markdown, no lists, no brackets or braces.

Style: speak like a strong, friendly coach. Be specific, not generic: "this leaves the e5 pawn without a defender" beats "this is a weak move". 2 to 4 sentences per move, within its word budget. Address the player as "you" only when "perspective" is "white" or "black" and the move is theirs; otherwise say White and Black. Write in the language given by "lang".

For the game narrative: a short story of the game (the opening, the turning points, how it was decided), then 2 to 4 lessons the player can take into their next games, each tied to the move indices it rests on. Lessons are about habits and patterns ("check what your opponent's last move attacks before replying"), not a retelling of the game."##;

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
    json!({ "kind": fact.kind, "note": fact.text_hint })
}

fn item_json(config: &NarratorConfig, item: &NarrationItem) -> Value {
    json!({
        "index": item.index,
        "moveNumber": item.move_number,
        "side": item.side,
        "played": item.san,
        "grade": item.classification,
        "kind": item.kind,
        "fenBefore": item.fen_before,
        "fenAfter": item.fen_after,
        "evalBefore": item.eval_before,
        "evalAfter": item.eval_after,
        "lines": item.lines,
        "notes": item.facts.iter().map(fact_json).collect::<Vec<_>>(),
        "wordBudget": word_limit(config, item),
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
            "task": "Explain each move in \"items\". Return one entry per item, with its index.",
            "items": items.iter().map(|i| item_json(config, i)).collect::<Vec<_>>(),
        }),
        ChunkContent::Narrative(game) => json!({
            "task": "Write the story of the game and the lessons for the player.",
            "wordBudget": config.budget.narrative_words,
            "result": game.result,
            "gameNotes": game.facts.iter().map(fact_json).collect::<Vec<_>>(),
            "keyMoments": game
                .turning_points
                .iter()
                .map(|i| item_json(config, i))
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

/// A fixed one-move game for "Test connection": 1.e4, plus a one-note narrative.
pub fn connection_test_request(config: &NarratorConfig) -> NarrationRequest {
    let fact = |id: &str, kind, hint: &str| NarrationFact {
        id: id.into(),
        kind,
        text_hint: hint.into(),
    };
    let item = NarrationItem {
        index: 1,
        fen_before: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
        fen_after: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1".into(),
        move_number: 1,
        san: "e4".into(),
        side: Side::White,
        classification: "best".into(),
        kind: CommentaryKind::Best,
        eval_before: Some("+0.2".into()),
        eval_after: "+0.3".into(),
        lines: vec![NarrationLine {
            role: LineRole::Best,
            moves: "1.e4 e5 2.Nf3".into(),
            eval: "+0.3".into(),
        }],
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
        assert!(first.contains("\"note\":\"1.e4 was the best move.\""));
        assert!(first.contains(&request.items[0].fen_after));
        assert!(first.contains("\"role\":\"best\""));
        let narrative = user_message(&request, &config, &ChunkContent::Narrative(&request.game));
        assert!(narrative.contains("The game ended after 1.e4."));
        assert!(!narrative.contains("\"items\":["));
    }
}
