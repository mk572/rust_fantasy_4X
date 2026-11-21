# Diplomacy system B (design, 21 November 2026)

The AI's statecraft today (`statecraft.rs`, `diplomacy.rs`; `diplomacy.md` section 8) is **system A**. It
decides peace in three places with three formulas (a losing war in `statecraft::wars`, a roll of peace patience in
`diplomacy::negotiate`, the original's formula towards the human), refuses a peace by a fourth (`peace_ok`) and
chooses its friends and enemies by separate parts that do not know each other (`dominant_war`, `appease`,
`coalition`, the hate rolls). **System B** is a new design, built beside A, so that a soak can say which one plays
better: each wizard has one of the two (task 761), the rest of the AI is the same.

## Three values for each other player (`Regard`)

* **Friendship** (how likeable): the core's `relation` and `hidden` of the pair, plus `statecraft::affinity` (fame,
  a Charismatic retort, the same race, shared books). Makes the wizard overlook faults and trade, make pacts,
  ally and accept peace. No second number: whatever moves the relation already moves it.
* **Fear** (how threatening): how near and how strong the other is (cities, army score, distance), and how
  ruthless: a slowly fading memory of what the wizard saw it do (cities razed, wars declared, treaties broken,
  the nasty spells), from events a human could see too. Fear does not push one way: it **makes the reaction
  extreme**, and the strength ratio picks the direction. Against someone who outclasses it, the wizard grovels
  (asks for peace early, gifts, accepts hard terms); against an equal or a weaker one it strikes first.
* **Strategic** (power politics): the weak and reachable neighbour is prey; the weaker wizards ally against the
  leader; a shared enemy makes a friend; a runner for the Spell of Mastery is stopped.

## War weariness (one number for each war)

Kept in the wizard's own diplomatic memory, fed by the events a human would be told of (task 760): **up** for a
battle lost (more for a big one) and for a city lost (a lot); **down** for a battle won and a city taken (a lot);
**up a little every turn without a clash** (a stalled war wears down); never below 0. The same memory holds the
ruthlessness the wizard has seen (cities razed, wars declared, treaties broken) with the turn it saw it, fading
with time.

## Where the memory lives

Not in the core (it holds facts, never a decision, and has no room for a wizard's grudges), and not in a throwaway
`Ai`: a proposal is answered by `diplomacy::rate(game, ..)`, which `answer_proposals` and `settle` call with a new
empty `Ai`. A shared `Diplomats` table (task 760), made once for each game by the host and handed to every `Ai`
and to those two functions, holds each wizard's memory (and its system, A or B), so the judge answers with what it
remembers. The memory is saved with the AI's plans. The master tells every seat the events of the turns
(`Controller::notice`), the same ones a human's client is shown, so a wizard hears of a battle it is dragged into on
another wizard's turn.

## One peace score

`peace_wish(me, other)`: war weariness, plus friendship (willing to overlook), plus fear (the weaker side begs,
the stronger finishes it), plus strategic (prey is a minus; a shared enemy elsewhere, a second front or a
threat to its own cities a plus), with the personality's modifier. **It offers peace at `OFFER` and accepts at
`ACCEPT`** (a little lower), from the same function, for the wizards among themselves and for the human's offers.
A cool-down keeps it from asking every turn; the dice stay only as pacing, never as the decision. Replaces
`wars`, the peace roll of `negotiate`, `peace_ok` and the peace formula of `approach_human` for a wizard of B.

## What is built (task 762: the peace)

`crates/f4x-ai/src/diplo_b.rs`, for a wizard whose memory says system B (`Ai::system`); every number is a knob,
`AIK_DIPLO_B_<NAME>`, so a soak can vary it. The first guesses:

```text
peace_wish = weariness
           + gap       (their power - ours) * GAP / max(theirs, ours)         GAP 50: losing is a plus, far ahead a minus
           + fear      FEAR_PCT % of fear if they are the stronger, minus half of it if we are  (FEAR_PCT 40)
           + friendship  (relation + hidden relation) * FRIEND_PCT / 100      FRIEND_PCT 10
           + strategic   -PREY (30) for a prey (150 % weaker, within 30 squares), +TWO_FRONTS (25) for the more
                         dangerous of two fronts
           + mood / MOOD_DIV (3)                                              the personality's modifier
           - YOUNG (40) while the war is younger than YOUNG_AGE (10) turns
```

* **Weariness** = what the events moved it by (`BATTLE_LOST` 5, `BATTLE_WON` -4, `CITY_LOST` 25, `CITY_TAKEN` -25, never
  below 0, and starting again when a new war begins) + `DRIFT_PER_TEN` (7) for every ten turns since the last clash
  or the start of the war: a war with nothing in it wears down at 0.7 a turn.
* **Fear** = nearness (100 minus `NEAR_PER_SQUARE` (5) for each square between the nearest cities, from 0 to 100) times
  their might (their power as a percent of ours, less 50, from 0 to 100), divided by 100. Power is the army plus a
  city's worth (`statecraft` `CITY_POWER`) for each city, as the observer sees it. Ruthlessness is added to it
  (times the nearness): the memory notes a razed city (`RUTH_RAZE` 15), a war declared (`RUTH_WAR` 6) and a treaty
  broken (`RUTH_BROKEN` 10) by a wizard it has met, fading by half every `RUTH_HALF` (150) turns, at most `RUTH_CAP`
  (60). The strategic term also has the Mastery caster (`MASTERY` -60) and the leader (`LEADER` -15 against the
  leader, +15 for the others while at war with it).
* **Offer** at `OFFER` (80) with a cool-down of `COOL` (15) turns for each wizard, kept in the memory; **accept** at
  `ACCEPT` (60). The dice are gone from the decision.
* **Where it plugs in:** `statecraft::run` calls `diplo_b::wars` in place of `statecraft::wars`;
  `diplomacy::rate` answers a peace by `diplo_b::accepts` from the judge's memory (in place of `peace_ok`);
  `diplomacy::negotiate` makes no peace roll for B; `approach_human` offers the human peace by `offer_due`. The
  dominant war, appeasing and the coalition of A are replaced by part 2 below; the hate rolls, the warnings and
  the human-directed parts are still A's.
* **What a B wizard asks of an A wizard** is answered by A's rules, and the other way round: the two systems meet in
  one game through the same proposals.

By arithmetic (not measured), a stalled war between equals with nothing in it reaches the accept mark after about
a hundred quiet turns and the offer mark after about a hundred and twenty-five, moved by 10 or so either way by the
friendship and the temper; a lost war reaches both much sooner. These are guesses to be read against a soak
(task 761), not tuned values.

## What is built (task 763: friends and enemies)

`diplo_b_stance.rs`, run from `statecraft::run` for B in place of A's appeasing, coalition and dominant war (and
`diplomacy::quarrel` is not called for B; `approach_human` does not ask for pacts for B). Every `REVIEW` (5)
turns each other wizard gets `Values { enemy, friend }`:

* **enemy** = prey (weaker and reachable, up to `PREY_VALUE` 60) + `HOSTILE_PCT` of the ill will + `LEADER_ENEMY` for
  the leader + `MASTERY_ENEMY` for the Mastery caster + `AT_WAR` for the one already fought + half the fear if the
  other is not stronger (strike first); less the share by which it is stronger once past `TOO_STRONG_PCT`, less 40
  for a pact and 80 for an alliance; unreachable wizards stay below `ENEMY_MIN`.
* **friend** = `STANCE_FRIEND_PCT` of the friendship + `COMMON_ENEMY` for a shared enemy + `GROVEL_PCT` of the fear of
  a stronger one + `LEAGUE` while a leader leads (less it for the leader itself and the chosen enemy).
* One **main enemy** (in the memory): chosen at `ENEMY_MIN` (40), kept for `HOLD` (40) turns and after that while its
  value is at `KEEP` (20). War is declared on it at `DECLARE` (45) from turn `FROM_TURN` (60), when it is reachable,
  there is no treaty, no young peace, and no war with someone the army is not 4 times as big as.
* The others are **courted** once per `COURT_COOL` (20) turns: a pact at friend `PACT_MIN` 30, an alliance at
  `ALLY_MIN` 55, a gift (A's gold gift) where the relation is too low for a pact and friend is at `GIFT_MIN` 25.

All numbers are first guesses, to be tuned against the soak. `soak --detail` prints a line per wizard: the system, and
for B the main enemy, the weariness against each wizard at war and the ruthlessness seen (`Ai::diplomacy_note`).

## Friends and enemies

Every few turns (with some hysteresis, so it does not flip each turn) each other wizard gets an enemy value and a
friend value from the three numbers. At most one or two main enemies, the only targets of a declaration of war;
the others are befriended by friendship and shared enemies; a war on someone else is ended by the peace score.
Replaces `dominant_war`, `appease`, `coalition` and the hate rolls for a wizard of B.

## The comparison

A soak with the two systems in one game (task 761): wizards alternate between A and B, the order rotated by
seed, so a seat is no advantage. It prints for each system the games, the wins, the mean survival turn and cities
held at the end, the wars, the peace made and the stalled war turns. A system needs many games to be told from
chance (24 seeds at 3 wizards, 400 turns, is the first run).
