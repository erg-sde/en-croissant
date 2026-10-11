import { Chess } from "chessops/chess";
import { parsePgn } from "chessops/pgn";
import { parseSan } from "chessops/san";
import { describe, expect, test } from "vitest";
import type { Annotation } from "@/utils/annotation";
import { getMoveText } from "@/utils/chess";
import { toPgnSafe } from "@/utils/commentary/pgnSafe";
import { createNode } from "@/utils/treeReducer";

const HAZARDS = [
    "{",
    "}",
    "[%",
    "[%eval 3.2]",
    "<script>",
    "\u0000",
    "\u001b",
    "\u007f",
    "\n",
    "\t",
    "é",
    "♞",
    "]",
    "%",
    "[",
    " ",
    "a",
    "Nf3",
    "\\",
];

// Deterministic pseudo-random generator so the generated cases are reproducible.
function mulberry32(seed: number) {
    let a = seed;
    return () => {
        a = (a + 0x6d2b79f5) | 0;
        let t = Math.imul(a ^ (a >>> 15), 1 | a);
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}

function hazardousTexts(count: number): string[] {
    const random = mulberry32(42);
    return Array.from({ length: count }, () =>
        Array.from(
            { length: Math.floor(random() * 12) },
            () => HAZARDS[Math.floor(random() * HAZARDS.length)],
        ).join(""),
    );
}

function hasControlCharacter(text: string): boolean {
    return Array.from(text).some((c) => c.charCodeAt(0) < 0x20 && !"\t\n\r".includes(c));
}

describe("toPgnSafe", () => {
    test("replaces braces, breaks commands and drops control characters", () => {
        expect(toPgnSafe("a {b} c")).toBe("a (b) c");
        expect(toPgnSafe("see [%eval 3.2] here")).toBe("see [ %eval 3.2] here");
        expect(toPgnSafe("x\u0000y\u001bz\u007f")).toBe("xyz");
        expect(toPgnSafe("line one\nline two\tend")).toBe("line one\nline two\tend");
        expect(toPgnSafe("<script>alert(1)</script>")).toBe("<script>alert(1)</script>");
    });

    test("output is comment-safe and stable for generated hazardous text", () => {
        for (const text of hazardousTexts(500)) {
            const safe = toPgnSafe(text);
            const label = JSON.stringify(text);
            expect(safe, label).not.toMatch(/[{}]/);
            expect(safe, label).not.toContain("[%");
            expect(hasControlCharacter(safe), label).toBe(false);
            expect(toPgnSafe(safe), label).toBe(safe);
        }
    });

    test("a comment made from generated hazardous text keeps the game intact", () => {
        for (const text of hazardousTexts(200)) {
            const label = JSON.stringify(text);
            const games = parsePgn(`1. e4 {${toPgnSafe(text)}} e5 2. Nf3 *\n\n1. d4 *\n`);
            expect(games, label).toHaveLength(2);
            const mainline = [...games[0].moves.mainline()].map((node) => node.san);
            expect(mainline, label).toStrictEqual(["e4", "e5", "Nf3"]);
        }
    });
});

describe("getMoveText", () => {
    const opt = { glyphs: true, comments: true, extraMarkups: false };

    function e4(annotations: Annotation[], comment = "") {
        const pos = Chess.default();
        const move = parseSan(pos, "e4")!;
        pos.play(move);
        const node = createNode({ fen: "", move, san: "e4", halfMoves: 1 });
        node.annotations = annotations;
        node.comment = comment;
        return node;
    }

    test("writes each basic glyph once, followed by NAGs", () => {
        expect(getMoveText(e4(["?", "N"]), opt)).toBe("1. e4? $146 ");
        expect(getMoveText(e4(["!!"]), opt)).toBe("1. e4!! ");
    });

    test("writes user comments through toPgnSafe", () => {
        expect(getMoveText(e4([], "threat {Qh5} [%eval 9]"), opt)).toBe(
            "1. e4 {threat (Qh5) [ %eval 9]} ",
        );
    });
});
