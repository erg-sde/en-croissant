import { describe, expect, test } from "vitest";
import { createTreeStore } from "@/state/store/tree";
import { samePosition, takeMainlineSnapshot } from "@/utils/commentary/snapshot";
import { defaultTree } from "@/utils/treeReducer";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const AFTER_E4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";

describe("samePosition", () => {
    test("treats missing counters as their defaults", () => {
        expect(samePosition("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -", START)).toBe(
            true,
        );
    });

    test("ignores an en passant square without a legal capture", () => {
        expect(samePosition(AFTER_E4.replace(" - ", " e3 "), AFTER_E4)).toBe(true);
    });

    test("keeps an en passant square with a legal capture", () => {
        const withCapture = "rnbqkbnr/ppp1pppp/8/8/3pP3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 3";
        expect(samePosition(withCapture, withCapture.replace(" e3 ", " - "))).toBe(false);
    });

    test("distinguishes side to move, castling rights and counters", () => {
        expect(samePosition(START, START.replace(" w ", " b "))).toBe(false);
        expect(samePosition(START, START.replace("KQkq", "Kkq"))).toBe(false);
        expect(samePosition(START, START.replace(" 0 1", " 2 3"))).toBe(false);
    });

    test("never matches an invalid FEN", () => {
        expect(samePosition("not a fen", "not a fen")).toBe(false);
    });
});

describe("takeMainlineSnapshot", () => {
    test("captures every mainline position from the root, ignoring variations", () => {
        const store = createTreeStore();
        store.getState().makeMoves({ payload: ["e4", "e5", "Nf3"] });
        store.getState().goToMove([0]);
        store.getState().makeMoves({ payload: ["c5"] });
        const { root, headers } = store.getState();

        const snapshot = takeMainlineSnapshot(root, {
            ...headers,
            time_control: "300+2",
        });

        expect(snapshot.rootFen).toBe(root.fen);
        expect(snapshot.plies.map((p) => [p.san, p.halfMoves])).toStrictEqual([
            [null, 0],
            ["e4", 1],
            ["e5", 2],
            ["Nf3", 3],
        ]);
        expect(snapshot.plies[1].fen).toBe(AFTER_E4);
        expect(snapshot.initialS).toBe(300);
        expect(snapshot.incrementS).toBe(2);
    });

    test("gives each run its own id and no times without a time control", () => {
        const { root, headers } = defaultTree();
        const first = takeMainlineSnapshot(root, headers);
        const second = takeMainlineSnapshot(root, headers);
        expect(first.runId).not.toBe(second.runId);
        expect(first.initialS).toBeNull();
        expect(first.incrementS).toBeNull();
    });
});
