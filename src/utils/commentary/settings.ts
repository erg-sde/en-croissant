import { z } from "zod";
import type { Perspective } from "@/bindings";
import type { Session } from "@/utils/session";
import type { GameHeaders } from "@/utils/treeReducer";

/**
 * Commentary settings. Every field has a default so settings stored by an older version parse
 * unchanged. The narration fields (`llmEnabled` to `exportToPgn`) are read by later slices.
 */
export const commentarySettingsSchema = z.object({
    explain: z.boolean().default(true),
    detail: z.enum(["keyMoments", "mistakes", "allMoves"]).default("keyMoments"),
    extendedGlyphs: z.boolean().default(true),
    threatLines: z.boolean().default(false),
    llmEnabled: z.boolean().default(false),
    consentVersion: z.number().default(0),
    provider: z.enum(["anthropic", "openaiCompatible"]).default("anthropic"),
    model: z.string().default("claude-opus-5-5"),
    baseUrl: z.string().url().nullable().default(null),
    language: z.string().nullable().default(null),
    perspective: z.enum(["auto", "white", "black", "both"]).default("auto"),
    exportToPgn: z.boolean().default(false),
});

export type CommentarySettings = z.infer<typeof commentarySettingsSchema>;
export type CommentaryDetail = CommentarySettings["detail"];

export const defaultCommentarySettings: CommentarySettings = commentarySettingsSchema.parse({});

function usernames(session: Session): string[] {
    return [session.lichess?.username, session.chessCom?.username, session.player]
        .filter((name): name is string => !!name)
        .map((name) => name.toLowerCase());
}

/**
 * Whose side "you" is. `auto` matches the game's player names against the signed-in accounts and
 * player aliases; exactly one side matching gives that side, anything else is third person.
 */
export function resolvePerspective(
    headers: GameHeaders,
    sessions: Session[],
    setting: CommentarySettings["perspective"],
): Perspective {
    if (setting !== "auto") {
        return setting;
    }
    const names = new Set(sessions.flatMap(usernames));
    const white = names.has(headers.white.toLowerCase());
    const black = names.has(headers.black.toLowerCase());
    if (white === black) {
        return "both";
    }
    return white ? "white" : "black";
}

/** The storage key of `commentarySettingsAtom`. */
export const COMMENTARY_SETTINGS_KEY = "commentary-settings";

/**
 * The stored commentary settings, read outside React (PGN export from the store and from file
 * saves, which must not import the atoms module). Defaults when nothing valid is stored.
 */
export function readCommentarySettings(storage: Pick<Storage, "getItem">): CommentarySettings {
    try {
        const raw = storage.getItem(COMMENTARY_SETTINGS_KEY);
        const parsed = commentarySettingsSchema.safeParse(raw ? JSON.parse(raw) : {});
        return parsed.success ? parsed.data : defaultCommentarySettings;
    } catch {
        return defaultCommentarySettings;
    }
}
