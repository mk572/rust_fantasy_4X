# Wizards, mana, research and spells

How the magic layer works. Code: `crates/f4x-core/src/{magic,magic_game,cast}.rs` (`magic.rs` is the wizard's own state and rules, `magic_game.rs` what needs the world: power, upkeep, commands, the turn; `architecture-plan.md`); screen:
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

Eight spells are on offer at a time (`Magic::candidates`, `RESEARCH_SLOTS`), the original's
`research_spells`: `refill_research` tops the offer up from `research_pool` (the original's
`Build_Research_List`: in each realm every researchable spell of its lowest rarity that is
still left, so commons come before uncommons; arcane spells in three steps, the first three, the
next five, the rest), picking at random and working the pool out afresh after each pick, then
sorts the offer by the time each would take. It runs when a wizard is made, every turn and when
a spell is learned. `SetResearch` takes one of the offered spells; the cost is its
`research_cost`, and the choice stands until the spell is learned (no switching). When the
remaining cost reaches 0 the spell becomes known (`SpellLearned`), leaves the offer, and the
player picks the next. Not done: the Spell of Mastery joining the offer when nothing else is
left (task 61).

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
home; battle spells are cast in battles (`combat.md` section 8).

## 8. Upkeep

Each turn the wizard pays mana for lasting spells they cast (global, city, unit) and for units
with mana upkeep. If the reserve cannot cover it, things are given up newest-last until it can:
mana-upkeep units first, then unit, city and global spells (`EnchantmentLapsed`,
`UnitsDisbanded`). The original lets the player choose; this is the deterministic default.

## 9. Turn order

Within the turn change, after the treasury: magic power applied → research checked → upkeep paid
→ casting progress → completed spells resolve.

## 10. Client

The screens follow the original's (`magic_screen.rs`, `spellbook.rs`, `bigbook.rs`):
- **M, the Magic button:** the three staves (drag one; the lock above holds its share, and the
  others give and take as `staff_adjust` says), the income under each, the casting skill,
  magic reserve and power base, what is being cast and researched, the summoning circle's city,
  the gems of the other wizards (grey until diplomacy: tasks 63, 83), the overland enchantments
  of every wizard (click your own to turn it off: `Command::TurnOffSpell`), and the Alchemy
  window (the slider and the arrow that turns the exchange round).
- **V, the Spells button:** the small spell book, spells by realm six to a page with the cost and
  a symbol for each turn it takes; click to cast (a spell with a target is aimed on the map; Esc
  cancels); casting another spell asks before giving up the one under way.
- **F3:** the Apprentice's big book, every spell the books allow (greyed when not known yet), then
  the two pages of spells on offer for research, with what each does (the help text, local only).
- **At the start of a turn with nothing being researched** (from the second turn, as in the
  original) the big book opens on the research pages and asks which to take; a spell just
  learned is announced first.

## 11. Differences from the original

- The fortress is a location on the player, not a building; the original also has the fortress
  city act as a building in the city screen. Banishment, the circle and the Spell of Return work
  (section 12); the aftermath of a wizard's defeat (neutral units and cities) does not.
- Heroes add to the casting skill that pays over turns and sage heroes to research (task 51); no node power, no Evil Omens or other global
  modifiers on cost, no Spell of Return handling, no spell research cost for the Spell of Mastery
  (placeholder data). Casting in battle is in `combat.md` section 8.
- The importer keeps the original's eligibility (`spell_contexts` in `import_mom.py`): 0, 2 and
  3 are castable in a battle and on the map, and `SpellDef::overland_x5` makes the map cost five
  times as much (Flight, Bless, Resist Elements, Wall of Fire and the like, as well as the
  spells of eligibility 0). Eligibility 4 (Word of Recall, Disenchant True and Area) is in the
  original's battle list too, but stays overland-only here until their battle effects exist
  (task 159).
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

`wizard.rs` holds the pick rules and `Game::create_wizard`; `content/wizards.ron` lists the
ready-made wizards' books and retorts (game rules, committed); their names and descriptions come from the
importer's local `wizard_texts.ron`, and without it a wizard is named after its id. Eleven picks buy books (one each) and
retorts (one; two for Warlord, Channeler, Divine Power, Famous, Infernal Power; three for
Myrran), at most six retorts, with the prerequisites in `retort_available`. A realm with `n`
books lets the wizard choose `n - 1` common spells at the start (more are filled from
`RealmDef::default_spells`); with eleven books in one realm the wizard knows all ten commons and
chooses two uncommon and one rare (`WizardBuild::spell_choices_of`).

The original's rule (`Init_Computer_Players_Spell_Library`, `Newgame_Screen_5`, `Human_Player_Wizard_Profile`
in ReMoM), per realm and number of books: **0 books** nothing; **1 book** no spell is known (but the realm's
research pool opens: 3 commons researchable); **2 to 10 books** the first `books - 1` common spells are known
(the human chooses them on the custom screen; presets, the computer wizards and anyone who does not choose
get the first ones of the realm's default list, `_default_spells`); the other commons become researchable up to
5, 6, 7, 8, 9, 10, 10, 10, 10 for 2 to 10 books, and with more than 7 books every uncommon, with more than 9
every rare and very rare spell; **11 books** all ten commons are known, plus two uncommons (default list
slots 11 and 12) and one rare (slot 13), and everything else in the realm is researchable. Besides, every wizard
gets the Arcane pool and Spell of Mastery as researchable, the computer wizards a spell of their main realm
(Water Walking, Counter Magic, Fire Bolt, True Light or Black Sleep) and, on Hard and Impossible, one more
creature spell to research. Our `WizardBuild::spell_choices_of` follows the first rule; the research pools are
`research.rs`'s own.

Ready-made wizards skip the budget check (the original's Lo Pan
spent twelve; ours has four sorcery books and spends eleven). The client's creation screen is `create_screen.rs`.

## 14. Global enchantments

A global spell sits in `Magic::global` and costs upkeep like any lasting spell. Its effect is
read where it matters (`globals.rs`), by spell id for engine rules, and through the spell's own
passive rules for unit stats (a rule may carry a condition such as `KindIs(Normal)` or
`RealmIs("chaos")`, checked against each of the owner's units). Spells with an engine rule carry
the `global_effect` hook so that casting them does not report "no effect yet". Implemented:
Evil Omens, Herb Mastery, Planar Seal, Awareness, Nature Awareness, Detect Magic, Holy Arms,
Chaos Surge, Charm of Life. The rest is task 87.

## 15. City enchantments with engine effects (task 25)

A city enchantment sits in the city's `Enchantments` (spell and caster), costs its upkeep, and is
dispelled like any other. Most are read by name where they matter. Stat-only ones (Heavenly Light,
Consecration, Prosperity, Inspirations, famine, pestilence...) are plain modifiers; the rest have
the hook `city_flag` (casting only enchants the city), and the code is in `city_spells.rs`:

| Spell | Effect |
|---|---|
| Gaia's Blessing | 50% more terrain food, the land greens and cleans each turn (`corruption.rs`), faster outpost growth, two rebels pacified (the help omits that, B8) |
| Stream of Life | doubled growth (stat), no rebels in the city and the units there heal completely each turn (task 24) |
| Evil Presence | an enemy city: the mana of its shrines, temples, parthenons and cathedrals is destroyed, unless the owner has death books; casting it on such a city is refused; an outpost shrinks faster (B22: the help, not the original's code, which stops them pacifying) |
| Chaos Rift | an enemy city: each turn every unit inside takes five strength 8 lightning bolts (armour piercing, stopped by magic immunity) and a building that nothing needs falls with 5 percent chance (`Event::ChaosRiftStrikes`; help only, B26) |
| Wall of Stone | not an enchantment: builds City Walls in a friendly city that has none (the original's `Cast_Wall_Of_Stone`) |
| Wall of Fire, Wall of Darkness, Flying Fortress, Cloud of Shadow | read when a battle starts at the city: `combat.md` section 7 |
| Spell Ward | cast with a realm (`Target::CityRealm`: the console's `ward:<city>:<realm>`, the client asks); the realms are kept in `SpellWards`, one ward costs the upkeep each; it turns away overland spells of that realm aimed at the city or at enemy units in it (`SpellFizzled`), bars fantastic creatures of the realm from entering the city, and stops the attackers casting spells of it in a battle there (B24) |
