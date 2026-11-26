# Player-facing text

The words live in their own crate, `f4x-lang` (`crates/f4x-lang`): the loader and `Texts` (`text`), the name tables
and name lists (`names`, `Names`) and the files in `content/text/`. It depends on `f4x-core` and `f4x-def`, not on
a client, so another client reads the same files; the client keeps the fonts and glyphs (`font*.rs`) and the keys
of its screens (`help_screens.ron`, which names the help topic of a screen area, stays in `crates/f4x-client/content`).
Tests: the loader's and the names' with the crate; the check that the client's code and the keys agree stays in
`crates/f4x-client/tests/it/text.rs`.

Every message the player reads comes from `crates/f4x-lang/content/text/<lang>/*.ron` by key. Code names a key and
gives the values; it holds no sentence. This is what makes a second language possible.

## Files and keys

- `crates/f4x-client/content/client_settings.ron` names the language (`language: "en"`); `en` is the fallback for a key another
  language lacks.
- The start menu's **Language** entry (below Settings) lists the folders of `text/` by the name each gives itself
  (`language.name` in its `language.ron`, so a language names itself in its own words; the code when it
  has none). A choice takes effect at once (messages, names, help and event sentences are loaded again)
  and is kept in `settings/language.txt` (one line, the code; not in `saves/`, which holds the game
  saves, and not in the shipped `client_settings.ron`), which wins over `client_settings.ron` at the next start
  when that language is still installed (`language.rs`).
- One file per screen or domain (`battle`, `spell`, `hero`, `create`, ...), at most about 100
  entries each. A key is `area.name` and lives in the file `area.ron`, once over all files.
- A file is a RON map from key to entry.

## Entries

```ron
{
    "spell.dispelled": "{spell} has been dispelled.",             // plain, with a value
    "city.units": (count: "n", zero: "no units", one: "{n} unit", other: "{n} units"),
    "create.gender_hint": (gender: "wizard", masculine: "Tab: he", feminine: "Tab: she", neuter: "Tab: they"),
}
```

- `{name}` puts in a value; `{{` and `}}` print a brace.
- `count` picks a form by the plural rule of the language (`text::plural`: zero, one, few, many,
  other; `other` is required, `zero` wins for 0 when given).
- `gender` picks a form by the gender of the named argument (`Args::named`). A hero's gender is
  `heroes.ron` (`World::hero_gender`), a wizard's is `wizards.ron`, `text/en/name/rival.ron` or the choice
  on the creation screen (Tab on the name step).
- `pick` takes one of several `lines` by the number named in `pick` (a wizard's own way of saying
  the same thing): `(pick: "salt", lines: ["Hello.", "Greetings."])`.
- A form can be another choice, so a gendered form may hold a count.
- A list argument (`Args::list`) is joined by the patterns of `list.ron` ("a, b and c").
- A name of the numbered tables (unit, building, race) is a **term** (`text::Term`): a string, or a
  record `(text: "...", gender: Masculine, dat_pl: "...")`. Code passes it with
  `Args::term("unit", names.unit_term(num))`, never as a finished string; the message picks the form:
  `{unit}` is the text, `{unit:dat_pl}` the form of that name (the form names are the language's own:
  `dat`, `gen`, `dat_pl`, ...). A form the term lacks prints as the text, a unit the language lacks is
  the English one. A `gender: "unit"` choice reads the term's gender. A unit's name is always the
  whole name, race included ("Dwarven Swordsmen"); nothing glues a race to a unit in code.
  A term may be tagged `plural: true` (a noun plural in form: "City Walls", "Spearmen", "Trade Goods");
  a `number` choice picks the wording by it, so one key serves "the Library falls" and "the City Walls
  fall": `(number: "building", singular: "the {building} falls", plural: "the {building} fall")`.
  `{name:a}` prints the name with its indefinite article (`{name:A}` capitalised, for a sentence's
  start): English "a Library", "an Armory" (the tag `an: true` on the term, `(text: "Armory", an: true)`),
  nothing before a plural noun ("City Walls"). A language writes its own phrase as a form named `a`
  (German `(text: "Zwerg", a: "ein Zwerg")`); with no rule and no form it prints the plain text. A
  message that says "a {x}" by hand is wrong for every name that takes "an": write `{x:a}`.
  A form name with a capital (`{x:Def}`) is the form `def` with its first letter capitalised, for a
  sentence that starts with the name. German names carry the forms `def`, `def_acc`, `def_dat`,
  `def_gen` (with the definite article: "die Bibliothek", "der Bibliothek") and `a`, `a_acc`, `a_dat`,
  `a_gen` (indefinite: "eine Bibliothek"; a plural has none): units, buildings, retorts, terrain, map
  features and the city sizes (`name/word.ron`). A sentence whose verb must agree with the name uses a
  `number` choice (`Die Zwergischen Schwertkämpfer treffen ...` / `Der Weise trifft ...`). Spells and
  fixed items are named after "der Zauber" / "der Gegenstand" in German, so they need no forms.
  German, for "den elfischen Speerkämpfern wurden 3 Schadenspunkte zugefügt": the unit's entry in
  `text/de/name/unit.ron` has `dat_pl`, the message says `Den {unit:dat_pl} wurden {n} ...`.

## In code

`App::text(key)` and `App::text_with(key, &Args::new().text("spell", name).count("n", 3))` in the
client; `Content::texts.get(key, &args)` anywhere else. A key nobody wrote prints as the key.

`crates/f4x-client/tests/it/text.rs` checks that every key named in code exists, every key is used, a
file holds only the keys of its area, and that the client's code holds no message of its own (the
test lists what it finds; numbers, the space between wrapped words and screenshot fixtures are
exempt there). Write a key as one string literal (`"spell.dispelled"`), also
where the choice is made by an `if`, so the test sees it.

The same test file fails when a name of the game (a unit, spell, item, building, race, realm, ...)
goes into a message as a string (`.text("spell", names.spell(n))`, or a variable that holds
that string): pass the term (`.term("spell", names.spell_term(n))`), because German needs the
name's gender, cases and number, and a string has lost them.

Debug output of the console and test messages are not player-facing and stay in code.

## What stays outside

- **Common words** the sentences put in (a city's size, a resource, an ore) are in `name/word.ron`, a map
  from a key (`city_size.town`, `resource.gold`, `ore.iron_ore`) to a term; code asks
  `Names::word(key)` and passes it with `Args::term`. A word is not a message, so it is not a key of
  `text/<lang>/*.ron`; put a word there only when a sentence needs its forms or tags (`{ore:a}`).
- Names of the numbered rules tables are one file each in `crates/f4x-lang/content/text/<lang>/name/`: `unit.ron`,
  `spell.ron`, `building.ron`, `race.ron` (name, plural and adjective: `4: ("Dwarf", "Dwarves", "Dwarven")`), `realm.ron`, `ability.ron`,
  `item.ron`, `item_power.ron`, `terrain.ron`, `feature.ron` (the map features). A file is a RON map from the definition's `num` to its name
  (`{ 95: "Spearmen" }`). The loader puts the name of the language in `crates/f4x-client/content/client_settings.ron` on each
  definition (`def.name`, so the screens need no change), a number the language leaves out is
  English, and a definition with no English name or a name for a number nobody has is a load error.
  A language is the same files in its own folder. Not yet in tables: ability descriptions, the
  descriptions of the ready-made wizards (our own help topics in `help.ron`, named by `help` in `wizards.ron`). `wizard.ron` holds the ready-made
  wizards' names by `num`, `level_path.ron` the names of the experience levels by the path's `num` (levels
  in order, then the bonus levels). `hero.ron` holds
  the five names of each hero by the hero unit's `num` (the first is the hero's name in the first
  wizard's game) and `rival.ron` the lists of names for the rival wizards (a name and its gender, one
  list for each rival slot); `city.ron` holds a race's city names by the race's `num`, in the
  order they are handed out. The client reads these three (`Names::bundle`) and hands them to
  `new_game` as a `NameBundle` (`f4x_def::names`); the game draws the names with its seed and
  stores plain strings. With no list the game uses ids: `<race id>_city_00`, the hero unit's `id`,
  `rival_1`.
- Text the core builds as game data is on keys when a language must order or inflect it: the
  default name of a made item is kept as parts (`Item.made`: kind, bonus, suffix) and put together
  by `Item::display_name` from `item.name_*`, `item.type_*` and `item.suffix_*`; the words that fill
  the random events (`event.none`, `event.thousand`, `event.plural_ending`, `event.ore_*`, the city
  size names `city.size_*`); the default names of wizards and cities (`setup.*`); the lines of the
  battle Info window (`battle_info.*`, `InfoItem.key`). The sentences of the random events
  are the game's own, in `text/<lang>/message/event_texts.ron` (a list by event number and voice,
  not a map of keys, so it sits in the `message/` folder): a language brings its own file with the
  same placeholders, and falls back to English for a file it lacks. A name a player typed (a hero,
  a made item) is kept as typed.
- The console prints English from the core and from its own words; it is a developer tool, and
  so are the validation errors of `rules_settings.ron` and `setup.rs` (they are written for the content
  author).

## Help topics

The help text is `crates/f4x-lang/content/text/en/help/help.ron`, one topic per index. A screen area names the topic it opens
(`help_screens.ron`). An entity names its own topic with `help: Some(index)` on its row: abilities,
spells, buildings, retorts, races, spell realms and ready-made wizards. The code never looks a topic up
by the entity's name; it follows the entity's `help`. A test (`f4x-core/tests/it`) keeps every such link
pointing at a topic with text, and every topic opened by an area or an entity. Units have no help
topic; the Settings switches are mapped to their topics in `settings_screen.rs`.
