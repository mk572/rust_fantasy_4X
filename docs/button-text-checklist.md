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
2. [ ] City screen: Buy, Buy (locked), Change, OK (the scroll arrows have no text)
3. [ ] Army list: Items, Ok
4. [ ] City list: OK (`reload` 22; the arrows have no text)
5. [ ] Item screen: Alchemy, Ok
6. [ ] Magic screen: Alchemy, Ok
7. [ ] Alchemy: Cancel, OK (the arrows have no text)
8. [ ] Road build: OK and CANCEL are baked into the border picture; the surveyor (`main` 41) too
9. [ ] Spell cast: Cancel, baked into the border and also a picture of its own
10. [ ] Load/Save: Load, Save, Quit, Settings, Ok (the inactive variants are already blank)
11. [ ] Start menu: Continue, Hall Of Fame, Load Game, New Game, Quit To DOS (large blackletter, a bigger style)

## Probably text, to verify first

12. [ ] Item crafting: 54 pictures (weapon types and their "chosen" variants, charges 1 to 4, OK, arrows)
13. [ ] New game screens: OK and Cancel (`newgame` 2 and 3) and the other entries of `newgame_flow.rs` and `newgame_picks.rs`
14. [ ] Site encounter and prompts: Yes/No (`resource` 3 and 4), `bottom_with_button`
15. [ ] Offers: `hire`, `merchant_buttons`
16. [ ] Settings screen (`load` 4 and 11)
17. [ ] Unit screen, diplomacy, spell book, upkeep and sell prompts: check whether the buttons already come from the client's `red_button` (text drawn by the client); if so they are fine
