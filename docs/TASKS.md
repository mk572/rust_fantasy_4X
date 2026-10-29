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
| 389 | Client | Modal stack for prompts, messages and help |
| 390 | Client | Standard buttons and hit-testing |
| 391 | Client | One input snapshot per frame |
| 392 | Client | One text drawing function |
| 393 | Client | Textures and scaling |
| 394 | Client | Screen trait |
| 395 | Client | Client test seams |

## Descriptions

**389. Modal stack for prompts, messages and help.** `View::frame()` in `view.rs` runs about ten `*_prompt_frame()` and `*_popup_frame()` functions (help, upkeep, attack, ward, resurrect, global, resign, event, offer, built, and more) in a hand-ordered `if` chain, and some return early and block the rest of the frame. Replace this with one stack of modals (a trait with `draw` and `handle_input`): the screen below always draws first and is dimmed, only the top modal gets input (no click leaks through), Esc and click-outside behave the same everywhere, and prompts that queue up (the end-of-turn events) no longer depend on the order of the chain. Built on the client's own draw helpers; macroquad supplies only the primitives. Convert one prompt at a time. Builds on 391 and 390.

**390. Standard buttons and hit-testing.** `ui::button` is called about 83 times, and about 105 places test `mouse_position()` against a rectangle by hand (picture buttons and screens in the original's coordinates each repeat hover, press and disabled states). Add one helper in `ui.rs` that takes a rect in layout space and returns `None`, `Hover`, `Pressed` or `Clicked`, maps the mouse through `Layout` in one place, and covers text buttons, picture buttons and art-less hot-zones. Give it right-click help, the hotkey label and the disabled-reason tooltip (as the Buy button's refusal text) so screens stop doing these by hand.

**391. One input snapshot per frame.** There are about 225 `is_key_pressed` and 125 `is_mouse_button_pressed` calls spread over the client files. Read the input once per frame into a small `Input` struct (mouse position, buttons, keys pressed, wheel, typed characters) and pass it down. A modal (389) can then take the input away from the screens below, and tests (395) can feed it fake input.

**392. One text drawing function.** Text is drawn through `draw_text`, `bm_text`, `bm_text_shadowed`, `bm_text_outlined` and `draw_flat_text`. Put all text through one function with a style (font, size, colour, outline, shadow) and one place for measuring and wrapping. This is the base for the bitmap fonts of task 300 and for localization.

**393. Textures and scaling.** Find out with a profile whether any pictures are rebuilt from `Image` each frame instead of kept as `Texture2D`. Consider drawing the whole frame into one `RenderTarget` at the original's 320x200 and scaling that up, which would remove the `screen_width()` and `screen_height()` scaling from about 20 call sites.

**394. Screen trait.** `Screen` is an enum and `view.rs` is large. Give each screen one implementation of a `Screen` trait (`update`, `draw`) so `view.rs` becomes the dispatcher and fewer files know about each other. Do this after 389 and 391.

**395. Client test seams.** With the input snapshot (391) and the modal stack (389), test click flows without a window (a prompt opens, Esc closes it, a click on a disabled button does nothing). Supports the UI test layer in `docs/rust_coding_rules.md`.
