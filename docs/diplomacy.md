# Diplomacy

The diplomacy system (tasks 63, 64 and 91). **Status:** the core is built (task 63, `crates/f4x-core/src/diplomacy.rs`:
sections 2 to 7 and the hooks of 9.3), the AI answers proposals (`crates/f4x-ai/src/diplomacy.rs`) and the console can talk
(`crates/f4x-console/src/talk.rs`). Not built: the diplomacy screen (task 91), personalities and what the AI starts on its own
(tasks 64 and 65) and the leftovers of task 167. This document records the original's rules
(taken from the reverse-engineered `DIPLOMAC.c`, `INITGAME.c`, `Spells128.c`, `Combat.c`,
`CONQUEST.c` and `Explore.c` in `reference/ReMoM`, plus quotes of the official strategy guide
(OSG) kept in its notes), then how to build it. Numbers below were read from that code; where the
code is marked buggy or unfinished by its authors that is said.

## 1. Overview

Two or more wizards (human and AI) each hold, towards every other wizard they have met:

- a **relation** (visible, -100 to +100) shown as a word from *Hate* to *Harmony*,
- a **treaty state**: none, wizard pact, alliance, or war (the original also has a "final war"
  value that no code ever sets),
- hidden **core reaction** terms and three **patience** counters that decide how willing an AI is
  to agree to a proposal,
- a **peace duration**, a number of turns in which an AI will not declare war on the human.

Relations drift towards a per-pair default, are pushed by what wizards do (attack cities, cast
hated spells, break treaties, give gifts) and are tested whenever someone proposes something.
Each turn the AI wizards decide whether to talk to the human (greeting, warning, demand, war or
peace), and among themselves they make treaties, trade spells and declare wars.

## 2. Data model (per ordered pair unless noted)

From `s_WIZ_DIPL`. `A -> B` means what wizard A holds about wizard B.

| Field | Meaning |
|---|---|
| `contacted` | The wizards have met. Symmetric. |
| `contact_stage` | Unmet, Greeted (AI greets the human next turn), Established. |
| `status` | `NoTreaty` 0, `WizardPact` 1, `Alliance` 2, `War` 3, (`Crusade` 4, unused). Symmetric. |
| `visible_rel` | The relation shown to the player, -100..100. Kept equal in both directions. |
| `hidden_rel` | Permanent core reaction: oath breaking, gifts. Not shown. |
| `default_rel` | The value `visible_rel` drifts back to; set at the start from the books, lowered by wars. |
| `treaty_modifier`, `exchange_modifier`, `peace_modifier` | "Patience": how receptive the AI is to treaty, spell-exchange and peace proposals. Clamped, recover over time. |
| `peace_duration` | Turns left in which this wizard will not attack the human (the "peace treaty"; it is not a status). |
| `hostility` (0-3) and `reevaluate_hostility_countdown` | How hostile the wizard feels; feeds the AI's war planning. |
| `da_action`, `da_strength`, `da_spell`, `da_city` | The last strongest thing the human did to this wizard; the AI reacts to it next turn. |
| `broken_treaty` | Which treaty the other side broke, for the oath-breaker remarks. |
| `warning_progress` | Counter for "get out of my territory" warnings (0, 1, 2). |
| `tribute_gold`, `tribute_spell` | What the human offered in the current talk. |

Per wizard: `personality` (Maniacal 0, Ruthless 1, Aggressive 2, Chaotic 3, Lawful 4,
Peaceful 5), `objective` (Expansionist, Militarist, Perfectionist, Pragmatist, Theurgist), and the
`Charismatic` retort (already in `wizard/magic.rs`).

### The relation scale

`level = (100 + visible_rel) / 20`, shown as: 0 Hate, 1 Troubled, 2 Tense, 3 Restless, 4 Unease,
5 Neutral, 6 Relaxed, 7 Calm, 8 Peaceful, 9 Friendly, 10 Harmony. (Red for 0-3, yellow for 4-6,
green for 7-10.) The original's help text calls these "diplomacy points"; a wizard's pact needs
a relation above +10 ("relaxed or better"), an alliance above +50.

### Where it starts (`INITGAME.c`)

For every pair of wizards: `max_death` = the higher Death books, `sum_life` = both Life books,
`max_chaos` = the higher Chaos books. If both `sum_life > 0` and `max_death > 0` the start score
is `-(sum_life + max_death) * 5`; otherwise `sum_life * 2 - max_death * 3`. Then `- max_chaos * 2`,
and `+ 2` for every book realm (Sorcery, Chaos, Nature, Life) the two share, counted as the
smaller of their books. The score is raised to at least -90 (`SETMIN(score, -90)`; ReMoM's
`SETMIN` is a maximum), becomes both the visible and the default relation, and the peace
duration starts at 0.

## 3. Contact

Wizards are *contacted* when one of them scouts a square holding the other's city or a visible
unit (invisible units by enchantment do not count), or when Nature Awareness is cast (all
wizards). It is checked as the map is explored (`Explore.c`) and once per turn over all units
(`NEXTTURN.c`). On contact the human's stage becomes `Greeted`; the next turn the AI opens a
greeting by personality (`do_Greeting + personality`), after which the stage is `Established`.
Until contact, the diplomacy screen shows an empty frame and nothing else applies.

## 4. What the player can do (the talk)

Reached from the magic screen by clicking a wizard's portrait; right click opens that wizard's
sheet (statistics, personality, objective). A talk starts with a greeting that depends on relation
(`Diplomacy_Greeting`): an AI that is too cold refuses an audience (the worst patience counter is
below -100: "cold shoulder"). Menu:

1. **Propose Treaty:** Wizard Pact, Alliance, Peace Treaty, Declaration of War on Another Wizard,
   Break Alliance With Another Wizard, Forget It. Entries not allowed in the present state are
   greyed: pact only with no treaty, alliance only below alliance, peace only at war. War on
   another needs a third wizard known to both and not already at war with the wizard; breaking
   an alliance needs a third wizard known to both. As read, the war entry is always greyed (the
   code greys it when the status is `>= NoTreaty`, which is every status) and the break-alliance
   list is of wizards the other is *not* allied with, which looks inverted; both are probably
   bugs, see §10.
2. **Threaten / Break Treaty:** Break Wizard Pact, Break Alliance, Threaten to Attack.
3. **Offer Tribute:** gold, or a spell the other wizard lacks (value-ordered).
4. **Exchange Spells:** swap one spell for another.
5. **Good-bye.**

### The test (`Diplomacy_Test`)

Every proposal is rated with
`final = relations + Random(100) - type_modifier + threat (+40 if allied)`, where
`relations = patience[type] + hidden_rel + visible_rel + personality_mod`,
`personality_mod = {0, 10, 20, 30, 40, 50}[personality]` (Maniacal 0 ... Peaceful 50; the array
is flagged "like" in the notes, unconfirmed), and `threat` compares the army strengths on the
other wizard's home continent (costs of both sides' units): if the human's is larger it is
`-50 + 100 * human / ai`, +50 if the AI has none; if smaller `-50 + 100 * ai / human`, -50 if the
human has none. Result: `final < -75` refused (0); `< -50` refused, a counter-offer would be due (1);
`< 0` maybe (2); otherwise accepted (3). Results 1 and 2 are turned into a yes with 50 percent
(the original's counter-offers are a TODO; a 1 that stays 1 shows no answer at all). After every
test the three patience counters drop by 10 and the tested one by a further `20 + Random(30)`
(treaty), `50 + Random(50)` (peace) or `20 + Random(50)` (exchange). Note that every proposal in
the table below, the peace treaty included, is tested with the **treaty** patience (type 0); the
peace and exchange types are never passed to the test for proposals.

| Proposal | Requires | `type_modifier` | On yes |
|---|---|---|---|
| Wizard Pact | relation > 10, else automatic no | 75 | status 1, hostility 0 |
| Alliance | relation > 50 | 125 | status 2 |
| Peace Treaty | (at war) | 60 | status 0, relation +40 (a typo adds it twice), peace duration `7 + Random(6)` turns for the human, hostile city curses between the two cancelled, both re-evaluate hostility in `15 + Random(10)` turns; patience falls by 20 more |
| War on a third wizard | not the human themself | 150 + the AI's relation to the third (or, if the human is allied with both: yes half the time, `Random(8) < 5`) | the AI declares war on the third wizard |
| Break alliance with a third | relation to the human > 24 else "maybe" | 175 + the AI's relation to the third | the AI breaks its treaties with the third |

**Threaten to Attack:** `relations = hidden + visible + personality_mod + threat + Random(100) +
Random(100)` with a threat that is `±100` if one side has no army. The human loses `Random(15)`
relation, the treaty patience becomes -120, and the AI's reaction is: below 0 it declares war;
0-89 it refuses; 90-199 it pays `25 * (2..9)` gold (at most its reserve) and promises peace for
`Random(15)` turns; 200 or more it gives a spell instead (and the same peace). Ten patience
decays are applied after any threat.

**Break treaty:** `Break_Treaties` (§6), no test.

**Tribute:** the list offers gold at 25, 50, 75 and 100 percent of the AI's reserve (rounded down
to 25; the code reads the AI's gold here, and greys amounts below the human's gold, which looks
inverted) or a spell. Gold: the relation rises by `4 * choice * Random(3) / 10` with choice 1 to 4
(`8 * Random(3) / 10` when the AI has no army); a branch adds it twice. A spell: the AI gets the
spell, the relation rises by `base * (Random(8) + Random(8)) / 10` with `base = (100 - rel) / 10`
(20 if the relation is negative), peace patience by the same up to 100, hidden relation by
`2 + Random(8)` up to 30.

**Exchange spells:** the AI builds a list of spells it could give; the player picks one, then one
of their own to give in return. The code computes a score (`-125 + hidden + visible / 2 +
personality_mod + Random(100)`) but does not use it; the only gate is that both sides must have
a spell the other lacks, else "nothing to trade". If the human's spell has the higher research
cost than the one received, relation +5 and the patience falls by two more steps. Afterwards
the patience drops three steps and exchange patience is capped at 50.

## 5. Changing relations (`Change_Relations`)

All events call `Change_Relations(value, attacker, defender, type, city, spell)`; the defender's
relation to the attacker moves, the pair is mirrored, and the human's strongest recent action per
wizard is remembered (`da_action`, `da_strength`) so the AI can comment next turn. Rules:

1. Both must be contacted; neither may be a monster, and nothing is done to the human defender
   (the human is never "changed").
2. **Diminishing returns:** a positive change on a defender whose relation is negative is doubled
   (capped so the result is at most +10); on a non-negative one it is divided by `rel / 25 + 1`.
   A negative change on a positive relation is doubled; on a non-positive one it is divided by
   `-rel / 25 + 1`.
3. **Personality of the defender:** Lawful doubles negatives; Maniacal makes negatives 1.5 times
   and positives 0.75 times.
4. **Charismatic attacker:** positives doubled, negatives halved.
5. If the pair are at war and the type is one of the first 14 (reaction events, not talks), a
   negative change is not applied; a quarter of it (only if the attacker is the human) moves the
   peace patience instead.
6. *(Not ported: the AI gets no special treatment, design.md section 8. This step and the difficulty-based war roll in section 8 are the original's AI safeguards and bonuses; the AI decides war, peace and proposals on its own logic instead.)* AI-to-AI changes are doubled when positive after turn 200, and divided by `difficulty / 2 + 1`
   when negative after turn 200 (an AI-only safeguard against late-game wars).
7. Applied: relation and the treaty and exchange patience move by the change; peace patience moves
   by a quarter of the pre-charisma change (negative) or the pre-charisma change (positive). The
   Earth Lore spell is mistakenly checked where Spell of Mastery was meant: an 11 percent chance
   of war. Relation is clamped to -100..100, capped at +65 unless allied, and capped at -25 when
   at war or worse; at war hostility becomes 3, allied 0; the reevaluation timer restarts.

Special value `-10000` means "declare this on the record": no numbers change, the human's
`da_action` is set to the type.

### Who triggers what

| Event | Change | Type | Source |
|---|---|---|---|
| Wizard pact in force | `+Random(3)` per turn, half the turns | 0 | `Diplomacy_Growth` |
| Alliance in force | `+Random(6)` per turn, half the turns | 0 | `Diplomacy_Growth` |
| Human army too strong | `-10` when `Random(100) >= human_strength * 50 / ai_strength` and a d20 = 1 (as written this never happens once the human has twice the AI's strength, unlike the OSG text, which says the penalty is certain at double strength) | 7 | `Diplomacy_Growth` |
| Too many cities (on even turns) | negative, grows with the surplus over `(landsize + 1) * 3`; AI-to-AI changes halved; at least -15 | 14 | `Diplomacy_Growth` |
| Attacking a wizard you have a pact or alliance with (a confirm box "You have a treaty with X. Do you still wish to attack?"; on yes the treaty is broken, see §6) | `-40` | 5 | `Combat.c` |
| Taking a city of another wizard (any city with population) | `-40`, **and the attacker declares war** (`Declare_War`) | 9 | `Combat.c` |
| Attacking a city of another wizard and failing (the defender wins) | `-60` if it is the defender's fortress city, else `-20` (only when the defender wins, as the code is written; the document had both added up) | 9 | `Combat.c` |
| Battle against an AI wizard's units ("defensive battle losses") | `-diplomatic_value` (computed from the losses; the formula was not read), only when the defender is an AI | 8 | `Combat.c` |
| A wizard is destroyed / a fortress is taken | `-40` / `-20` from the winner to every other wizard (two code paths in `CONQUEST.c`; the first also erases contact with the loser) | 7 | `CONQUEST.c` |
| Bad city spell (Earthquake -9, Corruption -25, Raise Volcano -50, Chaos Rift -5, Call the Void -20, Evil Presence -5, Famine -5, Cursed Lands -5, Pestilence -8) | by spell | 13 | `Spells128.c` |
| Overland enchantment (Spell of Mastery -50, Eternal Night -12, Evil Omens -20 vs Life/Nature wizards, Zombie Mastery -14, Wind Mastery -4, Suppress Magic -4, Time Stop ...) | divided at cast-time divisor | 10, 12 | `Spells128.c` |
| Aura of Majesty | `+10 / divisor` to everyone | 0 | `Spells128.c` |
| AI-to-AI friendly gravitation | `+5 + Random(5)` one in 20 turns | 1 | `NPC_To_NPC_Treaty_Negotiations` |
| Units of another wizard within two squares of a city (not settlers or builders) | pact or alliance: `-Random(5)` and a warning, a turn of grace, then `-5` and both treaties broken; no treaty: 6 percent a turn of `-Random(5)` | 73, 5, 0 | `DIPL_GetOffMyLawn` |
| Gift of gold or a spell | positive (§4) | direct | talk |

## 6. War, peace and breaking treaties

**`Declare_War(attacker, defender)`**: refused if either is neutral or they are already at war.
Breaks treaties (below). `default_rel` of both drops by 5. The relation becomes `-75 - Random(25)`
(the code writes the war constant 3 into the second direction by mistake, which the mirror fixes
later). Treaty and exchange patience become -200, peace patience -130, hostility 3, and both
re-evaluate hostility in `15 + Random(10)` turns.

**`Break_Treaties(breaker, victim)`**: not done at war. The oath-breaker penalty is -10 for a
wizard pact and -20 for an alliance (doubled for a Lawful breaker). In the original it is
*subtracted* from a negative number, so the stored hidden relation *rises*; the intent (OSG:
"every treaty you break labels you as untrustworthy and permanently worsens your relations with
all wizards") is a fall. Everyone else's hidden relation to the victim falls by 5 (this is the
original's reading of "rest of the world"; the victim, not the breaker). In an alliance the penalty
also lowers `default_rel` (to at least -100). The breaker's relation to the victim falls by
`Random(20)` if a real treaty was broken. Both sides' status becomes `NoTreaty`, all three
patience counters -200, and the broken treaty is recorded.

**Peace** (§4) and AI-to-AI peace: two AIs at war sign peace when `x + peace_patience > 100`,
where the code uses a leftover message index for `x` (a bug; intended is probably a random roll).

**Alliances pull in friends:** `G_DIPL_PickSides` breaks an alliance when one ally is at war with
the other's ally. In `NPC_To_NPC_Treaty_Negotiations`, an AI allied with another declares war on
whoever that ally is at war with. `NPC_Required_Alliances`: with 10 percent a turn, an AI ally of
the human who is at war with another wizard the human is not at war with asks the human to join.

## 7. The turn

In `Next_Turn_Calc` (`NEXTTURN.c`) the order is: `Decrease_Peace_Duration` (all peace durations
-1, at least 0) early in the turn, then

1. `Diplomacy_Growth_For_Enchantments`: relation effects of enchantments in force.
2. `Diplomacy_Growth`: pact/alliance gravitation; army-too-strong and too-many-cities reactions;
   then **gravitation to default**: for each AI wizard and other wizard with no recent action, if
   `Random(105) > |rel|` the relation moves `Random(2)` towards `default_rel`; (towards it from
   below every turn, from above only every tenth turn); finally relations are mirrored.
3. `Determine_First_Contacts`: a wizard met this turn becomes `Greeted` with no treaty.
4. `NPC_To_NPC_Treaty_Negotiations` (AI with AI). For each ordered pair of living AI wizards that
   have met, with chance `1 / (15 - 2 * difficulty)`: a score `hidden + visible + personality_mod +
   Random(100) + (3 + Random(5)) * difficulty` (+10 pact, +20 alliance); above 150 with the
   treaty patience they form an alliance, or else a pact; if exchange patience + 80 is below the score
   they swap one spell each. They then maybe gain relation, make peace if at war, lower patience
   three steps, and `G_DIPL_PickSides`, `G_DIPL_NeedForWar` and `DIPL_GetOffMyLawn` run.
5. `NPC_To_Human_Diplomacy` (§8) and `Resolve_Delayed_Diplomacy_Orders` (the human sees the result
   of every AI's decision as a message or a talk screen).
6. `End_Of_Turn_Diplomacy_Adjustments`: each patience counter +10 while below 100.
   `Modifier_Diplomacy_Adjustments`: patience that is negative or below 50 gains `Random(5)` twice;
   peace patience is clamped to -200..200; the last actions (`da_*`) and gifts are cleared.

## 8. How the AI decides

**Towards the human (`NPC_To_Human_Diplomacy`)**, once per turn per wizard (human is player 0 and
skipped; wizards without a fortress and "final war" wizards do nothing):
1. A newly met wizard greets (by personality).
2. Otherwise `score = hidden + visible + worst_patience + personality_mod`. If `score + visible >=
   -100` nothing happens. Otherwise, with `da = |da_strength| * 2` (how bad the recent action
   was): a negative recent action opens a *war-or-peace* decision with chance `da / 75`; else at
   war with a sufficiently positive peace patience (`30 + Random(100) < peace_patience`) the AI
   considers making peace; else with `Random(100) < da + 3` and 1 in 4 it opens a demand or a
   required-alliance request.
3. `G_DIPL_NeedForWar` and `DIPL_GetOffMyLawn` run as fallbacks if nothing else was decided.

**War or peace (`DIPL_HumanWarOrPeace`)**: with no war yet: relation <= -75 declares war; otherwise
`hidden + visible + personality_mod - Random(100) + treaty_patience <= -100` records a warning (a
counter); a second or third warning, or no treaty, rolls `Random(100) < -(da + mod/2)` to declare
war, else the AI breaks its treaty. At war: `score >= 50` (with `Random(100)` and the peace
patience) and a 1 in 8 chance makes the AI offer peace.

**Need for war (`G_DIPL_NeedForWar`)**: skipped before turn 100 (for the current player), in a
treaty, or without a fortress. A *Chaotic* wizard declares war on the human on `Random(300) <=
difficulty` (recorded as action 40 with strength 2000). A Militarist or Expansionist, not Lawful, rolls a d20
= 1 each turn; any wizard rolls a d20 = 1: if `relation + superiority + personality_mod + hidden
<= -150` it starts a *superiority war* (superiority = ±(army strength ratio) as in the test). An
AI whose allies are at war with the human may join with 1 in 10. At difficulty 2 or more, if the
human is at war with fewer than `difficulty` wizards and the relation is below -30, a
`Random(15) <= -rel / 10` roll declares war (the more they dislike each other, the likelier).
Superiority war against a treaty partner breaks the pact or alliance first (alliance only at
difficulty 2+, with chance 1/4 or 1/2 for an Expansionist; pact half the time or if Expansionist),
and caps the relation to 30.

**Get off my lawn (`DIPL_GetOffMyLawn`)**: non-settler units of a wizard inside two squares of
another's city (on its plane, or in a tower) trigger a warning sequence: first a mild relation
loss `-Random(5)` and a warning, a second step of nothing, and a third step the treaty is broken
with `-5`. Wizards at war ignore it.

## 9. Implementation plan

New module `crates/f4x-core/src/diplomacy.rs` (and `diplomacy/` submodules if it grows), data in
`Game`, saved and loaded with the game (`game/save.rs`).

**9.1 State.** `Diplomacy` in `Game`: a vector of `Relation` records indexed by ordered pair
(`PlayerId` is a `u8`; PLANNED item 3 wants more than four rivals, so size it by the number of
players, not a constant 6), plus per-player `Personality` and `Objective` (task 64). Content-
defined tables (personality mod, message tables) in `rules.ron` where the numbers are data.

**9.2 Commands and events.** `Command::Diplomacy(DiplomacyCommand)` with `Greet`, `Propose{to,
kind}`, `Threaten{to}`, `BreakTreaty{with}`, `OfferGold{to,amount}`, `OfferSpell{to,spell}`,
`ExchangeSpells{to,give,get}`, each answered by `Event::DiplomacyAnswer { from, kind, result }`
(accepted/refused/counter), `Event::TreatyChanged`, `Event::WarDeclared`, `Event::RelationChanged`
(for the log and the AI's memory). The talk itself is a **state machine in the core** (so it can
be replayed, saved and tested without a client), but the menus and dialogue text belong to the
client.

**AI decisions are not hard-wired in the game.** The core holds the state (relations, treaties,
patience), checks that a proposal is legal, and applies the result. It never decides whether an AI
wizard *accepts* a proposal, and it never makes an AI wizard open a talk, threaten, declare war or
offer peace. A proposal made to an AI wizard becomes a pending proposal (`Event::ProposalMade`
addressed to the receiver). The receiving AI evaluates it with its own logic in `f4x-ai` and
answers with a command (`Accept`, `Refuse` or `Counter`) like any other player. The same goes for
the AI's own initiatives (§8): the per-turn greeting, demand, war or peace decisions are commands
the AI issues from its turn, not code in `next_turn`. This way a different AI (a scripted one, an
MCP client, a language model, a remote player) can answer the same proposals through the same
hooks, a human receiver is asked instead of evaluated, and the AI can be changed or tuned without
touching the rules. What the core does offer is data and helpers the AI may use: the visible
relation, what a wizard may know about the other (never the hidden terms of another player), and
the original's rating of a proposal (`Diplomacy_Test`, §4) as an optional function. The default AI
uses that function and the §8 rules as its starting logic, kept as AI code with its numbers in
data, not as rules of the game. The core's own automatic parts remain rules: relation changes from
events (§5), drift and growth (§7), and the effects of an accepted treaty or declared war (§6).
Pending proposals are saved with the game; if the receiver does not answer in the same turn the
proposal lapses as refused.

**9.3 Hooks into existing code:**
- contact: in exploration and the per-turn unit sweep (the fog and visibility code);
- `Change_Relations` calls from battle results (task 42), city capture, city spells (task 25),
  global enchantments (task 24) and the Aura of Majesty (task 29);
- a battle between wizards whose treaty forbids it needs the treaty check in `AttackRequested`:
  **attacking a wizard you have a pact or alliance with first breaks the treaty** (original:
  `Break_Treaties` before the first blow);
- the overland AI (task 65) uses relation, treaties and `hostility` to choose targets and to
  stop attacking friends.

**9.4 Order of work:**
1. Data model, save format, relation scale and the words (tests: the scale, mirroring, clamps).
2. `Change_Relations` with every rule of §5 (table-driven tests per rule).
3. Treaties: `Start_Treaty`, `Declare_War`, `Break_Treaties`, `Declare_Peace`, and the oath-breaker
   rules (tests for both signs of the original bug, see §10).
4. Contact and the first-meeting greeting.
5. The talk: pending proposals, `Accept`/`Refuse`/`Counter` commands and the effects of each
   proposal of §4; `Diplomacy_Test` as an AI helper with a fixed seed (golden values).
6. Per-turn processing of §7, in the original's order, still without AI decisions.
7. Personalities and objectives (task 64): assignment at game start, the personality modifier,
   greeting tables.
8. AI decisions towards the human and among themselves (§8), written in `f4x-ai` together with
   the overland AI (65), not in the core.
9. Client (task 91): magic screen portrait click, the diplomacy screen with the original art
   (`diplomac.lbx` backgrounds, portraits `moodwiz`/`lilwiz`, gargoyle eyes, mirror
   `backgrnd` 18, treaty icons `magic.lbx` 60-74), message lists, the human-side menu with greyed
   options, the AI's messages, the wizard sheet on right click; plus the treaty icons next to the
   portraits on the magic screen and the status word.

**9.5 Messages.** The original's dialogue (`DIPLOMSG.LBX`) is original text; load it from
`content-local` only. The open-source game needs its own default lines per message kind
(greeting by personality, accept, refuse, counter, threat, war, peace offer, tribute thanks,
get-off-my-lawn warning, breaking a treaty) in `content/`.

## 10. Original quirks and how this game treats them

The original contains several mistakes marked by its decompilers. The default here is to implement
the **intended** behaviour (the strategy guide says what it was) and keep the original's number in
a named constant so a rule set can restore it (PLANNED item 5, rule sets):

| Original | Observed | This game |
|---|---|---|
| `Break_Treaties` subtracts a negative modifier | The hidden relation of the breaker rises | Falls (intended) |
| `Declare_Peace` adds +40 twice and clamps once | +80, clamp 100 | +40 |
| `Declare_War` stores the war constant into the other side's relation | Fixed by the mirror a moment later | Not reproduced |
| Spell of Mastery's 11 percent war is tested on Earth Lore | Cast Earth Lore: 11 percent war | On Spell of Mastery |
| `Diplomacy_Offer_Tribute` adds the gold gift's relation twice in one branch; amounts come from the AI's gold and are greyed by the human's | Odd | Gold offered from the human's reserve, gain applied once |
| The war-on-a-third-wizard entry is always greyed; the break-alliance list is of non-allies | Cannot be used | Enabled when a legal target exists; break-alliance lists allies |
| Proposals use the treaty patience for all types, peace included | Peace proposals wear out treaty patience | Same until play shows it is wrong |
| "Get off my lawn" first warning does nothing beyond a counter in one branch | Faithful | As in the original, until it is tested in play |
| Counter-offers (`IDK_Npc_Counteroffer`) | Not written; yes with 50 percent for results 1-2 | Same, until a counter-offer design exists (§11) |
| `Find_Worst_Modifier` is documented as the lowest of the patience counters but its comparisons are muddled (it also starts from 0) | The "cold shoulder" test is unreliable | Lowest of the three (intended) |
| "Final war" status | Never set | Left out |
| Attacking a treaty partner calls `Break_Treaties(defender, attacker)`: the victim is the breaker | The oath-breaker penalty is the victim's | The attacker breaks the treaty and takes the penalty |
| `Break_Treaties` lowers every other wizard's hidden relation to the victim by 5 even if no treaty existed (so each war declaration does) | As the original | As the original (tested) |
| Gravitation to the default runs for each AI toward each wizard, then the copies are mirrored in a way that depends on the order | Each AI pair drifts about twice a turn | Once per pair, by the AI's own record (the lower number's between two AIs) |
| `Change_Relations` after turn 200 doubles AI-to-AI friendships and softens AI-to-AI enmities by difficulty | Safeguards for the AI | Not ported (design.md section 8) |
| The Spell of Mastery's 11 percent war is tested on Earth Lore | Cast Earth Lore: 11 percent war | On the Spell of Mastery (as in the table above), tested |
| Gold tribute: amounts come from the AI's reserve and the human pays them | Odd | The giver chooses an amount; the share of the receiver's reserve it makes (a quarter to all) gives the choice 1 to 4, gain applied once |

Anything in the table must also be written down as a test so the choice is visible.

## 11. Open questions

- The values in OSG Table 25.2 (permanent relation modifiers) and the text of `DIPLOMSG` are not
  in the notes read; get them from the manual in `reference/` before fixing numbers in rule data.
- The three-way counter-offer ("if you also give me a spell I will accept") in the original is
  not implemented there. Do we add it (beyond the original, `docs/PLANNED.md`)?
- More than one human (hot seat, PLANNED 17) and asynchronous play (18) mean the human-only
  assumptions of §8 (player 0 is the human) must become "every human wizard".
- Should the AI explain its refusals (which patience was too low) for a better player experience
  than the original's single phrase?
- Victory interaction: Spell of Mastery and the conquest rules (task 67) feed relations; the
  victory screen does not need diplomacy.
