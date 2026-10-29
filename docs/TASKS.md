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

## Descriptions

**390. Standard buttons and hit-testing.** `ui::button` is called about 83 times, and about 105 places test `mouse_position()` against a rectangle by hand (picture buttons and screens in the original's coordinates each repeat hover, press and disabled states). Add one helper in `ui.rs` that takes a rect in layout space and returns `None`, `Hover`, `Pressed` or `Clicked`, maps the mouse through `Layout` in one place, and covers text buttons, picture buttons and art-less hot-zones. Give it right-click help, the hotkey label and the disabled-reason tooltip (as the Buy button's refusal text) so screens stop doing these by hand.

**392. One text drawing function.** Text is drawn through `draw_text`, `bm_text`, `bm_text_shadowed`, `bm_text_outlined` and `draw_flat_text`. Put all text through one function with a style (font, size, colour, outline, shadow) and one place for measuring and wrapping. This is the base for the bitmap fonts of task 300 and for localization.

**393. Textures and scaling.** Find out with a profile whether any pictures are rebuilt from `Image` each frame instead of kept as `Texture2D`. Consider drawing the whole frame into one `RenderTarget` at the original's 320x200 and scaling that up, which would remove the `screen_width()` and `screen_height()` scaling from about 20 call sites.

**394. Screen trait.** `Screen` is an enum and `view.rs` is large. Give each screen one implementation of a `Screen` trait (`update`, `draw`) so `view.rs` becomes the dispatcher and fewer files know about each other. Do this after 389 and 391. **Started:** the dispatch is now one function, `App::screen_frame` in `view.rs` (the `match` on `Screen`), which `frame()` and the modal base both call. The screens are still `App` methods with their state in `App` fields (`big_ui`, `diplo_ui`, `city_list_ui` and so on), so the trait needs each screen's state moved into its own struct first.

**395. Client test seams.** With the input snapshot (391) and the modal stack (389), test click flows without a window (a prompt opens, Esc closes it, a click on a disabled button does nothing). Supports the UI test layer in `docs/rust_coding_rules.md`.
