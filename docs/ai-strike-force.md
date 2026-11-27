# AI strike forces: gather first, attack separately

Plan, 27 November 2026. **Step 1 (the attack module, task 836) is built; step 2 (task 837) is not.**
It builds on the strategic AI's settings file (`f4x-def/content/ai_strategy.ron`, task 831, done).

## 1. Why

Before step 1 the AI mixed gathering and attacking: `strike.rs`, `raid.rs` and `perimeter.rs` each
attacked in their own way, `war.rs` only walked units, and the quick strikes skipped war targets and
left them "to the army". Since commit `0d12516a` (17 November 2026) nothing therefore attacked a war
target: nine swordsmen beside a target town held by one spearman fought no battle.

The goal: the dispatcher only **builds up a force** that can take a target, and a separate logic
**checks whether the force is enough and attacks**. The attack logic belongs to the AI only. The
gathering orders are what can later move into `f4x-core` beside the other unit orders
(`f4x-core/src/unit/handling`), so that there is one system of orders and not two.

## 2. Decisions taken

- A rally point is fine, but never a city: a city runs into the units-per-square limit.
- The rally point is the landing site for an attack across the sea, and else a square chosen along
  the path of the **unit that arrives last**, so that the army is not slowed down.
- All AI attacks go through the new attack logic.
- A Plane Shift crossing needs no city.
- Gates speed units up. That matters mostly for which unit arrives last, not for where the force
  musters.
- A strike on a lair or tower is planned and targeted: only the force sent against it attacks it.
- Independent cities belong to players with full diplomacy. A city is attacked only when the AI is
  planning or fighting a war against its owner.
- A preemptive strike on another player's units that come too close to a city makes sense, but only
  when the AI is open to declaring war on that player anyway (a bad relationship and the like).
- The campaign declares war on the owner of its target itself, but never against a partner of a pact
  or an alliance, nor in a peace period, and does not aim at such a city (decided with task 836).
- **One** AI setting says how many times as strong a force must be to attack: **1.2**.
- All the levers of the strategic AI belong in one settings file, those of the battle AI in another
  (tasks 831 and 832).
- The safety of the squares a force gathers on is refined later.

## 3. Limits of the game that shape the design

- All attackers of a battle stand on **one square** beside the target (`BattleStartError::BadSides`,
  `NotAdjacent`; a site is entered by one stack that could make the step).
- A battle takes `max_battle_side` units a side; a square holds `max_tile`.

## 4. Step 1: one attack module (built)

`attack.rs` decides and carries out every attack of the AI players at one threshold, `attack_force`.
What it does is in [`ai-design.md`](ai-design.md), "Attacks".

## 5. Step 2: rally point and one square beside the target

A new module, `assembly.rs`, used by the campaign (`war.rs`), the raids (`raid.rs`) and the walk
repair (`dispatch::resend`). The perimeter uses its square beside the target.

### 5.1 Travel time

How long a unit needs to a square is the shorter of its walk and its way through a gate:

- **Earth Gate:** walk to an own gate city, step to another own gate city on the same plane (one
  move), walk on.
- **Astral Gate:** walk to the own gate city, change to the other plane, walk on.

A unit whose way through a gate is the shorter one takes it. This decides which unit arrives last.
Core gets a read accessor for the travel time from a unit's square or from another square
(`World::path_turns` and `World::find_path` in `unit/pathing.rs` start where the units stand).

### 5.2 The square beside the target

One for each target: the last square of the way of the last-arriving unit before the target. It is
kept until it is blocked, then the nearest open square takes its place. Every unit sent against that
target goes there. (Today each group picks the nearest open square for itself, so a force can end up
split.)

### 5.3 The force

Picked by need as today (until the strength wanted and at least three units are reached, at most what
a battle holds; every free soldier in the win phase), but the soonest at the **target** first and not
the nearest to a rally city.

### 5.4 The rally point

1. Take the unit of the force that needs the longest to reach the square beside the target.
2. Walk its way from the start and take the first square that every member reaches no later than that
   unit does. (If its way begins with a gate, the walk starts at the gate's exit.)
3. A rally point is never a city, a lair, a square with an enemy, a square beside the target or a
   square without room for the force. If no square qualifies, the force goes straight to the square
   beside the target.

It is chosen once for a target and kept while it stays valid. It is saved in `Ai`
(`#[serde(default)]`), so a muster that is made anew does not move it.

### 5.5 March

When the force at the rally point, plus what already stands at or marches to the square beside the
target, is enough, the rally force moves there. A force at the target that is too weak (losses, a
stronger defence) gets more units this way. The attack itself is step 1's.

### 5.6 Special routes

- **Across the sea:** the rally point and the square beside the target are both the landing site
  (`naval::Plan::land`). When enough soldiers are available, war is declared (as `set_sail` does now,
  and a pact stops the landing) and they are tagged `Crossing::Sea` where they stand. `pickup.rs`
  already ferries a unit to its role's target, so muster members are brought to the landing site.
  No gathering at the port; `march_landed` goes away.
- **Raids:** `raid::gather` rallies on the path instead of in the city nearest the site.
- **Tower crossing:** the units walk to the tower and cross as today; the rally point is chosen on
  the far side once a unit is across.
- **Astral Gate:** the force musters on the far side like any other; the gate is only the way across
  (5.1).
- **Plane Shift:** the spell moves the whole stack of one square to the same coordinates on the other
  plane, from anywhere (`spell/special.rs`; it fails when an enemy holds the other square, the stack
  does not fit there or a unit cannot stand there). The force musters on one square that is no city,
  chosen so that the way of the last unit to it plus the walk from its other side to the target is
  the shortest. `wizardry::shift_target` and `shift_gather` drop their demand for a city.
- **Where the AI casts gates:** Earth Gate in the own city nearest the rally point first and then in
  far cities, as today; `war_plan::rally` is replaced. Astral Gate in the own city that gets the
  force to the other side soonest.

## 6. Open questions

None that blocks step 2. Left for later: whether the squares a force gathers on are safe
from other enemy units.

## 7. Tests and documents (step 2)

- Tests use the shipped content, units by number, and read the threshold from the content.
- New: the rally point lies on the way of the last-arriving unit and is no city; a gate changes who
  arrives last; all units for a target use one square; an overseas force is tagged for the landing
  site and attacks after landing; a force crosses by Plane Shift from a square that is no city; a
  weak force at a target gets more units.
- The `war.rs` and `war_plan.rs` tests that expect a rally city are rewritten.
- `docs/ai-design.md` (section 15) and `docs/ai-unit-registry.md` (musters) are updated with the
  code, and this plan's built parts move there.

## 8. Order of work and checks

1. Step 1 (task 836) is done; step 2 (rally point, task 837) is next.
2. After it: `cargo nextest run --workspace --exclude f4x-learn`, `cargo fmt --all`,
   `cargo clippy --workspace --tests --exclude f4x-learn`, and the speed guard
   (`cargo build --release -p f4x-tools`, `python3 scripts/perf_guard.py`).
3. Rally points cost path searches (up to one for each member and square of a path, once for a
   target); the speed guard shows whether that matters.
4. A pinned scripted play that moves is reviewed field by field before it is refreshed.
5. A soak through `scripts/cool_run.sh` would show cities taken and marches launched.
