# Wizards, mana, research and spells

How the magic layer works. Code: `crates/f4x-core/src/{magic,cast}.rs`; screen:
`crates/f4x-client/src/magic_screen.rs`. Formulas follow the original game's code (read from the
ReMoM source) unless noted. For where wizard state sits in the game, see
[`ecs-structure.md`](ecs-structure.md); for the turn order, [`game-flow.md`](game-flow.md) §4.

## 1. The wizard

A wizard is not an entity. It is a `Magic` record inside `Player`, plus the mana reserve
(`Player::mana`):

| Field | Meaning |
|---|---|
| `books` | spell books per realm id (1 to 10) |
| `retorts` | special abilities, an enum: Alchemy, Warlord, the three masteries, Channeler, Archmage, Artificer, Conjurer, Sage Master, Myrran, Divine/Infernal Power, Famous, Runemaster, Charismatic, Mana Focusing, Node Mastery |
| `skill_points` | invested casting skill; the skill itself is derived (below) |
| `mana_pct`, `research_pct`, `skill_pct` | how power is split (sum 100; starts 33/34/33) |
| `known`, `researchable` | spell sets (disjoint) |
| `researching`, `research_remaining` | the chosen research and what is left to pay |
| `casting` | the overland spell being paid for, and the mana still owed |
| `global` | lasting spells on the wizard, with upkeep |
| `skill_left` | skill available this turn for instant casting |

Only some retorts have effects so far: Alchemy, Archmage, Artificer, Conjurer, Sage Master,
Runemaster, Mana Focusing and the three realm masteries (research and cost). The rest are stored
and saved but do nothing yet.

`Game::init_wizard(player, books, retorts)` sets one up.

## 2. Casting skill

Skill is a triangular function of invested points: skill *k* needs more than *(k-1)·k* points
(2, 6, 12, 20, 30...), plus 10 for an Archmage. A new wizard starts with the points that give
twice the book total (plus 10 for Archmage). Skill is the most mana a wizard can put into a spell
per turn, and the amount available for instant casting.

## 3. Power and its split

- **Power base** = mana from all the wizard's cities (citizens of some races, religious
  buildings, mineral ores) plus the **fortress city's** bonus: one per book, five more on Myrror.
  Magic nodes will
  add power once they can be owned.
- **Split:** mana share and skill share are `power × pct` rounded to nearest; research gets the
  rest, plus all research from city buildings. If research is set to 0%, its share goes to skill
  (or to mana when skill is also 0). Mana Focusing adds a quarter to mana.
- **Research bonus** (percent) for a spell: Sage Master 25; Conjurer 25 for summoning spells; a
  realm mastery 15; each book beyond seven in the spell's realm 10; Runemaster 25 for arcane.
- **Turn change:** mana is added to the reserve; research is subtracted from the spell being
  researched; skill points are added (an Archmage gets half as much again).

## 4. Spells and the library

A new wizard gets, for each realm with *n* books:

| Rarity | Researchable spells (counting known ones) |
|---|---|
| Common | 3, 5, 6, 7, 8, 9, 10, 10, 10, 10 for 1..10 books |
| Uncommon | 1, 2, 3, 4, 5, 6, 8 for 1..7 books; all from 8 |
| Rare | 0, 1, 2, 3, 4, 5, 6, 7, 9 for 1..9 books; all at 10 |
| Very rare | 0, 0, 1, 2, 3, 4, 5, 6, 7 for 1..9 books; all at 10 |

and starts knowing *n − 1* random common spells of the realm. All arcane spells except Spell of
Mastery and Spell of Return are researchable. Choices are random and come from the game's seeded
generator, so a seed gives the same wizard.

## 5. Research

`SetResearch` picks a researchable spell; the cost is the spell's `research_cost`. The choice
stands until the spell is learned (no switching). When the remaining cost reaches 0 the spell
becomes known (`SpellLearned`), and the player picks the next.

## 6. Casting overland

- **Cost:** the spell's `cost`; a spell that can also be cast in battle costs **five times** as
  much overland. Reductions (percent): each book beyond seven 10, realm mastery 15, Artificer 50
  for crafting spells, Conjurer 25 for summoning, Runemaster 25 for arcane.
- `CastSpell`: if the cost is within `skill_left` and the mana reserve it is paid and takes effect
  **at once** (and needs its target now). Otherwise it is paid over turns: at each turn change
  the wizard puts `skill_left` (then the base skill) into it, limited by the mana reserve. The
  last payment may be taken from the next turn's skill, as in the original. When it is paid, a
  spell without a target takes effect; one with a target emits `SpellReady` and waits for
  `ResolveCast`.
- `CancelCast` throws the spell away; mana already paid is lost.
- **Targets** are checked against the spell's target kind (own unit, enemy city, tile...).

## 7. What a spell does

`resolve_cast` runs the spell's `OnCast` effects through the effect executor against a real
effect host over the ECS (`GameHost` in `cast.rs`): damage and healing of units, summoning,
terrain and feature changes, resources, dispelling. A spell that lasts (permanent or timed) is
attached to its target: a city (`Enchantments`), a unit (`UnitEnchantments`) or the wizard
(`Magic::global`); its passive effects then apply as stat modifiers. Spells whose effects are
still the `unimplemented` hook are cast, cost mana, and report `SpellHasNoEffectYet`.

Known gaps: timed durations are not counted down yet; instant `Modify`/ability grants have no
home; battle spells exist only in data.

## 8. Upkeep

Each turn the wizard pays mana for lasting spells they cast (global, city, unit) and for units
with mana upkeep. If the reserve cannot cover it, things are given up newest-last until it can:
mana-upkeep units first, then unit, city and global spells (`EnchantmentLapsed`,
`UnitsDisbanded`). The original lets the player choose; this is the deterministic default.

## 9. Turn order

Within the turn change, after the treasury: magic power applied → research checked → upkeep paid
→ casting progress → completed spells resolve.

## 10. Client

M opens the magic screen: power and split (click a bar), research choices (click), the spell
book (click to cast), alchemy buttons, casting status. A spell with a target closes the screen
and waits for a click on the map; Esc cancels. The top bar shows mana and its net change.
Until there is a wizard-creation screen the human gets fixed picks (3 sorcery, 2 nature,
Conjurer) and the rival 3 chaos, 2 death, Warlord.

## 11. Differences from the original

- The fortress is a location on the player, not a building; the original also has the fortress
  city act as a building in the city screen. Banishment, the circle and the Spell of Return work
  (section 12); the aftermath of a wizard's defeat (neutral units and cities) does not.
- No heroes' casting contribution or sage heroes, no node power, no Evil Omens or other global
  modifiers on cost, no Spell of Return handling, no spell research cost for the Spell of Mastery
  (placeholder data), no battle casting.
- Imported eligibility 2 and 3 (overland-only spells that the original still costs ×5) are
  collapsed to overland-only here, so those spells cost less than in the original. To be fixed in
  the importer.
- Starting known spells are random; the original takes them from a fixed table per named wizard,
  or lets a custom wizard pick.

## 12. Fortress, summoning circle, banishment

Code: `crates/f4x-core/src/fortress.rs`. Each `Player` has `fortress` and `summon_at` (positions of
cities), `banished` and `defeated`.

- **Placing:** `Command::PlaceFortress` founds the wizard's first city and puts both the fortress
  and the summoning circle in it; the city's race becomes the capital race. Only once per wizard.
- **Power:** the fortress city adds one mana per book to the power base (five more on Myrror).
- **Summoning:** summoned creatures appear at the circle, and a summoning spell then needs no
  target. Without a circle (tests, odd setups) they appear where aimed.
- **Move Fortress** moves the fortress and the circle to another own city; **Summoning Circle**
  moves only the circle. Both are instant city-targeted spells that run engine hooks
  (`move_fortress`, `move_summoning_circle`); the importer maps the original spells to them.
- **Capture:** `Game::capture_city(city, winner)` (for the combat layer to call) changes the
  owner. Taking the fortress city **banishes** the wizard: fortress and circle are gone, the
  winner gets 5 fame and half the loser's mana, and the loser starts casting the **Spell of
  Return** (cost from the spell data). While casting it the wizard gets no power, mana, research
  or skill, and cannot cancel. Taking the last city **defeats** the wizard.
- **Return:** when the spell is paid it asks for one of the wizard's cities; that city becomes the
  fortress and circle and its race the capital race (`return_to_fortress`). It cannot be cast any
  other way.
- **Destroying:** `Game::destroy_city` refuses a fortress city and leaves it with at least one
  citizen, as in the original.
- Spell targets are now checked before mana is paid.

Not done: neutralising a defeated wizard's units and cities, removing their enchantments, the AI
resigning, and drawing the fortress as a building. See tasks 70 and 82.

## 13. Creating a wizard

`wizard.rs` holds the pick rules and `Game::create_wizard`; `wizards.ron` (written by the
importer, optional) lists the ready-made wizards. Eleven picks buy books (one each) and
retorts (one; two for Warlord, Channeler, Divine Power, Famous, Infernal Power; three for
Myrran), at most six retorts, with the prerequisites in `retort_available`. A realm with `n`
books lets the wizard choose `n - 1` common spells at the start (more are filled from
`RealmDef::default_spells`). Ready-made wizards skip the budget check (the original's Lo Pan
spends twelve). The client's creation screen is `create_screen.rs`.
