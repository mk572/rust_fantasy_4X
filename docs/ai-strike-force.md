# AI strike forces: gather first, attack separately

Plan, 27 November 2026. **Nothing here is built.** *Needs:* task 831 (the strategic AI's settings file,
`f4x-def/content/ai_strategy.ron`), which is done first.

## 1. Why

The AI mixes gathering and attacking. `strike.rs`, `raid.rs` and `perimeter.rs` each attack in their
own way, `war.rs` only walks units, and quick strikes skip war targets and leave them "to the army".
Since commit `0d12516a` (17 November 2026) nothing therefore attacks a war target: nine swordsmen
beside a target town held by one spearman fought no battle, and took it at once when the town was not
marked as the target (a throwaway test of one turn; the test is not in the tree).

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
- **One** AI setting says how many times as strong a force must be to attack: **1.2**.
- All the levers of the strategic AI belong in one settings file, those of the battle AI in another
  (tasks 831 and 832).
- The safety of the squares a force gathers on is refined later.

## 3. Limits of the game that shape the design

- All attackers of a battle stand on **one square** beside the target (`BattleStartError::BadSides`,
  `NotAdjacent`; a site is entered by one stack that could make the step).
- A battle takes `max_battle_side` units a side; a square holds `max_tile`.

## 4. Step 1: one attack module

`attack.rs` replaces `strike.rs`. For each own stack it looks at the adjacent squares and attacks
when the stack is at least `attack_force` (1.2) times as strong as the defence.

| Target | Who may attack | The defence |
|---|---|---|
| Lair or tower | the raid force sent against **that** site | `site::expected_vs`; spell power counts for the attackers; `site::lost_before` is respected |
| City of a player not at war (wizard or independent) | only the campaign force sent against it, which declares war first | the visible defenders, with the city factor |
| Enemy units or city at war, roaming monsters | free, campaign muster, march, perimeter, reinforce, garrison | the visible defenders, with the city factor for a city |
| Units of a player not at war that stand in the zone of an own city | perimeter units, only when the AI is open to war on that player; it declares war first | the visible defenders |

- `attack_force: 1.2` is a new entry of `ai_strategy.ron`. It replaces `attack_ratio` (1.4),
  `committed_ratio` (1.1), the campaign edge by phase (2.0, 1.5, 1.3 to 1.05), `sortie_ratio` (2.5),
  `site::edge` (1.4) and `COMMITTED_LANDING` (1.2). There is no threshold by place, role or phase of
  the game any more.
- The same number says how large a force the dispatcher gathers (`war_plan` and the force of section
  5.3) and what `economy` builds towards, so a gathered force is one that attacks.
- Expected effect: the AI attacks earlier and with smaller armies than now, most of all early in the
  game (2.0 becomes 1.2).
- Other numbers keep their own entries, because they answer other questions: how much stronger a
  party must be to retry a lost site, when a city counts as threatened, how much mana a battle gets,
  the city factor of a defence. The monsters' own threshold (`monster.rs`, 1.0) is not an AI
  player's and keeps its entry.
- War targets are no longer skipped. This fixes the bug.
- Changes from today: a raid party no longer assaults another lair or tower it happens to pass, and
  raids no longer target cities at all (`raid::run`, `raid::gather`, `site::sites_near`;
  `site::neutral_city` goes). Independent cities are the war plan's targets, as wizards' cities are.
- War is declared by the AI's own decision, never by the game: `naval::declare_war_on_city` covers
  only wizards today and gets independent players too. A pact or an alliance stops the attack.
- **Open to war** is one test, `diplo_b_stance::open_to_war`, made of the conditions under which
  `declare` declares today: the round is at least `from_turn`, there is no treaty and no peace period,
  the player's enemy value (less the damping for wars already on) reaches `declare`, and the army can
  spare another front. The perimeter takes units of a player not at war as a threat to attack only
  then. Today it attacks any such units but those of a partner.
- Kept as today: the best nine fight (`view::party`); a garrison lends soldiers only when its city is
  safe without them, and they walk home; an empty city is walked into; raiders with a tired unit wait.
- The work after an attack moves here from `raid.rs`: remembering a lost assault (`site_losses`) and
  `restart_plan` after a cleared site.
- `dispatch::run` calls the module twice: first, while the units have their moves, and after the last
  job, for units that arrived with moves left.
- `raid.rs` and `perimeter.rs` lose their attack calls. A party that stands beside a target it is
  enough for stays there. `monster.rs` does not change.

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

None that blocks step 1. Left for later: whether the squares a force gathers on are safe
from other enemy units.

## 7. Tests and documents

- Tests use the shipped content, units by number, and read the threshold from the content.
- `strike.rs` tests move to `attack.rs`. The test that a war target is not struck becomes two: a
  march beside its target attacks it, and a force that is too weak waits.
- Raid and perimeter tests that expect an attack call the attack step.
- New: the rally point lies on the way of the last-arriving unit and is no city; a gate changes who
  arrives last; all units for a target use one square; an overseas force is tagged for the landing
  site and attacks after landing; a force crosses by Plane Shift from a square that is no city; a
  weak force at a target gets more units; a raid party passes a lair it was not sent against; no raid
  or passing stack attacks the city of a player not at war; the campaign declares war before it does;
  units near a city are struck, after a declaration, only when the AI is open to war on their owner.
- The `war.rs` and `war_plan.rs` tests that expect a rally city are rewritten.
- `docs/ai-design.md` (section 15 and "Quick strikes"), `docs/ai-unit-registry.md` (musters) and
  `docs/ai-diplomacy-b.md` (open to war) are updated with the code, and this plan's built parts move
  there.

## 8. Order of work and checks

1. Task 831 (the settings file), then step 1 (attack module), then step 2 (rally point). Each can be
   reviewed and committed by itself.
2. After each: `cargo nextest run --workspace --exclude f4x-learn`, `cargo fmt --all`,
   `cargo clippy --workspace --tests --exclude f4x-learn`, and the speed guard
   (`cargo build --release -p f4x-tools`, `python3 scripts/perf_guard.py`).
3. Known before this work: the speed guard is over its limit with five wizards (146 ms, limit
   140).
4. Rally points cost path searches (up to one for each member and square of a path, once for a
   target); the speed guard shows whether that matters.
5. A pinned scripted play that moves is reviewed field by field before it is refreshed.
6. A soak through `scripts/cool_run.sh` would show cities taken and marches launched.
