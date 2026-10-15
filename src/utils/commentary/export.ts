import type { GameHeaders, ReportState, TreeNode } from "@/utils/treeReducer";
import { toPgnSafe } from "./pgnSafe";
import type { PgnSafeText, Provenance, ValidatedNarration } from "./types";
import { restoreCommentary, restoreNarrative } from "./validate";

/** The generated text to write after a node's comment in exported PGN. */
export type CommentaryText = (node: TreeNode) => PgnSafeText | null;

/** The name written in the `[Annotator]` tag of a game with exported commentary. */
const ANNOTATOR = "En Croissant";

function text(narration: ValidatedNarration): string {
    return narration.sentences.map((s) => s.text).join(" ");
}

/** "Commentary by En Croissant 0.15.1, deterministic[, model]." */
function provenanceSentence(provenance: Provenance): string {
    const parts = [`${ANNOTATOR} ${provenance.appVersion}`, provenance.source];
    if (provenance.model) parts.push(provenance.model);
    return `Commentary by ${parts.join(", ")}.`;
}

/**
 * The commentary to export for each node of `root`'s tree: each move's validated sentences, and at
 * the root the game narrative preceded by one provenance sentence. Exported as ordinary comment
 * text; it is not read back as commentary.
 */
export function commentaryForExport(root: TreeNode, report: ReportState): CommentaryText {
    return (node) => {
        if (node === root) {
            const narrative = restoreNarrative(report.narrative);
            if (!narrative || narrative.narration.sentences.length === 0) return null;
            return toPgnSafe(
                `${provenanceSentence(narrative.provenance)} ${text(narrative.narration)}`,
            );
        }
        const commentary = restoreCommentary(node.commentary);
        return commentary && commentary.narration.sentences.length > 0
            ? toPgnSafe(text(commentary.narration))
            : null;
    };
}

function hasCommentary(node: TreeNode): boolean {
    return node.commentary !== undefined || node.children.some(hasCommentary);
}

/** `headers` with `[Annotator]` set when anything generated will be exported. */
export function withAnnotator(
    headers: GameHeaders,
    root: TreeNode,
    report: ReportState,
): GameHeaders {
    if (!report.narrative && !hasCommentary(root)) return headers;
    return { ...headers, other: { ...headers.other, Annotator: ANNOTATOR } };
}
