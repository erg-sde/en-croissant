import type { NarratorConfig, Perspective, SecretStatus } from "@/bindings";
import type { CommentarySettings } from "./settings";

/** The consent text version a user must have accepted before anything is sent to a provider. */
export const CONSENT_VERSION = 1;

/** Word budgets for one move, a key moment and the game narrative (PLAN.md section 6.4). */
export const WORD_BUDGET = { moveWords: 60, keyMomentWords: 140, narrativeWords: 220 } as const;

/** The provider settings sent to Rust; the key never leaves Rust, so there is no field for it. */
export function narratorConfig(
    settings: CommentarySettings,
    uiLanguage: string,
    perspective: Perspective,
): NarratorConfig {
    return {
        provider: settings.provider,
        model: settings.model.trim(),
        baseUrl: settings.baseUrl,
        language: settings.language ?? uiLanguage,
        perspective,
        budget: { ...WORD_BUDGET },
        repairRounds: 0,
    };
}

/**
 * Whether narration can run: enabled, consented to, and either a stored key or a key-less
 * OpenAI-compatible server.
 */
export function canNarrate(settings: CommentarySettings, secret: SecretStatus | null): boolean {
    if (!settings.llmEnabled || settings.consentVersion < CONSENT_VERSION) {
        return false;
    }
    return (
        !!secret?.configured ||
        (settings.provider === "openaiCompatible" && settings.baseUrl !== null)
    );
}

/** The host requests go to, for the "Narrate with …" label and the consent text. */
export function narratorHost(settings: CommentarySettings): string {
    if (settings.baseUrl) {
        try {
            return new URL(settings.baseUrl).host;
        } catch {
            return settings.baseUrl;
        }
    }
    return settings.provider === "anthropic" ? "api.anthropic.com" : "";
}
