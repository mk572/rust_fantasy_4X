# Mechanics described in the original help that are not implemented yet

Source: the original game's help text (807 topics, imported to `content-local/help.ron`;
numbers in brackets are topic indices there). Written in our own words. "Engine" means
`f4x-core`; what exists today is the stats/modifier maths, the effect executor
(`rules::exec`, against a host trait with no real world implementation yet), definitions
and validation, map generation, city founding and a turn counter. Everything below is
therefore missing unless marked otherwise.

## Status
Done since this list was written (each with tests; see `crates/f4x-core`):
- **Units and the map (step 4):** unit instances and stats through the modifier engine,
  movement types and costs (walking, flying, swimming, sailing, forester, mountaineer,
  pathfinding, non-corporeal, wind walking), roads, stacks of nine, plane shifting by tower
  or planar travel, A* routes, standing go-to orders, fog of war and sight.
  Not done: corruption, node melding, mineral transmutation.
- **City economy (step 5):** population with farmers, workers and rebels; food with the
  farmland cap and starvation; production, gold (taxes, trade, coast, mines, markets),
  mana, research; unrest from tax rate, racial table and troops; growth; the worked area
  with shared tiles; production queue, buying, selling, replaced buildings; units trained
  at guild experience; the treasury (food sold for gold, upkeep, disbanding the unpaid);
  healing; outposts and settlers. Not done: roads built by engineers, sold-building
  prerequisites beyond one level, weapon quality (mithril/adamantium).
- **Client:** units, selection, orders, fog, treasury, city screen with production menu.

Still open: wizards and the spell system (step 6), combat (step 7, deliberately paused),
diplomacy, AI, random events, victory and score.

## Corrections already made to imported data
- Unit levels: normal units reach Elite at 120 xp; Ultra-Elite and Champion need the
  Warlord ability or the Crusade spell, not xp [527-532]. Heroes top out at Demi-god at
  1000 xp, not 800 [533-541].
- Racial growth is a flat people-per-turn figure (a city figure is 1000 people), not a
  percentage [279-292]; added `Stat::CityGrowth`. Granary/Farmer's Market add food *and*
  growth [742-743].
- Religious buildings give mana as well as reducing unrest; the Wizard's Guild costs 3
  mana per turn; the Mechanician's Guild adds 50% production [735-746].
- Several spell bonuses were off (Resist Magic +3, Holy Armor +2, Endurance +1 move...).
- One real conflict inside the original: University upkeep is 3 in the data, 2 in the help.
- Race build bans, spell costs and the other building upkeeps match the help text.

## 1. City economy and population (framework step 5)
- Population in thousands, growth per turn from race, buildings and food surplus; people
  split into farmers, workers and rebels [295-297, 308-349, 776].
- Per-race output: farmers make 2 food (Halflings 3) and half a production; workers make
  2 production (Dwarves and Klackon 3); rebels produce nothing and pay no tax.
- Tax rate per citizen and the unrest percentage it causes; unrest from other sources;
  Dwarves double tax revenue; Nomads get a trade bonus; Klackon reduce unrest if their
  capital race [763, 279-292].
- Some races (Beastmen, Draconians, High Elves) yield only half power per citizen.
- Food: units consume food, starvation kills units [234, 295]. Gold maintenance per
  building and unit [298, 496].
- Production, buying an item for gold in one turn, selling buildings, the two non-building
  options (trade goods: 2 production to 1 gold; housing: faster growth) [305, 714-715].
- Building prerequisites and what each unlocks; unit cost reduced by coal and iron
  mines [381]; the Animist's Guild adds food per farmer, not per city [723].
- Nightshade: each plant adds 100 dispel strength against bad city enchantments [766].
- Cities share contested map squares, each getting half [302]. Outposts: need distance
  from other cities (four squares apart; `min_city_distance` is now 4),
  grow by tenths of a citizen into hamlets [400, 504].

## 2. Map, movement and exploration (step 4 and after)
- Units and stacks, movement points, terrain move costs, roads (built by engineers,
  can be enchanted), movement types: walking, forester, mountaineer, pathfinding,
  swimming, sailing, flying, wind walking, non-corporeal (half cost anywhere), planar
  travel, plane shift, teleporting and merging [253-261, 545-551, 565, 575].
- Gates (`gates.rs`, task 12): Earth Gate links two own cities with gates on the same plane for
  one move (`Command::EarthGate`); Astral Gate lets a stack in the city change plane, unless a
  city lies on the other side; towers link planes; Planar Seal closes them [750, 752].
- Fog of war: unexplored squares, sight radius (scouting), Oracle sees 4 squares, walls 3,
  Nature's Eye 4 [546, 731, 748].
- Magic nodes: melding a spirit to channel power, node strength per square (setting
  0.5 / 1 / 1.5, doubled on Myrror), node auras (+2 attack/defence to matching creatures),
  warping; corruption of land and purification (3 turns) [227-228, 653-656, 615-617].
- Minerals, terrain-changing spells, volcanoes, land corruption by units [404].
- World generation settings (done, task 10): land size 15/20/30% (ReMoM's table; the help text's figures were not checked), number of opponents 1-4 [648-652].

## 3. Units and combat (step 7)
- Attack order: thrown, breath, gaze, then first strike, then melee; to-hit is 30% base,
  each plus is +10% [430, 443-448, 543].
- Defence and resistance rolls; armor piercing halves defence, illusion ignores armour,
  chaos damage ignores everything, life steal turns victims undead, poison per level,
  stoning, cause death, destruction, power drain, dispel evil [429-439].
- All immunities with their specific rules (weapon immunity: 10 defence vs magic; magic
  immunity: 50; missiles; fire; cold; poison; stoning; death; illusion) [412-420].
- Ability rules: large shield +2 vs ranged, wall crusher 50%, healer 25% after combat,
  invisibility, long range, regeneration (1/turn, full after combat, revives if won),
  luck +1 to every roll, negate first strike, create/summon undead and demons, planar
  travel, undead rules (no healing, +50% upkeep for fantastic undead) [396-427, 552-553].
- Ranged ammunition (quiver refills after combat), magical ranged "spell shots", mana
  leak [542, 491].
- Experience, level effects, Warlord/Crusade, Armsmaster [515-541].
- Heroes: 13 hero abilities with levels and "super" versions; item slots by hero type
  (sword/bow/staff/armor/misc); item powers, charges, anvil to scrap items, hiring,
  noble/charmed/lucky traits, resurrection, recall [515-526, 577-588, 240, 246].
- Tactical battle: combat map, walls and wall sections, flee (50% of units lost, 25% for
  heroes), auto-resolve, strategic-combat-only option, summoned creatures vanishing after
  combat, mana "range" multiplier by distance from the wizard's tower, casting skill as
  the per-combat spend limit [593-597, 685].
- Transport: ships by building tier [725-727].

## 4. Magic and wizards (step 6)

**Done in the first pass** (see [`magic.md`](magic.md)): books, retorts (some effects), power split,
research, casting cost and skill, overland casting, mana upkeep, alchemy. The list below is what is
still open.

- Wizard entity: picks per realm, special abilities (Alchemy, Warlord, Channeler, Archmage,
  Artificer, Conjurer, Sage Master, Myrran, Divine/Infernal Power, Famous, Runemaster,
  Charismatic, realm masteries, Mana Focusing, Node Mastery) [693-710], the ten named
  wizards' loadouts [659-672].
- Power base split into mana, research and casting skill ratios [264-267, 275];
  mana reserve pays all upkeep, and when it runs dry enchantments fade and summoned
  creatures vanish [266]; alchemy converts gold and mana (1:1 with the ability, else 2:1) [693].
- Casting: overland vs combat lists, cost ranges ("5/25 mana" = base/maximum), cancel loses
  mana spent, research, learning, spell books ordered by rarity [759, 217].
- Dispel mathematics (strength vs mana spent, node protection, Archmage/mastery doubling),
  Disenchant, Disjunction, Spell Blast, Counter Magic, Spell Binding [610].
- (Done: fortress, summoning circle, banishment, Spell of Return; see `magic.md` section 12.)
  Spell of Mastery as a victory condition remains open,
  Spell of Mastery as a victory condition [749, 753, 804].
- Item creation (Enchant Item, Create Artifact) [713 and spell text].

### Spells still on the `unimplemented` placeholder: 134 of 214
| Count | Why |
|---|---|
| 37 | "Special" spells: map reveals, terrain/mineral changes, healing a map square, recalls, moving the fortress, resurrect... |
| 24 | Global enchantments (need wizard-level state: cost multipliers, dispel shields, node power, diplomacy) |
| 20 | Unit enchantments needing non-stat effects (movement types, immunities, conditional bonuses, regeneration...) |
| 19 | Battlefield effects (combat-wide modifiers by realm, per-turn bolts, mud, entangle, counter magic) |
| 9 | City enchantments needing non-stat effects (walls, gates, scouting, spell ward...) |
| 20 | Wizard-targeting, destruction, resistable-kill, disenchant and other special categories |

Missing effect primitives this implies: conditional modifiers (by attacker realm, ranged vs
melee, creature realm/undead/fantastic), per-figure resistance rolls with penalties and
bounded random counts, kill/banish/disintegrate, change allegiance for one combat,
random-effect tables, building destruction, map reveal, teleport/move unit or fortress,
grant movement type, wizard-state changes (mana, skill, fame, diplomacy), and spell
ownership transfer.

## 5. Diplomacy, AI and meta (step 8 and later)
- Relations on an 11-step scale; pacts, alliances (with help obligations), war; wizard
  personalities (maniacal ... peaceful) and objectives (pragmatist, militarist, theurgist,
  perfectionist, expansionist) [271, 782-794].
- Random events: plague, population boom, conjunctions, moons, mana short; each ends with a
  5% chance per turn [768-775]. Option to switch them off [683].
- Fame: sources and effects on upkeep and hero availability [765].
- Difficulty levels and the end-of-game score with its multipliers [642-647, 799-805].
- Advisors, hall of fame, settings, save/load, undo of options [216, 221, 628-641, 675-692, 798].

## Suggested order
1. Step 4: units on the map with movement types and stacks (pulls in sections 2 and part of 3).
2. Step 5: the population/economy model (section 1); the races already carry growth and
   building bans.
3. Step 6: a wizard entity with the power split and a real `EffectHost` over `hecs`; then
   add primitives from the list above in order of how many spells each unlocks.
4. Step 7: combat resolution using the attack order and immunity rules above.
