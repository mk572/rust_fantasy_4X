# AI strike forces: gather first, attack separately

Plan, 27 November 2026. **Both steps are built** (the attack module, task 836; gathering, task 837).
It built on the strategic AI's settings file (`f4x-def/content/ai_strategy.ron`, task 831, done).

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

## 4. What was built

- **Step 1, one attack module** (`attack.rs`, task 836): [`ai-design.md`](ai-design.md), "Attacks".
- **Step 2, gathering** (`assembly.rs`, task 837): the travel time with gates, the one square beside
  each target, the force picked by who is soonest, the rally point and the march, and the special
  routes: [`ai-design.md`](ai-design.md), "Gathering a force", and the musters of
  [`ai-unit-registry.md`](ai-unit-registry.md).

## 5. Left for later

- Whether the squares a force gathers on are safe from other enemy units.
- Moving the gathering orders into `f4x-core` beside the other unit orders (section 1).
