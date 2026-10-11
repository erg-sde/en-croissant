import type { LineFacts, Motif, MoveFacts, MoveRef, PieceRef } from "@/bindings";
import {
    type EnumeratedFact,
    enumerateFacts,
    type LineSlot,
    type SalienceContext,
} from "./salience";
import { type Segmented, segmentize, type Token, type Translate } from "./segmentize";
import type { CommentaryArrow, CommentarySentence, FactSelection } from "./types";

/** A sentence and the translated template it was rendered from, for validation. */
export interface SentenceDraft {
    template: string;
    sentence: CommentarySentence;
}

export interface NarrationContext extends SalienceContext {
    t: Translate;
    lang: string;
}

function capitalized(kind: string): string {
    return kind.charAt(0).toUpperCase() + kind.slice(1);
}

function playedMove(facts: MoveFacts): MoveRef {
    return { san: facts.played.san, side: facts.mover, at: { t: "before" } };
}

/** Plies before the first move of a line, counted like `TreeNode.halfMoves`. */
function lineStartHalfMoves(facts: MoveFacts, line: LineFacts, ctx: SalienceContext): number {
    const beforeMove = ctx.rootHalfMoves + facts.index - 1;
    return line.origin === "before" ? beforeMove : beforeMove + 1;
}

function lineOutcomeKey(line: LineFacts): string {
    switch (line.outcome.kind) {
        case "mate":
            return "Mate";
        case "material":
            return line.outcome.piece ? "Material" : "MaterialGain";
        case "eval":
            return "Eval";
    }
}

function numberToken(fact: EnumeratedFact, unit: string, nth = 0): Token | null {
    const ref = fact.vocabulary.numbers.filter((n) => n.unit === unit)[nth];
    return ref ? { kind: "number", ref } : null;
}

function pieceRef(fact: EnumeratedFact, square: string): PieceRef | undefined {
    return fact.vocabulary.pieces.find((p) => p.square === square);
}

function pieceToken(fact: EnumeratedFact, square: string): Token {
    const ref = pieceRef(fact, square);
    return ref ? { kind: "piece", ref } : { kind: "square", square };
}

function compact(values: Record<string, Token | null>): Record<string, Token> {
    return Object.fromEntries(
        Object.entries(values).filter((entry): entry is [string, Token] => entry[1] !== null),
    );
}

/** The side that mates in a mating line. */
function matingSide(line: LineFacts): Token | null {
    const value = line.eval.value;
    if (value.type !== "mate" || value.value === 0) {
        return null;
    }
    return { kind: "side", side: value.value > 0 ? "white" : "black" };
}

function lineSentence(
    facts: MoveFacts,
    fact: EnumeratedFact,
    line: LineFacts,
    slot: LineSlot,
    selection: FactSelection,
    ctx: NarrationContext,
): Segmented {
    const moves = fact.vocabulary.moves;
    const start = lineStartHalfMoves(facts, line, ctx);
    const outcome = lineOutcomeKey(line);
    // An alternative is named by its first move, and its line is what follows.
    const rest = slot === "Second" || slot === "Best";
    const shown = rest && moves.length > 1 ? moves.slice(1) : moves;
    const piece = fact.vocabulary.pieces[0];
    const values = compact({
        san: { kind: "move", ref: playedMove(facts) },
        line: {
            kind: "line",
            moves: shown,
            startHalfMoves: rest && moves.length > 1 ? start + 1 : start,
        },
        best: moves[0] ? { kind: "move", ref: moves[0] } : null,
        secondSan: moves[0] ? { kind: "move", ref: moves[0] } : null,
        piece: piece ? { kind: "pieceName", piece: piece.piece, ref: piece } : null,
        mateIn: numberToken(fact, "mateIn"),
        pawns: numberToken(fact, "pawns"),
        eval: numberToken(fact, "eval"),
        side: matingSide(line),
    });
    const kind = capitalized(selection.kind);
    return segmentize(
        [`Commentary.${kind}.${slot}.${outcome}`, `Commentary.Any.${slot}.${outcome}`],
        values,
        ctx.t,
        ctx.lang,
    );
}

function motifValues(fact: EnumeratedFact, motif: Motif): Record<string, Token> {
    const piece = (square: string) => pieceToken(fact, square);
    switch (motif.kind) {
        case "fork":
            return {
                attacker: piece(motif.from),
                list: { kind: "list", items: motif.targets.map((t) => piece(t.square)) },
            };
        case "pin":
            return {
                attacker: piece(motif.attacker),
                pinned: piece(motif.pinned),
                behind: piece(motif.behind),
            };
        case "skewer":
            return {
                attacker: piece(motif.attacker),
                front: piece(motif.front),
                behind: piece(motif.behind),
            };
        case "discoveredAttack":
            return { revealed: piece(motif.revealed), target: piece(motif.target) };
        case "hanging":
        case "trappedPiece":
        case "passedPawn":
            return { piece: piece(motif.square) };
        case "backRank":
            return { piece: piece(motif.king) };
        case "mateThreat":
            return { square: { kind: "square", square: motif.to } };
        case "promotion":
            return {
                piece: { kind: "pieceName", piece: motif.to },
                square: { kind: "square", square: motif.square },
            };
    }
}

function motifSentence(fact: EnumeratedFact, motif: Motif, ctx: NarrationContext): Segmented {
    const key = `Commentary.Motif.${capitalized(motif.kind)}`;
    const suffix = motif.kind === "discoveredAttack" && motif.isCheck ? "Check" : "";
    return segmentize([`${key}${suffix}`, key], motifValues(fact, motif), ctx.t, ctx.lang);
}

function factSentence(
    facts: MoveFacts,
    fact: EnumeratedFact,
    selection: FactSelection,
    ctx: NarrationContext,
): Segmented | null {
    const san: Token = { kind: "move", ref: playedMove(facts) };
    const seg = (key: string, values: Record<string, Token | null>) =>
        segmentize([key], compact(values), ctx.t, ctx.lang);
    const payload = fact.payload;
    switch (payload.kind) {
        case "line": {
            // The mating line of a found mate can be the played move's own line, which has no slot.
            const played = facts.played.rank !== null && payload.line.id === facts.played.rank - 1;
            const slot = payload.slot ?? (selection.kind === "mateFound" && played ? "Line" : null);
            return slot ? lineSentence(facts, fact, payload.line, slot, selection, ctx) : null;
        }
        case "criticality": {
            const c = payload.criticality;
            switch (c.kind) {
                case "onlyMove":
                case "clearlyBest":
                    return seg(`Commentary.${capitalized(c.kind)}`, {
                        san,
                        margin: numberToken(fact, "winPercent"),
                    });
                case "sacrifice": {
                    const ref = pieceRef(fact, c.square);
                    return seg("Commentary.Sacrifice", {
                        san,
                        piece: ref ? { kind: "pieceName", piece: ref.piece, ref } : null,
                        square: { kind: "square", square: c.square },
                    });
                }
                case "mateFound":
                case "mateCreated":
                case "mateLost":
                    return seg(`Commentary.${capitalized(c.kind)}`, {
                        san,
                        mateIn: numberToken(fact, "mateIn"),
                    });
                case "mateDelayed":
                    return seg("Commentary.MateDelayed", {
                        san,
                        wasMateIn: numberToken(fact, "mateIn", 0),
                        mateIn: numberToken(fact, "mateIn", 1),
                    });
                case "miss":
                    return seg("Commentary.Miss", {
                        san,
                        gift: numberToken(fact, "winPercent", 0),
                        givenBack: numberToken(fact, "winPercent", 1),
                    });
            }
            return null;
        }
        case "motif":
            return motifSentence(fact, payload.motif, ctx);
        case "hanging":
            return seg("Commentary.Hanging", {
                san,
                piece: pieceToken(fact, payload.motif.square),
            });
        case "attack":
            return seg("Commentary.Attack", { san, target: pieceToken(fact, payload.attack.to) });
        case "evalDelta":
            return selection.kind === "mate" ? seg("Commentary.Mate", { san }) : null;
        case "opening":
            return seg("Commentary.Opening", {
                name: { kind: "name", text: payload.opening.name },
                eco: { kind: "name", text: payload.opening.eco },
                move: numberToken(fact, "move"),
            });
        case "novelty":
            return seg("Commentary.Novelty", { san });
        case "time":
            return seg("Commentary.Time", { seconds: numberToken(fact, "seconds") });
    }
}

/**
 * The deterministic sentences for one move: at most one per selected fact, in selection order
 * (the facts the kind requires come first). Every chess value in a sentence comes from the cited
 * fact's vocabulary through `segmentize`.
 */
export function narrateMove(
    selection: FactSelection,
    facts: MoveFacts,
    ctx: NarrationContext,
): SentenceDraft[] {
    const enumerated = new Map(enumerateFacts(facts, ctx).map((f) => [f.id, f]));
    const drafts: SentenceDraft[] = [];
    for (const selected of selection.facts) {
        const fact = enumerated.get(selected.id);
        const segmented = fact ? factSentence(facts, fact, selection, ctx) : null;
        if (segmented) {
            drafts.push({
                template: segmented.template,
                sentence: { text: segmented.text, factIds: [selected.id], refs: segmented.refs },
            });
        }
    }
    return drafts;
}

function uciSquares(uci: string): { orig: string; dest: string } {
    return { orig: uci.slice(0, 2), dest: uci.slice(2, 4) };
}

/**
 * Arrows for the position after the move: the opponent's reply in red (two plies), squares of
 * selected motifs and hanging pieces in yellow.
 */
export function arrowsFor(selection: FactSelection, facts: MoveFacts): CommentaryArrow[] {
    const arrows: CommentaryArrow[] = [];
    for (const selected of selection.facts) {
        if (selected.id === `line:${facts.refutation}` && facts.refutation !== null) {
            for (const uci of facts.lines[facts.refutation].pvUci.slice(0, 2)) {
                arrows.push({ ...uciSquares(uci), brush: "red" });
            }
        } else if (selected.kind === "motif" || selected.kind === "hanging") {
            for (const square of selected.vocabulary.squares) {
                arrows.push({ orig: square, brush: "yellow" });
            }
        }
    }
    return arrows;
}
