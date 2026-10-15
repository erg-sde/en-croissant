import { describe, expect, test } from "vitest";
import type { Claim, NarratedSentence, NarrationFailure, Outcome, Slots } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { narrationFailureMessage } from "@/utils/commentary/errors";
import type { MovesByIndex } from "@/utils/commentary/gameFacts";
import {
    buildNarrationRequest,
    narratedGameRecord,
    narratedMoveRecord,
    type NarrationRunContext,
} from "@/utils/commentary/llm";
import { canNarrate, narratorConfig } from "@/utils/commentary/narrator";
import { buildGameNarrative, buildMoveCommentary } from "@/utils/commentary/report";
import { renderSlots } from "@/utils/commentary/segmentize";
import { defaultCommentarySettings } from "@/utils/commentary/settings";
import { validateSentences } from "@/utils/commentary/validate";
import { VERDICT_KINDS } from "@/utils/commentary/verdict";
import games from "./fixtures/gameFacts.json";
import { type FixtureName, facts, run } from "./helpers";

const EMPTY: Slots = {
    m0: null,
    m1: null,
    p0: null,
    p1: null,
    s0: null,
    s1: null,
    n0: null,
    n1: null,
    side: null,
    pieces: [],
};

function sentence(
    template: string,
    slots: Partial<Slots>,
    factIds: string[],
    claims: Claim[] = [],
): NarratedSentence {
    return { template, slots: { ...EMPTY, ...slots }, claims, factIds };
}

const SERVED = { model: "claude-opus-5-5", inputTokens: 100, outputTokens: 20 };

/** The rules a model-written sentence fails for the move in fixture `name`. */
function rulesFor(name: FixtureName, glyph: Annotation | null, s: NarratedSentence): string[] {
    const r = run();
    const f = facts(name);
    const selection = buildMoveCommentary(f, glyph, r).selection!;
    const rendered = renderSlots(s.template, s.slots, r.t, r.lang);
    const { result } = validateSentences(
        [
            {
                template: s.template,
                sentence: { text: rendered.text, factIds: s.factIds, refs: rendered.refs },
                claims: s.claims,
                problems: rendered.problems,
            },
        ],
        f,
        selection,
        r,
    );
    return result.failures.map((failure) => failure.rule);
}

// 2...Qg5?? 3.Nxg5: line 2 is White's reply, winning the queen.
const QG5 = { san: "Qg5", side: "black", at: { t: "before" } } as const;
const NXG5 = { san: "Nxg5", side: "white", at: { t: "line", id: 2, ply: 0 } } as const;
const QUEEN = { piece: "queen", square: "g5", at: { t: "line", id: 2, ply: 0 } } as const;
const WINS_QUEEN: Claim = { kind: "winsMaterial", side: "white", pawns: 9, line: 2 };

describe("model-written sentences", () => {
    test("a sentence built from the facts passes", () => {
        const good = sentence(
            "{{m0}} allows {{m1}}, which wins {{p0}}.",
            { m0: QG5, m1: NXG5, p0: QUEEN },
            ["line:2"],
            [WINS_QUEEN],
        );
        expect(rulesFor("queenLost", "??", good)).toStrictEqual([]);
        expect(renderSlots(good.template, good.slots, run().t, "en-US").text).toBe(
            "Qg5 allows Nxg5, which wins the queen on g5.",
        );
    });

    test("seeded bad outputs are each dropped with the named rule", () => {
        const cases: [string, FixtureName, Annotation | null, NarratedSentence, string][] = [
            [
                "a knight slot where the queen stands",
                "queenLost",
                "??",
                sentence(
                    "{{m0}} allows {{m1}}, which wins {{p0}}.",
                    { m0: QG5, m1: NXG5, p0: { ...QUEEN, piece: "knight" } },
                    ["line:2"],
                    [WINS_QUEEN],
                ),
                "pieceNotOnBoard",
            ],
            [
                "a move written into the template",
                "queenLost",
                "??",
                sentence("{{m0}} allows Nxg5.", { m0: QG5 }, ["line:2"]),
                "literalInTemplate",
            ],
            [
                "a fork claim with attacker and target swapped",
                "mateAllowed",
                "??",
                sentence(
                    "{{m0}} is a mistake.",
                    { m0: { san: "Nf6", side: "black", at: { t: "before" } } },
                    ["line:1"],
                    [
                        {
                            kind: "forks",
                            attacker: { piece: "pawn", square: "e4", at: { t: "after" } },
                            targets: [
                                { piece: "knight", square: "f6", at: { t: "after" } },
                                { piece: "queen", square: "h5", at: { t: "after" } },
                            ],
                        },
                    ],
                ),
                "claimContradicted",
            ],
            [
                "a material claim on a line that only has an evaluation",
                "mateAllowed",
                "??",
                sentence(
                    "{{m0}} was the better move.",
                    { m0: { san: "g6", side: "black", at: { t: "line", id: 0, ply: 0 } } },
                    ["line:0"],
                    [{ kind: "winsMaterial", side: "black", pawns: 1, line: 0 }],
                ),
                "claimUnsupported",
            ],
            [
                "engine jargon",
                "queenLost",
                "??",
                sentence("{{m0}} costs many centipawns.", { m0: QG5 }, ["line:2"]),
                "forbiddenToken",
            ],
            [
                "a square outside the cited facts",
                "queenLost",
                "??",
                sentence("{{m0}} weakens {{s0}}.", { m0: QG5, s0: "h7" }, ["line:2"]),
                "slotNotInFacts",
            ],
            [
                "a relation without its claim",
                "queenLost",
                "??",
                sentence("{{m1}} attacks after {{m0}}.", { m0: QG5, m1: NXG5 }, ["line:2"]),
                "relationWithoutClaim",
            ],
            [
                "a placeholder whose slot is empty",
                "queenLost",
                "??",
                sentence("{{m0}} allows {{m1}}.", { m0: QG5 }, ["line:2"]),
                "literalInTemplate",
            ],
            [
                "a piece name written into the template",
                "queenLost",
                "??",
                sentence("{{m0}} loses the queen.", { m0: QG5 }, ["line:2"]),
                "literalInTemplate",
            ],
            [
                "a side declared better without the claim",
                "queenLost",
                "??",
                sentence("After {{m0}}, {{side}} is better.", { m0: QG5, side: "white" }, [
                    "line:2",
                ]),
                "sideBetter",
            ],
            [
                "an equal verdict in a lost position",
                "queenLost",
                "??",
                sentence("After {{m0}} the game stays equal.", { m0: QG5 }, ["line:2"]),
                "magnitude",
            ],
            [
                "a motif no cited fact found",
                "queenLost",
                "??",
                sentence("{{m0}} creates an outpost.", { m0: QG5 }, ["line:2"]),
                "unknownMotif",
            ],
        ];
        const missed = cases
            .filter(([, name, glyph, s, rule]) => !rulesFor(name, glyph, s).includes(rule))
            .map(
                ([label, name, glyph, s, rule]) =>
                    `${label}: wanted ${rule}, got ${rulesFor(name, glyph, s).join(",")}`,
            );
        expect(missed).toStrictEqual([]);
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

describe("what the checker allows", () => {
    test("a piece by name, an unused slot and 'the better move' pass", () => {
        const s = sentence(
            "{{m0}} allows {{m1}}, which wins the {{q0}}; the better move was elsewhere.",
            { m0: QG5, m1: NXG5, p0: QUEEN, s0: "g5" },
            ["line:2"],
            [WINS_QUEEN],
        );
        expect(rulesFor("queenLost", "??", s)).toStrictEqual([]);
        expect(renderSlots(s.template, s.slots, run().t, "en-US").text).toBe(
            "Qg5 allows Nxg5, which wins the queen; the better move was elsewhere.",
        );
    });
});

describe("narration request", () => {
    test("carries facts and drafts only", () => {
        const ctx = gameContext();
        const request = buildNarrationRequest(ctx);
        expect(Object.keys(request).sort()).toStrictEqual(["game", "items", "lang", "perspective"]);
        // Verdict-only moves stay deterministic and are not sent.
        const withCommentary = ctx.results.flatMap((r, i) =>
            r?.commentary && !VERDICT_KINDS.includes(r.commentary.kind) ? [i] : [],
        );
        expect(request.items.map((i) => i.index)).toStrictEqual(withCommentary);
        const nf6 = request.items.find((i) => i.index === 6)!;
        expect(nf6.san).toBe("Nf6");
        const texts = ctx.results[6]!.commentary!.narration.sentences.map((s) => s.text);
        expect(nf6.facts.map((f) => f.textHint).filter(Boolean)).toStrictEqual(texts);
        expect(request.game.facts.every((f) => f.id.startsWith("game:"))).toBe(true);
        expect(request.game.turningPoints.map((i) => i.index)).toContain(6);
    });
});

describe("applying a narrated chunk", () => {
    test("keeps valid sentences, falls back to drafts for dropped ones, records the model", () => {
        const ctx = gameContext();
        const nf6 = ctx.moves[6]!;
        const qxf7 = { san: "Qxf7#", side: "white", at: { t: "line", id: 2, ply: 0 } } as const;
        const record = narratedMoveRecord(
            ctx,
            {
                index: 6,
                fenBefore: nf6.fenBefore,
                san: "Nf6",
                sentences: [
                    sentence(
                        "{{m0}} is a blunder: {{m1}} mates at once.",
                        { m0: { san: "Nf6", side: "black", at: { t: "before" } }, m1: qxf7 },
                        ["line:2"],
                        [{ kind: "mates", side: "white", mateIn: 1, line: 2 }],
                    ),
                    sentence(
                        "{{m0}} hangs the knight.",
                        { m0: { san: "Nf6", side: "black", at: { t: "before" } } },
                        ["line:0"],
                    ),
                ],
            },
            SERVED,
        )!;
        const texts = record.commentary.narration.sentences.map((s) => s.text);
        expect(texts[0]).toBe("Nf6 is a blunder: Qxf7# mates at once.");
        // The dropped sentence cited line:0; its draft comes back.
        const draft = ctx.results[6]!.commentary!.narration.sentences.find((s) =>
            s.factIds.includes("line:0"),
        )!;
        expect(texts).toContain(draft.text);
        expect(record.commentary.provenance).toMatchObject({
            source: "llm",
            provider: "anthropic",
            model: "claude-opus-5-5",
            usage: { inputTokens: 100, outputTokens: 20 },
        });
        expect(record.commentary.provenance.validation?.dropped).toBe(1);
        expect(record.san).toBe("Nf6");
    });

    test("nothing valid keeps the deterministic record; a position mismatch is ignored", () => {
        const ctx = gameContext();
        const nf6 = ctx.moves[6]!;
        const bad = sentence("Nf6 hangs.", {}, ["line:0"]);
        expect(
            narratedMoveRecord(
                ctx,
                { index: 6, fenBefore: nf6.fenBefore, san: "Nf6", sentences: [bad] },
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

    test("a narrated game summary is validated like the deterministic one", () => {
        const ctx = gameContext();
        const decided = { san: "Nf6", side: "black", at: { t: "before" } } as const;
        const narrative = narratedGameRecord(
            ctx,
            {
                sentences: [
                    sentence("{{m0}} decided the game.", { m0: decided }, ["game:decisive:0"]),
                    sentence(
                        "Black was crushed on move {{n0}}.",
                        { n0: { value: 99, unit: "move" } },
                        ["game:decisive:0"],
                    ),
                ],
                takeaways: [],
            },
            SERVED,
        )!;
        expect(narrative.narration.sentences[0].text).toBe("Nf6 decided the game.");
        expect(narrative.provenance.source).toBe("llm");
        expect(narrative.takeaways).toStrictEqual(ctx.narrative!.takeaways);
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
