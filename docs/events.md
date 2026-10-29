# Random events

How the original game's random events work, for task 68. Sources: ReMoM `MoM/src/EVENTS.c`
(`Determine_Event`, `Event_Twiddle`, `Get_Event_Victim`), the notes in `~/projects/ReMoM/doc/NextTurn/EVENTS-*.md`
and `doc/MoX-Events.md`, the manual (page 105), and the places the events are read
(`CITYCALC.c`, `MainScr.c`, `CityScr.c`, `REPORT.c`).

**Events are not enchantments and have no timers.** An event is a record in one table (`events_table`);
the rules that care read its status. A lasting event ends by a die roll each turn, not by a count of
turns; the number it keeps (`Duration`) only counts the turns it has run, and forbids an early end.
There is no countdown anywhere in the original, and spells have no turn limits either (see
`magic.md`); the old `Duration::Turns` and `timers.rs` were removed for that reason.

## The table

Eighteen events, ids as in `EVENTS.LBX`. Each has a status; some carry data.

| Id | Event | Good? | Kind | Data kept |
|---:|---|:-:|---|---|
| 1 | Great Meteor | no | one-off | city, player |
| 2 | The Gift | yes | one-off | player, item |
| 3 | Disjunction | yes | one-off, world | none |
| 4 | Diplomatic Marriage | yes | one-off | player, own city, neutral city |
| 5 | Earthquake | no | one-off | city, player |
| 6 | Piracy | no | one-off | player, gold lost |
| 7 | Plague | no | lasting, one city | city, player, turns run |
| 8 | Rebellion | no | one-off | city, player |
| 9 | Donation | yes | one-off | player, gold |
| 10 | Depletion | no | one-off | player (the mineral is picked when it happens) |
| 11 | New Minerals | yes | one-off | player (likewise) |
| 12 | Population Boom | yes | lasting, one city | city, player, turns run |
| 13 | Good Moon | yes | lasting, world | turns run |
| 14 | Bad Moon | yes | lasting, world | turns run |
| 15 | Conjunction, Chaos (red) | yes | lasting, world | turns run |
| 16 | Conjunction, Nature (green) | yes | lasting, world | turns run |
| 17 | Conjunction, Sorcery (blue) | yes | lasting, world | turns run |
| 18 | Mana Short | no | lasting, world | turns run |

"Good" only decides who is picked as the victim (below). The moons and conjunctions are marked good
although they help some wizards and hurt others.

Statuses: 0 nothing, 1 starting (chosen this turn), 2 ongoing, 3 parked (only when random events are
switched off: every event is set to 3 and nothing fires). The game also keeps `last_event_turn`
and an `event_delay_count`.

## Each turn

`Next_Turn_Calc` runs two steps, `Determine_Event` and then `Event_Twiddle`, only when random events
are switched on in the game settings (task 89) and the turn is a normal one (not Time Stop).

### 1. Does an event fire? (`Determine_Event`)

1. **Pressure.** `pressure = turns since the last event` (0 if the last event is in the future), kept
   at 0 while `event_delay_count < 5`: no event in the first five turns of a game. (ReMoM notes that
   the original counts calls, not turns, so a loaded game misbehaves. Count from the real turn, as
   task 68 says.)
2. **Difficulty weight.** Intro x1/2, Easy x2/3, Normal x3/4, Hard x4/5, Impossible x1.
3. **Roll.** `Random(512) > pressure` means no event. The chance is pressure / 512 and grows with
   every quiet turn until something fires. (The manual says the same: the longer without events,
   the likelier one.)
4. **Draw up to five times.** Each try draws `event = Random(18)` and a victim wizard, then vetoes
   the draw (try again) if any of these hold:
   - no victim, or the victim has no fortress;
   - the event's status is ongoing (2) or parked (3);
   - one of the six world events (moons, conjunctions, Mana Short) is ongoing and this draw is also
     one of the six: only one at a time;
   - Good Moon while Bad Moon runs, and the reverse;
   - Piracy and the victim has under 100 gold;
   - Meteor, Marriage, Earthquake, Plague, Rebellion or Population Boom and the victim has no city
     (a random city of the victim is picked); Marriage also needs a random neutral city;
   - Marriage or Meteor before turn 150;
   - Rebellion in the victim's fortress city, or in a city where summoned units outnumber normal
     ones, or with a hero in it;
   - Depletion or New Minerals and the victim has no suitable mineral square;
   - Disjunction and nobody has an overland enchantment;
   - Plague on a city with a Population Boom, and the reverse.
5. The first draw that survives is armed: status 1 with its data, `last_event_turn = turn`,
   `event_delay_count = 0`. If five tries fail, nothing happens this turn.

### 2. The victim (`Get_Event_Victim`)

Each wizard has `power = magic power + research + army strength`. For a bad event the weight is the
power (the strong are hit more); for a good event it is `600 - power`, at least 0 (the weak are
helped more). While the sum is over 500 every weight is halved. Then a weighted pick; no weight
means no victim.

### 3. Playing the event out (`Event_Twiddle`)

Every event with status 1 or 2 is advanced, in the order of the table. The player is told when
it starts and, for lasting ones, when it ends (the report goes to the human only if the event
concerns them; world events go to everyone).

| Event | What happens |
|---|---|
| Great Meteor | If the city still belongs to the victim: the effect of Call the Void on that city (buildings, population and units lost; the owner is told what). Status back to 0. |
| Gift | The victim gets an item made by `Make_Item(2, <the victim's books>, 0)` through the item screen. Back to 0. |
| Disjunction | Every wizard loses all 24 overland enchantments. Back to 0. |
| Marriage | The neutral city and its units go to the victim, producing Trade Goods; the area around it is explored. Back to 0. |
| Earthquake | The Earthquake spell's effect on the city; reported only if something was lost. Back to 0. |
| Piracy | The victim loses between 30 and 50 percent of the reserve, rounded down to a multiple of 10; never below 0. Back to 0. (The reconstructed code computes `gold * 100 / (29 + d21)`, which is over three times the reserve and then capped at 0; that is a fault of the reconstruction, the manual's 30 to 50 percent is the rule. Use `gold * (29 + d21) / 100`.) |
| Plague | Each turn, with chance (population / 10) the city loses one citizen if it has more than 2; no unrest: the +2 unrest belongs to the Pestilence spell only, and nothing in the code gives it to the event. Ends on a 1 in 20 roll once it has run over 3 turns, or when the population falls under 2 (set to 2). |
| Rebellion | Units of normal races go neutral, summoned ones are dismissed, the city goes neutral. Back to 0. The victim never loses the fortress city. |
| Donation | The victim gets 100 + 5 * d100 gold (105 to 600). Back to 0. |
| Depletion | A mineral special on one of the victim's city squares is removed. |
| New Minerals | A mineral special (iron, coal, silver, gold, gem, mithril, adamantium, quork, crysx) appears on a square of one of the victim's cities. |
| Population Boom | The city's population growth doubles. Ends on a 1 in 20 roll once it has run over 3 turns. |
| Good Moon | See the effects below. Each turn, once it has run over 3 turns, it ends when `d20 <= turns run - 2`, so the chance climbs from 10 percent. |
| Bad Moon, Conjunctions, Mana Short | The same end rule. |

The two end rules differ: Plague and Population Boom end on a flat 5 percent a turn after turn 3;
the six world events end on a chance that rises with their age (the manual says 5 percent for
all of them, and that is not what the code does; the code wins unless we decide otherwise, see
`divergences.md`).

## Effects while an event is ongoing (status 2)

- **Mana Short.** All power from nodes and cities is zero. Upkeep of spells and units still has to be paid
  from the reserve (manual); `CITYCALC.c` returns before the node and city sums.
- **Conjunction.** The matching kind of node gives double power and the other two kinds half
  (`CITYCALC.c` phase 3e). A warped node drains 5 whatever the conjunction.
- **Good Moon.** The mana of the religious buildings (shrine 1, temple 2, parthenon 3, cathedral 4,
  after Divine or Infernal Power) is doubled for a wizard with a life book and halved (rounded down)
  for one with a death book; **Bad Moon** the reverse. A wizard with both gets both changes. Only
  the religious buildings are affected, not nodes, terrain or the race (`CITYCALC.c`; implemented in
  `Events::moon_mana`). The help says 50 percent more; the game follows the code, because more mana
  makes for a faster game (decision of 17 October 2026, B50).
- **Population Boom.** Growth of the city doubles, as Stream of Life does (both are a +100 percent
  on the growth here, so together they triple it where the original quadruples; task 303).
- **Plague.** Marked in the city screen and the city's lines in the reports.

The main screen shows the running world event by name above the minimap ("Conjunction", "Good Moon",
"Bad Moon", "Mana Short"); the reports (`REPORT.c`) list the plague and boom cities.

## Mapping to this game

- **Its own domain, `world_event/mod.rs`, with no handlers.** `Events` is the table: which events are going
  on, the turns each has run and the data it carries (a city, a player, an amount). Its fields are
  private; the rules ask it (`is_ongoing`, `moon_mana`), as `CITYCALC.c` reads `events_table`, and
  `Game` holds it beside `diplomacy`. It is saved with the game. Nothing is registered anywhere:
  there are no callbacks, no trait objects and no per-event types.
  - **Readers** get the table through the view they already have: `Economy` carries it
    (`with_events`), the wizard's power will ask for Mana Short the same way. A rule that cares
    about an event is one line in the rule it changes, not a hook.
  - **Playing an event out** is one `match` on the event kind in `world_event/mod.rs` that returns plain
    outcomes (lose gold, destroy buildings of a city, a city changes owner, add an item, a citizen
    dies); `Game` applies them with the functions that already exist (Call the Void and Earthquake
    are spell effects, a rebellion is a conquest). The draw and the vetoes are one function over
    the table, the wizards and the cities. So the number of places that know about the 18 events
    is three: the table, the draw, the play-out.
  - **Where it is:** `world_event/mod.rs` (the table, the pressure, the weights, the end rules, the message
    text), `world_event/game.rs` (the draw, the vetoes, the playing out), `event_popup.rs` in the client
    (the `Notify2` box, the world event under the minimap, the city marks). The turn runs the
    events after the treasury and before the cities grow, as `Next_Turn_Calc` runs them before the
    cities' changes. What is left is task 303.
- Order in the turn: after the other end-of-turn steps that the original runs before it, as in
  `Next_Turn_Calc`; read the order there before placing it.
- Randomness from `GameRng`, drawn in the original's order (event type, victim roll, then the
  data rolls) so that a seed means the same thing as the order listed here.
- Messages: the table `EVENTS.LBX` has seven strings per event (started, ended, happened, elsewhere
  and others); the pop-ups are task 143 and 264, which need the events to exist first.
- Settings: random events on or off is a game setting (task 89). When off, no event fires and none
  is pending.
- Tests use the real numbers above: the pressure curve, the five tries, each veto, each end rule
  and each effect with the real node and temple values.

## Open questions

- The manual says every lasting event has a 5 percent chance to end each turn; the code has the
  rising chance for the six world events. Decide which one to follow (the code, as the game plays).
