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

## 3. The starting setup (task 32)

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

## 4. Recorded elsewhere

Decisions already written down in `design.md` section 11 that depart from, or choose between,
the original's ways. Kept there; listed so this file is a complete index.

- Population is kept in citizens plus tens, and **surplus tens carry over** instead of being
  discarded.
- Economy and movement rules in "Economy and movement" (spare food sold two for one, a finished
  building drops the city back to housing, replaced buildings cost no upkeep, a unit with any
  movement left may always step): check each against the original before relying on it.
- Saves are snapshots, not command logs.
- Help text rewritten where it disagrees with the game's code (`[Updated from game code: ...]`).
