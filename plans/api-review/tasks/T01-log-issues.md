# T01 · Log the review findings in ISSUES.md

**Area:** docs · **Issues:** creates I-17 to I-35

## Goal
Add one `ISSUES.md` entry per finding in `plans/api-review/review.md`, using the IDs
below, so later tasks can cite them and resolve them.

## ID map (fixed; don't renumber)

| ID | Finding | Severity |
|---|---|---|
| I-17 | B1 | high |
| I-18 | B2 | low |
| I-19 | C4 | medium |
| I-20 | A1 | high |
| I-21 | A2 | medium |
| I-22 | A3 | medium |
| I-23 | A4 | medium |
| I-24 | A7 | medium |
| I-25 | A8 | low |
| I-26 | C3 | low |
| I-27 | B3 | high |
| I-28 | C1 | medium |
| I-29 | A5 | low |
| I-30 | A6 | low |
| I-31 | C2 | low |
| I-32 | D1 | low |
| I-33 | D2 | low |
| I-34 | D3 | low |
| I-35 | P2 (in `review.md` under "Added during planning") | medium |

## Requirements
- Follow the existing entry format exactly: `### I-NN · Title · severity · confirmed`,
  then **Where:**, **Problem:**, **Fix:**, **Done when:**, separated by `---`. Read the
  existing I-13 to I-16 entries first.
- Before marking an entry `confirmed`, open each file:line the finding cites and check
  it. If the code differs, correct the line numbers. If the finding doesn't hold, leave
  the marker off and say so in your report.
- **Fix:** summarises the review's fix. Mention cross-references (I-15, I-16, I-14,
  I-13) where the review does.
- I-34 (D3): add "Deferred: revisit if the agent starts tagging."
- Put the new entries under **Open**, after I-16, in ID order.
- Don't touch existing entries.

## Out of scope
Any code change. Any edit to docs other than `ISSUES.md`.

## Done when
- `ISSUES.md` has I-17 to I-35 in the house format, and every file:line in them is
  current.

## Commit
`docs: log I-17 to I-35 from the API review`
