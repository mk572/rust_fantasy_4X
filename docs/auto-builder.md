# City auto-builder (PLANNED item 7)

The design the project owner agreed, with every weight. Numbers marked *(tune)* are first
guesses to adjust in play; they are constants and one table at the top of
`crates/f4x-core/src/city/autobuild.rs`, so tuning touches no rules.

Before this, the Grand Vizier was one switch per player walking a fixed list
(`rules.ron: autobuild_order`). The list stays as the **tie-break** between equal scores.

## 1. Shape

The auto-builder is an option **per city**. Switching it on opens a dialog that fills in a
`BuildPlan`. Each time the city finishes an item (and when the plan is first set), the engine
picks the next item from the plan. The same plan and the same scoring serve the human's cities
and the AI's.

The player-level Grand Vizier switch stays as "the default plan for every city that has none":
a city with its own plan uses it; otherwise a player with the Vizier on uses the default plan
(**buildings only, balanced**, so it behaves as the Vizier did, but with the scoring below).

## 2. The plan

| Field | Values | Default |
|---|---|---|
| Mix | `Buildings` / `Units` / `Both` | `Both` |
| Primary focus | one focus or `Balanced` | `Balanced` |
| Secondary focus | none or one focus | none |
| Settlers | on / off, with a minimum population | off, 4 |
| Event prompt | on / off | off |

"Both" means **equal production**: the city keeps how much production it has spent on units and on
buildings, and builds whichever side is behind (buildings on a tie).

**Focus** values: Magic (mana and research), Population (food and happiness), Production,
Defense, Offense, Exploration (ships, flying units, settlers, engineers), Gold.
`Balanced` weighs all seven equally, and then has no secondary.

**Focus weights**: primary 0.65 and secondary 0.35. With no secondary, the primary is 1.0.
`Balanced` gives each focus 1/7.

## 3. What the city decides, in order

On each choice the first rule that applies decides:

1. **Garrison first** (mix is not `Buildings`): the city has fewer defenders than its minimum
   (section 4): build the best defender.
2. **Unrest**: the city has rebels: build the best building that lowers unrest, if one is left.
3. **Gold floor**: net gold of the treasury is below 3 *(tune)*: build the best building that adds
   gold (a marketplace, bank or merchants' guild), and if none is left, Trade Goods.
4. **Settlers** (section 6), when allowed and wanted.
5. **Mix**: pick the side (units or buildings) that is behind, then the best item of that side
   (sections 5 and 7). A side with nothing to build gives way to the other.
6. **End of the list** (section 8).

Buildings whose gold upkeep the treasury cannot bear (net gold below the upkeep) are skipped in
rules 2 and 5, as the old Vizier did.

## 4. Garrison

Minimum defenders by city size (population in citizens) *(tune)*:

| Size | 0 (outpost) | 1 to 2 | 3 to 5 | 6 to 8 | 9+ |
|---|---|---|---|---|---|
| Minimum | 0 | 1 | 2 | 3 | 4 |

Raises: **+1** when a rival wizard's city is known within 10 squares; **+2** when Defense is the
primary focus, **+1** when it is the secondary. A defender is a Normal unit with a melee or ranged
attack standing in the city. An outpost builds nothing.

## 5. Scoring buildings

For a building *b* and a plan:

```
V(b)   = sum over focus f of weight(f) * affinity(b, f)            affinity is 0 to 3, table below
W(b)   = V(b) + 0.5 * max(W(d)) over buildings d that need b (depth 2, not yet built)
need   = 2.0 if the building adds food and the city's food is not above zero
         (3.0 if it lowers unrest and the city has rebels)
score  = W(b) * need / sqrt(cost) / (1 + 0.1 * upkeep_gold)
```

`W` gives credit for what a building opens (a Library for the University). Of equal scores the
building earlier in `autobuild_order` wins. A building scoring zero is not built (that is how
the focus ends a list).

### 5.1 Affinity table

Focus columns: **Mag** magic, **Pop** population, **Prod** production, **Def** defense, **Off**
offense, **Exp** exploration, **Gold** gold.

| Building | Mag | Pop | Prod | Def | Off | Exp | Gold |
|---|---|---|---|---|---|---|---|
| builders_hall | 0 | 1 | 1 | 0 | 0 | 0 | 0 |
| granary | 0 | 2 | 0 | 0 | 0 | 0 | 0 |
| farmers_market | 0 | 3 | 0 | 0 | 0 | 0 | 0 |
| foresters_guild | 0 | 1 | 2 | 0 | 0 | 0 | 0 |
| shrine | 1 | 1 | 0 | 0 | 0 | 0 | 0 |
| temple | 2 | 2 | 0 | 0 | 0 | 0 | 0 |
| parthenon | 2 | 2 | 0 | 0 | 0 | 0 | 0 |
| cathedral | 3 | 2 | 0 | 0 | 0 | 0 | 0 |
| oracle | 1 | 2 | 0 | 1 | 0 | 1 | 0 |
| library | 2 | 0 | 0 | 0 | 0 | 0 | 0 |
| sages_guild | 2 | 0 | 0 | 0 | 0 | 0 | 0 |
| university | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| wizards_guild | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| alchemists_guild | 1 | 0 | 0 | 0 | 1 | 0 | 2 |
| animists_guild | 2 | 2 | 0 | 0 | 1 | 0 | 0 |
| sawmill | 0 | 0 | 2 | 0 | 0 | 0 | 0 |
| miners_guild | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| mechanicians_guild | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| marketplace | 0 | 0 | 0 | 0 | 0 | 0 | 2 |
| bank | 0 | 0 | 0 | 0 | 0 | 0 | 3 |
| merchants_guild | 0 | 0 | 0 | 0 | 0 | 0 | 3 |
| barracks | 0 | 0 | 0 | 1 | 1 | 0 | 0 |
| smithy | 0 | 0 | 0 | 0 | 1 | 0 | 0 |
| armory | 0 | 0 | 0 | 1 | 2 | 0 | 0 |
| fighters_guild | 0 | 0 | 0 | 1 | 3 | 0 | 0 |
| armorers_guild | 0 | 0 | 0 | 2 | 2 | 0 | 0 |
| war_college | 0 | 0 | 0 | 2 | 3 | 0 | 0 |
| stables | 0 | 0 | 0 | 0 | 2 | 1 | 0 |
| fantastic_stable | 0 | 0 | 0 | 0 | 3 | 0 | 0 |
| city_walls | 0 | 0 | 0 | 3 | 0 | 0 | 0 |
| ship_wrights_guild | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| ship_yard | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| maritime_guild | 0 | 0 | 0 | 0 | 0 | 3 | 1 |

A test keeps the table in step with `content/buildings.ron`: every building must have a row.

## 6. Settlers

Settlers are built ahead of the focus choice when **all** hold:

- the plan allows settlers and the city's population is at least the plan's minimum;
- **the city is near its limit**: population is within 2 of the largest the land feeds
  (`max_size`), so growth is about to stop;
- **there is open space**: at least one site where a city may stand (`can_found_city`, on
  explored land, clear of lairs and nodes, at least `min_city_distance` from every known city),
  within 9 squares of the city, counting only sites that are at least `min_city_distance` apart
  (at most 5 are counted);
- fewer settlers are already wanted than there is room: the settlers alive or being built by
  this wizard within 9 squares must be fewer than `min(open sites, 2)`.

With the Exploration focus, settlers (and engineers) are also candidates in the ordinary unit
choice, under the same open-space and population conditions. Garrison and gold-floor rules
come first.

## 7. Choosing a unit

Candidates are the units the city can build, minus settlers (section 6) and units with no
attack, except in the exploration pool. Each candidate gets three profile values, each divided by
the best candidate's, so they lie between 0 and 1:

| Profile | Computed from |
|---|---|
| Offense | `figures * (max(melee, ranged) * 1.5 + moves * 0.5 + hit points * 0.3)`, ranged times 1.2 |
| Defense | `figures * (defense * 1.5 + hit points * 0.5 + resistance * 0.1 + melee * 0.5)` |
| Exploration | moves, sight, flying or sailing movement, transport, road building; settlers and engineers (when allowed) |

```
fit(u)   = off_weight * Offense(u) + def_weight * Defense(u) + exp_weight * Exploration(u)
score(u) = fit(u) / sqrt(cost)
```

`off_weight` is the Offense weight plus half the weight of the focuses that have no unit of
their own (Magic, Population, Production, Gold); `def_weight` and `exp_weight` are the Defense
and Exploration weights.

The pick is a **weighted roll** on the game's seeded random numbers: each candidate is weighed
`(score / best score)^4`, and candidates under a third of the best are dropped, so the best unit
usually wins but a city does not build one unit over and over. The garrison rule uses the
Defense profile alone, with no roll.

Every unit built posts a **news message** (the city's name and the unit); the auto-builder
never moves units.

## 8. When the list runs out

When the chosen side has no item left, the other side is tried. When neither has, in order:
a building that adds gold; **Housing**, when Population weighs in the plan (as primary or
secondary focus, or under Balanced) and the city can still grow; **Trade Goods**. Gold is the
last fallback.

Housing and Trade Goods never finish, so a city with a plan **looks again every turn** while it
is on one of them (or on nothing): a new building may have become worth building, or the gold
may have run low. Switching a plan on does the same at once for an idle city; a city with work
in hand finishes it first.

## 9. Events (optional)

With the event prompt on, the auto-builder turns itself off for a city and posts a message when
an enemy stack is within 4 squares of the city, or the city has rebels it cannot fix with a
building. Off by default; the garrison rule alone reacts then.

## 10. Shared with the AI

- The scoring and choice live in `f4x-core` (`city/autobuild.rs`) and take the player's view of the
  game, so the human's auto-builder, the scripted AI and the learning AI all call them. They are
  tested on fixed city situations.
- The **scripted AI** sets plans by rule instead of its own build list (`f4x-ai/src/economy.rs`,
  `plan_for`), every turn for every city, and the cities then choose by themselves:
  - focus: a rival wizard's city within 12 squares gives Offense with Defense behind it; else,
    while the wizard has fewer than 7 cities (counting settlers on the way), Production with
    Population behind it; else Balanced;
  - mix: `Units` while the army is under its target (home guard plus 4 plus half the cities) and
    the treasury and the food can bear more; `Buildings` when broke and the city has a defender;
    else `Both`;
  - settlers: allowed from population 3 with `eager_settlers` (a plan field only the AI uses: no
    wait for the population limit) while there is room for more cities, the wizard is not broke
    and the food is not short; the open-space check of section 6 still applies;
  - it still sets the production of the port that builds the ship, and still buys items
    (settlers first, then a city's first guard, then buildings, then the rest).
- The **learning AI** gets the plan in its action space: it learns which primary and secondary
  focus and which mix to give each city.
- The AI's **auto-buy**: a wizard with a lot of gold looks for items to buy in the cities it
  auto-builds. The human has no auto-buy.

## 11. Build order and status

1. `BuildPlan`, per-city storage, saving, the command and the console command. **Done**
   (`Command::SetAutoBuild`, console `autobuild`, `CitySave::autobuild`).
2. The scoring and the choice (sections 3 to 8), with tests on fixed cities. **Done**
   (`city/autobuild.rs`, `autobuild/tests.rs`, and `f4x-data/tests/it/autobuild.rs` against the shipped
   content). The event prompt flag is stored but does nothing yet.
3. Hooking it to production: the choice after each item, the Vizier default, the news message.
   **Done**: `apply_production` asks `Game::autobuild_choice` after a building and after a unit;
   the news line is the existing "city: item complete" message.
4. The dialog in the client. **Done** (`autobuild_dialog.rs`: the Auto button at the bottom
   left of the production screen (`production_screen.rs`, Change on the city screen) opens it; `--shot --city
   --autobuild` shows it, `--shot --city --choose` the window with the button).
5. The scripted AI sets plans. **Done** (not soak-tested: the behaviour tests are ignored by
   default and were not run).
6. The learning AI's action space. Not done.
7. The event prompt (section 9). Not done.

Differences from the plan above, as built: the Vizier's old `autobuild_order` list is only the
tie-break; Mix `Buildings` with the Vizier default skips the garrison rule.
