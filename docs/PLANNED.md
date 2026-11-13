# Planned modifications

Changes and extensions to do **after the basic game is playable** (the core loop of
`docs/TASKS.md` working: map, cities, magic, combat, AI, victory). Unlike the task list, these
are not gaps against the original: they are improvements and options on top of it. Keep the
table in ascending order by number; new items take the next free number.

Related tasks in `TASKS.md` are given as `T<number>`.

## Table

| \# | Area | Summary |
| --- | --- | --- |
| 1 | Movement | Faster movement along rivers |
| 4 | Content | Rules changes and new items from the community patch and the Caster of Magic expansion |
| 5 | Platform | Modding support |
| 6 | Client | Upscaled graphics |
| 8 | Options | Hardcore mode |
| 9 | Client | Battle success hint |
| 10 | Client | Font and TrueType support |
| 11 | Client | Higher resolution modes |
| 12 | Content | Replacement art |
| 13 | Client | Different settlement icons by race |
| 14 | Client | Fortress art by magic type |
| 16 | Tools | Map editor |
| 17 | Meta | MCP server for remote control |
| 18 | Meta | Hot seat multiplayer |
| 19 | Meta | Asynchronous multiplayer |
| 20 | Rules | Early-game speed-up options |
| 21 | Rules | (Not needed) Faster outpost growth |
| 22 | Rules | Alternative effect for the Charismatic retort |
| 23 | Platform | Packaging for Linux |
| 24 | Content | Content pass and balance with AI-vs-AI soak runs |
| 25 | AI | Battle AI tournament: neural against neural, with varied entries |
| 26 | Rules | Hold command in battle: ends the unit's turn and gives a defensive bonus |
| 27 | Rules | More collateral damage for spells: attack spells on cities kill citizens and destroy buildings |
| 28 | Rules | More useful buildings: most buildings get a tangible benefit of their own |
| 29 | Rules | Reduced upkeep for basic units; no upkeep for settlers |
| 30 | Rules | More interesting strategies for underplayed or underpowered races |
| 32 | Magic | Neutral cities: their own mana and spells |
| 36 | Meta | Multiplayer: scope and design (to be discussed) |
| 37 | AI | Record the battle AI's orders for replays |
| 38 | Rules | Casting mod: half the casting distance penalty, a Channeler that costs one pick, Lo Pan's fifth sorcery book back |
| 40 | Rules | Life and death nodes |
| 41 | Content | Free sounds: effects for events that have none, and replacements for the original's sound and music |
| 42 | Map | Mines, lumber camps and hunter's lodges on the map |
| 43 | Client | Proper windowing support: boxes and screens as windowed layers instead of one shared pane |
| 45 | Client | Start a game from a map seed chosen in the UI |
| 46 | Tools | Find the use of the unreferenced art files `combat`, `desert`, `tundra`, `cmbtwall` |

## Descriptions

**1. Faster movement along rivers.** Rivers exist as a tile flag (T1, done) but do not affect
movement. Add a rule that a step between two river tiles (or from a river tile to the shore at
its mouth) costs less, like roads do (half a move; decide whether it stacks with roads, which
should not give a quarter move), for land units and perhaps cheaper still for boats-on-rivers if
ships ever sail them. Make it a rule in data (a movement cost modifier per river step) so it can
be switched off, and show river-assisted paths in the path preview. Needs a decision on
direction (up or down stream) before any asymmetric rule; the river layer currently has no flow
direction.

**4. Community patch and Caster of Magic content.** Collect the rule changes of the community
patch (its files are in `reference/mom-classic/Master of Magic Community Patch/`) and of the
Caster of Magic expansion: list every changed number, ability, building, unit, spell and item;
decide per change whether it becomes the default, a rule-set option or a mod (see 5); and add
the new items, units and spells to `crates/f4x-def/content/` through `scripts/import_mom.py` where they
come from original rules data, or write them by hand. Details of the
expansion are not yet gathered here, so the first step is a survey document in `docs/` listing
what exists, before any code. The importer's conflict rules (`help-vs-implementation-conflicts.md`)
apply: patch behaviour that fixes a marked bug wins; anything else is an option.

**5. Modding support.** The rules are already data (`crates/f4x-def/content/*.ron`, validated by
`f4x-tools validate`). Make this a supported feature: load a stack of content directories
(base, then mods in order, later ones override or extend by id); a mod manifest with name,
version and dependencies; validation that reports conflicts by id; a list of the engine hooks
(`CUSTOM_HOOKS`, effect primitives) as the documented extension surface, and a decision on
scripting (none, or a small embedded language) for effects the data cannot express; mod-aware
saves that record the active mods and refuse to load without them; a mod picker in the client.
Document the data formats in `docs/` for modders. Keep IP-protected material (art, music, help text) out of the
repository.

**6. Upscaled graphics.** The client draws the original's 20x18 tiles and small unit pictures,
scaled with nearest-neighbour. Add an optional higher-resolution art path: an upscaling step in
`f4x-tools` (a documented, local-only pipeline, for example a pixel-art upscaler or a
diffusion-based one run on the user's own machine) writing to `assets-local/art-hd/`, and a
client setting that prefers it when present, with the same file naming so the existing loader
works. The output of upscaling original art is still original-derived and stays out of the
repository; the repository can only ship tools. Also check tile seams and shore blending
(the autotile table, T4) at the new size, and let the UI scale independently.

**8. Hardcore mode.** A new-game option for one-life play, bundling three rules: **no undo** (the
undo of orders, task 90, is off for the whole game); **permanent death** (a banished wizard is
defeated at once, whatever the Spell of Return setting (task 585) says, and the spell is not
available); and **one save file, constantly updated**: the game saves automatically after every
command that matters and at every turn end into a single file, cannot be saved elsewhere, and
cannot load an older state (loading the file continues the game, and a crash or quit never
rolls back). Store the flag in the saved game settings, show it in the new-game screen (86)
and on the hall of fame entry (88) so hardcore wins can be told apart, and make the client
refuse the quick-load key and the file picker while it is on. Closely related to the settings
work of tasks 89 and 90.

**9. Battle success hint.** Before the player commits to a fight (moving into an enemy stack,
a lair or a city), show an estimate of how it will go: a win chance, the expected losses on
each side and a plain verdict ("easy", "even", "risky", "hopeless"). Compute it by
simulating the battle many times with the real resolution code (`battle/attack.rs`, seeded
and thrown away, using the strategic auto-resolve of task 39 for speed) on copies of the
units as they stand, including walls, spells and heroes as they get implemented. Show it in a
tooltip over the target square and in the confirmation before an attack, and make its
accuracy and visibility an option (off in hardcore mode, item 8; "fog of war" versions that
estimate from only what the player has seen are an extension). Depends on the battle views
of task 106 and on auto-resolve (39).

**10. Font and TrueType support.** Today the client draws text with macroquad's built-in font and,
once task 113 is done, with the original's bitmap fonts. Add real font support: load TrueType
and OpenType files (and bitmap fonts in a simple data format) through a font setting, so a
player or a mod can choose the UI font, with a **font set** per role (title, heading, body,
small, numbers) and fallbacks when a glyph is missing. Needs: a text layout layer shared by
all screens (measure, wrap, centre, shadow and outline, colours, scaling that follows the
window), crisp rendering at any scale (rasterise at the target size, cache glyph atlases), full
Unicode and right-to-left and CJK coverage as an extension (the game's names and help are
English today but modded content may not be), and a licence check: only fonts under licences
the project can ship go into the repository (for example an open blackletter pixel-style face
to stand in for the original's display font), while the original's bitmap fonts stay local.
Modding support (item 5) should let a mod ship its own fonts and map them to the roles.

Chosen candidates for the replacement set (found by the project owner, October 2026): **Cloister
Black** for city names and other large writing, **Carolingia** for mid-sized text and **Xentype**
for small text. Their licences and the licence of any files we would ship have not been checked
yet; check them against the CC0 repository rule above before adding any font file, and keep the
files out of the repository if they cannot be redistributed (the set can then be a local
download, like the original's art).

**11. Higher resolution modes.** The client keeps the original's 320 by 200 layout and scales it by a
whole factor (`frame::Layout`, the city screen's `Layout`), so a bigger window just shows a
bigger copy. Add real higher-resolution modes: layouts designed for 1920 by 1080 and beyond
and for wide and ultrawide windows, where the map view shows more tiles instead of larger
ones, side panels use the extra width (lists, message log, minimap, spell and research
panels), the city screen spreads out (a larger cityscape, more room for units and
enchantments), and UI scale is a separate setting from the map zoom. Needs the font work (item
10), art that holds up at the new sizes (items 6 and 12), a layout system with breakpoints
(compact, standard, wide) shared by all screens, high-DPI handling, and a screenshot test per
breakpoint. Keep the original layout as one of the modes.

**12. Replacement art.** The placeholder art comes from the author's own copy of the original and is
local only; the repository must stay CC0 and free of IP-protected art, music and text. Before any public release, source
art that can be shipped: commissioned, generated or CC0 pieces for terrain tiles, units,
buildings, portraits, the interface frame, battle scenery and fonts, in the same sizes and
file layout as the extracted set so the client loads them unchanged (`assets/` next to the
code, `assets-local/` still taking precedence for a player who has the originals). Decide the
style (faithful pixel art, or a new look suited to the higher resolutions of item 11), list
every picture the client uses (the manifest of `f4x-extract art` is the checklist), and give each
a source and licence in a credits file. Moved here from the task list (formerly task 79)
because it goes beyond the original game.

**13. Different settlement icons by race.** The original draws every city on the overland map with the
same walled or open picture in five sizes, recoloured by banner; only the city screen's
houses differ by race (standard, tree or mound houses). Give each race its own settlement
look on the map and wherever a settlement is shown: its own pictures for outposts, hamlets,
villages, towns and cities, with and without walls, in the owner's banner colour, so that an
orc town, a dark elf grove and a lizardman mound village can be told apart at a glance.
Needs art for fourteen races (item 12 for anything not drawn from the original), a settlement
sprite table in the content data (race, size, walls) with the original's pictures as the
default, a client lookup that falls back to the default, and the same set in the minimap's
city markers and the city screen's header.

**14. Fortress art by magic type.** The original draws every wizard's fortress as the same grey tower
(the city picture's fortress sprite and the tower on the overland map). Give the fortress a
look that follows its wizard's strongest realm: a white spire for life, an overgrown living
tree for nature, a bone tower for death, a tower of fire and cooling lava for chaos, a sparkling
glass tower for sorcery, and a plain tower for wizards with no clear lead or for arcane. Choose
the look from the wizard's books when the fortress is placed (and keep it if the books change,
or follow them: decide), store it with the player, and draw it in all three places that show a
fortress: the cityscape, the overland city marker (a small flag or badge on the city) and the
city list. Needs art (item 12 for anything not from the original), a fortress sprite table in
the content data keyed by realm with the original's tower as the default, and a modding hook
so a mod can add looks for its own realms (item 5). Closely related to the banners and
settlement icons by race (item 13).

**16. Map editor.** Edit terrain, rivers, roads, features, nodes, towers, lair guards and treasure,
start positions and fortress sites, and save a map file that a new game can load instead of
generating one. Built on the layers in `docs/map-layers.md`; needs a map-file format with the
`Lair` and `Node` records and validation (the generator's rules as warnings, not errors).

**17. MCP server for remote control.** An MCP server that exposes the game: read state (map, cities,
units, players), issue `Command`s and receive `Event`s, save and load. Lets a Claude session or
other agent play, test or script the game; should run headless on the core crate with a seed, and
share the console's command set (item 15, done: `docs/console.md` section 5 is the interface). Authentication and a read-only mode matter if it is
reachable from outside the machine.

**18. Hot seat multiplayer.** Several human wizards on one machine, taking turns, with a hand-over
screen between players so nobody sees another's fog of war or research. Needs per-player views
(already derived from the fog), no shared selection state between turns, and an AI/human flag per
wizard in new-game settings.

**19. Asynchronous multiplayer.** Play by message: each player takes a turn when it is theirs and
the game state moves on, perhaps through a Discord bot that posts turn notices and takes the turn
files. Needs a deterministic turn file (a start snapshot plus a command log: the recording of task 74 in `game/replay.rs`), a
server or shared store to hold the game and enforce whose turn it is, and rules for timeouts and
skipped turns. Builds on hot seat (item 18).

**20. Early-game speed-up options.** New-game settings that skip the slow start: more of the map
explored at the start, extra starting units, extra mana and gold, a few starting buildings in the
capital, and a faster research start. Each as an independent option with sensible presets
(e.g. "quick start"); recorded in the saved settings so a game remembers how it began.

**21. (Not needed) Faster outpost growth.** An outpost grows by chance, a few tens of people a turn
(`grow_outpost` in `city/growth.rs`; the chance comes from the race, the land and features, task 21 of
`TASKS.md` for the original's edge rules). **Changed 24 October 2026:** it needed 100 tens, one
citizen, to become a city, ten times the original's 10 (and it started at 0 tens, not 3, and never
died at 0); that was our bug and is fixed, so the rate is now the original's. The text below was
written before the fix. In AI-against-AI soak runs (`f4x-tools soak`) most outposts were still outposts after 100 to 250
turns: one wizard held 15 "cities", 12 of them outposts, which stalls expansion, production and
army building. Add a rule option for how fast outposts grow: the original's rate (the default
while parity is the goal), a multiplier on the growth chance and the tens gained, or a fixed
number of turns to grow into a city. Housing production in an outpost (it already exists as a
production item) could add to it. Keep it a rules setting in the saved game settings, so a game
remembers which rate it used. The soak numbers must be run again first: after the fix the engine's rate is the original's, and
this option is only worth building if outposts are still too slow.

**22. Alternative effect for the Charismatic retort.** The retort's effect in the original is on
diplomacy: the wizard's positive diplomatic effects are doubled and its negative ones halved, and
it pays half for hiring heroes and mercenaries and buying items (`diplomacy.md`; the hiring part
is in tasks 48 and 49). Our AI will behave differently from the original's, so a retort that
rests on how the AI's attitude moves may do little or nothing there. Give it an alternative
effect that does not depend on the AI's behaviour, as an option beside the original's effect
(the original's stays the default while parity is the goal). What the effect should be is open;
decide it when the AI's diplomacy is settled (tasks 64 to 66), and keep the retort's pick cost
and the help text in step with whatever is chosen (`HELP_CORRECTIONS`).

**23. Packaging for Linux.** A Linux build: static or well-documented dynamic binaries (the
macroquad dependencies: X11 or Wayland, OpenGL, ALSA once audio exists), an AppImage or tarball,
a `.desktop` file and icon, testing on a current Debian/Ubuntu and Fedora, and the same
local-assets workflow as the Mac build (T81). A release workflow on GitHub Actions.

**24. Content pass and balance.** Fill out races, units, buildings and spells from the imported
data, then balance with AI-vs-AI soak runs (needs AI and combat).

**25. Battle AI tournament.** The held-out score against the scripted AI says little about which
trained network is strongest, so pit the networks against each other (`learning-ai.md`, section 7).
*Entries:* a few parent checkpoints (picked by a neural-against-neural round robin among the
checkpoints of a run, not by the score against the scripted AI) and many variants of each. A
variant is a parent plus (a) a *directed nudge*: an offset added to the final-layer bias of one
kind of output score of the policy (`net.rs`: the wizard head's bias on the spell slots against
"skip" for more or fewer casts, on "flee" for earlier or later flight, the melee and shoot
pointer biases and the "done" and move biases for aggressive or passive play, the mana head's bias
for casting at full strength, the wall-attack biases), and/or (b) a small Gaussian noise on the
weights, scaled per tensor. A nudge is a plain edit of numbers the checkpoint already holds
(`PolicyNet::nudged`, with a test that a zero nudge changes nothing); how strongly each knob moves
behaviour is measured, not assumed (try about 0.5, 1 and 2 per knob). Spell-slot nudges act more
on wizards with many spells, and the game has no true "defend" order until item 26, so "defensive"
means advancing less, passing sooner and fleeing more. Every variant is labelled by its *measured
style profile* (casts and mana a game, flee rate, units lost and killed, battle length), not by
what it was meant to do.
*Format:* neural against neural only, advice hidden, on a fresh scenario bank. The unit is the
*pair*: one scenario with the same dice, played twice with the sides swapped. The side with more
wins takes the pair; when the wins are level (one win each, or two draws by the step limit; a win
and a draw is a win for the winner), the *tie-breaks* decide, in order, taken over the two games: (1) more
enemy units killed; (2) more own units surviving; (3) more total hit points at the end, own minus
enemy (own positive, enemy negative); (4) less mana spent. If all four are level the pair is a true draw and
counts half a win to each side. The match file says which step decided each pair. A *match* is a
batch of pairs, and whoever has more pair points (1 for a pair won, half for a draw) wins the
match. Only if the points are level after the whole batch is there a seeded, logged coin flip;
the match file records that it was used. About 30 entries play a Swiss stage (short matches of 15 pairs, about 6 rounds),
rated by a Bradley-Terry fit of the pair results; the top 8 play single-elimination knockout with
longer matches of 101 pairs. The summed training reward is only logged. Report whether any variant beats its parent, which styles win, and (for information,
not for selection) how the final 8 do against the scripted AI. Head-to-head results can go in
circles between styles, and the winner is the best of its pool, not necessarily good against a
human.
*Code:* `examples/make_variants.rs` (recipes in, checkpoints with `.shape.ron` and a recipe note
out), `examples/tournament.rs` (profiles, Swiss stage, rating, knockout), `scripts/tournament.sh`
in the style of `train_battle.sh` (`-h`, fresh folder under `models-local/`, `--detach`,
`--notify`, `--dry-run`, a row in `scripts.md`), and a test that one network on both seats splits
a pair evenly. Time a batch of net-against-net games first to size the stages; run it when no
training run is using the CPU. The timing and the other sizes (entries, rounds) are proposals; the match sizes of 15 and 101 pairs are agreed.
*Status:* the tools are built (16 October 2026): `f4x_learn::duel` (pairs, tie-breaks, matches),
`net::Nudge` and `PolicyNet::nudged`/`jittered`, the examples `make_variants` and `tournament`
(`rank`, `run`, `vs`) and `scripts/tournament.sh`; see `learning-ai.md` section 7.9. The first
full tournament has not been run, and the strengths of the nudges still have to be measured.


**26. Hold command in battle.** A new order for a unit in battle, *Hold*: it ends the unit's turn
without attacking and gives it a defensive bonus until its next turn. A rule beyond the original
(which has only "done", with no bonus), so it is a rule-set option in the saved game settings, off
while parity is the goal. Open points to decide: the size and form of the bonus (for example +1
defence or a higher chance to block, not stacking with shields or defence spells, or stacking
with a cap); whether a unit may hold after moving or only when it has not moved; whether it
works for ranged units and units with no melee attack; and whether a held unit that is attacked
in melee strikes back as usual. Needs: the command and its legality in `battle/` (shown by
`Battle::legal_commands`, refused if the option is off), the bonus in the defence calculation and
the battle view, a console and client control (a key and a button beside "done"), a plain
description in the help text corrections (`HELP_CORRECTIONS`), and scripted-AI use (hold with
units that guard a wizard or a wall, or when the odds say wait for the enemy to come). For the
learning AI (`learning-ai.md`), Hold is a new order in the unit vocabulary (`act.rs`): the
observation and checkpoint signature change, so older checkpoints are refused and the networks
must be retrained; it also gives item 25's passive nudge a real "defend" order to push towards.

**27. More collateral damage for spells.** Today a damage spell aimed at a city or at the square it
stands on hurts only the units there (Fire Storm, Ice Storm and Black Wind through
`TargetSel::UnitsOnChosenSquare`; the strikes of `battle/` leave the city alone, and only
Earthquake, Call the Void, Chaos Rift and the new Armageddon volcano touch buildings or citizens).
Make the big attack spells on a city also kill citizens and destroy buildings, by the spell's
strength and element: for example Fire Storm and Fireball burn buildings, Ice Storm and Lightning
kill citizens rather than buildings, Black Wind and Death spells kill citizens only, and a spell
that kills units in the city kills a share of the citizens too. Decide the numbers per spell in
data (a `collateral` block on the effect: chance per building, chance per citizen, an element
filter such as "fire does not harm walls"), not in code; the rules are written once for overland
spells (`devastate_city` in `spell/terraform.rs` and `shake_city` in `spell/wizard.rs` are the
patterns) and once for a battle fought in a city (`city_damage` in `battle/`, which already turns
into lost citizens and buildings when the city falls). Needs: a setting to switch it off, an
event line for the loss (the `WorldSpellStruck` event has the counts), the Chancellor's scroll
(task 272) listing the city losses, the AI valuing it (a city it wants to keep is not bombed with
Fire Storm), and a decision whether Spell Ward and Wall of Darkness-style city spells shield the
buildings. Never kill a city's last citizen. Balance with item 24.

**28. More useful buildings.** Many buildings are only prerequisites for others (Builder's Hall,
Barracks, Smithy, Stables, Sawmill and so on). Give most of them a tangible benefit of their own,
stated in `crates/f4x-def/content/buildings.ron` as ordinary effects so no new engine code is needed where the
rules framework already has the stat. Examples to start from: the Barracks gives each unit
stationed in the city experience every turn; the Builder's Hall lowers the cost of buildings in
the city; the Smithy and the Stables improve the new units built there (a higher weapon quality
or a free move); the Sawmill and the Armourer's Guild add production or defence to the garrison;
the Fighters' Guild trains stationed units. The Shipwright's Guild adds
food in a coastal city (fishing: a food bonus for the sea squares the city works, or a flat bonus per
ocean square in its area). The Granary stores surplus food: a city's food beyond what it eats is
not wasted but kept in a store (a number on the city, saved with the game, with a cap set by the
building) and drawn on when the city runs short, so a famine or a siege costs the store before
it costs citizens; decide the cap, whether the store feeds the city's units as well, whether
it grows the city faster, and whether it is lost when the city is taken or starved. Needs: a list of every building with its current
effect (some, like the temple, already have several), a target for each building that has none,
new stats where a benefit has no stat yet (experience per turn for the stationed units is a
turn-change step in `unit/experience.rs`; a building cost reduction is a stat the production code
reads), care not to make the early game worse (Builder's Hall is built first in every city) and
not to double what a retort or an enchantment already does, building upkeep changes if a benefit
makes the building worth more than its gold, help-text entries written in the local help and
the plain building descriptions the client shows, the AI's building scores (`f4x-ai`) so it
builds them for the benefit and not only for the next building, and an option or mod switch
(item 5) so the original rules stay available.

**29. Reduced upkeep costs.** Basic units cost more to keep than they are worth early on, which
slows the opening and punishes wide armies. Lower the upkeep of the basic units (spearmen,
swordsmen, bowmen, and their equivalents of each race) by a stated amount, and make Settlers cost
nothing to keep, gold and food. Both are data in `crates/f4x-def/content/units.ron` (`upkeep`), so the change is
a rule set: a table of the old and new upkeep per unit, a decision about which units count as
basic (the tier-1 units a city can build with no prerequisite beyond a barracks), whether the food
cost stays (a settler that eats nothing is a rule change in `player/finance.rs` if Normal units always eat
one), what a free settler does to the number of settlers the AI builds (it may build far too many;
cap in `f4x-ai`), and the effect on the gold economy and the unrest of garrisons (a cheaper
garrison makes troop-based pacification cheaper). Check with soak runs (item 24) that wars still
cost something. Hero and fantastic creature upkeep stay as they are.

**30. More interesting strategies for underplayed or underpowered races.** Some races have little
reason to be picked or little to do with their strengths. Give each weak race a distinct way to
play, as a few effects in `crates/f4x-def/content/races.ron` and their buildings and units, never a flat bonus.
Examples: Barbarians get better outposts (faster outpost growth, a larger area, a free early
building) and better Triremes (more moves, more passengers or a cheaper Trireme), pushing them to
settle islands; Gnolls get research from victories in battle (a few research points per enemy
unit destroyed, or points for a captured city) or may sacrifice population for a stated gain (for
example one citizen for production or mana); one idea per race goes on a list kept in this entry
as each is decided. Needs: a survey of how each race performs (item 24's soak results by race
before and after), a list of the new effects and the stats or triggers they need that the rules
framework (`rules-framework.md`) does not have yet (a research gain from a battle needs a battle
event hook; a population sacrifice needs a command and a client control), the AI using each (a
Gnoll AI that sacrifices, a Barbarian AI that expands by sea), the new-game race screen and the
help text describing them, and the divergence recorded in `divergences.md`. Do the balance
with item 24, one race at a time, so the effect of each change is measured alone.

**32. Neutral cities: mana and spells.** Independent cities cannot cast today (task P31 notes it). Give them a small magic of their own, after the base game is playable; it feeds task P31 (a city that can cast is a better ally or enemy, and gives and takes spells in deals) and the battles at its gates.

- **Mana.** A neutral city makes its own mana by the normal rules (buildings, population, nodes in its land, the same `magic_income` sums a wizard's cities feed), keeps it in its own reserve, and pays the standard price for every spell. No distance rules: it casts anywhere it needs to, at the standard cost.
- **Learning.** A city never forgets a spell, also when it shrinks. When it gains a population it learns a new spell only if its population is then higher than the number of spells it knows, so it never knows more spells than its population has reached (a city that shrinks and regrows learns nothing until it passes the old count).
- **Spell lists.** Each race has a list of 20 spells the city picks from, in a data file (`crates/f4x-def/content/`, one list per race, in the rules framework's spell ids), chosen to suit the race (a first pass from the races' realms and the retorts they favour). The pick is a random one: a d20 roll on the list from the game's seeded generator (so it is repeatable from the seed); if the spell rolled is already known, the city goes down the list to the next one it does not know, starting again at 1 after 20; a city that knows all 20 learns nothing more. Spells need no research and no books. A smarter pick (weighted by what the race favours) can come later.
- **Casting strength.** Needs a formula, since a wizard's strength comes from casting skill and a city has none. A start: the casting skill of a city is its population plus a share of its mana income, capped by a number per size, and it decides which spells it can afford in one go and the strength of the ones that scale (Fireball, Dispel Magic's strength, resistance modifiers in battle). Decide the shape and write it into `magic.md`.
- **Use.** In battle the city casts as the defender (and as the attacker when a raider sallies out, task 154) with the combat AI's spell rules for a wizard, within the city's mana and the battle's casting limits; overland, a first version casts only what helps its own cities and defence (wards, Wall of Fire, healing, summons for the garrison). The AI wizards' answer to a city that casts: they weigh the risk in the attack decision.
- **Interaction with deals (task P31).** This item makes it possible to learn spells from neutral cities: a city can teach a spell it knows, for a gift, for its liking of the wizard, or as the price of a deal (a wizard learns it under the normal limits for learning spells: the books, the realm and rarity rules and the spells it already knows). Tribute in mana, and spells given as a gift, work on the city's reserve. The offers themselves belong to task P31, which comes first.

Needs: the data (20 spells for each of the 14 races), a `Magic`-like record for a neutral player in the core (known spells, reserve; saved with the game), the learning step in the growth code, the casting strength rule, the battle and overland AI hooks, the client showing what a neutral city knows when the player has the knowledge (the enemy city screen), help text, and tests with fixed seeds. Do the balance with item 24's soak runs, since a city with spells changes every siege.

**36. Multiplayer (to be discussed).** Nothing is decided; this item holds the questions so the scope is settled before any work (`design.md` section 13 asks the same). Items 18 (hot seat) and 19 (asynchronous play) are the small versions; this one is for the rest, mainly play over a network at the same time.

- **Scope.** Which kinds: hot seat (18), play by turn files (19), simultaneous turns over a network, or several of them. Whether wizards may be human and computer mixed, and how many humans (the game has up to five wizards).
- **Turn model.** The game is turn based with every wizard moving in the same turn. Options: all humans play at once and the turn ends when all have ended it (with a timer, or a vote to end early); or in sequence, one wizard at a time. Battles between two humans need both players present (a battle is played command by command, so a wizard who is away needs auto-resolve or an AI standing in).
- **Authority.** One side holds the game and checks every command (a server, or the host's game), and the others send commands and receive events. The core already works this way: a front end sends `Command` and gets `Event` back, and the game is deterministic from a seed.
- **What exists for it.** Recording and replay for local play (task 74, `game/replay.rs`): a command log with creation numbers in place of entities, a start snapshot, and replay from it. That is a base for turn files and for resynchronising a client, but not a network protocol.
- **What is missing.** A wire format and versioning for commands and events; per-player views of events (a wizard must not be told what its fog hides: `player/view.rs` limits what a player may read of the game, but there is no filter for the events a command returns); the hidden information in the random stream (a client must not run the generator ahead); battles in progress are not saved, so a dropped connection loses them; undo; handing control of a wizard to the AI when its player leaves; cheats and the console in a shared game; chat and diplomacy messages between humans (the wizard diplomacy of `diplomacy.rs` is for AI and human alike).
- **Open questions.** Which scope first; peer to peer or a central server; how to settle disconnects and timeouts; whether the AI may be replaced by a human mid-game; how saves, replays and the Hall of Fame treat multiplayer games.

**37. Record the battle AI's orders for replays.** Replays (task 74, `game/replay.rs`) record the commands the game is given, and every order a person or the scripted battle AI gives goes through `Game::apply` and is recorded. A learned battle brain (`set_battle_brain`, `next_battle_command`, `f4x-learn`) is asked for its orders inside the core (the auto-resolve loop in `battle/game.rs` calls `battle_command` directly), so those orders are not recorded, and a replay asks the brain again, which gives the same battle only if the brain answers the same way every time.

- **Aim.** A replay of a game with a learned brain plays the same battles whether or not that brain is present, in the same version or another.
- **How.** Record each order the brain gives, in order, with the battle's start, as part of the recording (the commands become `Battle` orders in the log, or a list kept with the `BeginBattle`/`AutoResolve` entry), and on replay feed the recorded orders back in place of asking the brain. The replay then needs no network weights.
- **Needs.** The record hook in the auto-resolve loop and in the client's brain-driven battle (`battle_screen.rs`), a replay that takes orders from the log when it finds them, a test with a fake brain whose answers change between runs, and the file format note in `docs/console.md`.
- **Priority.** Low: the learned AI is not part of feature parity with the original (decided 17 October 2026).

**38. Casting mod.** Three changes that belong together, to make the wizards that cast from afar
playable and to restore a preset the base game shortens: a decision per change whether it becomes
the default, a rule-set option or a mod (see 5), with the three switched together as one mod at
first (requested 18 October 2026).

- **Half the casting distance penalty.** In battle a spell costs more the further the battle is from
  the wizard's fortress (`range_percent` in `battle/casting.rs`, the original's
  `Combat_Casting_Cost_Multiplier`): half the cost at the fortress itself, then the base cost, and
  half the base cost again for each five squares past the fifth, up to three times (`x1.0` to
  `x3.0` on the other plane or with no fortress). The mod halves the added part: a quarter of the
  base cost for each five squares, up to twice (`x1.0` to `x2.0`), and the fortress discount stays.
  The Channeler's rule (two squares away anywhere but at the fortress) is unchanged. The budget a
  wizard has to spend (`budget` in `Caster`) and the AI's cost estimates read the same function, so
  they follow. Make the figures data (a table in `rules.ron`: the first step, the step width, the
  added share and the cap) so a mod can set its own and the original's are the default.
- **A Channeler that costs one pick.** The retort costs two picks (`retort_cost` in `wizard/mod.rs`);
  the mod makes it one. The cost of the other two-pick retorts (Warlord, Divine Power, Famous,
  Infernal Power) stays. Move the pick prices into data (`rules.ron` or the retort table) so the
  creation screen, the custom-wizard checks and the AI's wizard builder all read them.
- **Lo Pan's fifth sorcery book.** `crates/f4x-def/content/wizards.ron` gives Lo Pan four sorcery books, one fewer
  than the original, so that he fits the eleven picks (divergence 21). With the Channeler at one
  pick, five sorcery, five chaos and Channeler come to eleven picks again, so the mod gives him the
  original's five sorcery books with no other change. Without the mod he stays as now.

Needs: the data tables above and their loader, the mod switch (item 5: until that exists, a
game setting in `GameSettings`, saved with the game and shown beside the other rule options), the
new-game screen and the help texts that name the Channeler's cost, the casting range and Lo Pan's
books (`HELP_CORRECTIONS` for each, since the help states the original's figures), tests on the
real data for each change (the cost at 0, 5, 10, 15, 20 and 25 squares, the creation screen's
pick count with the Channeler, Lo Pan's books and picks), and an entry in `divergences.md` for
each change.

**40. Life and death nodes.** The base game has magic nodes for sorcery, nature and chaos only. Life and death
nodes are an option of the game setup (`docs/game-settings.md`), off by default so the base game stays as it is.
(The setting that lifts the ban on holding life and death books together is task 451 in `TASKS.md`, not part of this
item.)
- **Life nodes and death nodes**, built like the other three: a node is a terrain feature entity (`feature/`, `Node`
  with its realm; `crates/f4x-def/content/features.ron`) on a base terrain of its own (`TerrainClass` gains `LifeNode` and
  `DeathNode` beside `SorceryNode`, `NatureNode` and `ChaosNode`, with food, production and movement values in
  `crates/f4x-def/content/terrain.ron`; the nature node counts as forest and the chaos node as mountains, so decide what a life
  node and a death node count as, if anything). Parts:
  - **Placement and guards:** map generation places them with the node budget (`crates/f4x-def/content/lairs.ron`, the node
    budget formula of `docs/lairs.md`), the realm picked among five instead of three; the guards of a node follow its
    realm, so a life node is held by life-realm creatures and a death node by death-realm ones (as the original does
    for the other realms); a new realm's guard table is data.
  - **What a node gives** a wizard who owns it: read first how the other three do it (`node_income` in
    `wizard/magic_income.rs`, the node's power and aura squares, the owner's books of the node's realm, the
    original's `Node` rules) and give the new two the same rule with their own numbers as data. If the original
    gives the three realms different kinds of power, decide and write down what a life node and a death node give
    instead.
  - **Spells that touch nodes:** Meld with Node (the spell and its unit rule), Corruption on
    a node, Warp Node, Change Terrain's refusal on a node, and the awareness spells (Nature Awareness shows nature
    nodes; add whichever shows the new ones), so each takes the five realms, not three. Death Wish, Armageddon and
    the other world spells that look at nodes are checked for a node of any realm.
  - **Art and text:** a node picture and a map tile for each (`docs/extract-art.md`; the original has none, so a
    recolour of an existing node picture, with a note in `docs/divergences.md`), the node name keys in
    `crates/f4x-client/content/text/en/`, and the node colour the client draws on the minimap and the surveyor.
  - **AI:** the scripted AI's targets and the learned AI's observation of nodes include the new realms; saves list
    nodes by realm already, but old saves with only three realms still load.
- **Tests:** a game with the setting off is unchanged (the base tests stay green); with it on, map generation places
  all five node realms over many seeds, a wizard owning each node gets its bonus, and Meld with Node works on a life
  and a death node; a soak shows no crash.

Needs: the node entity work (`feature/node.rs`, done), the game-settings screen (`docs/game-settings.md`), and the art
tools if the node pictures are recoloured (`docs/extract-art.md`). It touches the AI's node targets (task 339), so do it
after that task or tell the AI work.

**41. Free sounds.** The original has sound for only a few things (`docs/sound.md` section 4: clicks, combat, spell
casts) and the repository may not ship its audio. Look for free-to-use sounds (CC0 first; CC-BY with credit where the
licence allows redistribution; no non-commercial or no-derivatives licences) from sources such as OpenGameArt,
freesound.org, Kenney's packs, and for music Incompetech-style libraries or pieces generated locally with the
`soundgen` project, and use them for two jobs:
- **Events with no sound today:** end of turn, city growth and shrinking, a building or unit completed, a unit moving
  on the map, a unit dying or taking a hit in battle, a diplomacy proposal arriving, a wizard banished or defeated,
  an item found, a hero hired or levelling up, a turn that ends with a shortage, and the new content of other planned
  items (life and death nodes, new wizards). Each gets a cue name in `sound.ron` and a switch under Sound Effects.
- **Replacements for the original's sounds and music:** one free file for every cue the original has (the 154
  effects and 118 songs of `index.tsv` are the checklist), so a build with no original files is complete and the
  game is no longer tied to the author's copy. The files go in `assets/audio/` beside the code and
  `assets-local/audio/` still takes precedence for a player who has the originals, as for the art (item 12).
Keep a credits file with each file's source, author and licence, and a check (a script or a test) that every file in
`assets/audio/` is listed there. Needs the audio layer and the cue names of tasks 76 and 97.

**42. Mines, lumber camps and hunter's lodges on the map.** `mapback.lbx` entries 87 (mine), 88 (lumber camp) and 90
(hunter's lodge) are extracted, but the original game never places them: they are not part of it (they may come from
Caster of Magic). Moved here from the on-hold task 124. Decide what they are for before drawing them: a site type on
the map that a building, spell, unit or ability can refer to (a mine for ore and mana, a lumber camp for wood, a lodge
for food), placed by the generator and shown with these pictures. Low priority; do it with item 4 (the community
patch and Caster of Magic content).

**43. Proper windowing support.** Partly done (status of 14 November 2026). Everything used to be drawn onto the same pane: a screen paints the whole frame, and a pop-up was either a function that took the frame over or a box painted over what the screen drew before. A modern game should have windowed layers: each box, panel and screen a window with its own place in a stack, so that a window can stand over a live screen, several can be open at once, and input goes to the one on top.

**Done (task 389, `crates/f4x-client/src/modal.rs`):** the **modal stack**. Questions and notices are layers over the screen below: drawn after it (dimmed if the player wants), the newest on top gets all the input of the frame (nothing below sees a click or a key), a box of a kind that is already open is replaced, and notices that turn processing makes wait in a queue by priority until nothing is open. About 25 kinds use it (help, confirmations, events, offers, the built-building and level-up boxes, the scenes). Esc closes the top modal. Tests can click through it (tasks 395 and 406).

**To do:**
- **Non-modal windows.** Every layer today is modal. A window should say whether it blocks the ones below or not, several should be open at once, and input should go to the topmost window under the pointer (a click outside a modal window stays swallowed).
- **The help pop-up as a window.** `help_popup.rs` still keeps a frozen picture of the frame it opened on (so a click that closes it cannot reach a button below). The confirmation boxes are modals; the help box should become a live window over the live screen, with Expanding Help drawn on it.
- **Screens as windows.** The screens the original draws full-screen stay full-screen by default, so the look does not change. The new part is that the map can stay visible and live behind a box such as the city screen or the advisors.
- **Drag, resize and place.** Windows can be dragged, resized (with item 11's higher resolutions) or placed on another monitor. Nothing of this exists in the client yet (the alchemy window's star is a slider, not a window drag).
- **A render target for each window,** which also gives fades, the original's page-flip effects and drop shadows without each screen doing its own. Task 424 is the open decision on drawing the frame into one render target; settle it first.
- Sketch of the stack data, to be settled when it is taken up: a window has a rectangle (in the original's 320 by 200 coordinates, scaled as now), an order, a title or none, and a modal flag.

Related: items 6, 10, 11 and 39 (graphics, fonts and resolution), task 289 (help pop-ups) and the full-screen scenes
of `end_scene.rs`. The request that opened this item had more detail after "windowed layers"; add it here.

Note: use macroquad window <https://docs.rs/macroquad/latest/macroquad/window/index.html>

**45. Start games from a seed in the UI.** The map seed is a command-line option today (`--seed N`); the Game settings
screen only shows it (task 148). Let the player type or paste a seed on the new-game settings, so a map can be shared and
replayed without the command line:
- the Game settings screen (`options_screen.rs`, from the start menu) gets a number field for the seed, with a "random"
  choice that draws a fresh one, and the field is part of the options that start the next game;
- the same seed with the same settings must give the same map (check `GenParams` and the settings that feed the generator,
  and say on the screen which settings the map depends on);
- a running game keeps showing its seed read-only, and the hall of fame and the save slots may list it;
- the console's `new` command takes `seed=N` as well, if it does not yet.
Related: task 148 (the seed display), the new-game flow (`newgame_flow.rs`), `docs/game-settings.md`.

**46. Find the use of the unreferenced art files.** (Moved here from the task list, formerly task 147: not needed for functional parity.) ReMoM's loader has no reference to four LBX files that `f4x-extract art` extracts fine: `combat`, `desert`, `tundra` and `cmbtwall`. `docs/extract-art.md` lists them as unknown rather than guessing. Find out what they are for: look at the pictures (frame counts, sizes, which terrain or battle element they resemble), search the original's disassembly and the other reverse-engineering sources in `reference/` for their names or for the loader slots they would occupy, and compare with the battlefield terrain and wall code (`cmbtwall` may be wall pieces beside `citywall`; `desert` and `tundra` may be terrain variants or battlefield backgrounds). Record the result in section 9 of `docs/extract-art.md` (replace the "unknown" row) and, if one is a real gap in the client, add a task for it. If nothing can be found, say so in the doc and leave the files unused.
