import type { FactVocabulary, MoveRef, NumberRef, PieceKind, PieceRef, Side } from "@/bindings";
import { formatScore } from "@/utils/score";

/** A value interpolated into a sentence. Every chess value becomes a recorded reference. */
export type Token =
    | { kind: "move"; ref: MoveRef }
    /** Several plies, shown with move numbers; `startHalfMoves` is the ply count before the first. */
    | { kind: "line"; moves: MoveRef[]; startHalfMoves: number }
    /** A piece with its square, e.g. "the knight on f7". */
    | { kind: "piece"; ref: PieceRef }
    /** A piece name alone, e.g. "knight"; `ref` records where it stands when there is one. */
    | { kind: "pieceName"; piece: PieceKind; ref?: PieceRef }
    | { kind: "square"; square: string }
    | { kind: "number"; ref: NumberRef }
    | { kind: "side"; side: Side }
    | { kind: "list"; items: Token[] }
    /** Text that is not a chess reference, such as an opening name. */
    | { kind: "name"; text: string };

export type Translate = (key: string | string[], options?: Record<string, unknown>) => string;

export interface Segmented {
    /** The translated sentence with each value as a `{{name}}` placeholder. */
    template: string;
    text: string;
    refs: FactVocabulary;
}

// Sentinels delimit values inside a translated string; control characters occur in neither
// translations nor chess values.
const OPEN = String.fromCharCode(1);
const CLOSE = String.fromCharCode(2);
const SENTINEL = new RegExp(`${OPEN}(\\d+)${CLOSE}`, "g");

function emptyRefs(): FactVocabulary {
    return { moves: [], pieces: [], squares: [], numbers: [], sides: [] };
}

function pushUnique<T>(items: T[], item: T) {
    if (!items.some((existing) => JSON.stringify(existing) === JSON.stringify(item))) {
        items.push(item);
    }
}

function record(token: Token, refs: FactVocabulary) {
    switch (token.kind) {
        case "move":
            pushUnique(refs.moves, token.ref);
            break;
        case "line":
            token.moves.forEach((m) => pushUnique(refs.moves, m));
            break;
        case "piece":
            pushUnique(refs.pieces, token.ref);
            pushUnique(refs.squares, token.ref.square);
            break;
        case "pieceName":
            if (token.ref) {
                pushUnique(refs.pieces, token.ref);
                pushUnique(refs.squares, token.ref.square);
            }
            break;
        case "square":
            pushUnique(refs.squares, token.square);
            break;
        case "number":
            pushUnique(refs.numbers, token.ref);
            break;
        case "side":
            pushUnique(refs.sides, token.side);
            break;
        case "list":
            token.items.forEach((item) => record(item, refs));
            break;
        case "name":
            break;
    }
}

export function formatNumber(ref: NumberRef, lang: string): string {
    switch (ref.unit) {
        case "eval":
            return formatScore({ type: "cp", value: ref.value });
        case "pawns":
            return new Intl.NumberFormat(lang, { maximumFractionDigits: 1 }).format(ref.value);
        default:
            return new Intl.NumberFormat(lang, { maximumFractionDigits: 0 }).format(ref.value);
    }
}

function formatLine(moves: MoveRef[], startHalfMoves: number): string {
    return moves
        .map((move, i) => {
            const halfMoves = startHalfMoves + i + 1;
            const number = Math.ceil(halfMoves / 2);
            if (move.side === "white") {
                return `${number}.${move.san}`;
            }
            return i === 0 ? `${number}...${move.san}` : move.san;
        })
        .join(" ");
}

function render(token: Token, t: Translate, lang: string): string {
    const pieceName = (piece: PieceKind) => t(`Commentary.Piece.${piece}`);
    switch (token.kind) {
        case "move":
            return token.ref.san;
        case "line":
            return formatLine(token.moves, token.startHalfMoves);
        case "piece":
            return t("Commentary.PieceOn", {
                piece: pieceName(token.ref.piece),
                square: token.ref.square,
                interpolation: { escapeValue: false },
            });
        case "pieceName":
            return pieceName(token.piece);
        case "square":
            return token.square;
        case "number":
            return formatNumber(token.ref, lang);
        case "side":
            return t(token.side === "white" ? "Commentary.Side.White" : "Commentary.Side.Black");
        case "list":
            return new Intl.ListFormat(lang, { type: "conjunction" }).format(
                token.items.map((item) => render(item, t, lang)),
            );
        case "name":
            return token.text;
    }
}

function capitalize(text: string, lang: string): string {
    return text.charAt(0).toLocaleUpperCase(lang) + text.slice(1);
}

/**
 * Renders a commentary template. Each value is interpolated as an opaque sentinel, the template is
 * translated, and only then are the sentinels replaced by the rendered values, which are recorded
 * as references. A translation can reorder words but cannot introduce a square, move or number of
 * its own, so every rendered reference is one the caller passed.
 *
 * `keys` are tried in order, as in i18next.
 */
export function segmentize(
    keys: string[],
    values: Record<string, Token>,
    t: Translate,
    lang: string,
): Segmented {
    const names = Object.keys(values);
    const sentinels = Object.fromEntries(names.map((name, i) => [name, `${OPEN}${i}${CLOSE}`]));
    const translated = t(keys, { ...sentinels, interpolation: { escapeValue: false } });

    const refs = emptyRefs();
    const template = translated.replace(SENTINEL, (_, i: string) => `{{${names[Number(i)]}}}`);
    const text = translated.replace(SENTINEL, (_, i: string) => {
        const token = values[names[Number(i)]];
        record(token, refs);
        return render(token, t, lang);
    });
    return { template, text: capitalize(text, lang), refs };
}
