# Combat

Code: `crates/f4x-core/src/battle/` (`mod.rs` the battle, `attack.rs` the dice, `map.rs` the
grid, `game.rs` the link to the overland game). Original rules were taken from the help text
and the reverse-engineered code (`Battle_Unit_Process_Attack`, `Battle_Unit_Attack_Target` in
ReMoM's `Combat.c`).

## 1. The model (task 33)

A **battle is a separate record**, not part of the overland `hecs` world. `Battle` holds the
two players, a `BattleMap`, a list of `BattleUnit`s, the turn, whose side is active, and its
own seeded random generator. A `BattleUnit` is a *copy* of an overland unit with every
modifier already applied (melee, ranged, defence, resistance, hit points per figure, figures
alive, damage on the front figure, moves, ammunition, flags for flying, first strike, long
range, the ranged attack kind), so combat needs no access to the world and can be tested, replayed or driven by
an AI alone.

`Game` links it in (`battle/game.rs`):
- `Command::BeginBattle { attackers, defenders }` copies two stacks into a battle
  (`Game::start_battle`): wounded units start wounded (`Hurt` becomes figures and front damage),
  the defenders' square gives the terrain, the sides are deployed on opposite edges.
- `Command::Battle(BattleCommand)` carries out an order and returns `Event::Battle(..)` events.
- When a side has no active unit left the results are written back (`finish_battle`): dead
  units are despawned, survivors keep their wounds as `Hurt`, `Event::BattleEnded` names the
  winner. The overland `AttackRequested` event is what a client or AI turns into
  `BeginBattle`.
- A battle in progress is **not saved**: finish or flee before saving (the client should
  block saving during a fight).

Not yet written back (task 42): experience, fleeing losses, loot, fame, summoned units
vanishing, moving the winner onto the square.

## 2. Attack resolution (task 34)

Everything is in `attack.rs` as pure functions over numbers and a random generator.

- **A die** is ten-sided. It hits on 8, 9 or 10 (30 percent); each plus to hit lowers the
  number needed by one (+10 percent), a 10 always hits (so at least 10 percent).
- **An attacking figure** rolls its strength (melee, ranged or thrown) in dice. The defender
  then rolls its **defence** in dice, blocking on the same terms, and only the hits left
  over do damage.
- **Damage lands on the front figure first.** A figure that dies hands the rest of the
  damage on to the next, which makes a **fresh defence roll** against it. Figures are
  `hits` hit points each; the front figure's damage is carried between attacks.
- **Melee exchange order** (`melee_exchange`): the attacker's thrown or breath attack with
  all its figures; then, if it has first strike (and the defender no negate first strike),
  its blows; then the defender hits back with the figures it still has; then an attacker
  without first strike swings with the figures it still has.
- **Ranged attacks** draw no reply. Arrows, slings and boulders take -1 to hit for every
  three squares of distance (at most -1 with long range) and use one ammunition; spell shots
  take no distance penalty. One attack or shot per unit per turn.
- A walking unit cannot hurt a **flying** unit in melee (the flier still hits back), unless
  it throws or breathes.
- **Doom damage** (`ignores_defense`) skips both the to-hit and the defence rolls.

Not yet (task 35-37): immunities and resistance rolls, gazes, touches, life steal, armour
piercing and illusion, large shield, the spell shots using mana, haste, blur.

## 3. The tactical map and the turn (task 40)

- The grid is **21 by 22** squares (the original's size). A square is open, rough (forest,
  swamp, rubble: two moves to enter), or blocked (trees, rocks: walkers cannot enter). Flying
  units pay one move everywhere. Moves are in half-moves like the overland map; a unit with
  any move left may take a last step that costs more.
- Movement is in eight directions, cannot enter an occupied square, and is found with a
  cheapest-path search (`BattleMap::reachable`, `Battle::reachable`).
- The map is generated from the overland terrain of the defenders' square (more rough ground
  in forest and swamp, rocks in mountains); the columns around the deployment edges stay open.
  Attackers stand on the west edge, defenders on the east, in blocks centred on the middle.
- **Turns:** the attacker plays all its units, then the defender; a new turn starts when the
  defender ends its turn. Each unit may move and make one attack per turn (a melee attack
  uses up the rest of its moves). `Done` ends a unit's turn, `EndTurn` the side's.
- **A side loses when it has no active unit.** `Flee` hands the battle to the other side
  (the losses of fleeing units are task 42). After **100 turns** the defender wins.
- Not yet (task 41, 43, 44): walls and gates, casting in battle, battlefield spells,
  auto-resolve.

## 4. Orders and events

| Order (`BattleCommand`) | Rule | Refused when |
|---|---|---|
| `Move { unit, to }` | Walk to a square in `Battle::reachable`; costs its path. | no moves left, unreachable |
| `Attack { unit, target }` | Melee exchange with an adjacent enemy (thrown/breath first); ends the unit's moves. | not adjacent, already attacked |
| `Shoot { unit, target }` | Ranged attack at any distance (`Missile` or `Magic` kind), uses one ammunition. | no ranged attack, out of ammunition, already attacked |
| `Done { unit }` | Gives up the rest of the unit's turn. | not the active side's unit |
| `EndTurn` | Hands play to the other side; the turn number rises when the attacker is up again. | |
| `Flee` | All the side's active units leave; the other side wins. | |

Events: `Moved` (with the path), `Attacked` (damage and kills both ways), `Shot`, `UnitDied`,
`TurnStarted`, `Fled`, `Ended`. `Game::battle_command` wraps them as `Event::Battle(..)` and,
when the battle ends, adds `Event::BattleEnded` and the results of `finish_battle`. A unit's
ranged kind comes from its abilities (`ranged_kind_of`): bows, slings and rock throwing are
`Missile`; thrown weapons and breaths are `Short` (used inside melee, no ammunition); the bolt
attacks are `Magic`.

## 5. Battles against sites (task 8)

`Command::EnterSite` copies a lair's guards into neutral units (`NEUTRAL`) on the site's square
and starts a battle against them (`ActiveBattle::site`). When it ends, `finish_site_battle`
removes the neutral survivors, and if the attacker won, clears the lair, gives the treasure
and moves the survivors in (see `docs/lairs.md`). A lost fight leaves the lair as it was.

## 6. The battle screen (task 45)

Code: `crates/f4x-client/src/battle_screen.rs`.

- **Start:** the client turns `AttackRequested` and `SiteEncounter` events of the human player
  into `BeginBattle` / `EnterSite` at once (a confirm box is task 133 and 128). Any time
  `Game::in_battle()` is true the screen shows.
- **Projection:** the original's: a cell is 32 by 16 pixels, a step in x moves (+16, +8) and a
  step in y moves (-16, +8) (`Combat_Grid_Screen_Coordinates`). The grid is scaled to fit the
  window. The original shows only a band of the 21 by 22 squares; this screen draws them all
  (task 131).
- **Art:** ground from `cmbgrass`/`cmbdesrt`/`cmbmount`/`cmbtundr` (Myrror: the `c` variants)
  by the overland terrain, entries 0-3 as plain ground, 48-52 trees and 53-57 rocks on blocked
  squares; rough squares are the ground tinted. Units are drawn as their figures (`figures1` to
  `figure16`: fifteen unit types to a file, eight directions per type, four frames; the type number
  is the unit's `units1` entry, or 120 plus its `units2` entry) at the original's positions for
  the number of figures, on a diamond in the owner's banner colour, with a health bar and the
  number of figures. `extract-art` writes a `_banner.png` mask beside every figure frame (palette
  indexes 214-218) so the client can paint the owner's banner shades.
- **Animation** (`battle_anim.rs`): the core finishes an order at once; the screen keeps the battle
  from before it and plays the events one after another (walk 0.2 s a square with the walk
  frames 0,1,2,1; a lunge and a red flash for a blow; a missile for a shot; a fade for a death),
  then shows the real battle. Input and the opponent wait meanwhile. Facing follows the original
  table (`facing`).
- **Controls:** click one of your units to select it; blue squares are where it can walk; red
  outlines are enemies it can hit (adjacent: melee, otherwise a shot if it has ranged attack and
  ammunition). Click a blue square to move, an enemy to attack. Space or D is `Done`, Tab picks
  the next unit that can still act, Enter or E ends the turn, Flee asks twice. The buttons are the original's `compix.lbx` pictures (Spell, Wait, Done, Auto, Info, Flee; Wait picks the next unit); Spell, Auto and Info are drawn greyed until casting (43), auto-play (133) and the info window exist. End turn is a plain button, as the original ends a turn when every unit is done. The panel shows the
  hovered (else selected) unit's melee, ranged, defence, resistance, figures, hit points, moves,
  ammunition and abilities, and the last combat lines.
- **The other side** is played by `battle/ai.rs` after a short delay per order: each unit attacks
  an adjacent enemy, else shoots, else walks to the reachable square nearest the closest enemy,
  else is done. This is a placeholder for the real battle AI (task 46).
- `--battle` together with `--shot FILE` opens a test fight for screenshots.

## 7. Differences from the original and open work

- Grid band and deployment squares: the original plays on part of the grid (task 131).
- No unit figures, animation, projectiles or combat frame (task 132); the map art lacks
  autotiled edges, roads, rivers and cities (task 112).
- No casting (43), battlefield spells (44), walls (41), experience/fame/loot write-back (42).
- The attacking stack does not move onto the defenders' square after a win; no attack prompt and
  no AI-versus-AI resolution (task 133).
