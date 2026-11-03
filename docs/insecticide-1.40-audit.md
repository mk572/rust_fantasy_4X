# Insecticide 1.40 audit against the Rust rewrite

**Repository:** [mk572/rust_fantasy_4X](https://github.com/mk572/rust_fantasy_4X)  
**Purpose:** Compare the Insecticide Community Patch 1.40 release notes with the rewrite's design and implementation, then record a focused code audit.  
**Scope:** This is an audit, not a request to reproduce every patch change. Some patch changes are balance choices, some are regressions later reversed, and some fix bugs the rewrite does not share.

## Executive summary

The second code pass found several strong safeguards already present:

- **Counter Magic:** the reserve is checked against the spell's difficulty and loses five strength per enemy spell attempt; the holder's own spells do not consume it. Tests cover the drain and owner exception.
- **Dispel targeting:** enchantments retain their caster. Dispel only removes spells cast by another wizard, and Spell Lock must be removed before other enchantments. Battle dispel handles combat spells and overland unit enchantments separately.
- **Invisibility:** battle visibility is explicit. Invisible enemies cannot be aimed at until revealed, adjacent, or seen by a unit with Illusions Immunity; legal-command generation and command validation enforce this.
- **Strategic-combat Defense bug:** auto-resolve is not a separate simplified damage formula. It starts a normal battle and asks the battle AI for commands, so attacks use the same dice-based resolver. The AI's expected-damage estimate also has no Defense-50 cap.
- **AI strength asymmetry:** strategic AI and battle AI share the same unit-strength formula. City defense estimates use visible or remembered defenders and a wall multiplier; there is no human-only fourfold reduction in the inspected path.

Two areas deserve follow-up rather than a clean bill of health:

1. **AI path invalidation and movement orders:** pathfinding avoids hostile squares and guarded sites, and movement stops when it reaches a hostile encounter. However, a stored GoTo path is not automatically recomputed merely because an enemy becomes adjacent or the strategic situation changes. There is also a documented edge case where a GoTo order that fails midway for a reason other than NoMovesLeft is silently cleared.
2. **AI strength is still an approximation:** the shared formula is consistent across sides, but it compresses attack, defense and hit points into one scalar. It does not fully model ammunition, special attacks, immunities, spells, tactical position or all battlefield conditions. This is a quality-of-estimation concern, not evidence of the specific fourfold human-garrison bug.

## Targeted code audit

### 1. Strategic-combat Defense values above 50

**Finding: addressed structurally; no matching cap found.**

`Game::auto_resolve_with` starts an ordinary battle, then repeatedly obtains a `BattleCommand` from the battle AI and applies it. There is no separate strategic-combat damage routine in this path. The battle attack resolver rolls Defense dice through `roll_successes`; it does not clamp Defense to 50. The battle AI's `expected_damage` estimates blocks using the target's Defense directly, also without a 50 cap.

The AI estimate is deliberately approximate: it uses expected hit and block rates and a minimum-damage floor, rather than simulating the full dice distribution. That is distinct from the original bug where Defense values collapsed to the same chance.

Relevant code: [battle/game.rs](../crates/f4x-core/src/battle/game.rs), [battle/attack.rs](../crates/f4x-core/src/battle/attack.rs), [battle/ai.rs](../crates/f4x-core/src/battle/ai.rs).

### 2. Counter Magic and dispel ownership

**Finding: the inspected behavior is correct and has focused tests.**

Counter Magic stores a reserve per side. When an enemy spell is checked, the code takes a snapshot of the current reserve, subtracts five with saturating arithmetic, and rolls against the spell's difficulty using the pre-drain strength. The holder's own spells bypass the reserve. Tests check the owner exception, the five-point drain, the approximate success rate at equal strength and difficulty, and the empty-reserve case.

Dispel uses `(SpellId, PlayerId)` pairs. The generic routine only attacks enchantments whose caster differs from the dispelling wizard. Spell Lock is handled first: while an enemy's Spell Lock is present, the attempt is made against the lock alone at its fixed difficulty; other enchantments are not considered until the lock is gone. Battle dispel separately checks combat enchantments and the original overland unit enchantments, then rebuilds the unit from the surviving set.

Relevant code: [spell/dispel.rs](../crates/f4x-core/src/spell/dispel.rs), [battle/mod.rs](../crates/f4x-core/src/battle/mod.rs), [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs), [spell/dispel/tests.rs](../crates/f4x-core/src/spell/dispel/tests.rs), [battle/tests.rs](../crates/f4x-core/src/battle/tests.rs).

### 3. Invisibility and Illusions Immunity

**Finding: addressed, with an explicit and tested visibility model.**

`Battle::seen_by` determines whether a side can make out an invisible enemy. The enemy is visible if it has revealed itself by attacking or casting, if one of the viewer's units is adjacent, or if a viewer's unit has Illusions Immunity. Otherwise it cannot be aimed at, and the battle command returns `Unseen`. Legal commands omit attacks and shots against unseen targets. Area spells are intentionally different: they can still hit an invisible unit because they do not aim at an individual target.

The separate hit penalty for invisibility is also waived for attackers with Illusions Immunity. Tests cover invisibility, adjacency, reveal duration, Illusions Immunity and the hit penalty.

Relevant code: [battle/mod.rs](../crates/f4x-core/src/battle/mod.rs), [battle/tests.rs](../crates/f4x-core/src/battle/tests.rs).

### 4. AI movement, stale paths and adjacent enemies

**Finding: partially addressed; this is the clearest follow-up item from this pass.**

The pathfinder computes a cheapest path while avoiding hostile squares and guarded lairs. The path is computed when a GoTo order is issued. While following it, movement stops at a hostile encounter or site encounter, and the path is cleared. A NoMovesLeft result preserves the remaining path for later; other movement errors clear it.

What I did not find is a general rule that invalidates or recomputes an existing GoTo path merely because an enemy stack becomes adjacent, a threat appears nearby, or the AI's preferred target changes. The code therefore avoids walking through known hostile squares, but does not establish the broader Insecticide behavior of reconsidering a saved route when nearby enemies change the tactical situation.

There is also a documented operational edge case: if a GoTo fails midway for an error other than NoMovesLeft (for example, StackFull), the path is cleared; the AI may have already counted the order as accepted. The repository's `docs/KNOWLEDGE.md` records a ship that remained outside a full city for 80 turns while the AI believed it was going home.

Recommended follow-up: make GoTo failures observable to the AI, and decide explicitly which new events should trigger route replanning (adjacent enemies, a newly hostile tile, blocked destination, full transport/city, or target no longer strategically useful). Avoid invalidating every path for every nearby enemy; use threat relevance and a test that proves the AI actually changes course when it should.

Relevant code: [unit/pathing.rs](../crates/f4x-core/src/unit/pathing.rs), [unit/movement.rs](../crates/f4x-core/src/unit/movement.rs), [game/mod.rs](../crates/f4x-core/src/game/mod.rs), [KNOWLEDGE.md](KNOWLEDGE.md).

### 5. AI strength estimation and human garrisons

**Finding: the specific asymmetry appears avoided; the estimate remains intentionally coarse.**

The battle AI and strategic AI share `battle::ai::strength(attack, defense, hit_points)`. The strategic AI obtains attack, Defense and hit points through the same PlayerView accessors regardless of which wizard owns the unit. For an actual city assault, it uses the strongest nine defenders (the number that can enter a battle), then applies a 1.3 city-defense multiplier. When the city is not visible, it uses remembered strength for up to 80 turns or a configurable guess.

I found no human-only multiplier or branch that reduces a human garrison's value to one quarter. The main limitation is that one scalar cannot fully represent tactical matchup: it uses the better of melee and ranged attack, and does not comprehensively account for ammunition, attack modes, special attacks, immunities, enchantment interactions or battlefield position. That may lead to inaccurate estimates in particular matchups, but it is not the reported fourfold side-specific bug.

Relevant code: [battle/ai.rs](../crates/f4x-core/src/battle/ai.rs), [f4x-ai/view.rs](../crates/f4x-ai/src/view.rs), [f4x-ai/military.rs](../crates/f4x-ai/src/military.rs).

## Recommendations

1. **Keep the current Counter Magic and dispel ownership model.** The code and tests already express the intended invariants; add regression tests if future changes alter the shared dispel path.
2. **Keep invisibility as a first-class visibility rule.** Preserve the distinction between individual targeting and area effects, and continue testing both legal-command generation and command validation.
3. **Do not build a second strategic-combat damage system.** Auto-resolve currently benefits from using the same battle rules as tactical combat. Improve the battle AI's evaluation separately from combat resolution.
4. **Prioritize AI route reconsideration and failure feedback.** This is the most concrete gap from the five-item audit. A GoTo command being accepted should not leave the AI believing a unit is progressing after the order has been silently discarded.
5. **Improve AI strength estimation incrementally.** Keep one shared base measure, but add matchup adjustments for ammunition, special attacks, immunities and relevant enchantments where evidence shows the current estimate is misleading. Do not introduce separate human and AI formulas.
6. **For Plan C, move spell-specific AI knowledge to data.** The current overland spell classifier still has numeric spell-ID checks for several special cases (Mastery, Earth Lore, Nature's Cures, Chaos Channels, Word of Recall, Stasis, Nature's Wrath and Spell Blast). Prefer stable string identifiers and explicit metadata or reusable custom-hook identifiers. Multiple spells should be able to share one special-rule identifier; a spell definition should refer to that identifier rather than relying on a magic number. AI desirability should remain separate from spell legality and target eligibility.

The audit is deliberately conservative: “addressed” means the inspected code has a matching invariant or architecture, not that every edge case in the original game has been exhaustively tested.


## Full Insecticide 1.40 release-note assessment table

The table below consolidates the 194 release-note assessments, now sorted by their actual patch issue identifiers. The former conversation-order numbers have been removed. Rows with multiple references remain grouped and are sorted by the lowest IW number in that group; IM and other non-IW references follow the IW entries.

“Investigate” means the release note identifies a behavior worth checking; it does not assert that the rewrite currently has the bug. “Resolved” means the inspected implementation has a matching rule or architecture. “Partial” means some of the reported behavior is present but a material part remains unverified or incomplete. “Divergence” marks an intentional alternative rule. “Superseded” marks a patch change later fixed, reversed or replaced.

**Priority scale:** P1 = critical correctness, crash, memory-safety or deterministic-state risk; P2 = high-impact rules, combat, AI or movement behavior; P3 = normal gameplay, economy, balance or design follow-up; P4 = UI, presentation or convenience work. Priorities are suggested investigation order, not claims that the rewrite currently contains the original bug.

### First 100 issue entries

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| IW001, IW007 | To Hit and To Defend UI | Investigate whether the UI distinguishes stats from abilities and modifiers. | P4 | UI |
| IW003 | Trireme and Galley transport capacity set to eight | Investigate whether ship capacity is configurable in content. | P3 | Investigate |
| IW005 and related | Extreme difficulty added and Intro removed without adjusting many mechanics | Investigate whether difficulty rules form a coherent set rather than a label-only mode. | P4 | Investigate |
| IW006 | Objective-based Town production | Investigate whether production choices follow the AI's objective and current needs. | P3 | Investigate |
| IW008 | Endurance Item Power description fails to identify the actual spell | Investigate descriptions and whether item powers point to their actual effect definitions. | P3 | Investigate |
| IW009 | Rename Strategic Combat Only to Automatic Combat | UI terminology choice; ensure the label describes what the option actually does. | P4 | Design |
| IW010 | Remove End of Turn Wait | UI/interaction design choice; not necessarily a rules issue. | P4 | Design |
| IW010 | Remove Auto Unit Information | Investigate whether the option is useful and how it maps to the rewrite's UI. | P4 | UI |
| IW010 | Rampaging Monsters begins on turn 40 | Balance and timing choice; test against the rewrite's game pace. | P3 | Design |
| IW013 | Encounter dialog supports four or more guardians | UI edge case; verify long guardian lists do not overflow. | P4 | UI |
| IW014 | Patrol hotkey L | UI convenience feature. | P4 | Design |
| IW016 | Tower of Wizardry tile auto-selection is plane-aware | Investigate map selection and plane context. | P3 | Investigate |
| IW017 | Wind Walking stack movement accounting | Investigate mixed movement allowances and which unit determines the stack's moves. | P3 | Investigate |
| IW018 | Overland income display inaccurate before movement; recalculation causes late-game lag | Investigate caching and calculation consistency between displayed and actual income. | P2 | Investigate |
| IW019, IW020 | City cycling with X | UI navigation feature. | P4 | UI |
| IW024 | Cities overview with T | UI navigation feature. | P4 | UI |
| IW030 | Archmage Spell Casting Skill display | Likely avoided structurally; verify the displayed value uses the same calculation as casting. | P4 | Investigate |
| IW040 | AI treasure item find | Investigate whether AI treasure rewards use the same generation and eligibility rules as human rewards. | P3 | Investigate |
| IW041 | Treasure item value limits | Investigate treasure generation separately from item filtering; the rewrite may intentionally use different bounds. | P3 | Investigate |
| IW041 | Loot encounter reseeds RNG to a static AI turn seed, repeating outcomes; fixed in 1.51 | Critical RNG ownership issue; gameplay systems must not reset a shared stream to a fixed seed. | P1 | Investigate |
| IW042 | Wizard's Pact war declaration | Investigate diplomacy rules around declaring war on a wizard with a Pact. | P3 | Investigate |
| IW043 | Diplomacy crashes if AI has no home-continent units | Handle the zero-unit case explicitly. | P1 | Investigate |
| IW044 | Dispel Confusion does not always restore original owner | Preserve original ownership in the temporary control state. | P2 | Investigate |
| IW045 | Invisibility target eligibility and detection | Visibility gates individual targeting; adjacency, reveal and Illusions Immunity are handled explicitly, while area spells remain distinct. | P2 | Resolved |
| IW047 | Engineer road-building fix has no effect | Verify the actual road-building calculation rather than assuming the patch changed it. | P4 | Investigate |
| IW048 | Human armies entering battle while moving | Investigate whether movement is interrupted cleanly when a battle begins. | P3 | Investigate |
| IW048 | AI pathfinding across player units and hostile tiles | Investigate passability, diplomacy and target handling when routes cross occupied or hostile tiles. | P2 | Investigate |
| IW049 | AI cannot flee on the first combat turn | Investigate whether the AI can retreat as soon as the battle rules permit it. | P3 | Investigate |
| IW050 | Strategic-combat units with Defense at or below 50 all receive the same damage chance | Auto-resolve uses ordinary battle resolution; no Defense-50 cap was found in the resolver or AI estimate. | P2 | Resolved |
| IW051 | Fortress defender casting with values above 16,383 | Investigate integer widths in defender casting statistics and budgets. | P2 | Investigate |
| IW051 | Strategic-combat healing, enhancement and direct damage in all phases | Auto-resolve runs the ordinary battle rules, so spell effects use the same tactical resolution path. | P2 | Resolved |
| IW051 | Strategic-combat mana over-spend | Auto-resolve uses legal battle casts and the supplied mana budget; no separate strategic spell budget is used. | P2 | Resolved |
| IW052 | Multiple realm bonuses in strategic combat | Investigate whether multiple applicable realm bonuses aggregate rather than overwrite one another. | P3 | Investigate |
| IW052 | Strategic-combat direct damage scaled by one half and enhancement by one third | Investigate explicit scaling; auto-resolve should use the same rules as tactical combat where possible. | P2 | Resolved |
| IW053 | Effective HP in strategic combat and AI | Investigate whether health and durability estimates agree with actual battle resolution. | P2 | Partial |
| IW054 | AI Possession targeting Heroes | Investigate target filtering for Heroes and other special unit classes. | P3 | Investigate |
| IW056 | Counter Magic strength loss | Counter Magic checks the pre-drain reserve and loses five strength per enemy spell attempt. | P2 | Resolved |
| IW056 | Counter Magic removes unrelated enchantments | Dispel tracks spell caster ownership and only removes eligible enemy enchantments; Spell Lock is handled first. | P2 | Resolved |
| IW057 | Spell Save modifiers apply to Life Drain, Death Spell and Holy Word | Battle spells use shared save and resistance machinery; keep explicit regression tests for the listed spells. | P2 | Resolved |
| IW058, IW121 | AI ignores Haste when choosing what to dispel | Investigate whether the AI recognizes Haste as a high-value target enchantment. | P3 | Investigate |
| IW059 | Vortex cap reduced to four despite original limit of ten | Investigate summon caps and whether the reduced limit is intentional. | P3 | Investigate |
| IW060 | Real-Time Game Tweaker can alter Resistance penalty for spells whose original penalty is zero | Treat zero as a valid default, not “not configurable”; distinguish an omitted value from an explicit zero override. | P4 | UI |
| IW061 | Dispel Evil target restriction | The special attack is limited to Chaos and Death creatures, with the stronger undead resistance penalty. | P2 | Resolved |
| IW062 | Life Drain versus Death Immunity | Life Drain's declared immunity traits include Magic and Death Immunity in the shared spell-damage resolver. | P2 | Resolved |
| IW063 | AI Healing Spell ability use | Investigate whether the AI recognizes and uses healing abilities when they are useful. | P3 | Investigate |
| IW063 | AI Doom Bolt animation misaligned | UI/animation issue; ensure spell effects and animation targets agree. | P4 | Investigate |
| IW064 | Hero items transfer to the victor after strategic combat | Fallen heroes are removed with the winning player as heir, which routes their items to the victor. | P2 | Resolved |
| IW065, IW067 | AI melee line advance disabled because another change removes lines | The changes interact; evaluate tactical behavior as a whole rather than individually. | P3 | Investigate |
| IW066, IW091 | AI ranged strength estimates | Investigate whether ranged attacks are included consistently in friendly and enemy strength estimates. | P3 | Investigate |
| IW067 | AI no longer uses lines or group movement | Treat as a tactical-AI design choice; verify whether the rewrite's battle AI needs formations. | P3 | Design |
| IW068 | AI ranged attacks against Invisible units with Illusions Immunity | Investigate visibility and target selection for ranged attacks against invisible targets. | P3 | Investigate |
| IW069 | Melee target prioritizes damaged enemies | Battle AI scores expected damage against remaining hit points and target worth, favoring kills over nearest-target selection. | P2 | Resolved |
| IW070 | Right-arrow hotkey in item creation | UI convenience feature. | P4 | Design |
| IW072 | Treasure/gift item no longer themed to recipient | Investigate whether recipient-themed rewards are a desired generation policy. | P3 | Investigate |
| IW073 | Demon Skin Defense bonus applied twice | Demon-skin mutation adds one +3 Defense temporary modifier through the shared stat path. | P2 | Resolved |
| IW074 | Hero spell indices above 127 | Likely avoided by typed IDs; verify no byte-sized spell index remains in Hero spell handling. | P3 | Investigate |
| IW075 | AI upkeep discount stacking | Investigate whether difficulty bonuses and other upkeep modifiers stack correctly. | P3 | Investigate |
| IW076 | Conjurer maintenance fraction rounds up from 0.5 | The half-step numeric newtype can represent the value; verify the rounding rule. | P3 | Investigate |
| IW077 | Wind Mastery ship movement differs by player and combat context | Investigate context and ownership consistency for global movement effects. | P3 | Investigate |
| IW078 | Chaos Channels unit classification for overland spells and Call Chaos | Mutation requires a normal, non-undead unit that has not already been mutated. | P2 | Resolved |
| IW079 | Cause Fear target | Investigate target selection and the actual effect on the intended target. | P3 | Investigate |
| IW080, IW085 | Thrown and Breath attacks respect ranged To Hit modifiers | Battle units carry separate melee and ranged To Hit bonuses; thrown attacks retain the applicable ranged bonuses. | P2 | Resolved |
| IW081 | Block bonus erroneously reduces enemy To Hit | Investigate that defensive block bonuses do not leak into the attacker's To Hit calculation. | P2 | Investigate |
| IW082 | Units on central tile of a walled Town gain +3 Defense | Investigate tile-specific defensive bonuses. | P3 | Investigate |
| IW083 | Weapon Immunity no longer overrides Missile Immunity but Defense set to 50 instead of 10 for non-melee; fixed in 1.51 | Immunity precedence is explicit: Weapon Immunity against normal weapons yields at least 10 Defense, with Magic Immunity applied afterward. | P2 | Resolved |
| IW084 | Siege automatic +3 Defense in strategic combat | Investigate whether the extra Defense is intentional and consistently represented. | P3 | Investigate |
| IW084 | Siege gains +2 with City Walls or Wall of Fire | Investigate the siege bonus and its stacking rules. | P2 | Investigate |
| IW087 | Damaged reachable melee targets prioritized, sleeping targets not | Investigate whether target scoring accounts for damage and incapacitated targets sensibly. | P3 | Investigate |
| IW088 | Regenerating moves recalled units back | Investigate recall position and whether regeneration incorrectly changes unit placement. | P3 | Investigate |
| IW089 | Defenders lose strategic movement after combat | Investigate movement reset after battle for units that defended. | P3 | Investigate |
| IW090 | Chaos Channeled units gain experience | Investigate mutation and progression classification. | P3 | Investigate |
| IW092 | Army strength threshold raised from 66% to 75% | Balance choice; do not copy the threshold without testing the rewrite's combat model. | P3 | Design |
| IW093 | Units without Endurance gain extra combat movement | Investigate movement restoration and whether units receive only their proper allowance. | P3 | Investigate |
| IW095 | AI less likely to raze a city captured from another AI | Investigate whether previous ownership should influence razing decisions. | P3 | Investigate |
| IW096 | AI razes more when enemy forces are nearby | Investigate whether razing depends on the city's actual defensibility. | P3 | Investigate |
| IW097, IM179 | Overland Spell Events disabled by default but setting has no effect | Verify the setting is wired through to the event system. | P3 | Investigate |
| IW098 | Charismatic Retort applied before affordability | Investigate whether discounts are applied before or after the affordability check. | P3 | Investigate |
| IW099 | AI refuses lower-ID Heroes to preserve Hero slots | Investigate recruitment policy; avoid using numeric ID as a proxy for Hero quality. | P4 | UI |
| IW099 | Fame contribution cap changes | Investigate recruitment affordability and Retort cap interactions. | P4 | UI |
| IW100 | At most three Arcane spells in the research pool | Investigate research-pool composition and whether the cap is configurable. | P3 | Investigate |
| IW101 | Spell trade priority uses realm index and desirability | Investigate explicit, data-driven spell trade valuation rather than realm ordering. | P3 | Investigate |
| IW102 | Chaos Channels does not grant Chaos Breath to low-strength Thrown units | Breath is only an available mutation when the unit has no ranged attack or only a weak thrown attack. | P2 | Resolved |
| IW103 | Combat dispels the target's intended spell rather than the caster's own | The shared dispel list only removes enchantments cast by another wizard; battle dispel uses that same routine. | P2 | Resolved |
| IW104 | Spell Lock combat-cast dispel protection fix failed | The battle dispel path calls the shared routine, which allows only Spell Lock itself to be targeted while it guards a unit. | P2 | Resolved |
| IW105 | Holy Arms prevents dispelling Holy Weapon | Investigate the distinction between a global protection and an individual unit enchantment. | P2 | Investigate |
| IW105 | Invulnerability remains protected from dispelling in combat | The rewrite documents a deliberate divergence allowing combat dispel; keep the divergence explicit and tested. | P2 | Divergence |
| IW105 | Possession can be dispelled by its caster to retain control | Investigate ownership semantics and whether dispelling should restore or retain control. | P2 | Investigate |
| IW106 | Merging units are immune to Cracks Call | Investigate target state and immunity during merging. | P2 | Investigate |

### Remaining 94 issue entries

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| IW107 | Raise Dead and Animate Dead have unsafe state handling and interfere with sound-error reporting | Critical safety concern; use typed state changes and test resurrection paths. | P1 | Investigate |
| IW108 | Animate Dead ownership | Investigate that raised units receive the correct owner. | P2 | Investigate |
| IW109, IW110 | Summon Hero reseeds RNG to a static value | Critical RNG issue; use the game's owned random stream or a derived deterministic stream. | P1 | Investigate |
| IW111 | Wrack damage fix introduces memory corruption | Critical safety concern; route spell damage through safe, shared damage machinery. | P1 | Investigate |
| IW112 | Call Lightning ignores Magic Immunity rather than Wraith Form | The rewrite documents a Wraith Form divergence; verify the implementation matches that deliberate rule. | P2 | Divergence |
| IW113, IW144 | AI Enchant Item and Create Artifact enabled technically but only after turn 180 and selected almost never | Investigate eligibility separately from desirability and scoring. | P2 | Investigate |
| IW114 | Suppress Magic no longer counters Spell of Return | Investigate spell-specific dispel immunity and context. | P2 | Investigate |
| IW115 | Combat spell choice depends on winning or losing | Investigate context-aware tactical spell scoring. | P2 | Partial |
| IW116, IW124 | AI prioritizes dispelling Call Lightning and Wrack | Investigate whether the AI recognizes dangerous active battlefield effects. | P2 | Investigate |
| IW117 | AI spell-realm lookup fixed but resulting modifier remains zero | Verify end-to-end scoring, not just lookup success. | P3 | Investigate |
| IW118 | Wall of Stone chance reduced in battle despite being a non-combat spell | Filter spell evaluation by valid casting context. | P3 | Investigate |
| IW118 | Wall of Fire preference reduced in combat though AI cannot cast it | Keep spell eligibility separate from desirability. | P3 | Investigate |
| IW119, IW120, IW121 | Spell category reclassification suppresses damage spells when AI is losing | Investigate category scoring and ensure losing-state logic does not suppress useful attacks. | P2 | Partial |
| IW120 | Various combat spell preferences | Investigate spell-specific tactical desirability, target resistance and matchup. | P2 | Partial |
| IW120 | AI cannot cast listed damage, banish and possession spells when overwhelmingly disadvantaged | Investigate whether tactical spell eligibility is incorrectly tied to overall battle odds. | P2 | Partial |
| IW121 | AI cannot cast Dispel Magic if the human has Holy Arms | Investigate target/eligibility logic for dispelling global enchantments. | P2 | Investigate |
| IW123 | Mana Leak turn and wall conditions | Investigate turn-dependent behavior and siege context for Mana Leak. | P3 | Investigate |
| IW125, IW130 | Less Immolation threat and protection-spell use | Investigate defensive spell valuation and threat estimation. | P3 | Investigate |
| IW126 | AI randomly disbands two garrison units per city every 20 turns from turn 80 | Treat as a poor heuristic; evaluate actual upkeep need and strategic value before disbanding. | P2 | Investigate |
| IW128 | Armageddon and Great Wasting cancel town-building pacification | City buildings continue to pacify; the world event adds unrest rather than disabling their effects. | P2 | Resolved |
| IW129 | Mercenary hiring timing, budget and type | Investigate recruitment timing and whether the AI can afford the right mercenaries. | P4 | UI |
| IW131 | AI fortress defender spell values above 16,383 | Investigate integer widths in the AI's fortress-defender spell valuation. | P2 | Investigate |
| IW132 | Excess population growth fix fails; corrected in 1.50 | The shared population-growth function caps gains while still applying population losses. | P2 | Resolved |
| IW134 | Merchant offer threshold is twice item value; AI gets offers and always buys at half price | Investigate merchant economy and distinguish human and AI offer rules. | P4 | Investigate |
| IW135 | AI research progress | Investigate research allocation and whether progress is retained and advanced correctly. | P3 | Investigate |
| IW135 | AI Power allocation for Spell of Mastery remains wrong | Investigate Power allocation and victory-spell planning. | P2 | Partial |
| IW135 | AI overallocates Power by 10% | Enforce resource-budget conservation and test allocation sums. | P1 | Investigate |
| IW137 | AI uses Alchemy before mana is exhausted and values Mana | Investigate resource conversion and reserve planning. | P3 | Investigate |
| IW138 | AI tax rate cannot fall below 30% | Investigate valid tax range and whether the minimum is intended. | P3 | Investigate |
| IW139, IW151, IW153 | Settler priorities and reserve | Investigate expansion priorities, settler availability and whether reserves are maintained. | P3 | Investigate |
| IW139, IW151, IW153 | Engineer priorities and reserve | Investigate engineer recruitment and infrastructure needs separately from settlers. | P4 | UI |
| IW140 | Invalidate saved path when adjacent enemies appear | Investigate whether new threats should cause the AI to reconsider a stored route. | P2 | Partial |
| IW141, IW143 | Research priorities | Investigate explicit AI research desirability rather than relying on incidental ordering. | P3 | Investigate |
| IW142 | Mana target for expensive overland spells | Investigate resource planning for spells that require a large reserve. | P4 | UI |
| IW142 | AI ignores Mana reserves and income when planning Spell of Mastery | Investigate resource planning and reserve forecasting. | P2 | Partial |
| IW144 | Barbarian Spearmen prevent AI Incarnation | Investigate spell eligibility and whether the AI rejects an otherwise valid summon. | P3 | Investigate |
| IW144 | Summon selection uses creature preference rather than exponential casting cost | Strong candidate for data-driven per-creature AI weights. | P2 | Partial |
| IW144 | AI will not Summon Hero with four or more Heroes | Investigate Hero roster caps and recruitment eligibility. | P4 | Investigate |
| IW144 | AI values summoning spirits to Meld with Nodes more, but still below late-game summons | Investigate the strategic value of node control in summon desirability. | P3 | Investigate |
| IW144 | AI uses different unit-enchantment priorities, not necessarily better | Investigate target suitability and whether enchantment value matches the unit's role. | P4 | Investigate |
| IW145 | AI cancels only Suppress Magic and Time Stop when out of Mana | Investigate upkeep maintenance and which global spells should be cancelled first. | P3 | Investigate |
| IW145 | AI cancels Evil Omens immediately | Investigate global-enchantment maintenance and whether the AI understands its effects. | P3 | Investigate |
| IW146 | AI Heroism, Holy Armor and Black Channels on Fantastic Units | Investigate target filtering for Fantastic Units and other special unit classes. | P3 | Investigate |
| IW146 | AI prioritizes unit enchantments on Heroes, with exceptions for travel spells on Settlers | Investigate spell-specific target suitability. | P2 | Partial |
| IW146 | AI prefers Fantastic Units and appropriate ranged units for enchantments | Investigate target suitability and whether the AI understands what each enchantment improves. | P2 | Partial |
| IW147 | AI moves the Summoning Circle more often | Investigate Summoning Circle placement and strategic relocation. | P3 | Investigate |
| IW148 | AI buys production only after progress has been made | Investigate purchase timing and whether immediate buying is ever preferable. | P3 | Investigate |
| IW149 | AI Trade Goods restriction above three Towns | Investigate production policy and whether the town-count threshold is desirable. | P3 | Investigate |
| IW150 | AI uses Trade Goods less unless it has at least 24 cities | Balance and economy design choice; test rather than copying the threshold. | P3 | Design |
| IW150, IW152 | Reduced enforcement of home-continent unit production | Investigate whether production should be tied to strategic geography. | P3 | Investigate |
| IW152, IW153 | Smithy before Builders' Hall; Granary before Shrine | Investigate contextual building priorities and their prerequisites. | P4 | UI |
| IW153 | AI cannot override Trade Goods or Housing with unit production | Investigate production override rules and queue priority. | P2 | Partial |
| IW153 | AI cannot produce Settlers below population four | Investigate population restrictions and settler eligibility. | P2 | Resolved |
| IW153 | AI cannot produce more than three ships total | Investigate global ship caps and transport needs. | P2 | Investigate |
| IW154 | Neutral cities prefer Builders' Hall and Granary | Investigate neutral-city production priorities. | P4 | UI |
| IW155 | AI buys production less often, especially in larger cities | Investigate purchase valuation by city size and production progress. | P3 | Investigate |
| IW156 | AI movement cancellation when passing a rival city | Investigate pathfinding and diplomacy when a route passes through a rival's territory. | P2 | Investigate |
| IW157 | AI keeps 33% fewer garrison units but still cannot build stacks due to other bugs | Investigate garrison and army assembly together; fixing one heuristic may expose another bottleneck. | P4 | UI |
| IW158 | Minerals matter more when settling | Investigate mineral value in city-site evaluation. | P2 | Investigate |
| IW158 | Settlers sometimes target a new colony despite nearby units, wasting 1–11 turns | Investigate target feasibility and route cost before assigning a settlement destination. | P2 | Partial |
| IW158 | Settler on Tower tile blocks gateway consideration for other Settlers | Investigate gateway traversal and shared destination occupancy. | P2 | Investigate |
| IW159 | Single-tile lake inconsistently treated as river, invalid building/river mouth and valid Plane Shift for land AI | Investigate consistent terrain semantics across building, movement and spell eligibility. | P2 | Divergence |
| IW160 | AI hostility to Spell of Mastery caster fix fails | Investigate victory-threat diplomacy and hostility transitions. | P2 | Resolved |
| IW161 | AI loading transports only slightly better | Investigate the entire embarkation and disembarkation workflow. | P2 | Partial |
| IW162 | AI abandons small-map continents if it cannot attack | Investigate expansion and attack feasibility on isolated landmasses. | P2 | Partial |
| IW163 | AI evaluates human garrisons four times weaker | The rewrite shares one strength formula across strategic and battle AI; investigate matchup accuracy, not a side-specific multiplier. | P2 | Resolved |
| IW163 | AI attacks hostile cities without formal war | Investigate the distinction between hostility, war status and legal attack eligibility. | P2 | Investigate |
| IW164 | AI does not disband units if it can afford 32 turns of upkeep | Investigate whether disbanding uses a sensible financial runway. | P3 | Investigate |
| IW165 | Arrow keys flip Apprentice spellbook pages | UI navigation feature. | P4 | UI |
| IW167 | AI receives no building-upkeep discount | Prefer player/AI parity unless a difficulty setting explicitly says otherwise. | P4 | UI |
| IW173 | Space quits Save/Load screen | UI hotkey choice. | P4 | Design |
| IM176 | Lo Pan receives an extra spellbook | Likely covered by explicit spellbook picks; verify the setup definition. | P3 | Investigate |
| IM177 | AI starting spell picks | Investigate data-driven defaults and whether AI spell picks use the same rules as human picks. | P3 | Investigate |
| IM178 | Magic settings renamed for clarity | UI clarity and naming. | P4 | Design |
| IM178, IM180, IM186 | Huge land size | Investigate map scalability and configurable world size. | P3 | Investigate |
| IM183 | Starting town on an Encounter Zone | Likely covered by map-generation exclusion rules; verify the starting-site validator. | P3 | Investigate |
| IM184 | Narrower Y placement range combined with IM183 reduces spacing and increases map-generation failures | Investigate interacting placement constraints, not each parameter in isolation. | P2 | Investigate |
| IM188 | Colored lair Chaos monsters at 50%; broken implementation replaced by actual fix | Investigate correct weighted realm composition; the note describes intended 40% Death, 40% Chaos and 20% Nature. | P2 | Investigate |
| IM189 | Secondary monster divisor maximum decreased | Investigate random ranges and encounter balance. | P3 | Investigate |
| IM192 | Neutral towns receive one additional close-combat unit | Investigate neutral garrison composition. | P3 | Investigate |
| IM193 | Neutral cities start with Granary at size six and Stables at size seven | Investigate neutral-city starting buildings and population thresholds. | P4 | UI |
| IM197 | AI starting spell grants with Basilisk and Pathfinding for Life | Investigate starting-spell allocation and race/realm-specific grants. | P3 | Investigate |
| IM197 | AI wizard with 11 [value omitted in pasted note] receives fewer spells | Investigate the allocation boundary; the pasted release note omitted the stat name. | P3 | Investigate |
| IM198 | Retorts affect AI Personality and Objective | Wizard setup weights personality and objective choices using specific Retort bonuses. | P2 | Resolved |
| IM200 | Alorra and Torin with Arcane Power | The rewrite deliberately restricts Arcane Power to Heroes with suitable magical ranged attacks; verify definitions. | P4 | Divergence |
| ITEMDATA.LBX / ITEMMAKE.EXE | Item Spellbook requirements | The rewrite deliberately avoids the faulty per-power book-byte filtering; verify crafting requirements instead. | P4 | Divergence |

## P2 code review: first pass

This is an initial source inspection of the P2 entries, not a claim that every P2 row has been exhaustively tested. “Resolved” in the table is reserved for concrete implementation evidence; incomplete areas remain marked Partial or Investigate.

### Combat rules and spell effects

- **IW045 — Invisibility and Illusions Immunity: Resolved.** The earlier targeted audit confirmed that visibility gates legal commands and command validation, with separate handling for area spells.
- **IW050, IW051, IW052 — strategic combat and damage: Resolved structurally.** Auto-resolve runs an ordinary battle driven by the battle AI rather than a second simplified damage engine. The attack resolver therefore uses the same Defense dice as tactical combat; the AI's expected-damage estimate has no Defense-50 cap. The chosen strategic mana budget is enforced through the normal legal-cast and battle-casting path. Relevant code: [battle/game.rs](../crates/f4x-core/src/battle/game.rs), [battle/attack.rs](../crates/f4x-core/src/battle/attack.rs), [battle/brain.rs](../crates/f4x-core/src/battle/brain.rs).
- **IW056 — Counter Magic and dispel ownership: Resolved.** The earlier targeted audit checked the five-point reserve drain, owner exception, caster ownership, and Spell Lock ordering; focused tests cover the core invariants. See [spell/dispel.rs](../crates/f4x-core/src/spell/dispel.rs) and [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs).
- **IW061 — Dispel Evil: Resolved.** The special attack is explicitly limited to Chaos and Death creatures, with a stronger penalty against undead; this is encoded in [battle/special.rs](../crates/f4x-core/src/battle/special.rs) and described in [combat.md](combat.md).
- **IW062 — Life Drain and Death Immunity: Resolved at the spell-damage layer.** The combat rules document Life Drain's Magic and Death immunity flags, and damage spells use their declared immunity traits in the shared resolver. See [combat.md](combat.md) and [battle/casting.rs](../crates/f4x-core/src/battle/casting.rs).
- **IW073 — Demon Skin double Defense: Resolved.** Chaos Channels adds one +3 Defense temporary modifier for the armor mutation; the mutation is represented as a single effect and read through the normal stat path. See [unit/mutation.rs](../crates/f4x-core/src/unit/mutation.rs).
- **IW078 and IW102 — Chaos Channels eligibility and breath: Resolved.** Mutation requires a normal, non-undead, not-yet-mutated unit. Wings are excluded for fliers/ships, and breath is only an option when there is no ranged attack or a weak thrown attack. See [unit/mutation.rs](../crates/f4x-core/src/unit/mutation.rs).
- **IW080/IW085 — thrown and breath To Hit modifiers: Resolved in the current combat model.** Battle units carry separate melee and ranged To Hit values; the combat documentation records that thrown attacks retain weapon-quality and ranged bonuses, while breath/gaze do not gain weapon-quality bonuses. See [battle/game.rs](../crates/f4x-core/src/battle/game.rs) and [combat.md](combat.md).
- **IW083 — Weapon Immunity versus Missile Immunity: Resolved.** The precedence is explicit in effective_defense: matching non-melee immunity is applied first, then Weapon Immunity against a normal weapon replaces that stop with a minimum Defense of 10; Magic Immunity is applied afterward. There are focused tests in [immunity/tests.rs](../crates/f4x-core/src/battle/immunity/tests.rs).
- **IW057 — spell-save modifiers: Resolved at the shared-rule level.** Battle spell definitions carry their save data, and resistance is calculated through the shared combat resistance machinery. The specific Life Drain, Death Spell and Holy Word cases should still retain regression coverage if their definitions change. See [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs) and [battle/immunity.rs](../crates/f4x-core/src/battle/immunity.rs).
- **IW128 — Armageddon/Great Wasting and city pacification: Resolved.** City economy continues to apply building pacification, then adds the event's unrest; the code comment explicitly says the buildings still calm the city. See [city/economy.rs](../crates/f4x-core/src/city/economy.rs).
- **IW127/IW132 — population above the cap: Resolved for the reported stuck-population behavior.** Positive growth stops at the configured cap, while negative growth still removes people and carries tens correctly. See [city/growth.rs](../crates/f4x-core/src/city/growth.rs).
- **IW112 — Call Lightning/Wraith Form: Divergence remains explicit.** Call Lightning's damage is Nature magic and is stopped by Magic Immunity in the shared spell-damage path; Wraith Form is not treated as shelter. The exact original-versus-patch rule should remain documented as a deliberate compatibility decision. See [battle/field.rs](../crates/f4x-core/src/battle/field.rs).

### AI behavior and strategic planning

- **IW069 — target damaged enemies: Resolved in broad intent.** The battle AI documents and implements target scoring by damage dealt relative to remaining hit points and unit worth, favoring kills rather than simply nearest targets. See [battle/ai.rs](../crates/f4x-core/src/battle/ai.rs).
- **IW103/IW104 — battle dispel ownership and Spell Lock: Resolved.** Battle dispel calls the shared dispel-list routine. It only removes spells cast by another wizard, and while an enemy Spell Lock is present the lock alone is targeted at its fixed difficulty. See [battle/spell.rs](../crates/f4x-core/src/battle/spell.rs) and [spell/dispel.rs](../crates/f4x-core/src/spell/dispel.rs).
- **IW064 — hero-item spoils: Resolved.** When a fallen Hero is removed after battle, the winning player is passed as heir; hero removal routes the fallen Hero's items to that heir. See [battle/game.rs](../crates/f4x-core/src/battle/game.rs) and [hero/game.rs](../crates/f4x-core/src/hero/game.rs).
- **IM198 — Retorts and AI personality/objective: Resolved.** Game setup weights both personality and objective choices using Retort-specific bonuses (e.g. Warlord, Archmage, Myrran and Conjurer). See [wizard/objective.rs](../crates/f4x-core/src/wizard/objective.rs).
- **IW115/IW120 — battle spell selection: Partial.** The wizard AI scores legal spells from their data, estimating damage, healing, summons, unit buffs and other supported effects against unit value and mana cost. It does not explicitly switch strategy based on whether the side is winning or losing, and unsupported categories are skipped. See [battle/brain.rs](../crates/f4x-core/src/battle/brain.rs).
- **IW116/IW124 — dispelling Call Lightning and Wrack: Investigate.** The battle wizard AI explicitly leaves field spells and dispels alone. The game resolves both field effects, but the AI currently has no valuation that would prioritize removing them. This is a concrete AI gap, not a combat-resolution bug. See [battle/brain.rs](../crates/f4x-core/src/battle/brain.rs) and [battle/field.rs](../crates/f4x-core/src/battle/field.rs).
- **IW140 — stale GoTo paths: Partial.** Pathfinding avoids hostile squares and the settler AI abandons a target after making no progress, but I still found no general invalidation of an existing route merely because an enemy becomes adjacent or the tactical situation changes. This remains a follow-up. See [unit/pathing.rs](../crates/f4x-core/src/unit/pathing.rs), [unit/movement.rs](../crates/f4x-core/src/unit/movement.rs) and [ai/expand.rs](../crates/f4x-ai/src/expand.rs).
- **IW144 — summon selection: Partial.** Overland AI classifies summons from their effect definitions and ranks the general Summon category with objective-specific boosts. I did not find per-creature desirability weights, so the specific preference-versus-casting-cost issue remains open. See [ai/wizardry.rs](../crates/f4x-ai/src/wizardry.rs).
- **IW146 — unit enchantment target selection: Partial.** The AI has generic unit-buff scoring and chooses among eligible friendly units, but the inspected path does not establish the full Hero/Fantastic-unit and Settler-travel exceptions from the patch notes. See [ai/wizardry.rs](../crates/f4x-ai/src/wizardry.rs).
- **IW158 — Settler site choice: Partial.** Site scoring currently values worked terrain and river access, then subtracts distance; it does not include mineral value. The AI does avoid destinations already assigned to other settlers and abandons a target when it detects no movement, but candidate reachability is checked only after the ten highest-scoring sites are selected. That ordering is worth testing because it can discard a reachable lower-ranked site. See [ai/expand.rs](../crates/f4x-ai/src/expand.rs).
- **IW161 — transport loading: Partial.** The AI has a dedicated naval plan, checks ship capacity, preserves the port's desired garrison, requires a minimum load, waits for sufficient landing strength (or a timeout), and records passengers for the voyage. This is substantially more than a simple “load units” heuristic, but it still deserves end-to-end tests for blocked routes, partial loads and failed embarkation. See [ai/naval.rs](../crates/f4x-ai/src/naval.rs).
- **IW162 — isolated continents: Partial.** The military planner gives unreachable enemy cities a cooldown before trying again, but I did not find a continent-level policy that abandons a landmass solely because no attack is currently feasible. See [ai/military.rs](../crates/f4x-ai/src/military.rs).
- **IW163 — human garrisons valued four times weaker: Resolved.** Strategic and battle AI share the same strength function and the same PlayerView stat accessors; no human-only reduction was found. The scalar estimate is still coarse. See [ai/view.rs](../crates/f4x-ai/src/view.rs) and [ai/military.rs](../crates/f4x-ai/src/military.rs).
- **IW198 — Retorts affecting AI personality/objective: Investigate.** The core setup path explicitly adds Retort-specific weights before drawing personality and objective; this item is resolved. See [wizard/objective.rs](../crates/f4x-core/src/wizard/objective.rs).
- **IW153 — Settler production threshold: Resolved for the core rule.** The core permits a citizen-costing unit when population is at least two, not four. Whether the AI chooses to build one at that population is a separate policy question. See [city/production.rs](../crates/f4x-core/src/city/production.rs).

### Highest-value follow-ups from this pass

1. Add battle-AI scoring for field dispels, especially Call Lightning and Wrack; this is a direct, visible omission.
2. Make route invalidation and GoTo failure feedback explicit, including newly adjacent threats and changed destinations.
3. Improve Settler site evaluation with mineral value and test the top-ten-before-reachability ordering.
4. Add matchup-aware and state-aware tactical spell scoring, including explicit winning/losing situations.
5. Give overland summons and unit enchantments data-driven target/desirability weights, including reusable spell-rule identifiers as planned for Plan C.

## Notes on interpretation

- The numbered table follows the order in which the release-note items were assessed in the conversation, not the original patch's issue-number order.
- The text of the release notes was supplied in the conversation. A few pasted lines were incomplete; those are called out rather than silently filled in.
- A “likely avoided” or “addressed” assessment is a hypothesis until the relevant code and tests are inspected. The targeted code audit above records what was actually checked in this second pass.
- Some release-note fixes were later reversed or superseded. Those are marked as design/balance investigations rather than requirements.

## Optional

These entries are retained as selectable alternatives rather than treated as mandatory rewrite work.

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| IW010, IW011 | Add Revolting Raiders option | Investigate whether the optional game mode is worth supporting. | P3 | Design |
| IW010, IW011 | Add Monsters Gone Wild option | Investigate whether the optional game mode is worth supporting. | P3 | Design |

## Reverted / Obsolete

These entries describe behavior later reversed, replaced or superseded by a later patch. Do not track them as independent implementation requirements; follow the later baseline or the replacement rule.

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| IW002 | Rampaging monster budget change, later reversed | Balance investigation; do not assume the temporary patch change is desirable. | P3 | Design |
| IW004 | Floating Island loses Death Immunity and gains standard unit flag/race; later fixed | Investigate unit flags and immunities; the patch's intermediate fix was itself defective. | P2 | Superseded |
| IW015 | Nature Awareness auto-casts Detect Magic; later fixed | Investigate unintended coupling between unrelated spell effects. | P3 | Investigate |
| IW029 | Wraithform item power only works if it is the last Hero in a stack and affects the whole stack; later fixed | Investigate per-unit equipment effects versus stack-wide effects. | P2 | Investigate |
| IW043 | Diplomacy against an overwhelmingly strong invader is backwards; later replaced | Investigate relative military-strength comparison and diplomacy direction. | P3 | Investigate |
| IW043 | AI war declaration against a stronger home-continent rival is backwards; later replaced | Investigate war-declaration strength comparison and home-continent context. | P3 | Investigate |
| IW055 | Runemaster dispel modifier additive instead of multiplicative, only in combat; removed in 1.50 | Investigate consistency of dispel modifiers across contexts; do not copy a reverted change. | P3 | Investigate |
| IW070 | Space exits crafting screen and right-arrow exit removed; later reversed | UI design choice; avoid adopting an intermediate hotkey regression. | P4 | UI |
| IW086 | AI melee targets exactly reachable are deemed unreachable; later fixed | Investigate movement-range boundary conditions. | P3 | Investigate |
| IW094 | Lawful AI avoids razing positive relations but still razes at zero; later fixed | Investigate threshold comparisons and neutral-relation behavior. | P3 | Investigate |
| IW127 | Population above maximum cannot grow but also cannot lose population; later fixed | Growth stops at the configured cap, but negative growth still removes population and normalizes the fractional remainder. | P2 | Resolved |
| IW133 | Hide population growth at 8,000 or more; later reversed in 1.50 | UI behavior; avoid copying a temporary display change without checking the later reversal. | P4 | Design |
| IM178 | Add higher magic settings, later removed | Design choice; do not treat a reverted option as a requirement. | P4 | Design |
| IM183 | Starting fortress maximum population raised from eight to eleven causes map-generation freezes; reversed in 1.50 | Investigate map-generation termination and starting-site constraints; the current generator has bounded retries. | P2 | Partial |
| IM185 | AI race choices on high difficulty; later reversed | Design choice; check race restrictions and avoid reproducing a change the Community Patch later reversed. | P3 | Design |
| IM187 | Encounter budgets increased, later reversed | Balance choice; investigate the desired difficulty curve rather than copying the temporary change. | P3 | Design |
| IM194 | Myrror city roads fixed but all Arcanus towns also receive roads; later fixed | Investigate plane-specific map initialization and terrain/road ownership. | P3 | Investigate |
| IM195 | Spell of Mastery research discount for 11 same-realm books changed substantially, later reversed | Balance investigation; do not reproduce a reverted economic change by default. | P3 | Design |
