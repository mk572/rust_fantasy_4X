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
| 698, 699 | Movement enhancement order differs in battle; Wind Mastery overland change reversed | Use an explicit order of movement modifiers and keep battle/overland semantics aligned where intended. | P2 | Investigate |
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

Relevant code: [battle/legal.rs](../crates/f4x-core/src/battle/legal.rs), [battle/ai.rs](../crates/f4x-core/src/battle/ai.rs), [battle/brain.rs](../crates/f4x-core/src/battle/brain.rs), [ai-design.md](ai-design.md).

**Recommended tests:** AI spell target becomes invalid between scoring and execution; friendly and inactive targets excluded; flying target rejected by a ground-only attack; battle memory reset between battles; multi-AI turn execution completes one wizard's whole turn before the next wizard begins.

### 6. Spell costs, immunity and dispel calculations

**Finding: shared spell definitions and shared dispel machinery are a strong foundation; slider and context interactions remain open.**

Battle spell definitions are derived from spell effects, and the shared dispel module owns the base formula, realm mastery and Archmage multipliers, Spell Lock difficulty and Counter Magic drain. This is preferable to separate per-spell formulas.

The dense IW720 cluster deserves its own boundary tests. It covers casting-cost reductions, Evil Omens, additional-power sliders, remaining mana, casting skill, spell charges, hero channeling and counter difficulty. These values should be computed once from a canonical spell-cost model and then reused for UI availability, command validation, actual mana deduction and dispel difficulty. Otherwise a spell can appear castable but fail at execution, or the UI and resolver can disagree.

The immunity fixes likewise argue for explicit attack/spell natures rather than broad labels. Weapon Immunity must distinguish melee weapons, thrown attacks, missiles and boulders; Wraith Form must not accidentally block unrelated spell realms; Death Immunity must be checked by Terror and undead-raising effects where specified.

Relevant code: [spell/dispel.rs](../crates/f4x-core/src/spell/dispel.rs), [battle/immunity.rs](../crates/f4x-core/src/battle/immunity.rs), [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs), [content/spells.ron](../content/spells.ron).

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
