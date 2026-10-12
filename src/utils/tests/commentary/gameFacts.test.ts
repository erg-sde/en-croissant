import { describe, expect, test } from "vitest";
import type { MoveFacts, Outcome } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { gameFacts, type MovesByIndex } from "@/utils/commentary/gameFacts";
import { narrateGame } from "@/utils/commentary/narrate";
import { buildGameNarrative, buildMoveCommentary } from "@/utils/commentary/report";
import { validateNarrative } from "@/utils/commentary/validate";
import games from "./fixtures/gameFacts.json";
import { run, translator } from "./helpers";

// 1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6?? 4. Qxf7#
const GLYPHS: (Annotation | null)[] = [null, null, null, null, null, null, "??", null];

function game() {
    const g = structuredClone(games.scholarsMate);
    return {
        moves: g.moves as MovesByIndex,
        fens: g.fens,
        result: g.result as Outcome,
    };
}

function selections(moves: MovesByIndex, r = run()) {
    return moves.map((m, i) => (m ? buildMoveCommentary(m, GLYPHS[i], r).selection : null));
}

describe("gameFacts", () => {
    test("finds the opening, the decisive move, the swings and the end", () => {
        const { moves, fens, result } = game();
        const facts = gameFacts(moves, fens, selections(moves), result);
        expect(facts.opening).toStrictEqual({
            eco: "C20",
            name: "King's Pawn Game: Wayward Queen Attack",
            lastNamedIndex: 3,
        });
        expect(facts.decisive).toBe(6);
        expect(facts.swings.map((s) => s.index)).toContain(6);
        expect(facts.swings.find((s) => s.index === 6)?.kind).toBe("mistake");
        expect(facts.terminal).toStrictEqual({ index: 7, san: "Qxf7#", mate: true });
        expect(facts.accuracy.white).toBeGreaterThan(facts.accuracy.black!);
        expect(facts.noveltyIndex).toBeNull();
    });

    test("no decisive move without a decisive result", () => {
        const { moves, fens } = game();
        expect(gameFacts(moves, fens, selections(moves), "*").decisive).toBeNull();
        expect(gameFacts(moves, fens, selections(moves), "1/2-1/2").decisive).toBeNull();
    });

    test("accuracy skips moves without facts instead of joining across them", () => {
        const { moves, fens, result } = game();
        const full = gameFacts(moves, fens, selections(moves), result).accuracy;
        moves[3] = null;
        const gap = gameFacts(moves, fens, selections(moves), result).accuracy;
        expect(gap.black).toBe(full.black);
        expect(gap.white).not.toBe(full.white);
    });
});

describe("game narrative", () => {
    test("narrates the game and its takeaways (en-US)", () => {
        const { moves, fens, result } = game();
        const narrative = buildGameNarrative(moves, fens, selections(moves), result, run())!;
        const texts = narrative.narration.sentences.map((s) => s.text);
        expect(texts).toContain(
            "The game followed the King's Pawn Game: Wayward Queen Attack (C20) through move 2.",
        );
        expect(texts).toContain("The game was decided by 3...Nf6.");
        expect(texts).toContain("4.Qxf7# ended the game with checkmate.");
        expect(narrative.takeaways.map((t) => t.narration.sentences[0].text)).toContain(
            "The game was decided in the opening with 3...Nf6.",
        );
        expect(narrative.provenance.validation?.failures).toStrictEqual([]);
    });

    test("every narrative sentence passes validation in every locale", () => {
        const { moves, fens, result } = game();
        const failures: string[] = [];
        for (const lang of ["en-US", "de-DE", "ru-RU", "zh-CN"]) {
            const r = run(lang, translator(lang));
            const facts = gameFacts(moves, fens, selections(moves, r), result);
            const { drafts, takeaways } = narrateGame(facts, moves, selections(moves, r), r);
            for (const set of [drafts, ...takeaways.map((t) => t.drafts)]) {
                const { result: validation } = validateNarrative(set, facts, moves, r);
                failures.push(...validation.failures.map((f) => `${lang}: ${f.rule} ${f.detail}`));
            }
        }
        expect(failures).toStrictEqual([]);
    });

    test("takeaways need two supporting moves", () => {
        const { moves, fens, result } = game();
        const r = run();
        const sel = selections(moves, r);
        // Two hurried mistakes: Nc6 and Nf6, both played in time trouble.
        const hurried = moves.map((m, i) =>
            m && (i === 4 || i === 6)
                ? ({ ...m, time: { remainingS: 5, spentS: 1, underPressure: true } } as MoveFacts)
                : m,
        );
        sel[4] = { ...sel[6]!, index: 4, kind: "mistake" };
        const facts = gameFacts(hurried, fens, sel, result);
        const { takeaways } = narrateGame(facts, hurried, sel, r);
        expect(takeaways.some((t) => t.indices.join() === "4,6")).toBe(true);

        const single = narrateGame(facts, hurried, selections(moves, r), r).takeaways;
        expect(single.some((t) => t.indices.length > 1)).toBe(false);
    });

    test("a narrative citing an unknown move fact is dropped", () => {
        const { moves, fens, result } = game();
        const facts = gameFacts(moves, fens, selections(moves), result);
        const { result: validation } = validateNarrative(
            [
                {
                    template: "x",
                    sentence: {
                        text: "x",
                        factIds: ["9/line:0"],
                        refs: { moves: [], pieces: [], squares: [], numbers: [], sides: [] },
                    },
                },
            ],
            facts,
            moves,
            run(),
        );
        expect(validation.failures.map((f) => f.rule)).toStrictEqual(["unknownFact"]);
    });
});
