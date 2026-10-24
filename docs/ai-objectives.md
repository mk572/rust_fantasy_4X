# AI objectives: steering the scripted AI

Task 238. Design for letting a wizard's objective (task 64: `wizard/objective.rs`, drawn at game start)
steer the strategic AI in `crates/f4x-ai`. Part of the scripted AI ([`ai-design.md`](ai-design.md)).

## 0. Status (24 October 2026)

Steps 1, 2 and 4 are built in `crates/f4x-ai/src/objective.rs` (the table, `Traits`), read by `economy::plan_for`
(cities wanted, strike army, the focus of a city), `wizardry::split` and `research_score` (power split and
research likes) and `diplomacy::quarrel` (war on a weaker neighbour). The personality moves the strike
army (`army_bias`: Maniacal +2 to Peaceful -1; ours, the original keeps personality in diplomacy). Difficulty slides every temper towards Maniacal (task 148: Hard one step, Impossible two; `objective::slid`), read by `army_bias`, `quarrel`, the raze score and the diplomacy modifier; steps past Maniacal add fighters to the strike army. Step 3
(building weights by category, casting as weighted categories) is open: task 366. The `AIK_CITIES`,
`AIK_STRIKE` knobs still override the table. First soak numbers: `ai-design.md` section 12.

## 1. State today (before the steps above)

Every AI wizard has an objective (Pragmatist, Militarist, Theurgist, Perfectionist, Expansionist)
and a personality. Nothing in `f4x-ai` reads the objective; the AI is a fixed ladder of rules with a
few constants, so there are no weights to scale yet.

| Place | What it is now | Hook for an objective |
|---|---|---|
| `economy.rs` `WANTED_CITIES` (7) | cities settled before the AI only builds up | the city target |
| `economy.rs` `STRIKE_BASE` (4) | fighters beyond the home guard | the army target |
| `wizardry.rs` `SPLITS` and `split` | power split, first affordable candidate | the research share |
| `wizardry.rs` `research_score` | a real score, the turns to learn times factors | a factor per spell kind |
| `wizardry.rs` `priority` | fixed order of spell kinds | not changed here (step 3) |
| `economy.rs` `plan_for` (was `BUILD_ORDER`) | the focus, mix and settlers of each city's auto-builder plan (`docs/auto-builder.md`) | the focus pair an objective gives a city |

The original uses the objective in four places: the expansion rate (`AIMOVE.c`), building weights
by category (`AIBUILD.c`), the magic investment (`AIDUDES.c`) and war declarations (`DIPLOMAC.c`).

## 2. Principles

- Objectives change **emphasis, not capability**: no objective forbids anything, and the rules of the
  ladder stay (never leave a city empty, no army the treasury cannot feed).
- One table of numbers per objective, in one place (`f4x-ai/src/objective.rs`), read through
  `Ai::objective()`. No tuning numbers scattered through the rules.
- The `knob` environment overrides keep working and win over the table, for experiments.
- Pragmatist is the neutral row: it keeps today's numbers, so a Pragmatist plays as the AI does now.
- The human has no objective; the learned AI and tests that build an `Ai` by hand get Pragmatist.

## 3. Steps

Each step changes play and gets its own soak, run only when asked (CLAUDE.md, Commands).

1. **Plumbing.** `PlayerView::objective()` for the AI's own wizard, the table type with the neutral
   row, tests. No change in play.
2. **The small hooks**, one table row per objective:

   | Objective | Cities wanted | Strike base | Research share | Research score |
   |---|---|---|---|---|
   | Pragmatist | 7 | 4 | as now | as now |
   | Militarist | 6 | 6 | lower | x1.5 for summons and unit buffs |
   | Theurgist | 6 | 3 | higher | x1.5 for life and arcane spells |
   | Perfectionist | 5 | 3 | higher | x1.5 for city buffs, global buffs |
   | Expansionist | 10 | 4 | as now | x1.5 for explore and transport spells |

   The numbers are a first guess. The original's expansion factors are 4, 3, 2, 2, 1 for
   Expansionist, Theurgist, Militarist, Pragmatist and Perfectionist, which fixes the order of the
   city targets, not their size.
3. **Weighted choices** (a redesign; decide after step 2): the original's building weights by
   category and objective, and the overland casting as weighted categories instead of a fixed
   priority. Needs its own design pass, and changes how the AI plays more than step 2 does.
4. **Diplomacy.** A Militarist or Expansionist wizard that is not Lawful may declare war on a weaker
   neighbour (`DIPLOMAC.c`, a 1 in 20 chance a turn). Belongs with the AI's diplomacy
   (`f4x-ai/src/diplomacy.rs`).

## 4. Testing and measuring

- Unit tests: the table, and that each objective moves its numbers in the stated direction.
- A behaviour test per step 2 hook (for example an Expansionist settles beyond 7 cities when land is
  free); these go with the `#[ignore]`d AI tests.
- Soak (on request): 8 seeds, compare the winners and city counts with the Pragmatist baseline. An
  objective is not tuned to win more: the check is that no objective plays worse than a broken AI.

## 5. Open questions

- Should a personality steer the same numbers (an Aggressive wizard builds a larger army)? The
  original keeps personality in diplomacy, the objective in economy and war.
- Should the human's rivals show their objective truthfully, or can the sheet be wrong (a wizard that
  has changed its mind)? The original shows it truthfully and never changes it.
