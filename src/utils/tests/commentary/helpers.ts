import i18next from "i18next";
import type { MoveFacts } from "@/bindings";
import enUS from "@/translation/en-US.json";
import type { CommentaryRun } from "@/utils/commentary/report";
import { pieceWords, type Translate } from "@/utils/commentary/segmentize";
import fixtures from "./fixtures/moveFacts.json";

export type FixtureName = keyof typeof fixtures;

/** Facts produced by the Rust extractor (see `write_frontend_fixtures` in commentary/facts). */
export function facts(name: FixtureName): MoveFacts {
    return structuredClone(fixtures[name]) as MoveFacts;
}

export const FIXTURE_NAMES = Object.keys(fixtures) as FixtureName[];

/**
 * A translation function over the shipped en-US strings and optional extra locales, with en-US as
 * the fallback like the app.
 */
export function translator(
    lang = "en-US",
    extra: Record<string, Record<string, string>> = {},
): Translate {
    const instance = i18next.createInstance();
    instance.init({
        lng: lang,
        fallbackLng: "en-US",
        returnEmptyString: false,
        initAsync: false,
        resources: {
            "en-US": enUS,
            ...Object.fromEntries(
                Object.entries(extra).map(([lng, t]) => [lng, { translation: t }]),
            ),
        },
    });
    return (key, options) => String(instance.t(key as string, options as never));
}

export function run(lang = "en-US", t: Translate = translator(lang)): CommentaryRun {
    return {
        t,
        lang,
        rootHalfMoves: 0,
        settings: { detail: "keyMoments", extendedGlyphs: true },
        perspective: "both",
        appVersion: "test",
        pieceWords: pieceWords(t),
        now: 0,
    };
}
