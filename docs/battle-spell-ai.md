# Battle spells and the battle AI

Task 600. Step 1 and 2 (14 November 2026): every spell with a Battle context in `spells.ron`, and how the scripted battle AI (`f4x-ai/src/battle/cast.rs`, `judge`) values it today. The list is read from the code; step 5 adds a test that keeps it true.

92 spells: 19 covered, 44 partial (a flat share, no spell-specific idea), 29 none (value 0: the AI never casts them).

| Num | Spell | Realm | Cost | Target | AI | How it is valued now |
|---|---|---|---|---|---|---|
| 1 | earth_to_mud | Nature | 15 | Tile | none | battle effect "earth_to_mud": value 0, never cast |
| 2 | resist_elements | Nature | 5 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 3 | wall_of_stone | Nature | 50 | OwnCity | none | wall stone: value 0, never cast |
| 4 | giant_strength | Nature | 8 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 5 | web | Nature | 10 | EnemyUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 7 | stone_skin | Nature | 10 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 11 | cracks_call | Nature | 20 | Tile | none | battle effect "cracks_call": value 0, never cast |
| 13 | ice_bolt | Nature | 10 | EnemyUnit | covered | damage by the game's own odds, kill first |
| 21 | elemental_armor | Nature | 35 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 22 | petrify | Nature | 35 | AnyUnit | partial | slay: flat 35% of worth of each non-immune enemy; ignores its save, the realm limit and the spell's odds |
| 24 | iron_skin | Nature | 40 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 30 | earth_elemental | Nature | 60 | Tile | covered | summon: 0.75 x strength of the creature |
| 31 | regeneration | Nature | 60 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 33 | entangle | Nature | 50 | Battlefield | none | battlefield entangle: value 0, never cast |
| 35 | call_lightning | Nature | 60 | Battlefield | none | battlefield call_lightning: value 0, never cast |
| 41 | resist_magic | Sorcery | 5 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 42 | dispel_magic_true | Sorcery | 10 | AnyUnit | none | counter_effect dispel", "30: value 0, never cast |
| 44 | guardian_wind | Sorcery | 10 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 45 | phantom_warriors | Sorcery | 10 | Tile | covered | summon: 0.75 x strength of the creature |
| 46 | confusion | Sorcery | 15 | EnemyUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 47 | word_of_recall | Sorcery | 20 | OwnUnit | none | recall anyone: value 0, never cast |
| 48 | counter_magic | Sorcery | 10 | None | none | counter_effect counter_magic: value 0, never cast |
| 50 | psionic_blast | Sorcery | 10 | EnemyUnit | covered | damage by the game's own odds, kill first |
| 51 | blur | Sorcery | 25 | Battlefield | none | battlefield blur: value 0, never cast |
| 53 | vertigo | Sorcery | 25 | EnemyUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 54 | spell_lock | Sorcery | 20 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 56 | flight | Sorcery | 25 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 60 | phantom_beast | Sorcery | 35 | Tile | covered | summon: 0.75 x strength of the creature |
| 62 | invisibility | Sorcery | 35 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 64 | banish | Sorcery | 20 | AnyUnit | partial | slay: flat 35% of worth of each non-immune enemy; ignores its save, the realm limit and the spell's odds |
| 66 | air_elemental | Sorcery | 50 | Tile | covered | summon: 0.75 x strength of the creature |
| 67 | mind_storm | Sorcery | 35 | EnemyUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 69 | magic_immunity | Sorcery | 50 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 70 | haste | Sorcery | 50 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 73 | creature_binding | Sorcery | 70 | EnemyUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 74 | mass_invisibility | Sorcery | 80 | Battlefield | none | battlefield mass_invisibility: value 0, never cast |
| 81 | warp_wood | Chaos | 10 | EnemyUnit | covered | warp_wood: own rule |
| 82 | disrupt | Chaos | 15 | Tile | none | battle effect "disrupt": value 0, never cast |
| 83 | fire_bolt | Chaos | 5 | EnemyUnit | covered | damage by the game's own odds, kill first |
| 86 | eldritch_weapon | Chaos | 15 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 87 | wall_of_fire | Chaos | 30 | OwnCity | none | wall fire: value 0, never cast |
| 88 | shatter | Chaos | 12 | EnemyUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 89 | warp_creature | Chaos | 18 | EnemyUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 90 | fire_elemental | Chaos | 20 | Tile | covered | summon: 0.75 x strength of the creature |
| 91 | lightning_bolt | Chaos | 10 | EnemyUnit | covered | damage by the game's own odds, kill first |
| 93 | chaos_channels | Chaos | 50 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 94 | flame_blade | Chaos | 25 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 96 | fireball | Chaos | 15 | EnemyUnit | covered | damage by the game's own odds, kill first |
| 99 | immolation | Chaos | 30 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 101 | warp_lightning | Chaos | 35 | EnemyUnit | covered | damage by the game's own odds, kill first |
| 102 | metal_fires | Chaos | 40 | Battlefield | none | battlefield metal_fires: value 0, never cast |
| 104 | doom_bolt | Chaos | 40 | EnemyUnit | covered | damage by the game's own odds, kill first |
| 105 | magic_vortex | Chaos | 50 | Tile | covered | vortex: own rule |
| 108 | warp_reality | Chaos | 50 | Battlefield | none | battlefield warp_reality: value 0, never cast |
| 109 | flame_strike | Chaos | 60 | Battlefield | covered | damage by the game's own odds, kill first |
| 112 | disintegrate | Chaos | 50 | AnyUnit | partial | slay: flat 35% of worth of each non-immune enemy; ignores its save, the realm limit and the spell's odds |
| 115 | call_chaos | Chaos | 75 | Battlefield | covered | call_chaos: own rule |
| 121 | bless | Life | 5 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 122 | star_fires | Life | 5 | EnemyUnit | covered | damage by the game's own odds, kill first |
| 124 | holy_weapon | Life | 10 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 125 | healing | Life | 15 | OwnUnit | covered | heal: share of hit points restored |
| 126 | holy_armor | Life | 18 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 128 | true_light | Life | 20 | Battlefield | none | battlefield true_light: value 0, never cast |
| 130 | heroism | Life | 20 | OwnUnit | none | heroism: value 0, never cast |
| 131 | true_sight | Life | 20 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 134 | dispel_evil | Life | 25 | AnyUnit | partial | slay: flat 35% of worth of each non-immune enemy; ignores its save, the realm limit and the spell's odds |
| 137 | raise_dead | Life | 35 | AnyUnit | none | battle effect "raise": value 0, never cast |
| 140 | prayer | Life | 30 | Battlefield | none | battlefield prayer: value 0, never cast |
| 141 | lionheart | Life | 40 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 143 | invulnerability | Life | 40 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 144 | righteousness | Life | 40 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 149 | mass_healing | Life | 50 | Battlefield | covered | heal: share of hit points restored |
| 150 | holy_word | Life | 60 | Battlefield | partial | slay: flat 35% of worth of each non-immune enemy; ignores its save, the realm limit and the spell's odds |
| 151 | high_prayer | Life | 60 | Battlefield | none | battlefield high_prayer: value 0, never cast |
| 162 | weakness | Death | 5 | EnemyUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 164 | cloak_of_fear | Death | 12 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 165 | black_sleep | Death | 15 | EnemyUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 167 | life_drain | Death | 10 | EnemyUnit | none | battle effect "life_drain": value 0, never cast |
| 168 | terror | Death | 20 | Battlefield | none | battlefield terror: value 0, never cast |
| 169 | darkness | Death | 25 | Battlefield | none | battlefield darkness: value 0, never cast |
| 170 | mana_leak | Death | 20 | Battlefield | none | battlefield mana_leak: value 0, never cast |
| 172 | possession | Death | 30 | EnemyUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 174 | black_prayer | Death | 35 | Battlefield | none | battlefield black_prayer: value 0, never cast |
| 178 | wall_of_darkness | Death | 40 | OwnCity | none | wall darkness: value 0, never cast |
| 179 | berserk | Death | 30 | OwnUnit | partial | lasting unit spell: flat 25% of the target's worth; no notion of what it does |
| 181 | wraith_form | Death | 30 | OwnUnit | partial | stat change or ability: flat 25% of worth, by the sign of the change; an ability on an enemy counts 0 |
| 182 | wrack | Death | 40 | Battlefield | none | battlefield wrack: value 0, never cast |
| 192 | word_of_death | Death | 40 | AnyUnit | partial | slay: flat 35% of worth of each non-immune enemy; ignores its save, the realm limit and the spell's odds |
| 194 | death_spell | Death | 50 | Battlefield | partial | slay: flat 35% of worth of each non-immune enemy; ignores its save, the realm limit and the spell's odds |
| 195 | animate_dead | Death | 50 | AnyUnit | none | battle effect "animate": value 0, never cast |
| 202 | dispel_magic | Arcane | 10 | AnyUnit | none | counter_effect dispel", "10: value 0, never cast |
| 205 | recall_hero | Arcane | 20 | AnyUnit | none | recall hero: value 0, never cast |

## After step 4 (14 November 2026)

The table above is how the AI valued each spell before the review. Now (`f4x-ai/src/battle/cast.rs`, `docs/ai-design.md` 6.5):

- **Valued for the first time:** Earth to Mud, Cracks Call, Disrupt, the 14 battlefield spells, the 3 walls, Counter Magic, both dispels, both recalls, Heroism, Raise Dead, Animate Dead and Life Drain.
- **Refined:** slays (the odds that the enemy fails its save, and the creatures the spell can destroy), curses with a save (the odds they land), Shatter (a cap is a curse), holds (Web, Confusion, Black Sleep: half the unit) and takeovers (Possession, Creature Binding: 1.5 times the unit, as the enemy loses it and we gain it).
- **The wizard has no worth floor**: it casts the best cast it can afford each turn (bounds: the casting skill, the reserve minus the upkeep, and the budget of task 601).
- **Kept true by tests:** `every_battle_spell_has_a_value_or_waits_for_what_it_needs` lists the spells that have no worth in a plain duel and why; a spell that gains or loses its worth fails it. The states that make the others worth something (a hurt unit, an enchanted enemy, a nearly dead unit, a shooter) are not all tested yet.
