import type { Claim, EvalValue, LineFacts, Motif, MoveFacts, PieceRef, Side } from "@/bindings";
import type { EnumeratedFact } from "./salience";
import type { ValidationRule } from "./types";

/**
 * Rules for model-written sentences: every relation a sentence states is declared as a claim, and
 * every claim follows from the fields of a fact the sentence cites (PLAN.md section 6.5, rules 5
 * to 7). Deterministic sentences are correct by construction and skip these rules.
 */

export interface ClaimFailure {
    rule: ValidationRule;
    detail: string;
}

type Relation = Claim["kind"];

/**
 * Words that state a relation, per language. A template using one must carry a claim of that kind.
 * Languages without a table skip the rule; slots and claims still carry the guarantee.
 */
const RELATION_WORDS: Record<string, Partial<Record<Relation, RegExp>>> = {
    en: {
        attacks: /\battack/,
        forks: /\bfork/,
        pins: /\bpin(s|ned|ning)?\b/,
        hanging: /\bhang|\bundefended\b|\bunprotected\b|en prise/,
        winsMaterial: /\bwins?\b|\bwinning (a|the|material)\b|\bwon\b|\bpicks? up\b/,
        mates: /\bmate\b|\bmates\b|\bmating\b|checkmate/,
        better: /\bbetter\b|\badvantage\b|\bthe upper hand\b/,
        threatens: /\bthreat/,
    },
    de: {
        attacks: /greift|angriff/,
        forks: /gabel/,
        pins: /fessel/,
        hanging: /hängt|ungedeckt|ungeschützt/,
        winsMaterial: /gewinnt|gewinn/,
        mates: /matt/,
        better: /besser|vorteil/,
        threatens: /droh/,
    },
    ru: {
        attacks: /атак|напада/,
        forks: /вилк/,
        pins: /связк|связан/,
        hanging: /висит|незащищ/,
        winsMaterial: /выигрыва|выигра/,
        mates: /мат(?![а-яё]*ериал)/,
        better: /лучше|преимуществ/,
        threatens: /угро/,
    },
    zh: {
        attacks: /攻击/,
        forks: /捉双|双重攻击/,
        pins: /牵制/,
        hanging: /悬空|无保护|没有保护/,
        winsMaterial: /得子|赢得|赢下/,
        mates: /将死|杀棋/,
        better: /优势|更好/,
        threatens: /威胁/,
    },
};

/** Words that grade an evaluation; "winning" needs a decisive score, "equal" a level one. */
const MAGNITUDE_WORDS: Record<string, { decisive: RegExp; level: RegExp }> = {
    en: {
        decisive: /\bwinning\b|\bdecisive\b|\bcrushing\b|\blost\b(?! (a|the|material))/,
        level: /\bequal\b|\blevel\b|\bbalanced\b/,
    },
    de: { decisive: /entscheidend|vernichtend|gewonnen|verloren/, level: /ausgeglichen/ },
    ru: { decisive: /решающ|выигранн|проигранн/, level: /равн/ },
    zh: { decisive: /决定性|胜势|败势/, level: /均势|平衡/ },
};

/** Motif names in English, and words that name no motif we detect. */
const MOTIF_WORDS: [RegExp, Motif["kind"]][] = [
    [/\bfork/, "fork"],
    [/\bpin(s|ned|ning)?\b/, "pin"],
    [/\bskewer/, "skewer"],
    [/\bdiscover/, "discoveredAttack"],
    [/\btrapped\b/, "trappedPiece"],
    [/\bback rank\b|\bback-rank\b/, "backRank"],
    [/\bpassed pawn|\bpasser\b/, "passedPawn"],
    [/\bpromot/, "promotion"],
];
const NEVER_WORDS = /\boutpost|\bweak square|\bhole\b/;

function lexicon<T>(tables: Record<string, T>, lang: string): T | undefined {
    return tables[lang.split("-")[0].toLowerCase()];
}

function whitePov(value: EvalValue): { cp: number | null; mate: number | null } {
    return value.type === "cp" ? { cp: value.value, mate: null } : { cp: null, mate: value.value };
}

function isDecisive(value: EvalValue): boolean {
    const { cp } = whitePov(value);
    return cp === null || Math.abs(cp) > 300;
}

/** The side that plays a line's first move. */
function lineMover(facts: MoveFacts, line: LineFacts): Side {
    const opponent: Side = facts.mover === "white" ? "black" : "white";
    return line.origin === "after" ? opponent : facts.mover;
}

/** The lines the cited facts are about. */
function citedLines(cited: EnumeratedFact[]): LineFacts[] {
    return cited.flatMap((f) =>
        f.payload.kind === "line" || f.payload.kind === "motif" ? [f.payload.line] : [],
    );
}

/** The motifs the cited facts state. */
function citedMotifs(cited: EnumeratedFact[]): Motif[] {
    return cited.flatMap((f) => {
        switch (f.payload.kind) {
            case "motif":
            case "hanging":
                return [f.payload.motif];
            case "line":
                return f.payload.line.motifs;
            default:
                return [];
        }
    });
}

function at(piece: PieceRef, square: string, kind?: string): boolean {
    return piece.square === square && (kind === undefined || piece.piece === kind);
}

/** Why `claim` does not follow from the cited facts, or `null` when it does. */
function entailment(claim: Claim, cited: EnumeratedFact[], facts: MoveFacts): ClaimFailure | null {
    const motifs = citedMotifs(cited);
    const lines = citedLines(cited);
    const unsupported = (detail: string): ClaimFailure => ({ rule: "claimUnsupported", detail });
    const contradicted = (detail: string): ClaimFailure => ({ rule: "claimContradicted", detail });
    const lineOf = (id: number) => lines.find((l) => l.id === id);

    switch (claim.kind) {
        case "forks": {
            const fork = motifs.find(
                (m): m is Extract<Motif, { kind: "fork" }> =>
                    m.kind === "fork" && at(claim.attacker, m.from, m.attacker),
            );
            if (!fork) {
                const reversed = motifs.some(
                    (m) =>
                        m.kind === "fork" &&
                        m.targets.some((t) => t.square === claim.attacker.square),
                );
                return reversed
                    ? contradicted(`fork by ${claim.attacker.square}`)
                    : unsupported(`fork by ${claim.attacker.square}`);
            }
            const missing = claim.targets.filter(
                (target) => !fork.targets.some((t) => at(target, t.square, t.piece)),
            );
            return missing.length > 0 || claim.targets.length < 2
                ? unsupported(`fork targets ${missing.map((t) => t.square).join(",")}`)
                : null;
        }
        case "attacks": {
            const direct = cited.some(
                (f) =>
                    f.payload.kind === "attack" &&
                    at(claim.attacker, f.payload.attack.from, f.payload.attack.attacker) &&
                    at(claim.target, f.payload.attack.to, f.payload.attack.target),
            );
            const byFork = motifs.some(
                (m) =>
                    m.kind === "fork" &&
                    at(claim.attacker, m.from, m.attacker) &&
                    m.targets.some((t) => at(claim.target, t.square, t.piece)),
            );
            if (direct || byFork) return null;
            const reversed = cited.some(
                (f) =>
                    f.payload.kind === "attack" &&
                    f.payload.attack.from === claim.target.square &&
                    f.payload.attack.to === claim.attacker.square,
            );
            return reversed
                ? contradicted(`${claim.attacker.square} attacks ${claim.target.square}`)
                : unsupported(`${claim.attacker.square} attacks ${claim.target.square}`);
        }
        case "pins": {
            const pin = motifs.some(
                (m) =>
                    m.kind === "pin" &&
                    m.attacker === claim.attacker.square &&
                    m.pinned === claim.pinned.square,
            );
            if (pin) return null;
            const reversed = motifs.some(
                (m) => m.kind === "pin" && m.pinned === claim.attacker.square,
            );
            return reversed
                ? contradicted(`pin by ${claim.attacker.square}`)
                : unsupported(`pin by ${claim.attacker.square}`);
        }
        case "hanging":
            return motifs.some((m) => m.kind === "hanging" && at(claim.piece, m.square, m.piece))
                ? null
                : unsupported(`hanging ${claim.piece.square}`);
        case "winsMaterial": {
            const line = lineOf(claim.line);
            if (!line) return unsupported(`line ${claim.line} not cited`);
            if (line.outcome.kind !== "material") {
                return unsupported(`line ${claim.line} wins no material`);
            }
            const forSide =
                lineMover(facts, line) === claim.side ? line.outcome.pawns : -line.outcome.pawns;
            if (forSide <= 0 || claim.pawns <= 0) {
                return contradicted(`line ${claim.line} does not win material for ${claim.side}`);
            }
            return null;
        }
        case "mates": {
            const line = lineOf(claim.line);
            if (!line) return unsupported(`line ${claim.line} not cited`);
            const { mate } = whitePov(line.eval.value);
            if (line.outcome.kind !== "mate" || mate === null) {
                return unsupported(`line ${claim.line} is not a mate`);
            }
            const mater: Side = mate > 0 ? "white" : "black";
            if (mater !== claim.side) return contradicted(`${claim.side} does not mate`);
            return line.outcome.mateIn === claim.mateIn
                ? null
                : contradicted(`mate in ${line.outcome.mateIn}, not ${claim.mateIn}`);
        }
        case "better": {
            const value =
                claim.at.t === "before"
                    ? facts.eval.before?.value
                    : claim.at.t === "after"
                      ? facts.eval.after.value
                      : facts.lines[claim.at.id]?.eval.value;
            if (!value) return unsupported("no evaluation");
            const { cp, mate } = whitePov(value);
            const leader: Side | null =
                mate !== null
                    ? mate > 0
                        ? "white"
                        : "black"
                    : Math.abs(cp!) > 100
                      ? cp! > 0
                          ? "white"
                          : "black"
                      : null;
            if (leader === null) return unsupported("the position is level");
            return leader === claim.side ? null : contradicted(`${claim.side} is not better`);
        }
        case "threatens": {
            const line = lineOf(claim.line);
            if (!line || line.origin !== "threat") {
                return unsupported(`line ${claim.line} is not a cited threat`);
            }
            return lineMover(facts, line) === claim.side
                ? null
                : contradicted(`${claim.side} does not threaten`);
        }
    }
}

/**
 * Rules 5 to 7 for one model-written sentence. `literal` is its template without placeholders;
 * `facts` is `null` for narrative sentences citing only game facts, which can carry no claim.
 */
export function claimFailures(
    claims: Claim[],
    literal: string,
    cited: EnumeratedFact[],
    facts: MoveFacts | null,
    lang: string,
): ClaimFailure[] {
    const failures: ClaimFailure[] = [];
    for (const claim of claims) {
        const failure = facts
            ? entailment(claim, cited, facts)
            : { rule: "claimUnsupported" as const, detail: `${claim.kind} without a move fact` };
        if (failure) failures.push(failure);
    }

    const text = literal.toLocaleLowerCase(lang);
    const relations = lexicon(RELATION_WORDS, lang);
    if (relations) {
        for (const [relation, pattern] of Object.entries(relations) as [Relation, RegExp][]) {
            const declared =
                claims.some((c) => c.kind === relation) ||
                // "wins" also describes a mate.
                (relation === "winsMaterial" && claims.some((c) => c.kind === "mates"));
            if (pattern.test(text) && !declared) {
                failures.push({
                    rule: relation === "better" ? "sideBetter" : "relationWithoutClaim",
                    detail: relation,
                });
            }
        }
    }

    const magnitude = lexicon(MAGNITUDE_WORDS, lang);
    if (magnitude) {
        const values = [
            ...(facts ? [facts.eval.after.value] : []),
            ...citedLines(cited).map((l) => l.eval.value),
        ];
        if (magnitude.decisive.test(text) && !values.some(isDecisive)) {
            failures.push({ rule: "magnitude", detail: "decisive without a decisive score" });
        }
        if (
            magnitude.level.test(text) &&
            (!facts ||
                isDecisive(facts.eval.after.value) ||
                Math.abs(whitePov(facts.eval.after.value).cp ?? 0) > 100)
        ) {
            failures.push({ rule: "magnitude", detail: "level without a level score" });
        }
    }

    if (lang.toLowerCase().startsWith("en")) {
        const kinds = new Set(citedMotifs(cited).map((m) => m.kind));
        for (const [pattern, kind] of MOTIF_WORDS) {
            if (pattern.test(text) && !kinds.has(kind)) {
                failures.push({ rule: "unknownMotif", detail: kind });
            }
        }
        if (NEVER_WORDS.test(text)) {
            failures.push({ rule: "unknownMotif", detail: "undetected concept" });
        }
    }
    return failures;
}
