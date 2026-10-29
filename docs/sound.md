# Sound and music

What the original plays and when, taken from ReMoM (`~/projects/ReMoM`: `MoX/src/SOUND.h`, the callers in
`MoM/src/`, `doc/MoX-Sounds.md`, `doc/Audio/`) and from the original's data files. This is the catalogue the
audio tasks (76, 97, 266 to 268, 357) build on. The audio files themselves are copyrighted: they are extracted
into `assets-local/` for the author's own copy and never committed (`design.md` section 9, `local-assets.md`).
Track and effect numbers below are identifiers, not content.

**Extraction (task 362) and the player (tasks 76 and 97) are built; the music has nothing to play yet.** The client
(`crates/f4x-client/src/audio.rs`, on macroquad's audio: OGG and WAV, needs ALSA on Linux) plays the effects and
music from `assets-local/audio` and is silent without the files. The songs are XMI, which it cannot play: it looks
for `music/NNN_name.ogg`, which task 363 will render, so until then the effects play and the music is silent.
`content/sound.ron` (made by `scripts/sound_table.py`) says which entry each spell and unit plays.

What is wired:

| Where | Plays |
|---|---|
| A left press that lands on a button (any screen; on the overland map and the battle field only outside the map window) | click, `SOUNDFX 2` |
| Battle: a unit walks (each square), strikes in melee, shoots | its move sound, its `CMBTSND` melee sound, the shot sound of its missile type |
| Battle: a spell is cast by a wizard or a unit | the spell's sound, as its picture starts |
| Overland: a spell is cast (yours; rivals' with Enemy Spells on) | the spell's sound |
| Music: main menu; battle (the foe wizard's pair, else the general pair); diplomacy (the wizard's friendly or angry tune); overland (the three standing tracks, re-picked from the power history each turn, where the original does it every fifth) | looped |
| Music, short pieces while their box is up: global spell (its own), Spell of Mastery started, random event (good or bad, with Event Music), building finished, site discovered, spell learned, spell fizzled, new outpost, game over (win, lose by conquest, lose by magic) | once; the background resumes when the box closes |
| Switches | Sound Effects gates every effect, Background Music every track but the two event pieces, Event Music those two and the fanfare (task 266 to 268) |

Also wired: volume sliders for the music and the effects at the bottom of the Settings screen (saved in
`saves/preferences.txt` as `music_volume` and `effects_volume`); the level-up box (`level_up.rs`) with the fanfare as an
event piece; the scream (`SOUNDFX` 34 for a feminine wizard, 35 otherwise, by the wizard's gender in `wizards.ron`) when a
wizard is defeated or banished; the vortex's round (`SOUNDFX` 7).

Not wired: the summon scene, the city-spell scroll and the Force of Nature scene (task 364), crossfading (365, ours),
the teleport and tunnel sounds (137), the scream inside the conquest scene (360; for now it plays when the notice
arrives), the intro (85), and the warning click. The overland map has no unit-move sound in the original either.

## 1. The files

All in the game folder (`reference/mom-classic/.../`), LBX archives like the art (`extract-art.md` section 1).

| File | Entries | What it holds | Format |
|---|---|---|---|
| `MUSIC.LBX` | 116 | All music, section 2 | XMI (Extended MIDI, AIL), played on a MIDI synth: the original used the `FAT*` instrument banks (`FAT.AD`, `FAT.OPL`, `FAT.MT`, `FAT-GEN.CAT`, `FAT-OPL3.BNK`) |
| `SOUNDFX.LBX` | 119 | Effects, section 4: clicks, unit moves, attacks, spells, screams | VOC, 8-bit unsigned mono PCM (`MoX-Sound-VOC.md`) |
| `CMBTSND.LBX` | 8 | Melee hit sounds, one per weapon sound class (0 to 7) | VOC |
| `NEWSOUND.LBX` | 21 | Later additions: spell sounds 1 to 20 and a unit move sound (entry 0) | VOC |
| `INTROSND.LBX`, `INTROSFX.LBX` | 2, 6 | The opening movie's music and effects (task 85) | XMI, VOC |
| `SNDDRV.LBX` | | Sound card drivers of the DOS game | not needed |

Decoding: a VOC to WAV is a header swap (sample rate `1000000 / (256 - byte)`, 8-bit mono). An XMI needs a
converter to standard MIDI (its meta events 0x20 and 0x21 are ignored by most players) and then a soft synth with a
sound font; the `FAT*` banks are FM and Roland patches and do not play on a soft synth as they are, so the music
will sound different from the DOS game unless it is rendered on a General MIDI font (ReMoM uses SDL_mixer with
FluidSynth: `Audio/MidiXmidiEvents-Sdl2MixerFluidSync.md`).

## 2. The three switches

| Setting | Controls |
|---|---|
| Sound Effects | Every `SOUNDFX`, `CMBTSND` and `NEWSOUND` effect of section 4, the button clicks included (`Play_Left_Click` tests it) |
| Background Music | The looping overland and combat music, the diplomacy music, the intro and every spell/summon/building/outpost/site/win/lose piece that interrupts it |
| Event Music | Only the two event pieces (`Events`, `Bad_Event`) of random events and the level-up fanfare |

The pieces that interrupt the background music (section 3, "short pieces") check Background Music in the original
(`Summon_Animation`, `WIZ_GlobalSpellAnim`, `Fizzle_Notification`, `Create_Outpost`, ...), not Event Music. Task 268's
description ("a wizard's victory or defeat, a city falling, a new spell or hero play only when Event Music is on")
does not match the original: Event Music covers random events and the hero's fanfare.

Every short piece does the same: stop all sound, play the piece once, and when the screen closes call
`Play_Background_Music` which picks the background track again.

## 3. Music

### 3.1 Background (overland)

Three tracks, `Bkrgnd_Good` (99), `Bkgrnd_Neutral` (100), `Bkgrnd_Negative` (101), picked by
`Get_Background_Music` from the human wizard's power history against the other wizards (the `history` array, index
`min(turn - 1, 287)`):

1. Neutral by default.
2. **Good** if the player's power plus 10% of the best other wizard's power reaches the best.
3. **Negative** if there is a weakest other wizard with power above 0 and the player's power minus 10% of the
   weakest's is at or below it. (Negative wins when both hold.)

The pick is re-evaluated by `Increment_Background_Music` in `Next_Turn_Calc`, right after `Record_History`, on every
fifth turn (a counter): when the pick changes the track restarts. It is chosen again after every short piece, after
loading a game (`Loaded_Game_Update`) and when the main screen starts (`Screen_Control`).

Our game: needs the per-wizard power history (not checked whether the core keeps one) or another measure of "ahead"
and "behind".

### 3.2 Screens and situations

| Where | Music | Source (ReMoM) | Ours (client file) |
|---|---|---|---|
| Opening movie | `Intro` (112), with `SFX IntroT1/T2/T3/M` | `INTRO.c Draw_Logos` | task 85 |
| Main menu | `Main_Menu` (104) | `ReMoM.c` | `menu_screen.rs` |
| New game screens, load, hall of fame, options | none (the menu music stops when you leave it; the screens are silent) | no `Play_Sound` in `NewGame.c`, `LoadScr.c`, `SCORE.c` (they only stop sound) | |
| Overland map and every screen reached from it (city, army, magic, spellbook, items, surveyor ...) | background track, section 3.1; screens do not change it | `Play_Background_Music` | `main.rs` |
| Combat vs a wizard | that wizard's pair: `<Wizard>_Cmbt1` or `_Cmbt2`, random (71 to 98, two per wizard in the order of the 14 wizards) | `Combat_Screen` | `battle_screen.rs` |
| Combat vs a neutral side (monsters, lairs, independent city) | `Combat_1` (102) or `Combat_2` (103), random | same | |
| Diplomacy room | the wizard's own pair: `<Wizard>_Good` and `<Wizard>_Bad` (41 to 68). The room starts with the wizard's mood from the message record: mood 0 and 1 play Good, mood 2 plays Bad, and it switches when the mood changes | `Start_Diplomacy_Music`, `Diplomacy_Screen_Load` | `diplomacy_screen.rs` |
| Hiring a hero or mercenaries | `Hiring_Screen` (70) exists in the file; ReMoM never starts it. Unverified whether the original did | no caller | `offer_prompt.rs` |

The wizards in order, as the file numbers them: Merlin, Raven, Sharee, Lo Pan, Jafar, Oberic, Rjak, Sssra, Tauron,
Freya, Horus, Ariel, Tlaloc, Kali. The pair index is `wizard_id * 2` (plus 1 or 2 for the combat pair). A wizard
that is a custom portrait (an addition of ours, such as the new wizards of `PLANNED.md`) has no
tracks, so the choice must fall back to a general pair (`Combat_1/2`, `Bkrgnd_*`).

### 3.3 Short pieces (stop the background, play once, resume)

| Piece (id) | When | Source | Ours |
|---|---|---|---|
| `New_Outpost` (107) | A settler founds an outpost; plays under the "name your outpost" box | `Create_Outpost` | `outpost_screen.rs`, event `CityFounded` |
| `Bldng_Finished` (108) | The building-complete box after a turn | `City_Built_Building_Message` | `built_popup.rs`, `ProductionCompleted` |
| `Site_Discovery` (105) | A stack steps on a lair, node, tower, temple or other site and the question/treasure box is shown | `Lair_Combat__WIP` | `site_prompt.rs`, `SiteEncounter` (task 357) |
| `Events` (106) / `Bad_Event` (114) | A random event's box. Good events play `Events`, bad ones `Bad_Event`. Good Moon plays `Bad_Event` when the human has no Death book, Bad Moon when it has no Life book. Only with Event Music on | `Show_Event_Message` | `event_popup.rs`, `RandomEvent` |
| `New_Spell` (40) | A spell is learned: the learn animation, and also for the global spells with no piece of their own (below) | `Learn_Spell_Animation`, `WIZ_GlobalSpellAnim` | `learn_spell.rs`, `SpellLearned` |
| `Fanfare` (115) | A hero gains a level (the level-up popup) | `Hero_LevelUp_Popup` | `level_up.rs` (task 76; plays with Event Music on, as in the original) |
| `Weak/Avg/Powerful/Mighty_Summon` (13 to 16) | The summon scene of a summoning spell, by the unit's rarity (0 to 3) | `Summon_Animation` | `battle_screen.rs`/`spell` scenes, `SpellCast` |
| City spell, good (`Good_<Realm>` 0 to 5) / bad (`Bad_<Realm>` 6 to 11) | The scroll of a city enchantment: the realm picks the pair (arcane, chaos, death, life, nature, sorcery), a negative city spell (curse) plays the bad one | `Cast_Spell_City_Enchantment_Animation_Load_Music` | `spell` scenes |
| Global spell pieces (19 to 39) | The global-spell scene. The piece is the spell's own, entry `19 + Param0` of its data row (below); if that is above 21 the generic `New_Spell` plays instead. Death Wish plays `Wind_Mastery`, Great Unsummoning `Suppress_Magic` | `WIZ_GlobalSpellAnim` | `global_spell.rs` |
| `Spell_Failure` (17) | The "your spell was countered" box (Spell Blast, Disjunction, Suppress Magic) | `Fizzle_Notification` | the fizzle box, `SpellFizzled`, `SpellBlasted` |
| `SoM_Started` (12) | Spell of Mastery is cast (the swirl, the announcement) | `Cast_Spell_Of_Mastery` | task 287 |
| `LOSE_By_Magic` (111) | The Spell of Mastery of another wizard completes and you lose | `Spell_Of_Mastery_Lose` | `game_over_screen.rs` |
| `Force_of_Nature` (27) | Casting Force of Nature (its own scene) | `Spells129.c` | |
| `WIN_Military` (109) / `LOSE_Military` (110) | The conquest scene (the last enemy wizard falls) and the win/lose animations | `Conquest_Animation`, `Win_Animation`, `Lose_Animation` | `game_over_screen.rs`, task 360 |
| `WIN_By_Magic` (69) | Exists, no caller in ReMoM (the win by Spell of Mastery) | | game over |
| `Upkeep_Warning` (18) | Exists, no caller in ReMoM | | `upkeep_prompt.rs` |
| `Fanf_not_used` (113) | Unused | | |

The global spell pieces (data row `Param0`, music = 19 + Param0): 0 Eternal Night (19), 1 Evil Omens (20),
2 Zombie Mastery (21), 3 Aura of Majesty (22), 4 Wind Mastery (23), 5 Suppress Magic (24), 6 Time Stop (25),
7 Nature's Awareness (26), 8 Force of Nature (27), 9 Herb Mastery (28), 10 Chaos Surge (29), 11 Doom Mastery (30),
12 Great Wasting (31), 13 Meteor Storm (32), 14 Armageddon (33), 15 Tranquility (34), 16 Life Force (35),
17 Crusade (36), 18 Just Cause (37), 19 Holy Arms (38), 20 Wrath of God (39). The cast scene shows for every
wizard's global cast, so the same piece plays for a rival's cast (with Overland Spell Events on).

Win and lose both play `Play_Background_Music` after the scene. Hall of Fame and the score screen only stop sound.

## 4. Sound effects

The original has **far fewer effects than task 97 assumes**. ReMoM has a sound call in exactly these places; there
is none for turn end, city growth, building placed, production, an overland unit move, unit death, healing, hero
hire, item pickup or any button except the two clicks below. (Task 97's "spoken lines": ReMoM has no voice
playback; the diplomacy room has music only.)

### 4.1 Clicks

| Effect | When |
|---|---|
| `SOUNDFX 2` Left_Btn_Clk ("OKBUTTN") | `Play_Left_Click`: a click on a button, list row, field or hot key in most screens (about 110 call sites) |
| `SOUNDFX 0` Std_Btn_Clk | `Play_Standard_Click`: the combat counter message (`CMB_CounterMessage`) and the warning box (`Warn`) |
| `SOUNDFX 2` again | `Play_Left_Click__DUPE` (same effect, a second copy of the function) on the main screen's unit window and move map right-click, hire popups' Accept/Reject, item screen right-clicks, spellbook page turns and Escape, the Chancellor and the spell disjunct/bind boxes |

The two duplicates exist only because the original had two copies of the function; one cue `click` is enough.
Cue for the standard click: `click_standard` (or the same).

### 4.2 Combat

| Effect | When | Choice |
|---|---|---|
| Unit move | Each square a unit walks in a battle (also the AI's `Auto_Move_Unit`), repeated per step | High nibble of the unit's `Sound` byte (section 4.5): 0 silent, 5 Troops, 6 Cavalry, 7 Flyer_S, 8 Flyer_L, 9 Monster_S, 10 Monster_L, 13 `NEWSOUND 0` |
| Melee | When a melee attack starts, the attacker's sound | Low nibble: `CMBTSND` entry 0 to 7; 8 silent |
| Ranged attack | The shot leaves, by the ranged type: Catapult 15, Cannon 16 (`EXPLODE`), Bow 17, Sling 18, Lightning 19, Fireball 20, Illusion (sorcery) 21, Death bolt 22, Ice 23, Priest/shaman 24, Drow 25, Sprite and Nature bolt 26 | `Ranged_Animation` |
| Teleport / Tunnel | Instant movement of a unit | `SOUNDFX 11` / `12` (`Battle_Unit_Teleport`, `Battle_Unit_Tunnel`) |
| Summon in combat | A unit appears in battle (Combat Summon: `SOUNDFX 107`) | `Combat_Summon` |
| Magic Vortex | `Flyer_S` (7) as the vortex's round moves | `Vortex_Combat_Round` |
| Combat spell | The cast sound of the spell (section 4.3), played with its animation; stops with `SFX_Silence` (106) first | `Combat_Load_Spell_Sound_Effect` |
| Scream | The conquest scene zaps the defeated wizard: `FemaleScream` (34) or `MaleScream` (35) by the defeated wizard's id (the code takes id - 2: female for 2, 8, 11 and 12 and its comments name Sharee, Kali, Ariel and Freya; male for the rest. The id order against the music's order is not checked); played at stage 11 and again on each stroke while Sound Effects is on | `Conquest_Animation_Draw` |

`SOUNDFX 106` ("Silence", a silent clip) is played before and after many sounds to cut off the previous one; it is
not a sound to reproduce. In our audio layer a new cue simply stops the previous effect of its channel.

Gaps in the original that we can fill (ours, optional): no sound when a unit takes damage or dies, no wall or
gate hit, no bolt/fire for the AI's moves when "show enemy moves" is off, no victory/defeat sting in combat (the
combat results scroll is silent in the original).

### 4.3 Spell cast sounds

Each spell row has a `Sound` byte (signed): positive = `SOUNDFX` entry, negative = `NEWSOUND` entry, 0 = none. It is
played with the cast animation, overland (`Spell_Animation_Screen`) and in battle (`Combat_Load_Spell_Sound_Effect`).
The values (realm sounds 28 to 33 are shared by many spells):

| Sound | Spells | Which |
|---|---|---|
| SOUNDFX 5 (Troops) | 13 | war_bears, sprites, giant_spiders, cockatrices, basilisk, stone_giant, gorgons, behemoth, nature_awareness, colossus, herb_mastery, great_wyrm, natures_wrath |
| SOUNDFX 6 (Cavalry) | 9 | wind_mastery, aura_of_majesty, storm_giant, djinn, mass_invisibility, great_unsummoning, sky_drake, suppress_magic, time_stop |
| SOUNDFX 7 (Flyer_S) | 15 | hell_hounds, fire_giant, gargoyles, doom_bat, chimeras, chaos_spawn, efreet, warp_reality, hydra, meteor_storm, great_wasting, chaos_surge, doom_mastery, great_drake, armageddon |
| SOUNDFX 8 (Flyer_L) | 12 | just_cause, guardian_spirit, planar_seal, unicorns, incarnation, angel, holy_arms, life_force, tranquility, crusade, arch_angel, charm_of_life |
| SOUNDFX 9 (Monster_S) | 12 | skeletons, ghouls, lycanthropy, night_stalker, shadow_demons, wraiths, zombie_mastery, death_knights, eternal_night, evil_omens, death_wish, demon_lord |
| SOUNDFX 10 (Monster_L) | 7 | magic_spirit, enchant_item, summon_hero, create_artifact, summon_champion, spell_of_mastery, spell_of_return |
| SOUNDFX 20 (ATK_Fire) | 1 | fire_bolt |
| SOUNDFX 21 (ATK_Illusion) | 1 | psionic_blast |
| SOUNDFX 22 (ATK_Death) | 2 | life_drain, word_of_death |
| SOUNDFX 23 (ATK_Ice) | 1 | ice_bolt |
| SOUNDFX 27 (ATK_LightningBolt) | 2 | call_lightning, lightning_bolt |
| SOUNDFX 28 (Gen_Nature) | 16 | resist_elements, wall_of_stone, giant_strength, stone_skin, water_walking, natures_eye, change_terrain, path_finding, transmute, natures_cures, elemental_armor, iron_skin, gaias_blessing, regeneration, entangle, earth_gate |
| SOUNDFX 29 (Gen_Sorcery) | 16 | resist_magic, floating_island, guardian_wind, word_of_recall, blur, spell_lock, flight, spell_blast, invisiblity, wind_walking, stasis, magic_immunity, haste, spell_ward, spell_binding, flying_fortress |
| SOUNDFX 30 (Gen_Chaos) | 7 | eldritch_weapon, wall_of_fire, chaos_channels, flame_blade, immolation, flame_strike, chaos_rift |
| SOUNDFX 31 (Gen_Life) | 23 | bless, star_fires, endurance, holy_weapon, healing, holy_armor, heroism, true_sight, dispel_evil, planar_travel, heavenly_light, prayer, lionheart, invulnerability, righteousness, prosperity, altar_of_battle, stream_of_life, mass_healing, high_prayer, inspirations, astral_gate, consecration |
| SOUNDFX 32 (Gen_Death) | 10 | weakness, cloak_of_fear, black_channels, wall_of_darkness, berserk, wraith_form, evil_presence, cloud_of_shadow, famine, pestilence |
| SOUNDFX 33 (Gen_Arcane) | 4 | summoning_circle, recall_hero, detect_magic, awareness |
| SOUNDFX 35 (MaleScream) | 1 | counter_magic |
| SOUNDFX 36 (MaleScream2) | 1 | metal_fires |
| SOUNDFX 37 (Empty2) | 1 | true_light |
| SOUNDFX 38 (Empty3) | 5 | terror, darkness, black_prayer, wrack, death_spell |
| SOUNDFX 40 (PlaneShift) | 1 | plane_shift |
| SOUNDFX 52 (MindTwist) | 1 | mana_leak |
| SOUNDFX 55 (BlackWind) | 1 | black_wind |
| SOUNDFX 56 (DrainPower) | 1 | drain_power |
| SOUNDFX 58 (CruelUnminding) | 1 | cruel_unminding |
| SOUNDFX 60 (DarkRituals) | 1 | dark_rituals |
| SOUNDFX 62 (CursedLands) | 1 | cursed_lands |
| SOUNDFX 69 (Earth2Mud) | 1 | earth_to_mud |
| SOUNDFX 72 (Earthquake) | 1 | earthquake |
| SOUNDFX 77 (DispelMagic) | 2 | dispel_magic_true, dispel_magic |
| SOUNDFX 80 (DisenchantTrue) | 4 | cracks_call, disenchant_true, disjunction_true, disenchant_area |
| SOUNDFX 86 (EnchantRoad) | 1 | enchant_road |
| SOUNDFX 91 (Disrupt) | 1 | disrupt |
| SOUNDFX 92 (Shatter) | 1 | shatter |
| SOUNDFX 93 (Fireball) | 1 | fireball |
| SOUNDFX 96 (Firestorm) | 1 | fire_storm |
| SOUNDFX 98 (RaiseVolcano) | 1 | raise_volcano |
| SOUNDFX 99 (Disintegrate) | 1 | disintegrate |
| SOUNDFX 100 (DoomBolt) | 1 | doom_bolt |
| SOUNDFX 102 (CalltheVoid) | 1 | call_the_void |
| SOUNDFX 103 (Corruption) | 1 | corruption |
| SOUNDFX 107 (CombatSummon) | 8 | earth_elemental, phantom_warriors, nagas, phantom_beast, air_elemental, fire_elemental, magic_vortex, animate_dead |
| SOUNDFX 109 (MoveFortress) | 1 | move_fortress |
| SOUNDFX 112 (Disjunction) | 1 | disjunction |
| NEWSOUND 1 | 1 | raise_dead |
| NEWSOUND 2 | 1 | holy_word |
| NEWSOUND 3 | 1 | black_sleep |
| NEWSOUND 4 | 1 | possession |
| NEWSOUND 5 | 1 | warp_node |
| NEWSOUND 6 | 1 | subversion |
| NEWSOUND 7 | 1 | petrify |
| NEWSOUND 8 | 1 | ice_storm |
| NEWSOUND 9 | 1 | vertigo |
| NEWSOUND 10 | 1 | banish |
| NEWSOUND 11 | 1 | confusion |
| NEWSOUND 12 | 1 | mind_storm |
| NEWSOUND 13 | 1 | creature_binding |
| NEWSOUND 14 | 1 | warp_creature |
| NEWSOUND 15 | 1 | warp_wood |
| NEWSOUND 16 | 1 | warp_lightning |
| NEWSOUND 17 | 1 | call_chaos |
| NEWSOUND 18 | 1 | earth_lore |
| NEWSOUND 19 | 1 | resurrection |
| NEWSOUND 20 | 1 | web |

The summon spells and the big global ones use the numbers 5 to 10 by realm (nature 5, sorcery 6, chaos 7, life 8,
death 9, arcane 10); in `SOUND.h` those numbers are named as the unit-move sounds (troops, cavalry, flyers,
monsters). Whether they sound like it was not listened to.

`SOUNDFX` entries that neither a spell row nor a call site uses: 14 (`Empty1`), 37 and 38 (`Empty2`, `Empty3`), 117.
Entries 39 to 114 are the spell-specific sounds (`StarFires`, `PlaneShift`, `RaiseDead`, ...); their names are in
`MoX/src/SOUND.h`, enum `SOUND_FX`. Several of them (for example 71 `CracksCall`, 111 `WallofStone`) are not what
the spell row of that spell names (Cracks Call uses 80, Wall of Stone 28): the rows are the authority.

### 4.4 The intro

`INTRO.c Draw_Logos` plays `Intro` music, then `SFX_IntroT1`, `IntroT2`, `IntroT3`, `IntroM`, `ATK_LightningBolt`,
and `Unknown116`/`118` in step with the scenes (task 85 reads the timing).

### 4.5 Unit sound bytes

Every unit has one byte: high nibble = move sound, low nibble = melee sound. ReMoM sets them at start-up
(`NEXTTURN.c`, from line 1863) rather than in the unit table, so a modder needs them per unit. The values and
the units with each:

| Byte | Move (high) | Melee (low) | Units | Which |
|---|---|---|---|---|
| 6 | 0: silent | CMBTSND 6 | 1 | dark_elf_nightblades |
| 8 | 0: silent | silent | 3 | floating_island, phantom_beast, phantom_warriors |
| 85 | 5: Troops | CMBTSND 5 | 5 | beastmen_minotaurs, troll_spearmen, troll_swordsmen, troll_halberdiers, troll_war_trolls |
| 86 | 5: Troops | CMBTSND 6 | 46 | barbarian_spearmen, barbarian_swordsmen, barbarian_bowmen, barbarian_berserkers, beastmen_spearmen, beastmen_swordsmen, beastmen_halberdiers, beastmen_bowmen, beastmen_engineers, dark_elf_spearmen, dark_elf_swordsmen, dark_elf_halberdiers, dwarven_swordsmen, dwarven_halberdiers, dwarven_engineers, dwarven_hammerhands, gnoll_spearmen, gnoll_swordsmen, gnoll_halberdiers, gnoll_bowmen, halfling_spearmen, halfling_swordsmen, halfling_bowmen, high_elf_spearmen, high_elf_swordsmen, high_elf_halberdiers, high_elf_longbowmen, high_men_spearmen, high_men_swordsmen, high_men_bowmen, high_men_engineers, high_men_pikemen, lizardman_spearmen, lizardman_swordsmen, lizardman_halberdiers, lizardman_javelineers, nomad_spearmen, nomad_swordsmen, nomad_bowmen, nomad_pikemen, nomad_rangers, orc_spearmen, orc_swordsmen, orc_halberdiers, orc_bowmen, orc_engineers |
| 102 | 6: Cavalry | CMBTSND 6 | 35 | dwarf, barbarian, dervish, beastmaster, bard, orc_warrior, healer, huntress, thief, war_monk, warrior_mage, assassin, ranger, golden_one, ninja, rogue, amazon, unknown, swordsman, paladin, black_knight, elven_archer, knight, chaos_warrior, chosen, barbarian_cavalry, beastmen_centaurs, dark_elf_cavalry, high_elf_cavalry, high_elf_elven_lords, high_men_cavalry, high_men_paladins, nomad_horsebowmen, orc_cavalry, death_knights |
| 103 | 6: Cavalry | CMBTSND 7 | 9 | sage, druid, magician, wind_mage, witch, warlock, illusionist, priestess, necromancer |
| 112 | 7: Flyer_S | CMBTSND 0 | 2 | great_drake, sky_drake |
| 113 | 7: Flyer_S | CMBTSND 1 | 1 | demon_lord |
| 114 | 7: Flyer_S | CMBTSND 2 | 1 | gorgons |
| 116 | 7: Flyer_S | CMBTSND 4 | 2 | draconian_doom_drakes, orc_wyvern_riders |
| 130 | 8: Flyer_L | CMBTSND 2 | 8 | gargoyles, chaos_spawn, chimera, doom_bat, demon, wraiths, shadow_demons, air_elemental |
| 131 | 8: Flyer_L | CMBTSND 3 | 3 | beastmen_manticores, fire_elemental, guardian_spirit |
| 132 | 8: Flyer_L | CMBTSND 4 | 1 | nomad_griffins |
| 134 | 8: Flyer_L | CMBTSND 6 | 16 | draconian, trireme, galley, catapult, warship, dark_elf_nightmares, draconian_spearmen, draconian_swordsmen, draconian_halberdiers, draconian_bowmen, draconian_engineers, draconian_air_ship, high_elf_pegasai, efreet, angel, arch_angel |
| 135 | 8: Flyer_L | CMBTSND 7 | 4 | draconian_shaman, draconian_magicians, sprites, cockatrices |
| 146 | 9: Monster_S | CMBTSND 2 | 1 | nagas |
| 147 | 9: Monster_S | CMBTSND 3 | 6 | magic_spirit, hell_hounds, night_stalker, werewolves, war_bears, giant_spiders |
| 149 | 9: Monster_S | CMBTSND 5 | 1 | troll_settlers |
| 150 | 9: Monster_S | CMBTSND 6 | 24 | barbarian_settlers, beastmen_settlers, dark_elf_settlers, draconian_settlers, dwarven_steam_cannon, dwarven_settlers, gnoll_settlers, gnoll_wolf_riders, halfling_settlers, halfling_slingers, high_elf_settlers, high_men_settlers, klackon_spearmen, klackon_swordsmen, klackon_halberdiers, klackon_engineers, klackon_settlers, klackon_stag_beetle, lizardman_settlers, nomad_settlers, orc_settlers, skeletons, ghouls, zombies |
| 151 | 9: Monster_S | CMBTSND 7 | 15 | barbarian_shaman, beastmen_priests, beastmen_magicians, dark_elf_priests, dark_elf_warlocks, halfling_shaman, high_elf_magicians, high_men_priests, high_men_magicians, lizardman_shaman, nomad_priests, nomad_magicians, orc_shaman, orc_magicians, troll_shaman |
| 161 | 10: Monster_L | CMBTSND 1 | 3 | hydra, earth_elemental, behemoth |
| 162 | 10: Monster_L | CMBTSND 2 | 1 | troll_war_mammoths |
| 165 | 10: Monster_L | CMBTSND 5 | 2 | colossus, storm_giant |
| 179 | 11: Teleport | CMBTSND 3 | 1 | unicorns |
| 182 | 11: Teleport | CMBTSND 6 | 1 | djinn |
| 192 | 12: Tunneling | CMBTSND 0 | 1 | great_wyrm |
| 210 | 13: NEWSOUND 0 (Monster_M) | CMBTSND 2 | 1 | basilisk |
| 213 | 13: NEWSOUND 0 (Monster_M) | CMBTSND 5 | 2 | fire_giant, stone_giant |
| 214 | 13: NEWSOUND 0 (Monster_M) | CMBTSND 6 | 2 | dwarven_golem, lizardman_dragon_turtle |

## 5. What our game has to supply

### 5.1 Triggers from the core

The core stays free of audio. The client maps `Event` values (and its own screen changes) to cues. The ones above
that have an `Event` today: `CityFounded`, `ProductionCompleted`, `SpellLearned`, `SpellCast` (global, city,
summon, battle), `SpellFizzled`, `SpellBlasted`, `MasteryStarted`, `RandomEvent`, `SiteEncounter`, `BattleStarted`,
`Battle(..)` (moves, melee, ranged, casts), `WizardDefeated`, `GameOver`, `CityCaptured`. Missing for a cue:

- A hero's level-up (there is no popup; the level is in `unit_sheet.rs`): add an event or detect the change.
- The unit rarity for the summon piece (0 weak to 3 mighty): derive it from the unit's cost or add it to the
  unit row (the original has it in the spell row: `Param0` of a summon spell is the unit type and the rarity is
  chosen by the spell's rarity).
- A power history for the background pick (3.1): the original records one value per turn per wizard
  (`Record_History`, 288 turns) for the score graph.
- The wizard's mood for the diplomacy music (`diplomacy.md`).

### 5.2 Rules data

The content files must carry the cues, not the file indices, so the game works with the files missing (silent)
and a mod can change them:

- `spells.ron`: `sound: Some("<cue>")` per spell, and `music:` for global spells.
- `units.ron`: `move_sound` and `melee_sound` cue names per unit (about 12 classes, section 4.5).
- A new `sound.ron`: cue name to the list of files (`assets-local/audio/<cue>.ogg` or `.wav`), the track pairs per
  wizard (so new wizards name theirs), and the three background tracks.
- The importer writes the cue names from the numbers above; the extractor (`f4x-tools extract-audio`) writes the
  files with the same names.

### 5.3 Ours (not in the original)

Candidates, to decide: end-of-turn chime, a message arrival ping for a diplomacy proposal (task 361), a
banished-wizard sting, the life and death nodes and the new wizards of `PLANNED.md`, and the "city grows" tick.
None is needed to match the original.

## 6. Open decisions

1. **Audio crate.** `rodio` (pure Rust, WAV/OGG; no MIDI), or `kira`, or SDL_mixer through `sdl2` (MIDI via
   FluidSynth as ReMoM does; heavy to build and package).
2. **How the music is made.** (d) **MT-32 emulation** with `CopperlineHQ/mt32-rs` (see below): the music as the
   original's Roland MT-32 setting played it. Other options: (a) Convert XMI to MIDI and render with a soft synth and a General MIDI sound font
   at extract time into OGG files in `assets-local/audio` (the font must be free; the OGGs stay local); (b) play
   MIDI live with `rustysynth`; (c) replace the music with a free soundtrack generated with `~/projects/soundgen`
   (ACE-Step on this machine): the only way to ship music in the CC0 repository, and it also gives the new wizards,
   life and death nodes and other additions tracks the original does not have.
3. **The effects.** VOC to WAV at extract time; ship none. A generated or recorded set could replace them the same
   way as (c).
4. **Gating.** As the original: `Sound Effects` off silences every effect including the clicks (task 266);
   `Background Music` off silences all music including the short pieces; `Event Music` only the two event pieces.

### mt32-rs (checked 2026-10-24)

**Spike built (task 363 step 1): `tools/xmi-play`** (see `docs/scripts.md`). The author's pick is the *new* MT-32 ROM
pair (control 2.07 + `mt32_pcm.rom`), not 1.07. Own XMI reader (delays, note durations to note-offs, tempo, loop
controllers 110 to 120 dropped, loops not yet followed), the `TIMB` timbres of `fat.mt` (GTL directory of patch, bank,
offset; 248-byte records) sent as SysEx into timbre memory with the patch pointed at them. Banks 55 are in the library;
bank 127 entries are not and use the unit's own timbres. `-R` renders a whole folder to OGG for the client (task 363 done); the client's `Audio::scan` finds `music/NNN_name.ogg`. The opening movie plays `112_intro` as a piece. Loops are not followed: a looping track restarts from its first note. The submodule is `third_party/mt32-rs`. Both test songs render with signal (`104_main_menu` 102 s,
`112_intro` 131 s); how right they sound is for the author's ears.


- A safe-Rust port of Munt's `mt32emu`, standalone (written for an Amiga emulator). Covers MT-32 old and new, CM-32L,
  LAPC-I and CM-32LN, claims bit-identical output to Munt at the native rate, about 0.5 s of CPU per minute of music.
- Needs the control and PCM ROMs of a real unit, passed as bytes (`Engine::open_with_analog(&control, &pcm, ..)`); the
  tests look for `MT32_CONTROL.ROM` and `MT32_PCM.ROM` in `$MT32_RS_ROMS`. The ROMs are copyrighted: they live in
  `assets-local/` like the other original files and are never committed. Which ROM revision to use is for the
  author to pick; the README says nothing about the old-versus-new differences.
- API: feed raw MIDI bytes (`Parser::parse`), pull 48 kHz stereo `i16` frames (`render`). It fits the client, or the
  extractor if we render to OGG once.
- **Licence: LGPL-2.1-or-later.** The repository is CC0. A dependency does not change the repo's licence, but a
  binary that links it (Rust links statically) is distributed under the LGPL's terms: the source of the crate and a way
  to relink must be offered. Keep it behind an optional cargo feature of the client (or in `f4x-tools` only, if the
  music is rendered at extract time, which keeps it out of the shipped game entirely and avoids the issue).
- **Maturity:** 1 star, 31 commits, no releases and no crates.io name stated; we would pin a git commit. Munt itself
  (C++) is the proven reference.
- **Still needed with it:** XMI to standard MIDI (the AIL format) and the original's MT-32 timbre bank
  (`FAT.MT`): the DOS drivers uploaded custom timbres to the unit before a song. Whether the songs rely on them is
  not checked; if they do, the converter must send them as SysEx before the first note.
- Fit with the rest: the music comes out as the original sounded on the MT-32 (the best-known setup for it), so no
  sound font is needed and decision 2(a) and (b) are not. The effects (VOC) are unaffected.
- **XMI to MIDI:** the `cnc-formats` crate (`xmi` feature, MIT or Apache-2.0, 0.1.0-alpha.5, March 2026) parses the
  Miles XMIDI container (`XmiFile`, `XmiSequence`, `XmiTimbre`) and `to_mid()` writes a Type 0 Standard MIDI File:
  delays become delta times and the implicit note durations become note-offs. Its docs are silent on `FOR`/`NEXT`
  loops, branches and several sequences per file, and it is an alpha, so it is checked against all 118 songs in task 363.
  Its `XmiTimbre` list shows which timbres of `FAT.MT` a song asks for. It would live in `f4x-tools` only.

