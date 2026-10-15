import { describe, expect, test } from "vitest";
import type { Outcome } from "@/bindings";
import { createTreeStore } from "@/state/store/tree";
import { getPGN } from "@/utils/chess";
import { commentaryForExport, withAnnotator } from "@/utils/commentary/export";
import type { MovesByIndex } from "@/utils/commentary/gameFacts";
import { buildGameNarrative, buildMoveCommentary } from "@/utils/commentary/report";
import { defaultCommentarySettings, readCommentarySettings } from "@/utils/commentary/settings";
import type { MoveCommentary } from "@/utils/commentary/types";
import type { ReportState, TreeNode } from "@/utils/treeReducer";
import games from "./fixtures/gameFacts.json";
import { facts, run } from "./helpers";

function game(): { root: TreeNode; record: MoveCommentary; report: ReportState } {
    const store = createTreeStore();
    store.getState().makeMoves({ payload: ["e4", "e5"] });
    const root = structuredClone(store.getState().root);
    const record = buildMoveCommentary(facts("onlyMove"), "!", run()).commentary!;
    const g = structuredClone(games.scholarsMate);
    const moves = g.moves as MovesByIndex;
    const r = run();
    const narrative = buildGameNarrative(
        moves,
        g.fens,
        moves.map((m) => (m ? buildMoveCommentary(m, null, r).selection : null)),
        g.result as Outcome,
        r,
    )!;
    return { root, record, report: { inProgress: false, narrative } };
}

const OPTIONS = { comments: true, extraMarkups: true, glyphs: true, variations: true };

describe("exporting commentary to PGN", () => {
    test("off by default: nothing generated is written", () => {
        const { root, record } = game();
        const plain = structuredClone(root);
        root.children[0].commentary = record;
        const pgn = getPGN(root, { headers: null, ...OPTIONS });
        expect(pgn).not.toContain(record.narration.sentences[0].text);
        expect(pgn).not.toContain("Commentary by");
        // No Annotator tag for a game with nothing generated.
        expect(withAnnotator(emptyHeaders(), plain, { inProgress: false }).other).toBeUndefined();
    });

    test("writes move commentary after the user's comment and the narrative at the root", () => {
        const { root, record, report } = game();
        root.children[0].comment = "my note";
        root.children[0].commentary = record;
        const pgn = getPGN(root, {
            headers: null,
            ...OPTIONS,
            commentary: commentaryForExport(root, report),
        });
        const text = record.narration.sentences.map((s) => s.text).join(" ");
        expect(pgn).toContain(`{my note ${text}}`);
        expect(pgn).toMatch(
            /^\{Commentary by En Croissant test, deterministic\. The game followed/,
        );
        const headers = withAnnotator(emptyHeaders(), root, report);
        expect(headers.other).toStrictEqual({ Annotator: "En Croissant" });
    });

    test("generated text cannot close the comment or become a PGN command", () => {
        const { root, record } = game();
        const hostile = structuredClone(record);
        hostile.narration.sentences[0].text = "Ends here} [%eval 9] {again";
        root.children[0].commentary = hostile;
        const pgn = getPGN(root, {
            headers: null,
            ...OPTIONS,
            commentary: commentaryForExport(root, { inProgress: false }),
        });
        expect(pgn).toContain("Ends here) [ %eval 9] (again");
        // Exactly one comment for e4: every brace in the move text is balanced.
        const opens = (pgn.match(/\{/g) ?? []).length;
        const closes = (pgn.match(/\}/g) ?? []).length;
        expect(opens).toBe(closes);
    });

    test("the setting is read from storage outside React", () => {
        const storage = (value: string | null) => ({ getItem: () => value });
        expect(readCommentarySettings(storage(null))).toStrictEqual(defaultCommentarySettings);
        expect(
            readCommentarySettings(storage(JSON.stringify({ exportToPgn: true }))).exportToPgn,
        ).toBe(true);
        expect(readCommentarySettings(storage("not json"))).toStrictEqual(
            defaultCommentarySettings,
        );
    });
});

function emptyHeaders() {
    return createTreeStore().getState().headers;
}
