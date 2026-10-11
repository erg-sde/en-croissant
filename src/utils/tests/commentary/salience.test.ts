import { parseSquare } from "chessops/util";
import { describe, expect, test } from "vitest";
import { positionFromFen } from "@/utils/chessops";
import { enumerateFacts, linePositions, selectFacts } from "@/utils/commentary/salience";
import { FIXTURE_NAMES, facts } from "./helpers";

const ctx = { rootHalfMoves: 0 };

describe("enumerateFacts", () => {
    test("assigns ids in a fixed order", () => {
        const ids = enumerateFacts(facts("mateAllowed"), ctx).map((f) => f.id);
        expect(ids).toStrictEqual(enumerateFacts(facts("mateAllowed"), ctx).map((f) => f.id));
        expect(ids.slice(0, 3)).toStrictEqual(["line:0", "line:1", "line:2"]);
        expect(ids).toContain("criticality:0");
        expect(ids.indexOf("criticality:0")).toBeLessThan(ids.indexOf("evalDelta:0"));
    });

    test("every piece in a vocabulary stands on its square in its position", () => {
        const misplaced: string[] = [];
        for (const name of FIXTURE_NAMES) {
            const f = facts(name);
            for (const fact of enumerateFacts(f, ctx)) {
                for (const piece of fact.vocabulary.pieces) {
                    const at = piece.at;
                    const pos =
                        at.t === "line"
                            ? linePositions(f, f.lines[at.id])[at.ply]
                            : positionFromFen(at.t === "before" ? f.fenBefore : f.fenAfter)[0];
                    const found = pos?.board.get(parseSquare(piece.square)!);
                    if (found?.role !== piece.piece) {
                        misplaced.push(`${name} ${fact.id} ${piece.piece} ${piece.square}`);
                    }
                }
            }
        }
        expect(misplaced).toStrictEqual([]);
    });

    test("a newly hanging piece counts only when the reply takes it", () => {
        const taken = enumerateFacts(facts("queenLost"), ctx).find(
            (f) => f.payload.kind === "hanging",
        );
        expect(taken?.weight).toBe(70);

        const f = facts("queenLost");
        const reply = f.lines[f.refutation!];
        reply.pvUci = ["g8f6", ...reply.pvUci.slice(1)];
        const ignored = enumerateFacts(f, ctx).find((x) => x.payload.kind === "hanging");
        expect(ignored?.weight).toBe(0);
    });
});

describe("selectFacts", () => {
    test("a mistake cites the refutation and the better move first", () => {
        const s = selectFacts(facts("mateAllowed"), "??", [], "keyMoments", ctx)!;
        expect(s.kind).toBe("mistake");
        expect(s.facts.map((f) => f.id).slice(0, 2)).toStrictEqual(["line:2", "line:0"]);
        expect(s.facts.length).toBeLessThanOrEqual(3);
    });

    test("a mistake on the engine's own first choice is an evaluation drop", () => {
        const s = selectFacts(facts("clearlyBest"), "?", [], "keyMoments", ctx)!;
        expect(s.kind).toBe("evalDrift");
        expect(s.facts[0].id).toBe("line:2");
    });

    test("a mistake without a better line has no commentary", () => {
        const f = facts("mateAllowed");
        f.best = null;
        expect(selectFacts(f, "??", [], "keyMoments", ctx)).toBeNull();
    });

    test("an only move cites its criticality and the failing alternative", () => {
        const s = selectFacts(facts("onlyMove"), "!", ["□"], "keyMoments", ctx)!;
        expect(s.kind).toBe("onlyMove");
        expect(s.facts.map((f) => f.id).slice(0, 2)).toStrictEqual(["criticality:0", "line:1"]);
    });

    test("a missed punishment, a sacrifice, a mate and an opening", () => {
        expect(selectFacts(facts("missedPunishment"), "?", ["⊗"], "keyMoments", ctx)?.kind).toBe(
            "miss",
        );
        expect(selectFacts(facts("sacrifice"), "!!", [], "keyMoments", ctx)?.kind).toBe(
            "sacrifice",
        );
        expect(selectFacts(facts("checkmate"), null, [], "keyMoments", ctx)?.kind).toBe("mate");
        expect(selectFacts(facts("openingNovelty"), null, [], "keyMoments", ctx)?.kind).toBe(
            "opening",
        );
    });

    test("detail levels", () => {
        expect(selectFacts(facts("openingNovelty"), null, [], "mistakes", ctx)).toBeNull();
        expect(selectFacts(facts("mateAllowed"), "??", [], "mistakes", ctx)?.kind).toBe("mistake");

        // With no fact of weight 35 or more, "all moves" adds nothing.
        const f = facts("openingNovelty");
        f.opening = null;
        f.novelty = false;
        f.refutation = null;
        f.second = null;
        expect(selectFacts(f, null, [], "allMoves", ctx)).toBeNull();
        expect(selectFacts(facts("clearlyBest"), null, [], "allMoves", ctx)?.kind).toBe(
            "positional",
        );
    });

    test("never more than three facts", () => {
        const counts = FIXTURE_NAMES.map(
            (name) => selectFacts(facts(name), "??", ["⊗"], "allMoves", ctx)?.facts.length ?? 0,
        );
        expect(Math.max(...counts)).toBeLessThanOrEqual(3);
    });
});
