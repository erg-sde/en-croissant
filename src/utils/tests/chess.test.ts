import { expect, test } from "vitest";
import { Chess } from "chessops/chess";
import { makeFen } from "chessops/fen";
import { parseSan } from "chessops/san";
import { ANNOTATION_INFO, type Annotation, NAG_INFO } from "../annotation";
import { getGameStats, hasMorePriority } from "../chess";
import { createNode, defaultTree, type TreeNode } from "../treeReducer";

test("NAGs are consistent", () => {
    for (const k of Object.keys(ANNOTATION_INFO)) {
        if (k === "") continue;
        const nag = ANNOTATION_INFO[k as Annotation].nag!;
        expect(NAG_INFO.get(`$${nag}`)).toBe(k);
    }
});

test("priority comparison", () => {
    expect(hasMorePriority([0, 0], [0])).toBe(false);
    expect(hasMorePriority([0], [0, 0])).toBe(true);
    expect(hasMorePriority([0], [1])).toBe(true);
    expect(hasMorePriority([1], [0])).toBe(false);
    expect(hasMorePriority([0, 0], [0, 1])).toBe(true);
    expect(hasMorePriority([0, 1], [0, 0])).toBe(false);
    expect(hasMorePriority([0, 1], [0, 2])).toBe(true);
    expect(hasMorePriority([0, 2], [0, 1])).toBe(false);
});

function mainline(moves: [string, number | null][]): TreeNode {
    const root = defaultTree().root;
    const pos = Chess.default();
    let node = root;
    moves.forEach(([san, cp], i) => {
        const move = parseSan(pos, san)!;
        pos.play(move);
        const child = createNode({ fen: makeFen(pos.toSetup()), move, san, halfMoves: i + 1 });
        child.score = cp === null ? null : { value: { type: "cp", value: cp }, wdl: null };
        node.children.push(child);
        node = child;
    });
    return root;
}

test("game stats measure losses between consecutive scored plies only", () => {
    const stats = getGameStats(
        mainline([
            ["e4", 30],
            ["e5", null],
            ["Nf3", -200],
            ["Nc6", -150],
        ]),
    );
    // Nf3 follows an unscored ply, so its 230 cp drop from e4 is not charged to White.
    expect(stats.whiteCPL).toBe(0);
    expect(stats.blackCPL).toBe(50);
});
