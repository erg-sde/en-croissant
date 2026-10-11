import type { BestMoves, MoveFacts } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { getAnnotation } from "@/utils/score";
import type { MainlineSnapshot } from "./snapshot";
import type { ExtendedGlyph } from "./types";

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

/** □ needs an only move this decisive, or an alternative that gets mated. */
const ONLY_MOVE_GLYPH_MARGIN = 20;

/** Whether a line's evaluation is a forced mate against `mover`. */
function matedIn(facts: MoveFacts, lineIndex: number | null): boolean {
    if (lineIndex === null) {
        return false;
    }
    const value = facts.lines[lineIndex]?.eval.value;
    if (!value || value.type !== "mate") {
        return false;
    }
    return facts.mover === "white" ? value.value < 0 : value.value > 0;
}

/**
 * Glyphs added to `getAnnotation`'s, never replacing it: □ for a decisive only move, ⊗ for a missed
 * punishment, ⊕ for a mistake made in time trouble. They reuse the reserved NAGs 7, 9 and 138.
 */
export function getExtendedAnnotation(
    facts: MoveFacts,
    glyph: Annotation | null,
    enabled: boolean,
): ExtendedGlyph[] {
    if (!enabled) {
        return [];
    }
    const extended: ExtendedGlyph[] = [];
    const onlyMove = facts.criticality.find((c) => c.kind === "onlyMove");
    if (
        onlyMove?.kind === "onlyMove" &&
        (onlyMove.margin >= ONLY_MOVE_GLYPH_MARGIN || matedIn(facts, facts.second))
    ) {
        extended.push("□");
    }
    if (facts.criticality.some((c) => c.kind === "miss")) {
        extended.push("⊗");
    }
    if (facts.time?.underPressure && (glyph === "?" || glyph === "??")) {
        extended.push("⊕");
    }
    return extended;
}
