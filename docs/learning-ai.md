# A self-learning AI

Design for AI players that learn the game by playing it, instead of following hand-written
rules (`crates/f4x-ai`, `battle/ai.rs`) or loading weights somebody else trained. Status
(13 October 2026): **phases 0 to 2 are built and tested (the battle AI); the strategic AI is
still design only** and waits for the review after phase 2 (section 13). The battle agent beats
the scripted battle AI but is not clearly better than random orders, so it is **not the default**
(section 7.8). Related tasks: 46 (battle AI), 64 to 67 (personalities, AI v1, spells, attack),
77 (soak runs), 169 to 171 (done: phases 0 to 2), 172 (follow-ups).

## 1. Goals and limits

Goals:

1. Two learned players that start knowing nothing about the game's strategy and get better
   through iterated self-play:
   - the **strategic AI** plays the overland game: research, spells, taxes, city production,
     settling, army movement, diplomacy, and the decision to attack;
   - the **battle AI** fights the tactical battles the strategic AI sends it into, to win
     with the least loss.
2. No presets that fix the play: no scripted opening and no pretrained network from outside.
   What the agents know comes from the sim's rules, the reward and, as a starting help and an
   input rather than a command, the existing scripted AI (section 9). Their weights keep
   adjusting through play; they are the project's own build product (section 11).
3. The game core stays deterministic and free of ML. The AI sees what a human player sees.

Limits, said plainly:

- The order of work is fixed only as far as the **battle AI** (phases 0 to 2). The strategic AI
  is decided after that (section 13).
- A full Master of Magic game is long (hundreds of turns, dozens of commands a turn, hidden
  information, a huge action space) and the reward is a win that comes hundreds of turns
  after the choices that earned it. Learning it from scratch is research-grade. The **battle
  AI** is a realistic target: short episodes, dense feedback. The **strategic AI** is a
  stretch goal: expect it to need the shaping, curriculum and league of sections 6 and 8, and
  expect it to find bad local habits first. Section 13 gives the order that gets something
  useful early.
- Nothing here has been measured. Sim speed decides how much is feasible; phase 0 measures it.
  The sizes given are starting guesses.

## 2. What the AIs control: the interface between them

The two agents are separate policies with separate training. They meet in one small contract,
the **battle brief**, which the strategic AI writes when it starts a fight (or when a fight is
forced on it):

| Field | Meaning |
|---|---|
| `mana_budget` | The most mana the battle AI may spend on the wizard's casting in this battle (0 up to the pool). Enforced by masking, not by trust. |
| `stakes` | How much the strategic AI values the units that fight against winning: from 0 (win at any cost) to 1 (preserve the army, flee early). It scales the loss term in the battle reward. |
| `may_flee` | Whether `Flee` is a legal action. |

Rules:

- The battle AI is trained on random briefs, so one network serves every case ("conditioned"
  policy). Its observation includes the brief.
- The strategic AI chooses who fights (units sent, target, whether the fight is played or
  auto-resolved) and writes the brief. It is rewarded through the game (section 6), not for
  the battle's own score, so it learns what a battle costs it.
- The battle AI never sees the overland game and the strategic AI never sees battle squares.
  After a fight the strategic AI observes only what a player does: the survivors, the loot,
  the city or site that changed hands.
- Auto-resolve exists in the core (`Command::AutoResolve`). The strategic AI may choose it; in
  training it is allowed but priced by its worse outcomes (the original's auto-resolve has no
  tactics).

A third, smaller concern, **diplomacy answers**, stays inside the strategic AI as extra action
families (section 5.3). The hand-written `rate_proposal` helper in the core is not used as a
teacher.

## 3. Architecture: where code lives

```
f4x-core         unchanged rules; gains the shared legal-action functions (section 4) and an
                 observation (section 5.1)
f4x-learn        NEW crate, all Rust: the environment (reset / observe / mask / step, many
                 games batched), the networks, PPO, the league and the evaluation
f4x-tools        gains `train` and `evaluate` subcommands
models-local/    NEW, gitignored: checkpoints (section 11)
f4x-ai           gains LearnedAi next to Ai: same entry point, loads a checkpoint
```

**As built (phases 0 to 2).** The plan above changed in three places:

- `f4x-learn` is in place (3,100 lines): `scenario.rs`, `build.rs`, `bank.rs` (scenarios),
  `env.rs`, `policy.rs`, `reward.rs`, `obs.rs`, `act.rs`, `agent.rs` (the environment and the
  decisions), `net.rs`, `train.rs` (Burn: the network, PPO, evaluation) and `brain.rs` (the
  learned player for the game). Burn is optional: the environment builds without it, and the
  networks need `--features train`.
- Training is run from **examples of f4x-learn** (section 7.9), not from `f4x-tools train` and
  `evaluate` subcommands. `f4x-tools` has only `soak --battle-ai CHECKPOINT` (feature
  `learned`).
- There is no `LearnedAi` in `f4x-ai` (that is the strategic AI of phase 6). The battle AI
  plugs into the core through the `BattleBrain` hook (`Game::set_battle_brain`), so the core
  stays free of ML.

**Rust, not Python.** The choice was "whichever is less resource-intensive". A Rust trainer
runs in the same process as the sim: no interpreter, no copying observations into numpy
buffers, no second runtime to keep in memory, and the game loads the very network code it was
trained with (no ONNX export, no `tract`). On one M4 with shared memory that is the cheaper
route. What it costs is our own work: PPO, GAE, masked sampling and the league are a few
thousand lines to write, and there are fewer ready tools for plotting and debugging.

Framework: **Burn** (0.22 on crates.io when this was written: autodiff, a pure-Rust `flex` CPU
backend, a `metal` backend, kernel fusion; the older `ndarray` backend is deprecated, and a
`rl` feature with reinforcement-learning utilities exists, not yet looked at); **Candle** is
the fallback. I have not checked the current state of either; a short spike in
phase 0 confirms that a 1 to 5 million parameter network trains at a useful speed on the M4
before anything else is built on it. For networks this small the CPU backend may beat the GPU
(no transfer cost), and the spike measures both.

**Running on a laptop: stay cool.** An overheated Mac slows everything to a crawl, so every
long run (training, evaluation, soak) goes through `scripts/cool_run.sh COMMAND...`: it runs
the command at background priority (`taskpolicy -b`, `nice -n 19`), reads the macOS thermal
state every 15 seconds (`ProcessInfo.thermalState`, no privileges; the same signal the imagegen
project uses, `scripts/thermal_state.swift`), **stops the whole command while the state is
`fair` or worse** (`COOL_PAUSE_AT`), and continues it only once the state is `nominal` again
plus a minute's rest. A state that cannot be read counts as hot. In addition the trainer uses
at most 4 of the M4's 10 cores (the thread count is a setting, default 4) and writes a
checkpoint at least every few minutes, so a pause or a stop loses little.

**Spike result (task 169, 12 Oct 2026, the M4 with 32 GB, run at background priority on 4
threads).** A network of 3.7 million parameters (four linear layers, input 2048), batch 256,
Adam: the `flex` CPU backend takes about 110 ms a step; the `metal` backend about 30 ms a step
once started, but it spends about 20 seconds in its first three steps compiling kernels. Both
learn (the loss falls). Building Burn with these features takes about 27 minutes the first
time (4 jobs, release), and a debug build is slower to run, so training always runs in release.
These are one short run each: the real network and batch will differ. The environment itself:
a generated battle builds in about 19 ms and a random-play battle takes about 88 ms (250
commands, 355 microseconds a command, most of it listing the legal casts), single thread.
Reproduce with `cargo run --release -p f4x-learn --example bench` and
`... --features spike --example spike -- flex|metal`.

Everything stays reproducible: the policy's sampling RNG is separate from the game's, and a
training run is fixed by `(seeds, config, code version)`.
## 4. Legal actions: one definition, shared with the console

The console (`docs/console.md`) already is the definition of what a player may do: every client
action has a **legal command** (`move`, `produce`, `cast`, `research`, `bmove`, `bshoot`...),
sent through `Game::apply`, and in legal mode its queries show only what the player can see.
The learner uses exactly that:

0. **What is done.** `Battle::legal_commands()` (every move, melee attack, shot and wall
   attack of the side to play, then `Done`, `EndTurn`, `Flee`) and `Game::legal_battle_casts()`
   (each castable spell with its aims and least and most mana) are in the core, tested against
   `Battle::apply` and `Game::battle_command` (the first exhaustively over every command that
   could be written), and the console's `blegal` query prints them. `Game::battle_command`
   checks a cast through the same `check_cast` the list uses. The overland enumerators (stack
   moves, spell targets, production items) wait for the strategic phases.
1. **Action families = the console's legal commands**, with the same arguments. Nothing is
   added for the learner, and nothing the console cannot say is allowed. A new legal command
   later becomes a new family by being added to a table, not by new code paths.
2. **Legality = the same checks the console runs.** Where a check already exists as a function
   (`can_build` behind `options`, `can_found_city`, `can_shift_plane`, `can_use_earth_gate`,
   `Battle::reachable`, `battle_spells`), the mask calls it. Where the console only tries the
   command and reports `apply`'s rejection (`move`, `cast` targets, battle targets), we add an
   **enumerator** in the core (`Game::legal_moves`, `legal_cast_targets`,
   `Battle::legal_commands` and so on) and make the console's queries use the same function,
   so `options`, and new queries such as `moves <units>` and `targets <spell>`, print what the
   mask contains. One source of truth, and the console gains the queries a human or an MCP
   client wants anyway.
3. **A test keeps them aligned**: over seeded random games, every enumerated action is
   accepted by `Console::execute` (access `Play`), and a sample of actions that are not
   enumerated is refused. It also checks that stepping with `Command`s and stepping with
   console lines give the same game.
4. **Training steps use `Command`s directly** (no text parsing, for speed); the console is the
   reference they are tested against, and the text interface remains for debugging a
   learned policy ("print what it would do").
5. **Observation = the console's legal-mode view**: the same visibility (`Sight`, `explored`)
   as the legal-mode queries, so the agent cannot know more than a player at the console, and
   exactly the information "exposed to players" is allowed.

Other core changes, small:

- The observation tensors of section 5.1 and 7.2 are built in `f4x-learn` (`obs.rs`) from the
  core's public accessors: the core stays free of any tensor layout.
- `mana_budget` as a battle setting checked in `Cast`.
- Starting games and battles from a `Snapshot` (exists in `save.rs`), for the curriculum.
- The policy's RNG is separate from the game's; episodes replay from the command log.

## 5. Strategic AI

### 5.1 Observation

Everything is what the player could see:

- **Map**: per plane, a multi-channel grid (terrain, river/road, ocean, owner of the square,
  fog state, city, outpost, tower, node, lair seen, own stack, enemy stack seen, corruption).
  Processed by a small convolutional net.
- **Entity sets**, each entity a feature vector plus a learned embedding of its content id:
  own cities (size, food, production, buildings, garrison), own stacks and units (stats with
  bonuses, level, enchantments, orders), visible enemy stacks and cities, known spells,
  spells being researched, and the other wizards (relation, treaty, contact).
- **Global vector**: gold, mana, research, the power split, casting skill, turn, number of
  wizards left, victory-relevant counters.
- **Content as numbers, not names.** Unit, building and spell ids are embedded from the
  vector of their rule stats (cost, attack, abilities, realm...) so a never-seen unit with
  familiar numbers is understood, and nothing needs hand-written "this unit is good". This is
  what "discover the game" means in practice: the agent learns from stats and results, not
  from advice.
- **Memory**: the game is partially observed, so a recurrent core (GRU over one token per
  turn) carries what was seen before.

### 5.2 Network

A shared encoder (conv map + transformer over the entity set, around 5 to 20 million
parameters to start), the recurrent core, a value head and the policy heads of section 5.3.
Transformer over entities rather than a flat vector, because the number of cities and units
changes during the game.

### 5.3 Actions: a turn is a sequence of small decisions

A turn is played as a sequence of decisions, ending when the policy picks `EndTurn` (with a
limit on decisions per turn so a stuck policy cannot hang a game). Each decision is factored,
with masks from `legal_actions`:

1. choose an **action family**,
2. choose the **actor** (a city, a stack, the wizard) by pointing at an entity token,
3. choose the **detail** (the target tile for movement, the item to build, the spell, a
   quantity).

| Family | Core command | Detail |
|---|---|---|
| Move stack | `GoTo` | destination tile (the core's pathing walks it, so the policy picks goals, not steps) |
| Attack / enter site | `BeginBattle`, `EnterSite`, `AutoResolve` | target stack or site, units sent, **battle brief** |
| City production | production commands | item to build |
| Settle / found | found-city commands | tile |
| Research | research command | spell to research |
| Cast overland | cast commands | spell and target |
| Money and power | tax, farmers, power split | discrete level |
| Diplomacy | `Diplomacy` commands | propose, answer, threaten, break |
| `EndTurn` | `EndTurn` | none |

Movement and attack picked as destinations keeps a game of hundreds of turns in a tractable
number of decisions per turn.

## 6. Strategic training

### 6.1 Algorithm

**PPO with GAE and action masks**, run with many parallel games; **V-trace** (IMPALA style)
once collection and learning run on different machines or processes. PPO is the robust
default and has the best track record in long, partially observed games. Considered and not
chosen first:

- AlphaZero/MuZero-style search: the sim is deterministic and snapshot-able, so search is
  possible, but the branching factor of a turn is enormous. Revisit for the battle AI (section 7.6).
- Evolution strategies: simple and parallel, but needs far more games for a network this size.

### 6.2 Reward

Sparse win/loss is not enough over this horizon, and a hand-built "good play" score is
exactly the preset knowledge we want to avoid. The compromise:

- **Terminal**: +1 win, -1 loss (or being destroyed), a small value for the rank at turn limit.
- **Dense, potential-based** (so it cannot be farmed and does not change the best policy): the
  change in a **relative empire score** each turn, computed from public game quantities: cities
  and population, the summed production value of units alive, mana and gold income, spells
  known, minus the same for rivals. Because it is a difference of potentials, shaping only
  speeds learning; the terminal reward decides.
- **Curiosity** for discovery: a small intrinsic bonus for new states (random-network
  distillation on the observation), decaying to zero, so the agent tries unfamiliar buildings,
  spells and units early instead of settling into one routine.
- Battle results reach the strategic AI only through the changes they cause (units lost, city
  taken), never as a separate reward.

### 6.3 Curriculum

Learning a full game first is the slow way. Stage the tasks (all start from `Snapshot`s or
small setups), each reward-checked and replaced by the next once solved:

1. Found a city and grow it (tiny map, 50 turns).
2. Build an economy: reach N cities and a positive income.
3. Defend: survive against a scripted raider.
4. Take a neutral city or clear a lair.
5. Two-wizard games on a small map, with a turn limit.
6. Full games: normal map, 2 to 4 wizards.

### 6.4 Self-play and the league

Training only against yourself drifts into one strategy. A **league** keeps:

- the current policy,
- frozen past checkpoints (sampled by recency and by win rate),
- a few "exploiter" policies trained only to beat the current main one,
- the scripted AI of `f4x-ai` as a **sparring partner and yardstick**. It is also the source of
  the advice input and the warm start of section 9, but it is not in the reward.

## 7. Battle AI

### 7.1 Why it comes first

Battles are short (tens of decisions), their reward is clear and nearly immediate, and the
rules are all in `battle/`. It is easy to generate unlimited varied training data, it is
useful at once (task 46 is open), and it is the part the strategic AI depends on.

### 7.2 Observation

- the battle grid as image channels (terrain, walls and gate, obstacles, each side's units,
  the active unit's reachable squares, ranged-attack coverage),
- the unit list as a set (stats with bonuses, hit points left, ammo, abilities, enchantments,
  which units have acted),
- the **brief** (`mana_budget`, `stakes`, `may_flee`), the mana already spent, the casting
  skill, the spell list and the turn.

### 7.3 Action

One battle command at a time (the core's `BattleCommand`): choose the unit (pointing at a unit
token), then the verb (`Move`, `Attack`, `Shoot`, `AttackWall`, `ShootWall`, `Cast`, `Done`,
`Flee`, `EndTurn`), then its target (a cell, an enemy unit, or a spell with its aim and its
mana amount). Masks come from `Battle::legal_commands`, and every `Cast` is clipped to the
remaining budget. Network size around 1 to 5 million parameters.

### 7.4 Reward

At the end of the battle, from the point of view of the side the agent plays. The agent cannot
always win, so a battle it is losing is not a lost cause: it should sell its units dearly. The
two outcomes mirror each other:

```
won:   reward =  1 - stakes * (own units lost / own units sent)
lost:  reward = -1 + (enemy units destroyed + own units saved by fleeing)
                     / (enemy units fielded + own units sent)
       - small * (mana spent / mana_budget)          (both cases)
```

- Units are **valued**: a unit is worth `1 + 0.01 * cost`, the cost the rules give it
  (production or gold, or the mana of a summoning), so losing or destroying a dear unit counts
  for more than a cheap one (`unit_value`). The formulas above use these values in place of
  counts. Units *standing below half their hit points* count in two small ways: in a loss an
  enemy left below half health gives a tenth of its kill value, and an own unit that survives
  below half health (by fleeing, or standing at the end of a win) counts nine tenths as
  survived (a tenth as lost). Before 16 October units were counted one each.
- A win is always better than a loss of the same battle: the loss term is at most 0, and a
  win with every unit lost and `stakes` 1 is 0 too, never below it.
- Losing with the most enemy units destroyed is therefore the aim when the fight is
  overwhelming; `stakes` only changes how the agent weighs its own losses when it can win.
- **Fleeing** (when `may_flee`) ends the battle as a loss, and every own unit that gets away
  counts the same as an enemy unit destroyed. So fleeing pays in a battle where the agent is
  unlikely to get any kills, and fighting on pays when it can destroy more enemy units than it
  would save by leaving: that trade is what the agent learns. Since task 250 (16 October) a
  fleeing unit is lost with one chance in two (a quarter for heroes), settled when the battle
  ends; the environment subtracts those from the units saved (`UnitsLostFleeing`). The runs of
  7.8 were trained before this, when every fleeing unit counted as saved.
- A small per-step cost stops stalling; a battle that runs to the 100-turn limit is a loss for
  both.
- Sampling `stakes` and `mana_budget` at random during training is what makes the policy obey
  the strategic AI later.

### 7.5 Scenarios: what varies, and how spells are handled

Battles differ in their armies, ground, wizards, mana and spells. The battle AI has to cope
with all of it, so the training data is a **scenario generator** and every scenario is a plain
`BattleScenario` value (written as RON, reproducible from one seed, replayable and loadable
for tests and for the evaluation suite). What a scenario holds:

| Part | What varies |
|---|---|
| **Armies** | Each side's units from the whole unit list: counts, mix (melee, ranged, fliers, walkers, summoned, fantastic), experience levels, heroes and their items, enchantments already on units, units that start damaged. Army size from 1 v 1 up to full stacks. |
| **Odds** | The strength ratio is sampled on purpose, not left to chance: even fights, **overwhelming** (the agent should win with little loss), **overwhelmed** (it should sell its units dearly or flee), and lopsided in kind (all fliers against no ranged). Without the second and third kind the reward terms of 7.4 never get exercised. |
| **Ground** | The overland terrain under the fight (the battlefield it makes), the plane, an open field, a city (walls, gate, towers, buildings, Wall of Fire or Darkness, a Spell Ward), a node, a lair or tower with guards. Which side attacks. |
| **Wizards** | Each side has a wizard or none (independent cities and monsters have none). If present: the **books and retorts** (these set the cost reductions and the realms of the spells), the **casting skill**, the **mana in the reserve**, the **distance to the fortress and plane** (the range multiplier of `combat.md` section 8, so the same mana buys less far away), and the spells known. |
| **Spells known** | A random subset (from 16 October: of the spells of one or two realms, 90% of wizards, or of three to five, and the arcane ones; at most 40, the number of spell tokens) of the spells the game can cast in battle, drawn **from `Game::battle_spells`**, the same list the console's `bspells` shows (bolts and damage, healing, summons, dispels, Counter Magic, the lasting spells on a unit, as `combat.md` section 8 lists), with the realm mix of the books. From none (a wizard who cannot cast) to the whole list. |
| **Mana** | The reserve and the **budget** (`mana_budget` of the brief): below the cost of any spell, only enough for one cheap spell, plenty. Budget 0 is a normal case: the agent must fight without the wizard. |
| **Battlefield enchantments** | Present at the start on either side, where the rules allow. |
| **Brief** | `stakes` and `may_flee` (section 2). |

**How the agent handles spells** (`combat.md` section 8 is the rule; this is what the network
sees and does):

- **Spells as tokens.** Each spell the wizard can cast is one token in the observation: its
  cost, whether extra mana adds strength (`takes_extra_mana`), its target kind (own unit,
  enemy unit, any unit, a square, none), its realm, and the rule stats that matter (damage
  dice, mana per point, what it dispels or summons), and whether it is `castable` now. Like
  units, spells are read from their stats rather than named, so a spell added to the game later
  is understood from its numbers.
- **One spell a turn, three limits.** The wizard casts at most one spell per turn. A cast costs
  casting skill (the skill left in the battle), and the mana reserve pays that times the range
  multiplier. The brief's `mana_budget` is a third, lower limit. The mask admits a cast only
  inside **all three** (`BattleCaster::affordable`, `battle_spell_most_mana`, the budget), so
  the agent cannot choose an illegal or over-budget spell.
- **The cast action** is three choices, as for other commands: the spell (a pointer into the
  spell tokens), its aim (a unit pointer or a square, from the target kind), and the mana put
  in (a few steps from the least to the most the limits allow; it only matters for spells with
  extra mana, such as bolts, dispels and Counter Magic). The wizard has no square on the field
  and no unit token; "skip casting" is simply not casting that turn.
- **Spells not in the game yet are not offered** and so not trained on. More arrive with task
  159 and its neighbours. Animate Dead (13 October, after the first runs) is in the game and
  the agent can aim it: its aim is a *dead* unit of the wizard's own side, so the observation
  gives the dead that could be raised (not the destroyed) a unit token after the live ones. A
  fallen unit is told apart by having no figures, no square and no moves (`obs.rs`); the token
  shapes did not change, so older checkpoints still load, but they have never seen such a token
  and have not learned to use it. A checkpoint is saved with a record of what it reads (`NAME.shape.ron`: the ability
  list and the token sizes, `obs::Signature`), and loading one whose record differs from the
  game now, or has none, is refused with the differences listed (16 October; before that a
  mismatch panicked, and the first runs' checkpoints have no record). Carrying a checkpoint over
  to a grown set of abilities or spells is not built: the network must be trained again.
- **What the agent knows of the enemy wizard** is what the battle view shows a player (the
  `battle` query lists each wizard's skill, mana and range cost, and the field enchantments are
  public); not their spell list. Task 169 checks this against the view.
- **Evaluation** keeps a fixed set of scenarios for each row of the table (a siege with a
  wizard, a fight with budget 0, an overwhelmed fight with a bolt-only wizard, no wizard at all,
  a far-away wizard) so a change that breaks one kind of fight is seen.

The generator is the "discovery" for the battle agent: it learns the rock-paper-scissors of
abilities (flying, first strike, immunities, ranged) and of spells from outcomes.

Algorithm: PPO with self-play (the same network plays both sides), with a league of past
selves, as in section 6.4. Two things to tune: a curriculum of army size (1 v 1, then small
groups, then full stacks and sieges) and early-game exploration of `Cast`, which is rarely
rewarded by chance.

### 7.6 A possible upgrade

Battles have limited information and a modest branching factor, so an **AlphaZero-lite**
(policy network guiding a tree search over `Snapshot`s, with the dice as chance nodes) may beat
plain PPO late. Plan it only after PPO has set a baseline.

### 7.7 As built (tasks 170 and 171)

What `f4x-learn` does today (the numbers are in section 7.8):

- **Decisions.** A battle is played as unit decisions and wizard decisions (`act.rs`). The unit
  that decides is the one the *scripted AI would move next*; the agent chooses what it does from
  1451 orders (a square, a target by token, a wall section, done), all masked by
  `Battle::legal_commands`. The wizard decides once a turn: nothing, flee, or a spell with its
  aim and its mana (least or most). The turn ends by itself when the scripted AI has nothing
  left to move. Choosing *which* unit goes, and casting later in the turn, are left for a later
  version (a pointer over the units).
- **Network** (`net.rs`, Burn, about a million parameters): a trunk reads the field picture
  (six channels), up to 32 unit tokens (the unit's numbers and a multi-hot of its abilities),
  up to 40 spell tokens and 17 globals into a context vector; heads give the unit's order
  (squares by a linear layer, targets by a pointer over the tokens), the wizard's choice, a
  spell's aim (none, a unit, a square) and mana, and a value. Fleeing has a head of its own:
  its logit rises with how much fleeing now would score above what the critic expects of
  fighting on (a learned gain), since that comparison is what the choice is about.
- **Advice.** The scripted AI's order for the unit is shown as a bonus to the logit of that
  order, scaled by a learned gain, in half the training battles (`--no-advice` for the control
  run). A short behaviour-cloning pass on the scripted AI's orders starts the network.
- **Training** (`train.rs`, `examples/train_battle.rs`): PPO with generalised advantage
  estimation, 16 battles at a time with one batched forward pass. Self-play (the same network
  on both sides, both learn) and play against the scripted AI. Random briefs (stakes, mana
  budget, whether it may flee) from task 171. The wizard's choice mixes in 10% of random
  allowed options so fleeing and casting stay tried; the mixture is the policy that is trained.
- **Scenarios** (`scenario.rs`, `bank.rs`): the bank holds 1200 generated scenarios whose odds
  were *measured* by letting the scripted AI play each four times (`make_bank`), since a
  strength estimate from the units' numbers did not predict who wins. A tenth is held out for
  evaluation.
- **Reward** (`reward.rs`): section 7.4, plus a mana term of at most 0.025 for the share of the
  budget spent (0.05 until 16 October), and 0.01 for each different spell
  the wizard cast in the battle (`SPELL_BONUS`, to push the agent to try spells and so to be
  more interesting to fight). Evaluations include the bonus too (the aim is an AI that casts),
  so their scores are not comparable with the baselines' (which never cast) or with the runs
  before it; the baselines in a log show the plain reward. The runs of 7.8 and the first run
  of 16 October were trained without it, and the run `run-20261016-v2c` with it but with
  evaluations that left it out, and with units counted one each.
- **In the game** (`brain.rs`): `LearnedBrain` plays a wizard's battles through the core's
  `BattleBrain` hook (`Game::set_battle_brain`; auto-resolve and the battle screen's AI side
  ask `Game::next_battle_command`). `f4x-tools soak --battle-ai CHECKPOINT` (built with
  `--features learned`) lets the AI wizards fight with a checkpoint; without one nothing changes.
  `Game::set_battle_mana_budget` limits a wizard's mana in the battle in progress, in the
  mask and in the casting rules (`BattleCaster::budget`).

### 7.8 Results of the first runs (12 October 2026)

All numbers are the agent as the attacker against the scripted AI on 300 battles of held-out
scenarios (100 of each odds class, with new dice), after 20 to 40 PPO iterations of 192 battles
(15 to 30 minutes on 4 threads of the M4, at background priority). The reward is the one of
7.4 with stakes 0.5; a win is 1 at best.

The two trained rows are the **best checkpoints** of their runs (iteration 20 and 25); the
final results after 40 iterations are below the table.

| Player | Won | Reward | Even fights won | Overwhelmed (reward) |
|---|---|---|---|---|
| The scripted AI as the attacker | 147 / 300 | -0.03 | 52 / 100 | 0 / 100 (-0.97) |
| Random orders (the unit's turn is still the scripted order of units) | 168 / 300 | 0.04 | 68 / 100 | 3 / 100 (-0.92) |
| Trained, advice and warm start (run 4, 20 iterations) | 162 / 300 | 0.11 | 68 / 100 | 2 / 100 (-0.79) |
| Trained, no advice and no warm start (control, 25 iterations) | 170 / 300 | 0.12 | 75 / 100 | 2 / 100 (-0.86) |

**After 40 iterations** (the last evaluation of each run, same 300 battles; the logs are in
`models-local/run4/log.txt` and `models-local/control/log.txt`): run 4 scored 0.045 (148 / 300
won; its best was 0.112 at iteration 20) and the control 0.079 (157 / 300; best 0.119 at
iteration 25). Neither run improved after about iteration 20 to 25, and the scores between
evaluations wander by 0.05 to 0.08 (the control's iteration 35 was 0.043 and iteration 30 was
0.119). So more of the same iterations does not help; the limit is the yardstick and the setup
(section 7.7's list, task 172), not the run length. The warm start alone plays badly (run 4: reward -0.35
before any PPO), the control starts near the random baseline (0.03 against 0.04); both are
clearly above it within five iterations.

What this says, without dressing it up:

- The learned agent scores clearly above the scripted AI (about +0.14 reward, +16 to +23 points in
  even fights; a difference of two to three standard errors) after about twenty minutes of
  training, and it plays through the core's hook in a whole game (`soak --battle-ai`).
- It is **not clearly better than random orders** in this setup: the scripted opponent walks
  at the nearest enemy and attacks, which already loses to a player that does something
  different. The yardstick is weak, so "beats the scripted AI" is a low bar. Section 10
  therefore asks for stronger yardsticks next (past checkpoints, a league, and Elo),
  not only more iterations.
- **The advice and the warm start did not help**: the control run, which starts from nothing and
  is never shown the scripted AI's order, reached the same level as fast or faster (reward 0.09
  after 5 iterations against 0.11 after 5 with the warm start, and equal later; the evaluations
  differ by less than their noise of about 0.07). The scripted AI still chooses *which unit
  moves next* in both, so the control is not wholly free of it. With a stronger teacher the
  answer may differ; with this one the cheaper reading is that there is little to learn from it.
- **Fleeing** was not found by plain PPO (the first run fled once in 1200 first turns, though
  fleeing at once is worth +0.36 in an overwhelmed fight). Two fixes helped a little: the 10%
  mix of random wizard options, and a head for fleeing whose logit follows what fleeing scores
  above the critic's expectation of fighting on (a learned gain, started high). Overwhelmed
  fights now average -0.8 instead of -0.94 and the agent flees in about 8% of its first turns,
  but it also flees now and then in fights it would win (overwhelming fights 91 of 100 won
  against 95). This is the next thing to work on.
- Evaluations jump by about 0.07 between checkpoints a few iterations apart, so a checkpoint
  is chosen by its score on the held-out set, not taken as the last one.

### 7.9 The harness and how to run it

Everything long goes through `scripts/cool_run.sh` (section 3); the thread limit is
`RAYON_NUM_THREADS=4`. Paths are relative to the repository, outputs go to `models-local/`
(gitignored).

| Step | Command | What it does |
|---|---|---|
| Speed | `cool_run.sh cargo run --release -p f4x-learn --example bench` | Battles built and played per second (section 3) |
| Framework | `... --features spike --example spike -- flex\|metal` | The Burn spike of task 169 |
| Generator | `... --example calibrate [N]` | How well the generator's idea of odds predicts who wins |
| Bank | `... --example make_bank models-local/bank.ron [count] [first_seed]` | 1200 scenarios with odds measured by the scripted AI, three threads |
| Unattended | `scripts/train_battle.sh --detach --name NAME [--iters N ...]` | The whole training run in the background: build, train through `cool_run.sh`, best checkpoint by held-out reward, `STATUS` and `summary.txt` in `models-local/NAME` (`--help` lists the options; default bank `bank-v2.ron`) |
| Training | `RAYON_NUM_THREADS=4 ... --features train --example train_battle OUT_DIR [--iters N] [--bc N] [--battles N] [--no-advice] [--no-briefs] [--seed N]` | Warm start, then PPO; writes `log.txt` and `net_NNNN.bpk` every 5 iterations; `BANK=` picks the bank |
| Probe | `... --features train --example probe CHECKPOINT [BANK]` | How often a checkpoint flees at its first decision, by odds |
| In a game | `cargo run --release -p f4x-tools --features learned -- soak --battle-ai CHECKPOINT ...` | AI wizards fight their battles with the checkpoint |

**Bank v2 (16 October).** `models-local/bank-v2.ron` was made on the current rules with the realm-limited wizards (1200 scenarios, 400 of each odds class, odds measured by the scripted AI); the old `bank.ron` is kept. No run has trained on it yet, and every old checkpoint is refused (their ability list and spell slots differ), so the next run starts from scratch with `BANK=models-local/bank-v2.ron`.

What exists on disk after the first runs: the bank (1.3 MB), six run directories (`run0` to
`run4` and `control`) with their logs and checkpoints, and `battle-v1.bpk`. A checkpoint is
a Burn record of about a million parameters, 4.8 MB. Runs 0 to 3 were earlier versions of the
code and their settings are not recorded in the logs, so they cannot be reproduced. Run 4 is the
configuration of section 7.7 with advice and warm start; the control is the same without them.

**How the first runs happened.** The timer for 2 am was cancelled when the user said the tokens
were refreshed, and the work was done by hand on 12 October: the legal-action functions, the
environment and the Burn spike (task 169, with the 27-minute first Burn build), then the agent,
PPO and the bank (170), then the brief, the mana budget and the hook (171); the undead rules
(task 29) were done in between at the user's request. All runs used `cool_run.sh` and four threads.
The tests: 10 in `f4x-learn` (9 without the `train` feature): the reward, scenarios replaying
the same battle and following their odds, the observation, a random agent playing by the masks
and never being refused, and a checkpoint playing through the core's hook; plus the core's test that every legal command is accepted by
`apply` and the console's `blegal` query.

**What changed in the game since the runs**, which the trained checkpoints have not seen: Animate Dead (above; the agent can aim it, no run has used it yet);
damage is now sorted by what dealt it (`BattleUnit::damage_kinds`, `fate`), so units drained to
death join the winner as undead and the scripted and learned players fight in a world where that
happens; units gain experience each turn and have Warlord, Crusade and Heroism levels (task 27),
so a unit's numbers in a long game differ from those in the generated scenarios (which fix the
experience). None of this is in the reward. Retraining on the current rules is a command, not
work, but the scenario bank should be remade (`make_bank`) because the odds were measured before.

## 8. Training both: how the stages combine

1. Train the battle AI alone on generated fights (section 7) until it beats the plain
   `battle/ai.rs` opponent and keeps improving only slowly.
2. Train the strategic AI with the **battle AI frozen** doing all tactical fights. Strategy
   sees a stable battle world, so the learning signal is not chasing a moving target.
3. Alternate: unfreeze the battle AI on the fights the strategic AI really creates (a replay
   buffer of real battle starts), retrain it briefly, freeze it, and continue the strategic
   league. Repeat. This keeps the battle distribution matched to what the strategic AI makes
   without the instability of training both at once.
4. **Speed-up**: a fight played out in full is expensive. Once the battle AI is steady, train a
   small **battle outcome model** from its fights (`units, terrain, brief` to expected
   losses and win chance) and let strategic training use it for most fights, with a share of
   real fights to correct drift. Auto-resolve in the core stays an option the strategic AI can
   pick.

## 9. Starting from the existing AI

The scripted AI (`crates/f4x-ai`, `battle/ai.rs`) has no weights; what it knows is in its
rules and constants. Three ways to carry that into the learned AI, so it **starts able to play,
then adjusts through play**, used together:

1. **Advice as input.** For each decision the scripted AI's own answer is given to the network
   as extra features: what it would build in this city, where it would send this stack, the
   city it is campaigning against, and in battle the command it would give this unit. The
   learned policy can follow, adjust or ignore it. The advice is read from the player-visible
   state only, so it adds no hidden knowledge. A flag says whether advice is present, and it is
   switched off at random in some episodes, so the agent stays able to play without it and
   does not just copy.
2. **A warm start.** A short behaviour-cloning pass trains the network to reproduce the
   scripted AI's choices before reinforcement learning starts, with a small penalty for
   straying from the scripted AI that decays to zero. Training then moves away from it
   wherever play pays more.
3. **A control run.** One run starts from nothing and gets no advice. It measures whether
   (1) and (2) help or only anchor the agent to the scripted AI's habits.

The scripted AI is also still the sparring partner and the yardstick of the league (6.4).
The battle opponent `battle/ai.rs` is weak (everything walks at the nearest enemy), so for
battles the advice is dropped more often and the warm start is short.

## 10. Evaluation and safety

- **Yardstick**: Elo and win rates against the scripted AI, and against past checkpoints.
  Fixed seeds, fixed setups, fixed league, run on a schedule while training.
- **Battle**: win rate and loss rate by army matchup against `battle/ai.rs`, plus handpicked
  scenario tests (a siege, a flier-versus-spearmen, a ranged-versus-melee).
- **Behaviour tests**: replays reviewed for stalling, suicidal moves, wasted mana, never-built
  buildings.
- **Reward hacking is expected.** An agent that finds a loop or a rules bug is a bug report:
  `f4x-tools` writes the command log of any episode that ends with an anomaly (score jump,
  illegal-looking state, a panic) and adds it to the `soak` findings (planned 24).
- **Determinism**: a recorded episode replays identically from its seed and checkpoint, so
  every finding can be reproduced.

## 11. Deliverables, weights and licensing

- Checkpoints are written to `models-local/` (gitignored). The network is small, so a trained
  checkpoint can ship with the game as the starting point; the game keeps adjusting its own
  copy (section 9 says how it starts). Whether a checkpoint is committed to the repository is
  decided when there is one worth shipping.
- **What the model is, and the licence.** There is no pretrained or third-party model. The network
  (about a million parameters: a trunk over the field, unit and spell tokens and several heads,
  section 7.7) starts from random weights and is trained by our own PPO on games generated by
  our own simulator, so the weights are the project's own product with no third-party terms.
  The framework is **Burn 0.22, licensed MIT or Apache-2.0** (checked in the crate's `Cargo.toml`;
  the licences of the rest of its dependency tree have not been audited, and should be before a
  release that bundles a trained network). Checkpoints are not committed.
- Training uses the rules in `content/`, which are committed and carry no art, so a network
  holds no protected material; if training ever uses `content-local/`, keep its networks local.
- Continuing to learn **during normal play** (adjusting weights after real games) is possible
  with the same code, off by default and saved per player; it needs the in-game save format to
  carry the network, which is decided with phase 6.
- The game selects an AI by setting: scripted, or learned with a named checkpoint. Difficulty
  levels come from checkpoints (an early one, a mid-training one, the best) and from sampling
  temperature, not from cheating bonuses.

## 12. Costs and risks

| Risk | Mitigation |
|---|---|
| Sim too slow for enough games | Phase 0 measures games per second; optimise the hot paths, shorter curriculum scenarios, the battle outcome model (8.4), more processes |
| Learning stalls on the long horizon | Potential-based shaping, curriculum, curiosity, shorter episodes first |
| Huge action space | Factored actions, masks from the core, goal-level moves (`GoTo`), pointer selection |
| Policy cycles in self-play | League with past checkpoints and exploiters |
| Brittle against new rules | Content embedded from stats, retraining is a command, regression suite |
| Exploits the sim's bugs | Treated as findings (section 10), then fixed in the rules |
| Burn (or Candle) too immature or slow on the M4 | The phase 0 spike decides; the fallback is the other framework; the sim and the env do not depend on it |
| The mask and the console disagree | The alignment test (section 4) |
| Compute | The local M4 is what there is. It should be enough for the battle AI; the strategic AI may only reach small maps and short games on it. Decided at the review after phase 2 |

## 13. Phases

The plan is to build the battle AI first and decide on the rest after seeing how it goes.

| Phase | Work | Done when |
|---|---|---|
| 0 (done) | Shared legal-action functions and the alignment test (section 4); `f4x-learn` env for battles; the Burn spike; measure battles per second | a random policy plays battles without a rules error, and numbers for speed and for the M4 exist |
| 1 (done, review open) | Battle generator; PPO for the battle AI with the scripted battle AI's advice and a warm start (section 9), the control run, the league | beats `battle/ai.rs` in the evaluation suite; advice versus control compared |
| 2 (done) | Battle brief (`mana_budget`, `stakes`); casting; wiring into the game as the battle AI of AI wizards (closes task 46) | spends no more than the budget, trades losses for the win as `stakes` changes, and plays in the real game |
| **Review** | Look at what phases 0 to 2 cost and gave, then decide whether to go on | a decision |
| 3 | (if go) Strategic env, factored actions on the console's families, reward shaping, curriculum stages 1 to 4 | agent founds and grows cities, builds an economy, takes a neutral city |
| 4 | Strategic self-play with the battle AI frozen: stages 5 and 6, league | wins more than half its games against the scripted AI on small maps |
| 5 | Alternating retraining (8.3), the battle outcome model (8.4), diplomacy actions | stable improvement over several league generations |
| 6 | `LearnedAi` in `f4x-ai`, setting in the new-game screen, difficulty by checkpoint | a human can play against it from the client |

Phases 3 to 6 are provisional.

## 14. Decisions

Taken (from the first review of this document):

1. **Trainer**: Rust (Burn, Candle as fallback), because it is the lighter on resources
   (section 3). Spike first.
2. **Compute**: the local M4.
3. **Weights**: the learned AI starts from the existing AI (as input and as a warm start) and
   keeps adjusting (section 9); a trained checkpoint may ship with the game (section 11).
4. **Rules**: training always uses the full ruleset. A trimmed one would teach the wrong game.
5. **What the agents may read**: everything exposed to a player, and nothing else (section 4,
   point 5).
6. **Order**: the battle AI first, then review.

Still open:

- Whether the battle AI should also be trained to play Auto-resolve's role (`Command::AutoResolve`
  currently has the plain battle AI play both sides): probably yes, after phase 2.
- How the strategic AI is judged "good enough" to be the default (phase 4 gives a number to
  decide on).
