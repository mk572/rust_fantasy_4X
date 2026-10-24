# Help topics that conflict with the game: checklist

The original's help text (topic numbers are those of `HELP.LBX`; the text is local only) says things the
game does not do. The game's own help (`content/help.ron`, written by us) must say what the game does,
so each topic below is resolved when its own text is written and matches the code. Tick the box then,
and note the decision in `docs/help-vs-implementation-conflicts.md` if it was not recorded there.

Started 24 October 2026 (task 289). The topics are in two lists: the ones where the importer already
corrects the local copy of the original (`HELP_CORRECTIONS` in `scripts/import_mom.py`; the reason is
the game's rule), and the ones that the conflicts document marks as open or decided but not yet
reflected in any help text. The comparison covered costs, upkeeps, building and race tables and the
spells' damage numbers (all agree, see the conflicts document, section A); prose that describes
behaviour beyond those numbers has not been compared topic by topic, so this list is a start, not
the end. When a topic is found to disagree, add it here.

## 1. The importer already corrects these (the game's rule is the reason)

- [ ] **55 ENCHANT ROAD**: our own rule: the original's enchanted roads cannot be dispelled; ours can, square by square (divergence 27).
- [ ] **198 EVIL OMENS**: the casting-cost code applies it to any wizard's cost whoever holds the spell; item and unit casts pay the spell's plain cost (decision for now, B11).
- [ ] **657 SPELL OF RETURN**: the conquest code defeats a banished wizard that cannot pay for the spell, and asks the human whether to resign.
- [ ] **765 FAMOUS**: the original's code gives no fame for founding a city, gives the winner a point for rare foes, and takes the fame of lost heroes and cities; only a human wizard with more than 20 fame loses a point for a major defeat.
- [ ] **593 FLEE BUTTON**: the combat code skips the roll on Intro and Easy for the player on turn, and always destroys sleeping, confused and webbed fleeing units.
- [ ] **607 HIGH PRAYER**: the code gives +2 to melee and defence, +3 to resistance and +1 (ten percent) to hit and to block.
- [ ] **196 PESTILENCE**: the city code takes 2 from the city's pacification while Pestilence is on it (the help does not say so).
- [ ] **773 BAD MOON**: the city magic code adds the full power of the religious buildings for the matching realm (a doubling), where the help says 50% (B50).
- [ ] **774 GOOD MOON**: the city magic code adds the full power of the religious buildings for the matching realm (a doubling), where the help says 50% (B50).
- [ ] **119 CALL THE VOID**: the spell's code kills each citizen after the first with an even chance (`Apply_Call_The_Void`); the help does not say so (B51).
- [ ] **486 WEAKNESS**: the code gives Weakness a resistance check at -2 (the spell's own help text says so).
- [ ] **733 UNIVERSITY**: the building data file charges 3 gold.
- [ ] **44 GUARDIAN WIND**: the unit-enchantment upkeep table charges 2 mana; the code grants full missile immunity.
- [ ] **574 GUARDIAN WIND**: the code grants full missile immunity.
- [ ] **732 ALCHEMISTS GUILD**: the code gives the +3 mana to the Animist's Guild, not the Alchemists Guild; the code lets an Alchemists Guild use nightshade.
- [ ] **723 ANIMIST'S GUILD**: the code gives the Animist's Guild +3 mana.
- [ ] **734 WIZARD'S GUILD**: the code adds 3 mana for a Wizard's Guild instead of charging it; the code lets a Wizard's Guild use nightshade.
- [ ] **26 EARTHQUAKE**: the code rolls 15 for each building and one in four for each unit.
- [ ] **57 WIND MASTERY**: our rule: both bonus and penalty apply to ships and fliers, overland and in battle; nothing stacks.
- [ ] **93 CHAOS CHANNELS**: the code adds 3 to the defence (CC_ARMOR).
- [ ] **440 CHAOS CHANNEL**: the code adds 3 to the defence (CC_ARMOR).
- [ ] **554 STASIS**: the code rolls the freeing check at -5, as help topic 68 says.
- [ ] **171 DRAIN POWER**: the code adds ten rolls of a twenty-sided die.
- [ ] **667 RJAK**: the code applies 150% for Infernal and Divine Power alike.
- [ ] **399 HEALER**: the code heals 20% of total hit points per turn, not 25% after combat.
- [ ] **728 SAWMILL**: the code adds 25% to the city's total production.
- [ ] **398 WALL CRUSHER**: the code uses 25% for ranged attacks.
- [ ] **281 DARK ELF TOWNSFOLK**: the code adds one mana per citizen for dark elf cities.
- [ ] **29 GAIA'S BLESSING**: the unrest code pacifies two for Gaia's Blessing (the wiki lists it too); the production code doubles the bonus of forests and nature nodes under Gaia's Blessing.
- [ ] **415 WEAPON IMMUNITY**: the code gives defence 10 against normal weapons, not against magical attacks.
- [ ] **433 CHAOS**: the code halves doom damage only for a non-innate source; a unit's own is the full strength.
- [ ] **478 HOLY ARMOR**: the code adds 2 defence (the spell text already says 2).
- [ ] **453 BLACK CHANNELS**: the code adds 2 melee (the spell text already says 2).
- [ ] **464 INVISIBILITY**: the code applies -1 to hit against invisible units.
- [ ] **564 INVISIBILITY**: the code applies -1 to hit against invisible units.
- [ ] **51 BLUR**: this game takes a flat 1 off the chance to hit; the original rolls a 10% miss for each point of damage.
- [ ] **797 BLUR**: this game takes a flat 1 off the chance to hit; the original rolls a 10% miss for each point of damage.
- [ ] **401 INVISIBILITY**: the original takes 1 off the chance to hit whatever the distance; here the penalty grows with distance, and the unit also shows itself by attacking or casting.
- [ ] **37 EARTH GATE**: the original's code and this game only link cities on the same plane.
- [ ] **750 EARTH GATE**: the original's code and this game only link cities on the same plane.
- [ ] **731 ORACLE**: the code lets an Oracle use nightshade.
- [ ] **737 PARTHENON**: the code lets a Parthenon use nightshade.
- [ ] **738 CATHEDRAL**: the code lets a Cathedral use nightshade.
- [ ] **766 NIGHTSHADE**: the code checks each spell cast at the city, and only for cities with such a building.
- [ ] **234 FOOD RESERVE**: task 292: the player (and the AI) chooses who goes when the food or gold runs out, instead of the newest units being dismissed unasked; a ship let go with passengers aboard sails for the nearest shore.
- [ ] **231 TOTAL GOLD**: task 292: the player (and the AI) chooses who is let go when the gold runs out.
- [ ] **266 MANA POINTS RATIO**: task 62: the player (and the AI) chooses what is given up when the mana runs out.

## 2. Marked in the conflicts document, no corrected text yet
Each row is one topic (or group). `Bn` and `Cn` are the rows of `docs/help-vs-implementation-conflicts.md`.

- [ ] **473 HEROISM**: B9: the original's combat level ignores Crusade, Warlord and experience; ours uses one level calculation overland and in battle (Elite floor).
- [ ] **553 UNDEAD**: B10: cold, poison and illusion immunity are given to the undead here, as the help says.
- [ ] **516 LEGENDARY**: B12: +3 fame per hero level, +4.5 for super legendary (the original has it disabled).
- [ ] **744 FORESTER'S GUILD**: B14: the +2 food also raises the city's maximum size.
- [ ] **Swamp**: B15: half a food; no topic names it, check the terrain help.
- [ ] **400 CITY SPACING**: B17: at least four squares between cities; only intact towers, nodes and encounter sites block.
- [ ] **768 RANDOM EVENTS 768 to 775**: B18: events run from turn one; each ends with a 5% chance a turn.
- [ ] **183 EVIL PRESENCE**: B22: destroys the mana of religious buildings; no effect if the owner has death books.
- [ ] **178 WALL OF DARKNESS**: B23: ranged attacks across it need illusions immunity.
- [ ] **72 SPELL WARD**: B24: total immunity to the realm's spells including combat; bars its fantastic creatures.
- [ ] **77 FLYING FORTRESS**: B25: only fliers enter, attack or leave; walkers that leave cannot re-enter.
- [ ] **110 CHAOS RIFT**: B26: five strength 8 bolts a turn, 5% for a building.
- [ ] **150 HOLY WORD**: B27: the undead resist at an extra -5.
- [ ] **622 CRUSADE**: B28: raises heroes as well; not the undead or creatures.
- [ ] **75 GREAT UNSUMMONING**: B32: a resistance roll, the caster's own fantastic creatures too.
- [ ] **199 DEATH WISH**: B32: every enemy normal unit that fails a resistance roll.
- [ ] **177 SUBVERSION**: B33: -25 relations with the target for every other wizard that has met it; peace ends.
- [ ] **164 CLOAK OF FEAR**: B34: fear is rolled figure by figure by the opponent; Death immunity negates it.
- [ ] **63 WIND WALKING**: B36: the spell gives its unit at least 3 moves; a stack uses the Wind Walker's moves.
- [ ] **113 METEOR STORM**: B38: all creatures outside cities, the caster's own too; 1% a building.
- [ ] **114 GREAT WASTING**: B39: three to six squares a turn, any land square, unrest and mana from corrupted squares.
- [ ] **40 NATURE'S WRATH**: B41: strikes the wizard who casts a death or chaos spell while another holds it.
- [ ] **443 THROWN / breath / Mithril / Adamantium**: B45: weapon quality counts for thrown and breath attacks.
- [ ] **Flying (ability entries)**: B46: a non-flier's melee strength is halved, rounded up, against a flier.
- [ ] **Engineers, roads**: B48: turns to build a road by terrain (grassland 3 ... mountains 8).
- [ ] **Meld**: B49: a melding spirit is used up whether or not the meld succeeds.
- [ ] **Spell books and Evil Omens**: B11: where Evil Omens applies (item and unit casts are exempt).
- [ ] **Naming**: C5: Cloud of Shadow or Darkness, Holy Light or Heavenly Light, Prime or Arcanus: pick one name.

## 3. Where our own rules replace the original's
Every section of `docs/divergences.md` is a place where the game departs from the original on purpose (ship
battles, lakes, selling items, map sizes, the outpost rate, and so on). Any help topic that describes one
of those things needs our own text. Add the topic here when the text is written.

## 4. Decided on 24 October 2026 while writing the screens' text
- Patrol (topic 224): ours lasts until the unit moves again; the original's lasts until you click the unit. Decided: keep ours; the help says so.
- Casting skill (topic 273): the magic screen now shows skill with hero help, and the base skill in brackets, as the original does.
- Seven Settings switches do nothing yet (task 375); their help describes what they should do.
- Strategic Combat Only (685): kept, with the new meaning (battle AI without the battle screen, after the mana prompt).
