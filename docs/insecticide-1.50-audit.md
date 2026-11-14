# Insecticide 1.50 audit against the Rust rewrite

**Repository:** [mk572/rust_fantasy_4X](https://github.com/mk572/rust_fantasy_4X)  
**Purpose:** Compare the Insecticide Community Patch 1.50 release notes with the rewrite's design and implementation, and record a focused source audit.  
**Scope:** This is an audit, not a request to reproduce every patch change. Version 1.50 contains many fixes to bugs introduced or exposed by earlier patches, plus substantial corrections to movement, combat, diplomacy, AI turn sequencing and spell effects.

## Executive summary

Version 1.50 is unusually broad. Its most important themes are:

- **Movement and stack compatibility:** flying, non-corporeal and sailing units must be able to travel together where their movement capabilities permit it. The release also replaces earlier movement-type stacking changes that had introduced new edge cases.
- **Combat correctness:** damage distribution, healing, immunity checks, spell dispelling, battlefield effects, fleeing, summoning limits and unit death state all receive fixes.
- **Diplomacy and AI sequencing:** numerous diplomacy bugs are fixed, and AI turns become sequential per wizard rather than split into global phases. This is important for deterministic state and fair execution.
- **Spell and enchantment lifecycle:** removing an enchantment must also remove the ability it granted; temporary control effects must restore ownership; and spell-specific effects must obey their stated immunities and targets.
- **Late release-note entries are especially valuable:** the long tail through IW720 and beyond contains concrete edge cases in spell costs, dispel resistance, unit enchantments, heroes under changed control, and battle targeting.

## Interpretation and priority

“Resolved” means the inspected implementation has a matching rule or architecture. “Partial” means the main behavior exists but a material edge case or policy is still incomplete. “Investigate” means the note identifies behavior that should be checked; it does not claim the rewrite has the bug. “Divergence” marks an intentional alternative rule. “Design” marks a balance or compatibility choice rather than a correctness requirement. “UI” marks presentation or interaction work.

**Priority scale:** P1 = crash, memory-safety, corrupted state, or deterministic-state risk; P2 = high-impact rules, combat, movement, spell behavior, or AI/diplomacy correctness; P3 = normal gameplay, economy, balance, and secondary behavior; P4 = UI, presentation, and convenience.

## Release-note assessment

The supplied release notes contain many related references. Rows below group references where they describe one behavior or one connected fix; all listed identifiers are retained.

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 012 | Myrror Node placement incorrectly checks Arcanus land | Plane-specific map-generation validation; verify the generator always queries the plane being populated. | P2 | Investigate |
| 032 | Autosave occurs halfway through end-of-turn processing | Save only at a stable turn boundary; test save/load around all end-turn phases. | P1 | Investigate |
| 040 | Producing a unit in a town already holding nine units | The production path should handle a full stack as an ordinary blocked/complete condition, not an error. | P3 | Investigate |
| 041, 042 | Nature Awareness restores contact with defeated wizards and creates Detect Magic | Spell effects must not mutate diplomacy contact or create unrelated enchantments. | P2 | Investigate |
| 043 | Units with no movement remaining incorrectly exhaust moving units in the stack | Stack movement must be based on units that can still move; verify zero-move companions do not poison the step profile. | P2 | Investigate |
| 044 | Planar Seal blocks targeting the other plane with spells | Distinguish blocking planar travel from spell targeting across planes. | P2 | Investigate |
| 045 | Entangle affects non-corporeal units | Verify target eligibility against movement/physicality traits. | P2 | Investigate |
| 047, 049, 050, 051, 052 | Wizard resource display, scouting level, Summon Demon count, live unit abilities and save modifiers | Presentation should reflect current effective state, not base definitions or maximum values. | P4 | UI |
| 053, 134, 135, 291 | Gift of the Gods and treasure item eligibility, item slot aliasing, and receiving more than six items | Reward generation must be independent of the recipient's crafting books; item identity and inventory transfer must not alias or silently drop items. | P2 | Investigate |
| 056, 057, 062 | Diplomatic gold rewards and spell offers have missing/unimplemented text | Verify implemented offer types have complete presentation and payload data. | P3 | Investigate |
| 058 | Cannot request war while an alliance still needs to be broken | Diplomacy actions should expose only legal next steps. | P2 | Investigate |
| 070, 372, 389, 390 | Treaty-breaking and Hidden Relation update the wrong relation or direction | Keep relation changes attached to the correct pair and direction; test treaty transitions. | P2 | Investigate |
| 075, 077 | AI and human spell/gold treaty offers are not transferred correctly; phantom offers can be received | Offer creation, acceptance and transfer must be one validated transaction. | P1 | Investigate |
| 078, 082, 086, 089, 090, 093, 096, 098, 102, 106 | AI-to-AI diplomacy, AI audience initiation, war declarations, warning text, defeated-wizard casting and relation overflow | Large diplomacy/state-machine cluster. Verify actor/target identity, defeated-player guards, relation bounds and transition-specific messages. | P2 | Investigate |
| 108 | Wind Walking units cannot be attacked by ground units | Wind Walking should affect overland movement, not grant an unintended battle-targeting immunity. | P2 | Investigate |
| 111 | Strategic-combat damage priority ignores value/Defense and has a flawed repeat-hit mechanism | Damage allocation must be deterministic and value-aware without biasing repeat hits; release note admits the 1.50 fix remains incomplete for some unit costs. | P2 | Partial |
| 113, 216, 679, 680, 700 | Healing, natural recovery, Healer ordering, Lion Heart extra hits and Life Steal with irreversible damage | Centralize the distinction between current battle hits, permanent unit hits, extra hits and irreversible damage. | P2 | Investigate |
| 125 | Spell Ward does not counter combat spells | Verify Spell Ward participates in battle spell-countering as described. | P2 | Investigate |
| 127, 172, 503, 731 | Animate Dead/Raise Dead target filtering, dead-unit count limits, display corruption and recalled-unit targeting | Resurrection must select only eligible dead units, exclude transported/recalled units, and handle large lists safely. | P1 | Investigate |
| 129 | Summon Demon exceeds the nine-unit battle limit and can crash above eighteen | Enforce side capacity before creating or inserting the summoned unit. | P1 | Investigate |
| 131 | Doom Bolt graphics misaligned | Presentation-only correction. | P4 | UI |
| 136 | Merging item power does not work | Verify item-granted Merging reaches the same movement and combat rules as innate Merging. | P2 | Investigate |
| 138, 148, 467, 684, 683 | Zero-health units, repeated death, memory corruption and spell/banishment dialog crashes | Treat death as a one-way lifecycle transition; validate all list bounds and UI data after removal or banishment. | P1 | Investigate |
| 139, 140, 144, 145, 146, 152, 153, 154, 155, 156 | Dispel removes granted abilities; Invisibility, undead immunities, Berserk, Black Sleep, Weapon Immunity, special attacks, Immolation, walls and Blur | Verify abilities are derived from active effects, immunity precedence is explicit, and attack modes are distinguished. | P2 | Partial |
| 141, 218, 219, 251, 253, 272, 273, 276, 515, 607 | Chaos Surge, Evil Presence, AI targeting, Miner’s Guild, event generation, volcano minerals, Prosperity and swamp food | Rules and event effects need end-to-end checks against their descriptions and target eligibility. | P3 | Investigate |
| 159, 160 | Diplomatic reaction counts all battle casualties; carried units can flee without their ship | Battle results must aggregate all losses and resolve passengers consistently with ship survival and adjacent land. | P2 | Investigate |
| 170 | Spellbooks from treasure grant the same number/rank of spells as starting books | Reuse one book-grant rule rather than maintaining separate reward logic. | P3 | Investigate |
| 174, 275 | Repeated Warp Creature casts crash | Repeated effects must update a typed state rather than overflow or duplicate invalid modifiers. | P1 | Investigate |
| 175 | Wrack squares damage and can corrupt memory | Damage amount and target iteration must be bounded and use shared damage machinery. | P1 | Investigate |
| 176, 465 | Call Lightning bolt count and sound-disabled crash | Resolve the correct number of strikes independently of optional sound resources. | P1 | Investigate |
| 177 | Call Chaos disintegrates units that resist | A successful resistance check must prevent the spell's destructive effect. | P2 | Investigate |
| 210 | Rebuilding a building in the same turn leaves it nonfunctional | Destruction and reconstruction must refresh the city's derived effects immediately. | P2 | Investigate |
| 220 | Negative growth cannot reduce a city already at maximum population | Population caps must constrain positive growth only; negative growth must still apply. | P2 | Resolved |
| 255, 262 | Confusion does not restore control reliably; AI-controlled confused units do not wander | Temporary control must restore the original owner on expiry/dispel, and random behavior must be implemented for AI units too. | P2 | Investigate |
| 261, 444 | Invulnerability cannot be dispelled in battle or by Disenchant Area | Confirm dispel eligibility is consistent across battle and overland contexts. | P2 | Investigate |
| 264 | Hero reward when attacker already has nine units | Reward delivery should use a safe fallback placement rather than discard a valid Hero. | P2 | Investigate |
| 269 | Spell Binding can fail | If this is a guaranteed effect by rule, remove unintended random failure; otherwise document the intended resistance. | P2 | Investigate |
| 270 | Create Artifact checks the wrong Suppress Magic cost | Reuse the actual item cost and dispel-difficulty calculation. | P2 | Investigate |
| 271 | AI curses uncontacted or defeated players | AI target selection must filter by active, known and legal opponents. | P2 | Investigate |
| 285 | Summon Champion should fall back to a Hero when no Champions remain | Summoning fallback should be explicit and deterministic. | P3 | Investigate |
| 293 | AI equips shields in staff slots | Item-slot compatibility should be enforced by shared equipment validation. | P3 | Investigate |
| 302 | Dispelling Web fails to restore movement type | Movement capabilities granted or removed by spells must be recomputed from active effects. | P2 | Investigate |
| 310 | Lawful wizard razes at Hidden Relation zero | Check strict versus non-strict relation thresholds. | P3 | Investigate |
| 314 | Terror counts Resistance buffs twice | Resistance modifiers must be applied once through the common effective-stat path. | P2 | Investigate |
| 317 | Floating Island receives the wrong realm/name | Correct unit definition and realm metadata; avoid a one-off runtime workaround. | P3 | Investigate |
| 318 | Wraith Form incorrectly protects against Flame Strike, Death Spell and Holy Word | Explicitly list which attack/spell natures each immunity blocks. | P2 | Investigate |
| 323 | Holy Weapon has no effect when cast in combat | Combat-cast enchantments must update the unit's effective weapon nature. | P2 | Investigate |
| 325 | Cancelling Spell Charge leaves a free, uncounted item power | Item editing must be transactional: cancellation restores the original state. | P2 | Investigate |
| 326 | Spell Lock combat behavior and Haste protection | Keep the same lock-first dispel semantics in combat and overland. | P2 | Resolved |
| 328 | AI treats Invulnerability units as invalid Dispel Magic targets | AI target legality should distinguish dispellable enchantments from unrelated control effects. | P2 | Investigate |
| 337, 348 | AI attacks impossible flying targets; ghost central structure blocks attacks | Battle AI must use legal attack targets and ignore non-existent map occupants. | P2 | Investigate |
| 347 | Units continue moving during Time Stop | Time Stop must suspend movement execution, including previously queued orders. | P2 | Investigate |
| 349, 350, 362, 363, 464, 681, 732 | Item IDs above 127, cursor state, dispel messages, wind-walking disband, map display and battle drawing | Mostly UI/serialization issues; validate high IDs and ensure view state follows authoritative state. | P3 | Investigate |
| 351 | AI turn order changed from simultaneous phases to sequential wizard turns | Execute each AI wizard's complete turn before advancing to the next; this is a major state-ordering and parity invariant. | P2 | Investigate |
| 368, 369, 397, 398, 407, 424, 433, 479 | Water survival, movement-type stacking, hostile towers, mixed-speed stacks, flying ships and movement corruption | Movement needs one shared capability model for stack composition, terrain survival, route costs and battle entry. | P2 | Partial |
| 370 | True Light blocks Mass Healing | Check spell compatibility and target filtering for mass effects. | P3 | Investigate |
| 371, 411, 489 | Overland pathfinding cannot enter valid tiles or leave ships | Pathfinding must use the same step legality as actual movement and embarkation. | P2 | Partial |
| 384, 388, 391, 401 | Visible/Starting Relation overflow and incorrect AI-to-AI penalties/hostility | Use bounded relation values and consistently identify the human/AI participants in diplomacy events. | P2 | Investigate |
| 385, 386, 387 | Audience continuation mismatch, Cancel causes diplomacy penalty, threats do not deduct paid gold | Diplomacy menus should be side-effect-free until an action is confirmed, and resource transfers must be symmetric. | P2 | Investigate |
| 416 | AI cancels Spell of Return due to fortress-targeting logic | Spell planning must understand the distinct target and state requirements of Spell of Return. | P2 | Investigate |
| 421 | Artificer halves merchant gold cost incorrectly | Apply discounts once and prevent negative treasury after purchases. | P1 | Investigate |
| 425 | Neutral units do not gain Resistance to All | Effect application should not assume every unit belongs to a wizard. | P3 | Investigate |
| 431 | Dispel Magic always removes Unit Curses | Curse dispels must use the same probability and difficulty calculation as other enchantments. | P2 | Investigate |
| 432 | Crusade grants two levels to health calculation | Separate level bonuses from figure hit-point calculation and apply each once. | P2 | Investigate |
| 440 | Death Immunity does not protect from Terror | Verify Death Immunity covers all death-effect resistance checks intended by the rules. | P2 | Investigate |
| 442, 443, 459, 484 | Difficulty encounter budgets, Runemaster dispel bonus and starting roads | Separate difficulty scaling, retort effects and map setup; test each at the boundary values. | P3 | Investigate |
| 445 | Diagonal combat movement inconsistently costs half a move | Choose one movement-cost rule and apply it consistently; do not introduce a fix that creates other movement errors. | P2 | Investigate |
| 447, 451, 495 | Auto-combat free turn, extra initial end-turn and skipped spellcasting when units immobilized | Battle turn transitions must be driven by battle state, not by whether units can move. | P2 | Investigate |
| 456, 457, 458 | Naval battle walkable tiles, Floating Island size/position and ships in combat | Battle map generation must account for ships, city status, Flight and Wraith Form consistently. | P2 | Investigate |
| 468, 490 | AI storm/stasis spells target random, missing or friendly units | Spell target selection must use validated legal targets, not stale indices. | P1 | Investigate |
| 470 | Illusion Immunity does not allow ranged attacks through Wall of Darkness | Separate visibility/targeting from attack resolution and test the immunity exception. | P2 | Investigate |
| 475 | Human secondary realm incorrectly defaults to Nature | New-game realm selection must preserve the chosen realm set. | P3 | Investigate |
| 476, 477 | Wall Crusher cannot attack wall with defender behind; blood drawn on wall | Wall attacks need their own target/animation path, not ordinary unit-hit handling. | P3 | Investigate |
| 481 | AI remembers enemy details from previous battles | Battle knowledge must be scoped to the current battle; do not leak stale combat observations. | P2 | Investigate |
| 492, 493 | Gaia's Blessing changes Chaos Node terrain and reports nonexistent food | Apply only eligible terrain changes and report actual resource deltas. | P3 | Investigate |
| 496, 498 | Death Immunity fails to prevent raising as undead | Resurrection/undead creation must consult Death Immunity in the shared eligibility rule. | P2 | Investigate |
| 501 | Flying Fortress can still be attacked into for some caster orderings | Fortress protection must not depend on player index or turn order. | P2 | Investigate |
| 502 | AI cannot find spells after banishing human player | Spell discovery must handle the human/AI transition and remaining spell pool. | P3 | Investigate |
| 511 | Engineers build roads slowly or cancel when blocked | Movement, work progress and blockage should be separate states; preserve accumulated progress. | P3 | Investigate |
| 521, 523, 528 | Neutral city/outpost classification, wall display and Surveyor wild-game calculation | Ensure UI and map overlays derive from actual city state and settled catchment. | P3 | Investigate |
| 541, 687 | Wall of Fire, Wall of Shadows, Wall of Stone and Flying Fortress effects/display in Outposts | Treat Outposts as a distinct battle/city context where applicable; test each wall and fortress effect there. | P2 | Investigate |
| 619 | Creature Binding and Possession can be dispelled by their caster | Ownership-changing spells should be dispellable only by an opposing wizard if that is the rule. | P2 | Investigate |
| 634 | Barbarian Swordsmen/Spearmen absent from raider pool | Check encounter composition against race/unit eligibility. | P3 | Investigate |
| 635, 637, 638, 639 | Sailing ships/Catapults ignore Weapon Immunity; wrong Weapon Immunity bonus; Weakness ignores Thrown; Shatter hides base attack | Centralize attack nature and effective stats for all physical attack modes. | P2 | Partial |
| 636 | Oracle overrides Nature's Eye detection range | Detection range should combine building and enchantment effects by explicit precedence/stacking. | P3 | Investigate |
| 651 | AI research bonus calculation wrong | Research selection and research progress must share the same bonus calculation. | P3 | Investigate |
| 655 | Dispel spells affect inactive units | Target queries must exclude dead, recalled and otherwise inactive units. | P1 | Investigate |
| 656 | Earth to Mud affects tiles outside battlefield | Validate target geometry against the battle map bounds. | P1 | Investigate |
| 663 | AI Plane Shift crashes on invisible tile | AI must choose only legal, known targets and handle hidden map cells safely. | P1 | Investigate |
| 666–670 | City income breakdown inconsistencies | Breakdown should be derived from the same calculation as actual income. | P3 | Investigate |
| 674 | Map exploration artifacts | Verify explored/visible state and rendering are not conflated. | P3 | Investigate |
| 682 | Razing from combat scroll gives inconsistent gold/fame | Route both UI paths through one raze transaction. | P3 | Investigate |
| 685, 686, 690 | Battle map trees, rocks and crossroads generated incorrectly | Verify map-feature counts and tile composition against map dimensions. | P3 | Investigate |
| 688 | Unit curses reveal invisible units | Curses should not bypass visibility unless explicitly defined to do so. | P2 | Investigate |
| 689 | Spell action remains enabled for exhausted 40-MP casters | Legal action state must account for remaining mana. | P3 | Investigate |
| 694 | Righteousness items fail against ranged attacks | Apply the protection to all specified attack modes. | P2 | Investigate |
| 695 | Destroyed City Walls remain in battle until next turn | Battle wall state should refresh immediately after destruction. | P3 | Investigate |
| 697 | Wrack limited to 40 figures and can corrupt memory | Iterate safely over the full eligible target set with no fixed-size overflow. | P1 | Investigate |
| 701 | Newly summoned units miss battlefield-wide effects until next turn | Apply persistent battlefield effects when a unit enters battle, not only at initial battle setup. | P2 | Investigate |
| 703 | Spellbook page titles disappear with ordering disabled | UI ordering should not affect title rendering. | P4 | UI |
| 704 | Word of Recall targets Summoning Circle in battle | Restrict the target to valid recall targets and context. | P2 | Investigate |
| 705 | Spell action strips short-range attacks | Selecting an action must not mutate the unit's attack definition. | P2 | Investigate |
| 711 | Unit figures move during spell animations | Animation should not mutate the authoritative unit formation. | P4 | UI |
| 712 | Conquered Oracles do not reveal terrain | Apply building ownership effects on conquest as well as construction. | P3 | Investigate |
| 715, 717 | AI Resist Magic on Wraith Form and Recall Hero at Summoning Circle | AI spell legality must be checked against actual target state and spell-specific constraints. | P2 | Investigate |
| 719, 721–725, 728 | Control-changed Heroes use wrong names, spell/item records, save modifiers, movement and skills | When ownership changes, resolve hero identity and abilities from the hero's stable source record, not the current side's index. | P2 | Investigate |
| 720 | Slider spell mana, casting-cost modifiers, Evil Omens, Spell Charges, fortress mana overflow, and hero channeling edge cases | Centralize spell cost, maximum infusion, counter difficulty and available-mana validation. This is a high-risk cluster with many interacting boundaries. | P2 | Investigate |
| 733 | Treasure claims and Hero summons not randomized correctly | Use the game-owned RNG stream without suppressing legitimate randomization or making reload exploits the primary design constraint. | P1 | Investigate |
| 735 | List-selector spells crash with more than six targets | Use dynamically sized target lists and validate selection indices. | P1 | Investigate |
| 737 | AI Call Chaos crashes and glitches graphics | AI and human casting must share one effect-resolution path; animation must tolerate every outcome. | P1 | Investigate |
| 738, 740, 741, 742 | Dispel resistance differs by context; neutral combat enchantments and battlefield walls have inconsistent dispel behavior | Use one explicit dispel difficulty/ownership model for overland and combat enchantments, with context only where intended. | P2 | Investigate |
| 744, 746, 747, 749 | Haste and ranged magical attacks, hero ammunition, First Strike/Doom Gaze ordering, ranged movement cost and ammo display | Attack sequence, ammunition and movement consumption should be derived from attack mode and figure state. | P2 | Investigate |

## Walk (task 457)

The rows are walked in order against our code, with the release notes of the wiki page
`reference/Unofficial Patch 1.50 ...html` (the numbers there follow the text of each note). Verdicts:
**not present** (we do not have the bug), **fixed** (we had it and fixed it), **UI gap** (a display
difference), **difference** (we do it another way).

### Walked on 10 November 2026: 012 to 052

| Rows | Verdict |
|---|---|
| 012 Myrror Node placement checks Arcanus land | **Not present.** `feature/place.rs` takes each plane's own grid for its nodes. No test of the land (not worth one, the user decided). |
| 032 Autosave halfway through end-of-turn | **Not present.** The client saves only when the master stops at `RoundStart`, "the one moment when no seat is half way through its turn" (`view.rs`). |
| 040 Unit produced in a town holding nine units | **Not present.** `production_blocked` keeps the finished unit waiting with its cost paid; test in `turn/tests.rs`. |
| 041, 042 Nature Awareness: contact with defeated wizards, Detect Magic | **Not present.** `contact_sweep` goes over `living_wizards()`; Detect Magic is its own global effect and nothing couples the two. |
| 043 A unit with no moves exhausts the moving units | **Fixed, with a client change.** Nobody was exhausted, but one spent unit in a selection refused the whole move (`NoMovesLeft`). The client now greys out spent units, marks patrolling units with a P, moves only the units that can move (`units_that_can_move`), and shows a yellow ! beside the boot when only part of the stack can go (`unit_window.rs`, `view.rs`, `pointer.rs`). Plane shift and the Earth Gate still take the whole selection. |
| 044 Planar Seal blocks spell targeting on the other plane | **Not present.** The seal is read in three places only: plane travel (`unit/movement.rs`) and the Plane Shift spell. The wiki page of the spell was checked too: all the rows agree, except that `claim_tower` does not claim a tower while the seal is up (the wiki says towers may still be liberated): **fixed** (10 November 2026): a tower is now claimed under the seal too, only the other plane stays dark (`claim_tower`; test `a_tower_claimed_under_the_planar_seal_shows_no_other_plane`). |
| 045 Entangle affects non-corporeal units | **Not present.** `battle/field.rs` holds only walkers with a body; test `entangle_costs_a_move_and_terror_can_freeze_the_enemy`. |
| 047 Gold and mana covered by the books with more than 11 | **Present.** The shelf has no limit; with 12 books and 99999 mana the last book hides the first digit. **Task 533** (Low). |
| 049 Scouting level shown one too high | **Not present.** The sheet shows the unit's current sight, floored (`unit_sheet.rs`); not compared with the help text. |
| 050 Summon Demon shows the maximum, not what is left | **Not present.** The battle screen reads `casting.demons`, the demons still to summon. |
| 051 Unit view shows current immunities and abilities | **Not present.** The sheet reads `unit_abilities` (items and spells included). Checking it found abilities the sheet never listed (below). |
| 052 Life Steal and Death Touch show the save modifier | **Fixed.** Both show it ("Life Steal -2": the special level, 3 more when an item or spell gave the ability). Death Touch was not listed at all. |

Found on the way, fixed (`unit_sheet.rs`, compared with the original's `UnitView.c`):

- Listed now: Death Touch, Destruction, Doom Damage ("Automatic Damage"), Power Drain, Merging (with the item power's picture), and a hero's abilities (Leadership, Legendary, Blademaster, Armsmaster, Constitution, Might, Arcane Power, Sage, Prayermaster, Agility, with the Super forms, then Lucky, Charmed and Noble) with their value for the hero's level, from `f4x_def::hero`. Heroes got an `armsmaster` formula there; the experience code uses it.
- Numbers after the name, as in the original: Poison Touch, Thrown, Fire and Lightning Breath show their strength; Stoning Touch, the gazes, Life Steal and Death Touch show their penalty to the save.
- Not listed, as in the original: Cavalry (a movement flag that does nothing here), Female, Chaos Channeled (its mutations are listed), Eldritch Weapon (a unit enchantment, listed as a spell), Magical Weapon.
- The rank and experience line moved out of the list to the top right corner of the sheet (picture right-aligned, the text under it), and a hero's list shows eight lines on its first page, in two columns (`divergences.md` 41).

Also settled by the walk (rows further down the table):

- 302 (dispelling Web fails to restore movement type) and 440 (Death Immunity does not protect from Terror): **not present**. A broken or dispelled Web gives flight back (`restore_stats_from`); Terror skips units with Death Immunity (`field.rs`).
- The Web spell itself was checked against the wiki page, apart from the 1.50 rows: **Web now has 12 hit points**; the unit it holds spends its turn striking them and cannot move or attack meanwhile (it counter-attacks as usual). What it takes off: its melee strength, the better of that and a bow, sling or magical ranged attack with a shot left (the 1.31 bug counted missiles only), a thrown weapon, breath or gaze added, Immolation's fire added. A boulder does not count (a catapult stays held until the Web is dispelled; the user agrees). `docs/combat.md` section 8, tests in `battle/tests.rs`. This is independent of the 1.50 rows (it also covers 1.52.03 rows 784 and 785).

### Walked on 14 November 2026: 053, 134, 135, 291 (and 292, 293)

Notes from the wiki page `reference/Unofficial Patch 1.50 ...html`. All six are **not present**; no code change.

| Rows | Verdict |
|---|---|
| 053 Gift of the Gods needed an item the receiver could create | **Not present.** `make_item` (`item/make.rs`) never looks at the wizard's books: they only limit what a wizard may craft (the module's first paragraph; `divergences.md` section 10). The event asks for `Grade::Artifact` and gives the item with `give_item`. |
| 134 Treasure items needed specific books; a prefab item is made much more often | **Not present.** Same function: a lair's treasure, the merchant and the events draw up to fifty of the 250 prefab items (each once a game, the cost between half the value and the value) and roll a random item only when none suits, so the prefab item is the usual result. 134 also has an AI line (no spell charges for a spell the wizard does not know): crafting refuses charges for a spell the wizard does not know or cannot cast in battle (`item/crafting.rs`, test `charges_need_a_known_battle_spell_and_one_to_four_of_them`), for the AI as for a human. |
| 135 The internal 0th item slot made one item appear in several places | **Not present.** An item is an entity with one holder (`HeldBy`: a vault slot, the award list, or a hero's slot, `item/entity.rs`); there is no slot 0 and a move changes the holder, so an item cannot be in two places. |
| 291 More than six items from one combat vanish | **Not present.** `give_item` puts every item in the vault, which has no limit (`VAULT_SLOTS` is the screen's size, 4, and the screen scrolls); test `an_armory_with_many_items_loses_nothing` (ten items waiting, a dead hero's three more all arrive, `lost: 0`). |
| 292 The vault has 8 slots instead of 4 | **Not present.** No limit at all, see 291. |
| 293 The AI puts shields in staff slots | **Not present.** The AI moves items with `Command::MoveItem` (`f4x-ai/src/offer.rs`), which goes through the same slot check as the item screen (`ItemError::WrongSlot`, `item/armory.rs`). |

### Walked on 14 November 2026: 056, 057, 062, 058

| Rows | Verdict |
|---|---|
| 056 The diplomatic "gold reward" event crashes | **Not present.** A gift of gold is a `DiplomacyCommand::GiveGold` that raises `GoldGiven`; the client shows it with `diplomacy.gives_gold` (`diplomacy_screen.rs`). Nothing is left unimplemented. |
| 057 The spell name is missing from an AI's spell offer | **Not present.** A spell offered in a swap (`Exchange`) fills `{give}` and `{get}` with the spell names (`diplomacy_prompt.rs`), a spell given away fills `{spell}` (`diplomacy.gives_spell`). We have no treaty offer with a spell attached: the swap and the gift are proposals of their own. The test `every_kind_of_proposal_asks_a_question_that_the_texts_have` checks each question. |
| 062 The name of the target wizard is missing or wrong in an AI's request to break an alliance | **Not present.** `{third}` is filled from the proposal's own third wizard for `WarOn` and `BreakAllianceWith` (`diplomacy_prompt.rs`). The lists of third wizards come from `check_proposal` for each wizard, not from a shared index (the original's list for Break Alliance holds the wizards that are *not* allied, a bug we do not share). |
| 058 "Declare War on" offered where "Break Alliance with" is the way | **Present, fixed.** `check_proposal` let a wizard be asked to declare war on its own ally (the test even said "allies may still be sent to war"). Now `WarOn(third)` is refused while the wizard asked is allied with `third`; the Break Alliance entry lists that wizard instead, and once the alliance is gone war is allowed. The AI asks through the same check. Test `what_the_treaty_allows_decides_what_can_be_proposed`. |

### Walked on 14 November 2026: 070, 372, 389, 390

| Rows | Verdict |
|---|---|
| 070, 372 Breaking a treaty raises the hidden relation instead of lowering it | **Not present.** Ours lowers it (`break_treaties`, `divergences`: `diplomacy.md` section 10). |
| 389, 390 Hidden relation changes the wrong player | **Present, fixed** (decided with the user). The `Stance` slots were used in two directions: breaking a treaty and asking wrote where the rating of a proposal reads (the slot of the one who asks), a gift wrote the other slot, and a wizard's own decisions (peace offers, warnings, need for war, its dealings with other wizards) read the slot the human's changes never reach. Now `hidden` and the patiences are read and written in one convention (documented on `Stance`): `stance(x, y)` is what `y` feels about `x`. A gift raises the receiver's reaction to the giver where the rating reads it; the AI's own decisions read `stance(other, me)`; and when a pact or alliance is really broken every other wizard's hidden relation to the *breaker* falls by 5 (the original lowers their record of the victim, and also when no treaty existed). Tests `breaking_a_treaty_costs_trust_with_everyone`, `ending_no_treaty_costs_the_breaker_nothing`, `a_spell_given_is_known_and_wins_trust`. Not unified: the mirrored patience copies in `change_relations` (equal in both directions, so no effect for the treaty and exchange patience; the peace patience there is still split by direction). |

### Walked on 14 November 2026: 075, 077, 078, 082, 086, 089, 090, 093, 096, 098, 102, 106

All **not present**; no code change.

| Rows | Verdict |
|---|---|
| 075 Spells offered as a bribe never reach the AI, and not the human for a spell id of 128 and up | **Not present.** We have no bribe attached to a treaty: a gift and a swap are commands of their own. A transfer is an `Outcome::Spell` for any `SpellId` (not a byte) and goes to any wizard, the AI included (`learn_spell`). |
| 077 Offers received that were never made | **Not present.** An answer re-checks the proposal (`check_proposal`, and that a wizard asked for a demand can pay) before anything moves, so an offer that went stale (a spell learned meanwhile, gold spent) transfers nothing; gifts check the giver's gold and spell. |
| 078 Severe bugs in AI-to-AI diplomacy | **Not applicable.** Our AIs deal with each other through the same commands and checks as the human (`negotiate`, `rate`; the original's AI code is not ported, `design.md` section 8). The direction faults of the stance record were the ones that applied (rows 389, 390 above). |
| 082, 098 The AI cannot start a conversation / stays silent instead of warning | **Not present.** `approach_human` (`diplomacy.md` section 8) starts every talk: a proposal waits in a yes-or-no box, a warning is a command that raises an event shown on the map; the chance of a warning is our own heuristic, not the original's. |
| 086 A failed threat shows the generic war text | **Not present.** The war after a threat shows its own text, `diplomacy.threat_war` (`diplomacy_screen.rs`). |
| 089, 090 The AI declares war on the wrong person | **Not present.** The target is chosen as a `PlayerId` from the wizards met, with no treaty and no peace left over (`quarrel`), and passed unchanged to `DeclareWar`; the quarrel is forgotten when that wizard is defeated or the war ends. |
| 093 Warnings about units close to cities use the wrong text group | **Not present.** The trespass warning has its own key, `diplomacy.trespass`. |
| 096 A treaty broken after warnings uses a text that does not say so | **Not present.** `diplomacy.treaty_broken` says it (`diplomacy_text.rs`). |
| 102 Defeated wizards cast spells | **Not present.** A defeated wizard's turn is skipped (`f4x-ai/src/lib.rs`), and a human wizard cannot be defeated and still play. |
| 106 Attacking a lawful wizard's capital overflows the visible relation | **Not present.** Every change of the visible relation is clamped to -100..100 (`change_relations`, `break_treaties`, `declare_war`, gifts, subversion, peace).|

### Walked on 14 November 2026: 108, 111, 113

| Rows | Verdict |
|---|---|
| 108 Wind Walking units cannot be attacked by ground units | **Not present.** Wind Walking sets `flies` only for a stack fighting at sea (`battle/game.rs`, `carried`); a flyer on land is attackable by ground units (they fight it with the reach penalty, `melee`: `reach_blocked`). |
| 111 Strategic-combat damage priority (also 112, 114, 115) | **Not applicable.** We have no separate strategic combat: auto-resolve plays the real battle rules through the battle AI (`design.md`). Rows 112, 114, 115 fall with it. |
| 113 Healing grants extra health (Regeneration, Life Steal, Healing, Mass Healing) | **Not present** for Regeneration, Healing and Mass Healing: `heal_points(.., false)` never adds extra hits and never exceeds what can be healed. Only Life Steal overheals (`heal_overflow`, as the original means it). Whether Life Steal should ignore irreversible damage is row 700. |

| 109 Turn number shown in combat | **Was a UI gap, fixed.** The turn was only a battle-log line. The battle panel now shows "Turn N" (`battle.turn`) between the unit name and the skill/mana/range block (`bar.rs`, `draw_battle_panel`). Position chosen by eye, not checked in the running game. |
| 110 Ammo and moves both shown | **Not checked** in the battle panel (needs a look at the running game). |

### Walked on 14 November 2026: 116 to 123 and 125 (battle spell target filters)

The text of these notes is not in the repository; the verdicts follow the one-line descriptions above and the code (`battle/spell.rs` `check_aim`, `f4x-ai/src/battle/cast.rs`). The battle AI picks only among `legal_battle_casts`, so the game's own refusals apply to it as to a human.

| Rows | Verdict |
|---|---|
| 116 Life Drain target | **Not present.** Refused on a Death creature or one immune to Death (`check_aim`, test `life_drain_takes_hit_points_that_a_hero_keeps_and_death_creatures_are_immune`). The AI gives Life Drain no value (a custom battle effect), so it never casts it: a gap, not the original's bug. |
| 117 Fireball damage preference | **Not present.** A damage cast is judged on each unit by the game's own damage code (`preview_spell_damage`): a kill outranks a wound, and the mana is fed until it kills. |
| 118 Duplicate Haste and Invisibility | **Not present.** A lasting spell on a unit that has it, or has what it gives, is refused (`check_stacking_in_battle`). |
| 119, 120 Shatter restrictions | **Present in the AI, fixed.** Shatter is a cap on attack (`Modify Max`), which the AI read as a blessing, so it was worth nothing on an enemy and never cast. A cap now counts as a curse. |
| 121 Curses on high resistance | **Present in the AI, fixed.** The AI valued a curse the same against any enemy. A curse with a save is now worth its chance to land (`Battle::save_failure_chance`, the check `resist_fails` rolls); a unit with resistance 10 or more is left alone. Test `a_curse_is_worth_less_against_a_unit_that_resists_it_better`. |
| 122 Invalid spell exclusions | **Not present.** The AI casts only legal aims. |
| 123 Possession on a Death-immune unit | **Not present.** A spell of the Death realm is refused on a unit with Death immunity (`check_aim`, the save check). |
| 125 Spell Ward against combat spells | **Not present.** `check_spell` refuses a cast of a warded realm (`ward_stops`), in battle as overland. |

### Walked on 14 November 2026: 124, 126, 127, 129, 131, 136

| Rows | Verdict |
|---|---|
| 124 Dispel target selection, no Disjunction on allies, Spell Lock against Banish and Dispel Evil | **Not present in the game**: a slay skips a unit with Spell Lock (`battle/spell.rs`). **Present in the AI, fixed**: it valued a slay on a locked unit as if it worked; now it does not. The AI casts a dispel only at an enemy unit that has lasting spells (task 600). Not tested on its own. |
| 126 Prefer Counter Magic; idle with unreachable fliers; do not skip caster turns at low mana | **Counter Magic:** was never cast (value 0), now valued while the enemy wizard has mana (task 600). **Low mana:** not present, a caster that cannot pay falls through to the unit orders (`next_command`). **Unreachable fliers:** present, fixed: a unit that cannot hurt any enemy (a walker against fliers, with the default melee rule) walked into them even when its side was outmatched; it now holds its place then (`order.rs` `next_command`, test `a_walker_outmatched_by_fliers_it_cannot_hurt_holds_its_place`). When the sides are even it still advances. |
| 127 Animate and Raise Dead filtering | **Not present.** Only a dead unit of the side that is not a hero, not destroyed outright and, for Raise Dead, not fantastic or undead (`can_animate`, `can_raise`), and only with room on the side (nine units). A recalled unit is not dead. |
| 129 Summon Demon above nine units | **Not present.** `demon_squares` refuses at nine units (`MAX_SIDE_UNITS`); so do the summons and Raise Dead. |
| 131 Doom Bolt graphics | **Not checked** (presentation; needs the running client). |
| 136 Merging item power does not work | **Present, fixed, with new rules** (task 602, `divergences.md` 44). The power gave the carrier nothing (it needs 22 books, so nobody crafts it and finds of the two prefab items are out in the original). It now gives the Merging ability, and Merging is a new rule: a unit sinks into the ground and comes up a turn later. Our finds ignore books (section 10), so the two items can turn up and work. |

Next: row 138 (zero-health units, repeated death, list bounds).

## P2 code review: first pass

This pass checks the current source architecture against the most consequential 1.50 themes. It is not a claim that every issue in the table has a dedicated regression test.

### 1. Movement types, stack movement and pathfinding

**Finding: partially addressed; movement semantics are explicit, but the 1.50 edge cases deserve focused tests.**

The current overland pathfinder builds a set of step profiles for the whole stack and asks the economy layer for the cost of each transition. It also computes hostile squares and guarded sites once per search. This is a much better foundation than independent per-unit movement checks: the route search and actual movement can share stack-level movement legality.

However, the 1.50 notes describe several interacting cases: Non-Corporeal plus Flying over water, Flight and Water Walking causing unintended passenger capacity, Wraith Form item movement, units with no movement remaining, and movement modifiers applied in a different order in battle and overland. A generic “stack can move” result is not enough to prove all these cases correct. The rewrite should preserve the distinction between:
- movement modes a unit itself has;
- terrain it can survive;
- movement allowance and remaining movement;
- transport capacity and units carried by another unit;
- battle movement, which is not identical to overland movement.

The current implementation's find_path uses the stack's StepProfiles and stack_step_cost; hostile squares and guarded sites are avoided except when they are the destination. The naval AI also has an explicit ferry plan with a passenger group, port, sea tile and landing tile. This is a sound structure, but the release-note cases need integration tests that exercise mixed stacks and real movement, not just isolated stat calculations.

Relevant code: [unit/pathing.rs](../crates/f4x-core/src/unit/pathing.rs), [unit/movement.rs](../crates/f4x-core/src/unit/movement.rs), [ai/naval.rs](../crates/f4x-ai/src/naval.rs).

**Recommended tests:** mixed Flying + Non-Corporeal stacks crossing water; ships with Wraith Form/Flight in city and non-city battles; stack containing a zero-movement unit; Flight and Water Walking not creating extra transport slots; movement modifier order parity between battle and overland.

### 2. Combat damage, healing and unit lifecycle

**Finding: the rewrite has shared combat resolution and explicit unit states; edge cases remain high priority.**

Battle units distinguish Active, Dead, Fled and Recalled. Damage is recorded by kind, including ordinary, drained and destructive damage. The battle unit also tracks extra_hits separately: these are temporary overhealing from Life Steal and are deliberately excluded when damage is written back to the real unit. This is the right kind of separation for the 1.50 notes about Life Steal, Regeneration, Mass Healing, irreversible damage and healing extra hits.

The attack resolver is a pure rules function over combatant and defender state. It handles attack modes, immunities, damage to the front figure and overflow to later figures. This reduces the risk of individual spells inventing their own damage arithmetic. The remaining audit priority is to verify that every spell and end-of-turn healing path uses the same underlying health rules and that a unit already marked Dead cannot be killed or processed again before cleanup.

The notes about Wrack's squared damage, figure caps, repeated Warp Creature, dead units dying twice, and Raise/Animate Dead with many targets are especially important memory-safety and lifecycle cases. They should be tested with unusually large stacks and repeated effects, not just ordinary battles.

Relevant code: [battle/mod.rs](../crates/f4x-core/src/battle/mod.rs), [battle/attack.rs](../crates/f4x-core/src/battle/attack.rs), [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs).

**Recommended tests:** healing with extra hits followed by battle write-back; irreversible damage plus Life Steal; repeated healing and repeated Warp Creature; Wrack against more than 40 figures; death procedures called twice before cleanup; Raise/Animate Dead with more than seven eligible and ineligible units mixed together.

### 3. Enchantment dispelling and granted abilities

**Finding: architecture supports removal and restoration, but the full list of 1.50 granted-ability cases needs verification.**

The battle model records combat spells with their caster and preserves a base copy of the unit before the first combat enchantment. This allows dispelling to rebuild the unit from the base plus the enchantments that remain, rather than trying to reverse every stat mutation independently. The shared overland dispel code also records spell ownership and applies Spell Lock before other dispels.

This is a good foundation for the 1.50 fix where dispelling Guardian Wind, Magic Immunity, Immolation, Wraith Form or True Sight must remove the ability granted by that spell. It also helps avoid double-applying modifiers. The audit still needs to confirm each ability-granting spell uses the same reversible representation, including spells cast in battle and spells whose effects influence movement or attack nature.

The notes on Spell Lock, Invulnerability, Spell Ward, Holy Weapon, Web and Creature Binding/Possession should be tested as separate rules: they involve dispel eligibility, ability removal, movement restoration and ownership restoration, which are related but not interchangeable.

Relevant code: [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs), [spell/dispel.rs](../crates/f4x-core/src/spell/dispel.rs), [battle/mod.rs](../crates/f4x-core/src/battle/mod.rs).

### 4. Temporary control and ownership changes

**Finding: explicit source-side state exists; confirm all expiry and dispel paths restore the original owner.**

BattleUnit has separate turned_from and bound_from fields. The former represents temporary enemy control (Confusion); the latter represents control-changing spells such as Possession and Creature Binding. Keeping these separate is important because their duration and dispel behavior differ.

The 1.50 notes call out Confusion, Possession, Creature Binding and control-changed Heroes. Those paths must not infer the original owner from the unit's current side or use the current side to look up hero records, items, abilities or spell charges. The stored original side/source identity should remain authoritative until the effect ends.

Relevant code: [battle/mod.rs](../crates/f4x-core/src/battle/mod.rs), [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs).

**Recommended tests:** caster attempts to dispel their own Possession/Creature Binding; opponent dispels; Confusion ends naturally and by dispel; control-changed Hero casts innate and item spells; hero's original movement and leadership data are restored after control ends.

### 5. AI turn sequencing and battle spell targeting

**Finding: important design invariants; verify end-to-end execution rather than individual helper functions.**

The 1.50 change to sequential AI turns is not merely a presentation detail. It prevents the world from changing between one AI wizard deciding on an action and the same wizard executing it after other AI players have already advanced their own phases. It also makes AI execution easier to compare with the human player's turn model.

Similarly, the notes about AI storm spells targeting random or non-existent units, illegal attacks against flying units, stale battle memory, and Plane Shift targeting invisible tiles all point to a general rule: AI planning may rank candidate actions, but the final command must validate the target against the current game state. A stale target must fail safely rather than mutate the wrong unit.

The battle code exposes legal-command generation and validates commands when applied. Preserve that separation: AI scoring should select from legal actions, and command application should independently reject stale or illegal targets.

Relevant code: [battle/legal.rs](../crates/f4x-core/src/battle/legal.rs), [f4x-ai/src/battle/order.rs](../crates/f4x-ai/src/battle/order.rs), [f4x-ai/src/battle/cast.rs](../crates/f4x-ai/src/battle/cast.rs), [ai-design.md](ai-design.md).

**Recommended tests:** AI spell target becomes invalid between scoring and execution; friendly and inactive targets excluded; flying target rejected by a ground-only attack; battle memory reset between battles; multi-AI turn execution completes one wizard's whole turn before the next wizard begins.

### 6. Spell costs, immunity and dispel calculations

**Finding: shared spell definitions and shared dispel machinery are a strong foundation; slider and context interactions remain open.**

Battle spell definitions are derived from spell effects, and the shared dispel module owns the base formula, realm mastery and Archmage multipliers, Spell Lock difficulty and Counter Magic drain. This is preferable to separate per-spell formulas.

The dense IW720 cluster deserves its own boundary tests. It covers casting-cost reductions, Evil Omens, additional-power sliders, remaining mana, casting skill, spell charges, hero channeling and counter difficulty. These values should be computed once from a canonical spell-cost model and then reused for UI availability, command validation, actual mana deduction and dispel difficulty. Otherwise a spell can appear castable but fail at execution, or the UI and resolver can disagree.

The immunity fixes likewise argue for explicit attack/spell natures rather than broad labels. Weapon Immunity must distinguish melee weapons, thrown attacks, missiles and boulders; Wraith Form must not accidentally block unrelated spell realms; Death Immunity must be checked by Terror and undead-raising effects where specified.

Relevant code: [spell/dispel.rs](../crates/f4x-core/src/spell/dispel.rs), [battle/immunity.rs](../crates/f4x-core/src/battle/immunity.rs), [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs), [crates/f4x-def/content/spells.ron](../content/spells.ron).

## Highest-value follow-ups

1. **Movement matrix tests:** cover Flying, Non-Corporeal, Sailing, Swimming, Flight, Water Walking and Wraith Form in mixed stacks, including zero remaining movement and transport capacity.
2. **Healing and damage lifecycle tests:** extra hits, irreversible damage, large Wrack target sets, repeated death processing and resurrection with mixed eligible/ineligible units.
3. **Effect-removal tests:** every spell-granted ability must disappear when its spell is dispelled; test both battle and overland contexts.
4. **Ownership-change tests:** Confusion, Possession and Creature Binding must restore the original owner and stable Hero identity on every exit path.
5. **AI execution tests:** sequential AI turns, stale-target rejection, and legal target filtering for area spells, storms, Plane Shift and movement.
6. **Spell-cost boundary tests:** exact remaining mana, cost reductions, Evil Omens, slider limits, hero/item charges and dispel difficulty.
7. **Diplomacy transaction tests:** treaty offers, gold/spell transfers, alliance-breaking order, relation bounds, defeated wizards and AI-to-AI events.

## Notes on interpretation

- The release-note text was supplied in the conversation. Issue identifiers are retained as written; related identifiers are grouped where they describe one behavior.
- Some 1.50 notes explicitly say a fix remains incomplete or introduces a new movement trade-off. Those are marked Partial rather than treated as a clean resolution.
- “Resolved” is intentionally conservative. A sound architecture is evidence, but not a substitute for a regression test of the exact reported case.
- The rewrite is not required to reproduce every original implementation detail. The objective is to preserve intended gameplay rules and avoid the reported failure modes, while documenting deliberate divergences.


## AI changes: release-note assessment

The AI notes combine balance tuning, tactical policy and correctness fixes. Some are explicitly reversed by the Community Patch, so they should not automatically become rewrite requirements.

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 036, 168, 169, 231, 232 | Personality changes curse casting; diplomacy reaction weights for curses/global enchantments; avoid recasting active globals | Overland AI classifies curse and global spell categories and checks some existing globals. It lacks a comprehensive personality-driven casting policy and full diplomatic-reaction valuation. | P3 | Partial |
| 059 | Diplomatic refusal flavor text shown more often | Presentation/dialogue frequency. | P4 | UI |
| 066, 088, 076, 079, 080, 082, 089, 090, 091, 099, 100, 101, 252, 278, 298, 299, 375, 402, 403, 453 | AI-to-AI Peace Treaties, per-turn war evaluation, warning escalation, treaty breaking, peace duration/counter, personality and military-strength effects | Rewrite has proposal answering, quarrel decisions, war-on-Mastery behavior and sequential AI turns, but narrower AI-to-AI treaty/war negotiation and warning progression. Peace duration and relation state need symmetric tests. | P2 | Partial |
| 104 | AI casts Spell of Return at lower casting skill threshold | Spell of Return is categorized as recall and considered by the overland caster; exact threshold and target-health policy need comparison. | P3 | Investigate |
| 116, 117, 118, 119, 120, 121, 123, 122 | Battle spell target filters: Life Drain size, Fireball damage preference, duplicate Haste/Invisibility, Shatter restrictions, high-resistance curses, Death-Immune Possession and invalid spell exclusions | Battle AI values damage/healing and scores legal aims, but generic effect scoring does not establish all these spell-specific filters. | P2 | Partial |
| 124, 328, 346, 259, 260 | Dispel target selection; do not Disjunction allies; respect Spell Lock for Banish/Dispel Evil | Shared legality and ownership-aware dispel rules are a good base. AI valuation still needs ally ownership, Spell Lock protection and actual dispel value. | P2 | Partial |
| 126, 130, 132 | Prefer Counter Magic; idle with unreachable flying units when outmatched; don't skip caster turns at low mana | Battle AI scores legal casts against unit value and mana. It needs explicit defensive reserve preference, tactical hold behavior and low-mana caster handling. | P2 | Partial |
| 134, 178, 294 | Do not create charges for unknown spells; allow predefined artifacts; store/reorganize AI hero equipment | Equipment handling exists, but predefined artifact selection, vault storage and all crafting constraints are not established by the reviewed path. | P3 | Partial |
| 149 | Push least valuable unit off a tile when AI exceeds nine units | Core capacity validation should prevent invalid stacks; AI recovery should choose the least valuable unit and a legal adjacent destination. | P2 | Investigate |
| 151, 158, 164, 165, 208, 316 | Better unit valuation, target selection, passed-unit accounting and outnumbered threshold | Battle AI has a value function and damage-based spell scoring; strategic AI has unit-strength estimates. Mixed stacks, passed units and defensive walls need tests. | P2 | Partial |
| 166 | Spell value in diplomatic trades | AI can answer proposals and exchange spells, but relative spell valuation needs explicit tests and tuning. | P3 | Investigate |
| 171 | AI Spell Binding | Spell Binding is classified with Disjunction, but full use as a targeted global-spell theft action needs validation. | P2 | Partial |
| 179 | AI can cast multiple overland spells per turn when skill allows | Overland caster uses a prioritized selection loop; verify it continues until skill, mana or legal options are exhausted. | P2 | Investigate |
| 181, 182, 183, 184, 185–207, 257, 258, 289, 355, 382 | Broad tactical spell preference tuning; situation-dependent spell groups; Call Lightning/Wrack dispel preference; Web and Mind Storm matchup changes; Guardian Wind only against missiles | Battle AI has a generic effect scorer but lacks many matchup-specific weights and field-spell dispel valuation. This is a broad tuning surface. | P2 | Partial |
| 187, 189 | Guardian Wind and Mind Storm matchup-specific priorities | Generic scoring does not fully express “only if missile threat exists” or the desired equal-strength Mind Storm condition. | P2 | Investigate |
| 217, 227, 248, 435 | Disband obsolete units only near cap; protect heroes/valuable units; never disband Stasis units or heroes due to gold shortage | A global-cap-aware, hero-safe disband policy was not found in the reviewed strategic AI flow. | P2 | Investigate |
| 224, 672, 243, 245 | Smarter power-base distribution and checking town defense needs every turn | Power allocation is an explicit AI concern and threatened-city tracking exists. Exact adaptive allocation and continuous garrison reassessment need verification. | P2 | Partial |
| 229, 230, 234, 238, 239 | Spell-category selection, less redundant dispelling, Spell Binding as dispel, high-priority globals and correct wizard-curse execution | Overland AI has explicit Disenchant, Disjunction, WorldSpell and WizardCurse categories. Duplicate-dispel coordination and effect-specific priorities remain open. | P2 | Partial |
| 233 | AI does not Move Fortress | Verify Move Fortress remains excluded as a useful AI spell; the AI should not choose it without understanding racial unrest and fortress defense. | P3 | Investigate |
| 235, 236, 237 | Flying Fortress city target, Spell Ward realm choice, Time Stop upkeep and Suppress Magic persistence | Requires target-aware city selection and upkeep-aware global-spell policy. Time Stop is considered; interaction with fantastic-unit upkeep and Suppress Magic needs tests. | P2 | Partial |
| 244, 249, 333, 478, 494 | Gather offensive stacks without stripping city/node garrisons; correct unit value; send only enough units to stay under stack cap | Military AI has campaign gathering and garrison logic, but source-city/node defense and assembled-stack reachability/capacity need integration tests. | P2 | Partial |
| 246 | Correct human-unit strength; war targets weighted far above non-war targets; peaceful/lawful wizards avoid cities without declared war | Shared strength and formal-war checks align with the intent. Verify no human-only strength discount and no non-war city targeting by peaceful/lawful personalities. | P2 | Partial |
| 250 | Settlers build whenever able, even if another player's unit is nearby | Site choice and command legality are separate. Whether the planner waits on nearby units needs an explicit test. | P2 | Investigate |
| 254, 263, 399, 408, 471 | Tactical fallback movement, searching for invisible enemies, avoiding invalid melee attacks and recognizing attack modes | Battle AI has target and movement scoring, but not all fallback behaviors or attack-mode preferences are established. | P2 | Partial |
| 265, 266, 274, 290, 319, 320, 330, 332, 338, 358, 392, 393, 400 | AI ship transport fixes | Dedicated naval plan, ferry state and passenger group exist. Many embarkation, loading, landing and retry edge cases still need end-to-end tests. | P2 | Partial |
| 267, 268 | Time Stop power allocation and alchemy of excess gold | Strategic resource-policy choices; matching thresholds and Time Stop-specific allocation were not established. | P3 | Investigate |
| 279, 280, 286, 441, 510 | City/terrain target selection for curses, Earthquake, Corruption, Raise Volcano and Warp Node | AI has curse categories and target scoring, but spell-specific terrain/wall/ward constraints and finite priorities need tests. | P2 | Partial |
| 288 | Do not disband Settlers or Engineers after turn 200 | No matching disband pass was found; if added, workers and expansion units must be protected. | P3 | Investigate |
| 295, 296, 297, 300, 301, 322, 331, 339, 361, 414, 462 | More frequent and smarter continent invasion selection | Naval AI plans landings against known enemy cities and towers, but target choice and retry policy remain constrained. | P2 | Partial |
| 303, 312, 313, 327, 429 | Better overland and combat buff targets; account for buff effects, existing enchantments, realm threat and heroes | Overland buff scoring is generic and battle buffs use unit worth. Duplicate effects and target-specific realm defenses need more explicit policy. | P2 | Partial |
| 308, 357, 367, 373, 394, 396, 487 | Avoid enemy-occupied shortcuts; reconsider stale GoTo orders and recalculate paths | Existing path planning and stale-target recovery exist in parts of the AI, but general re-evaluation of old orders should be tested across roles. | P2 | Partial |
| 321, 335 | Build more ships based on landmass size; prioritize Shipwright's Guild on small landmasses | Naval production has ship/building prerequisites, but no comprehensive landmass-size production policy was established. | P3 | Investigate |
| 334, 359, 415, 413, 417, 418, 420, 478, 486 | AI ships, flying and swimming units attack over sea and nearby land | Naval plans and movement-aware reachability exist; actual attack execution across sea and mixed movement stacks needs integration tests. | P2 | Partial |
| 340, 380, 341, 342, 343 | Use Planar Travel, Plane Shift and Astral Gate | AI has planar scouting and tower-crossing state; spell-driven Astral Gate transport and general Plane Shift use are not established by that alone. | P2 | Partial |
| 344, 395, 446, 466, 509 | Use flying/swimming/Wind Walking units to carry a whole stack over sea | Verify transport-capable movement is recognized by stack pathfinding without creating unintended passenger capacity. | P2 | Partial |
| 346 | Do not Banish or Dispel Evil units protected by Spell Lock | Shared Spell Lock rules exist; AI scoring should reject targets where the spell cannot affect them. | P2 | Investigate |
| 360 | Do not recruit units already marching toward a target into a second stack | AI tracks marching units and a campaign target; confirm every gathering path excludes units assigned elsewhere. | P2 | Partial |
| 376 | Confused units changing sides should not make AI panic | Battle units track control changes separately; strength and flee decisions must use current active-side membership without double-counting. | P2 | Investigate |
| 383 | Do not cast Prayer when High Prayer is active | Avoid redundant buffs by checking existing effects at target and stack scope. | P3 | Investigate |
| 426, 430 | Check Crack's Call availability when choosing Web | Spell valuation should account for the rest of the spell hand and follow-up tactics, not only Web's immediate value. | P3 | Investigate |
| 448 | Better Settler target selection | Current site scorer values terrain, river access and distance, but not every strategic/resource factor; overlaps the earlier P2 site-choice review. | P2 | Partial |
| 463 | AI recognizes Wall Crusher | Battle AI has explicit wall-targeting behavior; confirm Wall Crusher works when a defender is behind the wall. | P2 | Investigate |
| 469, 485 | Better overland spell targeting against enemy stacks | Overland caster has target categories and legal targets, but stack-curse/buff scoring needs dedicated tests. | P2 | Partial |
| 488 | Multiple stacks converge on one target for concentrated assault | Strategic AI tracks a campaign target and marching roster; coordinated independent stacks need verification. | P2 | Investigate |
| 504 | Do not attack fortress of a banished wizard | Rival-city targeting must filter defeated/banished owners at planning and execution. | P2 | Investigate |
| 643, 706, 714, 716 | Avoid ineffective Holy Word/Righteousness; Psionic Blast versus Illusion Immunity; Earth to Mud obstacle placement; avoid redundant Righteousness | These are effect-specific tactical rules; generic spell scoring does not demonstrate the matchup and terrain-state preferences. | P2 | Investigate |

## AI code review: first pass

### 1. Personality and objective generation

**Finding: partial; the rewrite supports the full Pragmatist objective and retort weighting, but only the primary realm influences initial personality/objective weights.**

The core defines all five objectives, including Pragmatist, and exposes weighted selection from realm books and retorts. The AI crate separately gives each objective a strategic profile: city targets, army size, city focus, power-split shift, research preferences and spell-casting boosts. This is a sound separation between wizard generation and how the AI acts on its chosen objective.

However, both personality_weights and objective_weights select a single primary realm and use only that row of the weight table. They do not incorporate the secondary realm described by the 1.50 notes. This is a concrete compatibility gap, though a balance choice rather than a core rules defect. The guaranteed Uncommon summon and Sss'ra/Myrran changes are explicitly reversed in the Community Patch and should not be treated as requirements.

Relevant code: [wizard/objective.rs](../crates/f4x-core/src/wizard/objective.rs), [ai/objective.rs](../crates/f4x-ai/src/objective.rs).

### 2. Diplomacy and war planning

**Finding: partial; the AI has meaningful diplomacy, but its scope is narrower than the 1.50 behavior.**

The strategic AI answers proposals, rates treaty/peace/war requests, can initiate quarrels, and can react to a rival casting the Spell of Mastery. It also has a distinct approach-to-human step for warnings and requests. play_round executes each AI's complete play_turn in sequence before ending the round, so one AI does not decide and then wait while other AI wizards execute separate global phases.

The current design does not yet reproduce the full patch's diplomacy behavior: repeated warning escalation, AI-to-AI peace treaty negotiation, time-based war-on-ally's-enemy behavior, the full set of relation/hostility counters and all personality/military-strength adjustments. The diplomacy documentation has a historical status paragraph that predates some of the current implementation; the source code is the authority.

Relevant code: [ai/diplomacy.rs](../crates/f4x-ai/src/diplomacy.rs), [diplomacy.md](diplomacy.md), [ai/lib.rs](../crates/f4x-ai/src/lib.rs).

**Recommended tests:** three-AI peace treaty lifecycle; AI allied to A declaring war on A's enemy B only after the intended delay; repeated warning escalation; no hostile target selection by Peaceful/Lawful personalities without formal war; banished wizards excluded from every diplomacy and military target path.

### 3. Battle spell valuation and tactical behavior

**Finding: partial; the battle AI has a real spell scorer, but it remains generic and lacks much of the patch's matchup-specific policy.**

The battle brain iterates legal battle casts and aims, values effects against friendly and enemy unit worth, discounts expected resistible damage, values healing based on missing hit points, and compares value against mana cost. This is substantially more capable than a fixed priority list and avoids attempting casts that are not offered by the legal-cast query.

Many 1.50 notes describe conditional value: do not cast Guardian Wind without missile threats; avoid Life Drain on large targets; avoid duplicate Haste/Invisibility; avoid Possession against Death Immunity; use Web against flyers or when the battle is favorable; use Mind Storm in an even matchup; avoid targets protected by Spell Lock; and dispel field effects such as Call Lightning/Wrack when useful. A generic scorer cannot infer all these conditions from coarse effect value. It needs spell-rule metadata or explicit named tactical evaluators.

Relevant code: [f4x-ai/src/battle/cast.rs](../crates/f4x-ai/src/battle/cast.rs), [f4x-ai/src/battle/order.rs](../crates/f4x-ai/src/battle/order.rs).

**Recommended tests:** matchup matrix for the named spells; no redundant buffs; legal target filtering after Spell Lock and immunity; field-dispel scoring for Call Lightning/Wrack; tactical behavior when outmatched and when no attack target is reachable.

### 4. Strategic military, invasion and transport

**Finding: partial; architecture has campaigns, naval plans and ferries, but many 1.50 refinements remain.**

The strategic AI tracks a campaign target, marching units, unreachable cities and a dedicated naval plan/ferry with ship and passenger identities. The naval planner finds a port, landing square and sea square, checks ship capacity, gathers a minimum landing party, and can wait for a stronger force before sailing. The AI also tracks settlers, scouts, planar scouts and raiding groups separately.

This is a good foundation for the patch's transport and invasion work. Remaining concerns include preserving city/node garrisons, not gathering units already assigned elsewhere, coordinating multiple stacks, re-evaluating stale destinations, choosing invasion continents more intelligently, and using flying/swimming/Wind Walking units to reach targets over water. Test the complete campaign rather than only the ferry planner.

Relevant code: [ai/military.rs](../crates/f4x-ai/src/military.rs), [ai/naval.rs](../crates/f4x-ai/src/naval.rs), [ai/scout.rs](../crates/f4x-ai/src/scout.rs), [ai/colony.rs](../crates/f4x-ai/src/colony.rs).

**Recommended tests:** blocked and reopened sea routes; partially loaded ships; ship lost before landing; city/node garrison preservation; multiple campaigns converging on one target; mixed movement-mode stacks; attacks across water without unreachable units.

### 5. Resource policy, spell selection and disbanding

**Finding: partial; objective-specific strategy and spell categories exist, but several high-level policy choices remain absent or coarse.**

Objective traits influence city count, army size, city focus, power allocation and preferred spell kinds. The overland caster classifies spells into Summon, UnitBuff, CityBuff, GlobalBuff, Curse, Disenchant, Disjunction, WorldSpell and Stasis categories. It also checks some target conditions and avoids certain redundant global spells.

The 1.50 notes go further: power allocation changes under Time Stop, alchemy converts excess gold, the AI avoids redundant Prayer/High Prayer, and disbanding protects heroes, settlers, engineers, node guards and strategically important fantastic units. Do not add a blanket disband mechanic without a clear global-cap need and a value model that protects strategic assets.

Relevant code: [ai/objective.rs](../crates/f4x-ai/src/objective.rs), [ai/wizardry.rs](../crates/f4x-ai/src/wizardry.rs), [ai/shortfall.rs](../crates/f4x-ai/src/shortfall.rs).

## Additional AI follow-ups

1. Add secondary-realm contributions to personality/objective weighting behind a documented balance rule; keep Community Patch reversals configurable rather than silently adopting them.
2. Expand diplomacy tests to cover AI-to-AI peace, warning escalation, ally-of-enemy war decisions, relation/hostility decay and banished-player filtering.
3. Add tactical spell evaluators for matchup-sensitive effects, duplicate buffs, Spell Lock, immunity, field dispels and battle state.
4. Test naval campaigns end to end, especially garrison preservation, mixed movement types, partial loads, stale targets and coordinated multi-stack assaults.
5. Add strategic asset-protection tests before implementing disbanding or resource-shortfall behavior.
6. Keep spell-category classification data-driven where possible, and use the planned stable string identifiers for reusable custom spell rules rather than coupling AI policy to numeric spell IDs.


## Gameplay and interface changes: release-note assessment

This batch mixes balance changes, map-generation rules, diplomacy, strategic combat and UI work. Community Patch reversals are recorded as historical alternatives, not automatic requirements for the rewrite.

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 003, 014, 547 | Tiny land setting, reduced land on other sizes, adjacent encounter zones and 200-tile continent cap | Add named size presets and explicit land, continent and encounter-spacing constraints. Current GenParams has dimensions, land percentage and feature counts, but not these original-style presets/caps. | P2 | Partial |
| 004 | Modded wizards with more than two book types display correctly | Wizard presentation must support arbitrary legal book combinations rather than assume the standard layout. | P3 | Investigate |
| 009 | Tower placement near water, minimum distance and node spacing | Keep water adjacency, tower spacing and node exclusion as separate placement predicates. Numeric spacing changes are balance choices. | P2 | Partial |
| 010 | Lower maximum population requirement for starting cities | Use bounded suitability scoring and graceful fallback so generation does not fail when few ideal sites exist. | P2 | Investigate |
| 046, 054 | Larger alchemy conversion; better replacement rewards when books/retorts cannot be awarded | Avoid arbitrary conversion caps where resource types permit larger values; reward fallback should preserve expected value when a category is exhausted. | P3 | Investigate |
| 060–074, 094 | Threat formula, end-turn relation shifts, refused alliance-break request, treaty proposals/duration, reactions, hidden relation, tribute and trade patience | Diplomacy needs explicit pair-scoped visible/hidden relation state and transition-specific formulas. Test human-AI and AI-AI paths separately. | P2 | Partial |
| 063, 092, 103, 281, 282, 449 | Alliance refusal, movement through allied territory, reactions to defeated wizards, ally war requests and peace blocking war | Separate proposal refusal, relation penalty, treaty retention, movement permission and war legality. A peace treaty must block Declare War On; refusal must not silently break an alliance. | P2 | Partial |
| 105, 107 | Banishment no longer transfers mana; razing an Outpost has city-conquest diplomacy effects | Defeat should not transfer treasury without an explicit rule. Outpost destruction should use the appropriate shared hostile-action consequences. | P2 | Investigate |
| 109, 110, 142, 284, 292, 304–307, 305, 306, 352, 412, 460, 680, 688, 692, 693, 696 | Combat turn/stat display, hero spells, vault slots, attack-specific hit bonuses, movement/upkeep, weapon abilities, bonus hits, curse animations, tower ownership and combat modifiers/attack types | Mostly UI. Derive display from authoritative effective stats and active effects, not separate UI calculations. | P4 | UI |
| 112, 114, 115, 309, 381 | Strategic-combat damage weighted by unit cost; improved unit valuation; protect heroes; raise undead for winner | Auto-resolve is a separate algorithm from tactical AI valuation. Define damage allocation, per-unit caps, deterministic tie-breaking and post-battle undead eligibility. | P2 | Partial |
| 149 | Human-produced unit pushed to adjacent tile when stack is full | Preserve the new unit and find a legal adjacent tile; do not evict an existing unit based on maintenance. | P2 | Investigate |
| 162 | Looted city gold based on production; avoids zero-population crash | Calculate loot from a valid city-economy snapshot and test zero-population/razed-city cases. | P2 | Investigate |
| 167, 173 | Spell of Mastery excluded from captured fortresses/trade; Animate Dead cannot retain enemy units | Unique victory spells need acquisition restrictions. Temporary battle resurrection must not create permanent ownership of enemy casualties. | P2 | Investigate |
| 212, 213 | Grand Vizier no longer changes unit production; excess population carries over | Advisor automation should respect player production orders. Growth should subtract the threshold and preserve its remainder. | P3 | Design |
| 221, 222, 428 | Astrologer/Historian ratings and graphs; revised Power Production and Spell Power definitions | Give metrics documented semantics and aggregate the correct quantities; skill/research is not power production. | P4 | UI |
| 283 | Rampaging monsters can destroy capital and eliminate owner | Capital destruction needs an explicit defeat transition covering remaining cities, units, diplomacy, save/load and victory checks. | P1 | Investigate |
| 329, 507 | Rare/Very Rare treasure spell rewards exclude Arcane | Filter eligible spell rarity before selecting a realm to avoid wasting a reward. | P3 | Investigate |
| 345 | Units crossing a transport tile do not accidentally enter patrol | Transport presence must not interrupt an existing movement order unless embarkation is explicitly commanded. | P2 | Investigate |
| 364, 434 | Raider spawning/option; heroes protected from low-gold desertion | Document raider composition and setting behavior. Hero-safe desertion is a design choice; the note itself questions whether the change is needed. | P3 | Design |
| 473, 499, 518, 519, 555 | Unit cap raised to 2,000 | A cap affects allocation, serialization, selection, battle rosters, pathfinding and UI. The DOS conventional-memory cost is historical, not a Rust target constraint. | P2 | Design |
| 474 | Wizard names up to 16 characters | Apply one documented name limit consistently across validation, UI and saves, or support longer UTF-8 names safely. | P3 | Investigate |
| 480 | Remove inland lakes and bays with diagonal entries | Define map topology and navigable/open water consistently for generation and rendering. | P3 | Investigate |
| 508 | Backup Saves setting; Node Owners always displayed | Backup rotation belongs in persistence/host code at a stable turn boundary; node ownership visibility is presentation policy. | P4 | UI |
| 512 | Max-population city switches from Housing to Trade Goods | End-turn production automation should not waste production on a completed housing target. | P3 | Investigate |
| 677, 678 | Correct spell error messages when there are no valid living targets | Validation and error selection should share the same live-target query after casualties and filtering. | P3 | Investigate |
| 691, 697, 720, 729, 730, 737, 741 | Combat terrain, Wrack animation/sound, additional-power slider, spell-target highlighting/responsiveness, Vortex placement, Call Chaos graphics/timing and Warp Node dispel message | Mostly presentation/input feedback, but target highlighting must use the actual aim validator. Animation must not freeze simulation or depend on sound being enabled. | P3 | Partial |
| 739 | Replace five-run LFSR with full-cycle RNG | Document period and reproducibility. Keep gameplay randomness seeded and independent from cosmetic animation. | P2 | Investigate |


## Gameplay and interface code review: first pass

### 1. Map generation and encounter rewards

**Finding: partial.** The current generator is deterministic and feature placement is data-driven. GenParams exposes dimensions, land percentage, tower count, strong/weak encounter counts, difficulty, lair percentages and per-plane node/mineral settings; counts can scale with map area. It does not yet express all the original's named land-size presets, continent cap, encounter adjacency, tower spacing/water rules or starting-city suitability policy as first-class settings.

Keep encounter creature budgets separate from treasure budgets and reward selection. That allows Community Patch balance reversals to be represented without rewriting generation. Model map constraints as explicit parameters or named algorithms, not scattered constants.

Relevant code/design: [mapgen.rs](../crates/f4x-newgame/src/mapgen.rs), [map-layers.md](map-layers.md), [game-settings.md](game-settings.md).

Tests: fixed-seed generation for each size preset; land/continent caps; tower/node/shore spacing; enough starting-city sites on low-land maps; encounter adjacency; treasure category exhaustion and difficulty scaling.

### 2. Diplomacy and treaty movement

**Finding: partial.** The AI has a threat modifier based on relative army strength and proposal rating based on relation, patience, personality, proposal type and alliance state. That is a useful foundation, but it does not establish all end-turn relation shifts, visible/hidden relation changes, tribute/trade effects, ally war requests, outpost razing, banishment reactions or allied-territory movement.

The core diplomacy layer should own treaty legality and relation transitions; the AI should decide which proposals to make or accept. This prevents UI or AI code from bypassing rules such as peace blocking war declarations.

Relevant code: [ai/diplomacy.rs](../crates/f4x-ai/src/diplomacy.rs), [diplomacy.md](diplomacy.md).

Tests: pair-scoped bounded relation changes; refused alliance-break request retains treaty and applies penalty; peace blocks Declare War On; tribute/trade update relations as specified; outpost razing has intended hostile consequences; ally movement permission is enforced by every movement path; defeated wizard reactions differ for friends and enemies.

### 3. Strategic combat and unit valuation

**Finding: partial.** The battle brain uses unit worth to evaluate tactical spells and actions. That is not the same as a strategic-combat resolver that distributes damage by unit cost. Keep auto-resolve independent from tactical AI valuation: calculate offensive/defensive/ranged potential, resolve damage deterministically, allocate it under explicit weighting and hit-point constraints, then apply survivor effects such as undead creation.

The stated outcome—more survivors but more wounded—is a balance consequence, not a hardcoded objective. Test mixed-cost stacks, heroes, one-figure units, overkill, ties and exceptional/zero-cost units.

Relevant code: [f4x-ai/src/battle/cast.rs](../crates/f4x-ai/src/battle/cast.rs), [f4x-ai/src/battle/order.rs](../crates/f4x-ai/src/battle/order.rs). A separate strategic-combat resolver was not established by this narrow review.

### 4. UI, unit information and combat feedback

**Finding: mostly UI.** Unit and combat views should expose effective movement, upkeep, hero spells, weapon abilities, melee/ranged To Hit modifiers, To Block, thrown/breath attacks, resistance/holy bonus levels, bonus hits and active curses. Also account for vault capacity, graph definitions, tower ownership and combat turn count.

Prefer a shared effective-stat/active-effect query over UI-specific calculations. Graph metrics should have one documented definition so the Astrologer and Historian do not reinterpret state independently. The existing unit-sheet work is a foundation, but the exact 1.50 field list needs a direct UI audit before marking it complete.

### 5. Persistence, capacity and randomness

**Finding: design implications.** A 2,000-unit cap affects saves, iteration, selection, combat and AI, not just one constant. Backup saves belong in persistence/host code at a stable completed-turn boundary. The RNG needs a documented reproducibility/period contract and should be isolated from cosmetic animation. The original's conventional-memory cost is not a reason to constrain the Rust rewrite.

Tests: save/load at maximum unit count; backup rotation and interrupted saves; long names through UI and serialization; reproducible RNG unaffected by sound/animation settings.

## Additional gameplay and interface follow-ups

1. Add explicit map-size presets and constraints for Tiny, continent size, encounter spacing, tower placement and starting-city suitability.
2. Separate encounter budgets, treasure budgets and reward-category selection; make Community Patch reversals independently configurable.
3. Add diplomacy transition tests for treaty duration, visible/hidden relation, tribute/trade, war requests, outpost destruction, banishment reactions and ally-territory movement.
4. Design and test strategic-combat damage allocation as its own deterministic resolver, separate from tactical AI valuation.
5. Audit unit-sheet and combat-display fields against effective stats, including negative and attack-mode-specific modifiers.
6. Test the maximum unit count across saves, UI, AI and combat; treat DOS memory notes as historical context.
7. Keep backup saves in persistence/host code and RNG state deterministic and independent from presentation.
8. Confirm the truncated final keyboard note before considering this batch complete.



## Optional

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 027, 035 | Starting Relations formula and diplomacy personality modifier | Starting relation formula became optional in the Community Patch; treat as configurable balance. Current personality diplomacy modifier is a simple fixed temper contribution, not the full revised formula. | P3 | Design |

### Optional 1.50 fixes: separate recommendations

These changes are supplied as optional patch files and are not pre-applied in 1.50. They are assessed separately from the main release-note audit: **Make default** means adopt as the rewrite's normal behavior; **Optional setting** means expose as a ruleset/game setting because it changes balance or player expectations; **Skip** means do not reproduce it as a general rule, or defer it because its dependency/benefit is too narrow.

| Patch reference | Optional change | Recommendation | Rationale |
|---|---|---|---|
| I001, 324 | Prevent slider-based spells from being enchanted into items, avoiding the maximum-slider-for-free exploit | Make default | Closes an item-creation exploit rather than changing the intended value of ordinary spells. Enforce the restriction in shared item-enchantment validation. |
| 005, 055 | Allow Life and Death book picks at the same time | Optional setting | This expands wizard-building combinations and changes the balance/identity of realm restrictions. Keep it available to custom rulesets rather than silently making it universal. |
| 006 | Reduce Myrran's cost to two picks | Optional setting | A direct wizard-creation balance change. Preserve the chosen baseline and let a ruleset select the cheaper cost. |
| 026 | Generate fewer neutral towns | Optional setting | Changes map density, expansion pace and early-game competition; this is a world-generation balance preference. |
| 030 | Allow Peaceful Death wizards and, at low chance, Maniacal Life wizards | Optional setting | Alters personality/realm compatibility and wizard-generation probabilities. Treat the combinations and their chance as ruleset data. |
| 033 | Halve the maximum combat duration | Optional setting | Changes the battle time limit and can affect long defensive or stalemate battles. Make the cap configurable rather than imposing a different limit. |
| 034 | Fortress-combat Lightning Bolts hit enemies in proportion to casting skill | Make default | This makes the fortress effect scale with the caster's skill instead of behaving as a flat or mis-scaled effect. Model it as a deterministic combat rule. |
| 037 | Make the unit-table maintenance-cost field functional | Make default | The field should be authoritative when supplied by data. This is important for modding and should not require a special compatibility switch. |
| 038 | Make unit buy cost always twice its production cost | Optional setting | A global economy/balance adjustment. Keep the multiplier in ruleset data so mods can choose the intended purchase economy. |
| 039 | Allow towns to display up to 16 buildings in the production list | Make default | A capacity/UI limitation, especially relevant when mods extend the building tree. Size the list from supported content rather than the original fixed limit. |
| 133 | Do not double jewelry mana cost for stat-increasing abilities | Make default | Removes a special-case crafting surcharge that penalizes these abilities; implement the intended cost formula consistently. |
| 147 | Do not charge fantastic undead creatures increased maintenance | Optional setting | This changes the upkeep balance of a broad unit category. Keep the policy configurable by ruleset rather than assuming one economic model. |
| 157 | Make ranged attacks consume all remaining movement instead of a fixed 10 movement points | Make default | A clear action-cost rule that prevents ranged attacks from leaving inconsistent movement allowances. Represent the cost as an explicit attack action rule. |
| 211 | Carry unused production into the next project | Optional setting | Production carryover materially changes city-economy efficiency and can interact with project switching. Offer it as a ruleset option, with tests against repeated switching exploits. |
| 215 | Allow Death units to heal naturally, except undead, zombies and skeletons | Make default | Gives living Death-realm units ordinary recovery while preserving the stated exceptions. Base the rule on unit traits, not realm ownership alone. |
| 277 | Allow Raise Volcano to target Hills and Mountains | Make default | Broadens the spell to the terrain types named by its effect and avoids an unnecessarily narrow target filter. Keep target legality in spell data/rules. |
| 427 | Allow production lists to display more than 10 unit types | Make default | Removes a hard UI/content ceiling that can hide valid unit choices, particularly in modded races. |
| 436 | Remove food maintenance from Ships and Catapults | Optional setting | The note emphasizes AI benefits, but this is still an economy rule affecting every player. Make the upkeep policy configurable and verify AI food planning with either setting. |
| 497 | Allow Elementals and Phantoms to be raised as undead | Make default | This is a spell-target eligibility correction. Express it through explicit resurrection/undead-creation traits and immunity checks, not a hard-coded list where possible. |
| 582 | Interrupt Spell of Mastery research when a spellbook is found in treasure; requires 055 | Skip | This is a narrow interaction tied to the optional Life-and-Death book-pick change, and its gameplay rationale is unclear. Do not add a special interruption rule unless reproducing that exact optional wizard-book behavior becomes a compatibility goal. |
| 650 | Slightly improve readability of the researched-spell text in the Apprentice screen (F3) | Make default | A presentation improvement with no rules or balance downside. |
| 708 | Give Flying movement precedence over Non-Corporeal movement as the stack's default movement type, matching v1.31 | Optional setting | This changes movement-type precedence and can affect mixed stacks. Make precedence explicit in the movement ruleset so classic/v1.31 behavior can be selected and tested. |
| 709 | Display combat ammunition at its v1.31 location | Make default | Pure presentation parity; no gameplay trade-off. |
| 713 | Use more appropriate summoning animations for Demons, Fire Elementals and Earth Elementals | Make default | Presentation-only improvement. Keep animation selection data-driven so additional modded creatures can define their own. |
| 718 | Split the Additional Power slider more proportionately in combat | Make default | Improves allocation feedback and avoids an extreme 99/1 split when the available infusion is small. The displayed split should match the actual allocation calculation. |
| 743 | Have the AI account for Confusion's temporary control change when assigning Summon Demons; partial fix for a crash above 18 units | Make default | AI command selection must use effective ownership/control and battle capacity. Treat the associated crash as a separate capacity-safety issue; this optional fix is explicitly only partial. |
| 745 | Prevent Hasted units from wasting ammunition on a second ranged attack when the first attack destroys the target; requires 744 | Skip | The listed prerequisite 744 is not included in this optional batch, and this is a narrow combat optimization. Revisit only if 744 is adopted and the attack sequence is implemented with equivalent target revalidation. |

## Reverted / Obsolete

These entries describe changes later reversed, replaced or superseded. They are historical context, not independent implementation tasks; use the later patch's behavior or replacement rule.

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 698, 699 | Movement enhancement order differs in battle; Wind Mastery overland change reversed | Use an explicit order of movement modifiers and keep battle/overland semantics aligned where intended. | P2 | Investigate |
| 002, 028, 029, 031 | Personality/objective weights from books and retorts; Pragmatist available to AI; secondary realm influence; guaranteed Uncommon summon and Sss'ra/Myrran defaults | Pragmatist is a valid AI objective. Secondary-realm weighting is not represented by the current primary-realm-only tables. Guaranteed summon and Sss'ra changes are explicitly reversed in the Community Patch. | P3 | Partial |
| 001, 013, 015 | Encounter creature budgets, primary divisor and maximum creature count | Separate encounter budget, creature selection and stack-size policy. Higher non-Node budgets are explicitly reversed in the Community Patch. | P3 | Design |
| 016–025, 017, 018, 019, 020, 021, 022, 023, 024, 025, 537 | Treasure budgets, minimum points, tower rewards, gold/mana, item value, prisoner heroes, spell rolls, special treasure frequency and distribution | Build treasure as a pipeline: creature budget, treasure budget, eligible categories, reward selection. Keep difficulty adjustment explicit. Most numeric changes are reversed in the Community Patch; eligibility and non-cumulative rolls are more durable rules. | P2 | Design |
| 439 | City growth is always reported | UI/event regression reversal. | P4 | UI |
| 748 | Escape key again exits advisors/list selectors instead of Space | Complete release-note entry; this reverses the Space-key behavior and restores Escape as the exit key. | P4 | UI |
