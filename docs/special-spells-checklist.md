# Checklist: the 50 `Effect::Special` spells and the enchantment lookups (task 482)

Target (decided by the user, 7 November 2026): spells use the default effect engine as far as they can;
what they cannot do goes through a hook, a tag (with arguments) that is part of the spell's definition,
and several spells may carry the same hook. `Effect::Special` and `hook::SPECIAL_SPELLS` go away. The
hook syntax is task 486 (`Custom(name: "wall", args: ["stone"])`), which comes first.

Method per spell: read its code; decide primitive or hook; write the definition in `content/spells.ron`;
move the Rust from the number match to the hook's handler; change every other number match for the spell
(AI, client, diplomacy) to a tag lookup; run the tests; tick the box. The grouping below is a first
guess from the spell names and the number-match sites, not a decision: a group's tags and arguments are
settled when its first spell is done.

Number-match sites to clear (a noisy `grep`, so check each): core `spell/cast.rs`, `battle/spell.rs`,
`spell/hero.rs`, `spell/special.rs`, `spell/terraform.rs`, `spell/dispel.rs`, `battle/mod.rs`,
`wizard/fortress.rs`, `diplomacy/mod.rs`, `game/endgame.rs`; AI `wizardry.rs`, `race.rs`, `diplomacy.rs`,
`battle/cast.rs`; client `city_screen.rs`, `summon_scene.rs`, `unit_sheet.rs`, `view.rs`,
`battle_screen.rs`; `f4x-console`, `f4x-learn`.

Finish: remove `Effect::Special`, `SPECIAL_SPELLS`, the `Special` arms; `hook::problems` checks every
tag against the code and every tag the code knows is used by a spell.

## A. Own files: the large game effects

Hook with its own `.rs` file; any spell may carry it. The AI race logic (`f4x-ai/src/race.rs`, `wizardry.rs`), `diplomacy/mod.rs` and `game/endgame.rs` ask by tag.

Proposed hook or primitive: **spell_of_mastery**

- [ ] 213 `spell_of_mastery`

Proposed hook or primitive: **spell_of_return**

- [ ] 214 `spell_of_return`

## B. City effects

Joins the walls of fire and darkness (spells 87 and 178), which task 486 already moved to `Custom(name: "wall", args: ["fire"])` and `["darkness"]`. The city screen's `num == 72` becomes a tag lookup.

Proposed hook or primitive: **wall (args stone, fire, darkness)**

- [x] 3 `wall_of_stone` (done 7 November 2026: `Custom(name: "wall", args: ["stone"])`; `SpellDef::has_hook_arg`)

Proposed hook or primitive: **spell_ward**

- [x] 72 `spell_ward` (done 7 November 2026: `CityEffect::SpellWard`)

## C. Moving things

Recall is shared by battle and overland.

Proposed hook or primitive: **recall (args anyone, hero)**

- [x] 47 `word_of_recall`
- [x] 205 `recall_hero`

Proposed hook or primitive: **move_building (args fortress, summoning_circle)**

- [x] 28 `move_fortress`
- [x] 203 `summoning_circle`

Proposed hook or primitive: **plane_shift**

- [x] 132 `plane_shift`

## D. Heroes and the dead

The client's summon scene matches 208 and 212 by number.

Proposed hook or primitive: **raise (args to decide)**

- [ ] 133 `resurrection`
- [x] 137 `raise_dead`
- [x] 195 `animate_dead`

Proposed hook or primitive: **summon_hero / summon_champion / incarnation (to decide: one tag with args or three)**

- [ ] 142 `incarnation`
- [ ] 208 `summon_hero`
- [ ] 212 `summon_champion`

## E. Spells against spells

`Effect::Dispel{strength}` may cover Disjunction and Disjunction True; check first.

Proposed hook or primitive: **dispel (args to decide: factor, steal)**

- [ ] 210 `disjunction`
- [ ] 61 `disjunction_true`
- [ ] 76 `spell_binding`

Proposed hook or primitive: **spell_blast**

- [x] 58 `spell_blast`

Proposed hook or primitive: **counter_magic**

- [x] 48 `counter_magic`

## F. Map tiles

Done 7 November 2026 (the user asked for a terrain effect category): one hook, `Custom(name: "terrain_effect", args: ["corruption"])`, with `TerrainEffect` in `spell/terraform.rs`. The cast goes through `Host::custom` (which now gets the hook's arguments) and `GameHost::terrain_effect`; the target checks in `check_target` ask the effect, not the number. Not turned into primitives: floating island stays a hook until task 411 makes it a `Summon`.

- [x] 15 `change_terrain`
- [x] 98 `raise_volcano`
- [x] 85 `corruption`
- [x] 55 `enchant_road`
- [x] 186 `warp_node`
- [x] 18 `transmute`
- [x] 10 `earth_lore`
- [x] 43 `floating_island`

## G. Against wizards and everything alive

Great Unsummoning and Death Wish.

Proposed hook or primitive: **wizard_drain (args power, skill) or a new primitive**

- [x] 171 `drain_power`
- [x] 191 `cruel_unminding`

Proposed hook or primitive: **global kill: extend `Slay` to the whole map, or a hook**

- [ ] 75 `great_unsummoning`
- [ ] 199 `death_wish`

## H. Changes to one unit

Least certain group: Black Wind and Nature's Cures may be `Slay` and `Heal` on a stack; Possession, Creature Binding and Subversion change ownership.

Proposed hook or primitive: **unit_change (args to decide)**

- [ ] 173 `lycanthropy`
- [ ] 93 `chaos_channels`
- [ ] 187 `black_wind`
- [ ] 68 `stasis`
- [ ] 89 `warp_creature`
- [ ] 172 `possession`
- [ ] 73 `creature_binding`
- [x] 177 `subversion`
- [ ] 19 `natures_cures`

## I. Battle field effects

Partly done 7 November 2026: the instant battle spells became one hook, `Custom(name: "battle_effect", args: [...])`, with `BattleEffect` in `battle/spell.rs` (counter_magic, call_chaos, animate, raise, warp_wood, earth_to_mud, cracks_call, vortex, disrupt, life_drain); not `battlefield`, which is for lasting enchantments. Left for their groups because overland code also matches their number: wall_of_stone, recall, chaos_channels, warp_creature. Earthquake and call_the_void are overland. `f4x-learn` `NOT_TRAINED` matches `Effect::Special` and its test `wizards_of_one_or_two_realms_fit_the_spell_slots` fails until the harness is reworked (user: skip for now).

Joins the 14 spells that already use `battlefield`.

Proposed hook or primitive: **battlefield**. Task 486's follow-up already gave the 14 existing spells an argument (`Custom(name: "battlefield", args: ["true_light"])`, the names in `Field::NAMES`); a new field effect adds a `Field` variant and its name.

- [x] 1 `earth_to_mud`
- [x] 11 `cracks_call`
- [x] 81 `warp_wood`
- [x] 82 `disrupt`
- [x] 115 `call_chaos`
- [x] 105 `magic_vortex`
- [x] 167 `life_drain`
- [ ] 26 `earthquake`
- [ ] 119 `call_the_void`

## J. Enchantments read by spell number (not Special spells)

Added 7 November 2026, decided by the user. Not one of the 50 `Special` spells: the spells below already use the default engine or a bare marker tag, but the engine finds what they do by their number (about 195 `SpellId::new(<number>)` literals in non-test code; not yet sorted by scope). Each scope gets a hook with arguments, as `battlefield` and `gate` did: an enum with a `NAMES` table in core, `hook::problems` checks the arguments, the lookup asks for the effect (`has_global(player, GlobalEffect::Crusade)`), and every number literal for that scope goes. A spell may list several arguments. Unit enchantments that are only stat changes or granted abilities stay data and need no tag; a "spell with no rules" read by name may be better as a primitive, decided per spell.

- [x] J1 `global_effect` (done 7 November 2026: `GlobalEffect` in `spell/global_enchantment.rs`, `has_global_effect` / `any_global_effect` / `held_global_effect`; a hook may list several arguments (`Hook.several`); the 24 spells: nature_awareness, herb_mastery, natures_wrath, wind_mastery, aura_of_majesty, suppress_magic, time_stop, meteor_storm, great_wasting, chaos_surge, doom_mastery, armageddon, just_cause, planar_seal, holy_arms, life_force, tranquility, crusade, charm_of_life, zombie_mastery, eternal_night, evil_omens, detect_magic, awareness); about 30 `has_global` sites in core and client
- [x] J2 `city_effect` (done 7 November 2026): `city_flag` renamed and given arguments; `CityEffect` (12 effects: gaias_blessing, stream_of_life, pestilence, famine, chaos_rift, evil_presence, consecration, natures_eye, dark_rituals, heavenly_light, cloud_of_shadow, flying_fortress) in `spell/city.rs`, `city_has_effect`, `held_city_effect`; the walls stay on the `wall` hook (`city_has_wall`). Left: Spell Ward (72), a `Special` spell, in group B; the client's cityscape art keyed on spell numbers (`city_screen.rs`, `outpost_screen.rs`) is J4
- [ ] J3 `unit_effect`: heroism, Bless, Righteousness, the resistance spells, Spell Lock and the like (`battle/game.rs`, `battle/spell.rs`, `battle/attack.rs`, `unit/mod.rs`); decide per spell tag or primitive
- [ ] J4 the rest of the `SpellId::new(<number>)` literals (`cast.rs`, `crafting.rs`, `hero.rs`, `dispel.rs`, `settings.rs` and others), sorted by what they are

## Last

- [ ] `Effect::Special`, `SPECIAL_SPELLS` and their arms removed; reverse check in `hook::problems`
- [ ] no `num ==` match for a spell left outside tests (a `grep` over the crates)
