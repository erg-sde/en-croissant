import type { MoveFacts, Outcome, Perspective } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { getExtendedAnnotation } from "./classify";
import { commentaryFingerprint, narrativeFingerprint } from "./fingerprint";
import { gameFacts, type MovesByIndex } from "./gameFacts";
import { arrowsFor, type NarrationContext, narrateGame, narrateMove } from "./narrate";
import { selectFacts } from "./salience";
import type { CommentarySettings } from "./settings";
import {
    COMMENTARY_SCHEMA_VERSION,
    type ExtendedGlyph,
    type FactSelection,
    type GameNarrative,
    type MoveCommentary,
} from "./types";
import { validateNarrative, validateSentences } from "./validate";

export interface CommentaryRun extends NarrationContext {
    settings: Pick<CommentarySettings, "detail" | "extendedGlyphs">;
    perspective: Perspective;
    appVersion: string;
    /** Piece names of `lang`, for the validator. */
    pieceWords: string[];
    now: number;
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
        return { extended, commentary: null, selection: null };
    }
    const drafts = narrateMove(selection, facts, run);
    const { narration, result } = validateSentences(drafts, facts, selection, run);
    if (narration.sentences.length === 0) {
        return { extended, commentary: null, selection };
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
