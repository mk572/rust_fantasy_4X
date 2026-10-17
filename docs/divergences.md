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
  it. What we do: the AI reads the game only through `PlayerView` (`player_view.rs`): its own units and
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

Decided with task 31; code in `neutral_cities.rs`, `player.rs`, `turn.rs`, `fortress.rs`.

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
- **Monsters and lair guards keep the shared owner `units::NEUTRAL`** (255): they have no
  treasury and no upkeep to pay.
- **Not copied:** the neutral cities' extra rules in the original's growth and production code
  (design.md section 8).

## 3. Sieges (task 41)

Decided with task 41; code in `conquest.rs`, `battle/`.

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

Decided with task 32; code in `setup.rs`, `economy.rs` (`site_max_size`).

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

Decided October 2026, `special_spells.rs`.

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
- **Which wizards roll.** Every wizard gets a roster, in player order; the original rolls its
  five wizard slots. Independent cities' players have none. The roll comes last in `new_game`,
  after the map and the starting cities, so those stay as they were; every later draw of a seed
  moves once, and the soak numbers in `ai-design.md` were not regenerated.
- **A hero that flees and is lost, or is killed by a spell, is `Dead`** and can be resurrected
  (the design doc's open question 4: whether the original makes the fled ones revivable is not
  settled). Unpaid upkeep and dismissal make a hero `Free` (hireable again with its experience);
  a defeated wizard's heroes are `Gone`.
- **Hero spells are refused before they are paid for.** The help says Summon Hero, Summon
  Champion and Resurrection "fail" with six heroes. Ours does not start the cast when the wizard
  has six heroes, no summoning circle, or nobody who could come (and Incarnation when Torin is
  serving or gone), so no mana is lost. A summoning that was paid over turns and then finds
  nobody is lost, with an event.
- **Resurrection names the hero.** The wizard chooses which dead hero returns
  (`Command::ResurrectHero`); the original lists the dead heroes for a human wizard
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
  the fortress (`f4x-ai/src/offers.rs`; the first fit, a better choice is task 222).
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
- Help text rewritten where it disagrees with the game's code (`[Updated from game code: ...]`).

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
- **Armageddon is riskier than the original's** (decided by the user, 17 October 2026): it raises
  three to six volcanoes a turn on **any** square of either plane, the caster's own cities included
  and the sea too (a volcano island; a ship on the square sinks), but not on a square with a node or
  other site, or a volcano already. A volcano that comes up on a **city's own square** (not the
  squares around) kills each of the city's buildings, units and citizens with one chance in four
  (the original destroys nothing; never the last citizen). While anyone's Armageddon lasts **no
  building reduces unrest in any city** (the original sets the *other* wizards' cities to unrest of
  its own, as a bug-looking assignment). The caster gets one mana a turn from **every** volcano in
  the world, whoever raised it; a wizard without Armageddon still gets one from each of their own.
- **Volcanoes in general** (also Raise Volcano's): each new volcano has a one in five chance of
  bringing a mineral, and each turn each volcano has two chances in a hundred of cooling into plain
  mountains (the original rolls `Random(100) < 3`, two in a hundred; its comment says three). Neither
  was in the game before.
- **Meteor Storm:** strength 4 fire attack on every unit outside a city, the caster's too, then
  1 percent for each building of other wizards' cities (`world_spells.rs`).
- **Call the Void:** no loss of citizens (the original loses half); the surrounding squares are
  corrupted at one in two.
- **Nature's Wrath** shakes the cities of the wizard who casts a death or chaos spell (not the
  holder's own).
- Details and the original's numbers: conflicts B38 to B41. Not done: Great Wasting's unrest in other wizards' cities (task 286).

## 18. Time Stop (spell 80)

- While a wizard keeps Time Stop going: only that wizard's units move or heal or gain experience,
  the other wizards' casting makes no progress, and the keeper has no income and pays no gold, food or
  other mana, only the spell's 200 mana a turn (ReMoM's `NEXTTURN.c`; its note says the manual says
  150). When the keeper cannot pay, the spell ends.
- The others' cities still grow and produce, and their gold and food are still counted, as in the
  original. Commands other than movement are not blocked (founding a city, production orders): task
  286. The help's "combat part" does not exist in the original's code and is not done.

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
- Heroes can be let go (the original never does): they return to the roster and may be hired again,
  and their items come home to the armory. Items that do not fit are the owner's to disband for
  mana (the AI: the cheapest). Task 293 (a bigger armory) and 294 (selling items) follow.
- A ship let go with passengers aboard turns neutral and sails, a turn's worth at a time, for the
  nearest shore with a free square, unloads there and is released. A ship in port or without
  passengers is released at once. If no shore is free the passengers are lost.
- Announced to wizards in contact: the shortage of mana, gold or food, and any hero who leaves.
