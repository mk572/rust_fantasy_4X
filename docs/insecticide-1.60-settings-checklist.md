# Insecticide v1.60 — settings and switches checklist

Standalone implementation tracker based on the v1.60 change log. This inventories named Patch features, World settings, ordinary game settings and bindable actions. It does not assume defaults that the release notes do not state.

Use `[ ]` for not implemented and `[x]` for implemented and tested. For each entry, eventually record a stable key, type, default, valid values, dependencies, lifecycle and persistence location.

## 1. Patch features

These are selected from the Patch interface when creating a game. Treat them as campaign rules and preserve their resolved values in the save.

### New-game flow and world generation
- [ ] **Custom Worlds** — Enables the map-generator customizer.
- [ ] **Different Planes** — Configures map generation globally or separately per Plane.
- [ ] **No Double Towers** — Prevents two Towers within 10 tiles from connecting to the same continents.
- [ ] **More Precise Neutrals** — Scales neutral City / Encounter Zone distance requirements with Land Size.
- [ ] **Old Tower Generator** — Disables land-enforcement for Towers and restores original placement mechanics.
- [ ] **Old Node Generator** — Disables land-enforcement for Nodes and restores original placement mechanics.
- [ ] **No AI Race Restriction** — Removes high-difficulty Arcanus AI race restrictions; supersedes 757REV and optional IM185.
- [ ] **Common Elves** — Allows High Elves as a neutral Race without the former restrictions.

### Wizard, spellbook and retort rules
- [ ] **Life And Death** — Life and Death Spellbooks are not mutually exclusive.
- [ ] **2-Pick Myrran** — Myrran Retort costs 2 picks.
- [ ] **Non-11 Book Matching** — If the human does not start with 11 books in one Realm, neither do computer players.
- [ ] **More 11-Book AIs** — Increases the odds of computer players starting with 11 books in one Realm.
- [ ] **Strict Retort Reqs** — AI Retort selection and treasure must obey Spellbook requirements.
- [ ] **v1.51 Default Relations** — Uses the 1.50/1.51 Starting Relations formula.

### Encounters, treasure and rewards
- [ ] **v1.51 Treasure** — Uses v1.51 budget and treasure rules; supersedes 777REV and optional TREASURE.TXT.
- [ ] **Extra Node Power** — Adds +1 Node Power per tile of influence; supersedes 768REV and optional IM181.
- [ ] **Revolting Raiders** — Selects the Raiders-generation variant. This is a Patch feature, not a normal game setting.
- [ ] **Monsters Gone Wild** — Selects the Rampaging Monsters-generation variant. This is a Patch feature, not a normal game setting.
- [ ] **Better Failed Rewards** — Failed specials convert to budget-equivalent treasure; failed spells/prisoners become Mana/Gold; empty treasure is prevented.

### Movement rules
- [ ] **No Snow Swimming** — Sailing/Swimming units do not move cheaper across Tundra than ground units.
- [ ] **No Deep Tundra** — All-surround Tundra has the same movement cost as other Tundra.
- [ ] **Mountains Of Grass** — Mountaineers have no movement penalty on Grasslands.

### Heroes and items
- [ ] **Lesser Hero Powerup** — Less useful Heroes gain additional Random Abilities; Healer and Golden One can equip staves.
- [ ] **Item Theming** — Restores Spellbook requirements for Magical Item rewards.
- [ ] **Cheaper Misc Stats** — Attribute-boost Item Powers do not cost twice as much on miscellaneous items.
- [ ] **Five Power Items** — Raises the maximum number of item Powers to five.
- [ ] **Charge Any Weapon** — Allows Spell Charges on any weapon, not only Staves and Wands.
- [ ] **Adamant Alchemy** — Alchemy costs 2 picks and permits Mithril and Adamantium Ores.

### Quickstart profile
- [ ] **Last Profile Reload** — Controls availability of the Last button.
- [ ] **Load Last Banner** — Controls whether Last stops at banner selection.

**Patch feature count: 30**

## 2. World settings

These are exposed through the optional World button in the New Game Menu. They configure map generation and belong in the campaign's world-generation snapshot. Different Planes determines whether values are global or per-Plane.

### Size and topology
- [ ] **Land Tiles** — Amount of land generated; supersedes IM186.
- [ ] **Max Continent Size** — Maximum size of each continent or island; replaces 547.
- [ ] **Single Continent** — Forces one continent.
- [ ] **Inland Lakes** — Controls whether inland lakes may appear; replaces 480.
- [ ] **Polar Caps** — Allows removal of polar Tundra caps.
- [ ] **Non-Grass Islands** — Allows Node/Tower island expansions to use non-Grass terrain by default.

### Terrain and regions
- [ ] **Terrain Height** — Grassland versus Hills/Mountains ratio. Low and Normal are referenced by legacy-mode eligibility.
- [ ] **Dynamic Elevations** — Uses the base terrain ratio of a Medium Land Size map.
- [ ] **Tundra Reach** — How far Tundra may extend from the north/south edges.
- [ ] **Max Desert Regions** — Number of Desert regions the generator tries to create.
- [ ] **Max Swamp Patches** — Number of Swamp regions the generator tries to create.
- [ ] **All-Terrain Rivers** — Allows Rivers over Hills, Mountains and Deserts.

### Rivers and roads
- [ ] **Max Rivers** — Number of Rivers the generator tries to create.
- [ ] **Capital Roads** — Connects starting cities to the initial neutral road network.
- [ ] **Max Road Length** — Maximum distance spanned by initial Roads.

### Starting locations and neutral cities
- [ ] **Capital Pop Priority** — Relative priority of starting-location food availability versus mutual distance; supersedes IM183, IM184, 010 and 791.
- [ ] **Max Neutral Cities** — Number of neutral Cities the generator attempts; supersedes optional 026.
- [ ] **Neutral Population** — Starting population ranges for neutral Cities.
- [ ] **Advanced Neutrals** — Allows neutral Cities to start with advanced buildings; supersedes IM193.

### Minerals and encounters
- [ ] **Mineral Density** — Amount of Terrain Specials placed.
- [ ] **Mineral Quality** — Quality of Minerals placed.
- [ ] **High Value Nodes** — Forces Nodes to use 1.5x Magic-setting encounter and Treasure budgets.
- [ ] **Tower Budget X** — Increases Tower of Wizardry encounter budgets.
- [ ] **Strong Lair Budget X** — Increases strong-lair encounter budgets.
- [ ] **Weak Encounter Budget X** — Increases weak-lair encounter budgets.
- [ ] **Monster Lair Realms** — Controls creature Realms eligible for colored lairs; supersedes IM188 and 778.
- [ ] **Encounter Diversity** — Controls possible variety in encounter guardians; supersedes IM189, 015 and 779.

**World setting count: 27**

## 3. Ordinary game and interface settings

These are not Patch features or World settings. They normally belong in user preferences.

### Restored or changed settings
- [ ] **End of Turn Wait** — Restored after removal in v1.40.
- [ ] **Auto Unit Information** — Restored after removal in v1.40.
- [ ] **Show Node Owners** — Restored after removal in v1.50.
- [ ] **Automatic Combat** — Reverted to the original Strategic Combat Only behavior.
- [ ] **Backup Saves** — Existing setting; v1.60 corrects its help text.

### Added settings
- [ ] **Allow Duplicate Keys** — Allows a key already used on the same screen to be bound again without deleting its previous binding.
- [ ] **Legacy Pointer Movement** — Restores pointer movement to key-activated controls and directional snapping to adjacent controls.
- [ ] **Screen Edge Scrolling** — Mouse at a screen edge scrolls the overland map in that direction.
- [ ] **Edge Scroll Delay** — Delay between reaching a screen edge and scrolling starting.
- [ ] **More Random Items** — Half of items are generated from scratch; those items can use the full Item Power range, not only attribute increases.

## 4. Settings/menu facilities

These are screens or controls, not simulation switches.

- [ ] **Patch button** — Opens the Patch feature interface in the New Game Menu.
- [ ] **World button** — Optional New Game Menu entry for additional map settings.
- [ ] **Last button** — Optional quickstart using the last Wizard profile; controlled by Last Profile Reload and Load Last Banner.
- [ ] **Multi-page Settings screen** — Settings are shown across pages; hovered option is highlighted.
- [ ] **Key-binding interface** — Accessible from the game Settings screen.
- [ ] **Item creation context help** — Context-based help on the item creation screen.
- [ ] **In-game Default Item Editor** — Replaces ITEMMAKE.EXE and is invoked by a key binding.

## 5. Dynamic key-binding actions

These are bindable actions, not on/off settings. v1.60 also converts all original non-debug hotkeys to dynamic bindings.

### New explicit actions
- [ ] **Quick Load** (overland) — Loads SAVE10.GAM, the quick-save slot.
- [ ] **Quit Without Saving** (overland) — Exits without performing an auto/continue save.
- [ ] **Default Item Editor** (overland) — Opens the in-game default item editor.
- [ ] **Find Next Army** (overland) — Cycles view through the player's non-garrison armies without selecting one.
- [ ] **Find Previous Army** (overland) — Reverse army-view cycling.
- [ ] **Save Location** (overland) — Saves current map location; up to eight locations are stored with the game.
- [ ] **Recall Location** (overland) — Recalls a stored map location.
- [ ] **Next City** (overland) — Cycles view through the player's cities without entering one.
- [ ] **Previous City** (overland) — Reverse city-view cycling.
- [ ] **Scroll Map Up** (overland).
- [ ] **Scroll Map Down** (overland).
- [ ] **Scroll Map Left** (overland).
- [ ] **Scroll Map Right** (overland).
- [ ] **Next Control** (interface) — Cycles visible interface controls by index.
- [ ] **Previous Control** (interface) — Reverse control cycling.

### Explicit hotkey consolidations
- [ ] **Generic Ok/Yes/Accept** — Includes the former A-key action for accepting Hero offers.
- [ ] **Plane** — The Cartographer's former N-key action now uses the overland Plane binding.
- [ ] **All remaining original non-debug hotkeys** — Inventory each original action, context and default binding. The change log does not enumerate all of them.

## 6. Derived compatibility mode — not a separate switch

The 1.31 legacy map-generation order is automatically eligible only when **all** of the following are true:

- [ ] Custom Worlds is enabled.
- [ ] Old Node Generator is enabled.
- [ ] Old Tower Generator is enabled.
- [ ] Land Tiles is at least 360 on each Plane, where applicable.
- [ ] Max Continent Size is zero.
- [ ] Single Continent is off.
- [ ] Terrain Height is Low or Normal.
- [ ] Tundra Reach is 8 or lower.
- [ ] Max Swamp Patches is 8 or lower.
- [ ] Max Desert Regions is 8 or lower.

Other features and settings may be adjusted freely. Calculate this eligibility from the resolved settings; do not expose a separate free-standing legacy-mode toggle.

## 7. Changes that are not switches

Do not create toggles for ordinary v1.60 fixes merely because they appear in the change log. Examples include placement-order improvements, tower/node spacing, encounter adjacency, city population/garrison corrections, item power-count fixes, flee-loss correction, retort reward fixes, AI load fixes, UI bug fixes, audio fixes and graphics-engine fixes.

## 8. Inventory summary

| Category | Count |
|---|---:|
| Patch features | 30 |
| World/map-generation settings | 27 |
| Ordinary game/interface settings | 10 |
| Settings/menu facilities | 7 |
| Explicit new key-binding actions | 14 |
| Existing original hotkeys converted to bindings | Not enumerated in the change log |

The Patch button, World button, Last button, settings pages and binding editor are listed separately because they are UI facilities rather than value switches.

## 9. Metadata to track for every setting

| Field | Meaning |
|---|---|
| Stable key | Namespaced internal string; never use a historical patch number as identity. |
| Category | Patch feature, world setting, user preference or binding policy. |
| Type | Boolean, enum, integer, multiplier, range, key chord, etc. |
| Default | Explicit value; do not guess where v1.60 is silent. |
| Valid values | Allowed values, bounds and units. |
| Dependencies | Required or incompatible settings/features. |
| Lifecycle | New-game only, campaign-mutable or user-preference mutable. |
| Persistence | Ruleset snapshot, world config, campaign save or user config. |
| Tests | Validation, boundary cases, behavior and save/load coverage. |

**Implementation principle:** use stable namespaced string keys. Persist Patch features and World settings in the campaign's resolved configuration; persist ordinary interface preferences and key bindings in user configuration. Save resolved values, not merely defaults, because defaults may change in later versions.
