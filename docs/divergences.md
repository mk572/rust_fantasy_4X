# Deliberate departures from the original

A register of places where the game does **not** do what the original does, and why. The
rule (`design.md` section 1, `rules-framework.md` decision 5) is that play, logic and balance
come before the help text, and the help text before the original's code. When we decide to
depart, or find that we must, it goes here with the reason, so later work does not "fix" it
back or wonder whether it was an oversight.

Not in this file: places where we simply have not got to the original's rule yet (those are
tasks in `TASKS.md`), conflicts between the original's help and its code (see
`help-vs-implementation-conflicts.md`), and the original's own bugs (resolved by the rules in
`design.md` section 11).

**Entry format:** what the original does, what we do, why, when decided, where it lives.
Add new entries at the end of their section; keep them short.

## 1. The AI and neutral players

Decided October 2026, `design.md` section 8.

- **Difficulty bonuses for the computer wizards are in (changed 16 October 2026, task 247).**
  The original gives computer wizards difficulty-based extras (gold, production, growth, mana,
  research, food, outpost growth, cheaper maintenance and unit upkeep, extra picks, starting
  gold). They are now the levels' settings (`game-settings.md`), applied to computer wizards only
  and changeable on the advanced screen; the internal `flat` settings give the old symmetric game.
  Still skipped: other code that treats the human and the AI differently, and the neutral cities'
  halved growth. Difficulty also scales lair guards, lair treasure, neutral city population and
  the final score, and sets how well the AI plays (task 148).
- **No hard-coded AI decisions in the core.** What an AI decides lives in `f4x-ai` and reaches
  the core only as ordinary commands.
- **The AI sees only what a human player would (task 181).** What the original does (as
  `design.md` section 8 has it): its computer wizards use knowledge a human does not have;
  the section lists "AI knowledge of hidden information" among what we skip.
  **Not checked:** we have not looked at what the original's AI can actually see, so this is
  design.md's claim, not a finding; check it in the original's AI code (ReMoM) before relying on
  it. What we do: the AI reads the game only through `PlayerView` (`player/view.rs`): its own units and
  cities, the cities of others on squares it has explored, the units on squares it sees now,
  and the terrain, rivers, nodes and lairs of explored squares. Why: the decision of October
  2026 that the AI has no advantage; our first AI broke it (it chose city sites on terrain it had
  never explored, counted every city for names and asked what stood on squares out of sight).
  Effect on play: settlers whose explored land has no site walk to unexplored land first
  (`nearest_unexplored`), so the AI expands more slowly at the start than the first version did;
  in a 3-seed, 200-turn soak the first wizard ends with 7, 2 and 5 cities and the second with
  5, 9 and 8 (the first version: 10, 2, 9 and 6, 6, 8). Two things still pass for what a player
  would know and are looser: a square the AI has explored shows its current city, not what it
  last saw, and the path search (`can_reach`, `reachable`) knows the real terrain, as the
  order to walk somewhere does. Built October 2026, task 181 in `TASKS-DONE.md`.
- **Neutral cities and units get no special rules either:** no half growth, no half production,
  no population cap, no free upkeep (next section).

## 2. Neutral cities (task 31)

Decided with task 31; code in `city/neutral.rs`, `player/mod.rs`, `turn.rs`, `wizard/fortress.rs`.

- **They pay upkeep.** The original gives a neutral garrison free upkeep. Ours pay food and
  gold like anyone's, which is design.md section 8 applied.
- **Weaker starting garrisons because of that.** The garrison is cut at generation to what the
  city can feed and pay for (`make_neutral_city`), so neutral cities hold somewhat fewer units
  than the original's. They keep them for at least 60 turns (tested on two seeds).
- **Every neutral city is a player of its own** (`Player::neutral`, ids from 100, brown banner,
  Grand Vizier on, 100 gold) with its own treasury, so its taxes pay its buildings and garrison.
  The original has one shared neutral owner and no treasury. Asked for as "a treasury for neutral
  cities"; the choice was one player per city over one shared player.
- **Run by a simple caretaker, not an AI.** Each turn the city puts enough citizens on farms to
  feed its garrison (`balance_farmers`) and sets the lowest tax rate that does not lose gold
  (`balance_taxes`). Nobody else decides for it.
- **Capturing one removes its player** and any garrison left; it defeats nobody. Neutral players
  are not wizards (`is_wizard`, `living_wizards`): no magic, fog or victory.
- **Monsters and lair guards keep the shared owner `unit::NEUTRAL`** (255): they have no
  treasury and no upkeep to pay.
- **Not copied:** the neutral cities' extra rules in the original's growth and production code
  (design.md section 8).

## 3. Sieges (task 41)

Decided with task 41; code in `city/conquest.rs`, `battle/`.

- **The field is mirrored in the rules:** the original has the defenders and the city at low x;
  the core has them at high x (the city at x 12 to 15, `x = 20 - cgx`). Only the code differs: the
  client draws each square where the original draws the mirrored one (`battle_screen.rs`,
  `battle_ground.rs`), so the picture is the original's.
- **An independent city's loot is its own treasury,** in proportion to the city's share of the
  citizens (all of it, as each independent city has one), where the original rolls a ten-sided die for each
  citizen. Our neutral cities have treasuries (section 2), so the wizard's formula fits.
- **Buildings are destroyed with the chance the original's documentation gives** (the city
  damage plus 10 percent). The ReMoM port of `Apply_Damage_To_City` reads as if the roll had to
  beat the chance, which would destroy nine in ten; this is taken to be a port slip.
- **Raze is always the second step** (the original's R key on the results scroll): the city is
  taken, fame is gained, and razing then costs the further fame. The original also has an
  immediate path (the first prompt, and the AI's `Raze_Check`) that skips the conquest fame; ours
  has the one path. The AI does not raze.
- **Outposts are destroyed when taken,** as in the original (a city with no citizens).

## 4. The starting setup (task 32)

Decided with task 32; code in `game/setup.rs`, `city/economy.rs` (`site_max_size`).

- **Starting gold follows the level (task 247).** The human gets `(5 - difficulty) * 25` (125 on
  Intro down to 25 on Impossible) and every AI 150, as in the original; both are settings.
  The internal `flat` settings (tests, soak, learning AI) give 100 to everyone.
- **Fortress sites are the nearest fitting site to a spread-out anchor**, not the original's
  random picks, so a seed gives a predictable, evenly spread start. The checks themselves are
  the original's (land, fortress distance 16 relaxing to 10, node/tower/lair distance 3
  relaxing to 1, room for 8 citizens or 7 from the fourth wizard). Distances use the
  original's `Range` throughout; the original uses a wrapping delta for fortresses, nodes and
  lairs. See task 155.
- **No two wizards share a race** while a free one remains; the original may repeat a race.
- **AI races are not limited by difficulty yet.** The original draws from nine races up to
  normal difficulty and from five (Klackon, High Elf, Halfling, High Men, Nomad) above it; that
  is a settings switch still to do (task 155 item 2).
- **A race from the wrong plane is refused** when the player names one (a Myrran wizard needs a
  Myrror race, anyone else an Arcanus one). The original's new-game screen just never offers
  it.
- **Upkeep before growth is *not* a departure:** the turn pays upkeep before cities grow, as
  the original does (`game-flow.md` section 4).

## 5. Special spells (task 56)

Decided October 2026, `spell/special.rs`.

- **Word of Recall into a full circle.** The original sends the unit anyway and then evicts the
  weakest unit of the stack (`Evict_Weakest_Unit`, lowest upkeep plus level). We make the spell
  unavailable while the summoning circle holds a full stack, so nothing is lost without the
  player choosing it. Revisit when evicting units has an interface.
- **Plane Shift's water rules.** The original has its own check (`Cast_Plane_Shift_Check`) for a
  destination square that is water. We use the normal movement rule: the shift is refused if
  any unit of the stack could not stand on the destination square. Not yet compared case by case
  with the original's check.
- **Earth Lore's block.** The original explores the map window on screen (12 by 10 squares).
  We explore a block of that size centred on the tile the wizard picks.

## 5b. Starting spells (tasks 252 and 315)

- **The AI's main-realm spells follow the wiki, not the code.** `INITGAME.c`
  (`Init_Magic_Personalities_Objectives`) indexes the spell list one place off, so it gives Sprites,
  Nagas, Hell Hounds, Guardian Spirit and Ghouls where its comments name Water Walking, Counter Magic,
  Fire Bolt, True Light and Black Sleep, and on Hard it tests the wrong spell before making the
  uncommon one researchable. We give the first five and, from Hard, Basilisk, Phantom Beast, Chimeras,
  Unicorns and Shadow Demons, as the wiki lists them. All of it is in `settings.ron`.

## 6. The Spell of Mastery's research cost (task 226)

Decided October 2026, `Magic::research_cost`.

- **Starting spells count.** The original lowers a wizard's Spell of Mastery research cost by
  half the research cost of each spell as the wizard gains it (`Player_Gets_Spell`; the starting
  spells are set directly and do not count) and by 3,000 more for a wizard with every realm at
  rank 11. We take the listed 60,000 less half the research cost of every spell the wizard
  knows, starting spells included, worked out when asked, so it needs no saved field. The
  rank-11 reduction is not modelled (no wizard can have it yet). The difference is a few hundred
  points of 60,000.

## 7. The wizard sheet (task 75)

Decided October 2026, `wizard_screen.rs`.

- **The spell books on the shelf cycle through their pictures.** `MAGIC.LBX` has three pictures
  for each realm's book. ReMoM's code for the Mirror screen (`WIZVIEW.c`, `Mirror_Screen_Load`)
  picks the next one with `var_2 = (var_2 + 1) / 3`, which never leaves the first, so a shelf of
  ten books of one realm shows ten identical books. We cycle first, second, third, first...
  (`n % 3`), so a shelf looks varied. **Not checked:** whether the real game's shelf is varied or
  identical; ReMoM is a reconstruction and the line may be its own error. Decided by the author.
- Heroes are listed by their class only; the original prints "Name The Class" (no personal hero
  names are kept yet, task 219).

## 8. Item names (task 218)

The original has two pairs of items that share a name. They are different items with different
costs and bonuses, so the game keeps both and tells them apart:

| Original name | Items in `content/items.ron` | What differs |
|---|---|---|
| White Lightning | `white_lightning_i`, `white_lightning_ii` | I: a sword, 2200 gold, +3 to hit and +2 defence. II: a wand, 1450 gold, +1 attack, spell skill 5, spell save 2. |
| The Resistor | `the_resistor_i`, `the_resistor_ii` | I: an axe, 950 gold, +5 attack. II: a shield, 1900 gold, +3 defence, 4 half-moves and +6 resistance. |

The numbering follows the original's table order. The original shows the plain name in both
places, so a player cannot tell them apart there; in ours the names carry I and II.

Two other points about the item data that are not departures but are easy to mistake for bugs:
movement bonuses of items are stored in half-moves (`HalfMoves`, the original's own unit: the data file holds whole moves and the original doubles them when it makes the item, so the values in `content/items.ron` are doubled; task 49 corrected them), and
to-hit bonuses are in pluses (1 to 3), the same as the units' to hit everywhere in the content.
The power table's "+6 Defense" row stores 5 in its bonus byte while its name says 6; we import 6.

## 9. The Elven Archer and the Knight (task 218)

The original's unit table (`UNITTYPE.c`) gives hero type 30 the name "Knight" and hero type 31 the
name "Elven Archer". Type 30 is the hero with the bow (melee 5, defence 6, ranged 8 with 8 arrows,
hero kind Bow), type 31 the melee hero (melee 8, hit points 9, no ranged attack). The hero names
in `NAMES.LBX` (Alorra, a female blademaster, at 30; Sir Harold, a noble leader, at 31) and ReMoM's
comments on the premade table (`NewGame.c`: "Alorra Elven Archer", "Sir Harold Knight") say the
opposite, as does common sense about the weapons. We follow the weapons: `elven_archer` is type 30
(the bow, Alorra) and `knight` is type 31 (Sir Harold); the stats, pictures and starting records stay
with their numbers, only the two names and ids differ from the original's table.

## 10. Items and crafting (tasks 49 and 50)

Where the item rules differ from the original's code or are read from ReMoM with a doubt:

- **Attack powers go to both attacks.** The original puts a power such as Vampiric, Lightning or
  Chaos on the melee or the ranged attack by the item's kind (`Item_Powers_To_Attack_Attributes`).
  Ours give the carrying hero the matching ability (`life_steal`, `armor_piercing`, `doom_damage`,
  ...), which holds for both attacks; a hero with a sword and a bow gets a shot with it too.
  The attack and to-hit bonuses are kept apart by kind as in the original (`MeleeToHit`,
  `RangedToHit`).
- **The award list is free to place from.** Items waiting after a reward are put on any hero at
  no cost; the original remembers where the reward was won and charges 20 mana to move it to a
  hero elsewhere (the vault is charged the same way for a hero away from the fortress, which we copy).
- **The made item is the designed one.** ReMoM's `Cast_Spell_Overland_Do` calls `Make_Item` after
  the item screen, which reads as if the made item came out of the random generator; the help
  (207, 211) says the wizard designs it, so the design is what is made. Unresolved in ReMoM.
- **Charges.** The spell of a staff or wand must be known and castable in battle (the original
  excludes eligibility 1 and 5; ours has no such number). One to four charges.
- **The item keeps its full value.** Artificer and the books lower the mana of the cast, as in the
  original, but the made item is worth (to the anvil, to merchants) the full cost.
- **Default names** are cut at 30 characters; the original blanks a long one.
- **Item movement** in `content/items.ron` was whole moves under the name of half-moves until
  task 49 doubled it.

### Random and prefab items (tasks 244 and 262): original bugs fixed

- **Books play no part in making an item (task 244).** `Make_Item` leaves out a prefab item when
  any of the five bytes that end its `ITEMDATA.LBX` record is above the wizard's books in nature,
  sorcery, chaos, life, death order. The bytes are not a per-realm requirement: each holds the book
  need of one power of the item in a position that follows no realm (137 of the 159 items with
  bytes carry exactly the needs of their powers; a Death 3 power sits in slot 0 on the
  Deathbringer, 2 on the Undertaker and 3 on a staff). The original therefore gives wizards items
  by books that have nothing to do with the item, and the merchant is asked for with twelve books in
  every realm anyway. Books are meant to limit what a wizard may *craft* (Enchant Item and Create
  Artifact, `crafting::check`), not what they are offered, found or given. **Ours:** the bytes are
  not kept in `items.ron` and `make_item` takes no books: any wizard can be offered or given any
  item, from the merchant, a lair or an event.
- **The value test.** `Make_Item` rejects a prefab item when `cost > value || value / 2 > cost`
  but only `if value == 0`: so with no value (the merchant) every prefab is rejected and a random
  item is always rolled, and with a value (a lair's 1200) the value is ignored. **Ours:** the test
  is what it was meant to be: with a value, the item's cost must lie between half of it and all of
  it; with none, any prefab item that fits the books may come. The merchant therefore also sells
  prefab items (their price is three times the cost, so the great ones are dear). **Check** with
  play.
- **A random artifact's power count.** `Create_Random_Item` reads the item kind before it is drawn,
  so every non-Enchant item gets four powers (its comment says melee weapons four, the rest
  three). **Ours:** the count follows the kind drawn (sword, mace, axe, staff and wand four; the
  rest three).
- **A merchant's prefab item not bought goes back** to the pool (declined, lapsed or too dear); the
  original keeps it taken once drawn. Prefab items are otherwise given once a game, as in the
  original.
- **The vault has no limit** (task 334). The original's fortress vault holds four items and six more wait on the award list, and items beyond that are lost. **Ours:** any number; the item screen shows four slots with an arrow on each side to scroll through them. Releasing heroes can therefore never leave items without a place.
- **A lair's spell treasure is a spell the wizard can use** (task 305). `WIZ_RollSpellReward` marks a realm
  valid when the wizard has *no* books in it (inverted: it also looks for a spell the wizard "knows", which
  only a realm with books has, so it rarely finds one) and its index arithmetic is muddled. **Ours:** a
  random realm the wizard has books in, or arcane; one book gives up to uncommon spells, two up to rare,
  more up to very rare (the original's `Max_Rarity`), arcane up to rare; a spell not known yet, never the
  Spell of Mastery or Return. A hoard with no such spell pays its value (rarity squared times 50) as gold
  and mana.

## 11. Heroes (task 219)

- **Availability is a state.** The original keeps whether a hero can be hired in its level (0 or
  more free, `-20` gone, other negatives dead). Ours has `Free`, `Serving`, `Dead` and `Gone` and
  keeps the experience separately, which gives the same results without the coding.
- **Arcane Power needs a magic ranged attack.** The original lets any hero but the Knight draw
  it (its own comment says the Elven Archer was meant), and doubles it only for the fireball,
  sorcery, death, ice, shaman and nature attacks, leaving the lightning attack out. Ours skips
  the draw for a hero with no magic ranged attack (lightning included) and doubles it for the
  rest.
- **Lucky stops at Lucky.** The original has no `break` after the Lucky case, so a Lucky draw
  also tries to raise Agility (a missing `break`, ReMoM's OGBUG note). Ours does not: Lucky, like
  Noble and Charmed, is open to every hero type and costs one pick, and Agility is a warrior
  ability drawn on its own.
- **Noble, Charmed and Lucky are general.** As in the original they ignore the warrior and mage
  budgets (any hero type can draw them); Torin never draws Noble.
- **One set of heroes for the game.** The original keeps a roster for each of its five wizard slots,
  so two wizards can both have the same hero. Here a hero is an individual: the 35 heroes are rolled
  once for the game, a hired hero serves one wizard and no other can hire it, a hero let go is free
  for anyone with the experience it has, and a dead one can be raised by any wizard with
  Resurrection. A hero's name is the first of the original's five. The roll comes last in `new_game`,
  after the map and the starting cities, so those stay as they were; every later draw of a seed
  moves once, and the soak numbers in `ai-design.md` were not regenerated.
- **A hero that flees and is lost, or is killed by a spell, is `Dead`** and can be resurrected
  (the design doc's open question 4: whether the original makes the fled ones revivable is not
  settled). Unpaid upkeep and dismissal make a hero `Free` (hireable again with its experience);
  a defeated wizard's heroes are `Gone`.
- **No limit of six heroes.** The original lets a wizard serve six heroes at most. Ours has no
  limit: a wizard may serve all 35. The hero screen scrolls six at a time. The chance of a hero
  offer still falls with the number of heroes served.
- **Hero spells are refused before they are paid for.** The help says Summon Hero, Summon
  Champion and Resurrection "fail" with six heroes. Ours does not start the cast when the wizard
  has no summoning circle, or nobody who could come (and Incarnation when Torin is
  serving or gone), so no mana is lost. A summoning that was paid over turns and then finds
  nobody is lost, with an event.
- **Resurrection names the hero.** The wizard chooses which dead hero returns
  (a `Target::Hero` target); the original lists the dead heroes for a human wizard
  (a single one is raised unasked) and a computer wizard takes the last in type order
  (`Units::ai_resurrection_pick`). Torin is never on the list, as in the original (only
  Incarnation brings him). The original also marks hero type 0 gone when the list is empty or
  there is no free place; that bug is not copied. Heroes arrive at the summoning circle (hired ones too).
- **Recall Hero is a battle spell that risks nothing**: the hero leaves the field and is at the
  summoning circle after the battle, not counted among the fleeing.
- **Heroes cannot be dismissed yet**: the Dismiss button refuses them until the items they carry
  have somewhere to go (task 221).
- **Names.** A hero's name comes from `content-local/hero_names.ron` (five a hero, one per wizard
  number), or the unit's own name without it.
- **Hero abilities (task 47).** Legendary adds fame (3 times level plus one; 4.5 times for the
  greater one): the original's fame code for it is commented out, the help promises it, so ours
  counts it (living hero units only). Charmed is +30 resistance as in the original, but ours is a
  real stat: the unit sheet shows it and overland spells honour it (the original adds it only
  inside battle checks, so its sheet shows nothing). A hero has one tier of an ability, never both the plain and the
  greater one as the original's two flags would allow. Leadership leaves out the undead as well as
  the fantastic creatures (the original tests the race only); the help says undead.

### Offers each turn (task 220): original bugs fixed

- **The computer wizards meet the merchant too.** The original offers the merchant's item to the
  human only, which leaves the computer wizards without a source of items. **Ours:** every wizard
  rolls it, with the same chance, price and Charismatic discount. A computer wizard buys when it
  has a hero to carry the item and the price is at most half its gold, and puts items on heroes at
  the fortress (`f4x-ai/src/offer.rs`; the first fit, a better choice is task 222).
- **Every wizard's chances use their own fame.** The original computes the fame once, in the
  human's merchant branch, and reuses it for every wizard's mercenary and hero chances, so the
  computer wizards were offered by the human's fame (or 0 when the human is skipped). **Ours:**
  each wizard's own.
- **The price is tested after the Charismatic discount** (the merchant's item and the
  mercenaries: the original tests the full price, then halves it). A wizard who can pay the half
  price is offered the item or company.
- **A level 2 company can be offered.** The original tests "above 90" twice, so the level is 3 or
  1. **Ours:** above 90 level 3, above 60 level 2, else level 1 (the same steps as the company's
  size). **Check** the intent against the help text.
- **The mercenary pool is drawn from a list.** The original draws a random unit number and tries
  again up to twenty times, with a test against an unset variable (`_turn / 2 < Total_Cost`, which
  reads an uninitialised cost) and tests on towers' owners that read as the wrong way round.
  **Ours:** a unit is drawn uniformly from the ordinary race units that are not settlers or
  boats and not the Draconian engineers (Draconians fly and need no roads; the original leaves out
  Draconian units with a construction value, which I take to mean these, not checked), of the races of the
  planes the wizard has a city or unit on (Myrror races need a Myrror presence). No offer before
  turn 30, as in the original; the unset-variable test is dropped; towers do not widen the pool.
- **The unit cap and eviction.** The original drops the weakest unit when the units at the
  fortress pass nine (`Evict_Weakest_Unit`). **Ours:** a company is not offered when the
  summoning circle's square cannot hold it, so nobody is lost.
- **The computer wizard's hero test** (`AI_Accept_Hero`): its comment says "cost less than 90% of
  gold", its arithmetic accepts a hero up to a ninth over the gold (`gold * 100 / 90 >= cost`) and
  reads the hero's base cost, not the price with its level. **Ours:** the price (with level and
  Charismatic) must be at most nine tenths of the gold. The computer wizards' mercenary test is the
  original's: from turn 50, a price of at most half the gold.
- **Offers wait for an answer** in `Player::offers` (saved) instead of being a modal call inside
  the turn change; what is not answered by the next turn change lapses. The computer wizards
  answer in `f4x-ai` at the start of their turn.
- **A hero's offer needs the fame the original asks of a hire** (`(cost - 100) / 10 < fame`) and is
  priced with the kept level, as `Hire_Hero_Popup` does; the offer is made even when the wizard
  cannot pay, as in the original.

## 12. Recorded elsewhere

Decisions already written down in `design.md` section 11 that depart from, or choose between,
the original's ways. Kept there; listed so this file is a complete index.

- Population is kept in citizens plus tens, and **surplus tens carry over** instead of being
  discarded.
- Economy and movement rules in "Economy and movement" (spare food sold two for one, a finished
  building drops the city back to housing, replaced buildings cost no upkeep, a unit with any
  movement left may always step): check each against the original before relying on it.
- Saves are snapshots, not command logs.
- Help text rewritten where it disagrees with the game's code (the importer's `HELP_CORRECTIONS`; the game's own help is `content/help.ron`).

## 13. Casting in battle (tasks 51, 159 and 265)

- **Life Drain gives a wizard mana, not skill.** The help says "converts the drained life into mana
  for a spell caster wizard"; the original's code adds three casting-skill points for each point
  drained (`Apply_Life_Drain`). Here a wizard gets 3 mana for each point; a hero or creature that
  cast it heals that many hit points, as in the original.
- **Every item's spell can be cast**, not only the first slot's (`COMBINIT.c`, bug H22), and the
  charges stay on the item after the battle.
- **A cast by a unit costs the spell's plain cost** in the unit's mana, as the original's does; the
  original's check that a unit has "at least 5 mana to cast anything" is not kept (the spell button
  needs only the mana of the spell).
- **Earth to Mud** follows the original's 12 half-moves to enter (the help says "all its movement
  points"), and turns open ground only, where the original tests the terrain group (grass and dirt).
- **Cracks Call** works on the one square aimed at (as `Apply_Cracks_Call` does); the help's
  "fissure" suggests more.
- **Heroes' skill at the fortress** (`Player_Hero_Casting_Skill`): half a hero's mana for each
  hero on the fortress square, added to the skill that pays for a spell over turns, for that turn
  only. The spell book's displayed limit in the original includes it; the limit for casting at once
  here does not.

## 14. Advisors, the Chancellor and the Settings page (tasks 71, 89 and 272)

Decided 16 October 2026, from ReMoM's `AdvsrScr.c`, `Surveyor.c`, `REPORT.c`, `Settings.c`.

- **The Surveyor's numbers come from the rules' data**, not the original's fixed strings. The
  original writes each terrain's food and bonuses as text (`Surveyor_IDK_Print_Terrain_And_Effect`);
  we print the terrain's name, its food in halves and its production percent from the content, and
  the shore, river and river-mouth gold bonus (10, 20, 30) as a line of its own, since those are not
  terrain types here. Minerals and sites say what their `CityBonus` gives.
- **An existing city's survey shows its real maximum size** but not the original's added guild,
  road, Inspirations and Prosperity bonuses (`Compute_Base_Values_For_Map_Square`); the percent
  figures are those of the land around it.
- **A lair's guards show only on squares in sight now**; elsewhere the panel says "Unexplored". The
  original keeps a scouted flag per lair (`Misc_Flags`), which we do not have.
- **The Surveyor is a map mode** (the side panel over the right column, the map live), a click
  centres the map. The original is a screen of its own with the same panel; the look is the same.
- **The Astrologer's Spell Research** counts a spell's place in its realm in the order of our
  content, not the original's spell table (`Astrologer_Research`); the scores are only a comparison.
  Only wizards are compared, never the independent players.
- **The History screen prints the real month** in its corner. ReMoM's reconstruction always prints
  the ninth month name (`months[8]`); we have not checked the original's code, so this may be its
  behaviour copied, or a reconstruction slip.
- **The Tax Collector's rows come from the rules' tax table** (`tax_unrest_percent`), gold a half per
  rate step, so a changed table shows up there. The original has the seven rows written in.
- **The Grand Vizier's question is a plain Y or N box**, not the original's confirmation box
  (task 143 is about those boxes). Switching it off asks nothing, as in the original.
- **The list box's colours are chosen by eye**: the original sets font colours by palette index
  (`COL_ListSel_Titles`), which we do not have.
- **The Chancellor's scroll** lists what the player has lost and gained, but not everything the
  original does: no per-unit names for drowned units and none for units killed by spells (our events
  count them and do not say whose they were), no dedupe of a city that grew and shrank in one
  turn, and no random events until task 68. Its building heading is "Destroyed Buildings", the
  original's unused string; the original's code prints "UNITS KILLED" over that list, which reads
  as a slip. U, D, the arrows and the wheel scroll a long scroll; the original has the keys as an
  empty TODO. Colours and the speed of the unrolling are chosen by eye.
- **The Settings page keeps its switches in `saves/preferences.txt`** (one `name=0|1` line each),
  not in a binary `MAGIC.SET` with the hall of fame and save names beside them. Escape saves them
  too; the original writes only on OK. A **Game...** button leads to the game's own settings
  (task 94), which the original keeps on the new-game screens. Random Events is a rules switch here
  as in the original, but it is stored with the preferences until task 274 decides where it lives.
- **An art scale (1, 2 or 4) is stored beside them** (`art_scale=N`, read at start-up, or
  `--art-scale N`): the screens keep the original's 320 by 200 layout and the art may be 640 by
  400 or 1280 by 800 for the same layout (`resolution.rs`, task 393). The original has one size.

## 15. Ice Storm and Fire Storm (task 56)

- **They follow the help text.** ReMoM has no code for either spell's effect (only their animations
  and the AI's wish to cast them), so there is no original rule to copy. Ice Storm is a strength 6
  magic cold attack and Fire Storm a strength 8 magic fire attack, each on every figure of every unit
  on the chosen square, the caster's own units and a city's garrison included, rolled with a battle's
  dice. Magic immunity and cold or fire immunity stop them. Decided 16 October 2026; they use the
  rules framework's `UnitsOnChosenSquare` selector, as Black Wind now does.

## 16. Wind Mastery (spell 57)

- **Our own rule, decided by the user on 16 October 2026.** The original scales only ships (the
  owner's Wind Mastery gives x1.5, each rival's halves; see B37 in
  `help-vs-implementation-conflicts.md`). Here a wizard with Wind Mastery gets x1.5 movement for all
  their ships and flying units, overland and in battle (`unit_max_moves`). While any wizard holds the
  spell, every wizard without it gets x0.5 for ships and fliers. Nothing stacks: two holders both get
  the bonus and no one is halved.

## 17. The world-scale spells (Meteor Storm, Great Wasting, Armageddon, Call the Void, Nature's Wrath)

- **Great Wasting** strikes three to six random land squares on either plane each turn (ReMoM: four
  to six), **anywhere**, the squares round the caster's own cities included (changed by the user,
  17 October 2026). A corrupted square gives its city no food, production or mineral, as before, and
  now also adds one unrest (one more rebel) **per corrupted square in the city's area**. The
  wizard who **keeps Great Wasting going** is proof against all that: their cities' corrupted
  squares still yield, add no unrest, and give **one power each** (counted once per square for the
  wizard, added to the power base like a volcano's, not to the city's own mana); in a **battle on a corrupted
  square** that wizard's spells carry no range penalty (the cost of distance from the fortress is
  capped at the normal one for one). Every one of those benefits ends the moment the spell does,
  and someone else's Great Wasting protects nobody. Every corrupted square, from any source, has a
  two in a hundred chance each turn of healing by itself (like a cooling volcano; the original's
  corruption is permanent until purified).
- **Armageddon differs from the original's** (decided by the user, 17 October 2026; the sea, the
  unrest and the volcano power were set again on 23 October 2026): it raises three to six volcanoes a
  turn on **any** square of either plane, the caster's own cities included, but not on a square with
  a node or other site, or a volcano already; **the sea too** (a volcano island; a ship on the
  square sinks), where the original never raises one, but a sea square is kept only one time in five
  of the times it is rolled; and a volcano that comes up on a **city's own square** (not the
  squares around) kills each of the city's buildings, units and citizens with one chance in four
  (the original destroys nothing; never the last citizen). The original's **unrest** is a bug-looking
  assignment that sets the pacifying of every other wizard's city to minus two, which wipes out its
  buildings; as in the **community patch** the buildings (the religious ones too) keep working and
  **every city that is not the caster's has two more unrest** while the spell lasts. Each volcano
  is an entity with its owner, the caster, and gives its **owner** one power a turn, as the
  original's `Volcano_Counts` does (an earlier rule here paid the caster for every volcano in the
  world; it is gone).
- **Volcanoes in general** (also Raise Volcano's): each new volcano has a one in five chance of
  bringing a mineral, and each turn each volcano has two chances in a hundred of cooling into plain
  mountains (the original rolls `Random(100) < 3`, two in a hundred; its comment says three). Neither
  was in the game before.
- **Meteor Storm:** strength 4 fire attack on every unit outside a city, the caster's too, then
  1 percent for each building of other wizards' cities (`spell/world.rs`).
- **Call the Void:** no loss of citizens (the original loses half); the surrounding squares are
  corrupted at one in two.
- **Nature's Wrath** shakes the cities of the wizard who casts a death or chaos spell (not the
  holder's own).
- **Great Wasting's unrest:** like Armageddon's, the original's assignment (the pacifying set to minus
  one) is read as one more unrest in every city that is not the keeper's (24 October 2026); with
  Armageddon also up, only Armageddon's two count, as in the original, where it overwrites.
- Details and the original's numbers: conflicts B38 to B41.

## 18. Time Stop (spell 80)

- **Stasis in a battle** (checked 24 October 2026): the original's combat code never reads the stasis
  mutation, so a unit in stasis would fight on. We keep the help's reading: it is as if asleep and
  does nothing.

- While a wizard keeps Time Stop going: only that wizard's units move or heal or gain experience,
  the other wizards' casting makes no progress, and the keeper has no income and pays no gold, food or
  other mana, only the spell's 200 mana a turn (ReMoM's `NEXTTURN.c`; its note says the manual says
  150). When the keeper cannot pay, the spell ends.
- The others' cities still grow and produce, and their gold and food are still counted, as in the
  original. Founding a city is refused to the other wizards (`SETTLE.c`; 24 October 2026) and the
  AI's armies, settlers and ships wait. Production and casting orders are not refused: they change
  nothing during the stop (the original skips the human's whole turn instead). The help's "combat part" does not exist in the original's code and is not done.

## 19. Magic Vortex (spell 105)

- Cast on a free square of a battlefield; at most ten at once; units cannot walk through one. At the
  start of each turn every vortex makes three random moves; its owner may move it one more square on
  their own turn (click it, then the square; the console has `bvortex`). The computer's vortices never
  get that move, as in the original. A vortex deals 5 damage to a unit on the square it comes to
  (not Magic Immunity or Righteousness), strikes each unit next to the square with a lightning bolt
  (strength 5, one chance in three; the original's code says one in three, the manual one in four),
  and adds 5 to the city damage over the city area.
- The learned brains do not cast it (`f4x-learn` `NOT_TRAINED`), so their input shape is unchanged.

## 20. The Spell of Mastery announcement and the AI's answer (task 61)

- The original shows the announcement to every player as a full-screen animation. Here it is
  `Event::MasteryStarted`, a text notice for the human and a log line; the animation is task 287.
- The original's AI only reads that a wizard is casting it (hostility +50, peace no protection).
  Here the AI also reads how much mana the caster has left and weighs a race against a war
  (`ai-design.md` section 7, step 4). That is a deliberate extension.

## 21. Lo Pan's starting books

The original's Lo Pan has five sorcery and five chaos books and the Channeler retort, twelve picks. Ours has four sorcery books (`content/wizards.ron`), eleven picks, as the other presets.

## 22. When mana, gold or food run short (tasks 62 and 292)

- The original disbands the newest units without asking, and only for the human wizard: its
  `Players_Apply_Upkeeps` calls `WIZ_MatchFoodUpkeep` and `WIZ_MatchGoldUpkeep` for player 0 alone,
  so an AI's gold is clamped at 0 and its army stays. It spares summoned creatures, heroes and
  anything with `Transport`, and the mana version (`WIZ_MatchManaUpkeep`) picks a random kind of
  asset and only the summons part is written.
- Here the human is asked before ending the turn (`upkeep_prompt.rs`): one list of the spells and
  summons that cost mana and of the troops that cost gold or food, the default (the newest) marked.
  The AI follows the same rules and answers through the same commands (`ReleaseUpkeep`,
  `ReleaseTroops`; `f4x-ai/src/shortfall.rs`: sell dispensable buildings, then let go of the
  cheapest units, loaded ships and heroes last). Independent cities pay upkeep like everyone and
  are settled by the turn change as they were.
- Food comes before gold, and a unit let go for food no longer counts for gold.
- Heroes can be let go (the original never does): they are free again, for any wizard to hire,
  and their items come home to the armory. Items that do not fit are the owner's to disband for
  mana (the AI: the cheapest). Task 293 (a bigger armory) and 294 (selling items) follow.
- A ship let go with passengers aboard turns neutral and sails, a turn's worth at a time, for the
  nearest shore with a free square, unloads there and is released. A ship in port or without
  passengers is released at once. If no shore is free the passengers are lost.
- Announced to wizards in contact: the shortage of mana, gold or food, and any hero who leaves.

## 23. Fame (task 72)

- The help (topic 765) says building a new city earns fame and any lost major battle costs a point.
  The original's code does neither, so the help now follows it (`HELP_CORRECTIONS`): founding gives
  no fame; the loser loses the fame of fallen heroes (half their level, rounded up) and of a lost
  city (its size), and a human wizard above 20 fame loses one more point for a major defeat.
- What counts as fame: earned fame, Legendary heroes (task 47), and 10 while Just Cause lasts
  (`Game::fame`). Fame lowers gold upkeep one for one and raises the chance and the quality of
  hero, mercenary and merchant offers. Each wizard's own fame is used (section 11).

## 24. Terrain food, production and invisibility (task 78)

- **Square food and production** follow the original's `Square_Food_x2` and `Square_Production_Bonus`
  (`Economy::square_food`, `square_production`): river squares, shore water, lakes and river mouths,
  and magic nodes (whose square has a base terrain of its own, section 33) give the original's food; Gaia's Blessing doubles
  forests and nature nodes. A river square gives no production bonus, as in the original, where a
  river replaces the land. Details in `help-vs-implementation-conflicts.md`.
- **Blur** is a flat -1 to the attacker's chance to hit (none for an attacker immune to illusions), not the original's 10% miss for each point of damage.
- **Invisibility in battle** (`Battle::seen_by`): as in the original, an invisible unit is hidden from
  the enemy (not drawn, not clickable, no ranged cursor) until an enemy unit is next to it or the
  enemy has a unit immune to illusions. Ours adds that a unit that attacks or casts shows itself
  until its side's next turn, and that the penalty to hit grows by one for each square of
  distance (the original takes a flat 1 off), none for attackers immune to illusions. A side's own
  invisible units are always targetable by it. Visible invisible units are drawn as ghosts. Area
  spells still reach hidden units. Scripted and learned battle AI see only what their side sees.
  Invisibility that begins mid-battle hides at once.

## 25. Banished wizards that resign (task 82)

- **Resignation** follows `Banish_Wizard`/`Resolve_Wizard_Conquest`: a wizard banished with
  cities left, whose casting skill with heroes is under 40 or whose mana is gone, is defeated at
  once (no -20 relations hit, only the final -40). This holds for the human too (task 341, the
  project owner's choice "as ReMoM"); a human who can pay is asked at the moment of banishment
  (`Command::Resign`, console `resign`; the client box shows the turns the spell needs).
- **The Spell of Return is paid with mana by every banished wizard** (task 341). The original lets a
  banished computer wizard pay its nominal skill every turn whether or not the reserve covers it, and
  floors the reserve at 0; ours does not. With no mana the spell cannot advance and the wizard can
  only resign. The computer wizard's script converts gold into mana to cover the payment (the
  original does it inside the rules, `AI_Update_Gold_And_Mana_Reserves`), with the AI work of task 339.
- **Conquest spells** are taken as in `Conquest_Spells`, including its quirk that a banished human
  keeps their spells (the original calls it only in the computer-wizard branch of `Banish_Wizard`).
- **Banished wizards and talk.** The original refuses the diplomacy screen to a banished human and
  has the computer wizards make no overtures to them. Ours refuses any banished wizard to start a
  proposal, a war, a treaty break or a gift (`DiplomacyError::Banished`), human or computer; an
  offer can still be answered. It does not copy the early return in `NPC_To_NPC_Treaty_Negotiations`
  (ReMoM marks it an `OGBUG`).
- **A defeat takes all the spell instances the wizard cast** (the original stores no caster for unit
  enchantments and so cannot end the loser's enchantments on other wizards' units; ours does).
- A resigned wizard's cities turn neutral, one neutral player each as the independent cities are;
  garrisoned normal units go with the city, heroes and conjured creatures are dismissed, all other
  units are dismissed, as in the original. The original halves the human's mana twice when they
  resign; ours halves it once.

## 26. Melee against fliers (task 107)

In the original a unit that does not fly deals no melee damage to a flying unit (`Combat.c`, `Battle_Unit_Process_Attack`; counter-attacks, thrown and breath attacks excepted). The setting `rules.melee_vs_fliers` chooses: `zero` (the default since 30 October 2026, the original's rule), `half` (the walker's melee strength halved, rounded up, so a walker that cannot be rid of a flier is not helpless; this was our rule before) or `full` (no rule). Only `half` and `full` diverge. "Walker" here is any unit that does not fly, ships and swimmers included. The flier's own blows are at full strength as before.

## 27. Enchanted roads can be dispelled (task 107)

Enchant Road is permanent in the original and nothing removes it. Ours is permanent too, but a dispel cast on a map square (Disenchant Area, Disenchant True) rolls against Enchant Road's difficulty (its cost, 100, times the usual realm multiplier) and on success turns that one square back into a plain road. The road keeps no caster, so the caster's own roads can be dispelled as well. Myrror's roads are magic by the plane and cannot be undone. Help topic 55 is corrected (`HELP_CORRECTIONS`).

## 28. Road building time (task 107)

`Turns_To_Build_Road` (`Terrain.c`) gives grassland 3, desert 4, forest, hills and tundra 6, mountains, swamp and volcano 8, a river square 5, and a node 4 (sorcery) or 5 (nature, chaos), because a node is a terrain of its own there. Ours matches on the ordinary ground and keeps a node on top of its ground, so a road on a node takes the ground's time (a sorcery node on hills: 6). Ours adds two turns for a river square's bridge to the time of its ground (grassland 5, as the original; forest 8, where the original's river square is 5). The turns are divided by the stack's construction points with a minimum of one turn, as in the original.

## 29. The 100-turn battle limit (task 107)

A battle still going after 100 turns is won by the defender (`TURN_LIMIT` in `battle/mod.rs`). The original has a limit too, **50 turns** (`Combat.c`, `Combat_Turn_Check`: `_combat_turn > 50` gives the battle to the defender), and ends with the message "All units retreat exhausted" (`message.lbx` 8) on the defeat scroll. We had recorded the limit as our own invention, which was wrong; the owner decided on 17 October 2026 to keep 100 knowing this.

## 30. Spell Animations setting covers battle casts (task 280)

In the original `magic_set.spell_animations` gates only the overland spell animations (summons, city enchantments, Change Terrain, the learn-spell scroll: `Spells130.c`, `Spells137.c`, `Spellbook.c`); `Combat_Cast_Apply_Spell_Effect` is always called with `anims_on` true, so battle animations ignore it. Ours skips the animations of the human's own battle casts when it is off, as the help (690, "spells you have thrown") reads and as a player who switches it off would expect. The rivals' casts still play. Decided 18 October 2026; `battle_screen/spell_anim.rs`. The overland animations do not exist yet (task 306, `overland_spell_animations.md`); when built they must check the same `Pref::SpellAnimations`, with the original's off path (the "You have completed casting..." box, which `spell_notices` already is).

## 31. Spells that are not cumulative (task 349)

The original refuses three pairs on the map only (`Cast_Spell_Overland`, `OverSpel.c`): Stone Skin
on a unit with Iron Skin, Resist Elements on one with Elemental Armor, and a third whose message
reads "Resist Magic is not cumulative with Magic Immunity". Its code for that third check tests
Resist Elements against Wraith Form and shows that message, which looks like a slip of the
reconstruction (ReMoM), since the asm is not at hand; **ours refuses Resist Magic on a unit with Magic
Immunity**, as the message says. The battle target check of the original has no pair check, so a
battle spell could be cast over its better twin; **ours refuses the pairs in battle too** (decided
23 October 2026), with `SpellError::NotCumulative`. As in the original (`COMBINIT.c`: Iron Skin
"else if" Stone Skin), the stronger spell wins when both are on a unit, whichever came first: Iron
Skin may still be cast on a unit with Stone Skin and replaces its bonus, and takes it back again if
Iron Skin is dispelled. The original asks the player to confirm a second cast of a global
enchantment (message 16) and refuses Holy Weapon while Holy Arms is up (message 17); ours refuses
Holy Weapon alike, for the AI too, and leaves the global's confirmation to the "continue casting?"
prompts (task 350).

## 32. Raise Volcano in the sea (task 348)

The original refuses Raise Volcano in water (message 36, "Volcanoes can not be raised in water").
**Ours allows it** (decided by the user, 23 October 2026): a volcano may rise in the sea and
makes a volcano island, as Armageddon's already do; a ship on the square sinks and those aboard
are left on the land. Mountains, hills and squares with a node or other site are still refused.


## 33. Node base terrains (task 338)

In the original a magic node is two things: the base terrain of its square (`tt_SorceryNode`,
`tt_NatureNode`, `tt_ChaosNode`) and the node record. Ours has both too: the node is a feature and
the placement step puts `sorcery_node`, `nature_node` or `chaos_node` terrain under it
(`content/terrain.ron`). Food, production and movement are the original's (`Square_Food_x2`,
`Square_Production_Bonus`, `TERRSTAT.LBX`): sorcery 4 / 0 / 1, nature 5 / 3 / 2, chaos 0 / 5 / 3.

- **Nature node food is 5** (decided by the user, 23 October 2026): the original's in-game
  `Square_Food_x2` gives 5, while its new-game code (`MAPGEN.c`, also marked there as a bug) gives 4.
  Until task 338 ours gave 4 (`B16`).
- **A river is cleared under a node**, since the original's node replaces the river tile.
- **A nature node is a forest and a chaos node a mountain** wherever the original asks
  (`Square_Is_Forest`, `Square_Is_Mountain`): Forester and Mountaineer movement, Gaia's Blessing and the
  battle ground. The original's `Square_Is_Node` tests the sorcery node three times and so finds only
  that one; ours finds all three.
- **A chaos node is not a volcano.** Both are drawn with similar art, but a volcano raised by a
  spell is a mountain square with an owner (a power point a turn for the owner, a 2% chance to cool
  each turn); a chaos node never changes and has no owner of its own.
- Change Terrain, Raise Volcano and Corruption refuse a node square, as the original does
  (Corruption with its own message 41; Change Terrain with the fixed-terrain words, task 350).

## 34. Terrain movement costs (task 240)

`TERRSTAT.LBX` entry 0 holds six bytes for each of the 770 terrain types of the map: the half-move cost
for the unused mode, walking, Forester, Mountaineer, swimming and sailing (255 is "cannot enter";
`Movement_Mode_Cost_Maps`, `MAPGEN.c`). Read on 24 October 2026 for the types a generated map uses
(ids as in `TerrType.h`; the Community Patch changes only the volcano, id 179, to a mountain's costs):

| Terrain | Original, walking | Ours (`terrain.ron`) |
|---|---|---|
| Grassland (162, 172 to 176) | 1 | 1 |
| Forest (163, 183, 184) | 2 | 2 |
| Mountains (164, 259 to 274) | 4 | 4 (changed 24 October 2026) |
| Hills (171, 275 to 291) | 3 | 3 (changed) |
| Desert (292 to 451) | 1 | 1 |
| Swamp (166, 177, 178) | 3 | 3 (changed) |
| Tundra (602 to 761) | 2 | 2 |
| River squares (233 to 258) | 2 | (the terrain's own) |
| Sorcery node / nature node / chaos node (168 to 170) | 1 / 2 / 4 | 1 / 2 / 4 (changed) |

So hills, swamp and mountains cost one more move in the original than in the game, and a chaos node
(a mountain) 4. The Forester column makes forests cost 1, the Mountaineer column hills and
mountains 1 (as ours does), and the Mountaineer column puts 3 on grassland and 2 on forest: a
slowdown the Community Patch removes, which we do not have. Entry 1 (196 bytes) is the 14 by 14 racial unrest
table (`TBL_Unrest`), which `rules.ron` (`racial_unrest`) already holds, identical (the Klackon's -2 is
the byte 254); `TERRTYPE.LBX` is the generator's autotile lookup (task 150). **Changed on 24 October 2026**
(decided by the user, task 372): hills, swamp and mountains and the chaos node now cost the original's 3, 3,
4 and 4. **Swamp food** is half a food (`food: 1`), which the original's code gets wrong (`Square_Food_x2`
returns 0, `OGBUG`) and a later patch corrects; ours follows the patch.

## 35. Immunity fixes (task 263)

Three places where the original's immunity code is a known bug and the game follows the fix
(`combat.md` section 2.1 has the whole audit): **ships and catapults** have no magical weapon, so
weapon immunity stops them; **Wraith Form** is no shelter from Wrack or Call Lightning; **death
immunity** protects against Terror. The other differences listed there are kept as the original.

## 36. Ships in battle, lost ships and lakes (tasks 233, 234, 235, 236, 253, 352)

- **Sea battles** (233, 352): the original keeps land units out of a fight on the sea unless a
  Floating Island or Wind Walking carries them, and has no ship on the field. Ours lays each
  ship out as a deck of brown squares with pointed ends (`units.ron` `deck`), with one square of
  water between ships that a walker may jump for two moves; the land units aboard fight on the
  deck, and the Floating Island is a ship here. See `combat.md` "Water battles".
- **Attacking from a ship** (234): follows ReMoM, which bars nothing.
- **A ship lost** (235): the original has no rule; ours lets the passengers use the fleeing odds
  beside land (some drown) and drowns them in open sea; fliers and swimmers always survive.
- **The stack limit** (236): unchanged, nine units a tile with passengers counted; a separate fix of
  the limits is planned.
- **Lakes with an inflow** (253): drawn with the original's `_1LakeRiv_*` pieces, but the map is
  not rewritten: a lake without an inflow stays a lake (the original makes it desert) and extra
  inflows stay rivers (the original turns them to grassland).
- **Selling an item** (294): the original's anvil only turns an item into mana. Ours adds a sale
  (`Command::SellItem`, S on the item screen, `sell-item` in the console): the seller is paid half
  the item's value in gold at once, and the item is offered at that price (`Offer::Resale`) to
  the other wizards in turn, those whose fortress is on the seller's plane first, each group
  starting after the seller; a wizard who cannot pay is not asked. A computer wizard answers
  through `f4x_ai::answer_resales` by the merchant's rule (a hero to carry it, the price at most
  half its gold); a human sees the merchant's box. The first to take it pays the same price to
  nobody (the gold the seller got is the market's); if nobody does, the item is gone.
- **Map sizes** (343): the original has one map size, 60 by 40; ours offers four (45 by 30 to
  90 by 60) and scales the counts per 2400 squares.
- **Neutral city placement** (user's design, 24 October 2026): the original rolls squares at random
  (1000 tries a plane), 15 cities for 2400 squares at most; that is kept with `original_map_generator`
  on. With the custom generator the plane is searched row by row from the equator outward (a row
  south, a row north, and so on, the four polar rows left out), each row from west to east: a city
  goes on the first square that feeds five citizens and keeps 6 squares from every city and fortress
  and 5 from a node, tower or lair (the original keeps 4, 5 and 4), or on the best square within two
  squares of it. It stops at 15 for every 2400 squares or at the last row, so the cities lie in the
  middle belt of the map (`f4x-tools start-stats` counts what a game gets).
- **Half upkeep for basic troops** (user's choice, inspired by the Caster of Magic mod; 24 October
  2026): spearmen and swordsmen of every race cost half the food and half the gold. Upkeep is whole
  numbers for each unit, so about every second basic troop (the ones with an odd serial number, the
  order units are made in) costs nothing and the others pay in full. The unit sheet shows the
  unit's own figure, so a free one shows none. `half_upkeep` in `content/settings.ron`
  (`enabled: true` turns it on; since 30 October 2026 it is off, so the AI learns on the original's rules; `units` names the troops).
- **No limit on the number of units** (decided by the user, 24 October 2026, task 144): the
  original's unit table holds `MAX_UNIT_COUNT` entries, so production stops and says "Maximum number
  of units exceeded" (`NEXTTURN.c`, message 66) when it is full. Ours has no such table: the only
  limit is nine units on a square (`large-stacks.md`). The warning and every check for it are not
  built and will not be.
- **Raiders and rampaging monsters** (task 376, `threat.rs`; the owner's design, 24 October 2026): the
  original's raiders are copies of a neutral city's garrison that walk out beside the city, with a
  third of the garrison removed to pay for them, and then head for a player's city; its monsters are
  made beside a lair and rampage. Ours raises the raiders *in* the city as new troops that cost no
  upkeep, and puts the city at war for good (`Aggressive`): all its spare soldiers take lairs and
  cities near it. Our new monsters take over the lair they are made in, the old guards are driven out
  and roam, settle in empty stands and attack the cities they find; the monsters that win a city
  vanish, as the original's message says, and the lair choice has no bias to the human's plane (the
  original rerolls towards it). A city a raiding city conquers is its own, not a new independent city
  (item 33's design is still planned). Both are off in the symmetric `flat` game.

## 37. The three event switches the original never reads (tasks 269 to 271, 276 to 278)

In the original `magic_set.city_spell_events`, `overland_spell_events` and `summoning_events` are set
by the Settings screen and by the defaults (`Settings.c`) and read nowhere else (a search of ReMoM
finds only `MOX_SET.h`, `Settings.c`); their help (topics 678 to 680) promises what no code does. What
the original really does: `enemy_spells` gates every other wizard's spell animation (`OverSpel.c`,
`Spells129.c`, `Spells130.c`, `Spells132.c`, together with contact and, for summons, Detect Magic),
`enemy_moves` moves the view while a rival's detected troops move (`MainScr.c`), and
`auto_unit_information` shows the hovered unit's figures in a battle (`Combat.c`).
Ours reads all six. Enemy Moves centres the map on a rival's step and slides it (`map_slide.rs`), Enemy
Spells hides a rival's global-spell mirror and spell sound, Auto Unit Information hides the battle's hover
window (the right-click sheet stays), and Overland Spell Events turns the global-spell mirror off
for every wizard, as its help reads. City Spell Events and Summoning Events turn off the city-spell scroll (`city_scroll.rs`) and the summon scene
(`summon_scene.rs`). A rival's summon needs Enemy Spells, Detect Magic and contact; its spell on a unit, a square or a city
needs Enemy Spells, Detect Magic and the square explored (a cast on the human's own city is always shown),
as in the original (`spell_sight.rs`). The global-spell mirror needs Enemy Spells only: the original tests nothing there. Decided 30 October 2026.

## 38. Refusals of the original that we added, and those we leave out (task 350)

Added (30 October 2026): a wizard target needs contact (line 27); Spell Blast needs the mana to match
the spell (68); Disenchant Area needs something of another wizard's on the square (28); Change Terrain on
a node says nodes may not be changed (34); a volcano cannot rise on a river (86); Lycanthropy refuses an
already mutated unit (64); a summoning needs room on the square (21); a finished unit waits, with its cost
paid, while nine units stand in the city (50, 66; `Event::ProductionBlocked`); Web is refused on a
non-corporeal unit (71); a mass spell with no unit it can touch (79 to 83) is refused at the cast
(`NothingToHit`). Floating Island (58 to 60) was already refused by `check_floating_island`.

Left out on purpose:
- **"Continue casting?" prompts (12 to 16).** The original lets the wizard cast anyway after a warning
  (Planar Seal up, hero limit reached, no global enchantments to dispel, no contact, already cast). Ours
  refuses the cast outright with the reason, which is the safer rule and the same for the AI; no prompt.
  The second cast of a global enchantment is refused by the stacking check (task 349).
- **No settlers from a fortress's last citizens (11).** Settlers now cost a citizen (setting
  `rules.settlers_cost_citizen`, on by default) and need a city of two; the original's special line for a
  fortress's last citizens is covered by that same rule, with the general text.
- **Line 73 (Sorcery-immune unit).** I could not find which spell the original refuses it for; ours casts
  and the unit resists. Task 351 covers the lines whose trigger is not traced.
- **Dispelled-enchantment notices (18, 19, 20, 45, 46) and the rampage (10):** task 351.
- **The human-only branches of the diplomacy rules (task 368).** The original's code treats the human differently
  in five places; this game decides each. **Army size and city count** (`Diplomacy_Growth`): the original lowers
  the AI wizards' relation to the *human* when the human's army is much stronger or the human holds too many
  cities; **ours:** it holds for every wizard, so an AI that grows far ahead is disliked by the others (`diplomacy_growth`).
  **Peace duration** after a war: the original leaves only the human alone for `7 + Random(6)` turns; **ours:** each of
  the two is left alone by the other, and an AI wizard does not start a war of its own accord on a wizard with
  time left (`f4x-ai` `diplomacy::quarrel`). **Peace patience** wears down by a quarter of what an attacker does
  in a war: the original for a human attacker only; **ours:** for every attacker, so AI wars end by peace offers
  too. **Kept as rules:** the strongest recent action (`last_action`) is still recorded for a human attacker only,
  because it exists so the AI can remark on it to the human, and Subversion still spares the human, whose
  feelings are their own to have.


## 39. Random events: the quirks decided (task 303)

Rules of `events.md` where the original's code, its manual and our game part ways, and which side the game follows.

- **Population Boom with Stream of Life.** Both are a +100 percent on the city's growth here, so together they triple it; the original doubles twice and quadruples. Kept as it is: the sum of bonuses is how every growth modifier of this game combines.
- **Piracy.** The reconstructed code of the original computes `gold * 100 / (29 + d21)`, over three times the reserve, which the cap at zero turns into a theft of the whole reserve or nothing. The game follows the manual: 30 to 50 percent, rounded down to tens (`pirates_take`).
- **When a lasting event ends.** The manual says 5 percent a turn for all of them. The original's code does that for Plague and Population Boom only (after turn 3) and gives the six world events a chance that rises with their age (`d20 <= turns run - 2`). The game follows the code (`flat_end`, `world_end`), as it plays; the help text is the part that is wrong.
- **Depletion and New Minerals.** ReMoM stubs both and the original's are said to be bugged, so we use our own rule: a square in the area of one of the victim's cities, and for New Minerals a mineral by the spawn weights of the square's terrain (`find_deposit`).
- **Plague.** No unrest: the +2 unrest belongs to the Pestilence spell only, as in the code.
