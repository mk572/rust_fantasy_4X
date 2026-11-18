# Deutsch (Entwurf)

Work in progress, **not reviewed**: the first German texts, written to exercise the language
support (`docs/text.md`). A key or a name this folder lacks is shown in English. Done so far: the
start menu, the common words (`name/word.ron`) and the names of races, realms, terrain, map
features, retorts and buildings. Every noun carries its gender (`Masculine`, `Feminine`,
`Neuter`) so a message can choose its article; case forms (`dat`, `gen`, ...) are added to a name
when a German message asks for one. To be read by a native speaker before it counts as finished.

Update: the nouns that sit in sentences (units, buildings, retorts, terrain, features, city sizes) carry the article and case forms (`def`, `def_acc`, `def_dat`, `def_gen`, `a`, ...) and the sentences use them. All message files, the name tables for units, spells, items, item powers, abilities,
races, realms, terrain, features, retorts, buildings and experience levels, the event sentences
and the common words are in German. Still English (falls back by file or number): the help text
(`help/help.ron`, 565 topics) and the proper names (wizards, rivals, heroes, cities).
Progress report: `cargo test -p f4x-lang --lib german_has_every -- --ignored`.
