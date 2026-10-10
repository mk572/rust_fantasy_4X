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
- Not yet (task 41, 43-45): walls and gates, casting in battle, battlefield spells, the
  battle screen, the battle AI, auto-resolve.
