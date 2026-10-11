# Deviations from PLAN.md

Every place where the implementation departs from `PLAN.md`, with the reason, recorded in the same PR
as the change. Citation drift against the baseline (`BASELINE.md`) is recorded here too.

## Citation drift

None. The baseline commit is the commit the plan was verified against (`BASELINE.md`).

## Deviations

### Tooling: `docs/` excluded from oxfmt and oxlint (slice 0)

`docs/**/*` is added to `ignorePatterns` in `.oxfmtrc.json` and `.oxlintrc.json`. oxfmt formats
Markdown and reflows the fenced code blocks in `PLAN.md`, destroying the aligned type listings, and
`research/maintainer-report-comments-branch-analysis.ts` is a reference copy of upstream code, not
project source. Without the exclusion `pnpm lint:ci` (`oxfmt --check`) fails on the planning docs.

## Questions for upstream

Decisions that need the upstream maintainer's input before a slice can be proposed upstream. The fork
keeps the plan's behaviour behind its settings flags until they are answered.

None yet.
