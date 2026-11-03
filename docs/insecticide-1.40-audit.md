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