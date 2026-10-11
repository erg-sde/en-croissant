import { parseUci } from "chessops";
import type { Chess } from "chessops/chess";
import { makeSquare, parseSquare } from "chessops/util";
import type {
    AttackFact,
    CommentaryKind,
    Criticality,
    EvalFacts,
    FactKind,
    FactVocabulary,
    LineFacts,
    Motif,
    MoveFacts,
    MoveRef,
    NumberRef,
    OpeningFacts,
    PieceKind,
    PieceRef,
    PositionRef,
    Side,
    TimeFacts,
} from "@/bindings";
import type { Annotation } from "@/utils/annotation";
import { positionFromFen } from "@/utils/chessops";
import type { CommentaryDetail } from "./settings";
import type { ExtendedGlyph, FactId, FactSelection, SelectedFact } from "./types";

/** Which role a line plays in the commentary of its move. */
export type LineSlot = "Line" | "Best" | "Second" | "Threat";

export type FactPayload =
    | { kind: "line"; line: LineFacts; slot: LineSlot | null }
    | { kind: "hanging"; motif: Extract<Motif, { kind: "hanging" }>; newly: boolean }
    | { kind: "attack"; attack: AttackFact }
    | { kind: "motif"; motif: Motif; line: LineFacts }
    | { kind: "criticality"; criticality: Criticality }
    | { kind: "evalDelta"; eval: EvalFacts }
    | { kind: "opening"; opening: OpeningFacts }
    | { kind: "novelty" }
    | { kind: "time"; time: TimeFacts };

/** A fact with its id, weight and the references it permits. */
export interface EnumeratedFact extends SelectedFact {
    payload: FactPayload;
}

/** Context the facts do not carry. */
export interface SalienceContext {
    /** `halfMoves` of the tree's root, for move numbers. */
    rootHalfMoves: number;
}

// Weights, higher wins (PLAN.md section 6.2).
const W = {
    refutationMate: 100,
    refutationMaterial: 90,
    sacrifice: 85,
    criticality: 80,
    mateTransition: 78,
    secondLine: 75,
    hangingTaken: 70,
    refutationMotif: 65,
    betterMove: 60,
    threatLine: 55,
    novelty: 50,
    opening: 50,
    evalLine: 45,
    passedPawn: 35,
    time: 20,
    attack: 15,
    positional: 10,
    never: 0,
} as const;

/** A positional sentence needs a fact at least this salient. */
const POSITIONAL_MIN_WEIGHT = 35;
const MAX_FACTS = 3;
const MOVE_BUDGET = 60;
const KEY_MOMENT_BUDGET = 140;

function emptyVocabulary(): FactVocabulary {
    return { moves: [], pieces: [], squares: [], numbers: [], sides: [] };
}

function otherSide(side: Side): Side {
    return side === "white" ? "black" : "white";
}

/** The side to move after `plies` plies from a position where `first` moves. */
function sideAfter(first: Side, plies: number): Side {
    return plies % 2 === 0 ? first : otherSide(first);
}

/** The position a line starts from and the side that plays its first move. */
function lineStart(facts: MoveFacts, line: LineFacts): { fen: string; first: Side } {
    return line.origin === "before"
        ? { fen: facts.fenBefore, first: facts.mover }
        : line.origin === "after"
          ? { fen: facts.fenAfter, first: otherSide(facts.mover) }
          : { fen: facts.fenAfter, first: facts.mover };
}

/** The positions of a line: index `k` is the position after `k` plies. */
export function linePositions(facts: MoveFacts, line: LineFacts): Chess[] {
    const [start] = positionFromFen(lineStart(facts, line).fen);
    if (!start) {
        return [];
    }
    const positions = [start.clone()];
    for (const uci of line.pvUci) {
        const move = parseUci(uci);
        const current = positions[positions.length - 1].clone();
        if (!move || !current.isLegal(move)) {
            break;
        }
        current.play(move);
        positions.push(current);
    }
    return positions;
}

function lineMoves(facts: MoveFacts, line: LineFacts, from: number, to: number): MoveRef[] {
    const { first } = lineStart(facts, line);
    return line.pvSan.slice(from, to).map((san, offset) => ({
        san,
        side: sideAfter(first, from + offset),
        at: { t: "line", id: line.id, ply: from + offset },
    }));
}

function pieceAt(pos: Chess | undefined, square: string): PieceKind | null {
    const index = parseSquare(square);
    const piece = pos && index !== undefined ? pos.board.get(index) : undefined;
    return piece ? piece.role : null;
}

/** The first shown position of `line` where every square in `squares` is occupied. */
function motifPly(positions: Chess[], shown: number, squares: string[]): number {
    for (let ply = 1; ply <= Math.min(shown, positions.length - 1); ply++) {
        if (squares.every((sq) => pieceAt(positions[ply], sq) !== null)) {
            return ply;
        }
    }
    return Math.min(shown, positions.length - 1);
}

function motifSquares(motif: Motif): string[] {
    switch (motif.kind) {
        case "fork":
            return [motif.from, ...motif.targets.map((t) => t.square)];
        case "pin":
            return [motif.attacker, motif.pinned, motif.behind];
        case "skewer":
            return [motif.attacker, motif.front, motif.behind];
        case "discoveredAttack":
            return [motif.revealed, motif.target];
        case "hanging":
        case "trappedPiece":
        case "passedPawn":
        case "promotion":
            return [motif.square];
        case "backRank":
            return [motif.king];
        case "mateThreat":
            return [motif.from];
    }
}

/** Pieces and squares of a motif, evaluated at `at` on `pos`. */
function motifVocabulary(motif: Motif, pos: Chess | undefined, at: PositionRef): FactVocabulary {
    const vocabulary = emptyVocabulary();
    for (const square of motifSquares(motif)) {
        vocabulary.squares.push(square);
        const piece = pieceAt(pos, square);
        if (piece) {
            vocabulary.pieces.push({ piece, square, at });
        }
    }
    if (motif.kind === "mateThreat") {
        vocabulary.squares.push(motif.to);
    }
    if (motif.kind === "hanging") {
        vocabulary.sides.push(motif.side);
        vocabulary.numbers.push({ value: -motif.see, unit: "pawns" });
    }
    return vocabulary;
}

/** Where the line's first mover captures `piece`, for the board reference of the piece won. */
function capturedPiece(
    line: LineFacts,
    positions: Chess[],
    piece: PieceKind,
    atPly: number,
): PieceRef | null {
    for (let ply = 0; ply < Math.min(atPly, line.pvUci.length); ply += 2) {
        const move = parseUci(line.pvUci[ply]);
        if (move && "from" in move && pieceAt(positions[ply], makeSquare(move.to)) === piece) {
            return { piece, square: makeSquare(move.to), at: { t: "line", id: line.id, ply } };
        }
    }
    return null;
}

function mateSide(line: LineFacts): Side | null {
    const value = line.eval.value;
    if (value.type !== "mate" || value.value === 0) {
        return null;
    }
    return value.value > 0 ? "white" : "black";
}

function lineVocabulary(facts: MoveFacts, line: LineFacts): FactVocabulary {
    const positions = linePositions(facts, line);
    const vocabulary = emptyVocabulary();
    vocabulary.moves = lineMoves(facts, line, 0, line.shownPlies);
    vocabulary.sides = [...new Set(vocabulary.moves.map((m) => m.side))];
    if (line.eval.value.type === "cp") {
        vocabulary.numbers.push({ value: line.eval.value.value, unit: "eval" });
    }
    const outcome = line.outcome;
    if (outcome.kind === "mate") {
        vocabulary.numbers.push({ value: outcome.mateIn, unit: "mateIn" });
        const side = mateSide(line);
        if (side && !vocabulary.sides.includes(side)) {
            vocabulary.sides.push(side);
        }
    } else if (outcome.kind === "material") {
        vocabulary.numbers.push({ value: outcome.pawns, unit: "pawns" });
        if (outcome.piece) {
            const piece = capturedPiece(line, positions, outcome.piece, outcome.atPly);
            if (piece) {
                vocabulary.pieces.push(piece);
                vocabulary.squares.push(piece.square);
            }
        }
    }
    return vocabulary;
}

function criticalityVocabulary(facts: MoveFacts, c: Criticality): FactVocabulary {
    const vocabulary = emptyVocabulary();
    const percent = (value: number): NumberRef => ({ value, unit: "winPercent" });
    switch (c.kind) {
        case "onlyMove":
            vocabulary.numbers.push(percent(c.margin), percent(c.secondWin));
            break;
        case "clearlyBest":
            vocabulary.numbers.push(percent(c.margin));
            break;
        case "mateFound":
        case "mateCreated":
            vocabulary.numbers.push({ value: c.mateIn, unit: "mateIn" });
            break;
        case "mateLost":
            vocabulary.numbers.push({ value: c.wasMateIn, unit: "mateIn" });
            break;
        case "mateDelayed":
            vocabulary.numbers.push(
                { value: c.from, unit: "mateIn" },
                { value: c.to, unit: "mateIn" },
            );
            break;
        case "miss":
            vocabulary.numbers.push(percent(c.gift), percent(c.givenBack));
            break;
        case "sacrifice":
            vocabulary.pieces.push({ piece: c.piece, square: c.square, at: { t: "after" } });
            vocabulary.squares.push(c.square);
            vocabulary.numbers.push({ value: -c.see, unit: "pawns" });
            break;
    }
    vocabulary.sides.push(facts.mover);
    return vocabulary;
}

function lineSlot(facts: MoveFacts, id: number): LineSlot | null {
    if (id === facts.refutation) return "Line";
    if (id === facts.best) return "Best";
    // When the move played was the engine's second choice, that line is the move itself.
    if (id === facts.second && facts.played.rank !== 2) return "Second";
    if (id === facts.threat) return "Threat";
    return null;
}

function lineWeight(line: LineFacts, slot: LineSlot | null): number {
    switch (slot) {
        case "Line":
            return line.outcome.kind === "mate"
                ? W.refutationMate
                : line.outcome.kind === "material"
                  ? W.refutationMaterial
                  : W.evalLine;
        case "Best":
            return W.betterMove;
        case "Second":
            return W.secondLine;
        case "Threat":
            return line.outcome.kind === "eval" ? W.never : W.threatLine;
        default:
            return W.never;
    }
}

function criticalityWeight(c: Criticality): number {
    switch (c.kind) {
        case "sacrifice":
            return W.sacrifice;
        case "onlyMove":
        case "clearlyBest":
        case "mateFound":
        case "miss":
            return W.criticality;
        case "mateCreated":
        case "mateLost":
        case "mateDelayed":
            return W.mateTransition;
    }
}

function motifWeight(motif: Motif): number {
    return motif.kind === "passedPawn" || motif.kind === "promotion"
        ? W.passedPawn
        : W.refutationMotif;
}

/** Whether the opponent's reply captures on `square` within its shown plies. */
function refutationTakes(facts: MoveFacts, square: string): boolean {
    if (facts.refutation === null) {
        return false;
    }
    const line = facts.lines[facts.refutation];
    return line.pvUci.slice(0, line.shownPlies).some((uci) => uci.slice(2, 4) === square);
}

function sameMotif(a: Motif, b: Motif): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * Every fact of a move, with ids in a fixed order: lines, hanging pieces, new attacks, newly
 * hanging pieces, motifs on the opponent's reply, criticality, the evaluation change, the opening,
 * the novelty and the clock.
 */
export function enumerateFacts(facts: MoveFacts, ctx: SalienceContext): EnumeratedFact[] {
    const out: EnumeratedFact[] = [];
    const counters = new Map<FactKind, number>();
    const add = (
        kind: FactKind,
        weight: number,
        vocabulary: FactVocabulary,
        payload: FactPayload,
        id?: FactId,
    ) => {
        const ordinal = counters.get(kind) ?? 0;
        counters.set(kind, ordinal + 1);
        out.push({ id: id ?? `${kind}:${ordinal}`, kind, weight, vocabulary, payload });
    };

    for (const line of facts.lines) {
        const slot = lineSlot(facts, line.id);
        add(
            "line",
            lineWeight(line, slot),
            lineVocabulary(facts, line),
            { kind: "line", line, slot },
            `line:${line.id}`,
        );
    }
    const [after] = positionFromFen(facts.fenAfter);
    const newlyHanging = facts.delta.newlyHanging.filter(
        (m): m is Extract<Motif, { kind: "hanging" }> => m.kind === "hanging",
    );
    for (const motif of facts.after.hanging) {
        if (motif.kind !== "hanging" || newlyHanging.some((m) => sameMotif(m, motif))) {
            continue;
        }
        add("hanging", W.never, motifVocabulary(motif, after ?? undefined, { t: "after" }), {
            kind: "hanging",
            motif,
            newly: false,
        });
    }
    for (const attack of facts.delta.newlyAttacked) {
        const vocabulary = emptyVocabulary();
        vocabulary.pieces.push(
            { piece: attack.attacker, square: attack.from, at: { t: "after" } },
            { piece: attack.target, square: attack.to, at: { t: "after" } },
        );
        vocabulary.squares.push(attack.from, attack.to);
        add("attack", W.attack, vocabulary, { kind: "attack", attack });
    }
    for (const motif of newlyHanging) {
        const weight = refutationTakes(facts, motif.square) ? W.hangingTaken : W.never;
        add("hanging", weight, motifVocabulary(motif, after ?? undefined, { t: "after" }), {
            kind: "hanging",
            motif,
            newly: true,
        });
    }
    if (facts.refutation !== null) {
        const line = facts.lines[facts.refutation];
        const positions = linePositions(facts, line);
        for (const motif of line.motifs) {
            const ply = motifPly(positions, line.shownPlies, motifSquares(motif));
            add(
                "motif",
                motifWeight(motif),
                motifVocabulary(motif, positions[ply], { t: "line", id: line.id, ply }),
                { kind: "motif", motif, line },
            );
        }
    }
    for (const c of facts.criticality) {
        add("criticality", criticalityWeight(c), criticalityVocabulary(facts, c), {
            kind: "criticality",
            criticality: c,
        });
    }
    const evalVocabulary = emptyVocabulary();
    if (facts.eval.winDelta !== null) {
        evalVocabulary.numbers.push({ value: Math.abs(facts.eval.winDelta), unit: "winPercent" });
    }
    if (facts.eval.after.value.type === "cp") {
        evalVocabulary.numbers.push({ value: facts.eval.after.value.value, unit: "eval" });
    }
    add("evalDelta", W.positional, evalVocabulary, { kind: "evalDelta", eval: facts.eval });
    if (facts.opening) {
        const vocabulary = emptyVocabulary();
        const move = Math.ceil((ctx.rootHalfMoves + facts.opening.lastNamedIndex) / 2);
        vocabulary.numbers.push({ value: move, unit: "move" });
        add("opening", W.opening, vocabulary, { kind: "opening", opening: facts.opening });
    }
    if (facts.novelty) {
        add("novelty", W.novelty, emptyVocabulary(), { kind: "novelty" });
    }
    if (facts.time) {
        const vocabulary = emptyVocabulary();
        vocabulary.numbers.push({ value: Math.round(facts.time.remainingS), unit: "seconds" });
        add("time", W.time, vocabulary, { kind: "time", time: facts.time });
    }
    return out;
}

/** The references a fact permits. */
export function vocabularyOf(fact: EnumeratedFact): FactVocabulary {
    return fact.vocabulary;
}

function criticalityOf(facts: MoveFacts, kind: Criticality["kind"]): Criticality | undefined {
    return facts.criticality.find((c) => c.kind === kind);
}

const MATE_TRANSITIONS: Criticality["kind"][] = ["mateCreated", "mateLost", "mateDelayed"];

/** Why the move gets commentary, or `null` when it does not at this detail level. */
export function decideKind(
    facts: MoveFacts,
    glyph: Annotation | null,
    extended: ExtendedGlyph[],
    detail: CommentaryDetail,
): CommentaryKind | null {
    const has = (kind: Criticality["kind"]) => criticalityOf(facts, kind) !== undefined;
    const mistakeKind: CommentaryKind = facts.played.rank === 1 ? "evalDrift" : "mistake";

    if (facts.terminal && facts.played.givesMate) return detail === "mistakes" ? null : "mate";
    if (extended.includes("⊗")) return "miss";
    if (glyph === "??" || glyph === "?" || glyph === "?!") return mistakeKind;
    if (detail === "mistakes") return null;
    if (glyph === "!!") {
        if (has("sacrifice")) return "sacrifice";
        if (has("onlyMove")) return "onlyMove";
        if (has("clearlyBest")) return "clearlyBest";
    }
    if (glyph === "!" || glyph === "!!" || extended.includes("□")) {
        if (has("onlyMove")) return "onlyMove";
        if (has("clearlyBest")) return "clearlyBest";
    }
    if (glyph === "!?") {
        if (has("sacrifice")) return "sacrifice";
        if (has("mateFound")) return "mateFound";
    }
    if (has("mateFound")) return "mateFound";
    if (MATE_TRANSITIONS.some(has)) return mistakeKind;
    if (facts.opening) return "opening";
    if (facts.novelty) return "novelty";
    if (glyph === "!" || glyph === "!!" || glyph === "!?") return "positional";
    return detail === "allMoves" ? "positional" : null;
}

/** The facts a kind cannot be narrated without; `null` when one is missing. */
function mandatoryFacts(
    kind: CommentaryKind,
    facts: MoveFacts,
    enumerated: EnumeratedFact[],
): EnumeratedFact[] | null {
    const byId = (id: FactId | null) => enumerated.find((f) => f.id === id) ?? null;
    const lineId = (index: number | null) => (index === null ? null : `line:${index}`);
    const crit = (k: Criticality["kind"]) =>
        enumerated.find(
            (f) => f.payload.kind === "criticality" && f.payload.criticality.kind === k,
        ) ?? null;
    const required = (...items: (EnumeratedFact | null)[]) =>
        items.every((f) => f !== null) ? (items as EnumeratedFact[]) : null;
    const optional = (...items: (EnumeratedFact | null)[]) =>
        items.filter((f): f is EnumeratedFact => f !== null);

    switch (kind) {
        case "mistake":
            return required(byId(lineId(facts.refutation)), byId(lineId(facts.best)));
        case "evalDrift":
            return required(byId(lineId(facts.refutation)));
        case "miss":
            return required(crit("miss"));
        case "sacrifice": {
            const base = required(crit("sacrifice"));
            return (
                base && [
                    ...base,
                    ...optional(
                        byId(lineId(facts.second)),
                        crit("onlyMove") ?? crit("clearlyBest"),
                    ),
                ]
            );
        }
        case "onlyMove":
        case "clearlyBest": {
            const base = required(crit(kind));
            return (
                base && [
                    ...base,
                    ...optional(byId(lineId(facts.second)), byId(lineId(facts.threat))),
                ]
            );
        }
        case "mateFound": {
            const played = facts.played.rank !== null ? facts.played.rank - 1 : facts.refutation;
            return required(crit("mateFound"), byId(lineId(played)));
        }
        case "opening":
            return required(enumerated.find((f) => f.kind === "opening") ?? null);
        case "novelty":
            return required(enumerated.find((f) => f.kind === "novelty") ?? null);
        case "mate":
            return required(enumerated.find((f) => f.kind === "evalDelta") ?? null);
        default:
            return [];
    }
}

const KEY_MOMENTS: CommentaryKind[] = [
    "mistake",
    "miss",
    "sacrifice",
    "onlyMove",
    "clearlyBest",
    "mateFound",
];

/** Kinds whose verdict is about the alternatives, where the second line explains it. */
const ABOUT_ALTERNATIVES: CommentaryKind[] = ["onlyMove", "clearlyBest", "sacrifice"];

/**
 * Whether `fact` may be added after the mandatory facts of a `kind` commentary. A mate, an opening
 * or a novelty is said on its own; the second line belongs to verdicts about the alternatives; and
 * a mate transition adds nothing once a chosen line already shows the mate.
 */
function mayFill(kind: CommentaryKind, fact: EnumeratedFact, chosen: EnumeratedFact[]): boolean {
    if (kind === "mate") {
        return false;
    }
    if (kind === "opening" || kind === "novelty") {
        return fact.kind === "opening" || fact.kind === "novelty";
    }
    const payload = fact.payload;
    if (payload.kind === "line" && payload.slot === "Second") {
        return ABOUT_ALTERNATIVES.includes(kind);
    }
    if (payload.kind === "criticality" && payload.criticality.kind.startsWith("mate")) {
        return !chosen.some(
            (c) => c.payload.kind === "line" && c.payload.line.outcome.kind === "mate",
        );
    }
    return true;
}

/**
 * What one move's commentary is about: its kind and at most three facts, the facts its kind cannot
 * do without first, then the most salient others. `null` when the move gets no commentary at this
 * detail level, or when a fact its kind requires is missing (no verdict without its reason).
 */
export function selectFacts(
    facts: MoveFacts,
    glyph: Annotation | null,
    extended: ExtendedGlyph[],
    detail: CommentaryDetail,
    ctx: SalienceContext,
): FactSelection | null {
    const kind = decideKind(facts, glyph, extended, detail);
    if (kind === null) {
        return null;
    }
    const enumerated = enumerateFacts(facts, ctx);
    const mandatory = mandatoryFacts(kind, facts, enumerated);
    if (mandatory === null) {
        return null;
    }
    const chosen = [...new Map(mandatory.map((f) => [f.id, f])).values()].slice(0, MAX_FACTS);
    const rest = enumerated
        .filter((f) => f.weight > W.never && !chosen.some((c) => c.id === f.id))
        .sort((a, b) => b.weight - a.weight);
    if (kind === "positional" && (rest[0]?.weight ?? 0) < POSITIONAL_MIN_WEIGHT) {
        return null;
    }
    for (const fact of rest) {
        if (chosen.length >= MAX_FACTS) break;
        if (mayFill(kind, fact, chosen)) {
            chosen.push(fact);
        }
    }
    return {
        index: facts.index,
        classification: glyph,
        extended,
        kind,
        facts: chosen.map(({ id, kind, weight, vocabulary }) => ({ id, kind, weight, vocabulary })),
        budgetWords: KEY_MOMENTS.includes(kind) ? KEY_MOMENT_BUDGET : MOVE_BUDGET,
    };
}
