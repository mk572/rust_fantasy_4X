# Overnight soak, 17 November 2026

`scripts/soak_overnight.sh` ran the nine variety settings of `scripts/soak_variety.sh` in seven full rounds and part of an eighth (stopped at 07:02, 69 of 72 runs), each round on seeds 1000 higher, through `cool_run.sh` with the release binary built at 02:03 (before the client work of the same day, which does not touch `f4x-core` or `f4x-ai`). Raw output: `soak-out/overnight-20261117/` (local, git-ignored).

**Size:** 216 games, 677 wizard-games. **0 panics** (`panics.txt` is empty). 98 wizards defeated. A game took 172 s on average (longest 588 s).

The soak prints counters per wizard and per game; what it does not print is marked "not measured" below. "Known" counts wizards that knew a spell at the end of a game (out of 677), "cast" counts casts.

## Games

| Setting | Games | Won by conquest | Won by the Spell of Mastery | No winner | Mean turn of a win | Refused orders a game |
|---|---|---|---|---|---|---|
| small-2p (500 turns) | 32 | 5 | 0 | 27 | 304 | 563 |
| easy-2p | 21 | 8 | 0 | 13 | 282 | 590 |
| long-2p | 14 | 4 | 8 | 2 | 742 | 3,587 |
| islands-3p | 32 | 0 | 1 | 31 | 495 | 1,496 |
| weak-3p | 24 | 1 | 5 | 18 | 532 | 4,517 |
| hard-3p | 21 | 0 | 20 | 1 | 444 | 6,538 |
| magic-4p | 24 | 0 | 7 | 17 | 536 | 3,569 |
| huge-4p | 24 | 0 | 18 | 6 | 592 | 4,080 |
| crowded-5p | 24 | 0 | 11 | 13 | 533 | 3,341 |
| **All** | **216** | **18** | **70** | **128** | | **2,954** |

Of the 149 games with three or more wizards, 1 ended by conquest and 62 by the Spell of Mastery.

## Per task

Status words: **settled** (could be closed), **partly** (some of the task is answered), **open** (the soak says nothing new or the counter is missing).

| Task | What the soak shows | Status |
|---|---|---|
| 367 overland spells never cast | 145 spells were known by some wizard and never cast on the map (87 of them never cast in battle either, out of 214 known). The most known: `summoning_circle` 671, `disenchant_area` 631, `disjunction` 492, `nagas` 292, `floating_island` 263, `sprites` 250, `ghouls` 201, `dark_rituals` 181. The list is in "Spells known and never cast". | open (tasks 641 and 642 carry the work) |
| 434 Transmute | `transmute`: known 124, cast 0. The AI does not cast it, so the task is not done. `TASKS.md` line 146 calls 434 done (status 31 October 2026); that contradicts this. | open, see question 4 |
| 435 planar war | Plane crossings 21,753; cities captured on the other plane 266; towers cleared 794; sites cleared 14,751. Plane Shift cast 37 times (known 136). The army does cross and takes cities, so the first half of the task works. Left as the task says: the garrison at the tower, the defence against a crossing rival, war declared before the crossing. Not measured: how often a rival's army crossed in. | partly |
| 436 gathering far from the target | Marches launched 34,341, soldiers sent 564,935, **called off 701 (2.0 %)**, dropped for another target 3,347 (9.7 %). The counter does not say why a march was called off, nor how many units arrived one by one. | partly (the 2 % is a small share; a counter for stragglers is missing) |
| 437 stack that suits the target | The per-march strength is not counted. Income-first buildings are not done (task text). | open |
| 499 research | Spells learned 22,134 (32.7 a wizard-game). The Spell of Mastery was cast in 70 games (known by 94 wizards). Plane Shift known by 136 wizards (20 %), Planar Travel by 111. Of 214 spells known, 127 were ever cast (map or battle). Not measured: spells researched and never cast by that same wizard (the soak only sums over all wizards). | partly |
| 502 game phases | Mana spent per wizard-game: mean 10,651, median 5,706 (the November 8 numbers were 300 to 5,600 over 600 turns); mana left at the end: mean 881, median 36, so wizards spend what they have. Spells cast 85,523 (126 a wizard-game; the early soak had 185 casts for three wizards, about 62 each). The Spell of Mastery won 70 games. 128 of 216 games ended with no winner (59 %; islands-3p 31 of 32, small-2p 27 of 32). The phases themselves are not logged, so a game's phase against its turn cannot be read. | open, see question 7 |
| 505 leave a lost fight with Plane Shift | `plane_shift` cast 37 times, known by 136. The soak does not say why it was cast, so a rescue cast cannot be told from the army's crossing. | open (a counter for the reason is needed) |
| 506 gold, march thresholds, conquest with 3+ wizards | **Gold:** mean 465 at the end, median 175, 3 wizards of 677 with 6,000 or more (the task found 6,000 to 8,000 for most): settled. **Marches:** battles won 81,989, lost 92,655 (47 % won). **Conquest with three or more wizards:** 1 conquest win in 149 games, 62 Mastery wins: not settled. | partly (gold part settled) |
| 509 spellcasting helper | Done and archived; for the record: 69 different map spells were cast in the 216 games, 79 in battle. | done |
| 515 plans that never succeed | Plans dropped by the 100-turn clock: **settler 155,458 (230 a wizard-game)**, campaign 607, landing 426, scout 321, raid 124, colony 104, planar 20. The settler count is not a plan that stood 100 turns once in a while: it is a bug or a metric mistake (the task says a plan that always times out is a bug to find). Part (4), "turns a plan stood without progress", is not in the soak. | open, see question 1 |
| 518 army size | Units per city at the end: mean 15.7 (max 58), over wizards with cities. Idle units are not counted (the counter "ships and other units with no role" is a sum over turns, 2,073,629). | open |
| 521 research stall | Wizards with more than 100 stalled turns: **82 of 677 (12 %)**; with any: 319 (47 %); mean 38.8 turns, longest 498. In the November 11 soak 8 of 24 had more than 100 (33 %). Better, not gone. | partly |
| 541 refused moves | `site.rs:196` `Move(NoMovesLeft)` 334 (about 1.5 a game; the task found 38 in 8 games); `expand.rs` Impassable 3 in all. Nothing is logged at `military.rs:627` or `:487` any more, but the line numbers have moved and `military.rs:513` shows `Move(Impassable)` 2,225 times, so the same kind of refusal is still there. | partly |
| 542 neutral goals | Neutral players: cities founded 3,463, lost 2,866, taken by wizards 2,229; battles won 28,285, lost 32,129; units lost 21,806. No goals exist yet. | open |
| 543 build settings | The soak does not time builds. A game took 32 s (small-2p) to 415 s (huge-4p) on the 02:03 release build; these are only a baseline for the comparison the task asks for. | open |
| 545 orders on a full square | **637,987 refused orders in 216 games** (577,321 by the wizards' own counter; the logs list 566,673; the gap was not checked). By source: `road.rs:158` StackFull 261,553 (41 %), `military.rs:142` BattleStart(BadSides) 62,581, `pickup.rs:294` StackFull 50,876 and Impassable 32,381, `road.rs:178` Road(CannotBuildHere) 43,762, `pickup.rs:483` Impassable 20,074, `pickup.rs:495` Impassable 12,898, `escort.rs:323` StackFull 12,898, `site.rs:250` Unreachable 11,882, `escort.rs:61` Impassable 9,374, `pickup.rs:466` StackFull 9,370. `colony.rs` is down to 321, `naval.rs` to 771, `assault.rs` to 2,842. The old sources are mostly gone; new ones are far larger. | open, see questions 2 and 3 |
| 561 stalled wars | Not measured: there is no counter for a stalled war. Diplomacy over all games: wars declared 8,348, peace offers 14,317, proposals accepted 5,485, refused or lapsed 9,703, war-weary peace offers 5,707, treaties broken 324, coalition requests 6,596. | open |
| 568 scouts and sea scouting | Scout plans dropped by the clock 321 in 677 wizard-games (0.47 each). Sea scouting is not counted separately. Arcanus explored at the end: 65 % on average, Myrror 45 %. The per-turn scout log of task 567 is not in the soak. | open |
| 598 trades | Done and archived; for the record: trades offered 494, done 228 (1.06 a game; 0.02 before), with gold 196, of items, units or heroes 175, spell gifts 463. | done |
| 640 battle casting | Battle casts 142,688 of 79 different spells, mana 3,378,845 (wizards spent 7,210,875 in all, assuming that includes the battle mana; not checked). By casts: `web` 19,536, `fireball` 13,530, `fire_bolt` 11,539, `resist_magic` 11,131, `resist_elements` 9,485. By mana: `fire_bolt` 469,091, `psionic_blast` 323,061, `web` 317,964, `lightning_bolt` 272,743, `terror` 208,524. These are the baseline for the tuning the task asks for; no constant has been changed, so nothing is compared. | open (baseline done) |
| 641 intents | The cast counts of the intents (casts / known): `wall_of_fire` 755 / 221, `wall_of_stone` 1,497 / 218, `flame_strike` 0 on the map (787 in battle) / 71, `corruption` 20 / 216, `warp_node` 4 / 44, `earthquake` 7 / 62, `raise_volcano` 1 / 113, `call_the_void` 1 / 31, `gaias_blessing` 44 / 53, `inspirations` 3 / 34, `famine` 0 / 39, `pestilence` 0 / 19, `chaos_rift` 0 / 61, `astral_gate` 0 / 25, `spell_ward` 0 / 40, `time_stop` 0 / 27. Sea crossing: `water_walking` 273 / 225, `flight` 260 / 122, `path_finding` 55 / 133, `wind_walking` 0 / 74, `floating_island` 0 / 263. Corruption, Earthquake, Volcano and Warp Node are cast far less often than they are known. | open (thresholds to tune) |

## The 15 spells of task 642

| # | Spell | Known | Cast on the map | Cast in battle | Note |
|---|---|---|---|---|---|
| 15 | `change_terrain` | 119 | 0 | 0 | never cast |
| 18 | `transmute` | 124 | 0 | 0 | never cast; the same spell as task 434 |
| 55 | `enchant_road` | 142 | 0 | 0 | never cast |
| 43 | `floating_island` | 263 | 0 | 0 | never cast (needs the naval planner) |
| 37 | `earth_gate` | 29 | 0 | 0 | never cast |
| 28 | `move_fortress` | 57 | 0 | 0 | never cast |
| 203 | `summoning_circle` | 671 | 0 | 0 | never cast; known by 99 % of the wizards, the most of any |
| 54 | `spell_lock` | 144 | 0 | 3 | cast three times in battle only |
| 173 | `lycanthropy` | 97 | 0 | 0 | never cast |
| 163 | `dark_rituals` | 181 | 0 | 0 | never cast |
| 135 | `planar_seal` | 98 | 0 | 0 | never cast |
| 133 | `resurrection` | 113 | 262 | 0 | cast elsewhere (`resurrect`) |
| 207 | `enchant_item` | 575 | 160 | 0 | cast elsewhere (`craft.rs`) |
| 211 | `create_artifact` | 437 | 759 | 0 | cast elsewhere (`craft.rs`) |
| 214 | `spell_of_return` | 677 | 17 | 0 | begun by the game when a wizard is banished |

All eleven "never cast" spells stayed at zero, as `LEFT_ALONE` says. The four others are cast. The soak does not log the mana of a map cast, so the mana use per map spell is not known; only battle mana is counted by spell (task 640).

## Spells known and never cast

Known by at least 100 wizards and never cast on the map or in battle: `summoning_circle` 671, `disenchant_area` 631, `disjunction` 492, `nagas` 292, `floating_island` 263, `sprites` 250, `ghouls` 201, `dark_rituals` 181, `disenchant_true` 163, `giant_spiders` 155, `mana_leak` 149, `fire_giant` 147, `doom_bat` 147, `enchant_road` 142, `cockatrices` 137, `gargoyles` 127, `basilisk` 124, `transmute` 124, `unicorns` 122, `change_terrain` 119, `chimeras` 117, `planar_travel` 111, `raise_dead` 106. Several are summons (`nagas`, `sprites`, `ghouls`, `giant_spiders`, `cockatrices`, `gargoyles`, `basilisk`, `unicorns`, `chimeras`, `fire_giant`, `doom_bat`): the AI's summoning only seems to pick a few of them. That and the dispels (`disenchant_area` 631, `disjunction` 492, `disenchant_true` 163) are the largest groups.

## Tasks closed

None. No task is settled by this data: the closest, 436 and 506, each have a part the soak cannot see. Tasks 509 and 598 were already done. `archive_done_tasks.py` was not run.

## Questions for you

1. **Settler plans dropped 155,458 times** (230 a wizard-game, in every setting). Is that the clock firing on a settler plan again each turn (a bug of the plan or of the counter) or real? Task 515 says such a plan is a bug to find. Should I look into it first? It makes the plan counter useless for the other kinds.
2. **`military.rs:142` BattleStart(BadSides): 62,581 refused orders.** The AI starts a battle between sides the game says are not enemies. I think this is a real bug (a war ended or an alliance formed after the plan). New task, or part of 545?
3. **`road.rs:158` StackFull: 261,553 orders (41 % of all).** The road builders go to a full square, every turn. Same for `pickup.rs`, `escort.rs`. Task 545 names `colony.rs`, `military.rs` and `assault.rs`, which are small now; should 545 be rewritten around these files?
4. **Task 434 and Transmute.** Line 146 of `TASKS.md` calls it done, the table keeps it open, and the soak shows 0 casts (known 124). Is 434 a duplicate of the Transmute entry in 642, to be closed into it?
5. **541:** `expand.rs` is nearly gone, `site.rs:196` is small (334), but `military.rs:513` still sends 2,225 `Move(Impassable)` orders. Close 541 and put that line into 545?
6. **Conquest with three or more wizards** (506 part 3): 1 of 149 games. Is that what you want (the Spell of Mastery as the usual win), or is conquest meant to happen?
7. **No winner in 59 % of the games**, 31 of 32 in islands-3p: should those settings run longer, or is "no winner" the result the AI should avoid? It decides how 502 is judged.
8. **Missing counters:** the reason for a Plane Shift cast (505), why a march was called off (436), idle units (518), turns a plan stood without progress (515), stalled wars (561), the mana of a map cast (all the mana questions). Add them before the next overnight soak?
