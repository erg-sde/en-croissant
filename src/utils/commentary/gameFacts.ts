import type {
    CommentaryKind,
    EvalValue,
    FactVocabulary,
    MoveFacts,
    MoveRef,
    Outcome,
    Side,
} from "@/bindings";
import { positionFromFen } from "@/utils/chessops";
import { harmonicMean } from "@/utils/misc";
import { getGamePhases } from "@/utils/phase";
import { getAccuracy } from "@/utils/score";
import type { FactId, FactSelection } from "./types";

/** A swing counts when it moves at least this much win chance. */
const SWING_MIN = 5;
const MAX_SWINGS = 3;

export interface GameSwing {
    index: number;
    san: string;
    side: Side;
    /** Win chance the mover lost (negative: gained), percentage points. */
    winDelta: number;
    kind: CommentaryKind;
}

/** What is counted about the game as a whole, from its move facts. */
export interface GameFacts {
    opening: { eco: string | null; name: string | null; lastNamedIndex: number | null };
    /** The move that left the reference database; `null` without one. */
    noveltyIndex: number | null;
    /** Position indices where the middlegame and endgame begin. */
    phases: { middlegamePly: number | null; endgamePly: number | null };
    swings: GameSwing[];
    /** The move after which the loser's win chance never again exceeds 50. */
    decisive: number | null;
    accuracy: { white: number | null; black: number | null };
    result: Outcome;
    terminal: { index: number; san: string; mate: boolean } | null;
}

/** The facts of each analysed move by position index (index 0, the root, has none). */
export type MovesByIndex = (MoveFacts | null)[];

function toScore(value: EvalValue) {
    return value.type === "cp"
        ? { type: "cp" as const, value: value.value }
        : { type: "mate" as const, value: value.value };
}

/** White's win chance in the position after each move, from the mover-POV facts. */
function whiteWin(facts: MoveFacts): number {
    return facts.mover === "white" ? facts.eval.after.win : 100 - facts.eval.after.win;
}

function loserOf(result: Outcome): Side | null {
    return result === "1-0" ? "black" : result === "0-1" ? "white" : null;
}

export function gameFacts(
    moves: MovesByIndex,
    fens: string[],
    selections: (FactSelection | null)[],
    result: Outcome,
): GameFacts {
    const analysed = moves.filter((m): m is MoveFacts => m !== null);

    const opening = analysed.find((m) => m.opening)?.opening ?? null;
    const novelty = analysed.find((m) => m.novelty);

    const boards = fens.map((fen) => positionFromFen(fen)[0]).filter((b) => b !== null);
    const phases = getGamePhases(boards);

    const swings = analysed
        .filter((m) => m.eval.winDelta !== null && Math.abs(m.eval.winDelta) >= SWING_MIN)
        .sort((a, b) => Math.abs(b.eval.winDelta!) - Math.abs(a.eval.winDelta!))
        .slice(0, MAX_SWINGS)
        .sort((a, b) => a.index - b.index)
        .map((m) => ({
            index: m.index,
            san: m.played.san,
            side: m.mover,
            winDelta: m.eval.winDelta!,
            kind: selections[m.index]?.kind ?? (m.eval.winDelta! > 0 ? "mistake" : "positional"),
        }));

    const loser = loserOf(result);
    let decisive: number | null = null;
    if (loser) {
        const loserWin = (m: MoveFacts) => (loser === "white" ? whiteWin(m) : 100 - whiteWin(m));
        // The first move after the last position in which the loser was still better.
        const lastHope = [...analysed].reverse().find((m) => loserWin(m) > 50);
        decisive = lastHope
            ? (analysed.find((m) => m.index > lastHope.index)?.index ?? null)
            : null;
    }

    const accuracies = { white: [] as number[], black: [] as number[] };
    for (const m of analysed) {
        const before = m.eval.before;
        if (!before || m.terminal) continue;
        accuracies[m.mover].push(
            getAccuracy(toScore(before.value), toScore(m.eval.after.value), m.mover),
        );
    }

    const last = analysed[analysed.length - 1];
    return {
        opening: {
            eco: opening?.eco ?? null,
            name: opening?.name ?? null,
            lastNamedIndex: opening?.lastNamedIndex ?? null,
        },
        noveltyIndex: novelty?.index ?? null,
        phases,
        swings,
        decisive,
        accuracy: {
            white: accuracies.white.length > 0 ? harmonicMean(accuracies.white) : null,
            black: accuracies.black.length > 0 ? harmonicMean(accuracies.black) : null,
        },
        result,
        terminal: last?.terminal
            ? { index: last.index, san: last.played.san, mate: last.played.givesMate }
            : null,
    };
}

/** A game fact with the board references it permits and the move indices they belong to. */
export interface EnumeratedGameFact {
    id: FactId;
    vocabulary: FactVocabulary;
    indices: number[];
}

function emptyVocabulary(): FactVocabulary {
    return { moves: [], pieces: [], squares: [], numbers: [], sides: [] };
}

/** The move number of the position at `index`. */
export function moveNumber(index: number, rootHalfMoves: number): number {
    return Math.ceil((rootHalfMoves + index) / 2);
}

function played(moves: MovesByIndex, index: number): MoveRef | null {
    const m = moves[index];
    return m ? { san: m.played.san, side: m.mover, at: { t: "before" } } : null;
}

/** Game facts with ids `game:<kind>:<ordinal>`, in a fixed order. */
export function enumerateGameFacts(
    facts: GameFacts,
    moves: MovesByIndex,
    rootHalfMoves: number,
): EnumeratedGameFact[] {
    const out: EnumeratedGameFact[] = [];
    const counters = new Map<string, number>();
    const add = (kind: string, indices: number[], fill: (v: FactVocabulary) => void) => {
        const ordinal = counters.get(kind) ?? 0;
        counters.set(kind, ordinal + 1);
        const vocabulary = emptyVocabulary();
        for (const index of indices) {
            const ref = played(moves, index);
            if (ref) {
                vocabulary.moves.push(ref);
                vocabulary.sides.push(ref.side);
            }
            vocabulary.numbers.push({ value: moveNumber(index, rootHalfMoves), unit: "move" });
        }
        fill(vocabulary);
        out.push({ id: `game:${kind}:${ordinal}`, vocabulary, indices });
    };

    if (facts.opening.name && facts.opening.lastNamedIndex !== null) {
        add("opening", [], (v) =>
            v.numbers.push({
                value: moveNumber(facts.opening.lastNamedIndex!, rootHalfMoves),
                unit: "move",
            }),
        );
    }
    if (facts.noveltyIndex !== null) {
        add("novelty", [facts.noveltyIndex], () => {});
    }
    for (const ply of [facts.phases.middlegamePly, facts.phases.endgamePly]) {
        if (ply !== null) {
            add("phase", [], (v) =>
                v.numbers.push({ value: moveNumber(ply, rootHalfMoves), unit: "move" }),
            );
        }
    }
    for (const swing of facts.swings) {
        add("swing", [swing.index], (v) =>
            v.numbers.push({ value: Math.round(Math.abs(swing.winDelta)), unit: "winPercent" }),
        );
    }
    if (facts.decisive !== null) {
        add("decisive", [facts.decisive], () => {});
    }
    if (facts.accuracy.white !== null || facts.accuracy.black !== null) {
        add("accuracy", [], (v) => {
            for (const side of ["white", "black"] as const) {
                const value = facts.accuracy[side];
                if (value !== null) {
                    v.numbers.push({ value: Math.round(value), unit: "winPercent" });
                    v.sides.push(side);
                }
            }
        });
    }
    if (facts.terminal) {
        add("terminal", [facts.terminal.index], () => {});
    }
    return out;
}
