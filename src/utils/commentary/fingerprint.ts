import type { MoveFacts } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import type { GameFacts } from "./gameFacts";
import type { CommentaryDetail } from "./settings";
import { COMMENTARY_SCHEMA_VERSION } from "./types";

// FNV-1a, 32 bits: a short, stable key, not a security boundary.
function fnv1a(text: string): string {
    let hash = 0x811c9dc5;
    for (let i = 0; i < text.length; i++) {
        hash ^= text.charCodeAt(i);
        hash = Math.imul(hash, 0x01000193);
    }
    return (hash >>> 0).toString(16).padStart(8, "0");
}

/**
 * The staleness key of a move's commentary: everything its text depends on. A re-run whose key
 * differs replaces the stored record.
 */
export function commentaryFingerprint(
    facts: MoveFacts,
    glyph: Annotation | null,
    options: { detail: CommentaryDetail; extendedGlyphs: boolean; lang: string },
): string {
    return fnv1a(
        JSON.stringify([
            COMMENTARY_SCHEMA_VERSION,
            facts.schemaVersion,
            facts.fenBefore,
            facts.played.uci,
            glyph,
            facts.eval,
            facts.lines.map((l) => [l.origin, l.eval.value, l.pvUci.slice(0, l.shownPlies)]),
            facts.criticality,
            facts.opening,
            facts.novelty,
            options,
        ]),
    );
}

/** The staleness key of a game narrative. */
export function narrativeFingerprint(
    facts: GameFacts,
    detail: CommentaryDetail,
    lang: string,
): string {
    return fnv1a(JSON.stringify([COMMENTARY_SCHEMA_VERSION, facts, detail, lang]));
}
