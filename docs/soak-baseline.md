# Soak baseline (8 November 2026)

The AI soaks as they stand after task 428 (performance) and the landing and trireme fixes, kept to compare
against after AI improvements. Re-run the same commands on the new code and compare the tables below.

- **Code:** commit `002da717` (AI ports: a port that can only build triremes counts one as its ship).
- **Rules:** the shipped presets, `max_tile: 20` (stacks and battle sides stay at 9).
- **Games are deterministic** for a seed, player count and code. Only the time column varies (two games ran in parallel, `--jobs 2`; compare
  times only between runs made the same way).
- **Tool:** `f4x-tools soak` (`docs/scripts.md`, `docs/ai-design.md` 8.1). A wizard is written `p0[Realm/Trait] cities(outposts)/units/gold`.

```
cargo build --release -p f4x-tools
for n in 2 3 4 5; do
  target/release/f4x-tools soak --seeds 21..28 --turns 1000 --players $n --jobs 2 > soak-p$n.log
done
```

## Summary

| Players | Won (of 8) | By Conquest | By Spell of Mastery | No winner (seeds) | Slowest game |
|---|---|---|---|---|---|
| 2 | 5 | 5 | 0 | 23, 25, 27 | 205 s (seed 27) |
| 3 | 1 | 0 | 1 | 21, 22, 23, 24, 25, 27, 28 | 300 s (seed 23) |
| 4 | 5 | 1 | 4 | 22, 23, 26 | 336 s (seed 22) |
| 5 | 6 | 1 | 5 | 23, 25 | 341 s (seed 23) |
| **All** | **17 of 32** | 7 | 10 | 15 games | |

Games with no winner after 1000 turns are the main thing to improve: the 3-player games almost never finish.

## 2 wizards

| Seed | Result | Turn | Survivors (cities/units/gold) | Battles | Cities taken | Wars | Refused | Time |
|---|---|---|---|---|---|---|---|---|
| 21 | p1 Conquest | 240 | p1 9c/78u/137g | 163 | 13 | 7 | 1 | 7 s |
| 22 | p1 Conquest | 75 | p1 9c/40u/110g | 17 | 5 | 3 | 0 | 0.7 s |
| 23 | **no winner** | 1000 | p0 **1c/20u/3703g**, p1 11c/211u/202g | 450 | 26 | 7 | 2 | 113 s |
| 24 | p0 Conquest | 840 | p0 5c/58u/103g | 446 | 24 | 16 | 0 | 25 s |
| 25 | **no winner** | 1000 | p0 3c/38u/9g, p1 14c/476u/93g | 327 | 19 | 14 | 10 | 203 s |
| 26 | p0 Conquest | 177 | p0 6c/46u/99g | 69 | 5 | 4 | 0 | 2.5 s |
| 27 | **no winner** | 1000 | p0 10c/238u/199g, p1 11c/311u/87g | 376 | 13 | 19 | 1 | 205 s |
| 28 | p1 Conquest | 422 | p1 18c/455u/283g | 241 | 18 | 6 | 3 | 64 s |

## 3 wizards

| Seed | Result | Turn | Survivors (cities/units/gold) | Battles | Cities taken | Wars | Refused | Time |
|---|---|---|---|---|---|---|---|---|
| 21 | **no winner** | 1000 | p1 5c/28u/5g, p2 8c/221u/91g (p0 defeated) | 334 | 20 | 2 | 0 | 87 s |
| 22 | **no winner** | 1000 | p1 7c/161u/95g, p2 10c/154u/**121458g** (p0 defeated) | 291 | 25 | 4 | 0 | 140 s |
| 23 | **no winner** | 1000 | p0 11c/302u, p1 9c/278u, p2 3c/48u | 418 | 20 | 27 | 1 | 300 s |
| 24 | **no winner** | 1000 | p0 19c/430u/131g, p2 **1c/11u/40g** (p1 defeated) | 385 | 28 | 10 | 6 | 124 s |
| 25 | **no winner** | 1000 | p0 10c/270u, p1 6c/173u, p2 3c/42u | 283 | 6 | 17 | 3 | 135 s |
| 26 | p0 Spell of Mastery | 800 | p0 6c/165u/125g, p2 10c/206u/161g (p1 defeated) | 419 | 18 | 4 | 1 | 94 s |
| 27 | **no winner** | 1000 | p0 10c/250u, p1 10c/312u/**8731g**, p2 7c/98u | 252 | 3 | 32 | 1 | 201 s |
| 28 | **no winner** | 1000 | p0 8c/187u/1056g, p1 8c/182u, p2 11c/448u | 350 | 11 | 8 | 4 | 207 s |

## 4 wizards

| Seed | Result | Turn | Survivors (cities/units) | Battles | Cities taken | Wars | Refused | Time |
|---|---|---|---|---|---|---|---|---|
| 21 | p1 Spell of Mastery | 951 | p1 12c/336u, p2 6c/93u | 482 | 40 | 27 | 7 | 128 s |
| 22 | **no winner** | 1000 | p0 10c, p1 11c, p2 6c, p3 7c (nobody defeated) | 360 | 20 | 48 | 5 | 336 s |
| 23 | **no winner** | 1000 | p1 5c, p2 10c, p3 7c (p0 defeated) | 535 | 50 | 25 | 10 | 245 s |
| 24 | p1 Spell of Mastery | 679 | p1 5c/139u, p2 8c/276u, p3 10c/210u | 403 | 15 | 18 | **153** | 145 s |
| 25 | p2 Spell of Mastery | 894 | p0 2c, p2 10c/265u, p3 5c/120u | 575 | 35 | 22 | 64 | 101 s |
| 26 | **no winner** | 1000 | p1 5c/37u, p3 11c/248u (p0, p2 defeated) | 441 | 20 | 20 | 2 | 111 s |
| 27 | p2 Conquest | 510 | p2 15c/252u | 528 | 34 | 11 | 3 | 49 s |
| 28 | p1 Spell of Mastery | 764 | p0 6c, p1 11c/298u, p2 2c(1 outpost), p3 8c | 256 | 16 | 21 | 4 | 136 s |

## 5 wizards

| Seed | Result | Turn | Survivors (cities/units) | Battles | Cities taken | Wars | Refused | Time |
|---|---|---|---|---|---|---|---|---|
| 21 | p4 Spell of Mastery | 708 | p1 6c, p3 6c, p4 9c/187u | 608 | 22 | 25 | 3 | 97 s |
| 22 | p2 Spell of Mastery | 930 | all five alive, 3c to 8c | 376 | 38 | 27 | **91** | 283 s |
| 23 | **no winner** | 1000 | p1 4c, p3 **20c/496u**, p4 9c (p0, p2 defeated) | 1105 | 38 | 29 | 1 | 341 s |
| 24 | p2 Spell of Mastery | 950 | p0 6c, p2 6c, p4 4c (p1, p3 defeated) | 716 | 27 | 22 | 1 | 152 s |
| 25 | **no winner** | 1000 | p0 10c, p2 11c/320u/**26672g**, p3 6c, p4 7c (p1 defeated) | 505 | 23 | 54 | 5 | 254 s |
| 26 | p0 Conquest | 826 | p0 10c/162u | 561 | 32 | 16 | 2 | 62 s |
| 27 | p2 Spell of Mastery | 778 | p2 19c/433u, p3 2c, p4 5c | 351 | 28 | 11 | 2 | 142 s |
| 28 | p1 Spell of Mastery | 751 | p1 10c, p2 10c, p3 13c | 673 | 60 | 58 | 7 | 174 s |

## Things to look at in these results

These are observations, not yet investigated unless noted.

- **3-player games stall.** In seeds 21, 22 and 24 one wizard is defeated and the other two cannot finish each other; seeds 23, 25, 27
  and 28 have nobody defeated at turn 1000. A winner that holds more than half the cities still fails to close the game (2-player seeds 23
  and 25 too).
- **Hoarded gold.** 3p seed 22 p2 holds 121458 gold (10 cities), 5p seed 25 p2 26672, 3p seed 27 p1 8731, 2p seed 23 p0 3703 with one city.
  The AI's `spend_gold` (RESERVE 40) is not spending it: nothing is left to buy, or the buying is blocked.
- **Refused orders** (an order the AI sent that the game refused) were mostly fixed by the landing fix (3p seed 25 had 969). Still high:
  4p seed 24 (153), 5p seed 22 (91), 4p seed 25 (64).
- **A small player kept alive** with one city (3p seed 24 p2, 2p seed 23 p0) never recovers or is never finished.
- **Spell of Mastery wins** are 10 of 17 wins and come late (turns 679 to 951); in the 2-player games the Conquest wins come earlier.
- Task 499 (spell research priorities) and tasks 474, 435, 495 and 488 should change the Spell of Mastery turns, the gold hoards and the
  stalled 3-player games; compare these tables after each.

## Older numbers (before the landing and trireme fixes)

3 wizards, seeds 21 to 26, 1000 turns: seed 25 had 969 refused orders and took 161 s (now 3 and 135 s); seed 24 p2 had 1 city, 13 triremes and
1625 gold (now it keeps one trireme and spends its gold). The other 3-player seeds had no winner either, except seed 26 (Spell of Mastery,
turn 712, now 800).

## Casting change versus the previous AI (8 November 2026)

`soak-versus`, 4 wizards, seeds 21 to 26, 700 turns, reference = commit 954e0da1 (no saving for summons, net mana margin 2), new = saving for
summons the pool cannot pay yet plus a net mana income of about 12 a turn from the secure phase on (`wizardry.rs`, `SAVING_TURNS`, knob
`NETMANA`). Mixed games, per wizard (reference / new): spells cast 60.8 / 73.4, heroes joined 3.1 / 5.2, items made 0.6 / 4.8, cities captured 5.3 /
6.5, cities lost 6.5 / 5.0, units at the end 113 / 150, score 491 / 519. Nobody won a mixed game in 700 turns. A plain 4-wizard soak, seeds 21 to
28, 1000 turns: 5 of 8 won (4 by Spell of Mastery, at turns 541 to 896).

## After the gold, casting, fleet, expansion, wide/tall and planar changes (8 November 2026)

Same command (`max_tile: 20`, seeds 21..28, 1000 turns, `--jobs 6`), code at the commit "wide/tall slider, no cap on cities ... planar crossing".

| Players | Won (of 8) | By Conquest | By Spell of Mastery | Turns of the wins | Baseline won |
|---|---|---|---|---|---|
| 2 | 7 | 4 | 3 | 87 to 823 | 5 |
| 3 | 6 | 0 | 6 | 513 to 958 | 1 |
| 4 | 7 | 1 | 6 | 215 to 873 | 5 |
| 5 | 8 | 0 | 8 | 486 to 933 | 6 |
| **All** | **28 of 32** | 5 | 23 | | **17 of 32** |

Not won: 2p seed 25; 3p seeds 21 and 25; 4p seed 21. Conquest wins nearly vanished with more players: the games end by the Spell of Mastery
(23 of 28). Larger armies (units per wizard 120 to 250), 10 to 13 cities per wizard and gold hoards under 1700 in `soak-versus`
(reference: 5 to 6 cities, 6000 gold). Still to do: conquest in games with three or more wizards, the planar war has no soak evidence yet
(no mention of crossings in the reports), and the sea and plane armies need counting in the soak reports.

