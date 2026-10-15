import { z } from "zod";
import type { GameFacts } from "./gameFacts";
import type { CommentaryKind, FactKind, FactVocabulary, GoMode, Perspective } from "@/bindings";
import type { Annotation } from "@/utils/annotation";

export const COMMENTARY_SCHEMA_VERSION = 1 as const;

/**
 * `"<kind>:<ordinal>"` within a move, e.g. `"line:2"` or `"hanging:0"`; line ids are the line's
 * index in `MoveFacts.lines`.
 */
export type FactId = string;

/** One sentence with the facts it cites and every board reference it contains. */
export interface CommentarySentence {
    text: string;
    factIds: FactId[];
    refs: FactVocabulary;
}

declare const validated: unique symbol;
/** Displayable text. Produced only by `validate.ts`. */
export type ValidatedNarration = { sentences: CommentarySentence[] } & {
    readonly [validated]: true;
};

declare const pgnSafe: unique symbol;

/** Text that can be placed inside a PGN `{...}` comment. Produced only by `toPgnSafe`. */
export type PgnSafeText = string & { readonly [pgnSafe]: true };

export type ValidationRule =
    | "unknownFact"
    | "uncitedFact"
    | "slotNotInFacts"
    | "pieceNotOnBoard"
    | "moveNotLegalForSide"
    | "wrongSide"
    | "literalInTemplate"
    | "claimUnsupported"
    | "claimContradicted"
    | "relationWithoutClaim"
    | "magnitude"
    | "sideBetter"
    | "unknownMotif"
    | "forbiddenToken"
    | "budget";

export interface ValidationFailure {
    rule: ValidationRule;
    sentence: number;
    detail: string;
}

export interface ValidationResult {
    checked: number;
    dropped: number;
    failures: ValidationFailure[];
}

/** Who wrote a record, from what. */
export interface Provenance {
    source: "deterministic" | "llm";
    provider?: "anthropic" | "openaiCompatible";
    model?: string;
    usage?: { inputTokens: number; outputTokens: number };
    generatedAt: number;
    appVersion: string;
    factsSchemaVersion: number;
    engine?: { goMode: GoMode; depth: number };
    validation?: ValidationResult;
}

export type ArrowBrush = "green" | "red" | "blue" | "yellow";

export interface CommentaryArrow {
    orig: string;
    dest?: string;
    brush: ArrowBrush;
}

/** The commentary of one move, stored on its tree node. */
export interface MoveCommentary {
    schemaVersion: typeof COMMENTARY_SCHEMA_VERSION;
    kind: CommentaryKind;
    perspective: Perspective;
    /** The only text field: plain text, no markup. */
    narration: ValidatedNarration;
    arrows: CommentaryArrow[];
    fingerprint: string;
    provenance: Provenance;
}

export type ExtendedGlyph = "⊗" | "□" | "⊕";

/** A fact chosen for narration, with the board references it permits. */
export interface SelectedFact {
    id: FactId;
    kind: FactKind;
    weight: number;
    vocabulary: FactVocabulary;
}

/** What one move's commentary is about. */
export interface FactSelection {
    index: number;
    classification: Annotation | null;
    extended: ExtendedGlyph[];
    kind: CommentaryKind;
    facts: SelectedFact[];
    budgetWords: number;
}

const side = z.enum(["white", "black"]);
const piece = z.enum(["pawn", "knight", "bishop", "rook", "queen", "king"]);
const positionRef = z.union([
    z.object({ t: z.literal("before") }),
    z.object({ t: z.literal("after") }),
    z.object({ t: z.literal("line"), id: z.number(), ply: z.number() }),
]);
const vocabulary = z.object({
    moves: z.array(z.object({ san: z.string(), side, at: positionRef })),
    pieces: z.array(z.object({ piece, square: z.string(), at: positionRef })),
    squares: z.array(z.string()),
    numbers: z.array(
        z.object({
            value: z.number(),
            unit: z.enum(["pawns", "mateIn", "winPercent", "move", "eval", "seconds"]),
        }),
    ),
    sides: z.array(side),
});
const commentaryKind = z.enum([
    "mistake",
    "evalDrift",
    "miss",
    "betterMove",
    "onlyMove",
    "clearlyBest",
    "sacrifice",
    "mateFound",
    "threat",
    "opening",
    "novelty",
    "phase",
    "mate",
    "positional",
    "book",
    "best",
    "excellent",
    "good",
    "inaccuracy",
    "error",
    "blunder",
]);

/**
 * The shape of a stored `MoveCommentary`. Session storage keeps no brand, so a record read back is
 * checked against this before it is shown (see `restoreCommentary` in validate.ts).
 */
export const moveCommentarySchema = z.object({
    schemaVersion: z.literal(COMMENTARY_SCHEMA_VERSION),
    kind: commentaryKind,
    perspective: z.enum(["white", "black", "both"]),
    narration: z.object({
        sentences: z.array(
            z.object({ text: z.string(), factIds: z.array(z.string()), refs: vocabulary }),
        ),
    }),
    arrows: z.array(
        z.object({
            orig: z.string(),
            dest: z.string().optional(),
            brush: z.enum(["green", "red", "blue", "yellow"]),
        }),
    ),
    fingerprint: z.string(),
    provenance: z
        .object({
            source: z.enum(["deterministic", "llm"]),
            generatedAt: z.number(),
            appVersion: z.string(),
            factsSchemaVersion: z.number(),
        })
        .passthrough(),
});

/** The game narrative, stored on the report. */
export interface GameNarrative {
    schemaVersion: typeof COMMENTARY_SCHEMA_VERSION;
    facts: GameFacts;
    /** Patterns over several moves, each citing the moves it is about. */
    takeaways: { narration: ValidatedNarration; indices: number[] }[];
    narration: ValidatedNarration;
    fingerprint: string;
    provenance: Provenance;
}

const narrationSchema = moveCommentarySchema.shape.narration;

/** The shape of a stored `GameNarrative` (see `restoreNarrative` in validate.ts). */
export const gameNarrativeSchema = z.object({
    schemaVersion: z.literal(COMMENTARY_SCHEMA_VERSION),
    facts: z.object({ result: z.string(), swings: z.array(z.unknown()) }).passthrough(),
    takeaways: z.array(z.object({ narration: narrationSchema, indices: z.array(z.number()) })),
    narration: narrationSchema,
    fingerprint: z.string(),
    provenance: moveCommentarySchema.shape.provenance,
});
