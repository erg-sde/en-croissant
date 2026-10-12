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
import {
    type EnumeratedGameFact,
    enumerateGameFacts,
    type GameFacts,
    type MovesByIndex,
} from "./gameFacts";
import type { SentenceDraft } from "./narrate";
import { enumerateFacts, linePositions, type SalienceContext } from "./salience";
import {
    type CommentarySentence,
    type FactSelection,
    type GameNarrative,
    gameNarrativeSchema,
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

/** Words a narrative sentence may have before it counts as too long. */
const NARRATIVE_BUDGET = 220;

/** The references a cited narrative fact permits, and the moves they belong to. */
interface Citable {
    vocabulary: FactVocabulary;
    indices: number[];
}

/**
 * Resolves a narrative citation: `game:<kind>:<n>` for a game fact, `<index>/<kind>:<n>` for a
 * fact of one move.
 */
function resolveCitation(
    id: string,
    game: Map<string, EnumeratedGameFact>,
    moves: MovesByIndex,
    ctx: SalienceContext,
): Citable | null {
    const gameFact = game.get(id);
    if (gameFact) {
        return gameFact;
    }
    const match = /^(\d+)\/(.+)$/.exec(id);
    const facts = match ? moves[Number(match[1])] : null;
    const fact = facts ? enumerateFacts(facts, ctx).find((f) => f.id === match![2]) : undefined;
    return fact ? { vocabulary: fact.vocabulary, indices: [Number(match![1])] } : null;
}

/**
 * Validates game narrative sentences. Citations resolve to game facts or to facts of single moves;
 * references must come from the cited facts or be the move played at a cited index; each move is
 * checked against the position it was played in. Rules 4 and 8 apply as for move commentary.
 */
export function validateNarrative(
    drafts: SentenceDraft[],
    facts: GameFacts,
    moves: MovesByIndex,
    ctx: ValidationContext,
): { narration: ValidatedNarration; result: ValidationResult } {
    const game = new Map(enumerateGameFacts(facts, moves, ctx.rootHalfMoves).map((f) => [f.id, f]));
    const failures: ValidationFailure[] = [];
    const kept: CommentarySentence[] = [];

    drafts.forEach((draft, index) => {
        const found: ValidationFailure[] = [];
        const fail = (rule: ValidationFailure["rule"], detail: string) =>
            found.push({ rule, sentence: index, detail });
        const { sentence, template } = draft;

        if (sentence.factIds.length === 0) fail("uncitedFact", "no fact cited");
        const cited: Citable[] = [];
        for (const id of sentence.factIds) {
            const resolved = resolveCitation(id, game, moves, ctx);
            if (resolved) cited.push(resolved);
            else fail("unknownFact", id);
        }
        const indices = cited.flatMap((c) => c.indices);
        const played = indices.flatMap((i) => {
            const m = moves[i];
            return m
                ? [
                      {
                          index: i,
                          ref: { san: m.played.san, side: m.mover, at: { t: "before" as const } },
                      },
                  ]
                : [];
        });

        for (const move of sentence.refs.moves) {
            const allowed =
                played.some((p) => sameMove(p.ref, move)) ||
                cited.some((c) => c.vocabulary.moves.some((m) => sameMove(m, move)));
            if (!allowed) fail("slotNotInFacts", `move ${move.san}`);
            const at = played.find((p) => sameMove(p.ref, move));
            const pos = at ? positionFromFen(moves[at.index]!.fenBefore)[0] : null;
            if (at && (!pos || pos.turn !== move.side || !parseSan(pos, move.san))) {
                fail("moveNotLegalForSide", move.san);
            }
        }
        const vocab = cited.map((c) => c.vocabulary);
        for (const piece of sentence.refs.pieces) {
            if (!vocab.some((v) => v.pieces.some((p) => samePiece(p, piece)))) {
                fail("slotNotInFacts", `piece ${piece.piece} ${piece.square}`);
            }
        }
        for (const square of sentence.refs.squares) {
            if (!vocab.some((v) => v.squares.includes(square)))
                fail("slotNotInFacts", `square ${square}`);
        }
        for (const number of sentence.refs.numbers) {
            if (!vocab.some((v) => v.numbers.some((n) => sameNumber(n, number)))) {
                fail("slotNotInFacts", `number ${number.value} ${number.unit}`);
            }
        }
        for (const side of sentence.refs.sides) {
            const fromMove = played.some((p) => p.ref.side === side);
            if (!fromMove && !vocab.some((v) => v.sides.includes(side)))
                fail("slotNotInFacts", `side ${side}`);
        }

        const literal = template.replace(PLACEHOLDER, "");
        if (SQUARE.test(literal) || DIGIT.test(literal)) fail("literalInTemplate", literal);
        for (const [pattern, name] of FORBIDDEN) {
            if (pattern.test(sentence.text)) fail("forbiddenToken", name);
        }
        if (wordCount(sentence.text, ctx.lang) > NARRATIVE_BUDGET * BUDGET_SLACK) {
            fail("budget", `${wordCount(sentence.text, ctx.lang)} words`);
        }

        if (found.length === 0) kept.push(sentence);
        failures.push(...found);
    });

    return {
        narration: { sentences: kept } as ValidatedNarration,
        result: { checked: drafts.length, dropped: drafts.length - kept.length, failures },
    };
}

/**
 * A stored game narrative read back from session storage, or `null` when it does not have the
 * current shape.
 */
export function restoreNarrative(raw: unknown): GameNarrative | null {
    if (raw === undefined || raw === null) {
        return null;
    }
    return gameNarrativeSchema.safeParse(raw).success ? (raw as GameNarrative) : null;
}
