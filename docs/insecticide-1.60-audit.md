# Insecticide 1.60 audit against the Rust rewrite

**Repository:** [mk572/rust_fantasy_4X](https://github.com/mk572/rust_fantasy_4X)  
**Purpose:** Assess the Insecticide 1.60 release notes as requirements and design guidance for the Rust rewrite.  
**Scope:** This is a rules, architecture and feature audit, not a requirement to reproduce the DOS implementation or every historical patch. v1.60 consolidates a large number of earlier patches into named patch features and world-generation settings. Those current controls take precedence over the historical patch IDs they obsolete.

## Executive summary

v1.60 is a major architectural milestone. It is less a conventional balance patch than a transition from a collection of individually applied patches to a configurable game with explicit world-generation policies, patch features, and dynamic key bindings.

The most important conclusions for the rewrite are:

- **Treat world generation as a constraint problem.** Starting cities, towers, nodes and encounters have different placement constraints. Place the most constrained objects first, backtrack or relax only explicitly relaxable constraints, and report when a map can only be completed by relaxing one. Keep the legacy 1.31 placement order as a named compatibility mode, not as a second collection of scattered conditionals.
- **Separate the generator from its configuration.** Land size, terrain ratios, rivers, roads, minerals, nodes, towers, encounters and neutral cities all expose tunable settings. These belong in a typed map-generation configuration that is saved with the game and can be validated before generation.
- **Make patch features first-class ruleset configuration.** The new Patch menu and the explicit feature list establish a clear distinction between the baseline rules and optional compatibility/balance variants. A feature should have a stable string key, a default, a description and validation rules. Do not identify behavior by obsolete patch numbers.
- **Keep generation deterministic and testable.** The full-cycle random feedback change is a reminder that random-number quality and reproducibility matter. Use a documented PRNG with explicit state/streams and seed handling; test distribution and repeatability rather than copying an opaque legacy feedback function.
- **Use one source of truth for derived world state.** Terrain transitions, city population limits, starting-city food suitability, movement costs, river outflows and continent connectivity should derive from authoritative tile/world data.
- **Make campaign configuration immutable.** The release explicitly moves Revolting Raiders and Monsters Gone Wild to patch features that must be selected when creating a game. The rewrite should snapshot ruleset and world settings at new-game creation and retain them for the entire campaign.
- **Treat UI settings and key bindings as data.** Dynamic key bindings, duplicate-key policy, context-specific controls, multi-page settings and optional pointer behavior call for a proper input-binding system rather than hard-coded key checks.
- **Do not confuse the editor and runtime.** The in-game item editor replaces ITEMMAKE.EXE; the rewrite should use validated content editing and avoid importing malformed item data into the live ruleset.

## Interpretation and priority

The release notes describe fixes in Insecticide, not necessarily bugs present in the Rust rewrite. Therefore, unless a behavior is an explicit invariant, the assessment below uses **Verify** rather than claiming the rewrite is already affected.

**Status meanings**
- **Design:** A rules/configuration decision; implement the underlying mechanism and select the desired default.
- **Verify:** Inspect the implementation and add a focused regression test. The release note alone does not prove the Rust rewrite has the same defect.
- **Architecture:** A system boundary or data-model requirement.
- **UI:** Presentation, interaction, input or audio behavior.
- **Optional:** Must be selectable rather than silently made the only behavior.
- **Not applicable:** A DOS-specific implementation constraint with no direct Rust equivalent.

**Priority scale:** P1 = corrupted state, nondeterminism, invalid generation or serious crash risk; P2 = core world generation, rules, AI or campaign correctness; P3 = balance, content, secondary behavior; P4 = UI, presentation, convenience or platform-specific compatibility.

## 1. World generation and world state

| Change | Assessment for the rewrite | Priority | Status |
|---|---|:---:|---|
| Full-cycle feedback function for random-number generation | Use a well-defined PRNG with a documented period and state transition. Seed it explicitly and make generation reproducible from the seed plus configuration. Prefer separate deterministic random streams for map topology, terrain, sites, encounters and neutral cities so a change in one subsystem does not arbitrarily reshuffle every later result. Test repeatability, state save/restore and statistical quality. Do not copy a legacy feedback function without understanding its guarantees. | P1 | Architecture |
| Towers connect to land at both ends where possible, otherwise at least one end; avoid unnecessary proximity | Model tower placement as a constrained site-and-connection problem. Score candidate sites by land connectivity, distance from other towers and remaining feasible placements. “Both ends where possible” is a preference; “at least one end” is the fallback requirement. Keep the no-unnecessary-proximity rule explicit and test maps with narrow continents and islands. | P2 | Verify |
| Towers remain at least four tiles from Nodes | Add a shared minimum-distance constraint between tower and node sites. Measure distance using the generator's declared map-distance metric, including wraparound behavior. | P2 | Verify |
| Nodes are placed on land whenever possible | Prefer legal land candidates, but define the fallback if no land site satisfies all other constraints. This is a placement preference with a clear fallback, not an unconditional requirement to fail generation. | P2 | Verify |
| Mandatory objects placed most-restrictive-first by default | Build a placement phase that computes candidate sets and orders object classes by constraint tightness (starting cities, towers, nodes, encounters, or finer-grained per-object ordering). Recompute feasibility as placements are committed. Avoid relying only on a fixed order if individual instances have different constraints. | P1 | Architecture |
| Encounter Zones are non-adjacent unless required to finish map generation | Treat non-adjacency as a hard preference that may be relaxed only when the generator otherwise cannot complete mandatory placement. Record/trace when the relaxation occurs. Do not allow adjacency as an ordinary placement choice. | P2 | Verify |
| Encounter Zones are not adjacent to starting Cities | Add a separate exclusion radius/adjacency rule for starting-city tiles. Do not conflate this with encounter-to-encounter spacing; the two constraints may have different relaxation policies. | P2 | Verify |
| Nodes, Towers and their generated islands do not merge into polar caps | Make polar-cap exclusion part of the island-growth boundary conditions. Test wraparound, map edges and small maps; do not rely on a post-generation visual correction that can disconnect or overwrite sites. | P2 | Verify |
| Rivers do not end abruptly without outflow tiles | Represent river flow as a connected path/graph with valid downstream termination. Validate every river segment after generation and ensure it reaches an allowed outflow, lake, sea or map boundary. | P2 | Verify |
| Tundra adjacent to rivers receives correct terrain transitions | Derive transition graphics from the final neighboring terrain/river topology after all terrain and river mutations. Keep visual transition selection separate from logical terrain identity. | P3 | Verify |
| Starting Cities always have Maximum Population of at least 5 | Starting-site scoring and city initialization must share the same effective maximum-population calculation. Reject or repair a candidate before it becomes a starting city if it cannot meet the minimum. | P2 | Verify |
| High Elf starting Cities are always on Forest | Express race-specific starting terrain suitability as data/configuration. Verify that the chosen tile is Forest after all terrain transformations, not merely when the candidate is first selected. | P2 | Verify |
| Neutral Cities respect terrain-imposed Maximum Population | Use one city-capacity calculation for neutral and player cities. Do not give neutral-city initialization a shortcut that bypasses terrain modifiers. | P2 | Verify |
| Barbarian Spearmen may appear in initial neutral garrisons | Ensure the neutral garrison roster is data-driven and includes the intended unit eligibility. This is a content/roster correction, not a special-case random roll. | P3 | Verify |
| Halfling, Beastmen and Nomad neutral-city garrisons do not become too large above a population threshold | Verify the population-to-garrison scaling boundaries for these races. Use a single documented scaling function and table-driven tests at, below and above each threshold. | P2 | Verify |
| Encounters eligible for two picks' worth of special treasure receive both picks | Represent treasure eligibility and pick count explicitly. Do not collapse a multi-pick reward to a boolean “special treasure eligible” flag. Test that the reward resolver consumes and grants the exact number of picks. | P2 | Verify |
| Random items receive the intended number of Powers | Store the intended power count as an invariant of item generation. Validate after generation and after loading/importing edited items. Do not let unavailable or rejected powers silently reduce the count unless the selected ruleset explicitly allows that. | P2 | Verify |

### World-generation architecture recommendation

Implement generation as a pipeline with explicit inputs and validation:

1. Resolve the selected ruleset and patch-feature set.
2. Validate the map settings and legacy-mode eligibility.
3. Generate base topology and land/sea layout.
4. Generate terrain elevations, regions, rivers and polar caps.
5. Derive terrain transitions and other presentation data.
6. Build candidate sets for starting cities, towers, nodes, encounters and neutral cities.
7. Place mandatory objects using the selected placement policy.
8. Generate island expansions, roads, minerals and garrisons.
9. Validate all global invariants: connectivity, distances, river outflows, city capacity, object counts and plane constraints.
10. Emit a generation report with seed, settings, chosen fallback/relaxations and validation results.

A failed mandatory constraint should produce a controlled retry or a clear generation failure. Avoid silently producing a map that violates an invariant.

## 2. Combat, wizards, spells and items

| Change | Assessment for the rewrite | Priority | Status |
|---|---|:---:|---|
| Normal difficulty no longer permits fleeing without losses | Flee outcome and losses should be calculated from one combat-resolution rule, parameterized by difficulty if appropriate. Test Normal and neighboring difficulties, including retreat paths and edge cases where no unit can escape. | P2 | Verify |
| Famous Retort treasure grants its 10-point value | Treasure generation and reward application must agree on the Retort's value. The release text appears to have lost a currency/icon glyph; preserve the numeric value only after confirming the intended resource from the source data. | P3 | Verify |
| Artificer Retort treasure grants the corresponding Item Crafting spells if not already known | Model treasure-granted spells as a set of eligible unlocks and filter known spells before awarding. Ensure the reward does not duplicate spells or consume a reward slot without granting anything. | P2 | Verify |
| Divine Power and Infernal Power cannot be awarded to Wizards with opposite Spellbooks | Put mutual-exclusion requirements in Retort eligibility data and use the same predicate for player selection, AI generation and treasure rewards. | P2 | Verify |
| Retorts reducing crafting spell Casting Cost do not make spells easier to counter | Separate casting-cost modifiers from countering/resistance calculations. A modifier to one cost domain must not leak into unrelated spell-resolution values. | P2 | Verify |
| Computer Wizards start with no more than six Retorts | Enforce the cap in the shared wizard-profile generator, including treasure/profile import paths. Validate after all profile bonuses and difficulty adjustments. | P2 | Verify |
| Computer players can disband obsolete units after loading from the main menu | AI turn initialization must not depend on transient state that is lost or reset during load. Test AI turn behavior immediately after load through each supported load path. | P2 | Verify |

## 3. User interface, input and settings

| Change | Assessment for the rewrite | Priority | Status |
|---|---|:---:|---|
| City rename and city cycling from City Screen | City-screen navigation should expose the same city identity and selection commands as the overland view. Renaming should be an explicit action with validation and immediate state/UI refresh. | P4 | UI |
| Enchantment list scroll wraps correctly at exactly 12, 18 or 24 active enchantments | Test pagination at exact page multiples, one below and one above each boundary. Use a collection-driven page model rather than special-case counts. | P4 | UI |
| Food, Gold and Mana income displays are accurate | Derive UI values from the same economy calculation used by the simulation. Avoid separately approximating income in the presentation layer. | P3 | Verify |
| No selected units no longer causes late-game frame-rate/responsiveness loss | Avoid per-frame work proportional to the entire map/army list when there is no selection. Profile late-game UI paths and ensure selection-dependent updates short-circuit cleanly. | P3 | Verify |
| Quick save no longer shares the auto-save slot | Give quick-save and auto-save distinct typed slot identities and filenames. Add tests that saving one cannot overwrite the other. | P1 | Verify |
| Meld/Build commands correctly reselect the originating stack | Command execution should return an explicit selection/navigation result, rather than relying on stale UI selection. Test success, failure and partial-stack cases. | P4 | UI |
| Units with no moves cannot receive movement orders through the opposite Plane | Command validation must check unit plane, remaining movement and active-view context. UI visibility must not bypass simulation legality. | P2 | Verify |
| Invalid destination for an exhausted army does not deselect it | Failed command validation should not mutate selection state. Separate command result from UI selection policy. | P4 | UI |
| Plane switching during spell targeting does not deselect the active stack | Treat targeting mode and stack selection as independent UI state. Plane changes should update the view without destroying unrelated selection unless the target becomes invalid. | P4 | UI |
| Treasure text retains final list separator after pick reward | Build reward text from structured entries and format separators at render time. Do not append separators incrementally as a side effect of reward resolution. | P4 | UI |
| Engineer build orders from the opposite Plane center the view on the Engineers | Resolve the target unit's plane and position before issuing a view-centering action. Do not silently redirect the command to the currently visible plane. | P4 | UI |
| Non-selected Engineers do not affect road build-time prediction unless already building at origin | Build-time preview must use the exact eligible-worker query used by the actual build action. Clearly define when a worker at the origin is already committed. | P3 | Verify |
| Advisor, Mirror and spellbook screens can close with their opening key | Use symmetric open/close bindings and a consistent screen-stack/modal policy. Escape and the opening shortcut should not produce conflicting transitions. | P4 | UI |
| Item creation has context help and deleting the name assigns a new default | The editor should provide contextual help from item schema/rules and a deterministic default-name generator. Empty names must be normalized before saving. | P4 | UI |
| Simultaneous mouse buttons do not trigger inappropriate controls | Input events should be normalized into intentional actions; simultaneous presses must not be interpreted as two independent clicks on unrelated controls. | P3 | UI |
| Unrecognized keys and Ctrl+key combinations do not trigger unrelated controls | Use a structured key/modifier event model. Match exact chords and ignore unsupported combinations; never fall through to a nearby or default binding accidentally. | P3 | UI |
| Right-click during confirmation dialog does not invoke background help/crash | Modal dialogs must capture input and prevent event propagation to underlying screens. Add a general modal-input regression test. | P1 | UI |
| Hiring-dialog ability help hit areas do not overlap | Generate hit areas from the final layout geometry and ensure they are disjoint or have a deterministic priority. | P4 | UI |
| Settings screen becomes multi-page and highlights hovered options | Represent settings as typed descriptors grouped into pages, with labels, help, valid values and dependencies. The UI should render from those descriptors rather than maintain separate settings lists. | P4 | UI |
| Restore End of Turn Wait, Auto Unit Information and Show Node Owners settings | Keep these as user preferences, distinct from simulation rules. Persist them in client/user configuration, not in the campaign ruleset unless they affect game state. | P4 | UI |
| Automatic Combat reverts to Strategic Combat Only | Make combat automation mode explicit and default to the stated behavior. Do not overload a generic boolean if the game may later support more than two combat policies. | P3 | Design |
| Backup Saves help text corrected | Keep setting descriptions/help synchronized with actual behavior. Ideally generate settings UI and help from one descriptor. | P4 | UI |
| Revolting Raiders and Monsters Gone Wild become patch features, not in-game settings | These change campaign rules and must be chosen before new-game creation. Store them in immutable campaign configuration; do not expose a mid-campaign toggle. | P2 | Architecture |
| Key binding interface | Add a binding editor with capture, conflict display, reset-to-default and validation. Store bindings by stable action ID and context, not by UI element index or hard-coded key. | P4 | UI |
| Original non-debug hotkeys are reimplemented as dynamic bindings | Inventory all actions and map them to stable action identifiers. The Hero-offer A key becomes generic Accept; Cartographer N reuses the Plane action. Keep contextual action IDs separate even when default keys match. | P4 | UI |
| Quick Load, Quit Without Saving and Default Item Editor bindings | Register these as explicit overland actions. Quick Load must target the quick-save slot; Quit Without Saving must bypass auto/continue save; the item editor should be an in-game tool with validation. | P3 | UI |
| Find Next/Previous Army and Next/Previous City bindings | Implement view cycling over eligible player armies/cities without changing selection or entering a city. Define wraparound and deterministic ordering. | P4 | UI |
| Save/Recall Location supports eight saved locations and persists with game | Store the eight locations in campaign state/save data, validate coordinates/plane on load and provide stable bindings for save and recall. | P4 | UI |
| Scroll Map Up/Down/Left/Right bindings | Route through the same viewport scroll API as pointer/edge scrolling. Respect map bounds and wrap rules. | P4 | UI |
| Next/Previous Control cycles visible controls by index | Build a stable ordered list of visible, enabled, focusable controls. Recompute after screen/layout changes and handle empty lists safely. | P4 | UI |

### Recommended input architecture

Use an action-oriented input layer:

- Each action has a stable string identifier and a context (overland, city, battle, wizard setup, modal dialog, etc.).
- A binding maps a key plus modifiers to an action; multiple bindings may map to the same action.
- Duplicate-key behavior is an explicit preference/policy. Define whether conflicts are permitted, prioritized or broadcast.
- Input dispatch considers the topmost modal context first. A modal dialog consumes relevant input and blocks background controls.
- Focus navigation, pointer position, directional snapping and edge scrolling are separate systems.
- User preferences (bindings, pointer movement, edge scrolling) are stored separately from campaign rules and save state, except campaign-specific saved map locations.

## 4. Audio, graphics engine and platform concerns

| Change | Assessment for the rewrite | Priority | Status |
|---|---|:---:|---|
| Event music/background track changes no longer glitch the synthesizer | Audio playback should be a stateful service with explicit track transitions, resource ownership and cancellation. Avoid overlapping synth voices or stale callbacks when changing tracks. | P3 | Verify |
| Good Event and Building Complete tracks no longer loop indefinitely | Model music loop policy explicitly per track/event. One-shot event tracks must terminate and return to the intended background state. | P4 | UI |
| Negative Food upkeep dialog receives event music | Ensure event presentation is driven by the event type and not by a narrow set of dialog-opening paths. Keep audio cues attached to semantic events. | P4 | UI |
| Graphics engine bugs that affect modded games are fixed | Add bounds and validity checks to asset/resource decoding and rendering. Modded content should fail validation with diagnostics rather than trigger undefined behavior or memory corruption. | P1 | Architecture |
| Reduced conventional-memory footprint and compatibility with Windows XP/base segment E000 | **Not applicable as a direct requirement.** Rust's memory model and modern targets do not reproduce DOS conventional-memory constraints. Retain only the general lesson: resource use should be measured, bounded and compatible with the chosen platform targets. | P4 | Not applicable |
| ITEMMAKE.EXE unsupported; feature-complete in-game editor replaces it; known old item bugs auto-correct on load | Implement the editor against the same item schema and validation used by runtime generation. Legacy import/migration should be a separate, explicit compatibility layer with versioned repair rules and a report of changes. Do not silently mutate arbitrary item data. | P2 | Architecture |

## 5. Patch features

Patch features are campaign rules/configuration, not ordinary user preferences. Each should have a stable key, typed value (usually boolean or enum), default, description, compatibility constraints and a clear point at which it is locked. Store the resolved feature set in the save metadata and include it in the ruleset/configuration fingerprint.

| Patch feature | Assessment and recommendation | Status |
|---|---|---|
| Custom Worlds | Enables access to map customization. This is primarily a UI/availability switch; the underlying map settings should remain well-defined independently. | Optional |
| Different Planes | Controls whether map generation settings are global or independently configured per Plane. Represent scope explicitly; validate that both planes receive complete settings when per-plane mode is active. | Optional |
| Last Profile Reload | Controls availability of the Last quickstart button. UI convenience only. | Optional |
| Load Last Banner | Determines whether Last quickstart stops at banner selection. Keep as a new-game flow preference. | Optional |
| No AI Race Restriction | Removes high-difficulty Arcanus AI race restrictions. Make the eligible race pool data-driven and difficulty-aware; do not hard-code the old list in AI logic. | Optional |
| v1.51 Treasure | Selects the v1.51 treasure budget/reward rules. Keep encounter budget, treasure budget and reward-selection policy separate. | Optional |
| Extra Node Power | Adds +1 Node Power per tile of influence. Make influence scaling a named ruleset parameter and test exact tile-count boundaries. | Optional |
| No Double Towers | Prevents two Towers within 10 tiles from connecting to the same continents. Model connected continent identity and tower exclusion as a distinct placement constraint. | Optional |
| Common Elves | Removes restrictions on High Elves as a neutral race. Keep neutral-race eligibility in content/configuration rather than generator hard-coding. | Optional |
| More Precise Neutrals | Scales distance requirements between neutral Cities and Encounter Zones with Land Size. Use a documented formula/table and the same map-distance function used by placement validation. | Optional |
| Old Tower Generator | Disables land enforcement and restores original tower placement mechanics. Keep as a separate generator policy; do not weaken modern tower constraints globally. | Optional |
| Old Node Generator | Disables land enforcement and restores original node placement mechanics. Same isolation requirement as Old Tower Generator. | Optional |
| Life And Death | Allows Life and Death Spellbooks together. Represent realm mutual exclusions as declarative compatibility rules so the feature changes one rule rather than bypassing validation. | Optional |
| 2-Pick Myrran | Changes Myrran Retort cost to two picks. Retort cost must be queried consistently by profile creation, UI, AI and treasure. | Optional |
| Non-11 Book Matching | If the human does not start with 11 books in one Realm, AI does not either. Treat this as a wizard-generation parity policy; avoid hidden difficulty exceptions. | Optional |
| More 11-Book AIs | Increases odds of AI Wizards receiving 11 books in one Realm. Make the probability explicit and seeded. Clarify interaction with Non-11 Book Matching and difficulty. | Optional |
| Strict Retort Reqs | AI must obey Spellbook requirements for Retorts; treasure also obeys them. Use the shared Retort eligibility predicate for human selection, AI and reward generation. | Optional |
| No Snow Swimming | Sailing/Swimming movement types no longer receive cheaper movement across Tundra than ground units. Keep terrain movement costs and movement capabilities composable and test all relevant terrain/movement combinations. | Optional |
| No Deep Tundra | All-surround Tundra uses the same movement cost as other Tundra. Represent deep/all-surround status as a terrain property and apply this feature only to the cost rule. | Optional |
| Mountains Of Grass | Mountaineers do not suffer a movement penalty crossing Grasslands. Keep movement-type terrain modifiers in data/rules, not a special case in pathfinding. | Optional |
| Lesser Hero Powerup | Additional random abilities for less useful Heroes; Healer and Golden One can equip staves. Express hero capability and item-slot rules through shared predicates/data. | Optional |
| v1.51 Default Relations | Uses the 1.50/1.51 Starting Relations formula. Keep both formulas named and selectable; the baseline should follow the release's stated default. | Optional |
| Item Theming | Restores Spellbook requirements for magical-item rewards. Use the same eligibility predicate for treasure and item creation. | Optional |
| Cheaper Misc Stats | Removes the extra miscellaneous-item cost multiplier for attribute boosts. Keep category pricing explicit and test price calculations. | Optional |
| Five Power Items | Raises the number of item Powers to five. Use one shared limit across editor, generation, validation and save/load. | Optional |
| Charge Any Weapon | Allows Spell Charges on any weapon type. Keep slot compatibility as data/rules shared by editor and generator. | Optional |
| Better Failed Rewards | Converts failed special rewards into budget-equivalent treasure; substitutes Mana/Gold for failed spell/prisoner rewards; prevents empty treasure. Keep reward fallback deterministic and budget-aware. | Optional |
| Adamant Alchemy | Alchemy costs two picks and permits Mithril/Adamantium. Treat cost and ore access as one coherent feature with shared validation. | Optional |

The table deliberately labels these as optional features rather than recommending that all be enabled. Their purpose is to preserve compatibility choices and allow a ruleset to select a known variant.

## 6. World settings

Map settings should be represented by a typed WorldGenConfig, validated before generation, and serialized into the campaign. Numeric settings need explicit units, bounds and defaults. Settings that are global or per-Plane should use an explicit scope rather than duplicated ad hoc fields.

| Map setting | Assessment and recommendation |
|---|---|
| Land Tiles | Define the target land-tile count or density unambiguously. Validate against map dimensions and plane count; expose the legacy 360-tile threshold only as part of legacy-mode eligibility. |
| Max Continent Size | Define whether this is a tile count, fraction or generator-specific size tier. Use one definition for continents and islands and test that the cap is respected. |
| Single Continent | A topology constraint, not merely a preference. Define behavior when the requested land count/shape cannot fit one connected continent. |
| Inland Lakes | Make lake creation an explicit topology option. Rivers must still have valid outflow behavior when lakes are enabled. |
| Terrain Height | Use an enum (Low/Normal/High or supported values) to control Grassland versus Hills/Mountains ratio. Validate compatibility with legacy mode. |
| Dynamic Elevations | Derive base terrain ratios from the Medium Land Size baseline, independently of the selected land-tile count. Document interaction with Terrain Height. |
| Polar Caps | Enable/disable polar Tundra caps as a topology/terrain option. Node/tower island growth must respect the selected boundary. |
| Tundra Reach | Define how far Tundra may extend from map edges; validate its range against map height and legacy-mode constraints. |
| Max Desert Regions | Treat as a target/cap for connected regions, not simply a count of desert tiles. |
| Max Swamp Patches | Treat as a target/cap for connected regions. Keep the setting's meaning distinct from total swamp-tile density. |
| Max Rivers | Define whether the value limits river systems, sources or paths. Validate after river generation. |
| All-Terrain Rivers | Controls whether rivers may cross Hills, Mountains and Deserts. Use terrain eligibility data and verify outflow continuity. |
| Non-Grass Islands | Allows Node/Tower island expansions to use non-Grass terrain. Keep island terrain selection separate from base continent terrain. |
| Capital Pop Priority | Controls the tradeoff between starting-site food availability and mutual distance. Make the scoring weights explicit and deterministic; supersedes the listed historical patches as a unified setting. |
| Capital Roads | Allows starting cities to connect to the initial neutral road network. Model roads as a graph and validate connectivity after placement. |
| Max Road Length | Set a maximum path length for initial roads; define whether length is measured in tiles or movement cost. |
| Mineral Density | Controls the amount of Terrain Specials. Define whether density is per land tile, per region or a target count. |
| Mineral Quality | Controls quality distribution independently from mineral density. Keep resource type/quality distinct from base terrain. |
| High Value Nodes | Forces Nodes to use the 1.5x Magic setting budgets for encounters and treasure. Keep this as a node-specific budget policy and do not alter ordinary lairs. |
| Tower Budget X | Multiplier/scale for Tower encounter budgets. Define exact arithmetic and rounding. |
| Strong Lair Budget X | Budget scaling for strong lairs, separate from Tower and weak-encounter budgets. |
| Weak Encounter Budget X | Budget scaling for weak encounters, separate from strong lairs and Towers. |
| Monster Lair Realms | Controls eligible creature Realms in colored lairs. Store eligibility in data and let this setting select/override the pool. |
| Encounter Diversity | Controls variety of encounter guardians. Define whether it affects realm variety, creature uniqueness, tier distribution or all three. Avoid an ambiguous scalar with undocumented side effects. |
| Max Neutral Cities | Controls the number of neutral Cities the generator attempts. Distinguish target count from hard maximum and report when constraints prevent reaching it. |
| Neutral Population | Controls starting population ranges. Use race/terrain-valid ranges and ensure population agrees with garrison scaling. |
| Advanced Neutrals | Allows neutral Cities to begin with more advanced buildings. Define eligible building pools and validate prerequisites, race restrictions and population support. |

### World settings validation

At minimum, validate:

- All numeric settings are within documented ranges and use consistent units.
- Per-Plane settings are complete and valid when Different Planes is enabled.
- Single Continent, Max Continent Size, Land Tiles and inland-lake constraints are mutually feasible.
- River, terrain and polar-cap settings can produce valid outflows and transitions.
- City, tower, node and encounter spacing rules can be satisfied or have a documented relaxation path.
- Legacy mode is enabled only when every stated eligibility condition is met.
- Budget multipliers and population ranges cannot overflow or produce negative values.
- The resolved settings and feature flags are captured in the save metadata.

## 7. Historical patch obsolescence and compatibility

The long list of obsolete IW/IM/M/W patches is a migration boundary, not a to-do list. Do not recreate those patches as individual Rust switches. Their behavior has been consolidated into the current baseline, patch features and map settings.

Recommended handling:

1. Maintain a small migration/reference table mapping historical patch identifiers to the v1.60 rule, feature or setting that supersedes them.
2. Do not expose obsolete patch IDs in the new UI or save model.
3. Where a historical patch changed behavior that is now optional, map it to the corresponding named feature/setting only when the semantics match exactly.
4. Treat the explicitly listed non-reversible patches as historical data that cannot be safely toggled off. Do not promise a reverse operation unless the original state can be reconstructed.
5. Keep the v1.31 legacy world-generation order distinct from the modern generator and from the optional Old Node/Old Tower placement mechanics. The legacy-mode eligibility predicate is a conjunction of settings, not a general “classic mode” toggle.
6. Preserve versioned import/migration for legacy item data and saves; do not attempt to reverse old patch effects by guessing.

## 8. Fit with Plan C runtime rulesets

Plan C's existing direction is appropriate, but v1.60 makes the configuration model more important. The following should be explicit additions/refinements to Plan C.

### Separate content definitions, campaign rules and user preferences

- **Content definitions:** units, buildings, spells, races, heroes, items, terrain, encounter tables, realm eligibility and other data-driven game content.
- **Ruleset/patch features:** named gameplay variants such as Extra Node Power, Strict Retort Reqs, Five Power Items, Raiders/Monsters variants and item-reward policies.
- **World-generation configuration:** Land Tiles, terrain, topology, rivers, roads, minerals, starting-site priorities, encounter budgets and neutral-city settings.
- **User preferences:** key bindings, pointer movement, edge scrolling, advisor/UI settings, sound preferences and similar client behavior.
- **Campaign state:** generated map, eight saved map locations, selected wizard, game state, and a frozen snapshot/fingerprint of the ruleset and world configuration.

Do not put user preferences into the simulation ruleset merely because the original game exposed them on the same Settings screen. Conversely, do not treat map settings or gameplay patch features as freely mutable user preferences after the game has started.

### Named identifiers, not patch numbers

Every feature should use a stable string key, for example:

- world.legacy_generation_order
- world.old_tower_generator
- world.old_node_generator
- world.no_double_towers
- encounters.v151_treasure
- encounters.better_failed_rewards
- items.five_power_limit
- items.item_theming
- ai.no_race_restriction
- wizard.strict_retort_requirements
- movement.no_snow_swimming

The names above are illustrative API keys, not a requirement to adopt those exact strings. The important property is that save files and definitions must not depend on historical patch numbers or UI labels. Keys should be stable, documented and namespaced.

As established in Plan C, special executable behavior remains registered by stable string key in Rust, while definitions refer to those keys. Patch features and settings are configuration values; they must not be implemented as spell IDs, registry indices or numeric patch identifiers.

### Snapshot and persistence

At new-game creation:

1. Resolve the base ruleset and enabled mods.
2. Resolve patch features and world settings, including defaults.
3. Validate all values and cross-setting constraints.
4. Determine whether legacy generation mode is eligible.
5. Generate the map from the resolved configuration and explicit seed.
6. Store the ruleset/mod fingerprint, feature set, world settings, generator version and seed with the save.

During a campaign, the feature set and world-generation configuration should be immutable. User preferences may change independently. If the engine later supports a gameplay option that is intentionally mutable mid-campaign, define that lifecycle explicitly rather than making all features mutable.

A save should record the resolved values, not just “use defaults”: defaults may change in a future release. Store a schema/version for the settings and a deterministic fingerprint of the simulation-affecting configuration.

## 9. Recommended implementation and testing order

### P1 — Correctness and safety foundations

1. Deterministic PRNG interface, explicit seeds and reproducible state.
2. World-generation validator and failure reporting.
3. Constraint-based mandatory-object placement with explicit relaxations.
4. Valid river connectivity/outflow and polar-boundary handling.
5. Distinct quick-save and auto-save slots.
6. Modal input capture and safe event dispatch.
7. Safe asset/item loading and versioned legacy-item repair.

### P2 — Core rules and configuration

1. Typed WorldGenConfig with validated ranges and global/per-Plane scope.
2. Named patch-feature registry with defaults, descriptions and compatibility rules.
3. Campaign-start snapshot and save metadata/fingerprint.
4. City suitability, terrain population caps and neutral garrison scaling.
5. Tower/node/encounter placement constraints and island growth.
6. Separate encounter, Raider and Rampaging Monster budgets.
7. Retort eligibility shared across wizard creation, AI and treasure.
8. Item reward eligibility, power-count validation and cost calculation.
9. AI load initialization and command legality across Planes.

### P3 — Balance and secondary simulation

1. Optional encounter and treasure budget variants.
2. Node influence and high-value node rules.
3. Movement terrain variants.
4. Item pricing, item power count, item charge eligibility and reward fallback.
5. Hero powerup and neutral-city advanced-start policies.
6. Combat flee behavior by difficulty.
7. Economy/UI values derived from simulation calculations.

### P4 — UI and convenience

1. Dynamic action/binding registry and binding editor.
2. Multi-page settings generated from descriptors.
3. City rename/cycle and advisor/screen close behavior.
4. Pointer movement, edge scrolling and delay.
5. Saved map locations and view-cycling controls.
6. Context help, item editor UI and audio event handling.

## 10. Regression-test checklist

- Same seed + same ruleset/features/settings produces the same map and generation report.
- Saving/restoring PRNG state reproduces the same subsequent random sequence.
- A change in one random stream does not perturb unrelated streams where streams are intentionally isolated.
- Every mandatory site is placed or generation fails with a precise explanation; no silent invariant violation.
- Starting cities satisfy minimum population, race terrain, food and spacing rules.
- Tower/node distance and continent-connection constraints hold, including small maps and polar edges.
- Encounter adjacency is avoided unless a recorded fallback is necessary; encounter zones never touch starting cities.
- Every river has a valid outflow and all terrain transitions match final neighbors.
- Node/tower island growth never merges into polar caps when caps are enabled.
- Neutral garrison sizes are correct at population threshold boundaries for all named races.
- Two-pick treasure rewards grant two picks; item generation produces the intended number of Powers.
- Retort requirements are identical for human wizard creation, AI generation and treasure.
- Crafting-cost modifiers do not change countering/resistance calculations.
- Raiders and Rampaging Monsters use separate formulas from ordinary encounter generation.
- Campaign-start features cannot be changed in an active game; user preferences can be changed without changing simulation rules.
- Save/load preserves exact feature and world-setting values even if defaults later change.
- Quick save never overwrites auto-save and vice versa.
- Modal dialogs block input to underlying screens.
- Duplicate key bindings follow the configured conflict policy; unknown keys and modifier combinations do nothing.
- City/army cycling and saved locations wrap/restore deterministically and respect Plane.
- Item editor output passes the same validation as generated and loaded items.

## Overall recommendation

Treat v1.60 as the point where the rewrite should formalize three separate configuration layers: **ruleset features, world-generation settings and user preferences**. The largest engineering investment should go into the constraint-based map generator and its deterministic test harness. The UI and key-binding changes are substantial, but they become much easier once actions and settings are data-driven.

Do not attempt to port the long obsolete patch list one patch at a time. Implement the current v1.60 behavior as coherent systems, then expose only the documented alternatives as named configuration choices.

## Optional

The following audit findings are explicitly optional variants or compatibility settings. They should be tracked as selectable configuration, not silently imposed as baseline behavior.

| Source section | Change | Assessment | Priority | Original status |
|---|---|---|:---:|---|
| World generation and world state | Legacy mode reproduces 1.31 placement order only under the specified compatibility conditions | Implement a named legacy-generation mode with an explicit eligibility predicate matching all stated requirements: Custom Worlds, Old Node Generator and Old Tower Generator enabled; at least 360 land tiles on each applicable plane; continent size limit zero; Single Continent off; Terrain Height Low or Normal; Tundra Reach, Max Swamps and Max Deserts no greater than 8. Other settings may vary. If the predicate is false, do not silently claim to be in legacy mode. Keep the 1.31 order isolated from the modern constraint-based generator. | P2 | Optional |
| World generation and world state | Raiders and Rampaging Monsters return to 1.31 generation, except Impossible is shifted to Extreme; other unofficial changes are in optional features | Keep Raider and Rampaging Monster generation separate from Encounter Zone generation. Make the baseline formula explicit, including the difficulty mapping. Put Revolting Raiders and Monsters Gone Wild behind campaign-start patch features; do not allow changing them mid-campaign. | P2 | Optional |
| Combat, wizards, spells and items | More Random Items: half of items are generated from scratch and can use the full Item Power range | Keep this as an optional item-generation policy. Separate the probability of using a generated item from the power-selection pool and item-power eligibility. Validate item legality, power count, cost and compatibility with the item schema. | P3 | Optional |
| Combat, wizards, spells and items | Item Theming restores Spellbook requirements for item rewards | Treat realm/book restrictions as an explicit item-reward eligibility policy. Make the restored restriction selectable via the named patch feature; do not bake it into generic item construction. | P3 | Optional |
| Combat, wizards, spells and items | Cheaper Misc Stats removes the extra cost multiplier for attribute boosts on miscellaneous items | Item pricing should be a composable formula over power, slot/category and material. Expose the category multiplier as a ruleset value; test equivalent powers across weapon, armor and miscellaneous slots. | P3 | Optional |
| Combat, wizards, spells and items | Five Power Items raises the maximum number of item Powers to five | Make the item power limit a validated ruleset setting. Ensure the editor, treasure generator, save format and UI all use the same limit. | P3 | Optional |
| Combat, wizards, spells and items | Charge Any Weapon permits Spell Charges on all weapon types | Model item-slot compatibility as data or a predicate shared by the editor and item generation. Do not hard-code the restriction in only one UI path. | P3 | Optional |
| Combat, wizards, spells and items | Better Failed Rewards converts failed specials to equivalent treasure, replaces failed spells/prisoners with Mana/Gold, and disallows empty treasure | Keep reward fallback policy separate from the encounter budget and the primary reward roll. Every fallback must preserve the intended budget semantics and must not create an infinite retry loop. Make it a patch feature. | P3 | Optional |
| Combat, wizards, spells and items | Adamant Alchemy makes Alchemy cost two picks and enables Mithril/Adamantium | Retort pick cost and ore eligibility are two parts of one named feature. Validate the pick cost consistently for human selection, AI profile generation and treasure awards. | P3 | Optional |
| Combat, wizards, spells and items | Lesser Hero Powerup gives weaker Heroes additional Random Abilities and lets Healer/Golden One equip staves | Express hero-specific ability/equipment eligibility in content data or named capability tags. Avoid identifying heroes by numeric ID or duplicating eligibility rules in the item UI and random ability generator. | P3 | Optional |
| User interface, input and settings | Pointer snapping to key-activated controls is optional; directional snapping is optional | Separate focus navigation from pointer position. Provide an explicit legacy pointer-movement setting rather than coupling keyboard focus to mouse coordinates. | P4 | Optional |
| User interface, input and settings | Legacy pointer movement still reaches controls with assigned bindings | If legacy pointer movement is enabled, derive its navigable controls from the same visible/enabled control registry used by keyboard focus. | P4 | Optional |
| User interface, input and settings | Allow Duplicate Keys | Support multiple actions on the same key within the same screen when enabled. Define conflict resolution and whether all actions fire or a priority is used. Do not silently remove an earlier binding. | P4 | Optional |
| User interface, input and settings | Legacy Pointer Movement | Restore pointer movement to key-activated controls and directional snapping to adjacent controls. Keep disabled/enabled state as a user preference and keep focus navigation independent when off. | P4 | Optional |
| User interface, input and settings | Screen Edge Scrolling | Add edge detection to overland map input, with a clear distinction between map viewport and UI panels. Do not scroll while a modal dialog or another screen owns input. | P4 | Optional |
| User interface, input and settings | Edge Scroll Delay | Treat delay as a user-facing input setting with a bounded duration; avoid frame-rate-dependent timing. | P4 | Optional |

## Reverted / Obsolete

These findings explicitly supersede or reverse an earlier patch behavior. Do not implement the superseded behavior as an independent requirement.

| Source section | Change | Assessment | Priority | Original status |
|---|---|---|:---:|---|
| World generation and world state | Dark Elf neutral-city garrisons are not too small | Verify Dark Elf population-to-garrison scaling independently; the note explicitly reverses the prior IM192 behavior, so do not inherit that earlier rule as the new baseline. | P2 | Verify |
