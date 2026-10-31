# Heroes and items: design

> Heroes and items are entities (the rework, tasks 326 to 333): the game has one hero entity of each
> type, an individual that any wizard may hire while it is free (`hero/kind.rs` for the rules,
> `hero/game.rs` for the entities), and each item is an entity held by a hero, a vault or an offer
> (`item/entity.rs`); see `ecs-structure.md` 3.5. There is no per-wizard roster and no `Armory`
> component on the player.

The design of the heroes and items (tasks 47 to 51, 72, 127, 138 to 143, 174, 175, 218 to 222 and
262): what the original does (section 2), the pieces and how they were built (3 to 5), the assets
(6) and where the help, the original and ReMoM disagree (7, task 223). Everything in it is built
except what section 7 and the open questions in 8 say. It follows the base rule of `design.md`:
each system is plain data plus functions over the pieces they need, not methods on `Game`
(`ecs-structure.md`, guideline 12).

Sources, so every number can be checked: the original's data files (`ITEMDATA.LBX`, `ITEMPOW.LBX`,
`NAMES.LBX`, `PORTRAIT.LBX`, ...), the ReMoM clone in `~/projects/ReMoM` (`MoM/src/*.c`, written
`File.c:function` below), its notes in `~/projects/ReMoM/doc/`, and the original help
(`content-local/help.ron`, topic numbers in brackets). "Level" is the 0-based level the code
stores (0 = "Hero", 8 = "Demi-God"); the original's formulas read `Level + 1`.

## 1. What exists

| Piece | State |
|---|---|
| 35 hero units in `content/units.ron`, `content/heroes.ron` | `kind: Hero`, `level_path: hero_xp`, `hero: (kind: Warrior / Bow / SwordOrStaff / Mage, portrait, hire_cost)` (the type fixes the three slots, `HeroKind::slots`, `SlotKind::fits`) and the 35 starting records of `NewGame.c:_hero_premade_table` (picks, fixed abilities by `HSA_*` name, casting skill, four innate spells), loaded into `Rules::hero_templates` and checked by `validate`. |
| 250 items in `content/items.ron`, 66 powers in `content/item_powers.ron` | The original's items and powers: `ItemDef {id, name, slot, kind, cost, effects, spell_skill, spell_save, powers, charges, books, flag}`; plain bonuses are effects (movement as `HalfMoves`, to hit in pluses). |
| `item/mod.rs`, `item/entity.rs` | `Item` (an instance: name, kind, value, `ItemBonus`, power ids, charges, the prefab id it copies) is the component of an item entity, held by `HeldBy` (a hero's slot, the vault, the award list, an offer). `HeroItems` is a hero's three slots read as one value. `modifiers` (attack and to hit to melee or ranged by kind, Flaming and Giant Strength), `Item::granted_spells` (the powers that are lasting spells in the original: the carrier gets them as spells that no dispel touches), `granted_abilities`. `Source::Item(slot)`; new stats `MeleeToHit`, `RangedToHit`. `Game::items()` is the item rules' read-only view (`items::Items`): what a unit carries, the spells, abilities and stat modifiers its items give, a wizard's armory. Which lasting spell or attack ability a power gives is data in `item_powers.ron` (`spell`, `attack`); only the few powers the code handles by id are in `OTHER_POWERS`. |
| `item/armory.rs` | `Armory` (the vault, which has no limit, and the award list, read from the item entities), `Game::move_item` (the item screen's rules, 20 mana for a far hero, the drowning check), `give_item`, `scrap_item` (the anvil), `Command::MoveItem` and `ScrapItem`. `plan_move` works over the `ItemWorld` trait, tested without a `Game`. |
| `item/crafting.rs`, `item/make.rs` | Enchant Item and Create Artifact (`ItemDesign`, `Craft`, `cost`, `check`, `available_powers`, `build`, `Game::craft_item` / `Command::CraftItem`, `Event::ItemMade`; the screen is `craft_screen.rs`), random and prefab items (`make_item`). |
| `hero/mod.rs`, `hero/kind.rs`, `hero/game.rs` | `Hero` (kind, name, `HeroAbilities`, casting skill, innate spells) and `HeroState` (`Free`, `Serving`, `Dead`, `Gone`) are components of a hero entity; while it serves it also has the unit components, and `HeroAbilities::ids` feeds `unit_abilities` (so Noble, Armsmaster and the rest are unit abilities). `kind::roll` (`Init_Heroes`, called last in `new_game`), `kind::pick` (`Pick_Random_Hero`), `hire_cost`, `level_of`. `Units::hero_units`, `hero_kind`, `hero_standing`, `hero_hire_price`, `Game::spawn_hero`, `pick_hero`, and `remove_unit(unit, Fate)`, which every place that takes a unit out of the game uses so the hero entity follows. A wizard may serve any number of heroes (`divergences.md`). |
| `unit/experience.rs` | Hero level path (`hero_xp`), Armsmaster training by the ability ids `armsmaster` and `super_armsmaster` (heroes themselves are not trained, as in the original). |
| `battle/spoils.rs`, `feature/lair.rs` | Fame for fallen heroes and their items to the winner (task 221); `Loot` carries items and a freed prisoner. |
| `player/offer.rs`, `spell/hero.rs`, `battle/spells/unit_cast.rs` | The hero, mercenary and merchant offers, the five hero spells, and the casting of heroes and items in battle. |
| `help.ron` topics 133, 142, 205, 207, 208, 211, 212, 240, 243, 246, 357, 515 to 526, 532, 533, 697, 702, 704, 765, 802 | The hero and item help. |

## 2. How the original works

### 2.1 Items

- **Record.** `s_ITEM`, 50 bytes (`MOM_DAT.h`): name[30], icon (`ITEMS.LBX` entry), slot type, item type,
  cost, attack, to hit, defense, moves in half-moves, resistance, spell skill, spell save, embedded
  spell and its charges, a 32-bit power mask. `ITEMDATA.LBX` has 250 of them, each followed by six
  bytes and a flag. The five bytes are the book need of each power of the item in a scrambled position, not a need per realm (task 244: 137 of the 159 items with bytes carry exactly the needs of their powers, none in a realm order). `Make_Item` compares them per realm with the wizard's books, which is a bug: books limit only what a wizard may craft. Ours drops the bytes and ignores books when an item is made (`divergences.md` section 10). The flag (`Make_Item` leaves flagged records out of the Enchant Item grade and unflagged ones out of the Create Artifact grade).
- **Types** (10): sword, mace, axe, bow, staff, wand, misc, shield, chainmail, platemail. **Slot
  types** (6, `e_ITEM_SLOT_TYPES`): 1 sword (sword, mace, axe), 2 bow, 3 sword or staff, 4 staff
  (staff, wand), 5 armour (shield, mail), 6 misc. An item's slot type is set by its type
  (`Create_Item_Record`); a hero's three slots depend on its hero type (`Hero_Slot_Types`,
  `Spells130.c`): warrior 0 = sword, armour, misc; type 1 = bow, armour, misc; type 2 = sword-or-staff,
  armour, misc; mage 3 = staff, misc, misc. An item fits a slot of its own type; slot 3 also takes
  types 1 and 4, slot 2 also takes type 1 (`ItemScrn.c:Move_Item`).
- **Powers** (`ITEMPOW.LBX`, 66 records of 30 bytes: name[18], item types allowed as a bit mask, cost,
  group, realm, books needed, power mask): 0 to 32 are the plain bonuses (attack 1 to 6, defence 1
  to 6, to hit 1 to 3, movement 1 to 4, resistance 1 to 6, spell skill 5/10/15/20, spell save -1 to -4),
  33 to 64 the special powers (Flaming, Lightning, Chaos, Destruction, Vampiric, Death, Power Drain,
  Holy Avenger, True Sight, Resist Elements, Elemental Armour, Giant Strength, Stoning, Phantasmal,
  Guardian Wind, Haste, Resist Magic, Magic Immunity, Wraith Form, Cloak of Fear, Bless,
  Righteousness, Invulnerability, Endurance, Planar Travel, Lion Heart, Water Walking, Regeneration,
  Pathfinding, Merging, Flight, Invisibility), 65 is Spell Charges. Each special power needs a realm
  and a number of books (the book count of the crafting wizard).
- **Crafting** (`ItemMake.c`). Cost = base by type (100 for sword, mace, axe, bow, shield, chain; 300 for
  staff and plate; 200 for wand; 50 for misc) + the cost of each power (a plain power on a misc item
  costs double) + for charges `spell cost * charges * 20`. Up to four powers; at most one of each
  group. Enchant Item allows only powers costing 200 or less; Create Artifact all. The cost is
  paid as the spell's casting cost, over turns like any spell; the finished item waits in a reserved
  record per wizard (`_ITEMS[136 - player]`, `SBookScr.c`) until the cast is done, then goes to the award list. The finished item's `cost` field
  is what the anvil and merchants use.
- **Where items live.** Three per hero, four in the fortress vault in the original (`Vault_Items[4]`; ours has no limit and the screen scrolls, task 334), at most six
  waiting after a reward (`GAME_AwardItems`). The item screen moves them; the anvil destroys one for
  half its cost in mana (help 246). A 138-record table holds every item in the game (137 + one
  scratch slot).
- **In battle** (`COMBINIT.c:Battle_Unit_Item_Stats`). Every item adds its defence, resistance and
  movement. Attack and to hit go to melee for sword, mace, axe and misc; to the ranged attack for a
  bow with a missile attack, a staff or wand with a magic ranged attack, a thrown axe and misc.
  Flaming adds 3 and Giant Strength 1 to melee (Flaming 3 to ranged too). Spell skill adds to the
  hero's mana. A shield gives the large-shield ability. The powers become unit enchantments
  (Cloak of Fear, Wraith Form, Regeneration, Pathfinding, Water Walking, Resist Elements, Elemental
  Armour, Endurance, Invisibility, Flight, Resist Magic, Guardian Wind, Magic Immunity, True Sight,
  Bless, Lion Heart, Planar Travel, Righteousness, Invulnerability; Holy Avenger gives Bless),
  Haste becomes the combat effect, and Vampiric, Lightning, Destruction, Chaos, Death, Power Drain,
  Holy Avenger, Phantasmal and Stoning become attack attributes (life steal, armour piercing,
  destruction, doom, death touch, power drain, dispel evil, illusion, stoning touch) on the melee or
  the ranged attack. Spell save lowers the resistance of what the hero's own spells hit. The same
  item enchantments drive overland movement (`UnitMove.c`: Pathfinding, Wraith Form, Water Walking,
  Flight, Wind Walking).
- **Embedded spell** (charges). Only `Items[0]` (the first slot) is read in battle: its spell can be
  cast `embed_spell_cnt` times by the hero without mana (`COMBINIT.c`, `CMBMAGIC.c`).
- **Loot and loss.** A hero that dies (battle, or a banishing spell) leaves its items in the
  winner's award list; dismissed, they are destroyed. Banish, Cracks Call, Disintegrate, Dispel
  Evil, Holy Word and Unsummon destroy the hero and its items (manual, p. 79).
- **Merchant** (`NEXTTURN.c:Determine_Offer`). Each turn the human has `2 + fame / 25` percent (doubled
  with Famous, then capped at 10) to be offered a random item (`Make_Item(0, ...)`; the 12 books it passes in every realm
  are a bug's workaround we do not need: books play no part) at three times its cost, half for a Charismatic wizard.

### 2.2 Heroes

- **35 hero types** (`UNITTYPE.c` rows 0 to 34, cost 100 to 600, upkeep 0 to 10 gold). 0 to 23 are heroes, 24 to
  33 champions (`Pick_Random_Hero`), 34 is Torin the Chosen, who is summoned by Incarnation only.
- **A roster per wizard.** (Ours: one set of hero entities for the game, see the note at the top.) At game start `Init_Heroes` (`INITGAME.c`) gives every wizard its own
  record for each of the 35 types: ability flags, casting skill and four innate spells from the
  premade table (`NewGame.c:_hero_premade_table`: picks count, pick type, fixed abilities, skill,
  spells), then spends the "picks" with `Random(14)` draws: each draw rolls one of leadership,
  legendary, armsmaster, blademaster, might, constitution, arcane power, prayermaster, casting skill,
  noble, charmed, lucky, agility, sage; a second hit on a martial or magic ability makes it the
  super version. Warrior and mage picks are separate budgets; some abilities need the right kind
  of hero. So the same hero type has different abilities in each wizard's game.
- **Level and availability** (`_HEROES2[player]->heroes[type].Level`). 0 or more: free to be hired,
  at that level (a dismissed or fallen hero keeps its level); -20: in service, summoned or gone for
  good (Disintegrate and the like); other negatives: see 2.4. Hire price is
  `cost + level * cost / 4`, half for a Charismatic wizard (`HIRE.c:Hire_Hero_Popup`). A hero is
  only offered when `(cost - 100) / 10 < fame` (`NEXTTURN.c:Pick_Random_Hero`), and the priestess
  and paladin need a life book, the black knight and necromancer a death book.
- **Six at most.** `NUM_HEROES = 6` slots; every spell and offer checks `Hero_Slot_Open`. (Ours has no limit: `divergences.md`.)
- **Names.** `NAMES.LBX` has 200 names of 13 characters, read at `player * 35 + type` for the
  computer wizards. The human wizard names a hero at hire (`GUI_InHeroNaming`).
- **Offers** (`Determine_Offer`). Per wizard each turn, hero: `(3 + fame / 25)`, doubled with Famous, capped at
  10, plus 10 for computer wizards, divided by `(heroes + 1) / 2 + 1`; mercenaries: `1 + fame / 20`
  with the same doubling and cap; items: above. Mercenaries (`CITYCALC.c:Generate_Mercenaries`): level
  from `Random(100) + fame` (above 90: 3, else 1: the code tests "above 90" twice, so level 2 never
  comes up), count from a second such roll (above 90: 3, above 60: 2, else 1), price
  `cost * (3 + level) / 2` per unit, half for a Charismatic wizard.
- **Summon Hero, Summon Champion, Incarnation, Resurrection, Recall Hero** (`Spells132.c`): the first
  two pick with `Pick_Random_Hero(zero cost)` and fail with six heroes (ours has no limit, `divergences.md`); Resurrection lists the dead
  heroes; Recall Hero moves a hero to the fortress, in battle too.
- **Upkeep and income.** Gold upkeep is the unit's own, none for a Noble hero; a Noble hero
  brings in 10 gold a turn (`CITYCALC.c`). Torin the Chosen has no gold upkeep but costs 12 mana a
  turn, and is not Noble unless the roll gives it to him like any hero. Fame is subtracted from the
  gold upkeep of the whole army. A Sage hero adds `Level * 3` (super: `Level * 9 / 2`) research points.
- **Level bonuses of a hero in battle** (`Battle_Unit_Level_Stats`, the hero branch; each line only
  for attacks the unit has): level 1 resistance, melee, ranged, defence; 2 to hit, resistance, melee,
  ranged; 3 resistance, melee, ranged, defence; 4 resistance, melee, ranged; 5 to hit, resistance,
  melee, ranged, defence; 6 resistance, melee, ranged; 7 resistance, melee, ranged, defence; 8 to hit,
  resistance, melee, ranged. Hit points +1 for every level 1 to 8 (`COMBINIT.c:290-340`). Task 174 ports
  these for all units; the hero table above is for it.

### 2.3 The abilities (formulas, `L` = Level, `x` = L + 1)

| Ability | Normal | Super | Where it acts |
|---|---|---|---|
| Leadership | melee `x / 3` for all friendly normal units (ranged half) | `x / 2` | battlefield, best hero only (`Combat.c:13361`) |
| Legendary | fame `3x` | `4.5x` | wizard's fame (`Player_Fame`: the code is commented out) |
| Blademaster | to hit `x / 2` | `3x / 4` | the hero |
| Armsmaster | `2x` xp to other units on the square each turn | `3x` | end of turn; best hero only |
| Constitution | hit points `x` | `3x / 2` | the hero |
| Might | melee `x` | `3x / 2` | the hero |
| Arcane Power | magic ranged attack `x` | `3x / 2` | the hero, only with a magic ranged attack |
| Prayermaster | resistance `x` for all friendly units | `3x / 2` | battlefield, best of this and Resistance to All |
| Agility | defence `x` | `3x / 2` | the hero |
| Sage | research `3L` | `9L / 2` | the wizard's research |
| Lucky | +10% to hit, +10% to block, +1 resistance; whoever attacks it has -10% to hit | | the hero |
| Charmed | never fails a resistance roll | | the hero |
| Noble | no upkeep, +10 gold (not Torin by default) | | the wizard's treasury |
| Casting skill | mana `(skill + 1) * x * 5 / 2` (plus item spell skill); half of it per hero at the fortress adds to the wizard's skill that pays over turns | | the hero in battle (task 51) |

## 3. New self-contained systems

Each is a module in `f4x-core` with plain data, free functions over what they need and a small read
handle where the economy and unit handles already work that way. None adds a method to `Game`;
`Game` only orchestrates (commands, events, saving).

### 3.1 `items` (what an item is)

```rust
pub enum ItemKind { Sword, Mace, Axe, Bow, Staff, Wand, Misc, Shield, Chainmail, Platemail }
pub enum SlotKind { Sword, Bow, SwordOrStaff, Staff, Armor, Misc }      // the six slot types
pub struct Item {                       // an instance: crafted items are not in the content
    pub name: String, pub kind: ItemKind, pub icon: u16, pub cost: u32,
    pub bonus: ItemBonus,               // attack, to_hit, defense, moves (half-moves), resistance,
                                        // spell_skill, spell_save
    pub powers: Vec<PowerId>,           // special powers (ids into the content's power table)
    pub charges: Option<(SpellId, u8)>,
}
impl Item { pub fn slot_kind(&self) -> SlotKind; }
pub fn fits(slot: SlotKind, item: &Item) -> bool;                       // 2.1 rules
pub fn hero_slots(hero_type: HeroKind) -> [SlotKind; 3];                // Hero_Slot_Types
pub fn modifiers(item: &Item, ranged: RangedKind) -> Vec<Modifier>;     // Source::Item; attack to melee or ranged by kind
pub fn granted_abilities(item: &Item) -> Vec<String>;                   // the power -> unit ability table
pub fn attack_traits(item: &Item, side: AttackSide) -> AttackTraits;    // life steal, doom, ...
pub fn mana_value(item: &Item) -> u32;                                  // cost; anvil gives cost / 2
```

`ItemDef` in the content becomes the same shape (`Item` is its `to_instance`), so a prefab item is a
copy. The `ItemSlot` list of `HeroDef` is replaced by `HeroKind` and `SlotKind` (done, task 218: mage
heroes have staff, misc, misc). This answers the open question of `design.md` section 13: **an item is data with
bonuses plus a list of power ids; its effects reach the rules through `Source::Item`, so the stat
engine (`rules/stats.rs`) and the ability list need no special case.**

### 3.2 `item_powers` (the power table)

`content/item_powers.ron` from `ITEMPOW.LBX`: id, name, group, cost, kinds allowed (a set), realm,
books needed, and what it does (a stat modifier, an ability, an attack trait, or nothing: Merging,
Spell Charges). One table drives the crafting screen, random items, the item pop-up and the
battle code.

### 3.3 `crafting` (Enchant Item, Create Artifact, random and prefab items)

```rust
pub struct ItemDesign { pub name: String, pub kind: ItemKind, pub icon: u16,
                        pub powers: Vec<PowerId>, pub charges: Option<(SpellId, u8)> }
pub enum Craft { EnchantItem, CreateArtifact }                          // power cost limit 200 / none
pub fn base_cost(kind: ItemKind) -> u32;
pub fn cost(design: &ItemDesign, rules: &Rules) -> u32;                 // the formula of 2.1
pub fn check(design: &ItemDesign, craft: Craft, magic: &Magic, rules: &Rules) -> Result<(), CraftError>;
pub fn build(design: &ItemDesign, rules: &Rules) -> Item;               // Create_Item_Record, incl. chain +1, plate +2 defence
pub fn random_item(rng: &mut GameRng, rules: &Rules, grade: Grade, value: u32) -> Item;
pub fn make_item(rng: &mut GameRng, rules: &Rules, grade: Grade, value: u32, taken: &mut Vec<String>) -> Item;   // items/make.rs; no books
```

The wizard's casting of the spell pays `cost` (Artificer: 50% less, help 697, a casting-cost reduction). The pending item is an entity held by `Crafting(player)` (the original's reserved record), so `Casting` stays as it is.
Random items follow `Create_Random_Item` (type 1 to 10, powers drawn from the plain 33, no repeated
group, stop at the value or the power count); prefab items follow `Make_Item` (250 records, each
once per game; the wizard's books are not looked at).

**Built (tasks 244 and 262)** in `item/make.rs`: `make_item(rng, rules, grade, value, taken)` with `Grade::{Any, Enchant, Artifact}` (the original's `Power` 0, 1, 2) and `random_item`, as `Make_Item` and `Create_Random_Item` with the fixes of `divergences.md` section 10 (value test, no book check, a power count that follows the kind). `taken` is `Game::prefab_taken` (saved). A prefab item is found by up to 50 draws; the random item draws a kind, then plain powers (one per group, at most 200 each for Enchant) until the count (Enchant 2; else 4 for sword, mace, axe, staff, wand, 3 for the rest), the value or 50 draws; no value means 700 to 1700. Icons are task 261.

### 3.4 `armory` (where a wizard's items are)

**Built (task 49, reworked in task 333).** There is no `Armory` component on the player: each item
is an entity whose `HeldBy` says where it is (`Hero(unit, slot)`, `Vault(player, slot)`, `Award(player)`,
`Crafting(player)`, `Offered(player)`), and `Armory` is the read of one wizard's vault and award list from those
entities (`item/armory.rs`, `item/entity.rs`). `place` puts a new item in the vault, else on the
award list, else it is lost; the anvil (`scrap_item`) gives mana; `move_item` moves between spots
with the rules of the item screen. Moving between heroes in different places is refused as in
`Check_Same_Location` (it costs 20 mana to a far hero); a hero may not be left to drown by taking
away its Water Walking or Flight (`Check_Wont_Drown`, `Game::drown_stranded`).

### 3.5 Heroes as entities (was `roster`)

**Built (tasks 219, 326 to 328).** The sketch of a per-wizard `Roster` of `HeroRecord`s is gone: the
game has one hero entity of each type (`hero/game.rs`), created by `kind::roll` when the game is
made. It holds `Hero` (the abilities and spells that were rolled, the name), `Experience` and a
`HeroState`: `Free` (offered, summoned or hired by anyone), `Serving` (it has the unit components
and an `Owner`), `Dead` (Resurrection brings it back) or `Gone` (Disintegrate, Banish and the like).
The hero type is the index into `Rules::hero_templates`; `kind::pick` takes a `Standing` (fame and
life and death books) and a `Class`; `Fate` is `Slain` / `Released` / `Gone`. Dead, free and gone are
separate states instead of the original's level coding (2.4). The old roster functions became:
`roll` (`kind::roll`), `candidates` and `pick` (`kind::pick`), `hire_cost`, `on_hired`
(`Game::spawn_hero`), `on_dead` (`Game::remove_unit`), `dead` (`Units::raisable_heroes`).

### 3.6 `hero` (the unit component and the abilities)

**Built (task 219)** in `hero/mod.rs`: `Hero {name, abilities, casting_skill, spells}` and `HeroAbilities` with `Tier`; the items stay in their own `HeroItems` component, and the effect functions below are task 47. Sketch:

```rust
pub struct Hero { pub kind: usize, pub name: String, pub abilities: HeroAbilities,
                  pub casting_skill: u8, pub spells: [Option<SpellId>; 4] }   // the items are entities
pub struct HeroAbilities { pub leadership: Tier, pub legendary: Tier, ... pub lucky: bool, pub charmed: bool, pub noble: bool, pub female: bool }
pub enum Tier { None, Normal, Super }
// pure functions of (tier, level), one per row of the table in 2.3:
pub fn leadership(t: Tier, level: u32) -> i32;  pub fn blademaster(..); pub fn might(..); ...
pub fn modifiers(hero: &Hero, level: u32, ranged: RangedKind) -> Vec<Modifier>;  // might, agility, blademaster, constitution, arcane power, lucky, items
pub fn battlefield(heroes: impl Iterator<Item = (&Hero, u32)>) -> FieldBonus;     // best leadership, best prayermaster
pub fn research(heroes) -> i32;  pub fn fame(heroes) -> i32;  pub fn gold(heroes) -> i32;
```

The abilities live on the hero entity from the start of the game and stay with it through dismissal
and death, so the unit carries what it needs and `unit_modifiers` / `unit_abilities` read the
component: the ids `armsmaster` and `super_armsmaster` that `unit/experience.rs` reads come out of
`HeroAbilities` the same way. Task 47's 13 abilities became the table above, not 13 ability records
in `abilities.ron` (the help text for them stays there for the unit sheet).

### 3.7 `fame` (task 72)

`fame::total(base, heroes, just_cause) -> i32` (base + legendary + 10 for Just Cause) replaces the
plain `Player.fame` where `player/finance.rs` and the offers read it; `Player.fame` stays the earned part.
Gains and losses stay where they are (`spoils.rs`, conquest).

### 3.8 `offers` (hire, mercenary and merchant offers each turn)

The offers are plain data, like the healing and treasury plans: `Offer::Hero`, `Offer::Mercenaries`
and `Offer::Item`, rolled by `Determine_Offer` with the draw order kept. Commands `AcceptOffer` and
`RejectOffer` apply the choice (gold, a hero or unit created at the fortress, an item to the
armory). Computer wizards accept by the AI's rule. Mercenaries are not heroes; they ride on this
system because they share the schedule, fame and the pop-up (task 143).

**Built (task 220)** in `player/offer.rs`, with these differences from the first sketch (an `OfferView`
trait): there is none (the chances, prices and the computer's answers are plain functions, tested without a game, and
`Game` rolls the offers); `Offer::Mercenaries` carries the unit's id and a 0-based level; an offer
waits in `Player::offers` (saved) until `AcceptOffer { player, index, name }` or `RejectOffer`, and
lapses at the next turn change. `determine_offers` runs last in `process_turn`. Every wizard
meets the merchant; the computer wizards answer in `f4x-ai/src/offer.rs`. The console has `offers`,
`accept`, `reject` and the override `offer <hero|mercenaries|item>`; the client shows a box over the map
(`offer_prompt.rs`, `--offer KIND` with `--shot`). The fixes of the original's bugs are in
`divergences.md` section 11.

### 3.9 Hero spells (resolve through the existing effect host)

**Built (task 48)** as one hook for each of the four overland spells in `spell/hero.rs` (the effect host lends out the game's generator, so the pick draws from the one the hook is given) and `Kind::Recall` in `battle/spell.rs`; the `Heroes<'a>` handle was not needed. Sketch:

New effect primitives (`rules/exec.rs`): `SummonHero(class)`, `Incarnate`, `Resurrect`, `RecallHero`,
`CraftItem(Craft)`. Each calls the hero and item functions (`Game::spawn_hero`, `pick_hero`, `give_item`). Cracks Call, Banish,
Disintegrate, Dispel Evil, Holy Word and Unsummon already exist as spells: they gain "the hero's
items are destroyed" and, for Disintegrate and the like, "gone for good" in the removal code.

## 4. What changed in `Game` (all built)

- **Components and state.** `Hero`, `HeroState` and `Experience` on the hero entities, `Item` and
  `HeldBy` on the item entities (the player entity has no roster or armory; the item being crafted
  is an entity held by `Crafting(player)`). `lair::Loot` carries items and a prisoner flag.
- **Commands.** `AcceptOffer`, `RejectOffer`, `MoveItem`, `ScrapItem`, `CraftItem`
  (`AcceptOffer` carries the hero's name), through the dispatcher like `found_wizard`.
- **Events.** `HeroJoined`, `HeroSpellFailed`, `ItemsAwarded`, `ItemMade`, the offer events.
- **Turn.** `determine_offers` runs last in `process_turn`; the Noble gold, Sage research and
  Legendary fame add to the existing `Income` and `Treasury` inputs (`wizard/magic_income.rs`,
  `player/finance.rs`).
- **Removal.** Every place that takes a unit out of the game goes through `Game::remove_unit(unit,
  Fate)`, which turns a hero's body into `Free`, `Dead` or `Gone` and sends its items to the winner,
  the armory or oblivion.
- **Save.** `game/save.rs` stores the hero and item entities with their serials.
- **Player view and AI.** `PlayerView` shows a wizard its own heroes and items. `f4x-ai` accepts hero
  offers by `AI_Accept_Hero`, equips by item fit (`f4x-ai/src/offer.rs`) and casts the heroes' spells.
- **Console and client.** Console commands mirror the game's commands; the client has the army list,
  unit window, item screen, crafting screen and offer pop-ups (tasks 138 to 143, 260).

## 5. What changes in the battle

- **`BattleUnit`** gets: `hero: Option<HeroBattle>` (name, level, casting skill, innate spells,
  item charges `Option<(SpellId, u8)>`), `item_attack: AttackTraits` for melee and ranged,
  `melee_to_hit` and `ranged_to_hit` (item to-hit goes to the matching attack only), and the
  mana from `(skill + 1) * x * 5 / 2 + item spell skill`. `battle_unit_from` (battle/game.rs) calls
  `hero::modifiers` through `unit_stat`, so most of this is the stat engine finding the new sources;
  the code there only adds the pieces that are not stats (charges, traits, mana).
- **Battlefield step.** `FieldBonus` (battle/field.rs) already holds the per-side battlefield spells;
  `hero::battlefield` adds leadership (best hero per side, melee and half of it to ranged, for
  normal units only: not undead nor channeled) and prayermaster (best of it and Resistance to All).
  The original has this as one routine (`Combat.c:13243`); when it runs
  again during a battle (a hero dying, a spell) I have not checked.
- **Casting.** A hero casts from its innate spells and its item's embedded spell with its own mana
  (shots at 3 mana, task 51), or the item's charges, with the cost rule of the battle wizard
  (`battle/casting.rs`). The item's spell save lowers the resistance of what the hero's own spells
  hit. Charges stay on the item after the battle (`Magic`-side bookkeeping: the item is in the
  hero component, the battle writes the new count back).
- **Spoils** (`battle/spoils.rs`): `Spoils` gains `items: Vec<Item>` (the loser's heroes that fell,
  their three slots) and the heroes lost; the game then places them in the winner's
  armory (`give_item`). A neutral winner takes nothing. Fame for heroes is unchanged.
- **Fate.** `Fate::Gone` is what Disintegrate, Banish and the other removal spells leave (items
  destroyed, hero gone for good); `Slain` leaves the hero Dead (revivable, items to the winner).
- **Recall Hero.** A battle action: the hero leaves the field to the fortress at once (status like
  `Fled`, with its items), cost 20 mana.
- **Unit abilities from items.** `unit_abilities` adds `item::granted_abilities` (large shield from a
  shield; flight, pathfinding, water walking and wraith form overland; invisibility, regeneration,
  cloak of fear, etc. as the existing ability ids), so `battle_unit_from` and movement need no
  change beyond reading the same list.

## 6. Where the assets are and what to import

Paths are in `reference/mom-classic/Master of Magic Official Release/` (the LBX files); art goes
through `extract-art` (already extracted to `assets-local/art/<lbx>/<entry>_<frame>.png`).

| What | File and entry | Status |
|---|---|---|
| Item rules (250 prefab items, powers, charges, books, the flag) | `ITEMDATA.LBX` entry 0: 56-byte records, 4-byte header | Importer reads only the five stat bytes and drops powers, spell skill, spell save, charges, books and flag. **Extend `import_items`**, write `content/items.ron` with all 250 (no skipping) and `content/item_powers.ron` from `ITEMPOW.LBX` (66 records of 30 bytes after a 4-byte header). |
| Item type, slot type | `ITEMDATA` bytes 0x20 and 0x21 | Today only the type is used; keep both. |
| Item icons | `ITEMS.LBX` (116 entries; 16x16, `assets-local/art/items/`) | Extracted. Index = the `icon_idx` byte; crafting picks by type from ranges (sword 0 to 8, mace 9 to 19, axe 20 to 28, bow 29 to 37, staff 38 to 46, chain 47 to 54, plate 55 to 61, shield 62 to 71, misc 72 to 106, wand 107 to 115; the original gives the sword White Lightning I a mace's picture, 17). |
| Item screen furniture (slot icons, anvil, buttons) | `ITEMISC.LBX` (36 entries), `ARMYLIST.LBX` entries 5 to 8 (background, outline, alchemy and OK buttons) | Extracted. |
| Crafting screen | `SPELLSCR.LBX` entries 13 (background), 14 to 23 (weapon buttons), 24 (OK), 25 to 34 (highlighted buttons), 35 and 36 (arrows), 37 to 41 (charges) | Extracted (`spellscr`). |
| Hero portraits | `PORTRAIT.LBX` (40 entries; the `bld1` column of the unit table for heroes is the entry) | Extracted. **Importer: read `bld1` as portrait** for the 35 hero rows. |
| Hero type (slots) | unit table column `bld2` of the 35 hero rows (`UNITTYPE.c`): 0 warrior (19 heroes), 1 bow (Dervish, Huntress, Ranger, Elven Archer), 2 sword-or-staff (Warrior Mage, row 24, Chaos Warrior), 3 mage (Sage, Druid, Magician, Wind Mage, Witch, Warlock, Illusionist, Priestess, Necromancer) | Not imported yet. **Add `hero: (kind: ..., portrait: ...)` to `HeroDef`.** |
| Hero ability roll tables | `NewGame.c:_hero_premade_table` (35 rows: picks count, picks type, fixed ability mask, casting skill, four spell numbers). `HERODATA.LBX` (one entry of 150 records of 10 bytes: five copies of 30 hero types, so one set a wizard) is **not read by ReMoM or, as far as can be told, by the game**: each record holds a 32-bit-looking ability field (bytes 2 and 3), a casting skill of 5, 10 or 15 (byte 4) and up to four spell numbers (bytes 5 to 8) that differ from the premade table's, so it is probably a leftover of an earlier design of the same data. Nothing in the game needs it (task 218). | **Import from `NewGame.c`** into `content/heroes.ron` (ability bit names from `HSA_*`; spell numbers need the importer's spell index map). |
| Hero ability flags and icons | `HSA_*` (`MOX_DEF.h`), `LVLMAKE.c:USW_HeroAbilities` (flag, icon, help), icons in `SPECIAL.LBX` / `SPECIAL2.LBX` | Extracted; the importer needs the flag names. |
| Hero names | `NAMES.LBX` entry 0: 200 names of 13 characters, indexed `player * 35 + type` | **Decision needed:** names are not rules. Treat like the city names: invented defaults in `content/`, the original's in `content-local/` (see `local-assets.md`). |
| Hero level names and xp | `hero_xp` in `level_paths.ron` (Hero 0, Myrmidon 20, Captain 60, Commander 120, Champion 200, Lord 300, Grand Lord 450, Super Hero 600, Demi-God 1000) | Imported already; check against `TBL_Experience` in task 174. |
| Hire pop-up art | `HIRE.LBX` (5 entries: banners, merchant buttons, name box) | Extracted. |
| Hero figures in battle and overland | `FIGURES1..16.LBX` | Extracted; mapping is the units' (done). |
| Help text | topics listed in section 1 | In `content-local/help.ron`. |
| `ITEMMAKE.EXE` | an executable beside the data files | Not needed (ReMoM has the screen's logic in `ItemMake.c`). |

## 7. Help, original and ReMoM disagree

`Help` is the in-game help, `Original` is what the code does where ReMoM's reading of the
disassembly is clear, `Ours` is what we have or will do. Items marked **Check** rest on a ReMoM
comment or one read of the code and need a second look before they are built (the
`help-vs-implementation-conflicts.md` rule).

| # | Topic | Help | Original code (ReMoM) | Proposal |
|---|---|---|---|---|
| H1 | Leadership [515] | "one per 2 levels" | Code: `x / 3` (super `x / 2`); the manual text copied into `Combat.c:13233` says "one per three levels" | Follow the code and the manual (`x / 3`); correct the help note (`HELP_CORRECTIONS`). **Decided** (31 October 2026, B52): the code; the saved wiki page agrees. Torin is a Life creature too and gets none of his own (B65). |
| H2 | Legendary [516] | +3 fame per level; more and stronger mercenaries; **cheaper hiring** | The fame lines are commented out (`CITYCALC.c:Player_Fame`, already B12); mercenary strength uses the wizard's fame, so legendary heroes help only through fame; nothing lowers the hiring price | Follow the help for the fame (`3x`, super `4.5x`); no price cut (not in the code). **Decided** (31 October 2026, B53): the wiki agrees. |
| H3 | Constitution [519] | "adding +1/level to the hero's **defense**" | Adds hit points `x` (super `3x / 2`) | Hit points; fix the help. |
| H4 | Agility | No help topic in 515 to 526 | Exists: defence `x` (super `3x / 2`), rolled by `Init_Heroes` | Implement from the code; write a help note. |
| H5 | Sage [522] | 3 points per level (super 50% more) | Research `Level * 3` and super `Level * 9` (`CITYCALC.c:951`) but the unit sheet shows `x * 3` and `x * 9 / 2`: 0-based against 1-based, and a factor 3 against 1.5 for the super | Use `x * 3` and `x * 9 / 2` (sheet, help). **Decided** (24 October 2026): super is 9/2 a level, not 9. |
| H6 | Armsmaster [518] | "adds 2 experience to all units stacked with the hero, per level" | `2x` (super `3x`), only to units that are **not** heroes, best hero only | Already ours (task 27); keep. |
| H7 | Prayermaster [523] | +1 per level to all resistance rolls for all units in the battle | `x` (super `3x / 2`), the best of it and Resistance to All, not additive | Follow the code. |
| H8 | Lucky [524] | +1 to all the hero's rolls (attack, defence, resistance) | To hit, to block and resistance +1 (not attack strength) | Follow the code: exactly +10% to hit, +10% to block and +1 resistance for the hero, and an attacker has -10% to hit against it. **Decided** (24 October 2026). |
| H9 | Charmed [525] | never fails a resistance roll | Same, in `Combat.c:12151` | As help. |
| H10 | Noble [526] | no money cost, +10 gold a turn | Same (`CITYCALC.c:491`, `Unit_Gold_Upkeep`), also Torin the Chosen has no upkeep | As help, but not for Torin: he is not Noble by default (the roll may make him one) and costs 12 mana a turn instead of gold. **Decided** (24 October 2026). |
| H11 | Hero level bonuses [533 and the level pages] | levels and xp thresholds | The hero branch above (2.2) differs from the normal-unit table | Task 174 ports both; this doc fixes the hero table. |
| H12 | Famous [702, 765] | "doubles the chance of a wizard hiring heroes, recruiting mercenaries and buying magical items" | Chance doubled **and then capped at 10 percent** (`SETMAX` takes the smaller value); heroes offered also need `(cost - 100) / 10 < fame` | Follow the code; the cap makes Famous matter only at low fame. |
| H13 | Charismatic [704] | half price for heroes, mercenaries and items | Heroes: half of `cost + level * cost / 4`; items: the affordability test is made before halving ("wrong order of operations" in a ReMoM note); mercenaries: `Total_Cost /= 2` | Half price, test after halving. **Decided** (31 October 2026, B56): the wiki says Insecticide fixed the test for heroes and mercenaries; ours tests after halving for all three. |
| H14 | Summon Hero / Champion [208, 212] | "will not require any initial gold" | `WIZ_HireHero` with no price; the summoned hero still pays upkeep | As help. |
| H15 | Resurrection [133] | "if six other heroes have been hired the spell fails" | Same (checks `Hero_Slot_Open`) | As help. |
| H16 | Recall Hero [205] | "instantly summons the target hero to your summoning circle, removing her from the current combat" | not read yet | Battle only, as the spell data has it and the wiki says. Built (task 48): `Status::Recalled`, no flee roll. **Decided** (31 October 2026, B57): no overland cast; the animation is the Arcane picture, as the original's default. |
| H17 | Anvil [246] | half the base creation cost in mana | Same | As help. |
| H18 | Enchant Item [207] | "only powers that cost 200 mana or less" | `Itam_Make_Screen_Build_Weapon_Powers_List` takes a maximum cost for a power (the player's crafting); random items for Power 1 use cost 200 and **at most 2** powers; the player may pick four (`NUM_ITEM_POWERS`; the screen warns "only 4 powers may be enchanted into an item") | Limit each power as the help says; four powers at most. **Done** (task 50, `crafting::check`). Wiki agrees (B58). |
| H19 | Artificer [697] | cost of making magical items -50% | A 50 percent reduction of the crafting spells' casting cost, in the same function as the other books' reductions (`NEXTTURN.c:1343`) | As help. |
| H20 | Item power Merging | not in the help | Needs 22 books in `ITEMPOW.LBX`, so no wizard can craft it | Import it with the 22 as it is. |
| H21 | Item attack and to hit | "attack" on every item | Goes to melee or to ranged by item kind (2.1); to hit is per attack | Port the kind rule; the current import puts every bonus on `Melee`. **Done** (task 49, `items::modifiers`; the plain bonuses are the item's `ItemBonus`, the import's `Melee` effect is read as its attack). |
| H22 | Embedded spells | any slot | Only `Items[0]` is ever usable (`COMBINIT.c:511`, marked OGBUG) | Allow every slot (the bug is not copied); say so in `divergences.md`. |
| H23 | Spells of a dead hero's owner | item and hero spells | Several places use the current battle controller, not the hero's owner (OGBUG notes at `CMBMAGIC.c:543` and others) | Use the owner. |
| H24 | Fame and upkeep [765] | "decreases the total amount paid for troop maintenance by one gold per fame point" | `Player_Fame` + the upkeep subtraction (`CITYCALC.c:312`) | Ours (`player/finance.rs`); only the fame total changes with heroes. |
| H25 | Hero 24's name | not looked up | ReMoM's unit table names row 24 "Unknown" (23 is Warlock, 25 Illusionist) | "Unknown" is right: the hero is Mystic X the Unknown, so the name only looks odd. **Decided** (24 October 2026). |

The original's own bugs listed in `content-local/OGBUG_INDEX.md` for these areas
(`Combat.c:3347` missing `item_pool_in_process`, `Combat.c:11985` slot search without a return,
`Spells129.c:1984` empty slots set to 0, the `Combat.c:18152` hero-spell scan off by one, `Combat.c:12529`
Power Drain item ignored) are not copied; each gets its normal rule.

## 8. Open questions

1. **Hero names** (decided): placeholders in `content/` (the unit's own name) until the original's are imported; the original's go to `content-local/hero_names.ron` as the city names do (task 218). The hire box lets the player name the hero (task 143).
2. **Item entities, not ids.** Crafted items cannot be content ids, so each item is an entity with
   its full `Item`; the vault, hero slots, offers and loot are one kind of thing (`HeldBy`).
3. **Dead heroes** (2.2, 2.4). The design replaces the original's level coding with an explicit
   `HeroState`.
4. **Whether a slain hero is revivable in the original** (task 223, a Check row): ReMoM does not
   show what a dead hero's level is stored as in every path (`Kill_Unit` keeps the level,
   `Spells132.c:590` looks for negatives other than -20), and the help says "regardless of the length
   of time". Ours: a slain hero is `Dead` and Resurrection brings it back.
