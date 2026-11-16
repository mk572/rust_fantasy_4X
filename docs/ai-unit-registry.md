# The AI unit registry

How the scripted AI (`crates/f4x-ai`) keeps track of what each of its units is for. Part of the AI
design ([`ai-design.md`](ai-design.md) section 13). Code: `crates/f4x-ai/src/roles.rs`; the
routines that take and give roles are named below.

## 1. The rule

Every unit the wizard owns has **exactly one role, written in the registry**, at any time, and the
AI never loses track of one. Nothing is derived: a soldier in a city is registered as `Garrison`
with that city, a unit on its way as `MoveTo` with the square, a soldier sent home as `Return`
with the city. A unit with nothing to do is registered as `AwaitingOrders`, the lowest priority
but a role all the same. A routine that gives a role back puts the unit on `AwaitingOrders` at
once (it is never removed); the next audit gives it the base role its place says.

The role of a unit lives in one place, the registry `Ai::roles` (`roles::Registry`). No routine
keeps a list of its own units (`raiders`, `scouts`, `marching`, ... no longer exist). A routine asks
the registry who holds its role, and takes or gives roles through it.

## 2. What the registry holds

A map from a unit's `Serial` to one `Assignment`, ordered by serial, saved with the AI:

| Field | Meaning |
|---|---|
| `role` | what the unit is for (section 3) |
| `since` | the turn it took the role |
| `to` | the square the role sends it to, if any (a raid site, a scout target, a settler's site, the shore tile a stranded soldier waits at, the sea square a rescue ship lies at, the city a soldier returns to) |
| `party` | the id of the group it acts with, if any (a raid party, a ferry and its passengers, a colony voyage, a rescue ship and the soldiers it came for) |
| `with` | the one unit it is tied to (a passenger's ship, a colony settler's ship, a waiting soldier's rescue ship; later: an escort's charge) |
| `last_pos`, `stuck` | where it stood at the last look and for how many looks in a row it has not moved, for roles that give up a target that cannot be reached |

### Groups

A group is the set of units that share a `party` id (`new_party`, `group`, `members_of`,
`party_of`). `with` ties one unit to another. Together they cover:

- a **ferry**: the ship holds `TransportReady` with no `with`; each passenger holds `Voyage` with
  `with` = the ship (`sail`, `ferries`, `passengers_of`, `end_voyage`);
- a **colony voyage**: the ship and the settler hold `Colonize`, the settler's `with` is the ship
  (`colonists`);
- a **rescue**: the ship holds `TransportSpace` with `to` = the sea square (then `TransportReady` and
  `Transporting` with the port); the soldiers it came for keep `AwaitTransport` and their `with` is the ship
  (`pickup::voyages`);
- an **escort**: the escort holds `Escort` with `with` = the unit it guards (`escort_of`); when the
  charge no longer holds `Settle`, `Road` or `Meld` (it founded its city, finished, or died)
  `escort::run` releases the escort.

The voyage data that is not about units (the landing plan, the colony plan, `naval_bad`,
`colony_bad`) stays in the plan fields of `Ai`; only who is on it is in the registry.

## 3. The roles

Listed in priority order (the lower number wins a claim, section 4). The roles with a task:

| Role | For | Held by | Taken and given in |
|---|---|---|---|
| `Colonize` | the ship and settler of a colony by sea | ship, settler | `colony.rs` |
| `Settle` | a settler walking to a site to found a city (`to` = the site) | settlers | `expand.rs` |
| `Voyage` | a passenger of a landing; `with` = its ship | soldiers | `naval.rs` |
| `TransportReady` | a ship whose load is decided and tied to it, with its target, about to sail: the landing party chosen (`to` = the sea square), a rescue with everyone aboard (`to` = the port) | ships | `naval.rs`, `pickup.rs` |
| `Transporting` | a ship that has sailed with its load, on its way to its target (a landing to its sea square, rescued soldiers to a port) | ships | `naval.rs`, `pickup.rs` |
| `SeaParty` | the fighters a sea scout carries to put ashore | soldiers | `scout.rs` |
| `PlanarScout` | the scout walking to an open tower to change plane (`to` = the tower) | one unit | `scout.rs` |
| `TransportSpace` | a ship with a target and room: on its way to the port to load a landing (`to` = the port), or to the sea square where stranded soldiers wait (`to` = the square, they are tied to it) | ships | `naval.rs`, `pickup.rs` |
| `AwaitTransport` | a stranded soldier waiting on a shore tile for a ship (`to` = the tile; `with` = the ship that comes) | soldiers | `pickup.rs`, the audit |
| `Perimeter` | a soldier dealing with the enemy in the zone of a city (`to` = the enemy's square, or the city it joins) | soldiers | `perimeter.rs` |
| `Raid` | out on a tower, node or encounter site (`to` = the site) | soldiers | `site.rs`, `naval.rs` |
| `March` | on the march against a city | soldiers | `military.rs` |
| `Muster` | walking to a city to be sent against a tower together | soldiers | `siege.rs` |
| `Escort` | going with a settler, an engineer or a spirit (`with` = the unit it guards) | soldiers | `escort.rs` |
| `Meld` | a spirit on its way to an open node (`to` = the node) | spirits | `meld.rs` |
| `Road` | engineers building a road or walking to where it starts (`to` = the city or frontier it leads to) | engineers | `road.rs` |
| `Scout` | scouting by land or sea (`to` = the patch) | soldiers, ships | `scout.rs`, `garrison.rs`, `naval.rs` |
| `Return` | walking home to the nearest own city (`to` = the city) | soldiers | the audit |

Every role is stored. The last ones are **base roles** (`Role::is_base`): the audit gives them from
where a unit stands and what it is doing when no routine has a task for it, and a routine's `claim`
takes any of them over. `has_role` means "a routine has a task for it", that is, a role that is
not a base role.

| Base role | For | `to` |
|---|---|---|
| `MoveTo` | a unit with an order of its own (a plain walk) | the end of its path |
| `Garrison` | a soldier standing in an own city | that city |
| `TransportFree` | a ship with no mission and no order | none |
| `AwaitingOrders` | nothing else fits: soldiers in it are *adrift*, ships and other units *unassigned* | none |

### Ships

A ship is in one of four states, and each is a role of its own, with the units tied by `with`:

- `TransportFree`: no passengers decided, no target. `naval.rs` and `pickup.rs` take a free ship
  (`!has_role`).
- `TransportSpace`: a target is set and the ship has room. It is on its way to the port to load a
  landing, or to the sea square beside a shore tile where stranded soldiers wait; for a rescue the
  soldiers it came for hold `AwaitTransport` with `with` = the ship.
- `TransportReady`: the units to carry are decided and tied to the ship (landing passengers hold
  `Voyage`, `with` = the ship) and the target is set; the ship is about to sail. A rescue becomes
  ready when everyone is aboard, with the nearest port as its target.
- `Transporting`: the ship has sailed with its load (the order was accepted) and is on its way. When
  the passengers are landed the ship goes back to `TransportSpace` with the port as its target. A
  ship ends its voyage by `end_voyage`, and is then free again (`AwaitingOrders`, then
  `TransportFree` at the audit).

`ferries()` lists the ships of landings (ready, transporting or space, with no party); a rescue ship has a party
and is read back by `pickup::voyages`. `Ferry` and `Pickup` in the code are read-backs of the
registry, not stores. A colony ship keeps the mission role `Colonize` with its settler.

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
  settlers give a blocked target up after a few); `clear_track` forgets it. The audit's own count
  for `Return` is `note_positions`.

A routine that picks units for a task takes the free ones (a base role only, no task of a routine) and claims
the role for them; the claim is the one test that a unit is free, so the routines no longer list
each other's roles to stay out of the way.

## 5. The end-of-turn audit

`roles::audit` runs at the end of the AI's turn (`lib.rs`, after `colony`, `pickup`):

1. **Tidy the registry**: drop the units that are gone.
2. **Look at every unit** (`roles::sync`, which also runs at the start of the turn as
   `roles::register`, so units built, hired or summoned since are registered before any routine
   runs): a unit a routine has a task for keeps its role; a `Return` ends when the soldier is in a
   city, has no order or has not moved for `RETURN_PATIENCE` looks; every other unit is written
   into the registry with its base role and target (the table above).
3. **Count** the soldiers and the other units awaiting orders, and report them in
   `Event::RolesAudited { adrift, unassigned }` (the soak lines `units adrift (no role)` and
   `ships and other units with no role`).
4. **Soldiers awaiting orders** go to the nearest own city that can be reached and has room, in
   groups by square (`send_home`), and take the role `Return` until they are there. A group no
   city can take (no own city reachable by land) walks to a shore tile and takes `AwaitTransport`
   (`pickup::wait_at_shore`); a free ship then fetches it (`pickup::run`).
5. Ships and other units awaiting orders are only counted. Each is a role still to build (roads
   for engineers, nodes for spirits, an escort for a settler).

The number to watch in a soak is `units adrift`: the audit sends those soldiers home, but a soldier
that needs it every turn shows a routine that lets go of its units without a next role.

### The escort

A settler that leaves a city for its site (`expand.rs` calls `escort::walk`) is given the strongest
free soldier the city can spare above `garrison_wanted`; a city with none sends its settler alone.
The two get one `GoTo` as a stack, so they walk at the pace of the slower. After the settlers have
moved, `escort::run` frees the escorts whose charge is gone and sends an escort that fell behind to
its charge. Settlers, spirits on their way to a node (`escort::walk`) and engineers working within 8
squares of a rival's city (`escort::guard`, which takes the soldier from the nearest city that can
spare one and sends it to the engineer, who waits for it) are escorted. Healers and low-experience
heroes have no role yet. Not built: the charge waiting when its escort is lost and an enemy is near.

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

`perimeter::run` (called from `military::army` where the call-in of troops used to be) looks at
the zone of every own city: the 21 squares its people work and the towers and nodes within 4
squares of it. For each stack of the enemy, a raider or a monster standing in a zone, it takes the
free soldiers within 6 squares (the army's spare ones, over `garrison_wanted`) and the soldiers that
already hold `Perimeter` for it:

- if together they are `attack_ratio` times as strong, they gather on one square beside it and the
  ones standing beside it attack when they are enough (`military::engage`); `to` = its square;
- if it is the stronger they join the garrison of the city (`to` = the city);
- a holder whose enemy is gone (or whose city is no longer threatened) gives the role back.

`Ai::threatened` (the cities the economy builds soldiers for) is set from the same look
(`military::threats`).

### The meld

`meld::run`: a spirit (the ability to meld; not a fighter for the army) takes `Meld` with the
nearest open node it can reach (`to`): not warped, not already the wizard's, no guards. It goes
with an escort when its city has a soldier to spare, and melds when it stands on the node; one that
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
groups and ties, standstill count, the audit sending a lone soldier home and keeping it `Return`,
stranded soldiers waiting for a ship.

## 7. Known gaps

- Several routines still *lose* units by clearing a role without choosing the next (a called-off
  march, a finished raid): the audit catches them a turn later as adrift. The fix is to hand the
  unit on at the same place (task 624).
- No ship is built for a pickup; a stranded group waits until a free ship exists.
- The plan clock (`plan_clock.rs`) still drops stale plans by plan kind; it should read `since`
  from the registry.
- A unit made in the middle of the AI's turn (bought production, a summoned unit) is registered
  at the end-of-turn audit, not at the moment it appears; at the start of every turn there is none
  left without a role.
- The ship states `TransportSpace` and `TransportReady` follow the ship's order and passengers;
  `naval.rs` and `pickup.rs` still decide who boards through `Voyage`, `Pickup` and `AwaitTransport`.
  Moving that decision into the transport roles (a ship holding "units to board" before they are
  aboard) is not built.
