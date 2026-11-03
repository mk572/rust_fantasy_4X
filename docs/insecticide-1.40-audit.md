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

The table below consolidates the 194 numbered assessments made in the conversation. “Investigate” means the release note identifies a behavior worth checking; it does not assert that the rewrite currently has the bug. “Divergence” means the rewrite has an explicit alternative rule or the patch change is a balance/design choice.

| # | Patch reference | Change / issue | Assessment |
|---:|---|---|---|
| 1 | IW017 | Wind Walking stack movement accounting | Investigate mixed movement allowances and which unit determines the stack's moves. |
| 2 | IW030 | Archmage Spell Casting Skill display | Likely avoided structurally; verify the displayed value uses the same calculation as casting. |
| 3 | IW041 | Treasure item value limits | Investigate treasure generation separately from item filtering; the rewrite may intentionally use different bounds. |
| 4 | IW045 | Invisibility target eligibility and detection | Verify melee, ranged and spell targeting all use the same visibility rule, including Illusions Immunity. |
| 5 | IW048 | Human armies entering battle while moving | Investigate whether movement is interrupted cleanly when a battle begins. |
| 6 | IW051 | Fortress defender casting with values above 16,383 | Investigate integer widths in defender casting statistics and budgets. |
| 7 | IW051 | Strategic-combat healing, enhancement and direct damage in all phases | Investigate whether every spell effect is applied in every auto-resolve phase. |
| 8 | IW051 | Strategic-combat mana over-spend | Investigate that the mana budget cannot be exceeded by spell selection or simultaneous effects. |
| 9 | IW052 | Multiple realm bonuses in strategic combat | Investigate whether multiple applicable realm bonuses aggregate rather than overwrite one another. |
| 10 | IW054 | AI Possession targeting Heroes | Investigate target filtering for Heroes and other special unit classes. |
| 11 | IW056 | Counter Magic strength loss | Verify the reserve is tested at its pre-drain strength and loses the intended amount per attempt. |
| 12 | IW056 | Counter Magic removes unrelated enchantments | Verify the dispel only removes the spell or enchantments actually targeted and eligible. |
| 13 | IW061 | Dispel Evil target restriction | Investigate that target validation restricts Dispel Evil to the intended creature realms. |
| 14 | IW062 | Life Drain versus Death Immunity | Investigate that Death Immunity blocks Life Drain in every applicable attack path. |
| 15 | IW063 | AI Healing Spell ability use | Investigate whether the AI recognizes and uses healing abilities when they are useful. |
| 16 | IW066, IW091 | AI ranged strength estimates | Investigate whether ranged attacks are included consistently in friendly and enemy strength estimates. |
| 17 | IW068 | AI ranged attacks against Invisible units with Illusions Immunity | Investigate visibility and target selection for ranged attacks against invisible targets. |
| 18 | IW073 | Demon Skin Defense bonus applied twice | Investigate whether modifiers are applied once across all stat aggregation paths. |
| 19 | IW074 | Hero spell indices above 127 | Likely avoided by typed IDs; verify no byte-sized spell index remains in Hero spell handling. |
| 20 | IW078 | Chaos Channels unit classification for overland spells and Call Chaos | Investigate eligibility and classification for units already changed or unsuitable for mutation. |
| 21 | IW079 | Cause Fear target | Investigate target selection and the actual effect on the intended target. |
| 22 | IW080, IW085 | Thrown and Breath attacks respect ranged To Hit modifiers | Investigate whether thrown and breath attacks share the appropriate accuracy modifiers. |
| 23 | IW081 | Block bonus erroneously reduces enemy To Hit | Investigate that defensive block bonuses do not leak into the attacker's To Hit calculation. |
| 24 | IW084 | Siege automatic +3 Defense in strategic combat | Investigate whether the extra Defense is intentional and consistently represented. |
| 25 | IW088 | Regenerating moves recalled units back | Investigate recall position and whether regeneration incorrectly changes unit placement. |
| 26 | IW093 | Units without Endurance gain extra combat movement | Investigate movement restoration and whether units receive only their proper allowance. |
| 27 | IW089 | Defenders lose strategic movement after combat | Investigate movement reset after battle for units that defended. |
| 28 | IW098 | Charismatic Retort applied before affordability | Investigate whether discounts are applied before or after the affordability check. |
| 29 | IW103 | Combat dispels the target's intended spell rather than the caster's own | Investigate ownership resolution when selecting a spell to dispel. |
| 30 | IW105 | Holy Arms prevents dispelling Holy Weapon | Investigate the distinction between a global protection and an individual unit enchantment. |
| 31 | IW108 | Animate Dead ownership | Investigate that raised units receive the correct owner. |
| 32 | IW112 | Call Lightning ignores Magic Immunity rather than Wraith Form | The rewrite documents a Wraith Form divergence; verify the implementation matches that deliberate rule. |
| 33 | IW128 | Armageddon and Great Wasting cancel town-building pacification | Investigate whether world events incorrectly remove a building's pacification effect. |
| 34 | IW131 | AI fortress defender spell values above 16,383 | Investigate integer widths in the AI's fortress-defender spell valuation. |
| 35 | IW135 | AI research progress | Investigate research allocation and whether progress is retained and advanced correctly. |
| 36 | IW144 | Barbarian Spearmen prevent AI Incarnation | Investigate spell eligibility and whether the AI rejects an otherwise valid summon. |
| 37 | IW146 | AI Heroism, Holy Armor and Black Channels on Fantastic Units | Investigate target filtering for Fantastic Units and other special unit classes. |
| 38 | IW149 | AI Trade Goods restriction above three Towns | Investigate production policy and whether the town-count threshold is desirable. |
| 39 | IW156 | AI movement cancellation when passing a rival city | Investigate pathfinding and diplomacy when a route passes through a rival's territory. |
| 40 | IM176 | Lo Pan receives an extra spellbook | Likely covered by explicit spellbook picks; verify the setup definition. |
| 41 | IM183 | Starting town on an Encounter Zone | Likely covered by map-generation exclusion rules; verify the starting-site validator. |
| 42 | IM197 | AI starting spell grants with Basilisk and Pathfinding for Life | Investigate starting-spell allocation and race/realm-specific grants. |
| 43 | IM197 | AI wizard with 11 [value omitted in pasted note] receives fewer spells | Investigate the allocation boundary; the pasted release note omitted the stat name. |
| 44 | IM200 | Alorra and Torin with Arcane Power | The rewrite deliberately restricts Arcane Power to Heroes with suitable magical ranged attacks; verify definitions. |
| 45 | ITEMDATA.LBX / ITEMMAKE.EXE | Item Spellbook requirements | The rewrite deliberately avoids the faulty per-power book-byte filtering; verify crafting requirements instead. |
| 46 | IW006 | Objective-based Town production | Investigate whether production choices follow the AI's objective and current needs. |
| 47 | IW040 | AI treasure item find | Investigate whether AI treasure rewards use the same generation and eligibility rules as human rewards. |
| 48 | IW042 | Wizard's Pact war declaration | Investigate diplomacy rules around declaring war on a wizard with a Pact. |
| 49 | IW048 | AI pathfinding across player units and hostile tiles | Investigate passability, diplomacy and target handling when routes cross occupied or hostile tiles. |
| 50 | IW049 | AI cannot flee on the first combat turn | Investigate whether the AI can retreat as soon as the battle rules permit it. |
| 51 | IW067 | AI no longer uses lines or group movement | Treat as a tactical-AI design choice; verify whether the rewrite's battle AI needs formations. |
| 52 | IW069 | Melee target prioritizes damaged enemies | Investigate consistency between melee and ranged target scoring. |
| 53 | IW075 | AI upkeep discount stacking | Investigate whether difficulty bonuses and other upkeep modifiers stack correctly. |
| 54 | IW087 | Damaged reachable melee targets prioritized, sleeping targets not | Investigate whether target scoring accounts for damage and incapacitated targets sensibly. |
| 55 | IW092 | Army strength threshold raised from 66% to 75% | Balance choice; do not copy the threshold without testing the rewrite's combat model. |
| 56 | IW095 | AI less likely to raze a city captured from another AI | Investigate whether previous ownership should influence razing decisions. |
| 57 | IW096 | AI razes more when enemy forces are nearby | Investigate whether razing depends on the city's actual defensibility. |
| 58 | IW099 | AI refuses lower-ID Heroes to preserve Hero slots | Investigate recruitment policy; avoid using numeric ID as a proxy for Hero quality. |
| 59 | IW101 | Spell trade priority uses realm index and desirability | Investigate explicit, data-driven spell trade valuation rather than realm ordering. |
| 60 | IW115 | Combat spell choice depends on winning or losing | Investigate context-aware tactical spell scoring. |
| 61 | IW116, IW124 | AI prioritizes dispelling Call Lightning and Wrack | Investigate whether the AI recognizes dangerous active battlefield effects. |
| 62 | IW120 | Various combat spell preferences | Investigate spell-specific tactical desirability, target resistance and matchup. |
| 63 | IW123 | Mana Leak turn and wall conditions | Investigate turn-dependent behavior and siege context for Mana Leak. |
| 64 | IW125, IW130 | Less Immolation threat and protection-spell use | Investigate defensive spell valuation and threat estimation. |
| 65 | IW129 | Mercenary hiring timing, budget and type | Investigate recruitment timing and whether the AI can afford the right mercenaries. |
| 66 | IW137 | AI uses Alchemy before mana is exhausted and values Mana | Investigate resource conversion and reserve planning. |
| 67 | IW139, IW151, IW153 | Settler priorities and reserve | Investigate expansion priorities, settler availability and whether reserves are maintained. |
| 68 | IW139, IW151, IW153 | Engineer priorities and reserve | Investigate engineer recruitment and infrastructure needs separately from settlers. |
| 69 | IW140 | Invalidate saved path when adjacent enemies appear | Investigate whether new threats should cause the AI to reconsider a stored route. |
| 70 | IW141, IW143 | Research priorities | Investigate explicit AI research desirability rather than relying on incidental ordering. |
| 71 | IW142 | Mana target for expensive overland spells | Investigate resource planning for spells that require a large reserve. |
| 72 | IW144 | Summon selection uses creature preference rather than exponential casting cost | Strong candidate for data-driven per-creature AI weights. |
| 73 | IW144 | AI will not Summon Hero with four or more Heroes | Investigate Hero roster caps and recruitment eligibility. |
| 74 | IW145 | AI cancels only Suppress Magic and Time Stop when out of Mana | Investigate upkeep maintenance and which global spells should be cancelled first. |
| 75 | IW146 | AI prioritizes unit enchantments on Heroes, with exceptions for travel spells on Settlers | Investigate spell-specific target suitability. |
| 76 | IW146 | AI prefers Fantastic Units and appropriate ranged units for enchantments | Investigate target suitability and whether the AI understands what each enchantment improves. |
| 77 | IW147 | AI moves the Summoning Circle more often | Investigate Summoning Circle placement and strategic relocation. |
| 78 | IW148 | AI buys production only after progress has been made | Investigate purchase timing and whether immediate buying is ever preferable. |
| 79 | IW150 | AI uses Trade Goods less unless it has at least 24 cities | Balance and economy design choice; test rather than copying the threshold. |
| 80 | IW150, IW152 | Reduced enforcement of home-continent unit production | Investigate whether production should be tied to strategic geography. |
| 81 | IW152, IW153 | Smithy before Builders' Hall; Granary before Shrine | Investigate contextual building priorities and their prerequisites. |
| 82 | IW155 | AI buys production less often, especially in larger cities | Investigate purchase valuation by city size and production progress. |
| 83 | IW158 | Minerals matter more when settling | Investigate mineral value in city-site evaluation. |
| 84 | IW162 | AI abandons small-map continents if it cannot attack | Investigate expansion and attack feasibility on isolated landmasses. |
| 85 | IW164 | AI does not disband units if it can afford 32 turns of upkeep | Investigate whether disbanding uses a sensible financial runway. |
| 86 | IW167 | AI receives no building-upkeep discount | Prefer player/AI parity unless a difficulty setting explicitly says otherwise. |
| 87 | IM177 | AI starting spell picks | Investigate data-driven defaults and whether AI spell picks use the same rules as human picks. |
| 88 | IM185 | AI race choices on high difficulty; later reversed | Design choice; check race restrictions and avoid reproducing a change the Community Patch later reversed. |
| 89 | IM198 | Retorts affect AI Personality and Objective | Investigate configurable wizard-trait effects on AI strategy. |
| 90 | IW001, IW007 | To Hit and To Defend UI | Investigate whether the UI distinguishes stats from abilities and modifiers. |
| 91 | IW002 | Rampaging monster budget change, later reversed | Balance investigation; do not assume the temporary patch change is desirable. |
| 92 | IW003 | Trireme and Galley transport capacity set to eight | Investigate whether ship capacity is configurable in content. |
| 93 | IW009 | Rename Strategic Combat Only to Automatic Combat | UI terminology choice; ensure the label describes what the option actually does. |
| 94 | IW010 | Remove End of Turn Wait | UI/interaction design choice; not necessarily a rules issue. |
| 95 | IW010 | Remove Auto Unit Information | Investigate whether the option is useful and how it maps to the rewrite's UI. |
| 96 | IW010, IW011 | Add Revolting Raiders option | Investigate whether the optional game mode is worth supporting. |
| 97 | IW010, IW011 | Add Monsters Gone Wild option | Investigate whether the optional game mode is worth supporting. |
| 98 | IW010 | Rampaging Monsters begins on turn 40 | Balance and timing choice; test against the rewrite's game pace. |
| 99 | IW013 | Encounter dialog supports four or more guardians | UI edge case; verify long guardian lists do not overflow. |
| 100 | IW014 | Patrol hotkey L | UI convenience feature. |
| 101 | IW016 | Tower of Wizardry tile auto-selection is plane-aware | Investigate map selection and plane context. |
| 102 | IW019, IW020 | City cycling with X | UI navigation feature. |
| 103 | IW024 | Cities overview with T | UI navigation feature. |
| 104 | IW052 | Strategic-combat direct damage scaled by one half and enhancement by one third | Investigate explicit scaling; auto-resolve should use the same rules as tactical combat where possible. |
| 105 | IW053 | Effective HP in strategic combat and AI | Investigate whether health and durability estimates agree with actual battle resolution. |
| 106 | IW057 | Spell Save modifiers apply to Life Drain, Death Spell and Holy Word | Investigate shared resistance/save resolution for these spells. |
| 107 | IW064 | Hero items transfer to the victor after strategic combat | Investigate post-battle ownership and item transfer. |
| 108 | IW070 | Right-arrow hotkey in item creation | UI convenience feature. |
| 109 | IW072 | Treasure/gift item no longer themed to recipient | Investigate whether recipient-themed rewards are a desired generation policy. |
| 110 | IW076 | Conjurer maintenance fraction rounds up from 0.5 | The half-step numeric newtype can represent the value; verify the rounding rule. |
| 111 | IW082 | Units on central tile of a walled Town gain +3 Defense | Investigate tile-specific defensive bonuses. |
| 112 | IW084 | Siege gains +2 with City Walls or Wall of Fire | Investigate the siege bonus and its stacking rules. |
| 113 | IW090 | Chaos Channeled units gain experience | Investigate mutation and progression classification. |
| 114 | IW100 | At most three Arcane spells in the research pool | Investigate research-pool composition and whether the cap is configurable. |
| 115 | IW102 | Chaos Channels does not grant Chaos Breath to low-strength Thrown units | Investigate eligibility; do not grant a breath attack where the underlying unit cannot support it. |
| 116 | IW106 | Merging units are immune to Cracks Call | Investigate target state and immunity during merging. |
| 117 | IW114 | Suppress Magic no longer counters Spell of Return | Investigate spell-specific dispel immunity and context. |
| 118 | IW133 | Hide population growth at 8,000 or more; later reversed in 1.50 | UI behavior; avoid copying a temporary display change without checking the later reversal. |
| 119 | IW134 | Merchant offer threshold is twice item value; AI gets offers and always buys at half price | Investigate merchant economy and distinguish human and AI offer rules. |
| 120 | IW154 | Neutral cities prefer Builders' Hall and Granary | Investigate neutral-city production priorities. |
| 121 | IW165 | Arrow keys flip Apprentice spellbook pages | UI navigation feature. |
| 122 | IW173 | Space quits Save/Load screen | UI hotkey choice. |
| 123 | IM178, IM180, IM186 | Huge land size | Investigate map scalability and configurable world size. |
| 124 | IM178 | Magic settings renamed for clarity | UI clarity and naming. |
| 125 | IM178 | Add higher magic settings, later removed | Design choice; do not treat a reverted option as a requirement. |
| 126 | IM187 | Encounter budgets increased, later reversed | Balance choice; investigate the desired difficulty curve rather than copying the temporary change. |
| 127 | IM188 | Colored lair Chaos monsters at 50%; broken implementation replaced by actual fix | Investigate correct weighted realm composition; the note describes intended 40% Death, 40% Chaos and 20% Nature. |
| 128 | IM189 | Secondary monster divisor maximum decreased | Investigate random ranges and encounter balance. |
| 129 | IM192 | Neutral towns receive one additional close-combat unit | Investigate neutral garrison composition. |
| 130 | IM193 | Neutral cities start with Granary at size six and Stables at size seven | Investigate neutral-city starting buildings and population thresholds. |
| 131 | IM195 | Spell of Mastery research discount for 11 same-realm books changed substantially, later reversed | Balance investigation; do not reproduce a reverted economic change by default. |
| 132 | IW004 | Floating Island loses Death Immunity and gains standard unit flag/race; later fixed | Investigate unit flags and immunities; the patch's intermediate fix was itself defective. |
| 133 | IW005 and related | Extreme difficulty added and Intro removed without adjusting many mechanics | Investigate whether difficulty rules form a coherent set rather than a label-only mode. |
| 134 | IW008 | Endurance Item Power description fails to identify the actual spell | Investigate descriptions and whether item powers point to their actual effect definitions. |
| 135 | IW015 | Nature Awareness auto-casts Detect Magic; later fixed | Investigate unintended coupling between unrelated spell effects. |
| 136 | IW018 | Overland income display inaccurate before movement; recalculation causes late-game lag | Investigate caching and calculation consistency between displayed and actual income. |
| 137 | IW029 | Wraithform item power only works if it is the last Hero in a stack and affects the whole stack; later fixed | Investigate per-unit equipment effects versus stack-wide effects. |
| 138 | IW041 | Loot encounter reseeds RNG to a static AI turn seed, repeating outcomes; fixed in 1.51 | Critical RNG ownership issue; gameplay systems must not reset a shared stream to a fixed seed. |
| 139 | IW043 | Diplomacy against an overwhelmingly strong invader is backwards; later replaced | Investigate relative military-strength comparison and diplomacy direction. |
| 140 | IW043 | AI war declaration against a stronger home-continent rival is backwards; later replaced | Investigate war-declaration strength comparison and home-continent context. |
| 141 | IW043 | Diplomacy crashes if AI has no home-continent units | Handle the zero-unit case explicitly. |
| 142 | IW044 | Dispel Confusion does not always restore original owner | Preserve original ownership in the temporary control state. |
| 143 | IW047 | Engineer road-building fix has no effect | Verify the actual road-building calculation rather than assuming the patch changed it. |
| 144 | IW050 | Strategic-combat units with Defense at or below 50 all receive the same damage chance | The rewrite's auto-resolve uses ordinary battle resolution; verify no simplified Defense cap is introduced. |
| 145 | IW055 | Runemaster dispel modifier additive instead of multiplicative, only in combat; removed in 1.50 | Investigate consistency of dispel modifiers across contexts; do not copy a reverted change. |
| 146 | IW058, IW121 | AI ignores Haste when choosing what to dispel | Investigate whether the AI recognizes Haste as a high-value target enchantment. |
| 147 | IW059 | Vortex cap reduced to four despite original limit of ten | Investigate summon caps and whether the reduced limit is intentional. |
| 148 | IW063 | AI Doom Bolt animation misaligned | UI/animation issue; ensure spell effects and animation targets agree. |
| 149 | IW065, IW067 | AI melee line advance disabled because another change removes lines | The changes interact; evaluate tactical behavior as a whole rather than individually. |
| 150 | IW070 | Space exits crafting screen and right-arrow exit removed; later reversed | UI design choice; avoid adopting an intermediate hotkey regression. |
| 151 | IW077 | Wind Mastery ship movement differs by player and combat context | Investigate context and ownership consistency for global movement effects. |

| 152 | IW083 | Weapon Immunity no longer overrides Missile Immunity but Defense set to 50 instead of 10 for non-melee; fixed in 1.51 | Investigate immunity precedence; avoid numeric fallback values as substitutes for correct immunity logic. |
| 153 | IW086 | AI melee targets exactly reachable are deemed unreachable; later fixed | Investigate movement-range boundary conditions. |
| 154 | IW094 | Lawful AI avoids razing positive relations but still razes at zero; later fixed | Investigate threshold comparisons and neutral-relation behavior. |
| 155 | IW097, IM179 | Overland Spell Events disabled by default but setting has no effect | Verify the setting is wired through to the event system. |
| 156 | IW099 | Fame contribution cap changes | Investigate recruitment affordability and Retort cap interactions. |
| 157 | IW104 | Spell Lock combat-cast dispel protection fix failed | Investigate Spell Lock protection in battle casting and dispel paths. |
| 158 | IW105 | Invulnerability remains protected from dispelling in combat | The rewrite documents a deliberate divergence allowing combat dispel; keep the divergence explicit and tested. |
| 159 | IW105 | Possession can be dispelled by its caster to retain control | Investigate ownership semantics and whether dispelling should restore or retain control. |