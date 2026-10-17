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
| B10 Undead | Undead also get cold, poison and illusion immunity (done, task 29: `undead.rs`; Holy Word's extra -5 for undead also follows the help, the original has it reversed). |
| B11 Evil Omens | Applied by one spell-cost function everywhere (spell book, summoning, counter magic, item spells, combat). |
| B13 Blur | Simplified (owner's decision): a flat -1 (10%) to the attacker's chance to hit, whatever the distance, none for an attacker immune to illusions. The original rolls a 10% miss for each point of damage (with a bug that caps the loss near half); that roll code is gone. |
| B14 Forester's Guild | Its food counts toward a city's maximum size. |
| B15 Swamp | Half a food (done: `terrain.ron` counts food in half-units, swamp is 1). |
| B16 Nature node | 4 food (ReMoM treats the main game's 5 as the odd one out). Done: a node square takes over its terrain and gives the food in `land_food` on the feature (sorcery 4, nature 4, chaos 0, in half-food units). |
| Square food | `Economy::square_food` follows `Square_Food_x2`: a river square gives 4 (two food) whatever it lies on, water beside land (the shore, eight neighbours) gives 1, open sea 0. A lake with a river running into it, and sea beside a river's end with at most three other land squares around it, give 4; a shore with four or more takes the plain 1 (the original's tile sets split that way). Gold from shore, river and river mouth (10, 20, 30) was already as in the original. |
| Square production | `Economy::square_production` follows `Square_Production_Bonus`: mountains 5, hills 3, forest 3, desert 3; a river square 0; a magic node its own (nature 3, chaos 5, sorcery 0, `land_production`); shared squares half. |
| Gaia's Blessing | Its last missing effect is built: forests and nature nodes give twice their production bonus. Food +50%, two rebels pacified, outpost growth +20 and the weekly repair were done before (help topic 29 says so). |
| B18 Events | Use the real turn number, so events run from turn one and after loading a save. |
| B19 Growth | The original suppresses shrinking at maximum size and at the largest size; here starvation and being over the limit still shrink (done, task 20). Surplus population carries over instead of being discarded. (Neutral cities growing at half rate is not a bug in the original, but we do not port it: neutrals get no bonus or penalty, design.md section 8.) |
| B20 Outposts | Done (task 21). Outpost growth includes wild game; the computer wizards' difficulty modifier is applied at the end of the growth chance (task 247; none in the symmetric `flat` game). |
| B21 Disenchant | Dispels only other wizards' enchantments, by the usual chance (`dispel.rs`, task 59); the code's always-succeeding roll and its test against the caster are not followed. |
| B27 Holy Word | The undead get an extra -5 to resist it, as the help says (the original has it reversed). |
| B28 Crusade | Raises heroes as well as normal units, as the original code does; the help says "normal units". Not for the undead or creatures. |
| B29 Invisibility [401] | The original takes 1 off the attacker's chance to hit (an attacker immune to illusions ignores it). Ours takes 1 for each square between attacker and unit (1 when adjacent), so a shot at a revealed invisible unit usually misses. In battle an invisible unit is unseen by the enemy, which cannot attack, shoot at or aim a spell at it (the original's click map and ranged cursor refuse it too), until it stands next to an enemy unit, attacks or casts a spell (ours: it then stays in view until its side's next turn); a side with a unit immune to illusions sees it; area spells reach it. A unit that can be seen is drawn as a ghost, as in the original. The player's own units are always targetable. On the world map an invisible unit shows only to a viewer with an illusion-immune unit in sight, and moving into its square asks to attack it as for any other unit. |

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
ends with a note `[Updated from game code: ...]`. 17 topic edits, applied by
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
| B8 Gaia's Blessing [29] | two rebels are pacified (task 24; the help did not say so) |

Not changed:
- **B8 Stream of Life / Gaia's Blessing** (task 24, done): the code reads the wrong table, which
  looks like a port error rather than the original's behaviour; the Dasm listing was not
  consulted. We follow the help: Stream of Life removes the city's unrest and heals its units.
  Gaia's Blessing also pacifies two rebels, as the original's code and the wiki have it (its
  help names no unrest effect).
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
| B8 | Stream of Life [751], Gaia's Blessing | city enchantments | unrest code reads the **owner's overland-enchantment table** at the same index numbers (`City_Rebel_Count`), where those slots mean Life Force and Crusade | Check (not marked OGBUG; may be a port error) | Stream of Life: no unrest, full healing (task 24); Gaia's: help plus 2 pacified (wiki) | Follow the help (done). |
| B9 | Heroism [473] | unit becomes elite | in combat, ignores Crusade, Warlord and xp when deciding the level (ReMoM note at the combat-init level code) | Check | one level for the map and battles (done, task 27) | Follow the help; document the exception if parity is wanted. |
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
| B22 | Evil Presence [183] | destroys the magic power of shrines, temples, parthenons and cathedrals; no effect if the owner can use death magic | `City_Rebel_Count` stops those four buildings from pacifying unless the owner has death books (`CITYCALC.c`); the mana code never looks at it; casting it on such a city is refused (`OverSpel.c`) | Verified | done (task 25): the mana is destroyed as the help says; the buildings still pacify | Follow the help (decided by the project owner; done). |
| B23 | Wall of Darkness [178] | no ranged attack from outside unless the attacker has illusions immunity | `Check_Attack_Ranged` only lets True Sight through (ReMoM `OGBUG`: ignores innate illusions immunity) | Verified | done (task 25): illusions immunity sees through | Follow the help (rule 1). |
| B24 | Spell Ward [72] | total immunity from spells of one realm including combat spells; bars fantastic creatures of that realm from entering | only overland spells aimed at the city or at units in it fizzle (`Apply_Automatic_Spell_Counters`); combat and movement never read the wards | Verified | done (task 25): all three follow the help (attackers' combat spells of the realm are refused; fantastic creatures of it, the owner's too, cannot enter) | Follow the help (done). |
| B25 | Flying Fortress [77] | only flyers may enter, attack or leave; the wizard's foot troops may use it overland; a walker that leaves in battle cannot re-enter | in battle the city area is shut to walkers outside and the ring round it to walkers inside (the gate too); the overland code never reads the spell (and a `Check_FlyingFortress_Bug` jumps past part of the restriction) | Check | done (task 25) in battle as the code does; no overland rule | Keep. |
| B26 | Chaos Rift [110] | five strength 8 lightning bolts on each unit in the city each turn, 5% chance a building is destroyed | the turn code for it is a stub (`CTY_ChaosRift`); only the outpost shrink chance (+10) exists | Check | done (task 25) as the help says | Follow the help (done). |
| B27 | Holy Word [spell text] | the undead have a much more difficult time resisting it | the code's extra penalty for the undead is applied the wrong way round (as a bonus to their resistance), so the undead resist better | Check | done (task 29): the undead get an extra -5, as the help says (`undead_penalty`, `battle/spells.rs`) | Follow the help. |
| B28 | Crusade [622] | raises the levels of "normal units" | `Calc_Unit_Level` and the battle code raise every non-fantastic unit that is not undead, heroes too | Check | heroes get the extra level (task 27, `unit_level`) | Follow the code; the help's wording is loose. |
| B29 | Invisibility ability [401] | the unit can only be hit by ranged attacks when adjacent to an enemy, and otherwise the attacker's to-hit is lowered by 1 | `BU_ProcessAttack` only subtracts 1 from the attacker's to-hit when the defender is invisible (ability or spell) and the attacker is not immune to illusions; nothing limits melee | Verified | the code's rule (task 37, `invisibility_penalty` in `battle/mod.rs`); melee is not restricted; help [401] corrected via `HELP_CORRECTIONS` | Follow the code. |
| B30 | Earthquake [26] | each building has a 10% chance of being destroyed; non-flying, corporeal creatures have a 15% chance of dying | `Apply_Earthquake` (`Spells129.c`) rolls one in four for each such unit and calls `Apply_Damage_To_City` with a building chance of 15. That function destroys a building when the roll *beats* the chance (`Random(100) > chance`), read literally 85 percent of them; we read the 15 as the chance of a building falling, as the siege code (`conquest.rs`) does | Check | done (task 56): 25% for units and 15% for buildings; help [26] rewritten via `HELP_CORRECTIONS` | Follow the code's numbers (the game wins); confirm the building comparison against the real game. |
| B31 | Drain Power [171] | drains 50 to 150 mana | `Apply_Drain_Power` (`Spells130.c`) adds ten rolls of a twenty-sided die: 10 to 200, about 105 on average, never more than the victim has | Verified | done (task 56) as the code does; help [171] rewritten via `HELP_CORRECTIONS` | Follow the code. |
| B32 | Great Unsummoning [75], Death Wish [199] | Great Unsummoning banishes every fantastic creature on both planes, the caster's own too, at a resistance penalty of 2; Death Wish slays every enemy normal unit that fails a resistance roll | both functions are empty stubs in ReMoM (`Cast_GreatUnsummoning`, `Cast_DeathWish` return false) | Check | done (task 56) as the help says: a resistance roll against sorcery at -2, and against death at 0 for Death Wish (the help gives no penalty, confirmed by the project owner); heroes are not "normal units" here | Follow the help (rule 2). |
| B33 | Subversion [177] | lowers every other wizard's relations to the target by 25 | `Apply_Subversion` (`Spells130.c`) subtracts 25, then overwrites the relation with the target's own feeling for that wizard, so only the end of the peace survives | Check | done (task 56): -25 for every other wizard that has met the target (not the human), and all peace ends, as the help says | Follow the help (rule 1: the overwrite looks like a bug). |
| B34 | Cause Fear, Cloak of Fear [164] | enemy units engaging a fearful unit in melee must overcome their fright, figure by figure, before they can attack or counter-attack; Death immunity negates it | `Apply_Fear_Attack` (`Combat.c`) rolls the fearful unit's *opponent's* figures, but two of its three call sites subtract the count from the attacker's own figures (marked OGBUG in ReMoM) and none passes a fearful defender, so a Cloak of Fear on a defender does nothing and one on an attacker costs the attacker figures | Verified (help read) | done (task 157 item 5): each figure of the unit in melee with a fearful unit rolls a death resistance check and the failures cannot strike, whether it is the attacker's swing or the counter-attack; a defender immune to death is not frightened | Follow the help (rule 1: the code's misdirected calls look like a bug). |
| B35 | Flee button [593] | each fleeing unit has a 50% chance of being destroyed, a hero 25% | The same rolls in `Combat.c`, plus two rules the help does not give: on Intro and Easy the player whose turn it is loses no unit by fleeing, and units under Black Sleep or Confusion, or held by a Web with hit points left, are always lost. No `OGBUG` marker | Verified (`Combat.c`, the flee section after a battle) | Code has both rules (task 250); the Web hit points are not modelled, so any Web counts | Keep the code's rules; the help text may be corrected with `HELP_CORRECTIONS` |
| B36 | Wind Walking [63] | the stack flies "with a speed of 3" | `Stack_Moves` uses the Wind Walker's own movement points for the whole stack, and nothing gives the spell speed 3 (only Flight sets 3 moves) | Check | **Follow the help** (task 103): the spell gives its unit at least 3 moves (`Modify Moves Min 3`) and a stack with a Wind Walker has the Wind Walker's moves left, every unit ending a step with what it has left. | The original's speed is the unit's own; the help text promises 3. |
| B37 | Wind Mastery [57] | doubles the speed of all ships and sky masters of the wizard, halves all others | `NEXTTURN.c`/`Combat.c` scale only ships (`Transport` > 0): 3/2 for the owner's Wind Mastery, 1/2 for each rival's (counted as a sum) | Verified | **Our own rule** (decided by the user): ships and fliers, overland and in battle (`unit_max_moves`); x1.5 for a holder, x0.5 for a wizard without it while anyone else holds it, nothing stacks; help [57] rewritten via `HELP_CORRECTIONS` | Neither the help nor the code. |
| B38 | Meteor Storm [113] | strength 4 magical fire attack on all creatures outside cities; the caster's cities are immune | `WIZ_MeteorStorm` attacks every unit not in a city, the caster's own too, and rolls 1% for each building of every other wizard's city | Verified (`Spells132.c`) | Follow both: they agree (the caster's units outside cities are struck, as the help says "all creatures") | Nothing to change. |
| B39 | Great Wasting [114], Armageddon | 3-6 squares corrupted each turn | `WIZ_GreatWasting` and `WIZ_Armageddon` use `Random(3)+3`, four to six; `CITYCALC.c` also makes the *other* wizards' cities unruly (-1, -2), written as `Pacify = -1` (marked as a bug) | Verified | Three to six squares for both (the help). Great Wasting is changed on purpose too: any land square, unrest and mana from corrupted squares, the keeper immune (`divergences.md` 17). Armageddon is changed on purpose (any square, cities, sea, unrest from lost building calm; `divergences.md` 17) | The caster's cities and everything within two squares are exempt, as in the code. |
| B40 | Call the Void [119] | every building 50% destroyed, each unit takes 10 damage, the area is corrupted | `Apply_Call_The_Void` also loses half the city's citizens (each beyond the first, one in two) and corrupts each square within two at one in two | Verified (`Spells129.c`) | Follow the help: no loss of citizens; the corruption as the code | The citizens are task 286 if wanted. |
| B41 | Nature's Wrath [40] | for every non-combat death or chaos spell cast, "the casting wizard's" cities suffer tremors | Struck is the wizard who casts the death or chaos spell, when another wizard keeps Nature's Wrath going (`Cast_Spell_Overland`, `Call_Forth_The_Force_Of_Nature`); the holder's own spells do nothing | Verified | Follow the code; the help's "casting wizard" is read as the caster of the death or chaos spell. 10% of non-flying corporeal units, 5% of buildings. | Nothing to change in the help. |
| B42 | Chaos Channels [93, 440] | demon-skin armor gives +2 defense | `CC_ARMOR` adds 3 | Verified (`COMBINIT.c`) | Follow the code (+3); help [93] and [440] corrected via `HELP_CORRECTIONS`. Wings are 2 moves of flight and the breath strength 2, as the help says. | The code's check that stops armour and breath on some units reads the wrong mutation (marked); the intended rule is used. |
| B43 | Stasis [68], [554] | the freeing check is at -5 (68) or -3 (554) | `Combat_Resistance_Check(.., -5, sbr_Sorcery)` in `NEXTTURN.c` | Verified | -5; help [554] corrected via `HELP_CORRECTIONS`. The original holds the unit for the turn after the cast whatever it rolls, then rolls each turn. | In a battle a unit in stasis is treated as asleep (the original has no code for it that I found). |

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
