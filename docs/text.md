# Player-facing text

Every message the player reads comes from `content/text/<lang>/*.ron` by key. Code names a key and
gives the values; it holds no sentence. This is what makes a second language possible.

## Files and keys

- `settings.ron` names the language (`language: "en"`); `en` is the fallback for a key another
  language lacks. `content-local/text/<lang>/` layers over `content/text/<lang>/`.
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
  `heroes.ron` (`Game::hero_gender`), a wizard's is `wizards.ron`, `wizard_names.ron` or the choice
  on the creation screen (Tab on the name step).
- `pick` takes one of several `lines` by the number named in `pick` (a wizard's own way of saying
  the same thing): `(pick: "salt", lines: ["Hello.", "Greetings."])`.
- A form can be another choice, so a gendered form may hold a count.
- A list argument (`Args::list`) is joined by the patterns of `list.ron` ("a, b and c").

## In code

`App::text(key)` and `App::text_with(key, &Args::new().text("spell", name).count("n", 3))` in the
client; `Content::texts.get(key, &args)` anywhere else. A key nobody wrote prints as the key.

`crates/f4x-data/tests/text.rs` checks that every key named in code exists, every key is used, a
file holds only the keys of its area, and that the client's code holds no message of its own (the
test lists what it finds; numbers, the space between wrapped words and screenshot fixtures are
exempt there). Write a key as one string literal (`"spell.dispelled"`), also
where the choice is made by an `if`, so the test sees it.

Debug output of the console and test messages are not player-facing and stay in code.

## What stays outside

- Names of the numbered rules tables are one file each in `content/text/<lang>/name/`: `unit.ron`,
  `spell.ron`, `building.ron`, `race.ron` (and `race_plural.ron`), `realm.ron`, `ability.ron`,
  `item.ron`, `item_power.ron`. A file is a RON map from the definition's `num` to its name
  (`{ 95: "Spearmen" }`). The loader puts the name of the language in `settings.ron` on each
  definition (`def.name`, so the screens need no change), a number the language leaves out is
  English, and a definition with no English name or a name for a number nobody has is a load error.
  A language is the same files in its own folder. Not yet in tables: ability descriptions, the
  names of the wizards (`wizard_texts.ron`), of the experience levels and of the heroes.
- Text the core builds as game data is on keys when a language must order or inflect it: the
  default name of a made item is kept as parts (`Item.made`: kind, bonus, suffix) and put together
  by `Item::display_name` from `item.name_*`, `item.type_*` and `item.suffix_*`; the words that fill
  the random events (`event.none`, `event.thousand`, `event.plural_ending`, `event.ore_*`, the city
  size names `city.size_*`); the default names of wizards and cities (`setup.*`); the lines of the
  battle Info window (`battle_info.*`, `InfoItem.key`). The sentences of the random events
  (`EVENTMSG.LBX`, `event_texts.ron`) are the original's text, local data like the help: a language
  brings its own file with the same placeholders, they are not keys. A name a player typed (a hero,
  a made item) is kept as typed.
- The console prints English from the core and from its own words; it is a developer tool, and
  so are the validation errors of `settings.ron` and `setup.rs` (they are written for the content
  author).
