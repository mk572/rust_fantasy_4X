# Checklist: the 50 `Effect::Special` spells (task 482)

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

- [ ] 3 `wall_of_stone`

Proposed hook or primitive: **spell_ward**

- [ ] 72 `spell_ward`

## C. Moving things

Recall is shared by battle and overland.

Proposed hook or primitive: **recall (args anyone, hero)**

- [ ] 47 `word_of_recall`
- [ ] 205 `recall_hero`

Proposed hook or primitive: **move_building (args fortress, summoning_circle)**

- [ ] 28 `move_fortress`
- [ ] 203 `summoning_circle`

Proposed hook or primitive: **plane_shift**

- [ ] 132 `plane_shift`

## D. Heroes and the dead

The client's summon scene matches 208 and 212 by number.

Proposed hook or primitive: **raise (args to decide)**

- [ ] 133 `resurrection`
- [ ] 137 `raise_dead`
- [ ] 195 `animate_dead`

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

- [ ] 58 `spell_blast`

Proposed hook or primitive: **counter_magic**

- [ ] 48 `counter_magic`

## F. Map tiles

Check which fit `ChangeTerrain` / `SetFeature` / a new reveal primitive before making hooks.

Proposed hook or primitive: **terrain (args to decide)**

- [ ] 15 `change_terrain`
- [ ] 98 `raise_volcano`
- [ ] 85 `corruption`
- [ ] 55 `enchant_road`
- [ ] 186 `warp_node`
- [ ] 18 `transmute`
- [ ] 10 `earth_lore`
- [ ] 43 `floating_island`

## G. Against wizards and everything alive

Great Unsummoning and Death Wish.

Proposed hook or primitive: **wizard_drain (args power, skill) or a new primitive**

- [ ] 171 `drain_power`
- [ ] 191 `cruel_unminding`

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
- [ ] 177 `subversion`
- [ ] 19 `natures_cures`

## I. Battle field effects

Joins the 14 spells that already use `battlefield`.

Proposed hook or primitive: **battlefield**. Task 486's follow-up already gave the 14 existing spells an argument (`Custom(name: "battlefield", args: ["true_light"])`, the names in `Field::NAMES`); a new field effect adds a `Field` variant and its name.

- [ ] 1 `earth_to_mud`
- [ ] 11 `cracks_call`
- [ ] 81 `warp_wood`
- [ ] 82 `disrupt`
- [ ] 115 `call_chaos`
- [ ] 105 `magic_vortex`
- [ ] 167 `life_drain`
- [ ] 26 `earthquake`
- [ ] 119 `call_the_void`

## Last

- [ ] `Effect::Special`, `SPECIAL_SPELLS` and their arms removed; reverse check in `hook::problems`
- [ ] no `num ==` match for a spell left outside tests (a `grep` over the crates)
