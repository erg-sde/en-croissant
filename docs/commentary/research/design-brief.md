# Design brief: natural-language commentary for En Croissant

## Who / why
Ed (edthechesshead) uses En Croissant daily and wants to extend it with natural-language commentary that is
*strictly better than chess.com's Game Review coach*. He will (a) run a fork, (b) try to upstream slices,
and (c) show the architecture at job interviews. "Crisp" matters more than "fast". Not quick-and-dirty.

## Product goals (in priority order)
1. Per-move explanations where they best fit: why a move was a mistake, what the better move achieves,
   what is threatened now, why a move was the only move / a real sacrifice. Reasons, never bare labels.
2. A game-level narrative (opening + where theory ended, turning points, what decided the game, 2–3 takeaways
   each citing a move) in the Report tab.
3. Surfaces: the "Generate report" flow (batch, whole game) AND the analysis-by-engine view
   (on-demand "explain this position" for the current node while browsing).
4. Optional LLM narration via a Claude API key entered in Settings (Anthropic first; provider interface so
   others fit), with an always-available deterministic/offline narration that is good on its own.
5. Later (design for, don't build first): player-aware context (own history in this line, clock/time pressure,
   rating gap), Maia-style "what players at your level play".

## Non-negotiable constraints (from the verified codebase analysis + upstream research)
- Pipeline shape is FACTS → SELECTION → NARRATION → VALIDATION → PERSISTENCE → PRESENTATION. The LLM is a
  verbalizer of a closed, typed fact set; it never computes chess. Every sentence must be checkable.
- Deterministic facts in Rust next to `analyze_game` (shakmaty is already there), typed via specta. Keep the
  existing `getAnnotation` thresholds/glyphs; extend, don't change.
- Deterministic narration is the default; the LLM is opt-in. Maintainer positions: "An AI model is way out of
  scope" (#230) and "don't want features dependent on paid third-party APIs" (#704) BUT he welcomed deterministic
  comments and has his own abandoned `report-comments` branch (`getComment(pos, move) -> {text, arrows} | null`).
- Secrets: never log; never put in toast text; don't copy the lichess plaintext-token-in-localStorage pattern
  without at least isolating it. LLM HTTP should go through Rust reqwest (keeps key out of the webview, no
  compile-time capability widening, allows a user-configurable base URL / local models).
- Privacy: sending positions to a provider is a new category vs the existing telemetry; explicit opt-in, show
  what is sent.
- Rendering: Comment.tsx uses rehypeRaw with csp:null → raw HTML executes with IPC reach. Generated text must
  be rendered through a sanitized path. PGN `{}` comments need `}` handling.
- Persistence: TreeNode has no id; node-resident optional fields are the stable choice for the live tree;
  sessionStorage persistence has no partialize/quota handling; trees are rebuilt from PGN on reload.
- Long-running jobs: today nothing protects the report from the tree changing underneath it; a commentary
  stage must snapshot + re-validate + be cancellable (incl. in-flight HTTP) + drive progress correctly.
- Known upstream bugs to fix or route around: bound lines kept in analyze_game; `assert_eq!` panic path;
  MultiPV hard-wired to 2; db_cache single-slot bug; opening match discards ECO and ply; errors cross IPC as
  bare strings; AnnotationEditor won't pick up programmatic comment writes; `getMoveText` glyph bug.
- Quality bar: i18n for all user-facing strings (16 locales, `pnpm i18n:extract`); vitest for pure TS
  modules (no Tauri imports); `#[cfg(test)]` for Rust; oxfmt/oxlint/tsgo clean; bindings regenerated and
  committed; small reviewable PRs; GPL-3.0-only compatible.

## Evidence to read (in this order)
1. critique-and-answers.md — verified spot checks + 12 deep answers (identity/keys, stale-tree protection,
   one-shot engine primitive, fact reliability, persistence budget, renderer safety, editor overwrite,
   progress/cancel, db cache, opening primitive, dtz scores, error surfacing).
2. map-*.md — one per subsystem: summary, key files, data model, extension points, conventions, gotchas.
3. study-*.md — competitors (what "better than chess.com" requires), academic grounding (facts→narration,
   validation, motif detection), upstream attitudes (maintainer quotes, open issues #108 #230 #559 #637 #913,
   discussion #928, PR #704 #649 #749 lessons).
4. The upstream clone itself for anything you need to confirm.
5. The maintainer's `report-comments` branch files (getComment design) in ../report-comments-branch/.
