import type { CommentaryKind, MoveFacts } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import type { NarrationContext, SentenceDraft } from "./narrate";
import { enumerateFacts } from "./salience";
import { segmentize, type Token } from "./segmentize";

/**
 * The one-line verdict every analysed move gets when it has no fuller commentary, so stepping
 * through a game always says something: book, best, excellent, good, or the mistake grades. These
 * kinds are deterministic only; narration skips them.
 */
export const VERDICT_KINDS: CommentaryKind[] = [
    "book",
    "best",
    "excellent",
    "good",
    "inaccuracy",
    "error",
    "blunder",
];

/** Win chance a move may lose (percentage points) and still count as excellent or good. */
const EXCELLENT_MAX_LOSS = 2;
const GOOD_MAX_LOSS = 5;

/**
 * The verdict of a move: its glyph's grade, else book while the game follows a named opening, else
 * by how much win chance it gave away. `null` for a move without an evaluation to judge it by.
 */
export function verdictKind(
    facts: MoveFacts,
    glyph: Annotation | null,
    bookUntil: number | null,
): CommentaryKind | null {
    if (facts.terminal) return null;
    if (glyph === "??") return "blunder";
    if (glyph === "?") return "error";
    if (glyph === "?!") return "inaccuracy";
    if (bookUntil !== null && facts.index <= bookUntil) return "book";
    if (facts.played.rank === 1) return "best";
    const loss = facts.eval.winDelta;
    if (loss === null) return null;
    if (loss <= EXCELLENT_MAX_LOSS) return "excellent";
    if (loss <= GOOD_MAX_LOSS) return "good";
    return null;
}

const KEYS: Partial<Record<CommentaryKind, string>> = {
    book: "Commentary.Verdict.Book",
    best: "Commentary.Verdict.Best",
    excellent: "Commentary.Verdict.Excellent",
    good: "Commentary.Verdict.Good",
    inaccuracy: "Commentary.Verdict.Inaccuracy",
    error: "Commentary.Verdict.Error",
    blunder: "Commentary.Verdict.Blunder",
};

/**
 * The verdict sentence, citing the evaluation change, and for a move that was not the engine's
 * choice, the move it preferred, citing that line.
 */
export function narrateVerdict(
    kind: CommentaryKind,
    facts: MoveFacts,
    ctx: NarrationContext,
): { drafts: SentenceDraft[]; factIds: string[] } {
    const enumerated = enumerateFacts(facts, ctx);
    const evalFact = enumerated.find((f) => f.kind === "evalDelta");
    if (!evalFact) return { drafts: [], factIds: [] };
    const san: Token = {
        kind: "move",
        ref: { san: facts.played.san, side: facts.mover, at: { t: "before" } },
    };
    const sentence = (key: string, values: Record<string, Token>, factIds: string[]) => {
        const { template, text, refs } = segmentize([key], values, ctx.t, ctx.lang);
        return { template, sentence: { text, factIds, refs } };
    };
    const drafts = [sentence(KEYS[kind]!, { san }, [evalFact.id])];
    const factIds = [evalFact.id];

    const bestFact =
        kind !== "book" && kind !== "best" && facts.best !== null
            ? enumerated.find((f) => f.id === `line:${facts.best}`)
            : undefined;
    const best = bestFact?.vocabulary.moves[0];
    if (bestFact && best && best.san !== facts.played.san) {
        drafts.push(
            sentence("Commentary.Verdict.BestWas", { best: { kind: "move", ref: best } }, [
                bestFact.id,
            ]),
        );
        factIds.push(bestFact.id);
    }
    return { drafts, factIds };
}
