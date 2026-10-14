import { describe, expect, test } from "vitest";
import type { MoveFacts } from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import type { MovesByIndex } from "@/utils/commentary/gameFacts";
import { buildMoveCommentary } from "@/utils/commentary/report";
import { verdictKind, VERDICT_KINDS } from "@/utils/commentary/verdict";
import games from "./fixtures/gameFacts.json";
import { facts, run, translator } from "./helpers";

// 1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6?? 4. Qxf7#
const GLYPHS: (Annotation | null)[] = [null, null, null, null, null, null, "??", null];

function moves(): MovesByIndex {
    return structuredClone(games.scholarsMate).moves as MovesByIndex;
}

describe("move-by-move verdicts", () => {
    test("every analysed move of a game gets commentary", () => {
        const r = { ...run(), bookUntil: 3 };
        const missing = moves().flatMap((m, i) =>
            m && !buildMoveCommentary(m, GLYPHS[i], r).commentary ? [i] : [],
        );
        expect(missing).toStrictEqual([]);
    });

    test("book moves, best moves and the mistake grades", () => {
        const m = moves();
        expect(verdictKind(m[1]!, null, 3)).toBe("book");
        expect(verdictKind(m[6]!, "??", null)).toBe("blunder");
        expect(verdictKind(m[6]!, "?", null)).toBe("error");
        expect(verdictKind(m[6]!, "?!", null)).toBe("inaccuracy");
        const notTop = { ...m[4]!, played: { ...m[4]!.played, rank: 2 } } as MoveFacts;
        const excellent = { ...notTop, eval: { ...notTop.eval, winDelta: 1 } };
        const good = { ...notTop, eval: { ...notTop.eval, winDelta: 4 } };
        expect(verdictKind(excellent, null, null)).toBe("excellent");
        expect(verdictKind(good, null, null)).toBe("good");
        expect(verdictKind({ ...m[7]!, terminal: true }, null, null)).toBeNull();
    });

    test("a verdict names the move and, when it was not the engine's choice, the better one", () => {
        const r = run();
        const mistakesOnly = { ...r, settings: { ...r.settings, detail: "mistakes" as const } };
        const best = buildMoveCommentary(facts("clearlyBest"), null, mistakesOnly).commentary!;
        expect(best.kind).toBe("best");
        expect(best.narration.sentences.map((s) => s.text)).toStrictEqual([
            "Ra5 is the best move.",
        ]);

        // Qg5 losing only four points of win chance: a good move, with the engine's choice named.
        const f = facts("queenLost");
        const good = buildMoveCommentary(
            { ...f, eval: { ...f.eval, winDelta: 4 } },
            null,
            mistakesOnly,
        ).commentary!;
        expect(good.kind).toBe("good");
        expect(good.narration.sentences.map((s) => s.text)).toStrictEqual([
            "Qg5 is a good move.",
            "The engine preferred Nf6.",
        ]);
        expect(good.provenance.validation?.failures).toStrictEqual([]);
    });

    test("verdicts pass validation in every locale", () => {
        const failures: string[] = [];
        for (const lang of ["en-US", "de-DE", "ru-RU", "zh-CN"]) {
            const r = { ...run(lang, translator(lang)), bookUntil: 3 };
            moves().forEach((m, i) => {
                const record = m ? buildMoveCommentary(m, GLYPHS[i], r).commentary : null;
                for (const f of record?.provenance.validation?.failures ?? []) {
                    failures.push(`${lang} ${i}: ${f.rule} ${f.detail}`);
                }
            });
        }
        expect(failures).toStrictEqual([]);
    });
});
