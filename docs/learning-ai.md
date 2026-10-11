# A self-learning AI

Design for AI players that learn the game by playing it, instead of following hand-written
rules (`crates/f4x-ai`, `battle/ai.rs`) or loading weights somebody else trained. Status:
**design only, nothing is built.** Related tasks: 46 (battle AI), 64 to 67 (personalities,
AI v1, spells, attack), 77 (soak runs).

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

**Rust, not Python.** The choice was "whichever is less resource-intensive". A Rust trainer
runs in the same process as the sim: no interpreter, no copying observations into numpy
buffers, no second runtime to keep in memory, and the game loads the very network code it was
trained with (no ONNX export, no `tract`). On one M4 with shared memory that is the cheaper
route. What it costs is our own work: PPO, GAE, masked sampling and the league are a few
thousand lines to write, and there are fewer ready tools for plotting and debugging.

Framework: **Burn** (autodiff, an `ndarray` CPU backend, a `wgpu` backend that runs on Metal);
**Candle** is the fallback. I have not checked the current state of either; a short spike in
phase 0 confirms that a 1 to 5 million parameter network trains at a useful speed on the M4
before anything else is built on it. For networks this small the CPU backend may beat the GPU
(no transfer cost), and the spike measures both.

Everything stays reproducible: the policy's sampling RNG is separate from the game's, and a
training run is fixed by `(seeds, config, code version)`.
## 4. Legal actions: one definition, shared with the console

The console (`docs/console.md`) already is the definition of what a player may do: every client
action has a **legal command** (`move`, `produce`, `cast`, `research`, `bmove`, `bshoot`...),
sent through `Game::apply`, and in legal mode its queries show only what the player can see.
The learner uses exactly that:

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

- `Game::observe(player)` and `Battle::observe(side)` build the tensors from section 5.1 and
  7.2.
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

At the end of the battle, from the point of view of the side the agent plays:

```
reward = win
       - stakes * (value of own units lost / value of own units sent)
       - small * (mana spent / mana_budget)
```

- "Value" of a unit is its build cost from the rules (the same number the game uses), not a
  hand-tuned rating.
- A small per-step cost stops stalling; a battle that runs to the 100-turn limit is a loss for
  both.
- Sampling `stakes` and `mana_budget` at random during training is what makes the policy obey
  the strategic AI later.

### 7.5 Training data

A **battle generator** builds random fights from the rules: random armies from the unit list
(both sides, different budgets and mixes, heroes, items), terrain, with and without a city,
walls, towers, node and lair guards, and spells in the casting wizard's book. This stands in
for the long tail of situations the strategic AI will create, and is the "discovery" for the
battle agent: it learns the rock-paper-scissors of abilities (flying, first strike, immunities,
ranged) from outcomes.

Algorithm: PPO with self-play (the same network plays both sides), with a league of past
selves, as in section 6.4. Two things to tune: a curriculum of army size (1 v 1, then small
groups, then full stacks and sieges) and early-game exploration of `Cast`, which is rarely
rewarded by chance.

### 7.6 A possible upgrade

Battles have limited information and a modest branching factor, so an **AlphaZero-lite**
(policy network guiding a tree search over `Snapshot`s, with the dice as chance nodes) may beat
plain PPO late. Plan it only after PPO has set a baseline.

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
  illegal-looking state, a panic) and adds it to the `soak` findings (task 77).
- **Determinism**: a recorded episode replays identically from its seed and checkpoint, so
  every finding can be reproduced.

## 11. Deliverables, weights and licensing

- Checkpoints are written to `models-local/` (gitignored). The network is small, so a trained
  checkpoint can ship with the game as the starting point; the game keeps adjusting its own
  copy (section 9 says how it starts). Whether a checkpoint is committed to the repository is
  decided when there is one worth shipping.
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
| 0 | Shared legal-action functions and the alignment test (section 4); `f4x-learn` env for battles; the Burn spike; measure battles per second | a random policy plays battles without a rules error, and numbers for speed and for the M4 exist |
| 1 | Battle generator; PPO for the battle AI with the scripted battle AI's advice and a warm start (section 9), the control run, the league | beats `battle/ai.rs` in the evaluation suite; advice versus control compared |
| 2 | Battle brief (`mana_budget`, `stakes`); casting; wiring into the game as the battle AI of AI wizards (closes task 46) | spends no more than the budget, trades losses for the win as `stakes` changes, and plays in the real game |
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
