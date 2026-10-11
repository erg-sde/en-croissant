import type { MoveFacts, Perspective } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { getExtendedAnnotation } from "./classify";
import { commentaryFingerprint } from "./fingerprint";
import { arrowsFor, type NarrationContext, narrateMove } from "./narrate";
import { selectFacts } from "./salience";
import type { CommentarySettings } from "./settings";
import { COMMENTARY_SCHEMA_VERSION, type ExtendedGlyph, type MoveCommentary } from "./types";
import { validateSentences } from "./validate";

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
        return { extended, commentary: null };
    }
    const drafts = narrateMove(selection, facts, run);
    const { narration, result } = validateSentences(drafts, facts, selection, run);
    if (narration.sentences.length === 0) {
        return { extended, commentary: null };
    }
    return {
        extended,
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
