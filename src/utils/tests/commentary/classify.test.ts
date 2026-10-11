import { expect, test } from "vitest";
import type { BestMoves } from "@/bindings";
import { type AnalysisEntry, createTreeStore } from "@/state/store/tree";
import { deriveGlyphs } from "@/utils/commentary/classify";
import { takeMainlineSnapshot } from "@/utils/commentary/snapshot";
import { getAnnotation } from "@/utils/score";
import { treeIteratorMainLine } from "@/utils/treeReducer";

function line(cp: number, san: string, uci: string, multipv = 1): BestMoves {
    return {
        depth: 20,
        multipv,
        nodes: 1,
        nps: 1,
        score: { value: { type: "cp", value: cp }, wdl: null },
        sanMoves: [san],
        uciMoves: [uci],
    };
}

// 1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6?? 4. Qxf7#, with two lines per searched position.
function game() {
    const store = createTreeStore();
    store.getState().makeMoves({ payload: ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"] });
    const fens = Array.from(treeIteratorMainLine(store.getState().root), ({ node }) => node.fen);
    const lines: BestMoves[][] = [
        [line(30, "e4", "e2e4"), line(25, "d4", "d2d4", 2)],
        [line(30, "e5", "e7e5"), line(40, "c5", "c7c5", 2)],
        [line(25, "Nf3", "g1f3"), line(20, "Nc3", "b1c3", 2)],
        [line(-20, "Nc6", "b8c6"), line(10, "g6", "g7g6", 2)],
        [line(-20, "Bc4", "f1c4"), line(-30, "Nf3", "g1f3", 2)],
        [line(-30, "g6", "g7g6"), line(-25, "Qe7", "d8e7", 2)],
        [line(5000, "Qxf7#", "h5f7"), line(-50, "Nf3", "g1f3", 2)],
    ];
    const analysis: (AnalysisEntry & { terminal: boolean })[] = fens.map((fen, i) => ({
        best: lines[i] ?? [],
        novelty: false,
        is_sacrifice: false,
        fen,
        terminal: i === fens.length - 1,
    }));
    return { store, analysis };
}

test("deriveGlyphs equals the annotations addAnalysis writes", () => {
    const { store, analysis } = game();
    const glyphs = deriveGlyphs(
        analysis,
        takeMainlineSnapshot(store.getState().root, store.getState().headers),
    );
    store.getState().addAnalysis(analysis, glyphs);

    const written = Array.from(treeIteratorMainLine(store.getState().root), ({ node }) =>
        node.annotations.length > 0 ? node.annotations[0] : null,
    );
    expect(written).toStrictEqual(glyphs);
    expect(glyphs[6]).toBe("??");
    expect(glyphs[7]).toBeNull();
});

test("deriveGlyphs calls getAnnotation with the previous two positions' lines", () => {
    const { store, analysis } = game();
    const snapshot = takeMainlineSnapshot(store.getState().root, store.getState().headers);
    const glyphs = deriveGlyphs(analysis, snapshot);

    for (let i = 0; i < 7; i++) {
        const expected = getAnnotation(
            i > 1 ? analysis[i - 2].best[0].score.value : null,
            i > 0 ? analysis[i - 1].best[0].score.value : null,
            analysis[i].best[0].score.value,
            snapshot.plies[i].halfMoves % 2 === 1 ? "white" : "black",
            i > 0 ? analysis[i - 1].best : [],
            false,
            snapshot.plies[i].san ?? "",
        );
        expect(glyphs[i]).toBe(expected === "" ? null : expected);
    }
});

test("deriveGlyphs skips positions without lines and game-over positions", () => {
    const { store, analysis } = game();
    analysis[3].best = [];
    const glyphs = deriveGlyphs(
        analysis,
        takeMainlineSnapshot(store.getState().root, store.getState().headers),
    );
    expect(glyphs[3]).toBeNull();
    expect(glyphs[7]).toBeNull();
});
