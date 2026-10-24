# Hotkeys

The keys of the overland map are not in the code: `content/hotkeys.ron` lists, for each action
(`end_turn`, `move_left`, `quick_save`, ...), the keys that do it. `crates/f4x-client/src/hotkeys.rs`
reads it.

- **Format:** a map from the action's name to a list of keys, each `"Key"`, `"Ctrl+Key"`,
  `"Shift+Key"` or `"Alt+Key"`. Ctrl is Control, or Cmd on a Mac, and must be held exactly when the
  key names it (so `M` and `Ctrl+M` are different keys). The file's header lists the key names.
- **Changing a key:** copy the action's line into `saves/hotkeys.ron` (the player's own) or into the
  `hotkeys.ron` of a content folder (a mod). The base is the shipped file; each later file replaces
  the keys of the actions it names, the player's last. An empty list turns an action's key off.
  An unknown action or key name is reported on start-up and the shipped keys are used.
- **Shared keys:** the arrows are in `move_*` and `scroll_*`: with a stack selected they move it, with
  nothing selected (or Shift held) they scroll the map. Two other actions on one key both fire.
- **What is not in the file:** the keys of a screen's own boxes (Enter and Escape in a question, the
  arrows in a list, the letters of a screen's buttons) and the battle's keys stay where they are.
- **Help:** the Keys topic (`content/help.ron`, topic 900, shown with the Game button's help) writes
  `{key:action}` where a key goes, and the pop-up fills in the keys in force, so it follows a change.
  Other help topics can do the same.
- F1 is the Surveyor like F2 to F9 are the other advisors; there is no key list on the map any more.
