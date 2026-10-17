# Planned modifications

Changes and extensions to do **after the basic game is playable** (the core loop of
`docs/TASKS.md` working: map, cities, magic, combat, AI, victory). Unlike the task list, these
are not gaps against the original: they are improvements and options on top of it. Keep the
table in ascending order by number; new items take the next free number.

Related tasks in `TASKS.md` are given as `T<number>`.

## Table

| # | Area | Summary |
|---|---|---|
| 1 | Movement | Faster movement along rivers |
| 2 | Options | Spell of Return setting: off, player only, all wizards |
| 3 | Options | More NPC wizards (the base game has up to four opponents) |
| 4 | Content | Rules changes and new items from the community patch and the Caster of Magic expansion |
| 5 | Platform | Modding support |
| 6 | Client | Upscaled graphics |
| 7 | City | Smarter city autobuilder |
| 8 | Options | Hardcore mode |
| 9 | Client | Battle success hint |
| 10 | Client | Font and TrueType support |
| 11 | Client | Higher resolution modes |
| 12 | Content | Replacement art |
| 13 | Client | Different settlement icons by race |
| 14 | Client | Fortress art by magic type |
| 15 | Tools | Console for game commands (first version done) |
| 16 | Tools | Map editor |
| 17 | Meta | MCP server for remote control |
| 18 | Meta | Hot seat multiplayer |
| 19 | Meta | Asynchronous multiplayer |
| 20 | Rules | Early-game speed-up options |
| 21 | Rules | Faster outpost growth |
| 22 | Rules | Alternative effect for the Charismatic retort |
| 23 | Platform | Packaging for Linux |
| 24 | Content | Content pass and balance with AI-vs-AI soak runs |
| 25 | AI | Battle AI tournament: neural against neural, with varied entries |
| 26 | Rules | Hold command in battle: ends the unit's turn and gives a defensive bonus |
| 27 | Rules | More collateral damage for spells: attack spells on cities kill citizens and destroy buildings |
| 28 | Rules | More useful buildings: most buildings get a tangible benefit of their own |
| 29 | Rules | Reduced upkeep for basic units; no upkeep for settlers |
| 30 | Rules | More interesting strategies for underplayed or underpowered races |
| 31 | Meta | Diplomacy with neutral cities: attitude, a leader portrait, talking, deals |
| 32 | Magic | Neutral cities: their own mana and spells |
| 33 | AI | Neutral cities that play strategically (replaces the raiders of task 154) |
| 34 | Rules | Larger tiles: up to 20 units on a square, armies and battles still limited to nine |
| 35 | Content | Palette-swap wizards: variants of the 14 ready-made wizards |
| 36 | Meta | Multiplayer: scope and design (to be discussed) |

## Descriptions

**1. Faster movement along rivers.** Rivers exist as a tile flag (T1, done) but do not affect
movement. Add a rule that a step between two river tiles (or from a river tile to the shore at
its mouth) costs less, like roads do (half a move; decide whether it stacks with roads, which
should not give a quarter move), for land units and perhaps cheaper still for boats-on-rivers if
ships ever sail them. Make it a rule in data (a movement cost modifier per river step) so it can
be switched off, and show river-assisted paths in the path preview. Needs a decision on
direction (up or down stream) before any asymmetric rule; the river layer currently has no flow
direction.

**2. Spell of Return options.** Today every banished wizard can cast the Spell of Return
(`fortress.rs`, `magic.md` section 12), and an AI wizard's resignation rule is task 82. Add a
new-game setting with three values: **off** (banishment is final: the wizard is defeated),
**player only** (the human may return, AI wizards are defeated on banishment) and **all
wizards** (as in the original). Store it in the game settings saved with the game (new field,
`serde(default)` = all wizards), check it in `capture_city` where banishment is decided, and
mention it in the new-game screen (T10, T75).

**3. More NPC wizards.** The base game allows one to four opponents (T10). Make the count a
setting beyond four: more wizard names, portraits and banner colours (the client's
`player_color` palette), larger default maps and more starting positions so wizards do not crowd
each other (spread by minimum distance rather than a fixed table), a check that `PlayerId` (a
`u8`) and the fog and save layouts cope, and a diplomacy screen that scales (T63, T75). Balance
note: AI cost grows with wizard count, so measure turn time first (T65).

**4. Community patch and Caster of Magic content.** Collect the rule changes of the community
patch (its files are in `reference/mom-classic/Master of Magic Community Patch/`) and of the
Caster of Magic expansion: list every changed number, ability, building, unit, spell and item;
decide per change whether it becomes the default, a rule-set option or a mod (see 5); and add
the new items, units and spells to `content/` through `scripts/import_mom.py` where they
come from original rules data, or write them by hand. Details of the
expansion are not yet gathered here, so the first step is a survey document in `docs/` listing
what exists, before any code. The importer's conflict rules (`help-vs-implementation-conflicts.md`)
apply: patch behaviour that fixes a marked bug wins; anything else is an option.

**5. Modding support.** The rules are already data (`content/*.ron`, validated by
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

**7. Smarter city autobuilder.** The basic autobuild (T16) picks the next sensible building.
Improve it: weigh the city's needs (food deficit, unrest, gold, production, defence), the
buildings' upkeep against the city's income, the race's strengths, what the neighbours already
provide, the wizard's strategy (retorts, books) and the game stage; avoid buildings that are
about to be obsolete; let the player set priorities or a build profile (growth, production,
research, defence); and share the evaluation with the AI's city production (T65) so both use
one scoring function, tested on fixed city situations.

**8. Hardcore mode.** A new-game option for one-life play, bundling three rules: **no undo** (the
undo of orders, task 90, is off for the whole game); **permanent death** (a banished wizard is
defeated at once, whatever the Spell of Return setting of item 2 says, and the spell is not
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
every picture the client uses (the manifest of `extract-art` is the checklist), and give each
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

**15. Console for game commands.** First version done: see `docs/console.md` (legal and override commands, the override log, one `Console::execute` hook for the client, the headless runner and future front ends). Still to do from the plan: setting features and lairs, clearing lairs, triggering events, disabling overrides in hardcore mode (item 8). The original plan: a text console in the client (and the headless tools) that runs
cheat and debugging commands through the same `Command` path as play where possible: change a
square's terrain or feature, add units, buildings and cities, set gold, mana or research, reveal the
map, clear a lair, trigger events. Commands that bypass the rules go through one `debug` command
family that is logged (and disabled in hardcore mode, item 8) so saves and replays stay honest.

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
files. Needs a deterministic turn file (a start snapshot plus a command log: the recording of task 74 in `replay.rs`), a
server or shared store to hold the game and enforce whose turn it is, and rules for timeouts and
skipped turns. Builds on hot seat (item 18).

**20. Early-game speed-up options.** New-game settings that skip the slow start: more of the map
explored at the start, extra starting units, extra mana and gold, a few starting buildings in the
capital, and a faster research start. Each as an independent option with sensible presets
(e.g. "quick start"); recorded in the saved settings so a game remembers how it began.

**21. Faster outpost growth.** An outpost grows by chance, a few tens of people a turn
(`grow_outpost` in `city_growth.rs`; the chance comes from the race, the land and features, task 21 of
`TASKS.md` for the original's edge rules) and needs 100 tens, one citizen, to become a city. In
AI-against-AI soak runs (`f4x-tools soak`) most outposts were still outposts after 100 to 250
turns: one wizard held 15 "cities", 12 of them outposts, which stalls expansion, production and
army building. Add a rule option for how fast outposts grow: the original's rate (the default
while parity is the goal), a multiplier on the growth chance and the tens gained, or a fixed
number of turns to grow into a city. Housing production in an outpost (it already exists as a
production item) could add to it. Keep it a rules setting in the saved game settings, so a game
remembers which rate it used. First check the soak numbers against the original's real rate
(`TASKS.md` 21), in case the engine's rate is the part that is wrong rather than the original's.

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
spells (`devastate_city` in `terraform.rs` and `shake_city` in `wizard_spells.rs` are the
patterns) and once for a battle fought in a city (`city_damage` in `battle/`, which already turns
into lost citizens and buildings when the city falls). Needs: a setting to switch it off, an
event line for the loss (the `WorldSpellStruck` event has the counts), the Chancellor's scroll
(task 272) listing the city losses, the AI valuing it (a city it wants to keep is not bombed with
Fire Storm), and a decision whether Spell Ward and Wall of Darkness-style city spells shield the
buildings. Never kill a city's last citizen. Balance with item 24.

**28. More useful buildings.** Many buildings are only prerequisites for others (Builder's Hall,
Barracks, Smithy, Stables, Sawmill and so on). Give most of them a tangible benefit of their own,
stated in `content/buildings.ron` as ordinary effects so no new engine code is needed where the
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
turn-change step in `experience.rs`; a building cost reduction is a stat the production code
reads), care not to make the early game worse (Builder's Hall is built first in every city) and
not to double what a retort or an enchantment already does, building upkeep changes if a benefit
makes the building worth more than its gold, help-text entries written in the local help and
the plain building descriptions the client shows, the AI's building scores (`f4x-ai`) so it
builds them for the benefit and not only for the next building, and an option or mod switch
(item 5) so the original rules stay available.

**29. Reduced upkeep costs.** Basic units cost more to keep than they are worth early on, which
slows the opening and punishes wide armies. Lower the upkeep of the basic units (spearmen,
swordsmen, bowmen, and their equivalents of each race) by a stated amount, and make Settlers cost
nothing to keep, gold and food. Both are data in `content/units.ron` (`upkeep`), so the change is
a rule set: a table of the old and new upkeep per unit, a decision about which units count as
basic (the tier-1 units a city can build with no prerequisite beyond a barracks), whether the food
cost stays (a settler that eats nothing is a rule change in `finance.rs` if Normal units always eat
one), what a free settler does to the number of settlers the AI builds (it may build far too many;
cap in `f4x-ai`), and the effect on the gold economy and the unrest of garrisons (a cheaper
garrison makes troop-based pacification cheaper). Check with soak runs (item 24) that wars still
cost something. Hero and fantastic creature upkeep stay as they are.

**30. More interesting strategies for underplayed or underpowered races.** Some races have little
reason to be picked or little to do with their strengths. Give each weak race a distinct way to
play, as a few effects in `content/races.ron` and their buildings and units, never a flat bonus.
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

**31. Diplomacy with neutral cities.** The independent cities act as players with a budget (task 31, `divergences.md` section 2) but can only be fought. Give them a voice, after the base game is playable and after task 292. Design on top of `docs/diplomacy.md` (the wizards' diplomacy) and `neutral_cities.rs`.

0. **Attitude.** Each neutral city keeps a score of how much it likes each wizard, human and AI, from a neutral start. It moves with gifts, deals kept and broken, attacks on it or its neighbours, the wizard's fame and strength; it fades slowly towards neutral. Save it with the city.
1. **A leader portrait for each race**, shown in the talk dialog (new local art or replacement art, item 12; the portraits are art, so local-only like the rest unless made for the project).
2. **A dialog when a unit tries to enter the city:** Attack, Talk, Leave. The AI can use the same choice (a rule in `f4x-ai`: attack the weak, talk to the strong or the friendly). A city with free access (below) is entered without asking.
3. **Talk: the offers** a wizard can make. The city accepts or rejects each one by how much it likes the wizard, whether the deal helps it, and the wizard's overall strength against its own (the same inputs for the human and the AI):
   - **Give a gift** (an item, a unit, gold): improves relations.
   - **Demand tribute** (a unit, citizens, a lump sum of gold, gold per turn, mana): the city decides by attitude and strength. (Cities cannot cast spells now; giving them spells is a separate change.)
   - **Hire mercenaries**: units of the city's race for gold paid to the city.
   - **Free access**: the wizard's units and ships may enter the city freely.
   - **Offer protection**: as free access, and the wizard defends the city when attacked; the city pays the upkeep of the wizard's units while they stand on its territory.
   - **Open trade**: both gain gold each turn by the city's size and whether they are joined by a port or a road.
   - **Surrender**: the city joins the wizard's empire.
   - **Sever ties**: the city ends its deals with another wizard.
   - More offers later (a shared sight, a joint war, passage for a stack through the territory).

Needs: the attitude score and its rules in the core (a `NeutralDiplomacy` module beside `diplomacy.rs`, with the offers as commands and the answers as events, all deterministic), standing deals saved with the game and checked each turn (tribute and trade are paid at the turn change; broken deals change attitude), the entry dialog and talk screen in the client, an AI that makes and answers offers, a console command per offer, news lines (a city that surrendered, a deal ended), and the help text.

**Decided (17 October 2026):**
- **Strength** is the wizard strength score the game already computes for the advisor screens (`astrology.rs`: the Astrologer's power, strength and research figures and the History graph's power; `endgame.rs`'s score for the hall of fame is the related one). A neutral city compares that with its own (its size, buildings and garrison, scored the same way); no new measure.
- **Surrender** adds fame to the wizard who receives it, and lowers the diplomatic standing of that wizard with the enemy wizards (those at war with them or hostile to them), who see a rival grow.
- **Protection and free passage in a battle.** With a protection pact, the wizard's units that stand in the city join the battle at once on the defender's side. The human plays that battle (unless auto-battle is on); an AI wizard's battle is played by the battle AI. With only free passage, the wizard's units sit out any battle, and are evicted if an enemy conquers the city.

**Pacts end only by being broken** (decided 17 October 2026): either side, the city included, may break a protection pact or a free-passage deal by a command; the protector's units in the city are then evicted to a free square next to it. A pact does not end by itself when the units leave. A city may break its own pact, for instance when it cannot afford the upkeep. Breaking has no penalty for now beyond losing the protection (no attitude or fame cost, no warning period); revisit when item 31 is balanced.

**32. Neutral cities: mana and spells.** Independent cities cannot cast today (item 31 notes it). Give them a small magic of their own, after the base game is playable; it feeds item 31 (a city that can cast is a better ally or enemy, and gives and takes spells in deals) and the battles at its gates.

- **Mana.** A neutral city makes its own mana by the normal rules (buildings, population, nodes in its land, the same `magic_income` sums a wizard's cities feed), keeps it in its own reserve, and pays the standard price for every spell. No distance rules: it casts anywhere it needs to, at the standard cost.
- **Learning.** A city never forgets a spell, also when it shrinks. When it gains a population it learns a new spell only if its population is then higher than the number of spells it knows, so it never knows more spells than its population has reached (a city that shrinks and regrows learns nothing until it passes the old count).
- **Spell lists.** Each race has a list of 20 spells the city picks from, in a data file (`content/`, one list per race, in the rules framework's spell ids), chosen to suit the race (a first pass from the races' realms and the retorts they favour). The pick is a random one: a d20 roll on the list from the game's seeded generator (so it is repeatable from the seed); if the spell rolled is already known, the city goes down the list to the next one it does not know, starting again at 1 after 20; a city that knows all 20 learns nothing more. Spells need no research and no books. A smarter pick (weighted by what the race favours) can come later.
- **Casting strength.** Needs a formula, since a wizard's strength comes from casting skill and a city has none. A start: the casting skill of a city is its population plus a share of its mana income, capped by a number per size, and it decides which spells it can afford in one go and the strength of the ones that scale (Fireball, Dispel Magic's strength, resistance modifiers in battle). Decide the shape and write it into `magic.md`.
- **Use.** In battle the city casts as the defender (and as the attacker when a raider sallies out, task 154) with the combat AI's spell rules for a wizard, within the city's mana and the battle's casting limits; overland, a first version casts only what helps its own cities and defence (wards, Wall of Fire, healing, summons for the garrison). The AI wizards' answer to a city that casts: they weigh the risk in the attack decision.
- **Interaction with deals (item 31).** This item makes it possible to learn spells from neutral cities: a city can teach a spell it knows, for a gift, for its liking of the wizard, or as the price of a deal (a wizard learns it under the normal limits for learning spells: the books, the realm and rarity rules and the spells it already knows). Tribute in mana, and spells given as a gift, work on the city's reserve. The offers themselves belong to item 31, which comes first.

Needs: the data (20 spells for each of the 14 races), a `Magic`-like record for a neutral player in the core (known spells, reserve; saved with the game), the learning step in the growth code, the casting strength rule, the battle and overland AI hooks, the client showing what a neutral city knows when the player has the knowledge (the enemy city screen), help text, and tests with fixed seeds. Do the balance with item 24's soak runs, since a city with spells changes every siege.

**33. Neutral cities that play strategically.** Replaces the raiders of task 154 (random troops sent out from neutral cities to cause trouble; dropped, not built). The independent cities become players with goals of their own: they build their own units and use them for those goals, as the AI wizards do, instead of throwing units at whoever is near. Design after the base game is playable, together with items 31 (attitude and deals) and 32 (spells).

- **They are players.** A neutral city already has a player record, a budget and an upkeep (`neutral_cities.rs`, task 292); `design.md` says the AI sees what a human sees and obeys the same commands, so the neutral ones use the same commands too. Give them a driver in `f4x-ai` (a neutral personality and objectives beside the wizards', `ai-objectives.md`), run at the turn change for every neutral player, deterministic and cheap.
- **Goals** (a first set): defend the city and its land; keep the roads and the sea lanes near it clear; take or clear nearby lairs and unguarded nodes for their treasure and mana; take a weak neighbouring city (another neutral city, a wizard's outpost or a badly garrisoned city) when strong enough; trade with friendly cities and wizards (item 31); ask a strong wizard for protection when threatened; found a new outpost with settlers when there is room (the outpost becomes a new independent city).
- **Units.** Each city chooses what to build for its goals from its budget, with the same production rules as anyone (the AI's `economy::cities` logic, tuned for a small city); no free units. Armies are small and local; they go home when the goal is met.
- **Attitude** (item 31) steers who they fight: a city does not attack a wizard it likes, shares an enemy with a protector, and may break a deal when a bigger neighbour offers more.
- **Settings** (`game-settings.md`): `raiders_enabled` and `raider_percent` become one `neutral_activity` setting (off, passive, active, aggressive) by difficulty.
- **Needs:** the neutral driver and objectives in `f4x-ai`; their budget and unit choice; the goal logic above; a way for a human to see what neutral cities are doing (the enemy-city screen, the history graph); news lines for a neutral city taking a city or clearing a lair; tuning with item 24's soak runs (a neutral city must be a worthwhile target and a possible friend, not a wall or a pest); the help text.

**Decided (17 October 2026):** each neutral city is independent: its own player record, units, gold and food, no faction by race and no shared treasury or army. A neutral army can conquer a city, and neutral settlers can found an outpost; the city taken or founded becomes a new independent neutral city of its own (a fresh record with its own budget; the conqueror does not own a conquered city). The conquered city and the conquering neutral city form a protection pact, the same pact as item 31's: while the conquering army stands in the conquered city, that city pays the army's upkeep and the army defends it in a battle (the army joins the defender's side), and unit ownership does not change (the units stay the conqueror's). The pact is a standing deal: it does not end when the army leaves (the city simply pays nothing and has no defender while the army is away, and the pact is in force again when the army, or any of the protector's units, comes back). It ends only when either side breaks it (an explicit command for the human and for the AI, with a cost to attitude, item 31). When it is broken, the protector's units in the city are evicted (moved to a free square next to it) and their owner's AI chooses a new goal for them: explore, go home to defend, clear a nearby node or lair, join another goal of the owner's, or wait; it is the same goal choice the owner makes for any army without orders (item 33), run as soon as the army is evicted. A human owner decides for their own army. Cities of one race do not help each other except through item 31's deals.

**Also decided:** neutral cities set a goal at the start of the game and pursue it (grow, explore, gain access to nodes, explore lairs, build defences, and so on; the list under Goals above), and change it when it is met or the situation changes. There is no growth ceiling for neutral cities beyond the rules that apply to everyone. A city may break its own pact (for instance when it cannot afford the upkeep). The owner's AI picks the evicted army's goal as described under the pacts.

**Also decided:** there is no grace period at the start. A wizard who starts next to an aggressive neutral neighbour has to deal with it. The starting goal is set by race and then adjusted to the situation: for example High Men build towards military buildings, Barbarians go out to attack for loot, and the others have their own leanings (a table by race in `content/`, like the races' other data); the goal is reconsidered when it is met, when the city is threatened, or when its surroundings change (a node or lair nearby, a wizard's outpost close, a trade partner). The `neutral_activity` setting still scales how aggressive they are by difficulty, but there is no number of quiet turns.

**Still to decide:** each race's starting goal and the rules that adjust it (the table), and how often the goal is reconsidered (every turn, every few turns, or on events only).

**34. Larger tiles: up to 20 units on a square.** Raise the number of units that may stand on one square to 20, while an army (the group that moves, attacks and fights together) stays at nine and a battle side at nine. The older design for a larger stack, [`large-stacks.md`](large-stacks.md) (stage 4 of `game-settings.md`), assumed the whole stack fights; this variant keeps the battle at nine, so the battle layouts (`battle/map.rs`), the nine-slot deployment and the battle screen stay as they are, and the cost moves to choosing who fights.

- **Rules.** A square holds at most 20 units (`MAX_TILE`, new). An army, the set of units selected to move or attack together, holds at most nine (`MAX_STACK` stays nine). A battle side has at most nine units (`MAX_SIDE_UNITS` stays nine, and the second copy in `battle/casting.rs` is merged with it).
- **Attacking.** The attacker's army is at most nine units, picked from the square (as today, by selection). The defender's side is chosen from every unit on the defending square.
- **Defending.** If more than nine units stand on the defended square, the defender chooses the nine who fight. The preset is the strongest nine, in this order: heroes first; then combat units by cost, the dearest first; among units of the same kind, the more experienced first; settlers, engineers and other non-combat units last; the human can change the choice in a pick screen before the battle (a list of the units on the square with checkmarks, nine allowed); the AI uses the preset or its own rule. Units that do not fight stay out of the battle and are not hurt. If the defender loses, the units that did not fight follow the fleeing rules (`overland_aftermath` in `battle/game.rs`, task 42): half are lost and a quarter of the heroes, the rest are driven out to a free neighbouring square (or are lost if there is none, as a fleeing stack with nowhere to go); units asleep, confused or held in a web are always lost. If the defender wins, they stay.
- **Cities.** A city's garrison can be up to 20 for the same reason, and the nine fighting units use the city deployment; the rest do not fight. Unrest and every other rule that counts a city's garrison count at most nine units (the strongest nine, in the defenders' preset order), so a larger garrison changes nothing there. The aim is that play is the same as with a limit of nine: the extra units are a reserve, a place to park units, and a choice of who fights. Upkeep does not depend on the square a unit stands on (`finance.rs` sums every unit the player owns), so it is unchanged.
- **Ships and gates.** A ship's passengers, a gate or teleport, and a stack standing on a ship stay by the existing capacities and the 20 limit for a square; an army of nine moves through a gate.
- **Selection and the client.** The main screen's stack panel and the unit list must show up to 20 (scrolling, or two rows) and let the player select up to nine of them as an army; the city screen's garrison list (`.take(9)`, task 119) shows 20 and scrolls.
- **AI.** `f4x-ai` gathers armies in nines (`military.rs`, `naval.rs`) and may leave a larger garrison; it picks defenders by the preset rule.
- **Settings.** `max_stack` in `game-settings.md` splits into `max_tile` (up to 20) and `max_army` (up to nine), with the defaults 20 and 9 (decided 17 October 2026); a game can set both to nine for the old rule.

Needs: the two limits in the core with tests (`movement.rs`, `gates.rs`, `special_spells.rs`, `transport.rs`, `neutral_cities.rs`, `lairs.rs`, the raise-dead room), the defender choice (a command `ChooseDefenders` carrying the units, with the preset when the defender does not answer), the battle start using the chosen nine, the pick screen and the larger lists in the client, the AI's rules, the help text, and a save check (the old limit of nine is already satisfied). Do `large-stacks.md` section 2's list of places built for nine first for what still depends on nine.

**35. Palette-swap wizards.** Variants of the 14 ready-made wizards: the same portrait with changed colours (hair, skin, robes, eyes) and a different set of books and retorts, so the player and the AI have more starting wizards to choose from. A list of new wizards in `content/wizards.ron` (id, a variant name, books, retorts, the base wizard and the palette change), shown in the new-game wizard pick and used by the AI's wizard draw.

- **Portraits.** The portraits are the original's art and so local-only (`docs/local-assets.md`), and a recoloured portrait is derived from it. The variants are made from the player's own copy. The recoloured portraits are committed (decided 17 October 2026), like the Myrror river pictures, as a documented exception in `docs/local-assets.md` (add the entry when they are made). The original portraits stay local-only. Recolour the base portrait once by index remapping (the portraits are 256-colour pictures: remap the palette entries that hold the hair, skin or cloth to new colours; a per-variant table of `(index range → new colour)` in the data), and generate the result once and keep it as files in the repository (as the Myrror river pictures are, under `crates/f4x-client/assets/`), so the variants show without the local art. Because the originals are not committed, the generator needs the player's own copy to run; the committed files are the output. A variant's portrait is there without the local art; the base wizards' portraits are not.
- **Variants.** For each of the 14 wizards one or more variants with a new name and description. A variant changes the portrait colours and starts with other books and retorts than its base (a different realm mix, or a different retort), within the pick rules the custom wizard follows (`wizard.rs`: the same budget of picks, the same limits). Examples to write: a dark-haired Merlin who is a chaos and life wizard rather than life and nature; a red-robed Raven. Decide the number per wizard (one to three) and who writes the lists (a first set by hand, balanced with item 24's soak runs).
- **Choice.** The new-game screen's ready-made list grows (a second page or a scrolling list; the original shows 14 portraits in a grid); the opponents' draw uses all of them; a setting can limit the pool to the original 14.
- **Names and text.** Each variant gets its own name and a short description in the style of the originals, written for the project (not the original's text; the original's descriptions belong to the base wizards).
- **AI.** The AI's personality and objective draw (task 64) is unchanged; the books and retorts decide its spells as they do now.

Needs: the data file and its loader (`f4x-data`), the palette tables and the importer step that writes the recoloured portraits (`scripts/import_mom.py` or a new script, logged in `docs/scripts.md` with `--help`), the new-game screen's longer list, saves (the wizard is saved by its books and retorts, so a variant needs only its portrait number, to check), the console and the help, tests that every variant is a legal pick, and a balance pass.

**36. Multiplayer (to be discussed).** Nothing is decided; this item holds the questions so the scope is settled before any work (`design.md` section 13 asks the same). Items 18 (hot seat) and 19 (asynchronous play) are the small versions; this one is for the rest, mainly play over a network at the same time.

- **Scope.** Which kinds: hot seat (18), play by turn files (19), simultaneous turns over a network, or several of them. Whether wizards may be human and computer mixed, and how many humans (the game has up to five wizards).
- **Turn model.** The game is turn based with every wizard moving in the same turn. Options: all humans play at once and the turn ends when all have ended it (with a timer, or a vote to end early); or in sequence, one wizard at a time. Battles between two humans need both players present (a battle is played command by command, so a wizard who is away needs auto-resolve or an AI standing in).
- **Authority.** One side holds the game and checks every command (a server, or the host's game), and the others send commands and receive events. The core already works this way: a front end sends `Command` and gets `Event` back, and the game is deterministic from a seed.
- **What exists for it.** Recording and replay for local play (task 74, `replay.rs`): a command log with creation numbers in place of entities, a start snapshot, and replay from it. That is a base for turn files and for resynchronising a client, but not a network protocol.
- **What is missing.** A wire format and versioning for commands and events; per-player views of events (a wizard must not be told what its fog hides: `player_view.rs` limits what a player may read of the game, but there is no filter for the events a command returns); the hidden information in the random stream (a client must not run the generator ahead); battles in progress are not saved, so a dropped connection loses them; undo; handing control of a wizard to the AI when its player leaves; cheats and the console in a shared game; chat and diplomacy messages between humans (the wizard diplomacy of `diplomacy.rs` is for AI and human alike).
- **Open questions.** Which scope first; peer to peer or a central server; how to settle disconnects and timeouts; whether the AI may be replaced by a human mid-game; how saves, replays and the Hall of Fame treat multiplayer games.
