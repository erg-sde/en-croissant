import type { MoveFacts, Outcome, Perspective } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { getExtendedAnnotation } from "./classify";
import { commentaryFingerprint, narrativeFingerprint } from "./fingerprint";
import { gameFacts, type MovesByIndex } from "./gameFacts";
import { arrowsFor, type NarrationContext, narrateGame, narrateMove } from "./narrate";
import { enumerateFacts, selectFacts } from "./salience";
import type { CommentarySettings } from "./settings";
import {
    COMMENTARY_SCHEMA_VERSION,
    type ExtendedGlyph,
    type FactSelection,
    type GameNarrative,
    type MoveCommentary,
} from "./types";
import { validateNarrative, validateSentences } from "./validate";
import { narrateVerdict, verdictKind } from "./verdict";

export interface CommentaryRun extends NarrationContext {
    settings: Pick<CommentarySettings, "detail" | "extendedGlyphs">;
    perspective: Perspective;
    appVersion: string;
    /** Piece names of `lang`, for the validator. */
    pieceWords: string[];
    now: number;
    /** Index of the last position the game follows a named opening through; `null` for none. */
    bookUntil?: number | null;
}

export interface MoveCommentaryResult {
    extended: ExtendedGlyph[];
    commentary: MoveCommentary | null;
    /** What the commentary is about, also used by the game narrative's takeaways. */
    selection: FactSelection | null;
}

/**
 * The deterministic commentary of one move: extended glyphs, then selection, narration and
 * validation. `commentary` is `null` when the move gets none at this detail level or when no
 * sentence survives validation.
 */
export function buildMoveCommentary(
    facts: MoveFacts,
    glyph: Annotation | null,
    run: CommentaryRun,
): MoveCommentaryResult {
    const extended = getExtendedAnnotation(facts, glyph, run.settings.extendedGlyphs);
    const selection = selectFacts(facts, glyph, extended, run.settings.detail, run);
    if (!selection) {
        return { extended, commentary: verdictCommentary(facts, glyph, extended, run), selection };
    }
    const drafts = narrateMove(selection, facts, run);
    const { narration, result } = validateSentences(drafts, facts, selection, run);
    if (narration.sentences.length === 0) {
        return { extended, commentary: verdictCommentary(facts, glyph, extended, run), selection };
    }
    return {
        extended,
        selection,
        commentary: {
            schemaVersion: COMMENTARY_SCHEMA_VERSION,
            kind: selection.kind,
            perspective: run.perspective,
            narration,
            arrows: arrowsFor(selection, facts),
            fingerprint: commentaryFingerprint(facts, glyph, {
                detail: run.settings.detail,
                extendedGlyphs: run.settings.extendedGlyphs,
                lang: run.lang,
            }),
            provenance: {
                source: "deterministic",
                generatedAt: run.now,
                appVersion: run.appVersion,
                factsSchemaVersion: facts.schemaVersion,
                validation: result,
            },
        },
    };
}

/**
 * The one-line verdict of a move without fuller commentary (see verdict.ts), validated like any
 * other sentence. `null` for a move that cannot be judged.
 */
function verdictCommentary(
    facts: MoveFacts,
    glyph: Annotation | null,
    extended: ExtendedGlyph[],
    run: CommentaryRun,
): MoveCommentary | null {
    const kind = verdictKind(facts, glyph, run.bookUntil ?? null);
    if (kind === null) return null;
    const { drafts, factIds } = narrateVerdict(kind, facts, run);
    const cited = enumerateFacts(facts, run).filter((f) => factIds.includes(f.id));
    const selection: FactSelection = {
        index: facts.index,
        classification: glyph,
        extended,
        kind,
        facts: cited.map(({ id, kind, weight, vocabulary }) => ({ id, kind, weight, vocabulary })),
        budgetWords: VERDICT_BUDGET,
    };
    const { narration, result } = validateSentences(drafts, facts, selection, run);
    if (narration.sentences.length === 0) return null;
    return {
        schemaVersion: COMMENTARY_SCHEMA_VERSION,
        kind,
        perspective: run.perspective,
        narration,
        arrows: [],
        fingerprint: commentaryFingerprint(facts, glyph, {
            detail: run.settings.detail,
            extendedGlyphs: run.settings.extendedGlyphs,
            lang: run.lang,
        }),
        provenance: {
            source: "deterministic",
            generatedAt: run.now,
            appVersion: run.appVersion,
            factsSchemaVersion: facts.schemaVersion,
            validation: result,
        },
    };
}

/** Words a verdict may have. */
const VERDICT_BUDGET = 20;

/**
 * The deterministic game narrative from the moves' facts and selections. `null` when no sentence
 * survives validation.
 */
export function buildGameNarrative(
    moves: MovesByIndex,
    fens: string[],
    selections: (FactSelection | null)[],
    result: Outcome,
    run: CommentaryRun,
): GameNarrative | null {
    const facts = gameFacts(moves, fens, selections, result);
    const { drafts, takeaways } = narrateGame(facts, moves, selections, run);
    const { narration, result: validation } = validateNarrative(drafts, facts, moves, run);
    if (narration.sentences.length === 0) {
        return null;
    }
    return {
        schemaVersion: COMMENTARY_SCHEMA_VERSION,
        facts,
        narration,
        takeaways: takeaways
            .map((t) => ({
                indices: t.indices,
                narration: validateNarrative(t.drafts, facts, moves, run).narration,
            }))
            .filter((t) => t.narration.sentences.length > 0),
        fingerprint: narrativeFingerprint(facts, run.settings.detail, run.lang),
        provenance: {
            source: "deterministic",
            generatedAt: run.now,
            appVersion: run.appVersion,
            factsSchemaVersion: moves.find((m) => m !== null)?.schemaVersion ?? 0,
            validation,
        },
    };
}
