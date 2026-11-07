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
- [x] Battle screen: Spell, Wait, Info, Auto, Flee, Done are one text-free 26x10 picture (`battle.png`, `battle_locked.png`, from the Info) with labels from the `battle.button_*` keys drawn by `action_button_art.rs` (`battle_label.*`: style 0 with the original's I, W, N, S, F, O; brown ink); they match the original's lettering
- [x] Main screen action row: Done, Patrol, Wait, Build, Meld, Purify, and the locked ones

## Pictures with baked text, confirmed (extractor `button_*` pictures)

1. [x] Main screen: Next Turn (one 67x16 picture, `main` 35 and 58)
2. [x] City screen: Buy, Change, OK (the scroll arrows have no text; a disabled button fades as the original's does, so the extractor's Buy (locked) picture is not needed)
3. [x] Army list: Items, Ok are text-free (`panel_41.png`, made from the Ok, shared by both) with the labels drawn by `panel_button_art.rs` (style 4 in the gold set `PANEL_WORD`, black shadow, kerned); both match the original pixel for pixel
4. [x] City list: OK is the army list's text-free Ok (`reload` 22 is the same picture; the arrows have no text)
5. [x] Item screen: Alchemy, Ok are text-free (`stone_59.png`, made from the Ok, shared by both) with lavender labels from `panel_button_art.rs` (composed from the text's shape, a diagonal sheen picture and a light and a dark emboss edge)
6. [x] Magic screen: Alchemy, Ok are text-free (`blue_58.png` from the Alchemy, `blue_26.png` from the Ok), labels from `panel_button_art.rs`; they now move a pixel when pressed, as the others do
7. [x] Alchemy popup: the title, Cancel and OK are text-free (`alchemy_frame.png`, `alchemy_stone.png`, `alchemy_art.rs`; titles and labels from `magic.alchemy_*` keys); the arrows have no text
8. [x] Road build OK and CANCEL, and the surveyor's Cancel: the gold blank (`Action::Ok`, `Action::Cancel` in `action_button_art.rs`) with labels from `ui.button_ok` and `ui.button_cancel`, drawn over the border's baked buttons
9. [x] Spell cast Cancel: it is the surveyor's panel (`main` 41, 47), done with item 8
10. [x] Load/Save: Load, Save, Quit, Settings, Ok are text-free planks (`plank_38/39/40/58.png`, widened from the original's blank load button), labels from `menu.*` keys drawn kerned by `plank_button_art.rs`
11. [x] Start menu: Continue, Hall Of Fame, Load Game, New Game, Quit are drawn by `start_menu_art.rs` (style 4 in the new gold gradient sets `MENU_WORD` and `MENU_WORD_HOVER`, kerned without the open "am" pair, each word set on its own centre, labels from the `menu.*` keys; New Game and Hall Of Fame match the original pixel for pixel); the `vortex` word pictures are no longer used. The original's "Quit To DOS" is "Quit"

## Probably text, to verify first

12. [x] Item crafting: the ten tabs share one blank (`tab.png`, up and chosen from the same picture, gold when chosen), OK is `craft_ok.png`, labels from `craft.tab_*` and `craft.ok` (`craft_art.rs`); the arrows and the x1 to x4 charges have no words and stay
13. [ ] New game screens: OK and Cancel are done (`wood_64.png`, `wood_30.png`, from the screen's blank plate `newgame` 4; labels from `newgame_button_art.rs`; the small OK fades while disabled and presses like the others). The options picture's title and the four labels are done too (`options_frame.png`, the blackletter of style 5 in the start menu's gold, from `newgame_button_art.rs`). The creation steps' titles, names and lists were already drawn by the font engine, and the Back button is now the small wooden plate (`Board::Small`) instead of a drawn box.
14. [x] Yes/No question boxes (`resource` 3 and 4, also the combat ones): `yes.png` and `no.png` with labels from `ui.button_yes` and `ui.button_no` (`Slab` in `action_button_art.rs`); `bottom_with_button` has no text. The slabs' hit area is now their whole 66 px (it was the left half)
15. [x] Offers: the pictures have no text (the buttons are the client's `red_button`; `merchant_buttons` is unused)
16. [x] Settings screen: Ok is a text-free plank (`plank_40.png`, from the original's blank load/save button) with the label drawn by `plank_button_art.rs`; the background (`load` 11) has no baked button text
17. [x] Unit screen, diplomacy, spell book, upkeep and sell prompts: the buttons come from the client's `red_button` (`backgrnd` 24, blank), the text is drawn by the client
