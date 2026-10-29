# Open Tasks

Everything not yet implemented.

Keep the table in ascending order by number.
Add a summary to the table, then details under Descriptions.

Bracketed numbers like [430] point at topics in the original help
(`content-local/help.ron`)
B/C numbers point at `help-vs-implementation-conflicts.md`.

Gaps found while working on the base game are added here as new tasks, not left in notes.

Finished tasks are moved to [`TASKS-DONE.md`](TASKS-DONE.md) with
`python3 scripts/archive_done_tasks.py` (a task is done when its row or title says "(done)").

The older open tasks are parked in [`TASKS-ONHOLD.md`](TASKS-ONHOLD.md).

## Table

| # | Area | Summary |
|---|---|---|
| 390 | Client | Standard buttons and hit-testing |
| 392 | Client | One text drawing function |
| 393 | Client | Textures and scaling |
| 394 | Client | Screen trait |
| 395 | Client | Client test seams |
| 396 | Client | Main menu credits roll |
| 397 | Client | Quit confirmation and auto-save on Quit |
| 399 | Client | Food warning on Next Turn |
| 400 | Client | Extra-mana screen for an overland spell |
| 401 | Client | Portrait picker for a spell that targets a wizard |
| 402 | Client | Banner with the spell's name in combat |
| 403 | Art | Identify the unused spell effect pictures |

## Descriptions

**390. Standard buttons and hit-testing.** `ui::button` is called about 83 times, and about 105 places test `mouse_position()` against a rectangle by hand (picture buttons and screens in the original's coordinates each repeat hover, press and disabled states). Add one helper in `ui.rs` that takes a rect in layout space and returns `None`, `Hover`, `Pressed` or `Clicked`, maps the mouse through `Layout` in one place, and covers text buttons, picture buttons and art-less hot-zones. Give it right-click help, the hotkey label and the disabled-reason tooltip (as the Buy button's refusal text) so screens stop doing these by hand.

**392. One text drawing function.** Text is drawn through `draw_text`, `bm_text`, `bm_text_shadowed`, `bm_text_outlined` and `draw_flat_text`. Put all text through one function with a style (font, size, colour, outline, shadow) and one place for measuring and wrapping. This is the base for the bitmap fonts of task 300 and for localization.

**393. Textures and scaling.** Find out with a profile whether any pictures are rebuilt from `Image` each frame instead of kept as `Texture2D`. Consider drawing the whole frame into one `RenderTarget` at the original's 320x200 and scaling that up, which would remove the `screen_width()` and `screen_height()` scaling from about 20 call sites.

**394. Screen trait.** `Screen` is an enum and `view.rs` is large. Give each screen one implementation of a `Screen` trait (`update`, `draw`) so `view.rs` becomes the dispatcher and fewer files know about each other. Do this after 389 and 391. **Started:** the dispatch is now one function, `App::screen_frame` in `view.rs` (the `match` on `Screen`), which `frame()` and the modal base both call. The screens are still `App` methods with their state in `App` fields (`big_ui`, `diplo_ui`, `city_list_ui` and so on), so the trait needs each screen's state moved into its own struct first.

**395. Client test seams.** With the input snapshot (391) and the modal stack (389), test click flows without a window (a prompt opens, Esc closes it, a click on a disabled button does nothing). Supports the UI test layer in `docs/rust_coding_rules.md`.

**396. Main menu credits roll.** The main menu has no Credits entry. Add the credits roll as a screen of its own (task 394 style: its own state struct), with its text in `content/text/en/*.ron` and the sources and licences that `PLANNED.md` asks to keep in a credits file. Listed in `screen_list.md` section 8.

**397. Quit confirmation and auto-save on Quit.** Quit leaves at once. Ask first (a modal question, as the resign question) and write the auto-save before the program ends, so a quit never loses the game. Listed in `screen_list.md` section 8.

**399. Food warning on Next Turn.** When Next Turn is pressed and a city would starve, the original warns first, in its own words. Add the question as a modal that lets the player go back, with the original's wording from `content-local` as the text key. Listed in `screen_list.md` section 8.

**400. Extra-mana screen for an overland spell.** An overland spell cannot be given extra mana; combat has the Up and Down keys for it. Add the screen the original shows for the spells that can take more mana, and the cast command that carries the amount. Check `design.md` for which spells allow it. Listed in `screen_list.md` section 8.

**401. Portrait picker for a spell that targets a wizard.** A spell aimed at a wizard (Spell of Return, Disjunction and the like where the original asks for a portrait) is aimed by clicking a city or unit. Add the picker with the wizards' portraits, as a modal question. Listed in `screen_list.md` section 8.

**402. Banner with the spell's name in combat.** The original shows the name of the spell being cast over the battle. Add it as a short banner (a modal or a battle overlay that does not stop the animation clock, to be decided) with the spell's name from the rules. Listed in `screen_list.md` section 8.

**403. Identify the unused spell effect pictures.** After Magic Vortex took `cmbtfx` 18 (role `Vortex`), these entries of the original's effect files are extracted but named by no row of `content/spell_art.ron`: `cmbtfx` 7 (14 frames, a purple crystal), 12 (8 frames, a green heart), 26 (8 frames, a small blue flame), 27 to 32 (single frames of 179x146, 82x36 and four of 179x136) and `specfx` 43. They sit loose in `assets-local/art/cmbtfx/` and `art/specfx/`. Find which spell or screen each belongs to (the order of the spell list in the original's effect table, the combat spells that still show a tint or a realm picture, `docs/overland_spell_animations.md`), add the rows with a role (`Role` in `f4x-data`'s `spell_art.rs`) and wire them where the picture is meant to play, or record in `docs/extract-art.md` that nothing in the game loads one. Check each with a screenshot.
