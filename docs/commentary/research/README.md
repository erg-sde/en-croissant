# Research: En Croissant codebase maps and prior art

Generated 2026-10-10 against upstream `franciscoBSalgueiro/en-croissant` at commit
`23f83142fcbd4d3af60a524855defe8c9a5703fe` (v0.15.1, 2026-09-07) by parallel code readers,
then spot-checked by a critic. Read these before touching the codebase; they save hours.

Reading order:

1. `design-brief.md` — goals, settled constraints, and the known upstream bugs.
2. `critique-and-answers.md` — verified spot checks, contradictions found in the maps, and 12 deep
   answers (node identity, stale-tree protection, one-shot engine primitive, fact reliability,
   persistence budget, renderer safety, editor overwrite, progress and cancellation, db cache,
   opening primitive, dtz scores, error surfacing). **Where a map and this file disagree, this file wins.**
   In particular the `chess.rs` line numbers in `map-rust-backend-conventions.md` are off by 10 to 50
   lines; the other maps' numbers are correct.
3. `map-*.md` — one per subsystem: summary, key files, data model, extension points, conventions, gotchas.
4. `study-*.md` — competitors (what beating chess.com's coach requires), academic grounding
   (facts to narration, validation, motif detection), upstream attitudes (maintainer quotes, open issues).
5. `maintainer-report-comments-branch-analysis.ts` — the maintainer's own abandoned 2024 attempt
   (`getComment(pos, move) -> {text, arrows} | null`) from the upstream `report-comments` branch.

Line numbers drift as upstream moves. Re-verify against the commit you actually fork.
