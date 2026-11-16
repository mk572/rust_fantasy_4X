# Soak results, 16 November 2026

Code: commit `e6e2cb32` plus the wounded-unit fix of the commit after it (the series itself ran on `e2217217` with the soak statistics; the kill attribution (`defeats:` line, `wizards eliminated`) was added while it ran and is **not** in these logs). Compared with `docs/soak-baseline.md` (8 November 2026, commit `002da717`). The raw logs and the full report (every counter, all wizards) are in `soak-out/s1/` (`report.md`; made with `scripts/soak_report.py`); soaks are deterministic per seed and code, but chaotic: 8 games differ by about two wins from noise alone.

## A. The baseline's settings (seeds 21..28, 1000 turns, Spell of Mastery on)

| Players | Won, baseline | Won, now | Conquest, baseline | Conquest, now | Mastery, baseline | Mastery, now | Mean turn of a win, now |
|---|---|---|---|---|---|---|---|
| 2 | 5 of 8 | 8 of 8 | 5 | 6 | 0 | 2 | 395 |
| 3 | 1 of 8 | 8 of 8 | 0 | 1 | 1 | 7 | 744 |
| 4 | 5 of 8 | 8 of 8 | 1 | 0 | 4 | 8 | 610 |
| 5 | 6 of 8 | 8 of 8 | 1 | 1 | 5 | 7 | 640 |
| **All** | **17 of 32** | **32 of 32** | 7 | 8 | 10 | 24 | |

## B. Conquest only (the same seeds, `--mastery off`)

| Players | Won, before the AI work | Won, now | Mean turn of a win, now |
|---|---|---|---|
| 2 | 6 of 8 | 7 of 8 | 347 |
| 3 | 4 of 8 (3 of 8 midway through the AI work) | 2 of 8 | 675 |
| 4 | 1 of 8 (2 of 8 midway) | 1 of 8 | 766 |
| 5 | not run | 2 of 8 | 592 |

The conquest result has **not improved** beyond noise at 3 to 5 wizards (and one game fewer than before at 3 wizards): most games stall with one big wizard and one or two others.

## C. 32 new games, four wizards, seeds 101..132, Mastery on

30 of 32 ended with a winner (5 by Conquest, 25 by the Spell of Mastery), mean turn of a win 682.

## Health of the economy

| Run | Largest treasury at the end | Wizards above 10,000 gold |
|---|---|---|
| Baseline (8 Nov), 3 wizards seed 22 | 121,458 | |
| Baseline (8 Nov), 5 wizards seed 25 | 26,672 | |
| A, B and C, 9 runs, 352 wizards | 3,150 (one outlier: 32,537, B with 5 wizards seed 25) | 1 |

The gold hoards of task 540 are gone. Refused orders per run of 8 games rose from a few hundred at most (the baseline's worst game had 153) to 1,300 to 8,100 (set C: 16,600 in 32 games). Most of it was the new wounded-unit code sending units to a full city every turn (`StackFull`, 60 times for one unit); that is fixed (a unit goes only where there is room and is left alone while it walks). What is left is mostly raid parties that find no site to march to ("Unreachable"), noted by `site.rs`; to be looked at.

## Races and realms (set C only: 128 wizards from 32 independent games)

Sets A and B use the same eight seeds, so the same wizards come back in every run: counting them together counts 28 "Sage Masters" that are really a handful of wizards. Only set C is an independent sample, and 128 wizards are still few per race (4 to 15), so read these as hints. A win is the game's winner (25 of 30 won by the Spell of Mastery).

| Race | Wizards | Won | Alive at end | Cities | Units | Cities captured | Battles won/lost | Spells cast |
|---|---|---|---|---|---|---|---|---|
| halfling | 15 | 2 | 73% | 13.6 | 155 | 17.3 | 247/192 | 136 |
| lizardman | 12 | 4 | 75% | 18.0 | 268 | 30.3 | 394/389 | 114 |
| high_elf | 12 | 2 | 33% | 5.1 | 50 | 11.0 | 149/116 | 50 |
| orc | 12 | 1 | 33% | **1.2** | **9** | 4.4 | 63/122 | 66 |
| gnoll | 11 | 0 | 36% | 3.5 | 34 | 6.2 | 93/144 | 46 |
| barbarian | 10 | 2 | 80% | 11.3 | 156 | 16.4 | 207/176 | 116 |
| draconian | 9 | 4 | 67% | 16.6 | 205 | 17.1 | 238/195 | 68 |
| high_men | 8 | 2 | 38% | 6.6 | 64 | 9.5 | 60/110 | 102 |
| klackon | 8 | 1 | 62% | 11.6 | 143 | 14.5 | 196/252 | 82 |
| nomad | 8 | 1 | 50% | 10.1 | 128 | 13.8 | 165/184 | 84 |
| dark_elf | 7 | 1 | 43% | 5.1 | 39 | 6.0 | 106/140 | 125 |
| dwarven | 6 | 3 | 100% | 17.7 | 240 | 12.7 | 131/232 | 235 |
| troll | 6 | 4 | 83% | 14.0 | 153 | 13.2 | 122/211 | 134 |
| beastmen | 4 | 3 | 100% | 12.5 | 145 | 13.2 | 211/347 | 290 |

| Strongest realm | Wizards | Won | Alive at end | Cities | Units | Cities captured | Spells cast |
|---|---|---|---|---|---|---|---|
| chaos | 41 | 6 | 56% | 9.7 | 118 | 14.4 | 94 |
| sorcery | 31 | 11 | 58% | 10.4 | 130 | 12.4 | 99 |
| life | 29 | 4 | 55% | 7.6 | 92 | 10.1 | 96 |
| nature | 28 | 4 | 68% | 9.8 | 106 | 13.0 | 102 |
| death | 26 | 11 | 62% | 15.5 | 204 | 19.4 | 135 |

Hints: Death and Sorcery wizards win about 35 to 42% against 14 to 15% for Life, Nature and Chaos (the wins are mostly by the Spell of Mastery, so research and mana decide). Dwarves, trolls, beastmen, draconians and lizardmen do well; **orcs end with 1.2 cities and 9 units on average after 1000 turns and gnolls never won**, which may be a fault in how the AI plays those races rather than a rules fact (not investigated); high elves, high men and dark elves are also weak. Retorts (all 9 runs, same caveat): Alchemy, Warlord and Myrran wizards do best; Sage Master wizards never win (28 of them, a handful of wizards).

## What the series says

1. The Spell of Mastery decides almost every game: with it on, every game ends (32 of 32, baseline 17), 24 of those by Mastery. With it off the conquest result is **unchanged**: 3 to 5 wizards end with a winner in 1 to 2 games of 8.
2. Stalled Mastery-off games have three shapes: (a) a wizard with 25 to 38 cities and a rival with 1 to 3 cities and 2 to 4 units that it does not finish, either at war and seeing the city (seeds 24 and 28, 5 wizards) or never having found it (seed 23, 5 wizards: "sees 0 of 2 cities"); (b) two wizards with 17 to 22 cities each (3 wizards, seeds 22, 23); (c) a leader against several mid wizards. Shape (a) is the most direct target: the straggler's city is probably across water or behind a plane (landing and exploration), and the siege party does not reach it.
3. The kill attribution (who eliminated whom, and whether neutral cities did it) is in the soak from now on; this series does not have it.

## Next

Find out why (a) happens (a snapshot of seed 24, 5 wizards at turn 600; `reach:` shows the leader sees the city), explore the last rival's plane, and look at the orc and gnoll AI. Re-run the series with the kill attribution on.
