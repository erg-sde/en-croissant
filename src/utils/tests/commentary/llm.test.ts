import { describe, expect, test } from "vitest";
import type { NarrationFailure, Outcome } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { narrationFailureMessage } from "@/utils/commentary/errors";
import type { MovesByIndex } from "@/utils/commentary/gameFacts";
import {
    buildNarrationRequest,
    formatEval,
    narratedGameRecord,
    narratedMoveRecord,
    type NarrationRunContext,
} from "@/utils/commentary/llm";
import { canNarrate, narratorConfig } from "@/utils/commentary/narrator";
import { buildGameNarrative, buildMoveCommentary } from "@/utils/commentary/report";
import { defaultCommentarySettings } from "@/utils/commentary/settings";
import { validateFreeNarrative, validateFreeText } from "@/utils/commentary/validate";
import games from "./fixtures/gameFacts.json";
import { facts, run } from "./helpers";

const SERVED = { model: "claude-opus-5-5", inputTokens: 100, outputTokens: 20 };

/** The rules each sentence fails, for 2...Qg5?? 3.Nxg5 (fixture queenLost). */
function rulesFor(sentences: string[]): string[][] {
    const { result } = validateFreeText(sentences, facts("queenLost"), 80, run());
    return sentences.map((_, i) =>
        result.failures.filter((f) => f.sentence === i).map((f) => f.rule),
    );
}

describe("coach sentences", () => {
    test("moves from the lines, legal moves and pieces where they stand pass", () => {
        const sentences = [
            "Qg5 puts the queen on g5 where 3.Nxg5 simply takes it, leaving Black a queen down.",
            "Developing with Nf6 was the natural move, and Bb4 would have followed.",
            "Black could also have defended the e5 pawn with d6.",
        ];
        expect(rulesFor(sentences)).toStrictEqual([[], [], []]);
        const { narration } = validateFreeText(sentences, facts("queenLost"), 80, run());
        // The moves named become chips, written as in the text.
        expect(narration.sentences[0].refs.moves.map((m) => m.san)).toStrictEqual(["Qg5", "Nxg5"]);
        expect(narration.sentences[0].refs.moves[1].at).toStrictEqual({
            t: "line",
            id: 2,
            ply: 0,
        });
    });

    test("invented moves, misplaced pieces and markup are dropped with the named rule", () => {
        expect(
            rulesFor([
                "Better was Qa5, hitting the e4 pawn.",
                "Now the bishop on e4 is loose.",
                "**Qg5** is a blunder.",
                "The position after Qg5 {is lost}.",
            ]),
        ).toStrictEqual([
            ["moveNotLegalForSide"],
            ["pieceNotOnBoard"],
            ["forbiddenToken"],
            ["forbiddenToken"],
        ]);
    });

    test("the narrative may name moves of the game and its lines only", () => {
        const moves = structuredClone(games.scholarsMate).moves as MovesByIndex;
        const { result } = validateFreeNarrative(
            [
                "After 3...Nf6, 4.Qxf7# ended the game at once.",
                "Black should have played Qa5 instead.",
            ],
            moves,
            run(),
        );
        expect(result.failures.map((f) => [f.sentence, f.rule])).toStrictEqual([
            [1, "moveNotLegalForSide"],
        ]);
    });
});

function gameContext(): NarrationRunContext {
    const g = structuredClone(games.scholarsMate);
    const moves = g.moves as MovesByIndex;
    const glyphs: (Annotation | null)[] = [null, null, null, null, null, null, "??", null];
    const r = run();
    const results = moves.map((m, i) => (m ? buildMoveCommentary(m, glyphs[i], r) : null));
    const narrative = buildGameNarrative(
        moves,
        g.fens,
        results.map((x) => x?.selection ?? null),
        g.result as Outcome,
        r,
    );
    return { tab: "tab", runId: "run", provider: "anthropic", run: r, moves, results, narrative };
}

describe("narration request", () => {
    test("sends both positions, the evaluations and the engine lines of moves worth explaining", () => {
        const ctx = gameContext();
        const request = buildNarrationRequest(ctx);
        expect(Object.keys(request).sort()).toStrictEqual(["game", "items", "lang", "perspective"]);
        // Moves graded book, best, excellent or good are not sent.
        const graded = ctx.results.flatMap((r, i) =>
            r?.commentary && ["book", "best", "excellent", "good"].includes(r.commentary.kind)
                ? [i]
                : [],
        );
        expect(request.items.some((i) => graded.includes(i.index))).toBe(false);
        const nf6 = request.items.find((i) => i.index === 6)!;
        expect(nf6.san).toBe("Nf6");
        expect(nf6.fenAfter).toBe(ctx.moves[6]!.fenAfter);
        expect(nf6.evalAfter).toBe("#1");
        expect(nf6.lines.find((l) => l.role === "reply")?.moves).toBe("4.Qxf7#");
        expect(nf6.lines.find((l) => l.role === "best")?.moves).toMatch(/^3\.\.\.g6/);
        expect(request.game.turningPoints.map((i) => i.index)).toContain(6);
    });

    test("evaluations are White's point of view in pawns or as mates", () => {
        expect(formatEval({ type: "cp", value: 123 })).toBe("+1.2");
        expect(formatEval({ type: "cp", value: -40 })).toBe("-0.4");
        expect(formatEval({ type: "mate", value: -3 })).toBe("#-3");
    });
});

describe("applying a coached chunk", () => {
    test("keeps the sentences that pass and records the model", () => {
        const ctx = gameContext();
        const nf6 = ctx.moves[6]!;
        const record = narratedMoveRecord(
            ctx,
            {
                index: 6,
                fenBefore: nf6.fenBefore,
                san: "Nf6",
                sentences: [
                    "Nf6 develops but ignores the threat on f7, and 4.Qxf7# is mate at once.",
                    "Instead Qa5 would have kept everything safe.",
                    "3...g6 was the only way to block the queen's attack.",
                ],
            },
            SERVED,
        )!;
        expect(record.commentary.narration.sentences.map((s) => s.text)).toStrictEqual([
            "Nf6 develops but ignores the threat on f7, and 4.Qxf7# is mate at once.",
            "3...g6 was the only way to block the queen's attack.",
        ]);
        expect(record.commentary.provenance).toMatchObject({
            source: "llm",
            provider: "anthropic",
            model: "claude-opus-5-5",
        });
        expect(record.commentary.provenance.validation?.dropped).toBe(1);
    });

    test("nothing valid keeps the deterministic record; a position mismatch is ignored", () => {
        const ctx = gameContext();
        const nf6 = ctx.moves[6]!;
        expect(
            narratedMoveRecord(
                ctx,
                { index: 6, fenBefore: nf6.fenBefore, san: "Nf6", sentences: ["Play Qa5."] },
                SERVED,
            ),
        ).toBeNull();
        expect(
            narratedMoveRecord(
                ctx,
                { index: 6, fenBefore: "8/8/8/8/8/8/8/8 w - - 0 1", san: "Nf6", sentences: [] },
                SERVED,
            ),
        ).toBeNull();
    });

    test("a coached summary keeps its lessons and replaces the story", () => {
        const ctx = gameContext();
        const narrative = narratedGameRecord(
            ctx,
            {
                sentences: ["Black developed naturally but overlooked the mate threat on f7."],
                takeaways: [
                    {
                        indices: [6],
                        sentences: [
                            "Before every move, ask what your opponent's last move threatens.",
                        ],
                    },
                    { indices: [6], sentences: ["Qa5 is never the answer."] },
                ],
            },
            SERVED,
        )!;
        expect(narrative.narration.sentences[0].text).toMatch(/^Black developed/);
        expect(narrative.takeaways.map((t) => t.narration.sentences[0].text)).toStrictEqual([
            "Before every move, ask what your opponent's last move threatens.",
        ]);
        expect(narrative.provenance.source).toBe("llm");
    });
});

describe("settings and failures", () => {
    test("narration needs consent and a key or a local server", () => {
        const on = { ...defaultCommentarySettings, llmEnabled: true, consentVersion: 1 };
        expect(canNarrate(on, { configured: true, hint: "abcd" })).toBe(true);
        expect(canNarrate(on, { configured: false, hint: null })).toBe(false);
        expect(canNarrate({ ...on, consentVersion: 0 }, { configured: true, hint: null })).toBe(
            false,
        );
        expect(
            canNarrate(
                { ...on, provider: "openaiCompatible", baseUrl: "http://localhost:11434" },
                { configured: false, hint: null },
            ),
        ).toBe(true);
        const config = narratorConfig(on, "de-DE", "white");
        expect(config.language).toBe("de-DE");
        expect(Object.keys(config)).not.toContain("key");
    });

    test("every failure has a message except a cancel", () => {
        const t = run().t;
        const failures: NarrationFailure[] = [
            { kind: "noKey" },
            { kind: "insecureBaseUrl" },
            { kind: "missingBaseUrl" },
            { kind: "auth", detail: { status: 401 } },
            { kind: "rateLimit", detail: { retryAfterMs: 2500 } },
            { kind: "network", detail: { message: "timed out" } },
            { kind: "provider", detail: { status: 529, message: "Overloaded" } },
            { kind: "malformedResponse", detail: { message: "eof" } },
            { kind: "refused", detail: { category: null } },
        ];
        const untranslated = failures
            .map((f) => narrationFailureMessage(f, t)!)
            .filter((m) => m.title.startsWith("Errors."));
        expect(untranslated).toStrictEqual([]);
        expect(
            narrationFailureMessage({ kind: "rateLimit", detail: { retryAfterMs: 2500 } }, t)
                ?.detail,
        ).toBe("Retry after 3 s");
        expect(narrationFailureMessage({ kind: "cancelled" }, t)).toBeNull();
    });
});
