# Diplomacy system B (design, 21 November 2026)

**System B is the AI's only diplomacy since 23 November 2026 (task 779).** System A, the AI's statecraft as it was before (`statecraft.rs`, `diplomacy.rs`: the original's rating `Diplomacy_Test` with its dice and patience, the dominant war, appeasing, the coalition, the hate rolls, three formulas for peace), was removed; it was too much the original's way of doing things. B has no dice: every answer and every offer follows from three values for each other player, and the numbers are levers (the `diplomacy_b` group of `ai_strategy.ron`, `AIS_DIPLO_B_<NAME>`). The human is treated like any wizard (`diplo_b_talk.rs`). The AI keeps its own opinion of every other player (`opinion.rs`, task 780): friendship, the lasting `hidden` regard, a broken-treaty flag, in its `Memory`, changed by `Memory::feel` from the core's events by the table of `diplomacy.md` section 5, with no dice (each `Random(n)` of the original is its average, `divergences.md`). A first opinion comes from the two wizards' books (`start_feeling`). The core's new fact events (task 782: `BattleLosses`, `AssaultFailed`, `FortressTaken`, `WizardDestroyed`, `EnchantmentCast`, `CitySpellCast`, `Subverted`) are weighed by the same table, except that a fortress taken or a wizard destroyed is judged by whom the AI likes (the original's flat -20 / -40 becomes `base * (friendship for the loser - friendship for the winner) / 100`, within +-base: a friend ridding it of an enemy pleases it) while the winner's ruthlessness, and so the fear of it, grows either way (`opinion.rs`: `enchantment_reaction`, `bad_city_spell`). The per-turn rules are the AI's own look (`Memory::look`, task 783, called once a turn from `play_kept`): a pact or alliance in force grows the friendship, the enchantments in force sour or sweeten it (a tenth of the cast's value), an army stronger than its own or too many cities, units next to its cities (a warning, a turn of grace, then the fine; the original's breaking of the treaty is left to the AI's own decision) and the drift back to the default. What each event is worth, the limits and the drift are the `opinion` group of `ai_strategy.ron` (`AIS_OPINION_<NAME>`). Each rule the original rolls a die for gives its average; a part of a point is added up from turn to turn (`Opinion::carry`). The client and the console read the AI's friendliness for the human through `Diplomats::feeling` (`set_feeling` for the demo screens, `refuses_audience` for "will not see you": a wizard at -90 or lower, not at war); the console's rivals do not play, so they show the feeling they start with. The soak's detail line of each wizard ends with its regard for every wizard it has met: friendship (feeling and lasting regard), fear and strategic value (`Ai::diplomacy_note`). The AI never attacks a pact or alliance partner (task 776): a partner is never chosen as the enemy (`choose_enemy`, `race::enemy`), `attack.rs` and `military::engage` send nothing against a partner's units, and the war plan aims at no partner's city (`attack::may_target`). Outside the talks, war is declared by the attack module before an attack on a player not at war: by the campaign on the owner of its target city, and by the perimeter on units near a city when the AI is open to war on them (`diplo_b_stance::open_to_war`, the test of the declaration below); never in a peace period (`ai-design.md`, "Attacks"). The soak counts the orders the game refused because of a treaty ("refused by a treaty", it stays at 0). The patience counters of the original have no place in B (its cooldowns are `Memory::due`).

## Three values for each other player (`Regard`)

* **Friendship** (how likeable): the wizard's own opinion of the other, its feeling and its lasting regard
  (`opinion.rs`). Makes the wizard overlook faults and trade, make pacts, ally and accept peace. (The trades
  also weigh `statecraft::affinity`: fame, a Charismatic retort, the same race, shared books.)
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
and to those two functions, holds each wizard's memory, so the judge answers with what it
remembers. The memory is saved with the AI's plans. The master tells every seat the events of the turns
(`Controller::notice`), the same ones a human's client is shown, so a wizard hears of a battle it is dragged into on
another wizard's turn.

## One peace score

`peace_wish(me, other)`: war weariness, plus friendship (willing to overlook), plus fear (the weaker side begs,
the stronger finishes it), plus strategic (prey is a minus; a shared enemy elsewhere, a second front or a
threat to its own cities a plus), with the personality's modifier. **It offers peace at `OFFER` and accepts at
`ACCEPT`** (a little lower), from the same function, for the wizards among themselves and for the human's offers.
A cool-down keeps it from asking every turn. Nothing is rolled.

## What is built (task 762: the peace)

`crates/f4x-ai/src/diplo_b.rs`; every number is a lever of the `diplomacy_b` group of `ai_strategy.ron`, and
`AIS_DIPLO_B_<NAME>` overrides it, so a soak can vary it. The first guesses:

```text
peace_wish = weariness
           + gap       (their power - ours) * GAP / max(theirs, ours)         GAP 50: losing is a plus, far ahead a minus
           + fear      FEAR_PCT % of fear if they are the stronger, minus half of it if we are  (FEAR_PCT 40)
           + friendship  (feeling + lasting regard) * FRIEND_PCT / 100        FRIEND_PCT 10
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
  city's worth (the `city_power` lever) for each city, as the observer sees it. Ruthlessness is added to it
  (times the nearness): the memory notes a razed city (`RUTH_RAZE` 15), a war declared (`RUTH_WAR` 6) and a treaty
  broken (`RUTH_BROKEN` 10), a fortress taken (`RUTH_FORTRESS` 10) or a wizard destroyed (`RUTH_DESTROYED` 20), the last two by every wizard that has met the winner, by a wizard it has met, fading by half every `RUTH_HALF` (150) turns, at most `RUTH_CAP`
  (60). The strategic term also has the Mastery caster (`MASTERY` -60) and the leader (`LEADER` -15 against the
  leader, +15 for the others while at war with it).
* **Offer** at `OFFER` (80) with a cool-down of `COOL` (15) turns for each wizard, kept in the memory; **accept** at
  `ACCEPT` (60). The dice are gone from the decision.
* **Where it plugs in:** `diplo_b_talk::approach` offers peace by `offer_due`; a peace is answered by the score
  from the judge's memory (`diplo_b_talk::shortfall`).

By arithmetic (not measured), a stalled war between equals with nothing in it reaches the accept mark after about
a hundred quiet turns and the offer mark after about a hundred and twenty-five, moved by 10 or so either way by the
friendship and the temper; a lost war reaches both much sooner. These are guesses to be read against a soak
(task 861), not tuned values.

## What is built (task 763: friends and enemies)

`diplo_b_stance.rs`, run from `statecraft::run` (the wars) and from `diplo_b_talk::approach` (courting). Every
`REVIEW` (5) turns each other wizard gets `Values { enemy, friend }`:

* **enemy** = prey (weaker and reachable, up to `PREY_VALUE` 60) + `HOSTILE_PCT` of the ill will + `LEADER_ENEMY` for
  the leader + `MASTERY_ENEMY` for the Mastery caster + `AT_WAR` for the one already fought + half the fear if the
  other is not stronger (strike first); less the share by which it is stronger once past `TOO_STRONG_PCT`, less 40
  for a pact and 80 for an alliance; unreachable wizards stay below `ENEMY_MIN`.
* **friend** = `STANCE_FRIEND_PCT` of the friendship + `COMMON_ENEMY` for a shared enemy + `GROVEL_PCT` of the fear of
  a stronger one + `LEAGUE` while a leader leads (less it for the leader itself and the chosen enemy).
* One **main enemy** (in the memory): chosen at `ENEMY_MIN` (25), kept for `HOLD` (40) turns and after that while its
  value is at `KEEP` (20). War is declared on it when its enemy value, less `WAR_DAMP` (15) for each war already fought, is at `DECLARE` (30), from turn `FROM_TURN` (5), when it is reachable,
  there is no treaty, no young peace, and no war with someone the army is not `BUSY_ARMY_X` (2) times as big as.
* The others are **courted** once per `COURT_COOL` (20) turns: a pact at friend `PACT_MIN` 30, an alliance at
  `ALLY_MIN` 55, a gift (A's gold gift) where the relation is too low for a pact and friend is at `GIFT_MIN` 25.

All numbers are first guesses, to be tuned against the soak (task 861). `soak --detail` prints a line per
wizard: the main enemy, the weariness against each wizard at war, the ruthlessness seen and the regard for each
wizard it has met (`Ai::diplomacy_note`).

## Friends and enemies

Every few turns (with some hysteresis, so it does not flip each turn) each other wizard gets an enemy value and a
friend value from the three numbers. At most one main enemy, which a war is declared on; the others are
befriended by friendship and shared enemies; a war on someone else is ended by the peace score.

## Answering and talking (task 779, `diplo_b_talk.rs`)

* A **pact** is accepted when the friend value (`diplo_b_stance::values`, plus the personality's mood) reaches
  `PACT_ACCEPT` and the friendship is above the rules' floor; an **alliance** at `ALLY_ACCEPT` over its floor.
* **War on a third wizard**: friendship with the asker plus the enemy value of the third reaches `WAR_ACCEPT`, never
  against one much stronger (`TOO_STRONG_PCT`) unless already at war with it. **Break an alliance with a third**: the asker is
  liked `BREAK_ACCEPT` more than the ally.
* **A threat**: a wizard that fears the asker (`YIELD_FEAR`) pays and yields, one clearly stronger than the asker declares war,
  else it refuses. **A demand** is yielded to by one that fears the asker. **A trade** is by worth (`trade.rs`).
* **Peace** is the peace score (`peace_wish`). Nothing is rolled.
* **Counter-offers** (`bargain.rs`): how many points a wizard is short of a pact, an alliance or a peace
  (`shortfall`) is worth `GOLD_PER_POINT` gold each. Short by at most `COUNTER_REACH`, it names a price (gold, or a
  thing and the gold that makes it up); with points to spare it pays up to their worth for what a priced offer asks of
  it; further short, it refuses whatever is offered. A price that covers the shortfall is accepted.
* **Approaching** (`approach`, at the end of the wizard's own turn, which waits for the answer to a proposal), every
  other player alike, computer or human: peace by `offer_due`; war on someone it hates (`WAR_BELOW`) with no treaty; the
  treaty broken at `BREAK_BELOW`; a warning to a partner whose friendship fell to `WARN_BELOW` (every `WARN_COOL`
  turns). Then at most one of: courting (pacts, alliances, a gift for a low relation) by `court`, a demand of gold from
  a weaker player it dislikes (every `DEMAND_COOL` turns), a gift from a close friend with a treaty (`GIFT_FRIEND`,
  every `GIFT_COOL` turns); else a trade (every `TRADE_COOL` turns). The wars it picks on its own turn come from the
  enemy value (`diplo_b_stance::run`), which also counts an ally's war (`ALLY_WAR`), so an ally joins the wars of its
  friend.
* **Call to arms** (`war_call.rs`, `diplomacy.md` section 6): the friendship for the caller, the temper and the enemy
  value of the third make the support for the war (`war_support`; a war the ally is already in counts as enough), and at
  `WAR_ACCEPT` it joins. Short by at most `COUNTER_REACH`, it names a price once (`GOLD_PER_POINT` gold for each point,
  at least the least price of a trade). Else, or when the price is not paid, it joins for a caller it feels at least
  the rules' `alliance_relation` for and refuses the rest. It never joins against one much stronger
  (`TOO_STRONG_PCT`). As the caller it pays a price it can spend and would part with, worth at most
  `COUNTER_REACH` x `GOLD_PER_POINT`, and thinks `CALL_PRICE_PENALTY` less of the ally that asked.
* **World maps** (`map_swap.rs`, the setting `expanded_diplomacy`): it takes a swap when the other's map shows it
  something new and it has explored less than `MAP_REFUSE_PCT` percent (three times) of the squares the other has. It
  offers one to a partner whose map would show it at least `MAP_LEAST_NEWS` squares and that it would take a swap
  with, at most every `MAP_COOL` turns for each, when it has nothing else to put to that player. To judge this it reads
  how much the other has explored and how much of that it lacks, which the human is not shown.
* **Trade and research pacts** (`econ_pact.rs`): both sides gain, so it takes either from any treaty partner. It
  offers one the two lack, research first, to a partner it has nothing else to put to, at most every `PACT_COOL` turns
  for each.
* **Calling** (`approach`): a wizard at war calls an ally that is not in that war, against the strongest such enemy
  with at least `CALL_ENEMY_PCT` percent of its own power, at most every `CALL_COOL` turns for each ally.
* **Trespass** (`judge_trespass`, `diplomacy.md` section 6): at the last warning it declares war at
  `TRESPASS_WAR_BELOW` (from `FROM_TURN`), breaks the treaty at `TRESPASS_BREAK_BELOW`, else overlooks it.
