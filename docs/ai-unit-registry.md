# The AI unit registry

How the scripted AI (`crates/f4x-ai`) keeps track of what each of its units is for. Part of the AI
design ([`ai-design.md`](ai-design.md) section 13). Code: the records are
`crates/f4x-core/src/unit/task.rs`, the rules that fill them `crates/f4x-ai/src/roles.rs`; the
routines that take and give roles are named below.

## 1. The rule

Every unit the wizard owns has **exactly one role, written in the registry**, at any time, and the
AI never loses track of one. Nothing is derived: a soldier in a city is registered as `Garrison`
with that city, a unit on its way as `MoveTo` with the square. A unit with nothing to do is registered as
`AwaitingOrders`, the lowest priority but a role all the same. A routine that gives a role back puts the unit on `AwaitingOrders` at
once (it is never removed); the next audit gives it the base role its place says.

The role of a unit lives in one place, the registry `Ai::roles` (`roles::Registry`). No routine
keeps a list of its own units (`raiders`, `scouts`, `marching`, ... no longer exist). A routine asks
the registry who holds its role, and takes or gives roles through it.

## 2. What the registry holds

A map from a unit's `Serial` to one `Assignment`, ordered by serial. Its home is the player's army
ledger (`ecs-structure.md` 3.12), with the squares of the gathering forces (`Tasks`): the AI reads
them from there as its turn begins, works on its own copy, and writes it back with
`Command::SetTasks` near the end of the turn, so the save and the replay have them. A save from
before (task 830) has them in the AI's plans; they go into the ledger on the AI's first turn.

| Field | Meaning |
|---|---|
| `role` | what the unit is for (section 3) |
| `since` | the turn it took the role |
| `to` | the square the role sends it to, if any (a raid site, a scout target, a settler's site, the sea square a pickup ship lies at, the city a soldier is sent to) |
| `party` | the id of the group it acts with, if any (a raid party, a pickup ship and the units it came for) |
| `with` | the one unit it is tied to (the ship that comes for a waiting unit; an escort's charge) |
| `last_pos`, `stuck` | where it stood at the last look and for how many looks in a row it has not moved, for roles that give up a target that cannot be reached |

### Groups

A group is the set of units that share a `party` id (`new_party`, `group`, `members_of`,
`party_of`). `with` ties one unit to another. Together they cover:

- a **settler by sea**: no role of its own (task 647): the settler holds `Settle` with a site of the
  ranked list (`city_site.rs`) as target and the tag `Crossing::Sea`, and a ship fetches it like any other waiting unit, settlers first;
- a **pickup** (task 647): the ship holds `TransportSpace` (or `TransportReady` when the units fill it) with
  `to` = the sea square, then `Transporting` with the drop square beside the destination; the units it
  came for keep their own roles, tagged `Crossing::Sea`, with `with` = the ship and `boarding` = the
  shore tile where they board (`pickup::voyages`);
- an **escort**: the escort holds `Escort` with `with` = the unit it guards (`escort_of`); when the
  charge no longer holds `Settle`, `Road` or `Meld` (it founded its city, finished, or died)
  `escort::run` releases the escort.

The voyage data that is not about units (the landing plan, `naval_bad`) stays in the plan fields of `Ai`; only who is on it is in the registry.

## 3. The roles

Listed in priority order (the lower number wins a claim, section 4). The roles with a task:

| Role | For | Held by | Taken and given in |
|---|---|---|---|
| `Settle` | a settler walking to a site to found a city (`to` = the site) | settlers | `expand.rs` |
| `TransportReady` | a ship whose planned passengers (tied to it) fill it, with its target, until it sails (`to` = the sea square of a landing, the sea square of a rescue) | ships | `naval.rs`, `pickup.rs` |
| `Transporting` | a ship that has sailed with its load, on its way to its target (a landing to its sea square, rescued soldiers to a port) | ships | `naval.rs`, `pickup.rs` |
| `SeaParty` | the fighters a sea scout carries to put ashore | soldiers | `scout.rs` |
| `PlanarScout` | the scout walking to an open tower to change plane (`to` = the tower) | one unit | `scout.rs` |
| `TransportSpace` | a ship with a target whose planned passengers (tied to it) are fewer than it holds: on its way to the port to load a landing (`to` = the port), holding part of a load, or on its way to the sea square where stranded soldiers wait (`to` = the square) | ships | `naval.rs`, `pickup.rs` |
| `Perimeter` | a soldier dealing with the enemy in the zone of a city (`to` = the enemy's square) | soldiers | `perimeter.rs` |
| `Reinforce` | a soldier on its way to an under-defended own city, or kept in a threatened one (`to` = the city; released on arrival unless the city is threatened) | soldiers | `reinforce.rs` |
| `Raid` | out on a tower, node or encounter site (`to` = the site) | soldiers | `site.rs`, `naval.rs` |
| `March` | on the march against a city | soldiers | `military.rs` |
| `Muster` | walking to the rally point of a party being assembled (`to` = the rally, `party` = the muster) | soldiers | `raid.rs`, `war.rs` |
| `Escort` | going with a settler, an engineer or a spirit (`with` = the unit it guards) | soldiers | `escort.rs` |
| `Meld` | a spirit on its way to an open node (`to` = the node) | spirits | `meld.rs` |
| `Road` | engineers building a road or walking to where it starts (`to` = the city or frontier it leads to) | engineers | `road.rs` |
| `Scout` | scouting by land or sea (`to` = the patch) | soldiers, ships | `scout.rs`, `garrison.rs`, `naval.rs` |

Every role is stored. The last ones are **base roles** (`Role::is_base`): the audit gives them from
where a unit stands and what it is doing when no routine has a task for it, and a routine's `claim`
takes any of them over. `has_role` means "a routine has a task for it", that is, a role that is
not a base role.

| Base role | For | `to` |
|---|---|---|
| `MoveTo` | a unit with an order of its own (a plain walk) | the end of its path |
| `Garrison` | a soldier standing in an own city | that city |
| `TransportFree` | a ship with no plans for it: any task can take it | none |
| `AwaitingOrders` | nothing else fits: soldiers in it are *adrift*, ships and other units *unassigned* | none |

### Ships

A ship is in one of four states, each a role of its own. `TransportFree` means no plans for it: any
task can take it. The ship that comes for waiting units (`pickup.rs`, task 647) holds a target (`to`)
and the units planned for it are tied to it (`with`; they keep their own roles and the tag
`Crossing::Sea`). Its role is set by the count:

- `TransportSpace`: planned units < the ship's capacity: on its way to the sea square beside the
  boarding tile.
- `TransportReady`: planned units = the capacity.
- `Transporting`: the sail order was accepted (`Registry::depart`): the ship is on its way to the
  drop square beside the destination with the units aboard. When they are ashore their tag is
  cleared (a landing party is released: the war job judges it where it stands) and the ship is free.

Every voyage is a pickup, whoever its units are: a landing party (`March` with the enemy city as
target), a reinforcement, a settler for a site over the sea. `Registry::plan_ship` sets the role from the count; the
campaign has no ferry code of its own any more, and `Role::Voyage` and `Role::Colonize` are gone.

`Pickup` in the code is a read-back of the registry (`pickup::voyages`), not a store.

**Where they board and land.** A boarding tile and a landing square hold no guarded site and nobody
hostile in sight: a walk onto either moves nobody and is not refused. A tile with room for part of a
stack gets that part; the rest wait, and stay booked while the tile or the way to it is full. A
booking ends without a log line when the world ends it (the ship is gone, the target needs no sea
crossing any more, the boarding tile can no longer be walked onto); `BadPassenger` is logged for the
other faults.

The three scouting roles (`Scout`, `PlanarScout`, `SeaParty`) count as "out scouting"
(`is_scouting`, `scouting`), which the army and garrison use to leave those units alone.

## 4. Taking and giving a role

- `claim(unit, role, turn, to)`: gives the unit the role. A unit that holds a role of **equal or
  higher priority** keeps it and the claim is refused (the error is the role it holds); a lower one
  is taken over; claiming the role already held renews it. So a unit **cannot hold two roles**: a
  raider is not marched, a marching soldier is not sent home.
- `release(unit, role)`: hands the role back, only if the unit holds that role; the unit is then
  `AwaitingOrders`, never without a role. `release_role`,
  `release_many`, `release_any`, `retain_role`, `prune` are the list forms.
- `assign(role, units, turn)`: the holders of the role become exactly these units (a new march or
  muster).
- `members(role)`, `is`, `any`, `count`, `target_of`, `targets`, `targeted`, `set_target`: ask.
- `track(unit, pos, ordered)` counts the looks a unit has stood still on an order (scouts and
  settlers give a blocked target up after a few); `clear_track` forgets it.

A routine that picks units for a task takes the free ones (a base role only, no task of a routine) and claims
the role for them; the claim is the one test that a unit is free, so the routines no longer list
each other's roles to stay out of the way.

## 5. The dispatcher and the end-of-turn audit

A walk the AI orders is walked at once and then at the end of each of its turns (`game-flow.md`
3.2, task 830): during its turn a unit still on its way stands where the last turn left it, with its
moves unspent. The end-of-turn walk never attacks; the AI sends the unit into a fight itself.
A ship whose passengers have no moves left to step ashore keeps them booked until they have gone.

Since task 645 (`docs/ai-design.md` section 14) a unit with nothing to do **awaits orders where it
stands**, and the turn is run by the dispatcher (`dispatch.rs`):

1. **Register** (`roles::register`, also at the very start of the AI's turn): units built, hired or
   summoned since are written into the registry at once, with the base role their place says. A
   unit with no entry at all counts as awaiting orders (`Roles::is_free`).
2. **Give up the missions that are over** (`dispatch::revalidate`, before any new job): a unit whose
   raid, scout or meld mission cannot be fulfilled any more (the lair is gone, the ground is
   explored, the node is taken) or whose order stopped short of its target (a raid counts as arrived
   beside its site) loses its role and its order and awaits orders. Counted as `missions over,
   unit reset to awaiting orders`.
3. **The jobs, in the order of the checklist** (the list in `dispatch.rs`): a city in danger
   (`perimeter.rs`), settlers, engineers and spirits, escorts, scouts, then the old war code
   (`site`, `military`, `naval`, `pickup`) until tasks 646 and 647 move it into jobs. A job
   takes units that are **free** (`Roles::is_free`: no task, not a city's garrison) and claims a
   role for them.
4. **The audit** (`roles::audit`, the last step): tidy the registry, sync every unit again, and
   count. `Event::RolesAudited { adrift, unassigned, unregistered, reset }` is the soak's `units
   adrift (no role)` (soldiers awaiting orders, summed over turns: the pool), `ships and other
   units with no role`, `units made in the turn with no registry entry` (code that makes a unit in
   the middle of the turn without registering it: to be fixed one by one) and the resets of step 2.
   The audit sends nobody anywhere.

**Garrison** is the role of a city's base defenders only: `garrison::post` picks them by the defence
strength the city wants (`wanted_strength`: `garrison_wanted` average soldiers), slow defensive
units first; fast units and heroes are kept for the pool. The soldiers beyond that in the city
await orders like any other.

### The escort

A settler that leaves a city for its site (`expand.rs` calls `escort::walk`) is given the strongest
free soldier the city can spare above `garrison_wanted`; a city with none sends its settler alone.
The two are sent to the site as one stack, with one `GoTo`, at the pace of the slower; a charge
that can cross what its escort cannot goes alone, and with no escort and an enemy near it waits.
After the settlers have moved, `escort::run` frees the escorts whose charge is gone and stops an
unescorted settler or spirit when an enemy comes near; `dispatch::repair_escorts` sends an escort
that is apart from its charge after it. Settlers, spirits on their way to a node
(`escort::walk`), healers and weak heroes alone in the field (`escort::stragglers`) and engineers
working within 8 squares of a rival's city (`escort::guard`, which takes the soldier from the
nearest city that can spare one and sends it to the engineer, who waits for it) are escorted. Not
built: the charge waiting when its escort is lost and an enemy is near.

### Orders

The AI moves its units itself, with `Command::GoTo` (road work with `Command::BuildRoad`), and ends a
walk it no longer wants with `Command::ClearOrders`. It gives no other standing order of a player
(`Command::GiveOrder`). The core walks a `GoTo` at once and at the end of each turn (section 5). The
registry holds the role and the target, so a walk that ended short of it is sent again, or the
mission dropped, by the dispatcher's step 2. Task 830 moves this movement into a helper in core that
reads the tasks from the ledger (`docs/TASKS.md`).

### The road

Engineers (a unit with `Construction`) are not soldiers (`is_fighter` leaves them out), so they
never garrison, march or get sent home. `road::run` gives each idle one work, with no plan kept but
`Ai::road_checked` (the own cities already looked at, cleared when the number of cities changes):

1. In an own city: the nearest three own cities within 12 squares, in order; the first whose way
   (the game's own path for the engineer) has a square without road gets `BuildRoad` along it.
   A city with no such link is marked looked at.
2. Else it walks to the nearest city not looked at.
3. With every city looked at: a stretch of up to 8 squares of the way to the campaign target, else
   to the nearest tower, built from where it stands, or a walk to the last square that has a road.
   Within 8 squares of a rival's city the engineer builds only with an escort
   (`escort::guard`) and waits where it stands until one is with it.
4. With nothing left it gives the role back.

`road::build_engineer` (after the cities' plans) makes the biggest free city build engineers while
there is road work and no engineer lives, with the builders' hall first when the city lacks it.
Not built: a second engineer when the roads are long.

### The perimeter

`perimeter::run` (called from `war::run` where the call-in of troops used to be) looks at
the zone of every own city: the 21 squares its people work and the towers and nodes within 4
squares of it. For each stack of the enemy, a raider or a monster standing in a zone, it takes the
free soldiers within 6 squares (the army's spare ones, over `garrison_wanted`) and the soldiers that
already hold `Perimeter` for it:

- if together they are `attack_force` times as strong, they gather on one square beside it, where
  `attack.rs` attacks when those standing there are enough; `to` = its square;
- units of a player not at war count only when the AI is open to war on that player;
- if it is the stronger they join the garrison of the city (`to` = the city);
- a holder whose enemy is gone (or whose city is no longer threatened) gives the role back.

`Ai::threatened` (the cities the economy builds soldiers for) is set from the same look
(`military::threats`).

### The meld

`meld::run`: a spirit (the ability to meld; not a fighter for the army) takes `Meld` with the
nearest open node it can reach (`to`): not warped, not already the wizard's, no guards. It is given
a meld order (the core melds it when it arrives), goes with an escort when its city has a soldier
to spare; one that
stands still four turns on the way gives the node up (`Ai::bad_sites`).

## 6. Where to add a role

1. Add the variant to `Role` and a place in `priority` (a task that should not be taken from a unit
   goes before the ones that can).
2. Take it with `claim` where the task starts, release it where the task ends, hand the unit to the
   next role in the same place.
3. Make the routine that must leave those units alone ask `has_role` (or the specific role), not a
   list of other roles.
4. Add the role to the tables above, and a test that the claim is refused for a unit holding a
   better role.

Tests (`roles.rs`, and each routine's own): every unit has an explicit role with its target, one role per unit, refused and taken-over claims,
groups and ties, standstill count, stranded soldiers waiting for a ship.

## 7. Musters

A party is assembled before it sets off. The registry holds one record per muster (`Muster`:
kind, rally square, objective, start turn) and the members hold `Muster` with the record's id as
their `party` and the rally as `to`.

Where a party gathers is `assembly.rs`'s (`ai-design.md`, "Gathering a force"): on the way of
the unit that arrives last, never in a city.

- `MusterKind::Raid` (`raid::gather`): free soldiers spread over the empire, none of whose squares
  could take a site alone but the best nine with the wizard's spells could, go to the rally point
  for the easiest site they could take (a lair, or a tower on a plane with no rival city), or to the
  square beside it when no rally point serves. `raid::release` ends it when all stand on the rally
  point or after `muster_turns`, and sends on a member with no order; `raid::form` then sends the
  party from there. No new raid starts while one is mustering.
- `MusterKind::Campaign` (`war::run`): the soldiers picked for the campaign hold `Muster` while
  they gather: on the rally point (those already beside the target hold it with the square beside
  it as their goal), at the landing site, or on the near side of a crossing. They stay the army's
  (`stacks` counts them). The muster ends when the march leaves (they become `March`), when the
  target changes or is called off, or when there is no campaign. A rally or target that changed
  ends the old muster and begins a new one. A landing's muster stays at the landing site while the
  ships fetch its walkers.

## 8. Plans and ends

- A settler or a spirit with no escort at its side does not set out while an enemy unit is within
  4 squares (`escort::walk`); one already walking stops (`escort::run`) and goes on when the enemy
  is gone or a soldier is spare.
- A healer or a weak hero (much weaker than the average fighter) alone outside a city is given an
  escort from the nearest city (`escort::stragglers`); it ends in a city or in a stack of fighters.
- A settler whose site (`city_site.rs`) is over the sea holds `Settle` with that site as target and the tag
  `Crossing::Sea`; the pickup planner fetches it, before the other groups, and the general rule for units waiting
  for a ship builds one when none is free (`economy.rs`, `rescue_port`). There is no separate voyage plan.
- Spirits: a spirit with a node to meld with holds `Meld`; one with none is recruited as a `Scout`
  (it flies); a node opening takes it from the scouting. The wizard summons a spirit ahead of
  every other spell when it has none and a node is open, a raid is out to clear one, or in the
  build phase while nobody scouts (`meld::spirit_wanted`, `wizardry::cast`).


- A role's plan begins at the turn the unit took it or was last given a new target (`planned`
  in the assignment). The plan clock (`plan_clock.rs`) reads it for raiders, settlers, scouts and
  the planar scout, and drops the plan that has stood too long. A cleared site restarts it.
- When a role ends (target reached, called off, nothing left to do, dropped by the clock) the unit
  is demoted to `AwaitingOrders` in the same call; the next routine that wants it claims it. A
  scout that finds nothing reachable to explore ends its role alone.
- A unit made in the middle of the AI's turn is given a base role by the end-of-turn audit; units
  made by the turn change or by a summon of the wizard's step are given one at the start of the AI's
  turn (`roles::register`, after `wizardry::run`).

## 9. The autobuilder reads the registry

`economy::cities` counts the roles once per turn (`Roster`) and builds by them:

- soldiers holding `AwaitingOrders` (4 or more): no more soldiers are built; a war (a campaign)
  with nobody holding `March` makes the cities build attackers;
- the ships holding `TransportFree` have places for every unit waiting for a ship (and a landing party
  when a landing is planned): no ship is built; otherwise (`Roster::short_of_ships`, task 758) the own
  port nearest to the waiting units builds one;
- units tagged `Crossing::Sea` with no ship assigned also ask the wizard for a spell (task 701,
  `wizardry::cross_target`): Water Walking or Wind Walking (`Kind::Cross`), and Flight (a `UnitBuff`), is
  cast on a walker that waits, the cheapest spell first, from the pool without the third the other
  spells leave over; the next sweep (`pickup::tidy_unit_registry`) drops the tag of a unit that no
  longer needs a ship;
- nobody holds a scouting role, no soldier is spare and the frontier is open (build phase): one city
  builds the fastest cheap fighter, which `scout.rs` recruits.

## 10. Known gaps

- A second engineer works the roads only when the roads are long (`road::engineers_wanted`); it
  takes a link or stretch no other engineer is on, but the stretch toward a far target is built by
  one engineer at a time.


## Crossing (17 November 2026)

`Assignment::crossing` (`Crossing::Sea` or `Crossing::Plane`) tags a unit that holds a role with a target but cannot walk there: the target is on the other plane (`Plane`), or on this plane with no known land route (`Sea`). `dispatch::revalidate` sets it for the roles with a target (raid, scout, meld, march, settle, road), and `dispatch::repair_escorts` sends an escort after its charge (the charge's square is its target; it runs after the jobs that move the charges) when the walk is refused as impassable; a unit that holds the tag is not sent again, keeps its role and its target, and waits for the transport (ships, a tower, a spell: task 647, `Registry::needing`). A new target clears the tag. The unit is tagged with what it wants to do, not with the means.
