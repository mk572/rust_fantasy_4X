# Buttons with hardcoded text (localization)

The original's buttons are pictures with their lettering baked in. For localization each one gets a
text-free picture in `crates/f4x-client/assets/button/` and a text key; the font engine draws the
label (`menu_bar_art.rs`, `action_button_art.rs`). Method per button: screenshot, check the lettering
against the fonts, carve missing glyphs into a small font (`scripts/button_label_font.py`), make the
text-free picture (`scripts/button_text_free.py`), match the old pixels. The pressed look is the up
picture moved a pixel under a dark top row and left column (`menu_bar_art::draw_button`); a
translucent shadow edge (`.edge(Color::from_rgba(..))`) follows the marbling.

## Done

- [x] Main screen menu bar: Game, Spells, Armies, Cities, Magic, Info, Plane
- [x] Main screen action row: Done, Patrol, Wait, Build, Meld, Purify, and the locked ones

## Pictures with baked text, confirmed (extractor `button_*` pictures)

1. [x] Main screen: Next Turn (one 67x16 picture, `main` 35 and 58)
2. [x] City screen: Buy, Change, OK (the scroll arrows have no text; a disabled button fades as the original's does, so the extractor's Buy (locked) picture is not needed)
3. [x] Army list: Items, Ok are text-free (`panel_41.png`, made from the Ok, shared by both) with the labels drawn by `panel_button_art.rs` (style 4 in the gold set `PANEL_WORD`, black shadow, kerned); both match the original pixel for pixel
4. [x] City list: OK is the army list's text-free Ok (`reload` 22 is the same picture; the arrows have no text)
5. [x] Item screen: Alchemy, Ok are text-free (`stone_59.png`, made from the Ok, shared by both) with lavender labels from `panel_button_art.rs` (composed from the text's shape, a diagonal sheen picture and a light and a dark emboss edge)
6. [x] Magic screen: Alchemy, Ok are text-free (`blue_58.png` from the Alchemy, `blue_26.png` from the Ok), labels from `panel_button_art.rs`; they now move a pixel when pressed, as the others do
7. [ ] Alchemy: Cancel, OK (the arrows have no text)
8. [ ] Road build: OK and CANCEL are baked into the border picture; the surveyor (`main` 41) too
9. [ ] Spell cast: Cancel, baked into the border and also a picture of its own
10. [x] Load/Save: Load, Save, Quit, Settings, Ok are text-free planks (`plank_38/39/40/58.png`, widened from the original's blank load button), labels from `menu.*` keys drawn kerned by `plank_button_art.rs`
11. [x] Start menu: Continue, Hall Of Fame, Load Game, New Game, Quit are drawn by `start_menu_art.rs` (style 4 in the new gold gradient sets `MENU_WORD` and `MENU_WORD_HOVER`, kerned without the open "am" pair, each word set on its own centre, labels from the `menu.*` keys; New Game and Hall Of Fame match the original pixel for pixel); the `vortex` word pictures are no longer used. The original's "Quit To DOS" is "Quit"

## Probably text, to verify first

12. [ ] Item crafting: 54 pictures (weapon types and their "chosen" variants, charges 1 to 4, OK, arrows)
13. [ ] New game screens: OK and Cancel are done (`wood_64.png`, `wood_30.png`, from the screen's blank plate `newgame` 4; labels from `newgame_button_art.rs`; the small OK fades while disabled and presses like the others). The options picture's title and the four labels are done too (`options_frame.png`, the blackletter of style 5 in the start menu's gold, from `newgame_button_art.rs`). To check: the other entries of `newgame_flow.rs` and `newgame_picks.rs`
14. [ ] Site encounter and prompts: Yes/No (`resource` 3 and 4), `bottom_with_button`
15. [ ] Offers: `hire`, `merchant_buttons`
16. [x] Settings screen: Ok is a text-free plank (`plank_40.png`, from the original's blank load/save button) with the label drawn by `plank_button_art.rs`; the background (`load` 11) has no baked button text
17. [ ] Unit screen, diplomacy, spell book, upkeep and sell prompts: check whether the buttons already come from the client's `red_button` (text drawn by the client); if so they are fine
