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
