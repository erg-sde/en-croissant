import type {
    CommentaryKind,
    EvalValue,
    LineFacts,
    LineRole,
    MoveFacts,
    NarratedGame,
    NarratedMove,
    NarrationFact,
    NarrationItem,
    NarrationLine,
    NarrationRequest,
    Served,
} from "@/bindings";
import { enumerateGameFacts, type MovesByIndex, moveNumber } from "./gameFacts";
import type { CommentaryRun, MoveCommentaryResult } from "./report";
import type { CommentarySettings } from "./settings";
import type {
    CommentarySentence,
    GameNarrative,
    MoveCommentary,
    Provenance,
    ValidationResult,
} from "./types";
import { validateFreeNarrative, validateFreeText } from "./validate";

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

/** Grades that say all there is to say; the coach explains the rest. */
const NOT_NARRATED: CommentaryKind[] = ["book", "best", "excellent", "good"];

/** Words per explained move. */
const COACH_BUDGET = 80;

/** Plies of an engine line shown to the model. */
const MAX_LINE_PLIES = 10;

/** The verified sentences citing `id`, joined: a note for the model. */
function hint(sentences: CommentarySentence[], id: string): string {
    return sentences
        .filter((s) => s.factIds.includes(id))
        .map((s) => s.text)
        .join(" ");
}

/** White's point of view: "+1.2", "-0.4", "#3" (White mates) or "#-3". */
export function formatEval(value: EvalValue): string {
    if (value.type === "mate") return `#${value.value}`;
    const pawns = value.value / 100;
    return `${pawns > 0 ? "+" : ""}${pawns.toFixed(1)}`;
}

/** Half-moves played before a line's first move, for move numbers. */
function lineStart(facts: MoveFacts, line: LineFacts, rootHalfMoves: number): number {
    const before = rootHalfMoves + facts.index - 1;
    return line.origin === "before" ? before : line.origin === "after" ? before + 1 : before + 2;
}

/** A line's moves with numbers, e.g. "3...Nf6 4.Qxf7#". */
function formatLine(sans: string[], startHalfMoves: number): string {
    return sans
        .slice(0, MAX_LINE_PLIES)
        .map((san, i) => {
            const halfMoves = startHalfMoves + i + 1;
            const number = Math.ceil(halfMoves / 2);
            if (halfMoves % 2 === 1) return `${number}.${san}`;
            return i === 0 ? `${number}...${san}` : san;
        })
        .join(" ");
}

function lineRole(facts: MoveFacts, line: LineFacts): LineRole | null {
    if (line.id === facts.refutation) return "reply";
    if (line.id === facts.threat) return "threat";
    if (line.origin !== "before") return null;
    if (line.pvUci[0] === facts.played.uci) return "played";
    if (line.id === facts.best || (facts.best === null && line.multipv === 1)) return "best";
    if (line.id === facts.second) return "second";
    return null;
}

function linesFor(facts: MoveFacts, rootHalfMoves: number): NarrationLine[] {
    return facts.lines.flatMap((line) => {
        const role = lineRole(facts, line);
        return role
            ? [
                  {
                      role,
                      moves: formatLine(line.pvSan, lineStart(facts, line, rootHalfMoves)),
                      eval: formatEval(line.eval.value),
                  },
              ]
            : [];
    });
}

function itemFor(ctx: NarrationRunContext, index: number): NarrationItem | null {
    const result = ctx.results[index];
    const facts = ctx.moves[index];
    const commentary = result?.commentary;
    if (!commentary || !facts || NOT_NARRATED.includes(commentary.kind)) {
        return null;
    }
    const sentences = commentary.narration.sentences;
    const selected = result.selection?.facts ?? [];
    return {
        index,
        fenBefore: facts.fenBefore,
        fenAfter: facts.fenAfter,
        moveNumber: moveNumber(index, ctx.run.rootHalfMoves),
        san: facts.played.san,
        side: facts.mover,
        classification: result.selection?.classification ?? commentary.kind,
        kind: commentary.kind,
        evalBefore: facts.eval.before ? formatEval(facts.eval.before.value) : null,
        evalAfter: formatEval(facts.eval.after.value),
        lines: linesFor(facts, ctx.run.rootHalfMoves),
        budgetWords: COACH_BUDGET,
        facts: selected.map(
            (f): NarrationFact => ({ id: f.id, kind: f.kind, textHint: hint(sentences, f.id) }),
        ),
    };
}

/**
 * The request for a narration run: every move worth explaining (mistakes and key moments, not the
 * moves graded book, best, excellent or good), with both positions, the evaluations, the engine
 * lines and the verified notes, and the game facts for the narrative. Built only from facts and
 * engine output, never from names, headers or comments.
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
            // The key moments, else every explained move, so the lessons have material.
            turningPoints: items.some((item) => turning.has(item.index))
                ? items.filter((item) => turning.has(item.index) || item.classification === "??")
                : items,
        },
        items,
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

/**
 * A coached move as a record: its sentences are checked (every move named must exist, every piece
 * named must stand on its square) and the ones that fail are dropped. `null` when nothing survives
 * (the deterministic record stays) or the move was not asked for.
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
    if (!original || !facts || facts.fenBefore !== narrated.fenBefore) {
        return null;
    }
    const { narration, result: validation } = validateFreeText(
        narrated.sentences,
        facts,
        COACH_BUDGET,
        ctx.run,
    );
    onValidated?.(validation);
    if (narration.sentences.length === 0) {
        return null;
    }
    return {
        index: narrated.index,
        fenBefore: facts.fenBefore,
        san: facts.played.san,
        commentary: {
            ...original,
            narration,
            provenance: provenance(ctx, served, original.provenance, validation),
        },
    };
}

/**
 * The coached game summary and lessons, checked like the moves. Lessons that lose every sentence
 * are dropped; with none left, the deterministic takeaways stay. `null` when no story sentence
 * survives.
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
    const { narration, result: validation } = validateFreeNarrative(
        narrated.sentences,
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
            narration: validateFreeNarrative(takeaway.sentences, ctx.moves, ctx.run).narration,
        }))
        .filter((t) => t.narration.sentences.length > 0);
    return {
        ...original,
        narration,
        takeaways: takeaways.length > 0 ? takeaways : original.takeaways,
        provenance: provenance(ctx, served, original.provenance, validation),
    };
}
