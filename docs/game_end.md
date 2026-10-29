# Game end

Code: `crates/f4x-core/src/endgame.rs` (the rules and the score), `wizard/fortress.rs` (`defeat`,
`capture_city`), `spell/cast.rs` (the Spell of Mastery hook); client: `game_over_screen.rs`.
The original's rules come from ReMoM's `CONQUEST.c`, `SCORE.c` and `SPLMASTR.c` and the review
notes under `~/projects/ReMoM/doc/@CodeReview/` (`CONQUEST-Resolve_Wizard_Conquest.md`,
`CONQUEST-CP_Is_Dead.md`, `CONQUEST-End_Of_Game_Score.md`, `SPLMASTR-*.md`).

## 1. How a game ends

| Way | When | Winner |
|---|---|---|
| **Conquest** | A wizard is defeated (their last city taken) and only one wizard is left. | the last one standing |
| **Human defeated** | The human wizard (`Game::human`) is defeated, even if several rivals remain. | whoever took the last city |
| **Spell of Mastery** | A wizard finishes casting it. | the caster |

The original (`Resolve_Wizard_Conquest`): when a wizard loses their last city, or is banished and
resigns, the game is lost if that wizard is the human; otherwise `CP_Is_Dead` checks whether any
computer wizard still has a fortress or a city, and if none does the human has won. The Spell of
Mastery ends it through `Spell_Of_Mastery` (the human wins; if a computer wizard casts it, the
human loses).

A wizard with a fortress city taken but other cities left is only **banished** (casting the Spell
of Return, task 15), not defeated. A banished wizard is still a wizard: conquest is not won
until they too have lost every city.

## 2. What the engine does

- `Game::game_over: Option<GameOver { winner, kind, turn }>`, saved with the game (as is
  `Game::human`, which the client sets; saves from before it load with the human unset and the
  client fills it in).
- `Event::GameOver { winner, kind, turn }` is emitted once, at the moment of the winning
  event, inside the same event list (so `WizardDefeated` comes first).
- **Afterwards every command is refused** with `Rejection::GameOver` (`Game::apply`). The game
  can still be saved and looked at.
- The Spell of Mastery is a custom spell hook (`spell_of_mastery`, set by the importer for the
  spell of that name). It is cast like any overland spell, over as many turns as its cost needs
  (5000 mana in the original data), and wins the game when it is paid for. It has no target.

## 3. The aftermath of a defeat (task 82)

`endgame::aftermath(loser, winner)` runs when a wizard is defeated:

- all the loser's units are dismissed (heroes, creatures and normal units). A defeated wizard
  has no city, so none of them is garrisoned. A wizard who resigns while banished still has
  cities: `cities_go_neutral` first turns each into a neutral city and hands it its garrisoned
  normal units (heroes and conjured creatures are dismissed);
- the loser's enchantments come off every city and every unit, their
  overland enchantments (`magic.global`) and the spell being cast are cleared;
- their magic nodes become unowned (and lose any Guardian Spirit);
- the conqueror takes half their gold (the original also takes half their mana; `banish`
  already takes that when the fortress falls, so it is not taken a second time);
- `Player::defeated_by` records who did it (used for the score).

## 4. The score

The original scores only the human wizard, at the end of the game (`End_Of_Game_Score_Draw`).
`Game::score(player)` gives the same breakdown for anyone:

| Part | Points |
|---|---|
| Spells known | 1 each |
| Townsfolk | citizens in the wizard's cities, halved |
| Wizards defeated | 50 each |
| Fame | 2 per point |
| Time | `2000 - 2 * turn`, only while the fortress stands and nobody else cast the Spell of Mastery; **negative late in the game, and added anyway** |
| Spell of Mastery | 250 for the caster |

Casting the Spell of Mastery counts every wizard still standing as defeated (the original
marks them all as conquests). A wizard counts as having defeated another when they took that
wizard's last city (`defeated_by`). The total is then scaled by difficulty: Intro halves it,
Easy takes three quarters, Normal leaves it, Hard doubles and Impossible triples it
(`scale_for_difficulty`). `Score::total` applies the game's `scoring.score_percent` (task 247), and
the game over screen shows the scale. The original draws a bar of `min(score / 40, 200)`.

## 5. The client

`Screen::GameOver` opens when `Game::game_over` is set (after any battle on screen ends). It
shows the headline (victory or defeat and why), the turn and the human's score breakdown, and
offers *New game*, *Look at the map* (the map stays visible; commands are refused) and *Quit*.

### The scenes (`end_scene.rs`)

Before the score the client plays the original's full-screen scenes from `CONQUEST.c` and `SPLMASTR.c`, one after the
other from a queue (`App::end_scenes`): `Win_Animation` when the human wins, `Lose_Animation` when the human is out,
and before the victory by the Spell of Mastery the parade of the rivals' portraits (`Spell_Of_Mastery`, a minute for
each rival that is not banished). `Conquest_Animation` plays when the raiders (the neutral player) destroy a wizard,
because the original returns early unless the raiders are on one side of it, and `Return_Animation` when a rival is
banished. One stage is a few ticks of the original's clock (18.2 a second): victory 4 ticks and 90 stages, defeat 4 and
45, conquest 5 and 30, return 5 and 40, parade 3 and 12 plus 60 for each rival. Esc or a click ends a scene. The hands
the victor holds up are chosen by a table the review of ReMoM could not read (`GAME_PlayVictoryAnim`'s `off_79C7F`); the
one used gives the shaman's hands to Raven, the priestess's to Sharee, Oberic's to Oberic, the draconian's to Sss'ra,
a woman's to Freya, Ariel and Kali, and a man's to the rest, which fits the five targets in their order.
`--shot FILE --scene win|lose|conquest|return|parade [--shot-after SECONDS]` shows each.

## 6. Not done

- A rival's win by the Spell of Mastery is only the defeat scene: the sphere scene before it
  (`Spell_Of_Mastery_Lose`) is task 360.
- The AI conquest rules that would feed relations (63, 67). The "has started casting"
  announcement is in as plain text (animation: task 287).

## 7. Differences from the original

- The original swaps back to the main menu program at the end (`GAME_EXE_Swap`); here the
  game stays loaded and refuses commands.
- The original's win test is made only when a computer wizard falls and counts the human as
  the winner; here the winner is a player and the rule works for any number of wizards. The
  human's own defeat ends the game as in the original.
- A banished wizard who resigns (a weak computer wizard at once, the human by choice) is defeated, as in the original; the original's double halving of the human's mana is not copied.
