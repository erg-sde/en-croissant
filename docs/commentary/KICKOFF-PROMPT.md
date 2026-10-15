# Kickoff prompt: En Croissant natural-language commentary

Paste everything below the horizontal rule into a fresh Claude Code session as its first message. It assumes the session can reach GitHub and that this `docs/en-croissant-commentary/` folder (plan, research maps, this file) is available to it, either checked out alongside or pasted in. Without the folder the prompt still works from the plan's citations alone, just more slowly.

Model note: establishing the project and the prerequisite slices (1 to 6) are mostly mechanical against a verified plan; Opus 5.5 in normal mode is enough for them. Switch to Fable 5.1 for the facts extractors (7a, 7b), the deterministic narrator (8), and later the LLM slice (12), where chess-domain judgment and prompt and validator design matter. Ultracode is not needed for implementation sessions; it earned its cost here in the research and design phase.

---

You are starting a new project: a fork of En Croissant (https://github.com/franciscoBSalgueiro/en-croissant, Tauri 2 + React 19 + TypeScript + Rust, GPL-3.0-only) that adds natural-language game commentary. The architecture is already decided and written down in `PLAN.md`; your job is to establish the project cleanly and deliver the first slices exactly as the plan specifies. Quality over speed. This work will be shown to other engineers, so every pull request must be something a senior reviewer would approve without edits.

## 1. Establish the project

1. Fork `franciscoBSalgueiro/en-croissant` into my GitHub account (or use the fork if it already exists), clone it, and add the original as the `upstream` remote. Base all work on upstream `master` at its newest commit. Record that commit hash and date in `docs/commentary/BASELINE.md`.
2. The planning material lives in `docs/commentary/` in this repository (`PLAN.md`, `KICKOFF-PROMPT.md`, `research/`); I placed it there before starting you. If it is missing, copy it from the `docs/en-croissant-commentary/` folder of my `erg-sde/Chess` repository, branch `claude/determined-newton-kjqpe2`. Read `research/README.md`, then `research/design-brief.md`, then `research/critique-and-answers.md` before touching code. The maps were made against upstream commit `23f83142fcbd4d3af60a524855defe8c9a5703fe` (v0.15.1, 2026-09-07); line numbers may have drifted, so re-verify any citation before relying on it and record drift in `docs/commentary/DEVIATIONS.md`.
3. Install with `pnpm i`. Confirm the toolchain: `pnpm lint`, `pnpm test`, and inside `src-tauri` `cargo test` and `cargo clippy -- -D warnings`. Run `pnpm dev` once in debug so `src/bindings/generated.ts` is regenerated (bindings are only written by a debug run, never by CI or by `pnpm build`). If this environment cannot start a Tauri webview, say so plainly and note that bindings must be regenerated on my machine before each PR; never hand-edit `generated.ts`.
4. Commit the docs folder as the first commit on a branch named `commentary/00-docs` and open a pull request against the fork's own `master`. All later PRs also target the fork's `master`; I decide which go upstream.

## 2. Working rules (non-negotiable)

- Follow `docs/commentary/PLAN.md` literally for module names, type names, command names, store actions, settings keys, i18n key prefixes, invariants, and slice order. If reality forces a change, make the smallest one and log it with the reason in `DEVIATIONS.md` in the same PR. Section 16 of the plan is the naming glossary; do not invent synonyms.
- One branch and one PR per plan slice, named `commentary/NN-<slug>` with the slice number from the plan's delivery table (section 13). Each PR description states: slice number and title, what it changes, which rows of the invariant table (section 11) it establishes, how it was tested, and anything deferred.
- Before every push: `pnpm format`, `pnpm lint`, `pnpm test`, `pnpm i18n:extract` (commit the regenerated locale files; only `en-US.json` needs real strings), and in `src-tauri`: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`. Rust fact extractors ship with `#[cfg(test)]` FEN fixtures. TypeScript modules under `src/utils/commentary/` import nothing from `@tauri-apps/*` or Mantine so vitest can load them.
- Additive only: do not change existing `getAnnotation` thresholds, existing binding field names, or the behaviour the existing `addAnalysis` store test asserts.
- Secrets: never log, toast, or commit an API key, a request body that contains one, or a provider response body. Redact before any error is surfaced.
- No model identifiers or AI attributions in code, comments, or commit messages. Imperative commit subjects, small commits.
- When a decision needs the upstream maintainer's input to be mergeable, do not guess: record it in `DEVIATIONS.md` under "Questions for upstream" and keep the fork's behaviour behind the plan's settings flags.

## 3. What to deliver in this session

Work the delivery table in section 13 of the plan in order. Finish each slice completely (code, tests, i18n, bindings, PR description) before starting the next. Target for this session:

1. Slice 0: the docs PR above.
2. Slices 1 to 6, the prerequisite fixes to shipped upstream behaviour that the commentary depends on: report robustness (exact scores only, a cancel token that cannot be lost, terminal progress on every exit path), surfaced report errors and store guards, PGN comment hygiene, safe comment rendering, opening match with ECO and ply, and self-describing analysis (`MoveAnalysis.fen`, configurable MultiPV, snapshot plus position-checked `addAnalysis` with a run id, one shared glyph derivation). These are small, independently valuable, and the most likely to be accepted upstream, so make them exemplary.
3. Slices 7a and 7b, the Rust `facts` module with fixture tests, wired as the `with_facts` post-pass of `analyze_game`, bindings regenerated.
4. If time remains, slice 8, the deterministic commentary (salience, extended glyphs, narrate, segmentize, validate, brands, the `TreeNode.commentary` field, store actions, `CommentaryText`, `CommentaryCard`, settings rows, i18n keys, vitest suites).

Stop before slice 12 (LLM narration). It is fork-first and needs my Anthropic key and a consent review; we will do it together in a later session.

## 4. How to report

At the end of the session write `docs/commentary/STATUS.md` with: slices completed (PR links), slices in progress (branch and what is left), test counts added, open questions, and the exact commands to resume. Reply in chat with a short summary and the list of PR links. If you hit a blocker that only I can resolve (GitHub permissions, the webview not starting, an upstream change that invalidates a plan section), finish everything that does not depend on it, then stop and tell me what you need.
