import type { PgnSafeText } from "./types";

function isControl(char: string): boolean {
    const code = char.charCodeAt(0);
    return (code < 0x20 && char !== "\t" && char !== "\n" && char !== "\r") || code === 0x7f;
}

/**
 * Makes text safe to place inside a PGN `{...}` comment. This is the only function that produces a
 * `PgnSafeText`.
 *
 * Braces are replaced, not escaped: PGN has no escape for `}` inside a comment, readers end a comment
 * at the first `}`, and the game scanner in src-tauri/src/pgn.rs toggles its comment state on every
 * brace, so an escape would still split multi-game files. `[%` is broken up so the text cannot be
 * read back as a command such as `[%eval]` or `[%clk]`. Control characters other than tabs and line
 * breaks are removed.
 */
export function toPgnSafe(text: string): PgnSafeText {
    return Array.from(text)
        .filter((char) => !isControl(char))
        .join("")
        .replaceAll("{", "(")
        .replaceAll("}", ")")
        .replaceAll("[%", "[ %") as PgnSafeText;
}
