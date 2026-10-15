import type {
    NarratedGame,
    NarratedMove,
    NarratedSentence,
    NarrationFact,
    NarrationItem,
    NarrationRequest,
    Served,
} from "@/bindings";
import { enumerateGameFacts, type MovesByIndex, moveNumber } from "./gameFacts";
import type { SentenceDraft } from "./narrate";
import type { CommentaryRun, MoveCommentaryResult } from "./report";
import { renderSlots } from "./segmentize";
import type { CommentarySettings } from "./settings";
import type {
    CommentarySentence,
    GameNarrative,
    MoveCommentary,
    Provenance,
    ValidationResult,
} from "./types";
import { mergeNarration, validateNarrative, validateSentences } from "./validate";
import { VERDICT_KINDS } from "./verdict";

/** Everything needed to turn the chunks of one narration run into records. */
export interface NarrationRunContext {
    tab: string;
    runId: string;
    provider: CommentarySettings["provider"];
    run: CommentaryRun;
    /** Move facts by position index. */
    moves: MovesByIndex;
    /** The deterministic result of each index. */
    results: (MoveCommentaryResult | null)[];
    narrative: GameNarrative | null;
}

/** The draft sentences citing `id`, joined: the hint the model rephrases. */
function hint(sentences: CommentarySentence[], id: string): string {
    return sentences
        .filter((s) => s.factIds.includes(id))
        .map((s) => s.text)
        .join(" ");
}

function itemFor(ctx: NarrationRunContext, index: number): NarrationItem | null {
    const result = ctx.results[index];
    const facts = ctx.moves[index];
    if (
        !result?.commentary ||
        !result.selection ||
        !facts ||
        VERDICT_KINDS.includes(result.commentary.kind)
    ) {
        return null;
    }
    const sentences = result.commentary.narration.sentences;
    return {
        index,
        fenBefore: facts.fenBefore,
        moveNumber: moveNumber(index, ctx.run.rootHalfMoves),
        san: facts.played.san,
        side: facts.mover,
        classification: result.selection.classification ?? "",
        kind: result.selection.kind,
        budgetWords: result.selection.budgetWords,
        facts: result.selection.facts.map(
            (f): NarrationFact => ({
                id: f.id,
                kind: f.kind,
                vocabulary: f.vocabulary,
                textHint: hint(sentences, f.id),
            }),
        ),
    };
}

/**
 * The request for a narration run: the moves that have deterministic commentary, with their
 * selected facts, vocabulary and draft sentences, and the game facts with the narrative's drafts.
 * Built only from facts and drafts, never from names, headers or comments.
 */
export function buildNarrationRequest(ctx: NarrationRunContext): NarrationRequest {
    const items = ctx.results
        .map((_, index) => itemFor(ctx, index))
        .filter((item): item is NarrationItem => item !== null);
    const narrative = ctx.narrative;
    const gameFacts: NarrationFact[] = narrative
        ? enumerateGameFacts(narrative.facts, ctx.moves, ctx.run.rootHalfMoves).map((f) => ({
              id: f.id,
              kind: "game",
              vocabulary: f.vocabulary,
              textHint: hint(narrative.narration.sentences, f.id),
          }))
        : [];
    const turning = new Set([
        ...(narrative?.facts.swings.map((s) => s.index) ?? []),
        ...(narrative?.facts.decisive != null ? [narrative.facts.decisive] : []),
    ]);
    return {
        lang: ctx.run.lang,
        perspective: ctx.run.perspective,
        game: {
            facts: gameFacts,
            result: narrative?.facts.result ?? "*",
            turningPoints: items.filter((item) => turning.has(item.index)),
        },
        items,
    };
}

function toDraft(sentence: NarratedSentence, run: CommentaryRun): SentenceDraft {
    const rendered = renderSlots(sentence.template, sentence.slots, run.t, run.lang);
    return {
        template: sentence.template,
        sentence: { text: rendered.text, factIds: sentence.factIds, refs: rendered.refs },
        claims: sentence.claims,
        problems: rendered.problems,
    };
}

function provenance(
    ctx: NarrationRunContext,
    served: Served,
    base: Provenance,
    validation: Provenance["validation"],
): Provenance {
    return {
        ...base,
        source: "llm",
        provider: ctx.provider,
        model: served.model,
        usage: { inputTokens: served.inputTokens, outputTokens: served.outputTokens },
        generatedAt: Date.now(),
        validation,
    };
}

/** The ids cited by the sentences validation dropped. */
function droppedIds(drafts: SentenceDraft[], failed: Set<number>): string[] {
    return drafts.flatMap((d, i) => (failed.has(i) ? d.sentence.factIds : []));
}

/**
 * A narrated move as a record: its sentences are rendered from their slots and validated; those
 * that fail are replaced by the deterministic sentences about the same facts. `null` when nothing
 * survives (the deterministic record stays) or the move was not asked for.
 */
export function narratedMoveRecord(
    ctx: NarrationRunContext,
    narrated: NarratedMove,
    served: Served,
    onValidated?: (result: ValidationResult) => void,
): { index: number; fenBefore: string; san: string; commentary: MoveCommentary } | null {
    const result = ctx.results[narrated.index];
    const facts = ctx.moves[narrated.index];
    const original = result?.commentary;
    if (!original || !result.selection || !facts || facts.fenBefore !== narrated.fenBefore) {
        return null;
    }
    const drafts = narrated.sentences.map((s) => toDraft(s, ctx.run));
    const { narration, result: validation } = validateSentences(
        drafts,
        facts,
        result.selection,
        ctx.run,
    );
    onValidated?.(validation);
    if (narration.sentences.length === 0) {
        return null;
    }
    const failed = new Set(validation.failures.map((f) => f.sentence));
    return {
        index: narrated.index,
        fenBefore: facts.fenBefore,
        san: facts.played.san,
        commentary: {
            ...original,
            narration: mergeNarration(narration, original.narration, droppedIds(drafts, failed)),
            provenance: provenance(ctx, served, original.provenance, validation),
        },
    };
}

/**
 * The narrated game summary, validated like the deterministic one. Takeaways that lose every
 * sentence are dropped; with none left, the deterministic takeaways stay. `null` when no narrative
 * sentence survives.
 */
export function narratedGameRecord(
    ctx: NarrationRunContext,
    narrated: NarratedGame,
    served: Served,
    onValidated?: (result: ValidationResult) => void,
): GameNarrative | null {
    const original = ctx.narrative;
    if (!original) {
        return null;
    }
    const drafts = narrated.sentences.map((s) => toDraft(s, ctx.run));
    const { narration, result: validation } = validateNarrative(
        drafts,
        original.facts,
        ctx.moves,
        ctx.run,
    );
    onValidated?.(validation);
    if (narration.sentences.length === 0) {
        return null;
    }
    const takeaways = narrated.takeaways
        .map((takeaway) => ({
            indices: takeaway.indices,
            narration: validateNarrative(
                takeaway.sentences.map((s) => toDraft(s, ctx.run)),
                original.facts,
                ctx.moves,
                ctx.run,
            ).narration,
        }))
        .filter((t) => t.narration.sentences.length > 0);
    const failed = new Set(validation.failures.map((f) => f.sentence));
    return {
        ...original,
        narration: mergeNarration(narration, original.narration, droppedIds(drafts, failed)),
        takeaways: takeaways.length > 0 ? takeaways : original.takeaways,
        provenance: provenance(ctx, served, original.provenance, validation),
    };
}
