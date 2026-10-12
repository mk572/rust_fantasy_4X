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

- **No AI bonuses or penalties.** The original gives computer wizards difficulty-based extras
  (gold, production, growth, mana, cheaper units, a slower outpost growth) and lets some code
  treat the human and the AI differently. We use the human's rules for everybody and skip every
  branch that exists only for AI parity. Difficulty scales lair guards, lair treasure, neutral
  city population and the final score, and may later set how well the AI plays.
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

- **The field is mirrored:** the original has the defenders and the city at the west; ours has
  them at the east (x 12 to 15), as our other battles do. Only the picture differs.
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

- **Starting gold is 100 for everyone.** The original gives the human `(5 - difficulty) * 25`
  (125 on Intro down to 25 on Impossible) and every AI 150. The flat figure removes an AI bonus
  (section 1) and keeps the start the same at every difficulty. Revisit if difficulty should
  affect the player's own start.
- **Fortress sites are the nearest fitting site to a spread-out anchor**, not the original's
  random picks, so a seed gives a predictable, evenly spread start. The checks themselves are
  the original's (land, fortress distance 16 relaxing to 10, node/tower/lair distance 3
  relaxing to 1, room for 8 citizens or 7 from the fourth wizard). Distances use the
  original's `Range` throughout; the original uses a wrapping delta for fortresses, nodes and
  lairs. See task 155.
- **No two wizards share a race** while a free one remains; the original may repeat a race.
- **AI races are not limited by difficulty.** The original draws from nine races up to normal
  difficulty and from five (Klackon, High Elf, Halfling, High Men, Nomad) above it, which is a
  bias against the AI (section 1).
- **A race from the wrong plane is refused** when the player names one (a Myrran wizard needs a
  Myrror race, anyone else an Arcanus one). The original's new-game screen just never offers
  it.
- **Upkeep before growth is *not* a departure:** the turn pays upkeep before cities grow, as
  the original does (`game-flow.md` section 4).

## 5. Recorded elsewhere

Decisions already written down in `design.md` section 11 that depart from, or choose between,
the original's ways. Kept there; listed so this file is a complete index.

- Population is kept in citizens plus tens, and **surplus tens carry over** instead of being
  discarded.
- Economy and movement rules in "Economy and movement" (spare food sold two for one, a finished
  building drops the city back to housing, replaced buildings cost no upkeep, a unit with any
  movement left may always step): check each against the original before relying on it.
- Saves are snapshots, not command logs.
- Help text rewritten where it disagrees with the game's code (`[Updated from game code: ...]`).
