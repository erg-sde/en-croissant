import { describe, expect, test } from "vitest";
import type { Annotation } from "@/utils/annotation";
import { narrateMove } from "@/utils/commentary/narrate";
import { buildMoveCommentary } from "@/utils/commentary/report";
import { selectFacts } from "@/utils/commentary/salience";
import { formatNumber, segmentize } from "@/utils/commentary/segmentize";
import { validateSentences } from "@/utils/commentary/validate";
import { type FixtureName, facts, run, translator } from "./helpers";

const GLYPHS: Record<FixtureName, Annotation | null> = {
    mateAllowed: "??",
    checkmate: null,
    queenLost: "??",
    onlyMove: "!",
    clearlyBest: "!",
    sacrifice: "!!",
    missedPunishment: "?",
    openingNovelty: null,
};
const NAMES = Object.keys(GLYPHS) as FixtureName[];

function sentences(name: FixtureName, lang = "en-US", t = translator(lang)): string[] {
    return (
        buildMoveCommentary(
            facts(name),
            GLYPHS[name],
            run(lang, t),
        ).commentary?.narration.sentences.map((s) => s.text) ?? []
    );
}

// A made-up locale that reorders every value, as a translation may.
const REORDERED = {
    "Commentary.Mistake.Line.Mate":
        "Mate in {{mateIn}} for {{side}} after {{line}}, allowed by {{san}}.",
    "Commentary.Mistake.Best.Eval": "{{eval}} was kept by {{best}}.",
    "Commentary.Mistake.Line.Material": "The {{piece}} falls to {{line}} after {{san}}.",
    "Commentary.Sacrifice": "On {{square}} the {{piece}} is en prise after {{san}}.",
    "Commentary.PieceOn": "{{square}}: {{piece}}",
};

describe("narrateMove", () => {
    test("writes reasons for the main kinds (en-US)", () => {
        expect(sentences("mateAllowed").slice(0, 2)).toStrictEqual([
            "Nf6 allows 4.Qxf7#, and White mates in 1.",
            "Better was g6, keeping the position at -0.30.",
        ]);
        expect(sentences("queenLost")[0]).toBe("Qg5 allows 3.Nxg5 Nf6, winning the queen.");
        expect(sentences("checkmate")).toStrictEqual(["Qxf7# is checkmate."]);
        expect(sentences("onlyMove").slice(0, 2)).toStrictEqual([
            "Ra5 was the only move: every alternative gives up at least 68% winning chances.",
            "Nothing else holds: Ra7 runs into 1...Nc3 2.Kd2 (-2.50).",
        ]);
        expect(sentences("sacrifice")[0]).toBe("Bxf7+ puts the bishop on f7 en prise.");
        expect(sentences("openingNovelty")).toStrictEqual([
            "The game follows the Nimzo-Indian Defense (E20) through move 3.",
            "Nc3 leaves the games in your reference database.",
        ]);
    });

    test("leaks no key, placeholder, sentinel or markup", () => {
        const leak = /Commentary\.|\{\{|\}\}|[<>]|\*\*/;
        const control = (text: string) => Array.from(text).some((c) => c.charCodeAt(0) < 0x20);
        const leaks = ["en-US", "de-DE", "ru-RU", "zh-CN"].flatMap((lang) =>
            NAMES.flatMap((name) =>
                sentences(name, lang)
                    .filter((text) => leak.test(text) || control(text))
                    .map((text) => `${lang} ${name}: ${text}`),
            ),
        );
        expect(leaks).toStrictEqual([]);
    });

    test("every deterministic sentence passes validation, in every locale", () => {
        const locales: [string, ReturnType<typeof translator>][] = [
            ["en-US", translator("en-US")],
            ["de-DE", translator("de-DE")],
            ["ru-RU", translator("ru-RU")],
            ["zh-CN", translator("zh-CN")],
            ["eo", translator("eo", { eo: REORDERED })],
        ];
        const failures: string[] = [];
        for (const [lang, t] of locales) {
            for (const name of NAMES) {
                const f = facts(name);
                const r = run(lang, t);
                const selection = selectFacts(f, GLYPHS[name], [], "keyMoments", r);
                if (!selection) continue;
                const drafts = narrateMove(selection, f, r);
                const { result } = validateSentences(drafts, f, selection, r);
                for (const x of result.failures) {
                    failures.push(`${lang} ${name}: ${x.rule} ${x.detail}`);
                }
                if (drafts.length === 0) {
                    failures.push(`${lang} ${name}: no sentence`);
                }
            }
        }
        expect(failures).toStrictEqual([]);
    });

    test("a translation can reorder values but not add any", () => {
        const t = translator("eo", { eo: REORDERED });
        expect(sentences("mateAllowed", "eo", t)[0]).toBe(
            "Mate in 1 for White after 4.Qxf7#, allowed by Nf6.",
        );
        expect(sentences("sacrifice", "eo", t)[0]).toBe(
            "On f7 the bishop is en prise after Bxf7+.",
        );
    });
});

describe("segmentize", () => {
    test("records a localized number as one reference", () => {
        const ref = { value: 12.5, unit: "pawns" as const };
        expect(formatNumber(ref, "de-DE")).toBe("12,5");
        const t = translator("de-DE", { "de-DE": { "Test.Pawns": "{{pawns}} Bauern" } });
        const out = segmentize(["Test.Pawns"], { pawns: { kind: "number", ref } }, t, "de-DE");
        expect(out.text).toBe("12,5 Bauern");
        expect(out.refs.numbers).toStrictEqual([ref]);
        expect(out.template).toBe("{{pawns}} Bauern");
    });

    test("lists use the locale's conjunction and record every item", () => {
        const at = { t: "after" } as const;
        const t = translator("en-US");
        const out = segmentize(
            ["Commentary.Motif.Fork"],
            {
                attacker: { kind: "piece", ref: { piece: "knight", square: "c7", at } },
                list: {
                    kind: "list",
                    items: [
                        { kind: "piece", ref: { piece: "queen", square: "a8", at } },
                        { kind: "piece", ref: { piece: "king", square: "e8", at } },
                    ],
                },
            },
            t,
            "en-US",
        );
        expect(out.text).toBe("The knight on c7 forks the queen on a8 and the king on e8.");
        expect(out.refs.squares).toStrictEqual(["c7", "a8", "e8"]);
    });
});
