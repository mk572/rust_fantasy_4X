# Wrapper audit (task 654)

What a wrapper is, which were found, and what was decided for each. The list is made by
`python3 scripts/find_wrappers.py` (a text search of function bodies of one call; it misses
wrappers of two statements and counts a caller by name, so a common name is over-counted).
Run it again to see what is left.

**Kinds.** *trivial*: the body is one call and every argument is a parameter. *std*: the same,
but the call is a standard method (`is_some`, `contains`, `len`, `new`). *task*: one call with
something computed, converted, checked or defaulted around it. Methods of trait impls are not
listed (the trait requires them).

**Decisions.** *Keep*: the wrapper is an interface on purpose, hides a private field, narrows
visibility, or its name adds meaning. *Open*: not decided yet; for a wrapper with a task the
rule of task 654 is proposed (one or two callers: inline; many callers: keep, but ask whether
its purpose belongs in the callee). *False hit*: the body does more than call. A wrapper that
only renames an item is better a re-export (`pub use path::item as name;`): no body, no call.

**Status.** Every row is read and decided (18 November 2026). Of the wrappers with a task, almost
all stay: they read a private representation, bind a default or a constant to a general function,
or are the short form of a pair with more than two callers. What the reading found besides is in
*Findings*: duplicated code and a table of numbers in code, which are not forwarders and are
listed as *Open* for tasks of their own.

## Already changed

| Wrapper | Where | Change |
|---|---|---|
| `view_turn` | f4x-ai `naval.rs` | removed; callers use `view.turn()` |
| `Game::fighter_strength` | f4x-core `battle/game.rs` | removed; callers use `units().unit_power` |
| `Game::pass_hero` | f4x-core `diplomacy` | removed; the call uses `pass_unit` |
| `met_near` | f4x-core `diplomacy` | removed; the call uses `foreign_owners_near` |
| `Economy::lair_of` | f4x-core `feature/entity.rs` | removed; the private `lair_info` took its name and doc |
| `Art::bm_font` | f4x-client `art.rs` | removed; callers use `font::shared_font` (and `label_font` lost its unused `app`) |
| `App::bar_button` | f4x-client `battle_screen/bar.rs` | removed; callers use `battle_button` |
| `picks_used` | f4x-newgame `computer_wizard.rs` | removed; callers use `picks_spent` |
| `WizardMut::cancel_casting` | f4x-core `wizard/sheet.rs` | removed; callers use `clear_casting` |
| `App::has_page`, `BattleUnit::has_enchantment`, `Economy::has_dependants` | f4x-client, f4x-core | removed (agreed): callers use the standard call |
| `Spells::instances_on` | f4x-core `spell/instance.rs` | removed: no caller once its two wrappers went |
| `Game::node_at`, `Game::city_enchantments` | f4x-core | removed; callers use `economy()` and `spells()` |
| `shipped()` (f4x-ai `testworld`, f4x-tools `testutil`), `content()` (f4x-client `test_app`) | test helpers | now re-exports of `f4x_def::testdir::shipped` |
| `city_instances`, `unit_instances`, `unit_modifiers`, `cast_instance`, `EventKind::from_number` | f4x-core | removed: no caller (dead code) |

## Counts

| kind | Keep | Open | False hit |
|---|---|---|---|
| trivial | 55 | 0 | 4 |
| std | 64 | 0 | 0 |
| task | 420 | 24 | 4 |

## Findings

Not wrappers, but found while reading them; none is changed.

1. **`fn stat(d: &UnitDef, s: Stat) -> f32`** is written twice, identically (`f4x-core` `city/autobuild.rs`, `f4x-ai` `builder.rs`): a method of `UnitDef` would serve both.
2. **`fn random(rng, n)`** (`rng.gen_range(1..=n.max(1))`) is written three times (`f4x-client` `cityscape.rs`, `f4x-core` `battle/map/layout.rs` and `feature/lair/create.rs`; the layout one lacks the `.max(1)`).
3. **`serial_of` and `sort_by_serial`** exist on `Game` (`game/serial.rs`) and on `UnitView` (`unit/mod.rs`) with the same bodies (one sorts with `sort_by_key`, the other with `sort_by_cached_key`).
4. **Books of a realm**: `computer_wizard::books` (`f4x-newgame`) and `books_of` (`f4x-client` `newgame_picks.rs`) both read `build.books.get(&realm).copied().unwrap_or(0)`: a method of `WizardBuild`.
5. **`id_numbered`** is the same function in two registries (`f4x-def` `feature.rs`, `map.rs`).
6. **Hero ability tables in code**: `leadership`, `blademaster`, `constitution`, `might`, `agility`, `arcane_power`, `prayermaster`, `armsmaster`, `legendary` and `sage` in `f4x-def/src/hero.rs` each bind two pairs of numbers to `by_tier`. That is a table of game numbers written as code, not content (the same kind of finding as tasks 653 and 657).

## Candidates

Callers is a text count and an upper bound.

| kind | crate | where | name | calls | callers | decision | why |
|---|---|---|---|---|---|---|---|
| trivial | f4x-ai | `crates/f4x-ai/src/garrison.rs:64` | `wanted_strength` | `wanted_share(view, at) * typical_strength` | 4 | False hit | the body multiplies |
| trivial | f4x-client | `crates/f4x-client/src/art.rs:383` | `Art::battle_info_icon` | `battle_info` | 1 | Keep | accessor: `icons` is private |
| trivial | f4x-client | `crates/f4x-client/src/diplomacy_screen.rs:209` | `App::open_diplomacy` | `try_open_diplomacy` | 1 | Keep | drops the bool result: not a pure forward |
| trivial | f4x-client | `crates/f4x-client/src/hotkeys.rs:209` | `Chord::pressed` | `modifiers_ok` | 41 | False hit | the body also checks the modifiers |
| trivial | f4x-client | `crates/f4x-client/src/hotkeys.rs:213` | `Chord::down` | `modifiers_ok` | 57 | False hit | the body also checks the modifiers |
| trivial | f4x-client | `crates/f4x-client/src/scroll.rs:33` | `Window::can_down` | `last_first` | 10 | False hit | the body compares |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2025` | `Game::wizards` | `wizards` | 134 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2029` | `Game::have_met` | `have_met` | 60 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2034` | `Game::treaty` | `treaty` | 159 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2039` | `Game::relation` | `relation` | 126 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2044` | `Game::war_age` | `war_age` | 4 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2049` | `Game::war_gain` | `war_gain` | 3 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2054` | `Game::drain_diplomacy_events` | `drain_events` | 14 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2111` | `Game::army_strength` | `army_strength` | 17 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2116` | `Game::check_proposal` | `check_proposal` | 34 | Keep | facade over the diplomacy state of the game |
| trivial | f4x-core | `crates/f4x-core/src/game/access.rs:250` | `Game::scenario_enchant_unit` | `add_unit_enchantment` | 3 | Keep | makes a crate-private function public for scenarios and tests |
| trivial | f4x-core | `crates/f4x-core/src/game/access.rs:255` | `Game::scenario_enchant_city` | `add_city_enchantment` | 2 | Keep | makes a crate-private function public for scenarios and tests |
| trivial | f4x-core | `crates/f4x-core/src/game/access.rs:261` | `Game::scenario_add_global` | `add_global` | 3 | Keep | makes a crate-private function public for scenarios and tests |
| trivial | f4x-core | `crates/f4x-core/src/game/access.rs:322` | `Game::scenario_enchant_roads` | `enchant_roads` | 9 | Keep | makes a crate-private function public for scenarios and tests |
| trivial | f4x-core | `crates/f4x-core/src/game/cheat.rs:84` | `Game::cheat_move_unit` | `put` | 11 | Keep | makes a crate-private function public for scenarios and tests |
| trivial | f4x-core | `crates/f4x-core/src/path_grid.rs:606` | `Searcher::distance` | `known` | 173 | Keep | the name says what the value is (cost to the targets of the last distance field) |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:124` | `PlayerView<'a>::content` | `content` | 1003 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:135` | `PlayerView<'a>::city_name_for` | `city_name_for` | 4 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:383` | `PlayerView<'a>::serial_of` | `serial_of` | 323 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:387` | `PlayerView<'a>::sort_by_serial` | `sort_by_serial` | 41 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:391` | `PlayerView<'a>::unit_def` | `unit_def` | 130 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:395` | `PlayerView<'a>::unit_position` | `unit_position` | 335 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:399` | `PlayerView<'a>::unit_stat` | `unit_stat` | 99 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:404` | `PlayerView<'a>::unit_sight` | `unit_sight` | 14 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:409` | `PlayerView<'a>::unit_moves_left` | `unit_moves_left` | 64 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:414` | `PlayerView<'a>::unit_hits` | `unit_hits` | 52 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:418` | `PlayerView<'a>::unit_has_ability` | `unit_has_ability` | 68 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:423` | `PlayerView<'a>::unit_mobility` | `unit_mobility` | 20 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:428` | `PlayerView<'a>::party_mobility` | `party_mobility` | 8 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:433` | `PlayerView<'a>::unit_power` | `unit_power` | 13 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:438` | `PlayerView<'a>::unit_has_order` | `unit_has_order` | 52 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:443` | `PlayerView<'a>::unit_destination` | `unit_destination` | 4 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:477` | `PlayerView<'a>::path` | `find_path` | 271 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:482` | `PlayerView<'a>::passengers_of` | `passengers_of` | 13 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:487` | `PlayerView<'a>::unit_food` | `unit_food` | 11 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:492` | `PlayerView<'a>::food_of` | `food_of` | 6 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:497` | `PlayerView<'a>::city_race` | `city_race` | 26 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:560` | `PlayerView<'a>::difficulty` | `difficulty` | 95 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:612` | `PlayerView<'a>::is_fortress_city` | `is_fortress_city` | 16 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:623` | `PlayerView<'a>::city_owner` | `city_owner` | 121 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:701` | `PlayerView<'a>::max_tile` | `max_tile` | 68 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:706` | `PlayerView<'a>::max_battle_side` | `max_battle_side` | 42 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:882` | `PlayerView<'a>::reachable` | `reachable_squares` | 48 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:887` | `PlayerView<'a>::reaches_any` | `reaches_any` | 16 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:893` | `PlayerView<'a>::unreached_region` | `unreached_region` | 4 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/player/view.rs:903` | `PlayerView<'a>::reach_signature` | `reach_signature` | 15 | Keep | interface: the AI reads the game only through the view |
| trivial | f4x-core | `crates/f4x-core/src/rules/exec.rs:90` | `strike` | `damage` | 22 | Keep | default method of the effect interface; the parameters are ignored on purpose |
| trivial | f4x-core | `crates/f4x-core/src/spell/cast.rs:538` | `Game::spell_has_hook` | `has_hook` | 9 | Keep | looks the spell up first: a view construction |
| trivial | f4x-core | `crates/f4x-core/src/wizard/mod.rs:94` | `retort_available` | `available` | 27 | Keep | looks the retort up first: a view construction |
| trivial | f4x-learn | `crates/f4x-learn/src/bank.rs:56` | `Bank::from_text` | `from_str` | 62 | Keep | hides the RON format and its error type |
| trivial | f4x-learn | `crates/f4x-learn/src/env.rs:154` | `BattleEnv::battle` | `battle` | 485 | Keep | hides the `game` field |
| trivial | f4x-learn | `crates/f4x-learn/src/policy.rs:48` | `scripted_command` | `next_command` | 9 | Keep | named entry point of the crate (examples and tests) |
| trivial | f4x-learn | `crates/f4x-learn/src/scenario.rs:75` | `BattleScenario::from_text` | `from_str` | 62 | Keep | hides the RON format and its error type |
| trivial | f4x-learn | `crates/f4x-learn/src/train.rs:613` | `Learner::plain` | `valid` | 178 | Keep | hides the burn `valid()` call behind a name that says why |
| std | f4x-ai | `crates/f4x-ai/src/roles.rs:661` | `Registry::get` | `get` | 1395 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/audio.rs:271` | `Audio::can_play` | `is_some` | 1 | Keep | agreed; the name says what the caller means |
| std | f4x-client | `crates/f4x-client/src/font.rs:160` | `Style::inked` | `new` | 62 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/icon_art.rs:34` | `IconArt::battle_info` | `copied` | 5 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/input.rs:59` | `Input::mouse_pressed` | `contains` | 95 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/input.rs:63` | `Input::mouse_down` | `contains` | 4 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/input.rs:67` | `Input::key_pressed` | `contains` | 260 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/input.rs:71` | `Input::key_down` | `contains` | 19 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/input.rs:90` | `Input::typed` | `copied` | 15 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/modal.rs:139` | `ModalStack::is_empty` | `is_empty` | 861 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/newgame_flow.rs:391` | `CreateState::ng_input` | `is_none` | 11 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-client | `crates/f4x-client/src/page.rs:111` | `App::close_page` | `take` | 2 | Keep | agreed; the domain word |
| std | f4x-client | `crates/f4x-client/src/view.rs:316` | `App::text_with` | `get` | 263 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-console | `crates/f4x-console/src/args.rs:55` | `Args<'a>::len` | `len` | 1410 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-console | `crates/f4x-console/src/args.rs:59` | `Args<'a>::is_empty` | `is_empty` | 861 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/battle/field.rs:161` | `BattleUnit::is_fantastic` | `is_some` | 11 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/battle/game.rs:176` | `Game::in_battle` | `is_some` | 74 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/battle/map.rs:115` | `Mover::walker` | `default` | 285 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/battle/map.rs:218` | `BattleMap::is_city` | `is_some` | 8 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/battle/map.rs:326` | `BattleMap::look` | `as_deref` | 22 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/battle/spell/unit_cast.rs:275` | `Game::can_summon_demon` | `is_ok` | 6 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/city/production.rs:369` | `Economy<'_>::sold_this_turn` | `is_some` | 6 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/feature/entity.rs:49` | `FeatureIndex::get` | `copied` | 1395 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/feature/entity.rs:53` | `FeatureIndex::insert` | `insert` | 276 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/feature/entity.rs:57` | `FeatureIndex::remove` | `remove` | 75 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/game/access.rs:105` | `Game::unit_has_order` | `is_ok` | 52 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/game/access.rs:117` | `Game::unit_building_road` | `is_ok` | 2 | Keep | agreed; hides the RoadWork component |
| std | f4x-core | `crates/f4x-core/src/game/access.rs:136` | `Game::component` | `ok` | 7 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/game/access.rs:142` | `Game::entity_exists` | `contains` | 4 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/game/access.rs:31` | `Game::map` | `as_ref` | 2519 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/game/replay.rs:141` | `Game::is_recording` | `is_some` | 4 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/item/crafting.rs:143` | `is_plain` | `is_some` | 1 | Keep | agreed; a game concept |
| std | f4x-core | `crates/f4x-core/src/item/mod.rs:123` | `Item::has_power` | `contains` | 4 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/item/mod.rs:200` | `HeroItems::iter` | `flatten` | 2771 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/map/fog.rs:141` | `Fog::forget` | `remove` | 18 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/map/mod.rs:348` | `Map::feature_entity` | `get` | 5 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/path_grid.rs:165` | `PathGrid::squares` | `len` | 84 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/path_grid.rs:406` | `Searcher::new` | `default` | 5489 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/player/entity.rs:33` | `Player<'a>::component` | `ok` | 7 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/player/entity.rs:49` | `Player<'a>::is_neutral` | `is_some` | 1 | Keep | hides the Neutral marker component |
| std | f4x-core | `crates/f4x-core/src/player/entity.rs:86` | `Player<'a>::has_grand_vizier` | `is_some` | 8 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/player/entity.rs:91` | `Player<'a>::is_defeated` | `is_some` | 33 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/player/view.rs:139` | `PlayerView<'a>::game_over` | `is_some` | 6 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/player/view.rs:265` | `PlayerView<'a>::can_aim` | `is_ok` | 26 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/player/view.rs:876` | `PlayerView<'a>::can_reach` | `is_some` | 30 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/player/view.rs:911` | `PlayerView<'a>::spell` | `get` | 1501 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/spell/city.rs:333` | `Economy<'_>::city_warded_against` | `contains` | 4 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/unit/mod.rs:616` | `UnitView<'a>::unit_is_unpaid` | `is_ok` | 5 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/unit/stasis.rs:26` | `Game::unit_in_stasis` | `is_ok` | 14 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/unit/transport.rs:216` | `economy` | `new` | 976 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/unit/undead.rs:47` | `UnitView<'_>::unit_is_undead` | `is_ok` | 23 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/wizard/entity.rs:103` | `Wizard<'a>::component` | `ok` | 7 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/wizard/fortress.rs:88` | `Wizard<'_>::is_banished` | `is_some` | 35 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:98` | `bundle` | `default` | 28 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-core | `crates/f4x-core/src/world_event/mod.rs:169` | `Events::ongoing` | `iter` | 4 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-def | `crates/f4x-def/src/half_int.rs:29` | `HalfInt::from_raw` | `Self` | 210 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-def | `crates/f4x-def/src/rules/defs.rs:778` | `SpellDef::has_hook` | `is_some` | 20 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-lang | `crates/f4x-lang/src/text.rs:34` | `Args::new` | `default` | 5489 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-master | `crates/f4x-master/src/lib.rs:46` | `plans` | `new` | 35 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-newgame | `crates/f4x-newgame/src/computer_wizard.rs:53` | `book_total` | `sum` | 9 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-setup | `crates/f4x-setup/src/original.rs:31` | `Check::is_complete` | `is_empty` | 3 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-setup | `crates/f4x-setup/src/say.rs:25` | `Say::get` | `get` | 1395 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-tools | `crates/f4x-tools/src/contestant.rs:39` | `plans_dropped` | `new` | 5 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| std | f4x-tools | `crates/f4x-tools/src/contestant.rs:44` | `settler_failures` | `new` | 8 | Keep | the name adds meaning (predicate, accessor or constructor of a type) |
| task | f4x-ai | `crates/f4x-ai/src/assault.rs:48` | `march` | `march_with` | 14 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-ai | `crates/f4x-ai/src/battle/order.rs:150` | `worth` | `strength` | 105 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/battle/order.rs:208` | `field_worth` | `sum` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/battle/order.rs:301` | `retreat` | `then_some` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/builder.rs:45` | `stat` | `map_or` | 206 | Open | duplicate helper: see Findings 1 and 2 |
| task | f4x-ai | `crates/f4x-ai/src/controller.rs:54` | `NeutralSeat::take_attacks` | `take` | 10 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/controller.rs:67` | `NeutralSeat::take_refusals` | `collect` | 11 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/garrison.rs:15` | `circle` | `and_then` | 21 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/intent.rs:166` | `is_land_boost` | `is_in` | 1 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-ai | `crates/f4x-ai/src/intent.rs:171` | `is_astral_gate` | `has_hook_arg` | 1 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-ai | `crates/f4x-ai/src/meld.rs:22` | `is_melder` | `unit_has_ability` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/military.rs:17` | `attack_ratio` | `knob` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/military.rs:21` | `sortie_ratio` | `knob` | 2 | Keep | names a tuning knob with its default in one place |
| task | f4x-ai | `crates/f4x-ai/src/military.rs:37` | `committed_ratio` | `knob` | 2 | Keep | names a tuning knob with its default in one place |
| task | f4x-ai | `crates/f4x-ai/src/naval.rs:149` | `is_tower` | `any` | 6 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/objective.rs:134` | `traits` | `traits_of` | 54 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/objective.rs:151` | `aggression_steps` | `from` | 1 | False hit | does arithmetic, a comparison or a search, not a call |
| task | f4x-ai | `crates/f4x-ai/src/plan_clock.rs:150` | `PlanClock::dropped` | `unwrap_or` | 30 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/race.rs:128` | `war_target` | `or` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:166` | `Registry::role_of` | `map` | 26 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:178` | `Registry::has_role` | `is_some_and` | 9 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:280` | `Registry::planar` | `copied` | 10 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:319` | `Registry::any` | `any` | 685 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:344` | `Registry::release_role` | `retain` | 11 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:349` | `Registry::release_many` | `retain` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:354` | `Registry::retain_role` | `retain` | 8 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:395` | `Registry::boarding_of` | `and_then` | 10 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:417` | `Registry::destination_of` | `and_then` | 1 | Keep | hides a private representation (component, map or cache) |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:422` | `Registry::crossing_of` | `and_then` | 19 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:550` | `Registry::any_muster` | `any` | 6 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:581` | `Registry::in_muster` | `is_some_and` | 6 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:616` | `Registry::party_of` | `and_then` | 2 | Keep | hides a private representation (component, map or cache) |
| task | f4x-ai | `crates/f4x-ai/src/roles.rs:621` | `Registry::with_of` | `and_then` | 4 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/scout.rs:476` | `best_target` | `best_target_with` | 6 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-ai | `crates/f4x-ai/src/site.rs:21` | `edge` | `knob` | 120 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/site.rs:26` | `expected` | `unwrap_or_else` | 32 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-ai | `crates/f4x-ai/src/spellhelper.rs:19` | `affinity_weight` | `knob` | 1 | Keep | names a tuning knob with its default in one place |
| task | f4x-ai | `crates/f4x-ai/src/spellhelper.rs:24` | `upkeep_weight` | `knob` | 1 | Keep | names a tuning knob with its default in one place |
| task | f4x-ai | `crates/f4x-ai/src/testworld.rs:29` | `content` | `clone` | 1003 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/testworld.rs:61` | `game` | `game_seeded` | 1736 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/testworld.rs:66` | `game_rival_across` | `build_game` | 7 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-ai | `crates/f4x-ai/src/testworld.rs:71` | `game_seeded` | `build_game` | 12 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/army_screen.rs:91` | `App::open_army_list` | `open_page` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/art.rs:1053` | `picture_path` | `from` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/art.rs:363` | `Art::has_wizard` | `len` | 1 | False hit | does arithmetic, a comparison or a search, not a call |
| task | f4x-client | `crates/f4x-client/src/art.rs:416` | `Art::draw_bitmap_text` | `draw_engine_text` | 1 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/art.rs:428` | `Art::draw_flat_text` | `draw_engine_text` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/art.rs:75` | `MinimapColours::terrain` | `unwrap_or` | 35 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/art.rs:88` | `MinimapColours::window_box` | `unwrap_or` | 1 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-client | `crates/f4x-client/src/art.rs:92` | `MinimapColours::mark` | `unwrap_or` | 16 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/art.rs:97` | `MinimapColours::named` | `unwrap_or` | 57 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/audio.rs:40` | `Archive::named` | `find` | 57 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/battle_ground.rs:126` | `file_grass` | `ground_file` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/battle_ground.rs:183` | `order_of` | `draw_order` | 14 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/battle_screen.rs:153` | `cell_origin` | `cell_origin_f` | 11 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/battle_screen.rs:158` | `cell_origin_f` | `original_origin` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/battle_screen.rs:818` | `App::battle_order` | `battle_order_as` | 30 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-client | `crates/f4x-client/src/bigbook.rs:55` | `App::spell_text` | `describe_spell` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/city_list_screen.rs:125` | `App::city_list_ok` | `panel_button` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/city_list_screen.rs:82` | `App::open_city_list` | `open_page` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/city_screen.rs:165` | `App::open_city_screen` | `open_page` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/city_screen.rs:73` | `heading_style` | `edge` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/city_screen.rs:99` | `Layout::rect` | `new` | 207 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/cityscape.rs:72` | `random` | `gen_range` | 117 | Open | duplicate helper: see Findings 1 and 2 |
| task | f4x-client | `crates/f4x-client/src/craft_art.rs:104` | `App::craft_ok` | `craft_blank` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/craft_screen.rs:216` | `App::open_craft` | `open_page` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/create_screen.rs:51` | `App::book_realms` | `to_vec` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/crowd_prompt.rs:50` | `cheapest_first` | `sort_by_key` | 2 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-client | `crates/f4x-client/src/diplomacy_list_screen.rs:142` | `App::open_diplomacy_list` | `open_page` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/diplomacy_list_screen.rs:354` | `App::open_talk_from_list` | `try_open_diplomacy_from` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/diplomacy_screen.rs:214` | `App::try_open_diplomacy` | `try_open_diplomacy_from` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/diplomacy_screen.rs:351` | `App::diplo_name` | `unwrap_or_else` | 20 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/diplomacy_screen.rs:926` | `DiploUi::diplo_propose` | `diplo_send` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:115` | `lapsed` | `line` | 7 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:119` | `gold_thanks` | `pick` | 1 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:123` | `spell_thanks` | `pick` | 1 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:127` | `treaty_broken` | `line` | 1 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:131` | `nothing_to_trade` | `line` | 1 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:18` | `line` | `get` | 250 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:23` | `pick` | `get` | 118 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:44` | `menu_prompt` | `line` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:48` | `propose_prompt` | `line` | 1 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:52` | `threaten_prompt` | `line` | 1 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:56` | `tribute_prompt` | `line` | 2 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:60` | `exchange_prompt` | `line` | 2 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:64` | `exchange_give_prompt` | `line` | 1 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/diplomacy_text.rs:79` | `cold_shoulder` | `pick` | 2 | Keep | the list of text keys the diplomacy screen uses |
| task | f4x-client | `crates/f4x-client/src/end_scene.rs:379` | `App::scene_pic` | `draw_pic` | 33 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/end_scene.rs:408` | `App::wizard_pictures` | `wizard` | 9 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/extra_mana.rs:40` | `most_extra` | `max` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:150` | `Style::survey_white` | `edge` | 13 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:155` | `Style::gold_shadowed` | `shadow` | 11 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:165` | `Style::flat` | `new` | 48 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:170` | `Style::outlined` | `effect` | 7 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:174` | `Style::shadow` | `effect` | 62 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:195` | `Style::center` | `align` | 108 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:199` | `Style::right` | `align` | 117 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:243` | `plain_text` | `plain_text_in` | 4 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-client | `crates/f4x-client/src/font.rs:266` | `plain_size` | `plain_size_in` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/font.rs:28` | `rgb` | `new` | 121 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:281` | `App::banner_ink` | `banner_set_ink` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:349` | `App::draw_styled_in` | `draw_line_in` | 12 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-client | `crates/f4x-client/src/font.rs:361` | `App::bm_wrap` | `wrap_lines` | 30 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:73` | `Style::nearest` | `new` | 68 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:79` | `Style::sh_nearest` | `shadow` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:84` | `Style::sh_set` | `shadow` | 17 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:89` | `Style::shadowed` | `sh_nearest` | 10 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/font.rs:94` | `Style::set` | `new` | 241 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/game_over_screen.rs:32` | `score_style` | `edge` | 8 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/hall_of_fame.rs:77` | `clean` | `replace` | 9 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/hotkeys.rs:319` | `Hotkeys::chords` | `map_or` | 9 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/hotkeys.rs:324` | `Hotkeys::pressed` | `any` | 41 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/hotkeys.rs:341` | `Hotkeys::down` | `any` | 57 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/hotkeys.rs:356` | `Hotkeys::uses_key` | `any` | 1 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-client | `crates/f4x-client/src/icon_art.rs:29` | `IconArt::item` | `map` | 456 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/item_screen.rs:52` | `hero_pages_for` | `max` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/menu_bar_art.rs:127` | `label_style` | `center` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/menu_screen.rs:30` | `slot_path` | `from` | 7 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/modal.rs:87` | `keep` | `map` | 89 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/newgame_button_art.rs:69` | `App::draw_board` | `draw_board_inked` | 4 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-client | `crates/f4x-client/src/newgame_flow.rs:168` | `App::open_creation` | `open_creation_with` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/newgame_flow.rs:28` | `ng_ink` | `inked` | 36 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/newgame_picks.rs:138` | `CreateState::books_of` | `unwrap_or` | 6 | Open | duplicate of the newgame books: see Findings 4 |
| task | f4x-client | `crates/f4x-client/src/options_screen.rs:207` | `App::open_options` | `open_page` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/page_turn.rs:206` | `App::open_test_page_turn` | `with_page` | 1 | Keep | cheat or test hook |
| task | f4x-client | `crates/f4x-client/src/panel_button_art.rs:101` | `sheen_label` | `sheen_text` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/resolution.rs:28` | `art_scale` | `load` | 12 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/road_plan.rs:48` | `button_rect` | `rect` | 2 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/scroll.rs:25` | `Window::last_first` | `saturating_sub` | 7 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/sell_prompt.rs:255` | `App::confirmation_box` | `question_box` | 12 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/settings_screen.rs:225` | `App::open_settings` | `open_page` | 6 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/spellbook.rs:163` | `App::book_pages` | `book_pages_sorted` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/unit_screen.rs:211` | `App::draw_production_sheet` | `draw_sheet_panel_as` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/unit_screen.rs:33` | `page_len` | `min` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/unit_screen.rs:38` | `previous_page` | `saturating_sub` | 1 | False hit | does arithmetic, a comparison or a search, not a call |
| task | f4x-client | `crates/f4x-client/src/unit_screen.rs:496` | `App::arrow_button` | `pic_button_on` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/unit_screen.rs:501` | `App::red_button` | `red_button_face` | 19 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-client | `crates/f4x-client/src/unit_screen.rs:513` | `App::red_button_sized` | `red_button_face_sized` | 8 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-client | `crates/f4x-client/src/unit_screen.rs:525` | `App::red_button_face` | `red_button_face_sized` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/unit_screen.rs:95` | `App::open_unit_sheet` | `open_unit_sheet_of` | 6 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-client | `crates/f4x-client/src/view.rs:1064` | `App::spell_arg` | `text` | 22 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/view.rs:1113` | `App::city_name` | `unwrap_or_else` | 63 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-client | `crates/f4x-client/src/view.rs:1507` | `App::tile_size` | `vec2` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/view.rs:1511` | `App::map_rect` | `rect` | 7 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/view.rs:1582` | `App::reveal_all` | `reveal_map` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-client | `crates/f4x-client/src/view.rs:311` | `App::text` | `text_with` | 1222 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-client | `crates/f4x-client/src/wizard_pick.rs:80` | `rows_window` | `new` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-client | `crates/f4x-client/src/wrath_scene.rs:94` | `App::open_test_wrath` | `enqueue_wrath` | 1 | Keep | cheat or test hook |
| task | f4x-console | `crates/f4x-console/src/args.rs:63` | `Args<'a>::get` | `map` | 1395 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-console | `crates/f4x-console/src/describe.rs:59` | `spell` | `clone` | 1501 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-console | `crates/f4x-console/src/legal.rs:371` | `spell_arg` | `resolve` | 22 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-console | `crates/f4x-console/src/legal.rs:443` | `battle_unit` | `Ok` | 12 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-console | `crates/f4x-console/src/legal.rs:593` | `bend` | `battle_order` | 1 | Keep | signature required by a table, attribute or interface |
| task | f4x-console | `crates/f4x-console/src/legal.rs:597` | `bflee` | `battle_order` | 1 | Keep | signature required by a table, attribute or interface |
| task | f4x-console | `crates/f4x-console/src/lib.rs:199` | `find` | `find` | 526 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-console | `crates/f4x-console/src/lib.rs:309` | `Ctx<'_>::say` | `push` | 222 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-console | `crates/f4x-console/src/meta.rs:251` | `save_path` | `named_path` | 2 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-core | `crates/f4x-core/src/battle/casting.rs:147` | `BattleCaster::new` | `with_mana` | 5489 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/battle/casting.rs:164` | `BattleCaster::mana_for` | `from_raw` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/field.rs:122` | `Field::of` | `named` | 184 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/game.rs:1337` | `Game::auto_resolve` | `auto_resolve_with` | 4 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/battle/game.rs:205` | `Game::battle` | `map` | 485 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/battle/immunity.rs:57` | `Realm::of` | `from_num` | 184 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/immunity.rs:67` | `Realm::id` | `new` | 1746 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/info.rs:45` | `BattleInfo::is_empty` | `is_empty` | 861 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/battle/info.rs:50` | `BattleInfo::side_rows` | `max` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/map.rs:237` | `BattleMap::is_breached` | `is_some_and` | 4 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/battle/map.rs:321` | `BattleMap::generate` | `from_layout` | 89 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/map.rs:332` | `BattleMap::is_playable` | `is_some_and` | 15 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/battle/map.rs:344` | `BattleMap::get` | `then` | 1395 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/map.rs:627` | `distance` | `max` | 173 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/map/layout.rs:271` | `random` | `gen_range` | 117 | Open | duplicate helper: see Findings 1 and 2 |
| task | f4x-core | `crates/f4x-core/src/battle/power.rs:86` | `def_power` | `unit_power` | 8 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/spell.rs:212` | `BattleEffect::of` | `named` | 184 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/battle/vortex.rs:45` | `Battle::vortex_at` | `position` | 6 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/city/autobuild.rs:105` | `Focus::index` | `position` | 267 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/city/autobuild.rs:330` | `stat` | `map_or` | 206 | Open | duplicate helper: see Findings 1 and 2 |
| task | f4x-core | `crates/f4x-core/src/city/autobuild.rs:334` | `is_settler` | `contains` | 9 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/city/conquest.rs:148` | `Economy<'_>::citizens` | `map_or` | 44 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/city/conquest.rs:256` | `Game::destroy_building` | `destroy_building` | 14 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/city/conquest.rs:39` | `city_size_class` | `min` | 12 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/city/economy.rs:339` | `Economy<'a>::units` | `new` | 1746 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/city/economy.rs:343` | `Economy<'a>::wizard` | `find` | 772 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/city/production.rs:311` | `Economy<'_>::city_weapon_quality` | `city_weapon_quality_with` | 5 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/city/production.rs:374` | `Economy<'_>::sold_building` | `map` | 4 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/city/production.rs:379` | `Economy<'_>::sale_price` | `map` | 5 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2059` | `Game::make_contact` | `with_diplomacy` | 70 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2065` | `Game::contact_near` | `with_diplomacy` | 3 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2086` | `Game::subvert` | `with_diplomacy` | 3 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2091` | `Game::start_treaty` | `with_diplomacy` | 30 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2096` | `Game::break_treaties` | `with_diplomacy` | 12 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2101` | `Game::declare_war` | `with_diplomacy` | 30 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2106` | `Game::declare_peace` | `with_diplomacy` | 4 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:2126` | `Game::diplomacy_command` | `with_diplomacy` | 1 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:451` | `Diplomacy::pair` | `get` | 59 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:455` | `Diplomacy::stance` | `get` | 50 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:459` | `Diplomacy::stance_mut` | `or_default` | 42 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:470` | `met_in_sight` | `owners` | 1 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:639` | `Diplomacy::have_met` | `is_some_and` | 60 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/diplomacy/mod.rs:644` | `Diplomacy::treaty` | `map_or` | 159 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/feature/entity.rs:136` | `Economy<'a>::feature_at` | `get` | 7 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/feature/entity.rs:141` | `Economy<'a>::feature_kind_at` | `map` | 4 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/feature/lair/create.rs:38` | `random` | `gen_range` | 117 | Open | duplicate helper: see Findings 1 and 2 |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:215` | `Game::scenario_set_experience` | `put` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:231` | `Game::scenario_set_hurt` | `put` | 14 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:281` | `Game::scenario_start_event` | `start` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:287` | `Game::scenario_found_city` | `spawn_city` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:299` | `Game::scenario_set_moves_left` | `put` | 6 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:327` | `Game::scenario_remove_unit` | `remove_unit` | 8 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:36` | `Game::is_explored` | `is_explored` | 50 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:41` | `Game::explored_corners` | `corners` | 2 | Keep | hides a private representation (component, map or cache) |
| task | f4x-core | `crates/f4x-core/src/game/access.rs:87` | `Game::city_farmers` | `map_or` | 3 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/game/cheat.rs:200` | `Game::cheat_defeat_player` | `defeat` | 1 | Keep | cheat or test hook |
| task | f4x-core | `crates/f4x-core/src/game/endgame.rs:219` | `Game::win_by_mastery` | `finish` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/replay.rs:88` | `deserialize` | `entity_of` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/game/serial.rs:26` | `Game::serial_of` | `map_or` | 323 | Open | duplicate in Game and UnitView: see Findings 3 |
| task | f4x-core | `crates/f4x-core/src/game/serial.rs:31` | `Game::sort_by_serial` | `sort_by_key` | 41 | Open | duplicate in Game and UnitView: see Findings 3 |
| task | f4x-core | `crates/f4x-core/src/game/undo.rs:64` | `Game::undo_label` | `map` | 1 | Keep | hides a private representation (component, map or cache) |
| task | f4x-core | `crates/f4x-core/src/hero/game.rs:111` | `UnitView<'_>::hero_name` | `map` | 10 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/hero/game.rs:123` | `UnitView<'_>::hero_kind` | `map` | 9 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/hero/game.rs:205` | `Game::make_hero_with` | `spawn` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/hero/game.rs:288` | `Game::remove_unit` | `remove_unit_to` | 47 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/hero/game.rs:54` | `UnitView<'_>::hero_state` | `map` | 12 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/hero/game.rs:72` | `UnitView<'_>::hero_xp` | `map_or` | 5 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/hero/game.rs:77` | `UnitView<'_>::hero_type_name` | `hero_name` | 10 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/item/armory.rs:49` | `Armory::items` | `chain` | 190 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/item/entity.rs:161` | `Game::spawn_item_with` | `spawn` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/item/entity.rs:52` | `ItemView<'a>::crafting` | `map` | 8 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/item/entity.rs:57` | `ItemView<'a>::item` | `map` | 456 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/item/entity.rs:62` | `ItemView<'a>::held_by` | `map` | 1 | Keep | hides a private representation (component, map or cache) |
| task | f4x-core | `crates/f4x-core/src/item/mod.rs:119` | `Item::slot_kind` | `slot_of` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/item/mod.rs:204` | `HeroItems::is_empty` | `all` | 861 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/item/mod.rs:209` | `HeroItems::has_power` | `any` | 4 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/item/mod.rs:286` | `ItemView<'a>::units` | `new` | 1746 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/item/mod.rs:383` | `Game::items` | `new` | 190 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/item/mod.rs:390` | `UnitView<'_>::items` | `new` | 190 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/map/fog.rs:32` | `sight_for` | `from_raw` | 15 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/map/road.rs:204` | `Game::process_road_work_for` | `work_group` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/map/vision.rs:162` | `VisionGrid::tracked_at` | `map` | 1 | Keep | hides a private representation (component, map or cache) |
| task | f4x-core | `crates/f4x-core/src/map/vision.rs:167` | `VisionGrid::tracked` | `map` | 1 | Keep | hides a private representation (component, map or cache) |
| task | f4x-core | `crates/f4x-core/src/map/vision.rs:45` | `Economy<'_>::city_sight` | `compute_half` | 8 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/map/vision.rs:51` | `Economy<'_>::visibility` | `flags` | 19 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/path_grid.rs:290` | `PathGrid::regions` | `get_or_init` | 1 | Keep | hides a private representation (component, map or cache) |
| task | f4x-core | `crates/f4x-core/src/player/entity.rs:54` | `Player<'a>::control` | `map` | 3 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/player/entity.rs:65` | `Player<'a>::name` | `player_name` | 677 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/entity.rs:70` | `Player<'a>::banner` | `map_or` | 109 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/player/entity.rs:75` | `Player<'a>::portrait` | `map_or` | 69 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/player/mod.rs:31` | `id_of` | `map` | 32 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/player/offer.rs:119` | `item_chance` | `offer_chance` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/race_player.rs:18` | `race_player_id` | `PlayerId` | 8 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:155` | `PlayerView<'a>::is_defeated` | `is_some_and` | 33 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:160` | `PlayerView<'a>::offers` | `offers` | 64 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:165` | `PlayerView<'a>::wizard` | `wizard` | 772 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:170` | `PlayerView<'a>::budget` | `budget` | 81 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:183` | `PlayerView<'a>::casting_cost` | `casting_cost` | 45 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:195` | `PlayerView<'a>::mastery_open` | `mastery_open_to` | 2 | Keep | interface of the view / report |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:235` | `PlayerView<'a>::treaty_with` | `treaty` | 9 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:240` | `PlayerView<'a>::income` | `income` | 95 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:254` | `PlayerView<'a>::paid_spell` | `and_then` | 2 | Keep | interface of the view / report |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:259` | `PlayerView<'a>::spell_needs_target` | `spell_needs_target` | 12 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:271` | `PlayerView<'a>::hero_to_summon` | `hero_to_summon` | 7 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:345` | `PlayerView<'a>::my_heroes` | `hero_units` | 9 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:350` | `PlayerView<'a>::armory` | `armory` | 73 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:370` | `PlayerView<'a>::fortress` | `and_then` | 133 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:374` | `PlayerView<'a>::my_units` | `player_units` | 102 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:379` | `PlayerView<'a>::my_cities` | `cities_of` | 70 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:573` | `PlayerView<'a>::explored` | `is_explored` | 51 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:783` | `PlayerView<'a>::in_node` | `is_some` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:788` | `PlayerView<'a>::has_lair` | `is_some` | 14 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:844` | `PlayerView<'a>::lair_guards` | `lair_guards_known_to` | 1 | Keep | interface of the view / report |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:849` | `PlayerView<'a>::can_found_city` | `is_ok` | 28 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/player/view.rs:86` | `CityReport::size_class` | `city_size_class` | 1 | Keep | interface of the view / report |
| task | f4x-core | `crates/f4x-core/src/rules/exec.rs:74` | `resistance_against` | `stat` | 2 | Keep | signature required by a table, attribute or interface |
| task | f4x-core | `crates/f4x-core/src/spell/global_enchantment.rs:17` | `magic_has_global` | `any_global` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/spell/global_enchantment.rs:58` | `magic_has_effect` | `any_global` | 6 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/global_enchantment.rs:71` | `Game::add_global` | `add_enchantment` | 28 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/hero.rs:35` | `HeroSummon::of` | `named` | 184 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/hero.rs:41` | `torin` | `saturating_sub` | 11 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/hero.rs:75` | `UnitView<'_>::ai_resurrection_pick` | `copied` | 3 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:148` | `Spells<'a>::all` | `matching` | 404 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:175` | `Spells<'a>::of_caster` | `matching` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:180` | `Spells<'a>::city_enchantments` | `enchantments_on` | 32 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:185` | `Spells<'a>::unit_enchantments` | `enchantments_on` | 29 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:190` | `Spells<'a>::unit_has` | `has` | 2 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:242` | `Spells<'a>::of_def` | `matching` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:260` | `Spells<'a>::def` | `Some` | 365 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:265` | `Spells<'a>::caster` | `Some` | 168 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:270` | `Spells<'a>::target` | `map` | 408 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:332` | `Game::spells` | `new` | 232 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:348` | `Game::add_city_enchantment` | `add_enchantment` | 23 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:358` | `Game::remove_city_enchantment` | `remove_enchantment` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:368` | `Game::add_unit_enchantment` | `add_enchantment` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:378` | `Game::remove_unit_enchantment` | `remove_enchantment` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/spell/instance.rs:406` | `Game::remove_enchantment` | `remove_on` | 6 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/special.rs:87` | `Game::earth_lore` | `explore_block` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/spell/temp_effect.rs:141` | `UnitView<'a>::unit_abilities` | `clone` | 17 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/temp_effect.rs:265` | `UnitView<'a>::unit_resistance_against` | `effective_resistance` | 14 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/temp_effect.rs:310` | `Game::add_temp_modifier` | `add_modifier` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/temp_effect.rs:315` | `Game::grant_temp_ability` | `grant_ability` | 18 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/temp_effect.rs:320` | `Game::remove_temp_ability` | `remove_ability` | 2 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/spell/temp_effect.rs:66` | `grant_ability` | `push_temp` | 6 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/spell/temp_effect.rs:71` | `remove_ability` | `push_temp` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/spell/terraform.rs:36` | `TerrainEffect::of` | `named` | 184 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/experience.rs:136` | `Game::gain_xp` | `gain_xp` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/kept.rs:61` | `disarm` | `with` | 1 | Keep | hides a private representation (component, map or cache) |
| task | f4x-core | `crates/f4x-core/src/unit/mobility.rs:69` | `def_mobility` | `unit_mobility` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:109` | `UnitView<'a>::wizard` | `find` | 772 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:219` | `UnitView<'a>::planar_seal_active` | `any_global_effect` | 5 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:224` | `UnitView<'a>::serial_of` | `map_or` | 323 | Open | duplicate in Game and UnitView: see Findings 3 |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:229` | `UnitView<'a>::sort_by_serial` | `sort_by_cached_key` | 41 | Open | duplicate in Game and UnitView: see Findings 3 |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:234` | `UnitView<'a>::unit_kind_id` | `map` | 5 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:244` | `UnitView<'a>::unit_weapon_quality` | `map_or` | 13 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:248` | `UnitView<'a>::unit_owner` | `id_of` | 156 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:253` | `UnitView<'a>::unit_owner_entity` | `map` | 1 | Keep | hides a private representation (component, map or cache) |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:257` | `UnitView<'a>::unit_position` | `map` | 335 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:261` | `UnitView<'a>::unit_has_ability` | `contains` | 68 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:281` | `UnitView<'a>::unit_enchantments` | `unit_enchantments` | 29 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:516` | `UnitView<'a>::unit_experience` | `map_or` | 4 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:554` | `UnitView<'a>::unit_power` | `unit_power` | 13 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:710` | `Game::units` | `new` | 1746 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/mod.rs:89` | `UnitView<'_>::unit_standby` | `map` | 9 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/unit/movement.rs:170` | `Economy<'_>::step_cost` | `step_cost_with` | 13 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/unit/mutation.rs:45` | `UnitView<'_>::unit_is_mutated` | `unit_has_ability` | 9 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/unit/pathing/grid.rs:22` | `half_moves` | `map_or` | 4 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/unit/undead.rs:98` | `Game::make_undead` | `make_undead` | 15 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/entity.rs:120` | `Wizard<'a>::portrait` | `map_or` | 69 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/entity.rs:149` | `Wizard<'a>::any_global` | `any_global_of` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/entity.rs:154` | `Wizard<'a>::player` | `Some` | 1616 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/entity.rs:160` | `of_player` | `Some` | 14 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/fortress.rs:73` | `Wizard<'_>::fortress` | `map` | 133 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/fortress.rs:78` | `Wizard<'_>::summon_at` | `map` | 24 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/fortress.rs:83` | `Wizard<'_>::summon_point` | `or` | 22 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/fortress.rs:93` | `Wizard<'_>::banished_by` | `and_then` | 4 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/magic_game.rs:313` | `Game::start_cast` | `start_cast_with` | 10 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-core | `crates/f4x-core/src/wizard/magic_income.rs:189` | `Game::mana_upkeep` | `mana_upkeep` | 22 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/magic_income.rs:194` | `Game::income` | `income` | 95 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/mod.rs:239` | `Game::preset_build` | `preset_build_of` | 8 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/mod.rs:88` | `all_retorts` | `map` | 11 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/objective.rs:92` | `Wizard<'_>::objective` | `map` | 35 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/retort.rs:52` | `stat_with` | `compute` | 6 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/retort.rs:63` | `stat_half` | `compute_half` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-core | `crates/f4x-core/src/wizard/retort.rs:91` | `stat` | `stat_with` | 206 | Open | duplicate helper: see Findings 1 and 2 |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:149` | `Wizard<'a>::skill_points` | `map_or` | 6 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:154` | `Wizard<'a>::skill_left` | `map_or` | 17 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:159` | `Wizard<'a>::casting_skill` | `base_casting_skill` | 34 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:211` | `Wizard<'a>::researching` | `and_then` | 19 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:216` | `Wizard<'a>::research_remaining` | `map_or` | 8 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:220` | `Wizard<'a>::fame` | `map_or` | 104 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:248` | `Wizard<'a>::has_global` | `any` | 17 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:256` | `Wizard<'a>::casting_cost` | `casting_cost` | 45 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:428` | `Wizard<'a>::instant_budget` | `max` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:442` | `Wizard<'a>::upkeep_percent` | `stat` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:447` | `Wizard<'a>::unit_upkeep_percent` | `stat` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:507` | `WizardMut<'a>::view` | `new` | 1124 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/wizard/sheet.rs:853` | `WizardMut<'a>::cast` | `cast_of` | 259 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/world_event/mod.rs:160` | `Events::is_ongoing` | `any` | 13 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/world_event/mod.rs:164` | `Events::get` | `find` | 1395 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-core | `crates/f4x-core/src/world_event/mod.rs:208` | `Events::end` | `retain` | 60 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/world_event/mod.rs:326` | `Events::mana_is_short` | `is_ongoing` | 4 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-core | `crates/f4x-core/src/world_event/mod.rs:54` | `EventKind::from_roll` | `nth` | 1 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-core | `crates/f4x-core/src/world_event/mod.rs:87` | `EventKind::from_id` | `find` | 3 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-def | `crates/f4x-def/src/feature.rs:97` | `Features::id_numbered` | `map` | 715 | Open | same pattern in two registries: see Findings 5 |
| task | f4x-def | `crates/f4x-def/src/half_int.rs:34` | `HalfInt::from_int` | `Self` | 544 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-def | `crates/f4x-def/src/half_int.rs:49` | `HalfInt::floor` | `div_euclid` | 254 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-def | `crates/f4x-def/src/half_int.rs:59` | `HalfInt::abs` | `Self` | 130 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-def | `crates/f4x-def/src/half_int.rs:70` | `HalfInt::saturating_sub` | `Self` | 164 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-def | `crates/f4x-def/src/half_int.rs:81` | `serialize` | `serialize_i32` | 2 | Keep | signature required by a table, attribute or interface |
| task | f4x-def | `crates/f4x-def/src/help.rs:43` | `Help::topic` | `find` | 46 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-def | `crates/f4x-def/src/help.rs:48` | `Help::topic_of` | `topic` | 10 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-def | `crates/f4x-def/src/help.rs:53` | `Help::screen` | `find` | 126 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-def | `crates/f4x-def/src/help.rs:60` | `Help::at` | `at_where` | 3021 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-def | `crates/f4x-def/src/hero.rs:172` | `leadership` | `by_tier` | 13 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:177` | `blademaster` | `by_tier` | 11 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:182` | `constitution` | `by_tier` | 10 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:187` | `might` | `by_tier` | 14 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:192` | `agility` | `by_tier` | 10 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:197` | `arcane_power` | `by_tier` | 12 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:203` | `prayermaster` | `by_tier` | 12 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:208` | `armsmaster` | `by_tier` | 10 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:213` | `legendary` | `by_tier` | 11 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/hero.rs:218` | `sage` | `by_tier` | 14 | Open | a table of numbers in code: see Findings 6 |
| task | f4x-def | `crates/f4x-def/src/map.rs:135` | `Terrains::id_numbered` | `map` | 715 | Open | same pattern in two registries: see Findings 5 |
| task | f4x-def | `crates/f4x-def/src/rules/defs.rs:80` | `two_food` | `from_int` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/rules/stats.rs:176` | `Modifier::new` | `amount` | 5489 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-def | `crates/f4x-def/src/settings.rs:1092` | `SettingsDef::level` | `get` | 112 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-def | `crates/f4x-def/src/settings.rs:260` | `fb_max_books_total` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:264` | `fb_max_books_per_realm` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:501` | `fb_spell_of_mastery` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:505` | `fb_spell_of_return` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:509` | `fb_limit_ai_races` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:532` | `default_monster_start` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:536` | `default_trireme` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:540` | `default_galley` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:544` | `default_max_tile` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:548` | `default_max_battle_side` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:552` | `fb_diagonal_extra_cost` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:556` | `fb_original_map_generator` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:560` | `fb_melee_vs_fliers` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:564` | `fb_half_upkeep` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:568` | `fb_arcane_candidate_cap` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:572` | `fb_ban_life_and_death` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:576` | `fb_walls_cover_city` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:580` | `fb_miners_guild_half_ore` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:584` | `fb_spell_binding_always_succeeds` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:588` | `fb_two_pick_myrran` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:592` | `fb_extend_tiny_islands` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:596` | `fb_mix_neutral_races` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:600` | `fb_adamant_alchemy` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:604` | `fb_color_wheel_relations` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:608` | `fb_defeat_loot` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:612` | `fb_lair_full_heal` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:616` | `fb_roads_at_start` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:620` | `fb_stronger_lairs` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:624` | `fb_treasure_patch_150` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:628` | `fb_suppress_spares_return` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:632` | `fb_frequency` | `fallback` | 0 | Keep | serde default function: the attribute names it |
| task | f4x-def | `crates/f4x-def/src/settings.rs:636` | `yes` | `fallback` | 5 | Keep | serde default function: the attribute names it |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:109` | `get` | `map_or` | 1395 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:114` | `Names::unit` | `get` | 2022 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:117` | `Names::ability` | `get` | 72 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:120` | `Names::building` | `get` | 157 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:123` | `Names::realm` | `get` | 420 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:126` | `Names::retort` | `get` | 60 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:129` | `Names::wizard` | `get` | 772 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:132` | `Names::spell` | `get` | 1501 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:135` | `Names::item` | `get` | 456 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:138` | `Names::item_power` | `get` | 6 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:141` | `Names::terrain` | `get` | 35 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:144` | `Names::feature` | `get` | 44 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-lang | `crates/f4x-lang/src/names.rs:149` | `Names::race` | `map_or` | 216 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-learn | `crates/f4x-learn/src/act.rs:146` | `spell_slot` | `position` | 1 | Keep | converts, defaults or names a test; the name adds meaning |
| task | f4x-learn | `crates/f4x-learn/src/act.rs:163` | `WizardOptions::any` | `any` | 685 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-learn | `crates/f4x-learn/src/bank.rs:52` | `Bank::to_text` | `unwrap_or_default` | 76 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-learn | `crates/f4x-learn/src/build.rs:186` | `neighbour_land` | `find` | 2 | False hit | does arithmetic, a comparison or a search, not a call |
| task | f4x-learn | `crates/f4x-learn/src/env.rs:163` | `BattleEnv::to_play` | `map` | 5 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-learn | `crates/f4x-learn/src/net.rs:132` | `PolicyNet::placeholder` | `new` | 3 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-learn | `crates/f4x-learn/src/net.rs:423` | `log_probs` | `log_softmax` | 10 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-learn | `crates/f4x-learn/src/reward.rs:48` | `reward` | `reward_with_budget` | 42 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
| task | f4x-learn | `crates/f4x-learn/src/scenario.rs:71` | `BattleScenario::to_text` | `unwrap_or_default` | 76 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-learn | `crates/f4x-learn/src/train.rs:842` | `probabilities` | `softmax` | 1 | Keep | binds a fixed argument (default, screen, key or constant) to a general function; the name says which; not clear-cut |
| task | f4x-master | `crates/f4x-master/src/lib.rs:132` | `State::from_text` | `map_err` | 62 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-newgame | `crates/f4x-newgame/src/computer_wizard.rs:44` | `books` | `unwrap_or` | 155 | Open | duplicate of the client's books_of: see Findings 4 |
| task | f4x-newgame | `crates/f4x-newgame/src/neutral.rs:22` | `neutral_city_count` | `scaled_count` | 11 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-setup | `crates/f4x-setup/src/flow.rs:59` | `target` | `join` | 408 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-setup | `crates/f4x-setup/src/original.rs:95` | `expected` | `filter` | 32 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-setup | `crates/f4x-setup/src/roms.rs:37` | `folder` | `join` | 103 | Keep | binds fixed arguments or a default to a general function; the name says which (read) |
| task | f4x-setup | `crates/f4x-setup/src/say.rs:30` | `Say::plain` | `get` | 178 | Keep | reads a private representation (component, map, table) with a default or a conversion |
| task | f4x-tools | `crates/f4x-tools/src/metrics.rs:268` | `never_cast` | `never_cast_in` | 3 | Keep | pair: the short form binds the default of the longer one, and has more than two callers |
