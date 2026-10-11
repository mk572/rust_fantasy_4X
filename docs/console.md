# The console

Code: `crates/f4x-console/`. Front ends: the client (F2), `f4x-tools console` (headless).
Planned mod item 15 (`PLANNED.md`); the MCP server (item 17), an AI player and a remote client are
meant to use the same hooks.

The game can be played entirely from text. Every action the client offers has a console
command, and the commands are split by what they let you do:

| Kind | Meaning | When it runs |
|---|---|---|
| **query** | Reads the game (and the console's own settings). | always |
| **legal** | Anything a player may do in normal play. Sent through `Game::apply` as the same `Command`s the client uses, with ownership checked. | always (unless the console is read-only) |
| **override** | Cheating: adding units, removing players, setting gold. | only in **override mode** |

`help` lists every command with its kind; `help <command>` describes one.

## 1. Modes and access

- **Mode** is the toggle, three values: `mode readonly` (queries only: nothing typed can change
  the game, a guard against slips), `mode legal` (the default: queries and legal commands) and
  `mode override` (also the cheats). Commands the mode does not allow are refused with a message
  that says which `mode` to switch to. Going down a mode is always allowed; going up is limited
  by the console's access (below). A read-only mode console cannot save, load or start a new
  game either.
- In override mode a legal command may also be used on things that are not yours (another
  wizard's units or cities, or a battle side that is not to play); that use is an override too.
- **Every override is written to the game's override log** (`Game::override_log`, saved with
  the game): switching override mode on, each override command that worked, and each legal
  command that acted for someone else. `history` shows it, `status` says how many there are,
  so a saved game that was tampered with can be told apart. Hardcore mode (`PLANNED.md` item 8)
  should refuse a console whose access is below `Full`.
- **Access** is what a front end may ever do, set when the console is made, and not changeable
  from inside it:

| Access | Allows | For |
|---|---|---|
| `ReadOnly` | queries; starts and stays in read-only mode | a spectator, a read-only remote |
| `Play` | queries and legal commands; `mode override` is refused | an AI player, a networked player, hardcore games |
| `Full` | everything, once the mode is `override` | the local player, developers |

In legal mode the queries only show what the console's player can see: the player's own
units and cities, and others' units and cities on visible squares. Override mode sees
everything.

## 2. Using it

**In the client:** F2 opens the console as a screen (F2 or Esc closes it). The prompt shows the
mode (`[read-only]>`, `[legal]>` or red `[OVERRIDE]>`). Up and Down walk through the history and Tab completes a
command name. A battle that starts from a console line keeps the console open, so a fight can
be played from text (`battle`, `bmove`, `battack`, ...).

**Headless:** `cargo run -p f4x-tools -- console [--seed N] [--load FILE] [--content DIR]
[--access readonly|play|full] [--script FILE]` reads lines from standard input (or the script;
blank lines and `#` comments are skipped) and prints each reply. Without `--load` it starts a
new game with the same setup the client's new-game screen makes. Because it needs only the core
crate and content, it also works as a test rig:

```
printf 'status\nproduce 1 barracks\nend\nunits\n' | cargo run -q -p f4x-tools -- console --seed 3
```

## 3. Naming things

Units and cities are named by their **creation number** (`Serial`: stable, never reused, and
the same after saving, loading and undo). `units` and `cities` show them.

| Word | Means |
|---|---|
| `12` or `12,13,15` | those units |
| `@10,5` or `@10,5,m` | every unit on that square |
| `10,5` / `10,5,m` | a square: x, y and the plane (`a` Arcanus, the default, or `m` Myrror) |
| `unit:12` `city:3` `tile:10,5,m` `player:0` | a spell target |
| `spearmen`, `"Fire Bolt"`, `fire_bolt` | a unit, building or spell by id, by name (case and punctuation ignored) or by a unique id prefix |
| `trade`, `housing` | the two production items that are not buildings or units |

A line is split on spaces; double quotes keep a name together. Battle squares are plain `x,y`.

## 4. Every UI action, as text

### Queries

| Command | Shows |
|---|---|
| `help [cmd]`, `commands` | commands (the second is tab separated, for programs) |
| `status` | turn, treasury, mana, power split, research and casting, game over |
| `players` | wizards and whether they are banished or defeated |
| `units [mine\|all\|x,y]`, `unit N` | units, one unit's statistics and abilities |
| `cities [all]`, `city N`, `options N` | cities, one city in full, what it can build |
| `tile x,y`, `map [x,y] [radius]` | a square, a text map (`#` unexplored, `~` water, `.` land, `^` hills, `C` city, `u` unit, `*` yours) |
| `spells` | known and researchable spells |
| `diplomacy [wizard]` | the wizards met: relation word and number, treaty, what may be proposed, proposals waiting |
| `battle` | the battle being fought: units, squares, figures, moves |
| `blegal [unit]` | what your side may do in the battle now: each unit's moves, attacks, shots and wall attacks, and the casts with their aims and mana (the list the learning environment uses) |
| `score [player]` | the score breakdown (`game_end.md`) |
| `mode`, `history` | the mode; the override log |

### Legal commands

| Client action | Command |
|---|---|
| End turn | `end` |
| Undo | `undo` |
| Move a stack a step; attack a stack or city; assault a site | `move <units> <pos>` (`attack` is the same). A hostile square starts the battle and a guarded site is assaulted, as in the client. |
| Fight a battle without the tactical map (the battle screen's Auto) | `auto <units> <pos>` |
| Standing order to walk somewhere; cancel it | `goto <units> <pos>`, `stop <units>` |
| Settler founds an outpost | `outpost <unit> [name]` |
| Engineers build a road; purify; meld a node; change plane; teleport by Earth Gate | `road <units> [pos ...]`, `purify`, `meld`, `shift`, `gate <units> <city pos>` |
| Assault a lair, tower or node | `enter <units> <pos>` |
| City production, buying, farmers, selling | `produce <city> <item>`, `buy`, `farmers <city> <n>`, `sell <city> <building>` |
| Tax rate, power split, Grand Vizier | `tax <rate>`, `power <mana> <research> <skill>`, `vizier <on\|off>` |
| Choose research | `research <spell>` |
| Cast; aim a spell paid over turns; give up a cast | `cast <spell> [target]`, `target <target>`, `cancelcast` |
| Alchemy | `alchemy <amount> <gold\|mana>` (converts that much gold into mana, or mana into gold) |
| Talk to a wizard you have met: propose, answer, break a treaty, declare war, give tribute | `propose <wizard> <pact\|alliance\|peace\|war <third>\|break <third>\|swap <give> <get>>`, `answer <wizard> <yes\|no>`, `breaktreaty <wizard>`, `declarewar <wizard>`, `givegold <wizard> <amount>`, `givespell <wizard> <spell>` (the query `diplomacy [wizard]` shows relation, treaty and what is waiting) |
| Battle screen: move, melee, shoot, done, end turn, flee | `bmove <unit> <x,y>`, `battack <unit> <target>`, `bshoot <unit> <target>`, `bdone <unit>`, `bend`, `bflee` |
| Save, load, new game | `save [name]`, `load [name]` (`saves/<name>.ron`), `new [seed] [wizard=<preset>] [name=] [books=life:3,nature:2] [retorts=a,b] [race=<id>] [players=2..5] [land=small|medium|large|%] [difficulty=intro..impossible] [magic=weak|normal|powerful]`. `load` and `new` replace the whole game, so they need full access. |
| Quit | `quit` (the front end decides what that means) |

Pure view actions (camera, zoom, screens, selection, help pages) have no command: the queries
give the same information.

### Override commands

| Command | Does |
|---|---|
| `give <gold\|mana\|fame> <n> [player]` | add (or remove, with a negative number) |
| `spawn <unit> <pos> [player]`, `kill <units>`, `restore <units>`, `teleport <units> <pos>` | make, remove, heal and refill, or move units |
| `undead <units>` | make units undead (task 29) |
| `fortress <pos> [name] [race=] [player=]` | place a wizard's fortress and first city. A game-start action, not normal play: the game refuses it once the wizard has one |
| `newcity <pos> <name> [player] [race=] [pop=]`, `build <city> <building>` | found a city, add a building without cost |
| `terrain <pos> <terrain-id>` | change a square |
| `learn <spell> [player]`, `reveal [player]` | teach a spell, explore the map |
| `eliminate <player>`, `win <player>` | defeat a wizard on the spot (the usual aftermath and game-end checks follow), or end the game with this wizard winning by the Spell of Mastery |
| `as <player>` | play as another wizard from now on |

Not done yet: setting a feature or lair, clearing a lair, triggering events (the random events
are task 68), setting the turn number.

## 5. Hooks for other front ends

Everything a front end needs is public in `f4x_console`:

```rust
let mut console = Console::new(player, Access::Play);   // one per player/session
let reply = console.execute(&mut game, "cast fire_bolt unit:7");
// reply.ok, reply.lines (text), reply.events (core Events), reply.error (typed)
let all = f4x_console::commands();                      // the table: name, kind, usage, help
let hits = console.complete("spa");                     // names this console may run
```

- **One entry point.** `Console::execute(&mut Game, &str) -> Reply` parses, checks the kind
  against the access and mode, runs, and returns text lines, the core `Event`s and a typed
  `ConsoleError` (`Unknown`, `Usage`, `Parse`, `NoSuch`, `NotAllowed`, `Refused`, `Failed`).
- **Introspection.** `commands()` is the same table the console runs, with each command's
  kind, usage and help, so a front end can build its tool list or completion from it (an MCP
  server maps each command to a tool, the `commands` query gives the same as text).
- **Access, not trust.** A front end is given the access it may use and nothing can raise it
  from inside; an AI player gets `Play`, a remote spectator `ReadOnly`.
- **One path to the rules.** Legal commands call `Game::apply`; override mutators live in
  `f4x-core/src/cheat.rs`, are only called by the override commands and are logged.
- **Same state.** The console holds no game state of its own (only the player it acts as, the
  mode and the access); `load` and `new` replace the `Game` it was given, so a front end can
  keep its own copy in step by reading `Game` after each line.

To add a command: write a handler `fn(&mut Ctx, &Args) -> Result<(), ConsoleError>` in the
fitting module (`query.rs`, `legal.rs`, `cheats.rs`, `meta.rs`), add it to that module's
`TABLE` with its kind, usage and help, and (for a new `Command` of the game) give it a legal
command so every client action keeps a text equivalent.

## 6. Notes and open work

- `new` repeats the client's new-game setup (`setup.rs`); the client should call it instead
  of keeping its own copy, and the wizard creation screen's remaining choices (spell picks,
  banner) have no text equivalent yet.
- Messages for events are one line each (`describe.rs`); events without a nicer line print in
  their debug form.
- The AI crate (`f4x-ai`) still plays through `Command`s, not the console; the console is for
  AI *clients* that want text, such as an MCP server.
