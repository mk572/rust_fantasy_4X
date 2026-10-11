# Conflicts: original help text vs the original implementation

Open items to resolve later. "Help" is the original game's in-game help (807 topics, in
`content-local/help.ron`; numbers in brackets are topic indices). "Implementation" is the
original game's data files plus the ReMoM clone, which is a faithful port of the original
code and marks places where that code departs from the manual with `OGBUG` comments.
Written in our own words; nothing here is copied from either source.

## How to read this
- **Verified** items were checked against a data file or a specific ReMoM function (named).
  **Check** items rest on a ReMoM comment or on the help text alone and need a second look.
- **Content now** says what `content-local/` currently does, so a decision can be applied
  by changing the importer (`scripts/import_mom.py`) rather than by hand.
- **Suggested** is a default for rows the resolution rule does not cover (see below).
- The full list of 633 `OGBUG` markers is not repeated here. Regenerate a local index with
  `python3 -I scripts/list_ogbugs.py` (writes `content-local/OGBUG_INDEX.md`, gitignored).
  By area: combat 214, AI 189, map/terrain 86, spells 70, cities/turns 34, other 28,
  presentation 12. Only the AI and presentation groups (about a third) are irrelevant to rules.

## Resolution rule (decided by the project owner)
1. Anything ReMoM marks as a **bug in the original game** (`OGBUG`): use ReMoM's fix, i.e.
   the behaviour its note says the code should have.
2. Anything ReMoM marks as **to-do / not implemented**: use the original game's behaviour.
3. Everything else (ReMoM reproduces the original code or data exactly, no marker, and the
   help disagrees) is not covered by the rule and stays open below.

### Decided by rule 1 (ReMoM-marked bugs)
| # | Decision |
|---|---|
| B9 Heroism | Combat uses the same level calculation as overland: Heroism sets the Elite floor, Warlord and Crusade add on top. (ReMoM's note offers two fixes and does not pick; this one matches the help.) |
| B10 Undead | Undead also get cold, poison and illusion immunity. |
| B11 Evil Omens | Applied by one spell-cost function everywhere (spell book, summoning, counter magic, item spells, combat). |
| B13 Blur | Checks the *attacker's* illusion immunity; no cap on the total reduction. |
| B14 Forester's Guild | Its food counts toward a city's maximum size. |
| B15 Swamp | Half a food. Needs terrain food in half-units (the sample `terrain.ron` uses whole numbers). |
| B16 Nature node | 4 food (ReMoM treats the main game's 5 as the odd one out). |
| B18 Events | Use the real turn number, so events run from turn one and after loading a save. |
| B19 Growth | The original suppresses shrinking at maximum size and at the largest size; here starvation and being over the limit still shrink (done, task 20). Surplus population carries over instead of being discarded. (Neutral cities growing at half rate is not a bug in the original, but we do not port it: neutrals get no bonus or penalty, design.md section 8.) |
| B20 Outposts | Done (task 21). Outpost growth includes wild game; no AI difficulty modifier applies to anyone (no AI advantage, design.md section 8). |
| B21 Disenchant | Dispels only other wizards' enchantments, by the usual chance (`dispel.rs`, task 59); the code's always-succeeding roll and its test against the caster are not followed. |

### Decided by rule 2 (ReMoM to-do)
| # | Decision |
|---|---|
| B12 Legendary fame | The original formula ReMoM has commented out: +3 fame per hero level, +4.5 for "super" legendary. This also matches the help. |

### Applied now
- B17: help and code agree (cities at least four squares apart); our setting was wrong.
  `min_city_distance` is now 4 in `content/rules.ron` and the default, with the city test
  updated. Blocking only on towers, nodes and *intact* encounter sites is done too (task 9).

### Decided: go with the game files (rows the rule did not cover)
ReMoM states these were verified one-to-one against the original assembly, so the code and
data are the original game and the help text is the odd one out. Decision: the game wins.
The imported help (`content-local/help.ron`) is rewritten to match, and each changed topic
ends with a note `[Updated from game code: ...]`. 16 topic edits, applied by
`HELP_CORRECTIONS` in `scripts/import_mom.py` (it fails loudly if a text no longer matches).

| # | Game behaviour now in help and content |
|---|---|
| A1 University | upkeep 3 gold |
| A2 Guardian Wind | upkeep 2 mana; full missile immunity (also fixes C4) |
| B1 Mana from guilds | Animist's Guild +3 mana; Alchemists Guild none (still +1 to hit weapons) |
| B2 Wizard's Guild | +8 research and +3 mana (not a cost) |
| B3 Infernal/Divine Power | +50% power and pacifying effect (Rjak's blurb corrected) |
| B4 Healer | extra 20% of total hit points per turn for the stack |
| B5 Sawmill | +25% of the whole city's production |
| B6 Wall Crusher | 50% melee, 25% ranged |
| B7 Dark Elf | +1 power per citizen |
| C1 Holy Armor | +2 defence |
| C2 Black Channels | +2 melee (the other bonuses were already right) |
| C3 Invisibility | -1 to hit against the unit (the two enchantment-list copies said -2) |
| D1 Weapon Immunity [415] | defence of at least 10 against *normal weapons* (melee, arrows, boulders, thrown); the help said "against magical attacks", which the code never does (task 35, `docs/combat.md` 2.1) |
| D2 Chaos [433] | doom damage (no to-hit, no resistance, no defence) is the full attack strength when the unit has it itself, half only when an item or spell gives it; the help said half (task 36, `combat.md` 2.2) |

Not changed:
- **B8 Stream of Life / Gaia's Blessing** stays open: the code reads the wrong table, which
  looks like a port error rather than the original's behaviour. Needs the Dasm listing.
- **C5 naming** (Cloud of Shadow/Darkness, Holy Light/Heavenly Light): both names occur in
  the game's own text, so both are left as is.

## A. Help disagrees with a data table

| # | Topic | Help says | Data/table says | Content now | Suggested |
|---|---|---|---|---|---|
| A1 | University upkeep [733] | 2 gold/turn | 3 (`BUILDDAT.LBX` record) | 3 | Keep 3 (data drives the game). |
| A2 | Guardian Wind upkeep [spell text] | 1 mana/turn | 2 (unit-enchantment upkeep table in the executable data, via ReMoM `unit_enchantment_upkeep_table`) | 1 (from help) | Use 2, the table the game actually charges. |

Everything else that could be matched automatically agrees: the casting cost of every spell
with a matching help topic (207 of 214; 7 have no help entry or a differently spelled
one), the other building upkeeps, all race build bans, the xp thresholds, and every other
unit, city and global enchantment upkeep that has a help topic (Armageddon, the five ward
spells and Nightshade have none). Spell damage values (Fireball/Ice Bolt/Lightning/Psionic base
5, Ice Storm 6, Fire Storm 8, Doom Bolt 10, Flame Strike and Star Fires 15) also agree.

## B. Help disagrees with the code's behaviour

| # | Topic | Help says | Code does | Status | Content now | Suggested |
|---|---|---|---|---|---|---|
| B1 | Alchemists Guild [732] | adds +3 mana | gives +3 mana to the *Animist's Guild* instead (`City_Mana_Production`); nothing for the Alchemists Guild | Verified | +3 mana on Alchemists Guild | Decide: original quirk (Animist's +3) or help (Alchemists +3). Prefer help; it is clearly the intent. |
| B2 | Wizard's Guild [734] | +8 research, **costs** 3 mana/turn | **adds** 3 mana (`City_Mana_Production`) | Verified | -3 mana | Prefer code (+3): a cost on the top research building contradicts how the game plays; confirm against the manual PDF in the game folder. |
| B3 | Infernal / Divine Power | Divine and Infernal: +50% [701, 708]; but Rjak's blurb says Infernal "doubles" [667] | 150% of religious-building power for both; unrest relief also x1.5 (`City_Mana_Production`, `City_Rebel_Count`) | Verified | not modelled | Use +50%; the blurb is the outlier. |
| B4 | Healer ability [399] | heals 25% of damage to the stack **after combat** | heals an extra 20% of total hit points to every unit in the stack **each turn** (`NEXTTURN.c` heal pass; the code quotes the manual's 20% per turn) | Verified | not modelled | Use the code (20%/turn); fix the help text. |
| B5 | Sawmill [728] | +25% production of **forest squares** | +25% to the city's **total** production (`City_Production_Production`) | Verified | +25% city production | Use the code. |
| B6 | Wall Crusher [398] | destroys a wall section 50% of the time | 50% in melee, **25% for ranged** attacks (combat code) | Verified | not modelled | Use the code (adds the ranged case). |
| B7 | Dark Elf cities [281] | grow slowly, no Cathedral/Maritime; nothing about power | +1 mana per citizen (others with a power race get half) (`City_Mana_Production`) | Verified | not modelled | Use the code; the help omits it. |
| B8 | Stream of Life [751], Gaia's Blessing | city enchantments | unrest code reads the **owner's overland-enchantment table** at the same index numbers (`City_Rebel_Count`), where those slots mean Life Force and Crusade | Check (not marked OGBUG; may be a port error) | not modelled | Verify against the Dasm listing; implement as city enchantments either way. |
| B9 | Heroism [473] | unit becomes elite | in combat, ignores Crusade, Warlord and xp when deciding the level (ReMoM note at the combat-init level code) | Check | not modelled | Follow the help; document the exception if parity is wanted. |
| B10 | Undead [553] | gain every immunity of death creatures | combat init grants only some of them; cold, poison and illusion immunity are missing for raised/black-channelled units (ReMoM note) | Check | not modelled | Follow the help. |
| B11 | Evil Omens [spell text] | all nature and life spells cost 50% more | several cost calculations (spell book display, summoning-master, counter-magic cost, item spells) ignore it (ReMoM notes in `Spellbook.c`, `SPLMASTR.c`, `CMBMAGIC.c`, `Combat.c`) | Check | not modelled | Follow the help: one cost function everywhere. |
| B12 | Legendary heroes [516] | +3 fame per level | formula exists but ReMoM has it disabled (`Player_Fame`, marked TODO) | Check | not modelled | Follow the help (+3/level, +50% for super). |
| B13 | Blur [797] | each point of damage has a 10% chance to miss | also checks the *defender's* illusion immunity instead of the attacker's; total reduction capped near half (ReMoM notes) | Check | not modelled | Follow the help; check the attacker's immunity. |
| B14 | Forester's Guild [744] | +2 food | applied to food production but left out of the city's maximum size, so it does not raise the growth cap (ReMoM `OGBUG`) | Verified | +2 food | Include it in the cap (intent). |
| B15 | Swamp food | the manual gives swamp half a food | swamp yields 0 (ReMoM `OGBUG`, `Terrain.c`) | Verified | food 1 in sample terrain | Decide; manual intent is 0.5. |
| B16 | Nature node food | not stated | 5 base food in the main game, 4 in the new-game generator (ReMoM `OGBUG`) | Verified | n/a | Pick one (4 for consistency?). |
| B17 | City spacing [400] | outpost must be at least four squares from any city | any city within distance 3 blocks it (`Map_Square_Survey`), i.e. at least 4 apart; only *intact* lairs, towers and nodes block | Verified | our `min_city_distance` is 3 (wrong) and every encounter blocks | Set 4; block only on towers, nodes, intact encounters. |
| B18 | Random events [768-775] | each ends with a 5% chance per turn | events are not processed during the first five turns after a game starts or is loaded (ReMoM `OGBUG` in `EVENTS.c`) | Check | not modelled | Count from the real turn number instead of launch time. |
| B19 | Population growth | "x people per turn" | excess above the next whole figure is discarded when a figure is gained; growth is clamped so it cannot go negative at maximum size; neutral cities grow at half rate (ReMoM notes) | Verified | done (task 20) | Follow the code except the neutral half rate, which is dropped (design.md section 8). |
| B20 | Wild game / outposts | outposts grow from food | outpost growth ignores wild game and applies the AI difficulty modifier to the human player too (ReMoM `OGBUG`) | Check | done (task 21) | Follow intent: include wild game; no AI modifier at all. |
| B21 | Disenchant Area and True [help topics of the same names] | dispel every *enemy* enchantment on the targeted square, each with the usual dispel chance | `Cast_Disenchant` (`OverSpel.c`) tests a city's enchantments only when the stored caster is the *casting* wizard, and rolls `Random(250) <= threshold + 500`, which always succeeds; the unit loop's bit test `>= 0` is always true. Read literally it would strip the caster's own enchantments every time; ReMoM carries it without an `OGBUG` mark, so it may be a disassembly artefact | Check | done (task 59): the help's rule, enemy enchantments only, with the dispel formula | Follow the help. If ReMoM or the original is shown to behave as the code reads, record that here. |

## C. The help contradicts itself

| # | Topics | Contradiction | Suggested |
|---|---|---|---|
| C1 | Holy Armor spell vs unit-ability entry [478] | +2 vs +3 defence | Data: spell text +2. Use +2 (content now). |
| C2 | Black Channels spell vs ability entry [453] | +2 vs +3 melee | Use +2 (content now). |
| C3 | Invisibility ability [401] vs spell [464] | enemy to-hit -1 vs -2 | Check the combat code before choosing. |
| C4 | Guardian Wind [450, 574] | "impervious to missiles" vs "almost totally immune" | Treat as missile immunity (ability list says so). |
| C5 | Naming | Cloud of Shadow (spell) vs Cloud of Darkness (effect); Holy Light vs Heavenly Light; Prime vs Arcanus plane | Pick one name each in our content; cosmetic. |

## D. Our own content that disagrees with both sources (already known)
- `rules.ron` `min_city_distance` was 3; fixed to 4 (see B17).
- The imported Dwarven/Klackon/Halfling production and food per citizen and Dwarf double
  tax are not modelled at all yet (see `docs/mechanics-gaps.md` §1).
- (Fixed) The Animist's Guild's +1 food is per farmer, as in the original; tested.

## Resolving
For each row, choose the code, the help, or a third behaviour, then record the decision in
this file (add a **Decision** column) and apply it in `scripts/import_mom.py` or the
engine. Rows marked **Check** should be verified against the Dasm listing or by testing
the original game before a decision is final.
