import { parseUci } from "chessops";
import { beforeEach, describe, expect, test } from "vitest";
import { type AnalysisEntry, createTreeStore } from "@/state/store/tree";
import { deriveGlyphs } from "@/utils/commentary/classify";
import { buildMoveCommentary } from "@/utils/commentary/report";
import { takeMainlineSnapshot } from "@/utils/commentary/snapshot";
import { defaultTree, type TreeState, treeIteratorMainLine } from "@/utils/treeReducer";
import { facts as commentaryFacts, run as commentaryRun } from "./commentary/helpers";

const store = createTreeStore();

const snapshot = () => takeMainlineSnapshot(store.getState().root, store.getState().headers);

beforeEach(() => {
    store.setState(defaultTree());
});

const e4 = parseUci("e2e4")!;
const d5 = parseUci("d7d5")!;
const treeE4D5: () => TreeState = () => ({
    ...defaultTree(),
    position: [0, 0],
    root: {
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        move: null,
        san: null,
        children: [
            {
                fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
                move: e4,
                san: "e4",
                children: [
                    {
                        fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
                        move: d5,
                        san: "d5",
                        clock: undefined,
                        children: [],
                        score: null,
                        depth: null,
                        halfMoves: 2,
                        shapes: [],
                        annotations: [],
                        comment: "",
                    },
                ],
                clock: undefined,
                score: null,
                depth: null,
                halfMoves: 1,
                shapes: [],
                annotations: [],
                comment: "",
            },
        ],
        score: null,
        depth: null,
        halfMoves: 0,
        shapes: [],
        annotations: [],
        comment: "",
    },
    report: {
        inProgress: false,
    },
});

const treeE4D5Nf3: () => TreeState = () => ({
    ...defaultTree(),
    position: [0, 0],
    root: {
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        move: null,
        san: null,
        children: [
            {
                fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
                move: e4,
                san: "e4",
                children: [
                    {
                        fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
                        move: d5,
                        san: "d5",
                        clock: undefined,
                        children: [],
                        score: null,
                        depth: null,
                        halfMoves: 2,
                        shapes: [],
                        annotations: [],
                        comment: "",
                    },
                ],
                clock: undefined,
                score: null,
                depth: null,
                halfMoves: 1,
                shapes: [],
                annotations: [],
                comment: "",
            },
            {
                fen: "rnbqkbnr/pppppppp/8/8/8/5N2/PPPPPPPP/RNBQKB1R b KQkq - 1 1",
                move: parseUci("g1f3")!,
                san: "Nf3",
                children: [],
                clock: undefined,
                score: null,
                depth: null,
                halfMoves: 1,
                shapes: [],
                annotations: [],
                comment: "",
            },
        ],
        score: null,
        depth: null,
        halfMoves: 0,
        shapes: [],
        annotations: [],
        comment: "",
    },
    report: {
        inProgress: false,
    },
});

const getNewState = () => {
    const s = store.getState();
    return {
        root: s.root,
        position: s.position,
        headers: s.headers,
        dirty: s.dirty,
        report: {
            inProgress: false,
        },
    };
};

test("should handle save", () => {
    store.setState({ dirty: true });
    store.getState().save();

    expect(getNewState()).toStrictEqual({ ...defaultTree(), dirty: false });
});

test("should handle setState", () => {
    store.getState().setState(treeE4D5());
    expect(getNewState()).toStrictEqual(treeE4D5());
});

test("should handle reset", () => {
    store.setState(treeE4D5());
    store.getState().reset();
    expect(getNewState()).toStrictEqual(defaultTree());
});

test("should handle setHeaders", () => {
    store.getState().setHeaders({
        ...defaultTree().headers,
        orientation: "black",
        start: [1],
    });

    expect(getNewState()).toStrictEqual({
        ...defaultTree(),
        dirty: true,
        headers: {
            ...defaultTree().headers,
            orientation: "black",
            start: [1],
        },
    });
});

test("should handle setStart", () => {
    store.getState().setStart([1]);

    expect(getNewState()).toStrictEqual({
        ...defaultTree(),
        dirty: true,
        headers: { ...defaultTree().headers, start: [1] },
    });
});

test("should handle makeMove", () => {
    store.getState().makeMove({ payload: e4 });

    expect(getNewState()).toStrictEqual({
        ...defaultTree(),
        dirty: true,
        position: [0],
        root: {
            ...defaultTree().root,
            children: [
                {
                    fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
                    move: e4,
                    san: "e4",
                    children: [],
                    score: null,
                    clock: undefined,
                    depth: null,
                    halfMoves: 1,
                    shapes: [],
                    annotations: [],
                    comment: "",
                },
            ],
        },
    });
});

test("should handle makeMoves", () => {
    store.getState().makeMoves({ payload: ["e4", "d5"] });

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        dirty: true,
        position: [0, 0],
    });
});

test("should handle goToStart", () => {
    store.setState(treeE4D5());
    store.getState().goToStart();

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        position: [],
    });
});

test("should handle goToEnd", () => {
    store.setState({ ...treeE4D5(), position: [] });
    store.getState().goToEnd();

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        position: [0, 0],
    });
});

test("should handle goToNext", () => {
    store.setState({ ...treeE4D5(), position: [] });
    store.getState().goToNext();

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        position: [0],
    });
});

test("should handle goToPrevious", () => {
    store.setState({ ...treeE4D5(), position: [0] });
    store.getState().goToPrevious();

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        position: [],
    });
});

test("should handle goToBranchEnd", () => {
    store.setState({ ...treeE4D5(), position: [] });
    store.getState().goToBranchEnd();

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        position: [0, 0],
    });
});

test("should handle goToBranchStart", () => {
    store.setState({ ...treeE4D5(), position: [0, 0] });
    store.getState().goToBranchStart();

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        position: [],
    });
});

test("should handle goToMove", () => {
    store.setState({ ...treeE4D5(), position: [] });
    store.getState().goToMove([0]);

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        position: [0],
    });
});

test("should handle deleteMove", () => {
    store.setState(treeE4D5());
    store.getState().deleteMove([0]);

    expect(getNewState()).toStrictEqual({ ...defaultTree(), dirty: true });
});

test("should handle setAnnotation", () => {
    store.setState({ ...treeE4D5(), position: [0] });
    store.getState().setAnnotation("!");

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        dirty: true,
        position: [0],
        root: {
            ...treeE4D5().root,
            children: [
                {
                    ...treeE4D5().root.children[0],
                    annotations: ["!"],
                },
            ],
        },
    });
});

test("should handle setComment", () => {
    store.setState({ ...treeE4D5(), position: [0] });
    store.getState().setComment("test");

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        dirty: true,
        position: [0],
        root: {
            ...treeE4D5().root,
            children: [
                {
                    ...treeE4D5().root.children[0],
                    comment: "test",
                },
            ],
        },
    });
});

test("should handle setFen", () => {
    store.setState({ ...treeE4D5(), position: [0] });
    store.getState().setFen("rnbq1bnr/ppppkppp/8/4p3/4P3/8/PPPPKPPP/RNBQ1BNR w - - 2 3");

    expect(getNewState()).toStrictEqual({
        ...defaultTree(),
        dirty: true,
        root: {
            ...defaultTree().root,
            fen: "rnbq1bnr/ppppkppp/8/4p3/4P3/8/PPPPKPPP/RNBQ1BNR w - - 2 3",
        },
    });
});

test("should handle setScore", () => {
    store.setState({ ...treeE4D5(), position: [0] });
    store.getState().setScore({
        value: {
            type: "mate",
            value: 1,
        },
        wdl: null,
    });

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        dirty: true,
        position: [0],
        root: {
            ...treeE4D5().root,
            children: [
                {
                    ...treeE4D5().root.children[0],
                    score: {
                        value: {
                            type: "mate",
                            value: 1,
                        },
                        wdl: null,
                    },
                },
            ],
        },
    });
});

test("should handle setShapes", () => {
    store.setState({ ...treeE4D5(), position: [0] });
    store.getState().setShapes([
        {
            brush: "red",
            orig: "e4",
            dest: "d5",
        },
    ]);

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        dirty: true,
        position: [0],
        root: {
            ...treeE4D5().root,
            children: [
                {
                    ...treeE4D5().root.children[0],
                    shapes: [
                        {
                            brush: "red",
                            orig: "e4",
                            dest: "d5",
                        },
                    ],
                },
            ],
        },
    });
});

test("should handle addAnalysis", () => {
    store.setState({ ...treeE4D5(), position: [0] });
    const analysis: AnalysisEntry[] = [
        {
            best: [
                {
                    depth: 1,
                    multipv: 1,
                    nodes: 1,
                    score: {
                        value: {
                            type: "cp",
                            value: 10,
                        },
                        wdl: null,
                    },
                    nps: 1000,
                    sanMoves: ["e4"],
                    uciMoves: ["e2e4"],
                },
            ],
            novelty: false,
            is_sacrifice: false,
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        },
        {
            best: [
                {
                    depth: 1,
                    multipv: 1,
                    nodes: 1,
                    score: {
                        value: {
                            type: "cp",
                            value: 20,
                        },
                        wdl: null,
                    },
                    nps: 1000,
                    sanMoves: ["d5"],
                    uciMoves: ["d7d5"],
                },
            ],
            novelty: false,
            is_sacrifice: false,
            fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        },
    ];
    store.getState().addAnalysis(analysis, deriveGlyphs(analysis, snapshot()));

    expect(getNewState()).toStrictEqual({
        ...treeE4D5(),
        dirty: true,
        position: [0],
        root: {
            ...treeE4D5().root,
            children: [
                {
                    ...treeE4D5().root.children[0],
                    score: {
                        value: {
                            type: "cp",
                            value: 20,
                        },
                        wdl: null,
                    },
                },
            ],
            score: {
                value: {
                    type: "cp",
                    value: 10,
                },
                wdl: null,
            },
        },
    });
});

test("should handle promoteVariation", () => {
    store.setState(treeE4D5Nf3());
    store.getState().promoteVariation([1]);

    expect(getNewState()).toStrictEqual({
        ...treeE4D5Nf3(),
        dirty: true,
        position: [0],
        root: {
            ...treeE4D5Nf3().root,
            children: [
                {
                    ...treeE4D5Nf3().root.children[1],
                },
                {
                    ...treeE4D5Nf3().root.children[0],
                },
            ],
        },
    });
});

function entry(fen: string, san: string, uci: string, cp: number): AnalysisEntry {
    return {
        best: [
            {
                depth: 1,
                multipv: 1,
                nodes: 1,
                score: { value: { type: "cp", value: cp }, wdl: null },
                nps: 1000,
                sanMoves: [san],
                uciMoves: [uci],
            },
        ],
        novelty: false,
        is_sacrifice: false,
        fen,
    };
}

function mainlineFens(): string[] {
    return Array.from(treeIteratorMainLine(store.getState().root), ({ node }) => node.fen);
}

function mainlineScores(): (number | null)[] {
    return Array.from(treeIteratorMainLine(store.getState().root), ({ node }) =>
        node.score ? node.score.value.value : null,
    );
}

test("addAnalysis skips a position the engine returned no lines for", () => {
    store.getState().makeMoves({ payload: ["e4", "d5", "Nf3"] });
    const fens = mainlineFens();
    const analysis = [
        entry(fens[0], "e4", "e2e4", 10),
        entry(fens[1], "d5", "d7d5", 20),
        { best: [], novelty: false, is_sacrifice: false, fen: fens[2] },
        entry(fens[3], "Nf6", "g8f6", 30),
    ];
    store.getState().addAnalysis(analysis, deriveGlyphs(analysis, snapshot()));

    expect(mainlineScores()).toStrictEqual([10, 20, null, 30]);
});

test("addAnalysis stops at the first position that no longer matches the run", () => {
    store.getState().makeMoves({ payload: ["e4", "d5", "Nf3"] });
    const fens = mainlineFens();
    const analysis = [
        entry(fens[0], "e4", "e2e4", 10),
        entry(fens[1], "d5", "d7d5", 20),
        entry(fens[2], "Nf3", "g1f3", 30),
        entry(fens[3], "Nf6", "g8f6", 40),
    ];
    const glyphs = deriveGlyphs(analysis, snapshot());

    // While the engine runs, the user replaces 1... d5 with 1... e5 2. Nf3 on the mainline.
    store.getState().deleteMove([0, 0]);
    store.getState().goToMove([0]);
    store.getState().makeMoves({ payload: ["e5", "Nf3"] });
    store.getState().addAnalysis(analysis, glyphs);

    expect(mainlineScores()).toStrictEqual([10, 20, null, null]);
});

test("addAnalysis matches positions regardless of FEN counters and en passant squares", () => {
    store.setState(defaultTree("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3"));
    store.getState().makeMoves({ payload: ["e5"] });
    const analysis = [
        entry("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1", "e5", "e7e5", 10),
        entry(mainlineFens()[1], "Nf3", "g1f3", 20),
    ];
    store.getState().addAnalysis(analysis, deriveGlyphs(analysis, snapshot()));

    expect(mainlineScores()).toStrictEqual([10, 20]);
});

test("addAnalysis writes only the current report run", () => {
    store.getState().makeMoves({ payload: ["e4"] });
    const run = snapshot();
    const analysis = [
        entry(mainlineFens()[0], "e4", "e2e4", 10),
        entry(mainlineFens()[1], "e5", "e7e5", 20),
    ];
    store.getState().beginReport(run);
    expect(store.getState().report).toStrictEqual({ inProgress: true, runId: run.runId });

    store.getState().addAnalysis(analysis, deriveGlyphs(analysis, run), {
        showVariations: false,
        runId: "an earlier run",
    });
    expect(mainlineScores()).toStrictEqual([null, null]);

    store.getState().addAnalysis(analysis, deriveGlyphs(analysis, run), {
        showVariations: false,
        runId: run.runId,
    });
    store.getState().endReport(run.runId);
    expect(mainlineScores()).toStrictEqual([10, 20]);
    expect(store.getState().report.inProgress).toBe(false);
});

describe("applyReport", () => {
    // Commentary records come from the frontend fixtures; only their shape matters here.
    const record = (fingerprint: string) => ({
        extended: ["□" as const],
        commentary: {
            ...buildMoveCommentary(commentaryFacts("onlyMove"), "!", commentaryRun()).commentary!,
            fingerprint,
        },
    });

    function analysed() {
        store.getState().makeMoves({ payload: ["e4", "e5"] });
        const fens = mainlineFens();
        return [
            entry(fens[0], "e4", "e2e4", 10),
            entry(fens[1], "e5", "e7e5", 20),
            entry(fens[2], "Nf3", "g1f3", 30),
        ];
    }

    test("writes commentary and extended glyphs, never the user's comment", () => {
        const analysis = analysed();
        store.getState().goToMove([0]);
        store.getState().setComment("my note");
        store
            .getState()
            .applyReport(analysis, [null, "!", null], { showVariations: false }, [
                null,
                record("a"),
                null,
            ]);

        const e4 = store.getState().root.children[0];
        expect(e4.comment).toBe("my note");
        expect(e4.commentary?.fingerprint).toBe("a");
        expect(e4.annotations).toStrictEqual(["!", "□"]);
        expect(store.getState().root.children[0].children[0].commentary).toBeUndefined();
    });

    test("setComment leaves the commentary alone", () => {
        const analysis = analysed();
        store
            .getState()
            .applyReport(analysis, [null, null, null], { showVariations: false }, [
                null,
                record("a"),
                null,
            ]);
        store.getState().goToMove([0]);
        store.getState().setComment("edited");
        expect(store.getState().root.children[0].commentary?.fingerprint).toBe("a");
    });

    test("a re-run replaces stale commentary and removes it where there is none", () => {
        const analysis = analysed();
        const apply = (results: (ReturnType<typeof record> | null)[]) =>
            store
                .getState()
                .applyReport(analysis, [null, null, null], { showVariations: false }, results);
        apply([null, record("a"), record("b")]);
        const first = store.getState().root.children[0].commentary;
        apply([null, record("a"), null]);
        expect(store.getState().root.children[0].commentary).toBe(first);
        expect(store.getState().root.children[0].children[0].commentary).toBeUndefined();
        apply([null, record("c"), null]);
        expect(store.getState().root.children[0].commentary?.fingerprint).toBe("c");
    });

    test("appendComment adds a paragraph to the user's comment", () => {
        store.getState().makeMoves({ payload: ["e4"] });
        store.getState().appendComment([0], "first");
        store.getState().appendComment([0], "second");
        expect(store.getState().root.children[0].comment).toBe("first\n\nsecond");
    });
});
