import { makeFen } from "chessops/fen";
import { positionFromFen } from "@/utils/chessops";
import { parseTimeControl } from "@/utils/clock";
import { type GameHeaders, type TreeNode, treeIteratorMainLine } from "@/utils/treeReducer";

/** One mainline position as it was when a report started; index 0 is the root. */
export interface MainlinePly {
    fen: string;
    san: string | null;
    halfMoves: number;
    clock?: number;
}

/** The mainline a report run was started on, captured when the user submits the report. */
export interface MainlineSnapshot {
    rootFen: string;
    runId: string;
    plies: MainlinePly[];
    /** Increment and initial time in seconds, from the first time control field. */
    incrementS: number | null;
    initialS: number | null;
}

export function takeMainlineSnapshot(root: TreeNode, headers: GameHeaders): MainlineSnapshot {
    const plies = Array.from(treeIteratorMainLine(root), ({ node }) => ({
        fen: node.fen,
        san: node.san,
        halfMoves: node.halfMoves,
        clock: node.clock,
    }));
    const [first] = headers.time_control ? parseTimeControl(headers.time_control) : [];
    return {
        rootFen: root.fen,
        runId: crypto.randomUUID(),
        plies,
        incrementS: first ? (first.increment ?? 0) / 1000 : null,
        initialS: first ? first.seconds / 1000 : null,
    };
}

function canonicalFen(fen: string): string | null {
    const [pos] = positionFromFen(fen);
    return pos ? makeFen(pos.toSetup()) : null;
}

/**
 * Whether two FENs describe the same position, compared in chessops' canonical form: missing
 * counters default the same way on both sides and an en passant square counts only when a capture
 * is legal. The tree keeps the user's root FEN as written while the engine reports a normalized one,
 * so string equality is not enough.
 */
export function samePosition(a: string, b: string): boolean {
    const canonical = canonicalFen(a);
    return canonical !== null && canonical === canonicalFen(b);
}
