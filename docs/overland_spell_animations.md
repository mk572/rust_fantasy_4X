# Overland spell animations

What the original plays when a spell is cast on the main map, spell by spell, for task 306. The numbers are ReMoM's
spell indexes (`spl_*` in `Spellbook.h`, 1 to 214). Only spells with an overland cast are listed (`content/spells.ron`);
the battle animations are done (`spell_fx.rs`, `battle_screen/spell_anim.rs`). Sources: `OverSpel.c`
(`Cast_Spell_Overland_Do`), `SBookScr.c` (`Spell_Animation_Load_Graphics`, `Spell_Animation_Screen__WIP`),
`Spells129.c` to `Spells132.c` (the special casts) and `Spells137.c` (the summon, city and global animations).

## Rules that apply to all of them

- **Who sees them.** The human's own casts always. A rival's summon needs the Enemy Spells setting on, contact with that wizard and Detect Magic active for the human; a rival's city spell needs Enemy Spells, Detect Magic and the city square explored (a cast on the human's own city is always shown); global enchantments are shown for every wizard's cast, without a test.
- **Spell Animations setting** (`Pref::SpellAnimations`, task 280). Off, by kind: the summon scene runs 40 stages instead of 130; the city spell skips the animated build-up and only sets the enchantment (`Cast_Spell_City_Enchantment_Animation_1` runs its loop only with the setting on); the global spell shows the mirror as a still with the line "You have completed casting ..." (`cnst_Spellcast_Msg_1`; "<wizard> has cast ..." for a rival) and the spell's name; the learn-spell scroll becomes the box "You have mastered the spell of X" (`spellscr` 60); the square animations are skipped (`Spell_Animation_Screen` is only called with the setting on in `Spells130.c` and `Spells132.c`; `OverSpel.c` and `Spells129.c` call it without a test, check each).
- **Square animations.** `Spell_Animation_Load_Graphics` picks the picture, `Spell_Animation_Screen__WIP` scrolls the map to the square (`OVL_BringIntoView`), waits 3 frames, then draws the picture 20 pixels left and 18 up of the square for at least 12 frames, with the spell's sound (`Spell_Animation_Load_Sound_Effect__WIP`). The realm pictures are `specfx` 0 Nature, 1 Sorcery, 2 Chaos, 3 Life, 4 Death, 5 Arcane.
- **Music.** Summon, city and global animations start their own music (task 76); wire the animation first and the music with it later.
- **Already in the client** and not repeated here: the plain spell notice boxes (`event_popup.rs`), the dispel and fizzle boxes, the learn-spell animation (`learn_spell.rs`).
- **Not overland:** Doom Bolt, Star Fires and Warp Lightning are battle-only in our content (the original also lets them hit an enemy stack overland).

## The table

| # | Spell | Animation to wire up |
|---|---|---|
| 2 | Resist Elements | Square animation: the Nature realm picture (`specfx` 0) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 3 | Wall of Stone | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 4 | Giant Strength | Square animation: the Nature realm picture (`specfx` 0) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 6 | War Bears | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Weak Creature Summons" (MUSIC 013 to 016) |
| 7 | Stone Skin | Square animation: the Nature realm picture (`specfx` 0) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 8 | Water Walking | Square animation: the Nature realm picture (`specfx` 0) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 9 | Sprites | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Weak Creature Summons" (MUSIC 013 to 016) |
| 10 | Earth Lore | Map animation `specfx` 45 "FULGRN2" played 22 steps with the palette recoloured (`Open_File_Animation__HACK`), with its sound |
| 12 | Nature's Eye | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; the area round the city is explored afterwards |
| 14 | Giant Spiders | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 15 | Change Terrain | Square animation `specfx` 8 on the changed square (`Spell_Animation_Load_Graphics`, drawn from the 4th frame, at least 12 frames, map scrolled to the square first) |
| 16 | Path Finding | Square animation: the Nature realm picture (`specfx` 0) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 17 | Cockatrices | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 18 | Transmute | Same square animation as Change Terrain: `specfx` 8 |
| 19 | Nature's Cures | Square animation: the Nature realm picture (`specfx` 0) on the stack |
| 20 | Basilisk | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 21 | Elemental Armor | Square animation: the Nature realm picture (`specfx` 0) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 23 | Stone Giant | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Powerful Creature Summons" (MUSIC 013 to 016) |
| 24 | Iron Skin | Square animation: the Nature realm picture (`specfx` 0) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 25 | Ice Storm | Square animation `specfx` 10 "HAIL" on the enemy stack, then the damage |
| 26 | Earthquake | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the negative spell music; the screen shakes (30 steps) with the Earthquake sound |
| 27 | Gorgons | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Powerful Creature Summons" (MUSIC 013 to 016) |
| 28 | Move Fortress | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; the Fortress building appears in the cityscape |
| 29 | Gaia's Blessing | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 31 | Regeneration | Square animation: the Nature realm picture (`specfx` 0) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 32 | Behemoth | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 34 | Nature Awareness | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 22 "NATAWARE" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 36 | Colossus | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 37 | Earth Gate | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; the Earth Gate building appears in the cityscape |
| 38 | Herb Mastery | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 24 "HERBMAST" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 39 | Great Wyrm | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 40 | Nature's Wrath | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 23 "FRCNATUR" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 41 | Resist Magic | Square animation: the Sorcery realm picture (`specfx` 1) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 43 | Floating Island | Square animation: the Sorcery realm picture (`specfx` 1) on the island square (not the summon scene) |
| 44 | Guardian Wind | Square animation: the Sorcery realm picture (`specfx` 1) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 47 | Word of Recall | Square animation: the Sorcery realm picture (`specfx` 1) on the recalled stack |
| 49 | Nagas | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Weak Creature Summons" (MUSIC 013 to 016) |
| 52 | Disenchant True | Square animation `specfx` 13 "WARPNODE" on the targeted square |
| 54 | Spell Lock | Square animation: the Sorcery realm picture (`specfx` 1) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 55 | Enchant Road | Map animation `specfx` 46 "FULLBLUE" played 22 steps with the palette recoloured (`Open_File_Animation__HACK`), with its sound |
| 56 | Flight | Square animation: the Sorcery realm picture (`specfx` 1) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 57 | Wind Mastery | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 19 "WINDMAST" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 58 | Spell Blast | Target-wizard screen: `specfx` 40 "SPELBLST" on the target wizard's portrait (`Target_Wizard_Screen`) |
| 59 | Aura of Majesty | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 18 "MAJESTY" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 61 | Disjunction True | None found overland: `specfx` 47 is loaded for the spell but its target screen plays no animation; settle in the task |
| 62 | Invisiblity | Square animation: the Sorcery realm picture (`specfx` 1) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 63 | Wind Walking | Square animation: the Sorcery realm picture (`specfx` 1) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 65 | Storm Giant | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Powerful Creature Summons" (MUSIC 013 to 016) |
| 68 | Stasis | Square animation `specfx` 53 "STASIS" on the stack |
| 69 | Magic Immunity | Square animation: the Sorcery realm picture (`specfx` 1) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 71 | Djinn | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 72 | Spell Ward | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 75 | Great Unsummoning | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 39 "UNSUMMON" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 76 | Spell Binding | None found overland: `specfx` 44 "SPELBIND" is loaded for the spell but `Spell_Target_Global_Enchantment_Screen` plays no animation; settle in the task |
| 77 | Flying Fortress | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 78 | Sky Drake | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 79 | Suppress Magic | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 20 "SPLMASTR" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 80 | Time Stop | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 21 "TIMESTOP" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 84 | Hell Hounds | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Weak Creature Summons" (MUSIC 013 to 016) |
| 85 | Corruption | Square animation `specfx` 7 on the square |
| 86 | Eldritch Weapon | Square animation: the Chaos realm picture (`specfx` 2) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 87 | Wall of Fire | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 92 | Fire Giant | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 93 | Chaos Channels | Square animation: the Chaos realm picture (`specfx` 2) on the unit |
| 94 | Flame Blade | Square animation: the Chaos realm picture (`specfx` 2) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 95 | Gargoyles | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 97 | Doom Bat | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 98 | Raise Volcano | Square animation `specfx` 11 "VOLCANO" on the square |
| 99 | Immolation | Square animation: the Chaos realm picture (`specfx` 2) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 100 | Chimeras | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 103 | Chaos Spawn | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Powerful Creature Summons" (MUSIC 013 to 016) |
| 106 | Efreet | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Powerful Creature Summons" (MUSIC 013 to 016) |
| 107 | Fire Storm | Square animation `specfx` 6 on the enemy stack, then the damage |
| 110 | Chaos Rift | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the negative spell music |
| 111 | Hydra | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 113 | Meteor Storm | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 28 "METEORS" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 114 | Great Wasting | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 27 "GRTWASTE" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 116 | Chaos Surge | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 25 "CHAOSRGE" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 117 | Doom Mastery | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 26 "DOOMSTRY" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 118 | Great Drake | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 119 | Call the Void | Square animation `specfx` 12 "CALLVOI2" on the city square |
| 120 | Armageddon | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 29 "ARMAGEDN" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 121 | Bless | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 123 | Endurance | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 124 | Holy Weapon | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 126 | Holy Armor | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 127 | Just Cause | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 33 "JUSTCAUS" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 129 | Guardian Spirit | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Weak Creature Summons" (MUSIC 013 to 016) |
| 130 | Heroism | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 131 | True Sight | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 132 | Plane Shift | Plane-shift transition (`Cast_Plane_Shift_Draw`): the stack's square is captured on the other plane and swapped in, with the spell's sound |
| 133 | Resurrection | Square animation: the Life realm picture (`specfx` 3) on the summon-circle square, after the hero is hired |
| 135 | Planar Seal | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 35 "PLANSEAL" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 136 | Unicorns | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 138 | Planar Travel | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 139 | Heavenly Light | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 141 | Lionheart | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 142 | Incarnation | None (the original plays no animation for Incarnation) |
| 143 | Invulnerability | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 144 | Righteousness | Square animation: the Life realm picture (`specfx` 3) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 145 | Prosperity | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 146 | Altar of Battle | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; the Altar of Battle building appears in the cityscape |
| 147 | Angel | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Powerful Creature Summons" (MUSIC 013 to 016) |
| 148 | Stream of Life | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; the Stream of Life building appears in the cityscape |
| 152 | Inspirations | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 153 | Astral Gate | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; the Astral Gate building appears in the cityscape |
| 154 | Holy Arms | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 34 "HOLYARMS" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 155 | Consecration | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; then `Apply_Consecration` |
| 156 | Life Force | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 31 "LIFEFRCE" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 157 | Tranquility | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 30 "TRANQUIL" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 158 | Crusade | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 32 "CRUSADE" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 159 | Arch Angel | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 160 | Charm of Life | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 36 "STREAMLF" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 161 | Skeletons | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Weak Creature Summons" (MUSIC 013 to 016) |
| 163 | Dark Rituals | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; the Dark Rituals building appears in the cityscape |
| 164 | Cloak of Fear | Square animation: the Death realm picture (`specfx` 4) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 166 | Ghouls | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Weak Creature Summons" (MUSIC 013 to 016) |
| 171 | Drain Power | Target-wizard screen: `specfx` 42 "DRAINPOW" on the target wizard's portrait |
| 173 | Lycanthropy | Square animation: the Death realm picture (`specfx` 4) on the unit |
| 175 | Black Channels | Square animation: the Death realm picture (`specfx` 4) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 176 | Night Stalker | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 177 | Subversion | Target-wizard screen: `specfx` 13 "WARPNODE" on the target wizard's portrait (the code loads 13, not the unused "SUBVERT" 43: probably an original slip) |
| 178 | Wall of Darkness | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 180 | Shadow Demons | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Avg Creature Summons" (MUSIC 013 to 016) |
| 181 | Wraith Form | Square animation: the Death realm picture (`specfx` 4) on the unit, map scrolled to it first (`Spell_Animation_Screen`) |
| 183 | Evil Presence | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the negative spell music |
| 184 | Wraiths | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Powerful Creature Summons" (MUSIC 013 to 016) |
| 185 | Cloud of Shadow | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |
| 186 | Warp Node | Square animation `specfx` 13 "WARPNODE" on the node |
| 187 | Black Wind | Square animation `specfx` 14 "BLACKWND", on the target square (`Spell_Animation_Load_Graphics` returns 1 for it, a different draw placement: check) |
| 188 | Zombie Mastery | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 17 "ZOMBIES" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 189 | Famine | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the negative spell music |
| 190 | Cursed Lands | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the negative spell music |
| 191 | Cruel Unminding | Target-wizard screen: `specfx` 41 "CRUNMIND" on the target wizard's portrait |
| 193 | Death Knights | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 196 | Pestilence | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the negative spell music |
| 197 | Eternal Night | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 15 "ETRNIGHT" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 198 | Evil Omens | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 16 "EVLOMENS" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 199 | Death Wish | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 38 "DETHWISH" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 200 | Demon Lord | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Mighty Creature Summons" (MUSIC 013 to 016) |
| 201 | Magic Spirit | Summon scene (`Summon_Animation`): the SPELLSCR summon background of the unit's race (46 + race), the caster's figure and the flame frame, `cmbtfx` 22 over it; music "Weak Creature Summons" (MUSIC 013 to 016) |
| 203 | Summoning Circle | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music; the Summoning Circle building appears in the cityscape |
| 204 | Disenchant Area | Square animation `specfx` 9 on the targeted square (`Spell_Animation_Screen`) |
| 206 | Detect Magic | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 37 "DETMAGIC" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 207 | Enchant Item | Summon scene in grey arcane colours (`Summon_Animation(-1, 3)`) before the item crafting screen |
| 208 | Summon Hero | Summon scene (`Summon_Animation`) with the hero's portrait figure (male -2 or female -3 background variant), music "Mighty Creature Summons", before the hire pop-up |
| 209 | Awareness | Global spell animation (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`): the casting wizard's MOODWIZ face in the diplomacy mirror (BACKGRND 18, DIPLOMAC 1) with `specfx` 56 "AWARENES" composed through the MASK1/MASK2 pictures (specfx 54, 55) over the map; ends on a click or by itself |
| 210 | Disjunction | None found overland: `specfx` 48 is loaded for the spell but its target screen plays no animation; settle in the task |
| 211 | Create Artifact | Summon scene in grey arcane colours (`Summon_Animation(-1, 3)`) before the item crafting screen |
| 212 | Summon Champion | Summon scene as Summon Hero, champion variant (-20 male, -30 female), before the hire pop-up |
| 213 | Spell of Mastery | See task 287 (swirling vortex animation and sound) |
| 214 | Spell of Return | City spell scroll (`Cast_Spell_City_Enchantment_Animation_1` and `_2`): SPELLSCR 73 "CITYSPEL" with the city's cityscape, "You cast X" or "<wizard> casts X", the positive spell music |

## Not tied to one spell

| Animation | Where | Notes |
|---|---|---|
| Learn-spell animation | `Learn_Spell_Animation` (`Spellbook.c`), after research completes | **Done** (`learn_spell.rs`): the lab (WIZLAB), then the library page with the entry decoded under `specfx` 49 "NEWSPELL"; off path the box "You have mastered the spell of X" (`spellscr` 60). Still to do: the New Spell music (task 76) and the fades. |
| Fizzle and dispel boxes | `Fizzle_Notification`, `Notify2` | `specfx` 50 FIZZLE, 51 and 52 DISPELL: the boxes exist in the client; check they use these pictures. |
