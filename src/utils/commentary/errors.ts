import { match } from "ts-pattern";
import type { NarrationFailure } from "@/bindings";
import type { Translate } from "./segmentize";

export interface FailureMessage {
    title: string;
    /** Redacted provider text, shown only in an expandable section. */
    detail: string | null;
}

/** What to tell the user about a failed narration; `null` for a cancel, which is silent. */
export function narrationFailureMessage(
    failure: NarrationFailure,
    t: Translate,
): FailureMessage | null {
    return match(failure)
        .with({ kind: "cancelled" }, () => null)
        .with({ kind: "noKey" }, () => ({ title: t("Errors.Commentary.NoKey"), detail: null }))
        .with({ kind: "insecureBaseUrl" }, () => ({
            title: t("Errors.Commentary.InsecureBaseUrl"),
            detail: null,
        }))
        .with({ kind: "missingBaseUrl" }, () => ({
            title: t("Errors.Commentary.MissingBaseUrl"),
            detail: null,
        }))
        .with({ kind: "auth" }, (f) => ({
            title: t("Errors.Commentary.Auth"),
            detail: `HTTP ${f.detail.status}`,
        }))
        .with({ kind: "rateLimit" }, (f) => ({
            title: t("Errors.Commentary.RateLimit"),
            detail:
                f.detail.retryAfterMs !== null
                    ? t("Errors.Commentary.RetryAfter", {
                          seconds: Math.ceil(f.detail.retryAfterMs / 1000),
                      })
                    : null,
        }))
        .with({ kind: "network" }, (f) => ({
            title: t("Errors.Commentary.Network"),
            detail: f.detail.message,
        }))
        .with({ kind: "provider" }, (f) => ({
            title: t("Errors.Commentary.Provider"),
            detail: `HTTP ${f.detail.status}: ${f.detail.message}`,
        }))
        .with({ kind: "malformedResponse" }, (f) => ({
            title: t("Errors.Commentary.MalformedResponse"),
            detail: f.detail.message,
        }))
        .with({ kind: "refused" }, (f) => ({
            title: t("Errors.Commentary.Refused"),
            detail: f.detail.category,
        }))
        .exhaustive();
}
