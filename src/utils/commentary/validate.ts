import type { Chess } from "chessops/chess";
import { parseSan } from "chessops/san";
import { parseSquare } from "chessops/util";
import type {
    FactVocabulary,
    MoveFacts,
    MoveRef,
    NumberRef,
    PieceRef,
    PositionRef,
} from "@/bindings";
import { positionFromFen } from "@/utils/chessops";
import type { SentenceDraft } from "./narrate";
import { enumerateFacts, linePositions, type SalienceContext } from "./salience";
import {
    type CommentarySentence,
    type FactSelection,
    type MoveCommentary,
    moveCommentarySchema,
    type ValidatedNarration,
    type ValidationFailure,
    type ValidationResult,
} from "./types";

/** Budgets allow this much slack before a sentence counts as too long. */
const BUDGET_SLACK = 1.2;

export interface ValidationContext extends SalienceContext {
    lang: string;
    /** Piece names of the request language, which templates must not contain as literal text. */
    pieceWords: string[];
}

const FORBIDDEN: [RegExp, string][] = [
    [/centipawn/i, "centipawn"],
    [/\bcp\b/i, "cp"],
    [/[{}]/, "brace"],
    [/\[%/, "PGN command"],
    [/[<>]/, "angle bracket"],
    [/https?:\/\//i, "URL"],
    [/\*\*|__|`/, "markdown"],
];

const SQUARE = /[a-h][1-8]/;
const DIGIT = /\d/;
const PLACEHOLDER = /\{\{\w+\}\}/g;

function sameMove(a: MoveRef, b: MoveRef): boolean {
    return a.san === b.san && a.side === b.side && samePositionRef(a.at, b.at);
}

function samePositionRef(a: PositionRef, b: PositionRef): boolean {
    return a.t === b.t && (a.t !== "line" || (b.t === "line" && a.id === b.id && a.ply === b.ply));
}

function samePiece(a: PieceRef, b: PieceRef): boolean {
    return a.piece === b.piece && a.square === b.square && samePositionRef(a.at, b.at);
}

function sameNumber(a: NumberRef, b: NumberRef): boolean {
    return a.unit === b.unit && Math.abs(a.value - b.value) < 1e-6;
}

/** Positions of a move's facts, resolved lazily. */
class Boards {
    private lines = new Map<number, Chess[]>();

    constructor(private facts: MoveFacts) {}

    at(ref: PositionRef): Chess | null {
        if (ref.t === "line") {
            const line = this.facts.lines[ref.id];
            if (!line) return null;
            if (!this.lines.has(ref.id)) {
                this.lines.set(ref.id, linePositions(this.facts, line));
            }
            return this.lines.get(ref.id)?.[ref.ply] ?? null;
        }
        const [pos] = positionFromFen(
            ref.t === "before" ? this.facts.fenBefore : this.facts.fenAfter,
        );
        return pos;
    }
}

function wordCount(text: string, lang: string): number {
    const segmenter = new Intl.Segmenter(lang, { granularity: "word" });
    return [...segmenter.segment(text)].filter((s) => s.isWordLike).length;
}

/** The failures of one sentence, in rule order. */
function check(
    draft: SentenceDraft,
    index: number,
    allowed: Map<string, FactVocabulary>,
    played: MoveRef,
    boards: Boards,
    selection: FactSelection,
    ctx: ValidationContext,
): ValidationFailure[] {
    const { sentence, template } = draft;
    const failures: ValidationFailure[] = [];
    const fail = (rule: ValidationFailure["rule"], detail: string) =>
        failures.push({ rule, sentence: index, detail });

    // 1. Citations.
    if (sentence.factIds.length === 0) {
        fail("uncitedFact", "no fact cited");
    }
    const cited = sentence.factIds.map((id) => allowed.get(id));
    sentence.factIds.forEach((id, i) => {
        if (!cited[i]) fail("unknownFact", id);
    });
    const vocab = cited.filter((v): v is FactVocabulary => v !== undefined);

    // 2. Every reference comes from a cited fact (the played move is always allowed).
    const refs = sentence.refs;
    for (const move of refs.moves) {
        if (!sameMove(move, played) && !vocab.some((v) => v.moves.some((m) => sameMove(m, move)))) {
            fail("slotNotInFacts", `move ${move.san}`);
        }
    }
    for (const piece of refs.pieces) {
        if (!vocab.some((v) => v.pieces.some((p) => samePiece(p, piece)))) {
            fail("slotNotInFacts", `piece ${piece.piece} ${piece.square}`);
        }
    }
    for (const square of refs.squares) {
        if (!vocab.some((v) => v.squares.includes(square))) {
            fail("slotNotInFacts", `square ${square}`);
        }
    }
    for (const number of refs.numbers) {
        if (!vocab.some((v) => v.numbers.some((n) => sameNumber(n, number)))) {
            fail("slotNotInFacts", `number ${number.value} ${number.unit}`);
        }
    }
    for (const side of refs.sides) {
        if (!vocab.some((v) => v.sides.includes(side))) {
            fail("slotNotInFacts", `side ${side}`);
        }
    }

    // 3. References agree with the board.
    for (const piece of refs.pieces) {
        const square = parseSquare(piece.square);
        const found = square !== undefined ? boards.at(piece.at)?.board.get(square) : undefined;
        if (!found || found.role !== piece.piece) {
            fail("pieceNotOnBoard", `${piece.piece} ${piece.square}`);
        }
    }
    for (const move of [...refs.moves]) {
        const pos = boards.at(move.at);
        if (!pos) {
            fail("moveNotLegalForSide", `${move.san}: no position`);
        } else if (pos.turn !== move.side) {
            fail("wrongSide", `${move.san} is not ${move.side}'s move`);
        } else if (!parseSan(pos, move.san)) {
            fail("moveNotLegalForSide", move.san);
        }
    }

    // 4. The template carries no chess content of its own.
    const literal = template.replace(PLACEHOLDER, "");
    if (SQUARE.test(literal) || DIGIT.test(literal)) {
        fail("literalInTemplate", literal);
    }
    const lower = literal.toLocaleLowerCase(ctx.lang);
    for (const word of ctx.pieceWords) {
        if (
            word &&
            new RegExp(`(^|[^\\p{L}])${word.toLocaleLowerCase(ctx.lang)}($|[^\\p{L}])`, "u").test(
                lower,
            )
        ) {
            fail("literalInTemplate", word);
        }
    }

    // 8. No markup or jargon, and within budget.
    for (const [pattern, name] of FORBIDDEN) {
        if (pattern.test(sentence.text)) fail("forbiddenToken", name);
    }
    if (wordCount(sentence.text, ctx.lang) > selection.budgetWords * BUDGET_SLACK) {
        fail("budget", `${wordCount(sentence.text, ctx.lang)} words`);
    }
    return failures;
}

/**
 * Checks sentences against the facts they cite and the board, drops the ones that fail, and mints
 * the rest as `ValidatedNarration`. This is the only code that creates that type.
 *
 * Implemented here: citations, references within the cited facts' vocabulary, board agreement,
 * literal chess content in templates, forbidden tokens and the word budget. The claim and lexicon
 * rules for model-written text arrive with it.
 */
export function validateSentences(
    drafts: SentenceDraft[],
    facts: MoveFacts,
    selection: FactSelection,
    ctx: ValidationContext,
): { narration: ValidatedNarration; result: ValidationResult } {
    const allowed = new Map(enumerateFacts(facts, ctx).map((f) => [f.id, f.vocabulary]));
    const played: MoveRef = { san: facts.played.san, side: facts.mover, at: { t: "before" } };
    const boards = new Boards(facts);
    const failures: ValidationFailure[] = [];
    const kept: CommentarySentence[] = [];
    drafts.forEach((draft, i) => {
        const found = check(draft, i, allowed, played, boards, selection, ctx);
        if (found.length === 0) {
            kept.push(draft.sentence);
        }
        failures.push(...found);
    });
    return {
        narration: { sentences: kept } as ValidatedNarration,
        result: {
            checked: drafts.length,
            dropped: drafts.length - kept.length,
            failures,
        },
    };
}

/**
 * A stored commentary record read back from session storage, or `null` when it does not have the
 * current shape. Records are validated when written; storage keeps no brand, so a well-formed
 * record is re-branded here.
 */
export function restoreCommentary(raw: unknown): MoveCommentary | null {
    if (raw === undefined || raw === null) {
        return null;
    }
    const parsed = moveCommentarySchema.safeParse(raw);
    if (!parsed.success) {
        return null;
    }
    return raw as MoveCommentary;
}
