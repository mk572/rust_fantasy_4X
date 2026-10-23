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
- A walking unit hits a **flying** unit in melee at half strength, rounded up (the flier still
  hits back at full strength; thrown and breath attacks are unchanged). Divergence 26: the
  original deals nothing.
- **Doom damage** (`ignores_defense`) skips both the to-hit and the defence rolls.

Ability rules (task 37): **large shield** adds 2 to the defence against anything but a melee blow
(arrows, thrown weapons, breath, spell shots and damage spells; before armour piercing halves it).
**Lucky** adds 1 to hit, to block and to resistance in battle. **Regeneration** (the ability, or
the Regeneration spell, which grants it) heals one hit as the unit's side's turn begins, heals the
unit fully after the fight and brings back a slain one (not drained or destroyed) if its side won.

**Spell shots** (task 38): ammunition is taken from the unit definition for each battle, so the
quiver is full again after every fight. A spell caster (`spellcaster` 20 mana, `spellcaster_major`
40) with a magical ranged attack shoots with its mana instead, 3 a shot (`BattleUnit::mana`,
`shoots_with_mana`; shots = mana / 3). Mana Leak takes 5 mana from each unit and one shot from a
unit with spell shots but no mana. Heroes' mana waits for task 51.

Not yet: haste, fear.

### After the battle (task 42, `battle/spoils.rs`)

Each of the winner's surviving normal units and heroes gains 2 xp for every enemy unit that died
(summoned ones too; fled ones give nothing), up to the last level experience gives. The winner
gains 1 fame for four or more kills and 1 for a very rare foe (an enemy unit of cost 600 or
more); the loser loses `(level + 1) / 2` fame for each hero it lost, and a human loser with more
than 20 fame loses one more when the winner got the four-kill point (the original's rule). A
neutral winner gets nothing. The fallen heroes' items are loot in the original (task 221).

### 2.1 Immunities and resistance (task 35)

`battle/immunity.rs` holds the rules as pure functions; the dice code asks it for the defence
to roll against each attack (`attack::process_attack`).

- **An attack has a nature** (`AttackNature`): the immunities that stop it, and its realm if it
  is magical. Melee blows, arrows and slings, boulders and thrown weapons are *normal weapons*
  unless the attacker's weapon is magical. Arrows and slings are also *missiles*, fire breath
  is *fire*, every spell shot (fireball, lightning, ice, nature, sorcery, death, drow, sprite,
  shaman bolts) is *magic* with its realm, lightning breath has a realm but no immunity, and
  the `illusion_attack` ability makes any of a unit's attacks an *illusion*.
- **A unit's immunities** are its abilities `weapon_immunity`, `magic_immunity`,
  `missile_immunity`, `fire_immunity`, `cold_immunity`, `poison_immunity`, `stoning_immunity`,
  `death_immunity` and `illusion_immunity` (`Immunities::of`). Lasting spells can grant them:
  a passive `GrantAbility` effect on a spell is read by `Game::unit_abilities` (Magic Immunity
  grants `magic_immunity`, Guardian Wind `missile_immunity`).
- **The defence rolled** (`effective_defense`, the original's `Battle_Unit_Defense_Special`):
  1. An illusion rolls no defence at all (0) unless the defender has illusion immunity.
  2. Against anything but melee, an immunity in common with the attack makes the defence 50.
  3. Weapon immunity against a normal weapon (melee, arrow, boulder or thrown) makes the
     defence at least 10, and this *replaces* the stop in step 2: a unit with both weapon and
     missile immunity gets 10, not 50, against normal arrows (the original's order, kept).
  4. Magic immunity makes the defence 50 against any non-melee attack that has a realm (spell
     shots and breath).
- **Magical weapons** (`Game::unit_has_magical_weapon`) ignore weapon immunity: heroes,
  fantastic and summoned creatures, and units with the `magical_weapon` ability (Flame Blade
  and Holy Weapon grant it) and units trained with magic, mithril or adamantium weapons (task 18).
  The original also uses a Chaos or Black Channels race change and Eldritch Weapon.
- **One fix from ReMoM's `OGBUG` notes is used:** thrown normal weapons *are* stopped by weapon
  immunity (the original's test for them can never match). Cold immunity is not set by any unit
  attack in the original, only by the Ice Bolt and Ice Storm spell data, so a unit's ice bolt is
  stopped by magic immunity alone.
- **Spells** (`Effect::Damage`): the spell's data lists who it cannot hurt (`immune`, read from
  the spell table's immunity byte: Fire Bolt stops fire and magic immunity, Ice Bolt cold and
  magic, Psionic Blast illusion and magic, Life Drain magic and death immunity), and Righteousness
  makes a unit proof against chaos and death spells outright (`EffectHost::protected_from`).
  In a battle the damage is dice against the defence (section 8); overland it is still the
  spell's flat amount.
- **Resistance** (`effective_resistance`, `resistance_failure`): a ten-sided die rolling above
  the resistance fails, by the amount it beats it. A spell's `resist_modifier` (negative for a
  spell that is hard to resist) is added first. Magic immunity adds 30 against any realm;
  Righteousness +30 and Bless +3 against chaos and death; Elemental Armor +10, or else Resist
  Elements +3, against chaos and nature; Resist Magic +5 against any realm. They are read from
  the unit's lasting spells by name (`Game::unit_resistance_against`), so Resist Elements and
  Resist Magic have no flat resistance modifier any more.
- **Spells read by name** (Resist Elements, Elemental Armor, Resist Magic, Spell Lock, Bless, Heroism,
  Righteousness) have no rules of their own; cast in a battle, the unit carries them and the checks above find
  them by name. Heroism also raises the unit to the level it gives (`heroism_modifiers`).
- **Not here:** the defence bonuses the same enchantments give against ranged magic (Elemental
  Armor +10, Bless +3, Resist Elements +3, task 156), armour piercing and large shield (36, 37),
  gazes and touches that call a resistance check (36).

- **Weapon quality** (task 18, `unit::WeaponQuality`): set where a unit is trained (an alchemists
  guild gives magic weapons, mithril or adamantium in the city's area raises them, the Alchemy
  retort gives at least magic) and copied into `BattleUnit::weapon_quality`. Mithril adds one and
  adamantium two to melee, thrown and missile attacks and to defence (magic adds nothing to
  these); all three add a plus to hit with those attacks and make the weapons magical.

### 2.2 Special attacks (task 36)

`battle/special.rs` and `attack::process_attack`, following the original's
`Battle_Unit_Process_Attack`. The unit's `Stat::SpecialLevel` (the original's special attack
number, kept positive) is poison strength, or the resistance penalty of its gaze, touch and
life steal. Every attack of the unit (melee, thrown, ranged) carries its special attacks, as in
the original; a gaze only counts in the thrown and breath step.

Per attack, in order:
1. **Gaze** (stoning, death, or both for a multiple gaze), once, against *each of the target's
   figures*: the figure resists at the gaze's penalty (stoning is nature, death is death) or
   is killed. Stoning immunity, death immunity and magic immunity stop the matching gaze. The
   damage of a multiple gaze's ranged strength ignores defence. A defender with a gaze gazes
   back at the attacker after the attacker's thrown step, with the figures it has left
   (`melee_exchange`).
2. **For each attacking figure**, if the target is not magic immune:
   - *Dispel evil*: against creatures of chaos or death (the unit's realm), resist at -4 (-9
     for the undead) or lose a figure.
   - *Stoning touch* and *death touch*: resist at the penalty or lose a figure (stoning
     immunity, death immunity).
   - *Life steal*: resist at the penalty; every point by which it fails is a point of damage no
     defence blocks, and the attacker heals the same (death immunity stops it).
   - *Destruction*: resist (no penalty) or lose a figure.
3. **The blows**, as before. `doom_damage` makes the attack strength the damage, with no to-hit
   or defence roll (the original halves it when it comes from an item or a spell; nothing
   here is not innate yet).
4. **Poison**: one resistance check per point of strength, each failure a point of damage
   no defence blocks (poison immunity; magic immunity does not stop it).

A failed check "kills a figure" by dealing a whole figure's hit points of damage no defence
blocks, so a figure already hurt dies sooner, and the damage that is left spills on to the next.
Armour piercing halves the defence (rounding down) before the immunity floor of 10 is applied;
lightning breath pierces armour too. Healing (`DefenderState::heal`) mends the front figure,
then brings dead figures back, and nothing over full strength is kept.

Not here (task 157): power drain (it needs battle mana: now there, see section 8), the extra penalty and half
strength of touches and doom damage granted by items and spells, hit points above full
strength from life steal, and healing that skips irreversible damage.

Damage is sorted by what dealt it (`Damage::drained` and `gone`, kept per unit in
`BattleUnit::damage_kinds`). A unit that dies mostly of life steal, or of the blows and poison of a
creator of undead (a victim with magic immunity is spared), has fallen `Fate::Drained`: the
winner takes it after the battle as an undead unit, full strength, unless it was a
creature of the Death realm or a hero (`raise_drained`, `battle/game.rs`); this holds for a fight at a lair too, and the winner's standing units count against the stack of nine. If the guards or monsters win, the drained attackers are raised for the shared neutral owner `unit::NEUTRAL` and stay on the site as permanent neutral units: the guards made for the fight go, those units do not (`finish_site_battle` removes only the fight's guards), and a later stack that walks in meets them as an enemy stack. What a destruction
attack or a severing spell (Disintegrate, Dispel Evil, Petrify and the other slaying spells)
kills is `Fate::Gone`. Animate Dead raises one of the casting wizard's own dead (not a hero,
not summoned in the battle, not gone) on a free square of their half of the field, undead for
good.

## 3. The tactical map and the turn (task 40)

- The grid is **21 by 22** squares (the original's size). A square is open, rough (forest,
  swamp, rubble: two moves to enter), or blocked (trees, rocks: walkers cannot enter). Flying
  units pay one move everywhere. Moves are in half-moves like the overland map; a unit with
  any move left may take a last step that costs more.
- **Teleporting and merging** units (`teleport`: unicorns, djinn; `merging`: great wyrm) skip the path
  search: they may go to any free, unblocked square for one move (task 13).
- Movement is in eight directions, cannot enter an occupied square, and is found with a
  cheapest-path search (`BattleMap::reachable`, `Battle::reachable`).
- The map is generated from the overland terrain of the defenders' square (more rough ground
  in forest and swamp, rocks in mountains); the columns around the deployment edges stay open.
  Attackers stand on the west edge, defenders on the east, in blocks centred on the middle.
- **Turns:** the attacker plays all its units, then the defender; a new turn starts when the
  defender ends its turn. Each unit may move and make one attack per turn (a melee attack
  uses up the rest of its moves). `Done` ends a unit's turn, `EndTurn` the side's.
- **A side loses when it has no active unit.** `Flee` hands the battle to the other side
  (the losses of fleeing units are task 42). After **100 turns** the defender wins (the original: 50; divergence 29).
- Casting is in section 8; battlefield spells are section 9. City walls and gates are in
  (section 7).

## 4. Orders and events

| Order (`BattleCommand`) | Rule | Refused when |
|---|---|---|
| `Move { unit, to }` | Walk to a square in `Battle::reachable`; costs its path. | no moves left, unreachable |
| `Attack { unit, target }` | Melee exchange with an adjacent enemy (thrown/breath first); ends the unit's moves. | not adjacent, already attacked |
| `Shoot { unit, target }` | Ranged attack at any distance (`Missile` or `Magic` kind), uses one ammunition. | no ranged attack, out of ammunition, already attacked |
| `Cast { spell, aim, mana }` | The wizard casts a spell (section 8). Only through `Game::battle_command`: `Battle::apply` needs the rules and refuses. | no wizard, already cast this turn, not known, not a battle spell, not in the game yet, too dear, bad target |
| `Done { unit }` | Gives up the rest of the unit's turn. | not the active side's unit |
| `EndTurn` | Hands play to the other side; the turn number rises when the attacker is up again. | |
| `Flee` | All the side's active units leave; the other side wins. | |

Events: `Moved` (with the path), `Attacked` (damage and kills both ways), `Shot`, `UnitDied`,
`TurnStarted`, `Fled`, `Ended`, and for spells `SpellCast`, `SpellCountered`, `SpellDamaged`,
`SpellImmune`, `SpellHealed`, `SpellEnchanted`, `SpellDispelled`, `Summoned`, `CounterMagicRaised`. `Game::battle_command` wraps them as `Event::Battle(..)` and,
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
  step in y moves (-16, +8) (`Combat_Grid_Screen_Coordinates`). The core's x is the original's
  mirrored (`x = 20 - cgx`), so a square is drawn where the original draws the mirrored one and
  the picture is the original's (the city and the defenders at the upper left). Like the
  original it draws only the band of 200 squares it plays on, scaled to fit the window.
- **The field** (`battle/map/layout.rs`, in the original's frame; tasks 110 and 131) is built from
  the square: terrain set, roads on it and its eight neighbours, the city with its houses and
  walls. Rough ground, dirt, roads, trees and rocks are laid as the original lays them; walking
  costs are grass and dirt 2 half-moves, rough 4, road 1, plus 1 for each tree or rock on the
  square (to 4); the corners outside the band and the structure square (an outpost's, a lair's,
  a tower's, a node's, a wizard's fortress) are shut to everyone. The seed comes from the
  square (`x * y * (plane + 5) + 10039`), as in the original, so a fight on one square is
  fought on the same ground.
- **A wizard's fortress** (task 290): the capital's field has the fortress (`cmbtcity` 17) on the
  structure square, which is shut; houses keep off that cell (eleven fill the area from eleven
  citizens, below that as many as the citizens) and the defenders' deployment skips it.
- **Rivers** (task 255): the original has `Carve_River_Terrain` and never calls it. Ours does: a
  battle on plains with a river on the overland square (and its neighbours deciding which way it
  runs) has a river walked across the field. A river square costs a walker 4 half-moves and a
  ship 2; a road over it is a bridge (1); trees, rocks and the city's ground keep off it. The
  files hold only six of the twelve river pictures (`cmbtcity` 103 to 108; 97 to 102 and all of
  Myrror's `chriver` 0 to 11 are empty, which is why the original never shows one), so both
  planes use those six, which are saved as PNG files in `crates/f4x-client/assets/rivers/`
  (`river_<shape>_<frame>.png`, embedded by `river_art.rs`) and committed, the one piece of the
  original's art in the repository, at the project owner's choice (17 October 2026); Myrror's
  (`river_myrror_<shape>_<frame>.png`) were generated once from them, the blue water repainted in
  the colour of Myrror's ocean (`chriver` 12, purple) and the grassy banks in that of its
  ground, and are kept as files too. Deviations: the first kind of river starts at (6, 15) and the second at
  the band's top edge, because the original's start squares lie outside the playable band (or
  end after one step).
- **Water battles** (task 254): a fight on a sea square is all ocean. Walkers cannot enter it,
  ships sail it (2 half-moves a square), swimmers take the cheaper way, fliers cross it. Only
  ships, fliers and swimmers take part; land units aboard sit the fight out, and go down with
  their side if it loses (`ActiveBattle::left_out`). A side with a Floating Island, or a unit
  with Wind Walking, brings its land units: the island's 3 by 3 deck (a road-cost block, drawn
  as grass because the island's own picture, `cmbtcity` 117, is blank in the files) is where they
  stand; the island itself does not fight. The decks are where `Generate_Combat_Map` puts them
  (`island_decks`). Summoning onto the sea is not possible.
- **Deployment order** (`deployment_order`): units without a ranged attack take the front squares,
  shooters the squares behind; with more than five shooters the strongest in melee move forward
  until the front has four (the original picks the wrong list for that search; ours picks the
  shooters, as meant). At a wizard's fortress the defenders also keep off the fortress's square.
- **Nodes** (task 160): a fight in a node's aura gives its realm's creatures, on both sides,
  +2 to melee, ranged (if any), defence and resistance (`Battle::node_aura`); a fight on the node
  itself also dispels every spell of another realm against a strength-50 dispel, unless the
  caster has Node Mastery (`node_site`, `battle_spell_stopped`). A nature node's ground is
  forest, a chaos node's mountains and a sorcery node's grassland, as in the original.
  The side panel names the node and the aura.
- **Art** (`battle_ground.rs`, task 112): ground from `cmbgrass`/`cmbdesrt`/`cmbmount`/`cmbtundr`
  (Myrror: the `c` variants), entries 0 to 47 picked by the field (grass 0-3, dirt 4-7, grass
  edged by dirt 8-31, rough ground edged by its neighbours 32-47), trees 48-52 and rocks 53-57 at
  their offsets in the cell; `cmbtcity` for roads (69-96, five frames), houses (2-16), the city
  base (0) and an outpost; `citywall` for the walls (twelve to a bank, frame 1 is rubble). Units are drawn as their figures (`figures1` to
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
  the next unit that can still act, Enter or E ends the turn, Flee asks twice. The buttons are the original's `compix.lbx` pictures (Spell, Wait, Done, Auto, Info, Flee; Wait picks the next unit); Auto and Info are drawn greyed until auto-play (133) and the info window exist; Spell opens the wizard's spell list (section 8). End turn is a plain button, as the original ends a turn when every unit is done. The panel shows the
  hovered (else selected) unit's melee, ranged, defence, resistance, figures, hit points, moves,
  ammunition and abilities, and the last combat lines.
- **The other side** is played by `battle/ai.rs` after a short delay per order: each unit attacks
  an adjacent enemy, else shoots, else walks to the reachable square nearest the closest enemy,
  else is done. This is a placeholder for the real battle AI (task 46).
- `--battle` together with `--shot FILE` opens a test fight for screenshots.

## 7. Differences from the original and open work

- The cell effect (`cmbtcity` 120) and the wall-rise animation belong to the spell animations (task 159(6)).
- No unit figures, animation, projectiles or combat frame (task 132); the field's art is task 112 (done).
- No experience/fame/loot write-back (42).
- The attacking stack does not move onto the defenders' square after a win; no attack prompt and
  no AI-versus-AI resolution (task 133).

## 7. City sieges (task 41)

Code: `battle/map.rs` (the city and its walls), `battle/mod.rs` (wall attacks, the defence bonus,
city damage), `battle/game.rs` (starting a city battle), `city/conquest.rs` (what a fallen city
costs). The numbers are the original's (`Generate_Combat_Map`, `Deploy_Battle_Units`,
`Update_Move_Map_City_Area_Restrictions`, `Battle_Unit_Attack`, `End_Of_Combat`); the field is
mirrored, so the defenders are at the east and the city stands at x 12 to 15, y 10 to 13.

- **A fight at a city or outpost is a city battle**: the city area is open ground. A city with
  the City Walls building has a wall of twelve sections round the edge of the area; the four
  middle squares are inside. The gate is the section at 12,12, facing the attackers. Outposts
  have no walls.
- **Deployment** is the original's: defenders inside in three columns of four (the gate square
  first), skipping the corner towers when there are walls; attackers outside in three columns.
- **Movement:** an attacker outside cannot enter a standing wall section, except at the gate,
  and nobody can enter a standing corner tower. A unit inside cannot leave except by the gate,
  because the squares just outside the standing walls are shut to it. Fliers, teleporters, the
  merging and the non-corporeal pass over the walls (but not the towers). A fallen section (`Wall::Broken`) is open
  ground again.
- **Wall crushers** (the attackers' units with `wall_crusher`) strike a standing section with
  `AttackWall` from next to it (it falls half the time, and the blow uses up the unit's moves) or
  `ShootWall` with a ranged attack (a quarter of the time, one shot). A wall crusher that hits a unit standing on
  a wall section may bring that section down too, by the same chances.
- **Defence:** a unit in the city, hit by an attacker outside it, has +3 defence on a standing wall
  section and +1 elsewhere in the city. Only a city with walls gives this, and attackers that
  have come inside get none of it.
- **City damage:** at the start of each turn of the battle every attacking unit inside the city area adds one
  (`Battle::city_damage`): a siege that gets into the city and stays there costs the city more when it falls.
- **Taking the city** (`Game::conquer_city`, `city/conquest.rs`): an outpost is destroyed; any other city
  changes hands with the loser losing fame equal to its size (1 to 5), the winner gaining the size less
  two, and the winner looting the loser's gold in proportion to the city's share of their citizens.
  The fighting kills each citizen but one with a chance of 10 percent plus twice the city
  damage (at most 50), and destroys each building nothing else needs with a chance of the city
  damage plus 10 percent (at most 75). The old owner's spells on the city end, production
  starts at trade goods. A fortress city's wizard is banished (`wizard/fortress.rs`).
- **Razing:** until the turn ends the taker may `RazeCity` (console `raze <city>`, the client asks): the size plus one more fame
  lost, a tenth of every building's cost looted, and the city is destroyed. This is the original's
  path when the player presses R on the results scroll. The client asks at once; an AI never razes (no
  decision exists for it yet).
- **Console:** `battle` lists the walls and the city damage, `bwall <unit> <x,y>` and
  `bshootwall <unit> <x,y>` strike a wall, `bvortex <n> <x,y>` moves a Magic Vortex one square, `raze <city>` razes.
- **City enchantments** (task 25, `spell/city.rs`; read when the battle starts, `Game::start_battle`):
  - *Wall of Fire* (`Battle::wall_of_fire`): a walker (not a flier, not a teleporter or merger) that steps
    from outside the city area into it, or strikes into it from outside, takes the original's Fireball of
    its base strength 5 (fire, each figure, stopped by fire and magic immunity). Shots are not affected.
  - *Wall of Darkness* (`Battle::wall_of_darkness`): no ranged attack from outside the city area on a unit
    inside it (`BattleError::WallOfDarkness`), unless the shooter has illusion immunity (the help; the
    original's code only lets True Sight through, B23). The AI does not try.
  - *Flying Fortress* (`BattleMap::with_flying_fortress`): every square of the city area is shut to walkers
    outside, and the squares round it to walkers inside, the gate too; fliers, teleporters, mergers and the
    non-corporeal pass. No wall defence bonus.
  - *Cloud of Shadow*: the city's owner has Darkness up for the whole battle (a field spell of the defender).
  - *Spell Ward* (`Battle::warded_realms`): the attackers cannot cast spells of a warded realm.
  - Walls from *Wall of Stone* are the City Walls building, built overland (the spell is instant).
- **Not here** (tasks 158, 163, 164): Wall of Stone cast in battle, the AI's decision to raze,
  the diplomatic effects of attacking a city, monsters and raiders taking a city, and Heavenly Light's True
  Light.


## 8. Casting in battle (task 43)

The wizard does not stand on the field: each side with a wizard (not the independent cities and
monsters) has a `BattleCaster` (`battle/casting.rs`) and casts through `BattleCommand::Cast`,
which `Game::battle_command` carries out (`battle/spells.rs`) because it needs the spell rules.
It follows the original's `Combat_Cast_Spell` and `Combat_Cast_Apply_Spell_Effect`.

- **Skill and mana.** The wizard has their whole casting skill to spend over the battle (none
  while they are on the way home from banishment, `casting_return`). A spell costs its price
  (`casting_cost(.., in_battle)`: the base cost, not the overland five-fold, less the usual
  reductions) in skill, and the reserve pays that **times the range multiplier**: half at the
  fortress itself, one for one up to five squares from it, then half as much again for each five
  squares more, up to three times (and three times on the other plane or with no fortress). A
  Channeler counts as two squares away anywhere but at the fortress. A spell can be cast when its
  cost is at most `min(skill left, mana x 10 / multiplier)` (`BattleCaster::affordable`). The
  reserve is paid at once (the game's `mana` goes down as the spell is cast).
- **Once a turn.** One spell for each turn of the wizard's side, whatever it did.
- **Which spells.** The ones the wizard knows with a `Battle` context whose effects are in the
  game (`Game::battle_spells`): bolts and other damage (`Effect::Damage`), `Heal`, `Summon`,
  `Dispel`, Counter Magic and the lasting spells on one unit that only change numbers (Giant
  Strength, Stone Skin, Iron Skin, Flame Blade, Holy Weapon, Holy Armor, Lionheart, Haste, ...). The
  rest wait for their effects (task 159); they are not offered and `Cast` refuses them.
- **Extra mana.** Fire, Ice and Lightning Bolt add a point of strength for each extra mana,
  Psionic Blast one for each two and Fireball one for each three (`DamageTraits::mana_per_point`,
  set by the importer); a dispel's strength is the mana put in (three times for the "True"
  spells); Counter Magic's reserve is the mana. The `mana` of the command is the total in the
  spell, from its cost up to what the wizard can afford (`battle_spell_most_mana`); `0` is the
  least. The skill cost is the least cost plus the extra less the realm reductions.
- **Damage spells** (`Battle::spell_damage`, `attack::spell_attack`): the spell attacks with its
  strength in dice (hit on 8 or better, as for a blow) against the target's defence, which its
  realm and immunities change as for any ranged magic. They are not resisted. Magic immunity
  and Righteousness (against chaos and death) stop them outright. The spell data's attributes
  come through the importer: Lightning Bolt and Warp Lightning halve the defence, Doom Bolt ignores it,
  Fireball hits each figure separately (none loses more than one figure; the damage short of a
  kill pools over the unit), Warp Lightning is a chain of bolts one die weaker each (the
  damage pools; the overflow is not blocked again). Star Fires can only be thrown at chaos and
  death creatures.
- **Counter Magic.** The caster's enemy's reserve tries to stop every spell: a dispel of the
  reserve's strength against the mana put in (times the caster's mastery, as in `spell/dispel.rs`);
  the attempt drains the reserve by five whether or not it works, and a stopped spell still
  costs its mana and the wizard's turn.
- **Lasting spells** change the unit's numbers for the battle only (`BattleUnit::apply_lasting`):
  the original keeps them on the battle unit and they are gone afterwards. The unit remembers
  its numbers from before the first one (`base`) and the spells on it (`combat_spells` with
  their casters), so a **dispel** (`Game::dispel_battle_unit`) can take one off: it works on the
  spells cast in the battle and on the lasting spells of the overland unit the fighter was
  copied from (those *are* taken off the overland unit), only those cast by another wizard, and
  then the unit is rebuilt from its base (re-read from the overland unit if that changed) with the
  spells that are left. A unit cannot be given a spell it has.
- **Summons** appear on a free square of the caster's half of the field, at most nine units a
  side, ready to act; they are made like any unit and the overland copy is dropped at once, so
  nothing is left on the map afterwards.
- **A spell that kills the last enemy ends the battle** at once, with the usual results.
- **Console:** `battle` shows each wizard's skill, mana and range cost; `bspells` lists the
  spells that can be cast; `bcast <spell> [unit:N | tile:x,y] [mana:N]` casts; `uspells <unit>` lists what a unit can cast on its own and `ucast <unit> <spell> [aim] [mana:N]` casts it.
- **Client:** the Spell button opens the list (cost, `+` where extra mana helps); picking one
  waits for a target (purple squares), Up/Down or Page Up/Down change the mana, Enter casts a spell
  with no target, Esc cancels. The panel shows the wizard's skill, mana and range cost.

**Not here** (task 159): the spells that need new effects (Magic Vortex, ...),
the battlefield node counter (a node of another realm may stop the spell), the AI wizard's
casting (task 46), and the original's rule that Hit points added by Lionheart are lost with the spell.

### Units casting (task 265)

A battle unit casts on its own with `BattleCommand::UnitCast` (`battle/spells/unit_cast.rs`), not
using the wizard's skill or reserve, and not the wizard's one cast a turn:

- a **hero** casts its four innate spells for the spell's plain cost in its own mana
  (`Game::hero_mana`), and the spell of an **item** for one charge, from any slot; the charges left
  are written back to the item after the battle;
- a **creature caster** (`spellcaster` 20 mana, `spellcaster_major` 40) casts the spells of its realm;
- a unit with `casts_doom_bolt`, `casts_fireball`, `casts_healing` or `casts_web` casts that spell
  once a battle, for no mana (Counter Magic does not stop the Web);
- a unit with Creature Summoning (`summon_demon`, the Demon Lord) calls a demon with
  `BattleCommand::SummonDemon`, three times a battle, for no mana (console `summondemon <unit>`);
- a cast uses up the unit's turn; the items' spell save makes the resistance checks of the unit's
  spell harder; extra mana makes a bolt or a Life Drain stronger, up to the unit's mana.

`Game::battle_unit_spells(unit)` lists them, `legal_unit_casts()` the legal casts for the AI and the
console (`uspells`, `ucast`, `blegal`). Spells of the other kinds added with it: Earth to Mud (a 5 by 5
area of open ground; walkers pay 12 half-moves to enter, fliers and the bodiless do not), Cracks Call
(one square: a walker or crawler there is destroyed one time in four, a wall on it falls), Disrupt
(one wall section), Life Drain and Word of Recall (any of the wizard's units, not only a hero).

## 9. Battlefield enchantments and destroying spells (tasks 44 and part of 159)

**Battlefield enchantments** (`battle/field.rs`): fourteen spells with the `battlefield` hook and
target *Battlefield*; the battle knows what each does by the spell's id (`Field::of`), as the
original's code does. Each side can have each one up once (a second cast is refused,
`SpellError::AlreadyUp`); they last the battle. The stat changes are kept on every unit as a
`FieldBonus` and recomputed by `Battle::refresh_fields` whenever a spell goes up or a unit changes,
so they never pile up and never push a stat below nothing:

| Spell | Effect |
|---|---|
| True Light / Darkness | creatures of life (death) gain 1 to melee (if it has any), ranged (if any), defence and resistance, creatures of death (life) lose 1 of each; the two cancel, whoever cast them |
| Warp Reality | -2 to hit for every unit but chaos creatures, both sides |
| Black Prayer | the caster's enemies: -1 melee, ranged and defence, -2 resistance |
| Prayer | the caster's units: +1 to hit, +1 to block, +1 resistance |
| High Prayer | instead of Prayer: +1 to hit, +1 to block, +3 resistance, +2 melee (if it has any), +2 defence (the code; the help said more) |
| Holy Bonus (a unit ability: Angel, Arch Angel, High Men Paladins; level `SpecialLevel`) | the unit's side, from the highest level among its standing units (never added up): +level melee (if it has any), defence and resistance, the same bonus also counting as Resistance to All. Only the attack of melee, as in the original (the help says "attack strength") |
| Resistance to All (Unicorns, Guardian Spirit) | the unit's side: +level resistance, highest level only, on top of any Holy Bonus. Both are recomputed at each side's turn start and when a creature is summoned, so a fallen bearer's bonus goes at the next turn |
| Metal Fires | the caster's units that are not fantastic and have no Flame Blade: +1 melee (if any), +1 missile and thrown attack, magical weapons |
| Mass Invisibility | the caster's units are invisible: attackers not immune to illusions are -1 to hit them (also the `invisibility` ability, the Invisibility spell: its game name is `Invisiblity`) |
| Blur | each point of an enemy attack's hits against the caster's units misses one time in ten, from attackers that do not see through illusions (the original checks the *defender's* immunity, an OGBUG) |
| Terror | at the start of each of their turns, each of the caster's enemies must pass a resistance check at +1 against death or loses its turn (it can still strike back) |
| Entangle | each of the caster's enemies that walks and has a body (not fliers, not the non-corporeal) loses one whole move at the start of its turn (the help is right; the original's code takes it from every unit, treated as a bug) |
| Mana Leak | at the start of each turn the caster's enemy wizard loses 5 mana (the game's reserve is kept in step, `sync_battle_mana`), and each enemy unit with spell shots loses one |
| Call Lightning | at the start of each turn three to five bolts of strength 8 (armour piercing, stopped by magic immunity) at random enemy units |
| Wrack | at the start of each turn each figure of each enemy unit without death immunity (or Wraith Form) must pass a resistance check at +1 against death or the unit loses a hit point |

The turn's order is the original's: Mana Leak, Call Lightning (attacker's, then defender's), Wrack,
then each side's Terror and Entangle as its own turn begins.

**Instant spells on the whole field**: Flame Strike (strength 15 fire, area, on every enemy
unit), Mass Healing (five hit points to every unit of the caster's that is not a death creature),
Death Spell (each figure of each enemy unit without death immunity resists at -2 or dies), Holy
Word (the same on fantastic creatures; its resistance check is against life, the spell's own
realm, where the original's code uses death, an OGBUG; the undead resist at an extra -5, as the help says: the original has it reversed) and Call Chaos (each
enemy unit gets one of: nothing, 5 healing, Chaos Channels, Warp Creature, a strength-15 Fire Bolt,
Warp Lightning, Doom Bolt, Disintegrate; the two spells not in the game yet do nothing).

**Destroying and cursing one unit** (`Effect::Slay`, `SpellDef::save`): Word of Death (-5), Petrify
(stoning immunity stops it), Dispel Evil (-4, on chaos and death creatures only, undead -9),
Banish (-3, fantastic creatures only, each 15 more mana one point lower), Disintegrate (the unit is
destroyed if its resistance is under 10), and the curses Weakness (-2 melee and ranged; a resistance
check at -2), Vertigo (-2 to hit, -1 defence; a check at 0) and Mind Storm (-5 to everything, no
check, but not on units immune to illusions). A unit protected by Spell Lock is spared the
mass spells that destroy.

Console: `battle` lists the field enchantments' effects through the units' numbers; the spell events
are described. The client's panel names the enchantments over the field.

Dispel Magic (and True) also tries each battlefield enchantment of the caster's enemy
(`Game::dispel_battle_fields`; task 161), at the spell's difficulty with the masteries of its caster.
Counter Magic cannot be dispelled, and the dispel still aims at a unit.

**Not here** (task 159): Disenchant Area and True in battle, the node auras and counter, Wall of Stone in battle (tasks 158 and 163),
true sight (it cancels invisibility), the Mass Invisibility rule that an invisible unit next to an
enemy is seen, Entangle's exemption for fliers in the help text.
