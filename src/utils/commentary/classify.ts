import type { BestMoves } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { getAnnotation } from "@/utils/score";
import type { MainlineSnapshot } from "./snapshot";

/** The fields of a `MoveAnalysis` entry the glyph derivation reads. */
export interface GlyphInput {
    best: BestMoves[];
    is_sacrifice: boolean;
    terminal?: boolean;
}

/**
 * The `getAnnotation` glyph of every analysed position, derived once per report so the store, the
 * PGN export and the commentary all read the same value. Entry `i` describes the move that led to
 * `snapshot.plies[i]`; positions the engine did not search (game over, no lines) get `null`.
 */
export function deriveGlyphs(
    analysis: GlyphInput[],
    snapshot: MainlineSnapshot,
): (Annotation | null)[] {
    return analysis.map((entry, i) => {
        const ply = snapshot.plies[i];
        if (!ply || entry.terminal || entry.best.length === 0) {
            return null;
        }
        const prev = i > 0 ? analysis[i - 1].best : [];
        const prevprev = i > 1 ? analysis[i - 2].best : [];
        const annotation = getAnnotation(
            prevprev[0]?.score.value ?? null,
            prev[0]?.score.value ?? null,
            entry.best[0].score.value,
            ply.halfMoves % 2 === 1 ? "white" : "black",
            prev,
            entry.is_sacrifice,
            ply.san ?? "",
        );
        return annotation === "" ? null : annotation;
    });
}
