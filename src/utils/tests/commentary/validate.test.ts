import { describe, expect, test } from "vitest";
import type { FactVocabulary, MoveRef } from "@/bindings";
import type { SentenceDraft } from "@/utils/commentary/narrate";
import { buildMoveCommentary } from "@/utils/commentary/report";
import { selectFacts } from "@/utils/commentary/salience";
import { restoreCommentary, validateSentences } from "@/utils/commentary/validate";
import { facts, run } from "./helpers";

// 3...Nf6?? in 1. e4 e5 2. Qh5 Nc6 3. Bc4: line:2 is the reply 4. Qxf7#, line:0 the better 3...g6.
const f = facts("mateAllowed");
const selection = selectFacts(f, "??", [], "keyMoments", run())!;
const reply: MoveRef = { san: "Qxf7#", side: "white", at: { t: "line", id: 2, ply: 0 } };

function refs(partial: Partial<FactVocabulary>): FactVocabulary {
    return { moves: [], pieces: [], squares: [], numbers: [], sides: [], ...partial };
}

function draft(
    text: string,
    extra: Partial<SentenceDraft["sentence"]> = {},
    template = "{{x}}",
): SentenceDraft {
    return { template, sentence: { text, factIds: ["line:2"], refs: refs({}), ...extra } };
}

function rules(d: SentenceDraft): string[] {
    return validateSentences([d], f, selection, run()).result.failures.map((x) => x.rule);
}

describe("validateSentences", () => {
    test("keeps a sentence whose references come from its facts", () => {
        const ok = draft("Nf6 allows 4.Qxf7#.", { refs: refs({ moves: [reply] }) });
        const { narration, result } = validateSentences([ok], f, selection, run());
        expect(narration.sentences).toStrictEqual([ok.sentence]);
        expect(result).toStrictEqual({ checked: 1, dropped: 0, failures: [] });
    });

    test("drops uncited and unknown citations", () => {
        expect(rules(draft("x", { factIds: [] }))).toContain("uncitedFact");
        expect(rules(draft("x", { factIds: ["line:9"] }))).toContain("unknownFact");
    });

    test("drops references the cited facts do not permit", () => {
        const notInLine: MoveRef = { san: "Qxf7#", side: "white", at: { t: "after" } };
        expect(rules(draft("x", { refs: refs({ moves: [notInLine] }) }))).toContain(
            "slotNotInFacts",
        );
        expect(rules(draft("x", { refs: refs({ squares: ["d5"] }) }))).toContain("slotNotInFacts");
        expect(
            rules(draft("x", { refs: refs({ numbers: [{ value: 2, unit: "mateIn" }] }) })),
        ).toContain("slotNotInFacts");
        expect(
            rules(draft("x", { factIds: ["line:0"], refs: refs({ moves: [reply] }) })),
        ).toContain("slotNotInFacts");
    });

    test("drops a piece that is not on its square", () => {
        // A knight on c4 where White's bishop stands.
        const wrong = { piece: "knight" as const, square: "c4", at: { t: "after" as const } };
        expect(rules(draft("x", { refs: refs({ pieces: [wrong], squares: ["c4"] }) }))).toContain(
            "pieceNotOnBoard",
        );
    });

    test("drops a move that is illegal or not the side's move", () => {
        const wrongSide: MoveRef = {
            san: "Qxf7#",
            side: "black",
            at: { t: "line", id: 2, ply: 0 },
        };
        expect(rules(draft("x", { refs: refs({ moves: [wrongSide] }) }))).toContain("wrongSide");
        const illegal: MoveRef = { san: "Qxf8", side: "white", at: { t: "line", id: 2, ply: 0 } };
        expect(rules(draft("x", { refs: refs({ moves: [illegal] }) }))).toContain(
            "moveNotLegalForSide",
        );
    });

    test("drops chess content written into the template", () => {
        expect(rules(draft("Bxe5 wins.", {}, "Bxe5 wins."))).toContain("literalInTemplate");
        expect(rules(draft("mate in 2", {}, "mate in 2"))).toContain("literalInTemplate");
        expect(rules(draft("the knight falls", {}, "the knight falls"))).toContain(
            "literalInTemplate",
        );
        expect(rules(draft("ok", {}, "{{san}} allows {{line}}."))).toStrictEqual([]);
    });

    test("drops jargon, markup and overlong sentences", () => {
        expect(rules(draft("loses 300 centipawns"))).toContain("forbiddenToken");
        expect(rules(draft("a {comment}"))).toContain("forbiddenToken");
        expect(rules(draft("see <b>this</b>"))).toContain("forbiddenToken");
        expect(rules(draft("**bold**"))).toContain("forbiddenToken");
        expect(rules(draft(Array(200).fill("word").join(" ")))).toContain("budget");
    });

    test("one bad sentence does not drop the others", () => {
        const good = draft("Nf6 allows 4.Qxf7#.", { refs: refs({ moves: [reply] }) });
        const bad = draft("x", { factIds: [] });
        const { narration, result } = validateSentences([good, bad], f, selection, run());
        expect(narration.sentences).toHaveLength(1);
        expect(result.dropped).toBe(1);
        expect(result.failures.every((x) => x.sentence === 1)).toBe(true);
    });
});

describe("restoreCommentary", () => {
    test("accepts a stored record and rejects malformed or older ones", () => {
        const record = buildMoveCommentary(f, "??", run()).commentary!;
        const stored = JSON.parse(JSON.stringify(record));
        expect(restoreCommentary(stored)).toStrictEqual(record);
        expect(restoreCommentary({ ...stored, schemaVersion: 0 })).toBeNull();
        expect(
            restoreCommentary({ ...stored, narration: { sentences: [{ text: 1 }] } }),
        ).toBeNull();
        expect(restoreCommentary(undefined)).toBeNull();
    });
});
