import { describe, expect, test } from "vitest";
import { narrateMove } from "@/utils/commentary/narrate";
import { enumerateFacts, selectFacts } from "@/utils/commentary/salience";
import { validateSentences } from "@/utils/commentary/validate";
import { facts, run } from "./helpers";

// 3. Bc4, searched with the turn passed back: White threatens 4. Qxf7#.
describe("threat lines", () => {
    test("the threat line is played from the position with the turn passed back", () => {
        const f = facts("threat");
        expect(f.threat).not.toBeNull();
        const threat = enumerateFacts(f, run()).find((e) => e.id === `line:${f.threat}`)!;
        expect(threat.vocabulary.moves).toStrictEqual([
            { san: "Qxf7#", side: "white", at: { t: "line", id: f.threat, ply: 0 } },
        ]);
    });

    test("a good move's threat is narrated and passes validation", () => {
        const f = facts("threat");
        const r = { ...run(), settings: { detail: "allMoves" as const, extendedGlyphs: true } };
        const selection = selectFacts(f, "!", [], "allMoves", r)!;
        expect(selection.facts.map((s) => s.id)).toContain(`line:${f.threat}`);
        const drafts = narrateMove(selection, f, r);
        const { narration, result } = validateSentences(drafts, f, selection, r);
        expect(result.failures).toStrictEqual([]);
        const texts = narration.sentences.map((s) => s.text);
        expect(texts).toContain("Bc4 threatens 4.Qxf7#, mating in 1.");
    });
});
