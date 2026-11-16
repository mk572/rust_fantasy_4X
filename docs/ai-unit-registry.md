# The AI unit registry

How the scripted AI (`crates/f4x-ai`) keeps track of what each of its units is for. Part of the AI
design ([`ai-design.md`](ai-design.md) section 13). Code: `crates/f4x-ai/src/roles.rs`; the
routines that take and give roles are named below.

## 1. The rule

Every unit the wizard owns has **exactly one role** at any time, and the AI never loses track of
one. A unit with nothing to do has the role `AwaitingOrders`; it is not left without one.

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

- a **ferry**: the ship holds `Voyage` with no `with`; each passenger holds `Voyage` with `with` =
  the ship (`sail`, `ferries`, `passengers_of`, `end_voyage`);
- a **colony voyage**: the ship and the settler hold `Colonize`, the settler's `with` is the ship
  (`colonists`);
- a **rescue**: the ship holds `Pickup` with `to` = the sea square; the soldiers it came for keep
  `AwaitTransport` and their `with` is the ship (`pickup::voyages`);
- an **escort** (planned): the escort holds `Escort` with `with` = the unit it guards; if the
  charge is released or dies, the audit releases the escort too.

The voyage data that is not about units (the landing plan, the colony plan, `naval_bad`,
`colony_bad`) stays in the plan fields of `Ai`; only who is on it is in the registry.

## 3. The roles

Listed in priority order (the lower number wins a claim, section 4):

| Role | For | Held by | Taken and given in |
|---|---|---|---|
| `Colonize` | the ship and settler of a colony by sea | ship, settler | `colony.rs` |
| `Settle` | a settler walking to a site to found a city (`to` = the site) | settlers | `expand.rs` |
| `Voyage` | a ship carrying a landing, and its passengers | ship, soldiers | `naval.rs` |
| `SeaParty` | the fighters a sea scout carries to put ashore | soldiers | `scout.rs` |
| `PlanarScout` | the scout walking to an open tower to change plane (`to` = the tower) | one unit | `scout.rs` |
| `Pickup` | a ship fetching stranded soldiers (`to` = sea square) | ship | `pickup.rs` |
| `AwaitTransport` | a stranded soldier waiting on a shore tile for a ship (`to` = the tile) | soldiers | `pickup.rs`, the audit |
| `Raid` | out on a tower, node or encounter site (`to` = the site) | soldiers | `site.rs`, `naval.rs` |
| `March` | on the march against a city | soldiers | `military.rs` |
| `Muster` | walking to a city to be sent against a tower together | soldiers | `siege.rs` |
| `Scout` | scouting by land or sea (`to` = the patch) | soldiers, ships | `scout.rs`, `garrison.rs`, `naval.rs` |
| `Return` | walking home to the nearest own city (`to` = the city) | soldiers | the audit |

Roles the registry does **not** store, because the world already says them (`roles::derive`):

| Role | When |
|---|---|
| `Garrison` | a soldier standing in an own city |
| `Docked` | a ship in an own city |
| `Ordered` | a unit with an order of its own (a plain walk) |
| `AwaitingOrders` | none of the above: soldiers in it are *adrift*, ships and other units are *unassigned* |

Planned roles (tasks 625 to 628): `Perimeter`, `Explore`, `Escort`, `Road`, `Meld`.

The three scouting roles (`Scout`, `PlanarScout`, `SeaParty`) count as "out scouting"
(`is_scouting`, `scouting`), which the army and garrison use to leave those units alone.

## 4. Taking and giving a role

- `claim(unit, role, turn, to)`: gives the unit the role. A unit that holds a role of **equal or
  higher priority** keeps it and the claim is refused (the error is the role it holds); a lower one
  is taken over; claiming the role already held renews it. So a unit **cannot hold two roles**: a
  raider is not marched, a marching soldier is not sent home.
- `release(unit, role)`: hands the role back, only if the unit holds that role. `release_role`,
  `release_many`, `release_any`, `retain_role`, `prune` are the list forms.
- `assign(role, units, turn)`: the holders of the role become exactly these units (a new march or
  muster).
- `members(role)`, `is`, `any`, `count`, `target_of`, `targets`, `targeted`, `set_target`: ask.
- `track(unit, pos, ordered)` counts the looks a unit has stood still on an order (scouts and
  settlers give a blocked target up after a few); `clear_track` forgets it. The audit's own count
  for `Return` is `note_positions`.

A routine that picks units for a task takes the free ones (no registry role, no order) and claims
the role for them; the claim is the one test that a unit is free, so the routines no longer list
each other's roles to stay out of the way.

## 5. The end-of-turn audit

`roles::audit` runs at the end of the AI's turn (`lib.rs`, after `colony`, `pickup`):

1. **Look at every unit** (`derive`): soldier, ship or other; give it the registry role, or the
   role its place and orders say, or `AwaitingOrders`.
2. **Tidy the registry**: drop the units that are gone, and the `Return` of a soldier that is in a
   city or has not moved for `RETURN_PATIENCE` looks.
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

## 6. Where to add a role

1. Add the variant to `Role` and a place in `priority` (a task that should not be taken from a unit
   goes before the ones that can).
2. Take it with `claim` where the task starts, release it where the task ends, hand the unit to the
   next role in the same place.
3. Make the routine that must leave those units alone ask `has_role` (or the specific role), not a
   list of other roles.
4. Add the role to the tables above, and a test that the claim is refused for a unit holding a
   better role.

Tests (`roles.rs`, and each routine's own): one role per unit, refused and taken-over claims,
groups and ties, standstill count, the audit sending a lone soldier home and keeping it `Return`,
stranded soldiers waiting for a ship.

## 7. Known gaps

- Several routines still *lose* units by clearing a role without choosing the next (a called-off
  march, a finished raid): the audit catches them a turn later as adrift. The fix is to hand the
  unit on at the same place (task 624).
- No ship is built for a pickup; a stranded group waits until a free ship exists.
- The plan clock (`plan_clock.rs`) still drops stale plans by plan kind; it should read `since`
  from the registry.
