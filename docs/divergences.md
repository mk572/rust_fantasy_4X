# Deliberate departures from the original

A register of places where the game does **not** do what the original does, and why. The
rule (`rules-framework.md`, decision 5) is parity first, departures by decision. When we
decide to depart, or find that we must, it goes here with the reason, so later work does not
"fix" it back or wonder whether it was an oversight.

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
movement bonuses of items are stored in half-moves (`HalfMoves`, the original's own unit), and
to-hit bonuses are in pluses (1 to 3), the same as the units' to hit everywhere in the content.
The power table's "+6 Defense" row stores 5 in its bonus byte while its name says 6; we import 6.

## 9. Recorded elsewhere

Decisions already written down in `design.md` section 11 that depart from, or choose between,
the original's ways. Kept there; listed so this file is a complete index.

- Population is kept in citizens plus tens, and **surplus tens carry over** instead of being
  discarded.
- Economy and movement rules in "Economy and movement" (spare food sold two for one, a finished
  building drops the city back to housing, replaced buildings cost no upkeep, a unit with any
  movement left may always step): check each against the original before relying on it.
- Saves are snapshots, not command logs.
- Help text rewritten where it disagrees with the game's code (`[Updated from game code: ...]`).
