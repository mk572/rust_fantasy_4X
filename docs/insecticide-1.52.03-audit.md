# Insecticide 1.52.03 audit against the Rust rewrite

**Repository:** [mk572/rust_fantasy_4X](https://github.com/mk572/rust_fantasy_4X)  
**Purpose:** Assess the Insecticide Community Patch 1.52, 1.52.02 and 1.52.03 release notes against the Rust rewrite's rules, architecture and implementation priorities.  
**Scope:** This is a compatibility and design audit, not a mandate to reproduce every historical DOS implementation detail. The later 1.52.03 notes supersede earlier behavior where they explicitly fix or reverse it. The release also makes several earlier Insecticide changes optional or restores v1.31 behavior; those reversals are important compatibility decisions.

## Executive summary

The 1.52 series is especially useful for identifying places where the rewrite should prefer robust underlying rules over reproducing patch-specific workarounds:

- **Deterministic world state:** Random-event scheduling must be independent of save/load boundaries. Terrain changes and city razing must update movement costs through the same derived-terrain mechanism.
- **Movement and pathfinding:** The patch explicitly replaces several earlier pathfinding fixes with Dijkstra's shortest-path algorithm. The rewrite should use a cost-aware shortest-path implementation rather than accumulating special-case pathfinding patches.
- **Trait generation:** Random ability selection must never consume a pick without granting a trait, grant two mutually exclusive variants for one pick, or depend on a single hard-coded hero.
- **Content-driven world generation:** Terrain graphics, encounter rewards, creature realms, treasure budgets and starting-city suitability should be defined through data and explicit constraints.
- **AI safety and legality:** AI target selection must use the same live, legal target queries as gameplay, including ownership, status, visibility, battle position and capacity.
- **Compatibility is not always the latest patch:** Several 1.52 changes reverse Insecticide changes or restore v1.31 behavior. The rewrite should preserve these as named ruleset choices where they are genuine balance preferences, while adopting crash fixes and correctness invariants as defaults.

## Interpretation and priority

“Resolved” means the inspected implementation has a matching rule or architecture. “Partial” means the main behavior exists but a material edge case or policy remains incomplete. “Investigate” means the release note identifies behavior to verify; it does not claim the rewrite has the bug. “Design” marks a balance or compatibility choice rather than a correctness requirement. “UI” marks presentation or interaction work.

**Priority scale:** P1 = crash, memory-safety, corrupted state, or deterministic-state risk; P2 = high-impact rules, movement, spell behavior, world generation or AI correctness; P3 = normal gameplay, economy, balance and secondary behavior; P4 = UI, presentation and convenience.

## v1.52.03 release-note assessment

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 810 | Random Events may occur during the first five turns after entering a game from the main menu; save/load no longer causes events to occur more frequently than intended | Separate event eligibility from event scheduling. A load must restore the exact random-event schedule/state and must not reset an entry-turn suppression counter or reroll an already resolved event. Whether events are allowed in turns 1–5 is a ruleset policy; save/load frequency invariance is a correctness requirement. | P1 | Investigate |
| 811 | Node bonus auras correctly apply in combat again; fixes regression from 755 | Battle setup and effective-stat calculation must include node aura effects exactly once, using the same source of truth as overland/node state. Add regression coverage for the specific aura and combat entry path. | P2 | Investigate |
| 818 | Razing a City restores the tile's movement costs; terrain alteration updates movement costs | Movement cost must be derived from the current tile terrain and city/road/feature state, not left stale after a mutation. Centralize tile-state changes and invalidate/recompute path costs after razing or terrain alteration. | P2 | Investigate |
| 801, 806 | Chaos Warrior can roll Super Arcane Power as a random ability; Arcane Power can upgrade normally; eligible Heroes no longer receive both normal and super versions | Random trait generation needs explicit upgrade/exclusion semantics. A pick must resolve to exactly one valid outcome; upgrading an existing trait must replace it rather than append a second trait. | P2 | Investigate |
| 818 | Change Terrain and Gaia's Blessing can use all available Grassland graphics | Terrain type and terrain graphic variant should be distinct data fields. Spell-driven terrain changes should select from all legal variants rather than assume one fixed graphic. | P3 | Investigate |
| 808 | AI can use empty Towers of Wizardry last used by an allied player | Tower access/availability must be based on current ownership, alliance and occupancy rules, not stale last-user state. Clarify whether the tower remains allied/usable after the previous visit and share the same eligibility query with the human UI. | P2 | Investigate |
| 812 | AI Raise Dead/Animate Dead no longer resurrects units on corner tiles of City Walls | Resurrection target enumeration must exclude invalid wall/corner positions and use legal battle-unit positions, not raw tile iteration. Validate the resulting unit placement before insertion. | P1 | Investigate |
| 817 | AI can target Transmute without crashes or memory corruption | AI spell target construction must validate target type, index and current state before evaluation and cast. Avoid retaining references/indices into collections that can change during spell evaluation. | P1 | Investigate |

## v1.52.02 release-note assessment

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 799 | Map generator can use every intended Tundra graphic | Keep terrain identity separate from graphic variant and make the generator's eligible variant set data-driven. | P3 | Investigate |
| 800 | Hill tiles with Terrain Specials are no longer incorrectly converted to Grassland | Terrain-special placement must not overwrite the underlying terrain type unless the special explicitly transforms it. Treat terrain and special features as separate layers. | P2 | Investigate |
| 801, 806 | Chaos Warrior random ability; new Arcane Power pick-loss/double-grant regression; 806 corrects it | Treat 801 as an intermediate regression, not the final intended behavior. Implement the corrected 806 semantics: one pick grants one eligible trait or upgrades the existing trait, never both variants. | P2 | Investigate |
| 801 / IM200 | Heroes without Ranged Magical Attack cannot roll Arcane Power; all Heroes with it become eligible, replacing a one-hero-specific exception | Eligibility should be expressed as a capability/trait predicate, not a hard-coded hero identity. Verify that modded Heroes participate through the same predicate. | P2 | Investigate |
| 802 | Overland spells can be cast and Mana channeled on turn one | Do not add an arbitrary first-turn lockout to spell casting or power allocation. Validate the actual game-state prerequisites instead. | P2 | Investigate |
| 805 | Lycanthropy sound playback no longer glitches | Audio playback must be decoupled from simulation and tolerate missing, repeated or interrupted sound resources. | P4 | UI |
| 803 | AI secondary realms are correct when primary realm is Nature | Wizard generation must select primary and secondary realms from one validated combination, without a Nature-specific defaulting error. | P2 | Investigate |
| 795, 798 | Previous Icon button works after cumulative compatibility fixes from 761 | Treat item-icon navigation as ordinary UI state and test wraparound/previous selection after item edits and screen transitions. | P4 | UI |
| 796 | Keyboard cannot move units selected on the plane not currently in view | Input commands must validate that the selected unit is on the active plane and currently actionable. The UI selection state must not bypass world-command legality. | P2 | Investigate |
| 797, 004 | Wizard customization displays and selects Spellbook realms in consistent order; supersedes 004 | Define one canonical realm ordering and reuse it in wizard creation, profile display and other realm lists. Do not maintain separate presentation orderings. | P4 | UI |
| 804 | Long Retort names do not hide selected Retorts in wizard profile overview | UI layout must accommodate the supported text length and selected-item count without clipping or suppressing entries. | P4 | UI |

## v1.52 release-note assessment

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 767, 283 | Rampaging Monsters cannot destroy a player's Fortress Town again; reverses a change that could corrupt memory | Fortress destruction must be blocked by a central, explicit encounter/raider target-legality rule. This is also a lifecycle safety invariant: no world event should partially destroy the capital or leave ownership/state inconsistent. | P1 | Investigate |
| 775, 162 | Capturing towns yields intended Gold again | Loot should be calculated from a defined city snapshot and the intended capture rule, with zero-population and razed-city cases handled safely. Verify against the earlier 1.50 audit item 162 rather than assuming its formula. | P2 | Investigate |
| 778 | Abandoned Keeps, Dungeons, Monster Lairs and Mysterious Caves use intended documented creature realms | Encounter content should declare eligible realms in data. Do not infer realm from lair type through scattered special cases. | P2 | Investigate |
| 779 | More expensive Encounter Zone guardians are always the primary/scoutable guardians | Separate the primary guardian selection from the remaining encounter roster, and ensure scouting reveals only the intended information. | P3 | Investigate |
| 780 | Tower of Wizardry guaranteed spell respects treasure budget; rarity is lowered to fit | Guaranteed rewards must still pass budget constraints. Implement a deterministic rarity fallback that selects the highest eligible rarity within the remaining budget. | P2 | Investigate |
| 781 | Multiple spell treasures check each rarity against remaining budget instead of summing or discarding them | Reward generation should decrement a shared remaining budget after each reward and re-evaluate the next reward against it. Avoid both over-budget rewards and discarding valid lower-rarity rewards. | P2 | Investigate |
| 790 | Small Node/Tower islands can extend to intended size | Map generation should distinguish island seed placement from subsequent land growth and avoid treating special-site occupancy as a blanket growth prohibition. | P3 | Investigate |
| 791 | Starting Town selection correctly accounts for Swamp and Nature Node Food/Maximum Population | Starting-site suitability must use the same effective food and population calculations as the live city simulation. Avoid duplicate approximate formulas in map generation. | P2 | Investigate |
| 792 | Units created at game start no longer receive occasional extra Experience | Starting-unit initialization must use a deterministic, explicit experience value and must not inherit stale/randomized runtime state. | P2 | Investigate |
| 758, IM195 | 11-book Wizards no longer get disproportionate Spell of Mastery research discount | Research cost modifiers should be applied through a shared, bounded calculation. Test maximum book counts and ensure each discount is applied once. | P2 | Investigate |
| 770 | Armageddon and Great Wasting increase Unrest to compensate for not negating building pacification | City unrest should be calculated from explicit spell and building effects. Do not encode the old behavior as a hidden suppression of building effects; test the combined net unrest. | P2 | Investigate |
| 784 | Webs can be damaged by Ranged Magical rather than only Ranged Missile attacks | Web damage eligibility should use attack traits/nature, not a narrow attack-category check. Keep the valid attack types explicit. | P2 | Investigate |
| 028 | AI again gets a guaranteed non-starting Uncommon Summoning Spell in its primary Realm | Distinguish starting spell grants from AI assistance/reward grants. Make the guarantee explicit and ensure it does not accidentally consume or duplicate a starting spell. | P3 | Investigate |
| 765 | AI no longer counts various situational abilities when calculating unit strength | Unit valuation should distinguish always-on strength from conditional ability value. Avoid double-counting situational abilities when their conditions are not met; this is an AI valuation refinement. | P2 | Investigate |
| 772 | AI can use flying “stall” tactic when attacking an enemy Fortress | Tactical AI needs a fortress-specific evaluation of flying units and siege constraints. Do not let generic path/attack scoring discard a valid stall tactic. | P2 | Investigate |
| 782 | AI does not use Healing on sleeping or shattered units | Healing target queries must filter units that cannot benefit from the spell, including status conditions that prevent healing. Share the eligibility predicate between AI scoring and cast validation. | P2 | Investigate |
| 783 | AI can recognize up to 200 land-attack targets per continent instead of 25 | Replace arbitrary small fixed target arrays with dynamically sized collections or a documented capacity safely above supported map requirements. Preserve deterministic target ordering. | P2 | Investigate |
| 787 | Oberic may combine Spellbooks into one Realm at high difficulty like other Wizards | Wizard-generation rules should be consistent across wizard identities unless a specific character rule says otherwise. Avoid a one-off exclusion. | P3 | Investigate |
| 793 | AI no longer automatically detects Invisible units on the gate tile in non-siege battles | Visibility and detection must be determined by the same rules for all battle positions; the gate tile must not grant an accidental detection exception. | P2 | Investigate |
| 794, 031 | Sss'ra selects Realms randomly rather than abandoning Myrran to avoid higher difficulty; replaces prior behavior | Wizard profile generation should not silently discard a selected retort to satisfy difficulty. Treat realm/retort selection as one validated profile-generation process. Randomness must be seeded and reproducible. | P3 | Investigate |
| 756, 776 | Escape again closes windows | Keyboard shortcuts should be contextual and consistent. Escape should cancel/close transient UI layers unless a modal action explicitly captures it. | P4 | UI |
| 786 | Wizard customization can reset after spell selection even when there were no spells to select | Wizard customization should have a valid reset path from every stage, including empty selection states. | P4 | UI |
| 788 | Myrran Retort selection shows the correct error when fewer than three picks remain | Validate pick cost before applying the selection and present the actual unmet requirement. Keep validation and UI messaging based on the same rule. | P4 | UI |
| HELP.LBX | Context help revised to match current game state | Help text should be versioned with the ruleset and generated/maintained alongside rule definitions where possible. It must not describe obsolete behavior as current. | P4 | UI |



## Optional

These entries are selectable alternatives or compatibility variants, not mandatory baseline behavior.

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 768, IM181 | Node Power options 2.0 and 2.5 removed from defaults, retained as optional patch | Node influence power is a balance/configuration parameter. Keep the baseline values as the default and expose higher values only through a ruleset option. | P3 | Design |
| 773 | Experimental Swamps-in-combat feature becomes optional | This is a presentation/battle-map experiment with gameplay-adjacent terrain consequences. Keep it disabled by default and selectable by ruleset or battle presentation settings. | P3 | Design |
| 777, 013, 016–025, 537 | Encounter Zone and Treasure generation restored to v1.31 baseline; combined Insecticide/Unofficial Patch modifications become optional | Keep encounter creature budgets, treasure budgets, difficulty scaling, reward eligibility and reward weights as separate stages. The default baseline should not penalize lower difficulty by reversing the intended difficulty relationship. Expose the combined higher-budget variant as an optional ruleset. | P2 | Design |
| 762, IW076 | Conjurer upkeep reduction rounds up again; favorable rounding is now optional | Upkeep rounding is a balance rule. Define the exact rounding mode in ruleset data; default to the restored v1.31 behavior and offer the alternative as an option. | P3 | Design |
| 771, 105 | Defeating/banishing a Wizard again allows the victor to loot half remaining Mana; no-loot variant is optional | Defeat loot is a diplomacy/economy rule, not an incidental side effect of banishment. Default to the restored behavior for this patch line and expose the no-loot variant as an option. | P3 | Design |
| 763 | Starting Relations restored to v1.31; 1.50 formula becomes optional | Starting relation formula is a balance/personality policy. Keep both formulas available as named ruleset choices; make the restored formula the baseline for this patch line. | P3 | Design |
| 757, IM185 | AI restricted to selected Arcanian races on highest difficulties again; unrestricted race selection becomes optional | AI race selection by difficulty is a balance/identity policy. Support both pools through data/configuration rather than hard-coded difficulty branches. | P3 | Design |
| 759, IW049 | Enemies may flee during the first combat turn again; no-first-turn-flee is optional | First-turn fleeing is a combat balance rule. Make the restriction configurable, defaulting to the restored behavior. Ensure flee legality is still validated by battle state. | P3 | Design |

### Optional changes bundled with v1.52

The following are not default behavior in v1.52. They should be assessed independently and represented as ruleset options where appropriate.

| Patch reference | Optional change | Recommendation | Rationale |
|---|---|---|---|
| IM187, 013, 017, 537 | Greatly increase Encounter Zone budgets, producing more monsters and more loot | Optional setting | A major encounter difficulty and reward-density change. Keep budgets separate from treasure budgets and allow a ruleset to select the expanded variant. |
| 016 | Fix monster-to-treasure ratio at 100% on Arcanus and 125% on Myrror, removing reverse difficulty adjustment | Optional setting | Changes reward density and difficulty scaling. Preserve the v1.31 baseline as default; make the alternative formula explicit. |
| 019 | Increase Gold and Mana treasure amounts | Optional setting | Direct economy/balance adjustment. |
| 020 | Increase maximum magical-item value and reduce its budget cost by 17% | Optional setting | Changes item reward value relative to encounter difficulty; keep value and budget cost distinct. |
| 022 | Make Spell Treasure cost substantially more budget | Optional setting | Changes the reward mix and scarcity of spells; expose the budget weight. |
| 023, 025 | Redistribute treasure weights: fewer Special Treasures and slightly fewer items, more Gold, Mana and spells | Optional setting | Reward-category weights are a balance profile and should be data-driven. |
| 024 | Reduce Special Treasure budget cost and stop it from discarding all other treasure | Optional setting | Changes reward composition and expected value; implement as a separate reward-selection policy. |
| IW049 | Prevent enemies from fleeing during the first combat turn | Optional setting | First-turn flee restriction is a combat balance choice; default behavior in v1.52 permits fleeing. |
| IW076 | Round Conjurer's reduced upkeep down for fractions of 0.25 and 0.5 instead of up | Optional setting | Explicit alternative rounding policy; keep exact arithmetic and rounding mode configurable. |
| IM181 | Allow Node Power per influence tile to be 2.0 and 2.5 | Optional setting | Additional node-power balance options, not baseline values. |
| IM185 | Allow AI to select any Race at the highest difficulty settings | Optional setting | Changes high-difficulty AI composition; preserve restricted race pools as the default for this patch line. |
| 027 | Use revised Starting Relations alignment formula: Life/Nature good, Death/Chaos evil | Optional setting | A broad diplomacy balance change. Keep the v1.31 starting-relation formula as baseline and make the alignment formula a named alternative. |
| 105 | Do not loot half of a defeated/banished Wizard's remaining Mana | Optional setting | Alternative defeat-economy rule; keep the restored half-loot behavior as baseline. |
| 774 | Represent Swamps in combat by converting half the tiles to River tiles with a different animation | Skip by default; optional experiment only | Experimental visual/battlefield transformation with potential tactical implications. Do not include in the core terrain model unless its battle rules are fully specified and tested. |
| 785 | Allow Immolation to damage Webs | Make default | This is a coherent attack-interaction correction: Web damage eligibility should include Immolation as an explicit damage source. |
| HOTKEY.TXT | Change Space/Escape and several other keyboard shortcuts | Optional setting | Keyboard layouts are user-facing preferences and compatibility choices. Define a central, remappable shortcut table rather than baking these historical changes into gameplay. |

## Cross-version conclusions and implementation guidance

### 1. Prefer derived terrain and movement costs

The 818 changes are a strong argument against storing independently mutable movement costs on tiles. Store terrain, roads, city occupancy and other movement-affecting features as authoritative state, then derive the effective movement cost through one query. Mutations such as razing a city, Change Terrain and Gaia's Blessing should update the underlying state and invalidate any cached path data.

Terrain graphics should also be separate from terrain identity. A Grassland tile can select among multiple graphic variants without changing its movement, food or production rules. Terrain Specials should be a separate feature layer and must not accidentally replace Hill or other base terrain.

### 2. Make pathfinding a shortest-path problem

The 1.52.03 release explicitly obsoletes earlier pathfinding patches 371, 411, 487, 489 and IW140 in favor of Dijkstra. The Rust rewrite should use Dijkstra (or another algorithm proven equivalent for non-negative edge costs) over the actual legal movement graph. Each edge must account for the moving stack's capabilities, terrain costs, roads, water/transport transitions and any relevant restrictions. Pathfinding and movement execution must share legality and cost calculations so a route is neither falsely rejected nor impossible to execute.

If the rewrite later uses A*, it must preserve optimality with an admissible heuristic and be tested against Dijkstra as a reference implementation.

### 3. Treat random events and random trait generation as deterministic state machines

Random-event timing must be saved as explicit game state or derived from a persisted deterministic schedule. Loading must not reinitialize event counters or consume additional gameplay RNG. Random trait generation should operate on a candidate set with explicit prerequisites, exclusions and upgrade rules. Each pick should produce exactly one result, and the generated result should be validated before committing it.

### 4. Keep encounter and treasure generation modular

The 1.52 baseline restores v1.31 encounter and treasure behavior while making the combined Insecticide/Unofficial Patch changes optional. Keep these stages separate:
1. Encounter-zone selection and creature budget.
2. Guardian selection, including primary/scoutable guardians.
3. Creature realm eligibility by lair type.
4. Treasure budget and difficulty adjustment.
5. Reward category weights and category-specific costs.
6. Reward selection with remaining-budget validation.

This makes it possible to support both the v1.31 baseline and optional expanded rewards without tangled conditionals.

### 5. Share target eligibility between AI and gameplay

The AI fixes for Raise Dead, Animate Dead, Transmute, Healing, Invisible units and Tower access all point to the same architectural rule: AI should query legal actions and legal targets from core rules, not recreate partial target filters. Validate target identity and state at the moment of execution as well as during AI evaluation. Avoid stale indices when collections can be mutated.

### 6. Keep historical reversals as named ruleset policies

Not every later patch is automatically the right default for a clone. The following should be explicit policy/configuration points rather than scattered constants:
- Starting Relations formula.
- Conjurer upkeep rounding.
- Wizard defeat Mana loot.
- Encounter and Treasure budget/weight profile.
- Node influence power choices.
- AI race pools at high difficulty.
- First-turn enemy fleeing.
- Entangle versus Non-Corporeal units.
- Swamp battle-map representation.
- Keyboard shortcuts.

The 1.52 behavior is the compatibility baseline for this audit; optional variants should be selected by a ruleset or game setting.

## Recommended follow-ups

1. Add deterministic save/load tests for Random Event timing, including loading immediately before and after an event check.
2. Centralize terrain-derived movement costs and test city razing, terrain alteration, Change Terrain and Gaia's Blessing.
3. Implement/verify Dijkstra pathfinding against a reference shortest-path test suite, including mixed movement modes and transport.
4. Add property-based tests for random ability picks: one pick, one trait; no duplicate normal/super variants; eligibility derived from traits/capabilities.
5. Audit all resurrection and spell-target queries for legal battle positions, live units, status, ownership and safe collection access.
6. Replace fixed AI target capacities with dynamic collections or explicit bounded capacities with safe overflow behavior.
7. Separate encounter and treasure generation into the pipeline described above, with named v1.31 and optional expanded profiles.
8. Add ruleset-level tests for all reversals and optional balance policies before choosing a default for the Rust game.

## Reverted / Obsolete

These entries describe earlier patch behavior that this release reverses, replaces or makes obsolete. Do not track the superseded implementation as a separate task; implement the later baseline or replacement.

| Patch reference | Change / issue | Assessment | Priority | Status |
|---|---|---|:---:|---|
| 816 | Overland pathfinding uses Dijkstra; reverses/obsoletes 371, 411, 487, 489 and IW140 | Use a cost-weighted shortest-path algorithm over legal movement edges. This supersedes the earlier collection of pathfinding workarounds: do not implement those older patches as independent path rules. Ensure the edge-cost model matches actual movement, terrain, roads, ships, transport and movement capabilities. | P2 | Partial |
| 769, IW002 | Rampaging Monster costs restored to v1.31 values, distinct from Encounter Zone costs | Keep raider and encounter creature budgets separate. The restoration is a balance choice; do not unify these values merely because both systems generate monsters. | P3 | Design |
| 760, 045 | Entangle again affects Non-Corporeal units; reverses 1.50 fix | This is a rules compatibility reversal. Keep target eligibility explicit in spell data/rules so the chosen baseline can be tested and optionally changed; do not assume Non-Corporeal universally implies Entangle immunity. | P2 | Design |
| 761, IW070 | Previous Icon button restored; reverses earlier Insecticide change | Restore ordinary previous-icon navigation. This is a UI compatibility choice with no rules impact. | P4 | UI |
