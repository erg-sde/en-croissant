# En Croissant natural-language commentary

Design and kickoff material for extending [En Croissant](https://github.com/franciscoBSalgueiro/en-croissant) with natural-language game commentary: per-move explanations with reasons, a game narrative, an always-available deterministic narrator, and an optional Claude-backed narrator that is validated sentence by sentence against the board.

| File | What it is |
|---|---|
| `PLAN.md` | The architecture plan: thesis, module layout, data model, pipeline, IPC contract, UX, settings and secrets, persistence, invariants with proving tests, test and evaluation strategy, PR-sized delivery plan, glossary. |
| `KICKOFF-PROMPT.md` | A prompt to paste into a fresh Claude Code session to fork En Croissant and deliver the first slices of the plan. |
| `research/` | Verified maps of the upstream codebase (v0.15.1), prior-art studies, the design brief, and the maintainer's abandoned 2024 comment generator. Read `research/README.md` first. |

How this was produced (2026-10-10 to 2026-10-11): seven parallel code readers mapped the upstream subsystems and three researchers studied competitors, grounding research and upstream attitudes; a critic spot-checked the maps and twelve follow-up readers answered its gaps. Three architects then designed independently (upstream-first, quality ceiling, correctness by construction), three judges scored them, one synthesizer merged the winner with grafts from the others, three adversarial critics attacked the result (45 findings, 3 blockers), and a reviser applied 40 of them. Every file path and line number in `PLAN.md` was checked mechanically against the upstream clone.
