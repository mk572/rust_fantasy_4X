# Lairs (guarded sites)

Source: ReMoM `Create_Lair`, `Generate_Lairs`, `Lair_Make_Guardians`, `Lair.c`. Code: `crates/f4x-core/src/lairs.rs`.

- Every tower, node and encounter site gets a `Lair` (guards + hoard) at map generation; towers share one on both planes.
- Guards are neutral units made on entry (`Command::EnterSite`), fought with the battle model, then removed. Winning takes the hoard and clears the site (encounter sites vanish; towers and nodes stay).
- Moving onto a guarded site gives `Event::SiteEncounter` and nothing moves; an unguarded site pays out on arrival.

Differences from the original (gap tasks in TASKS.md):
- Strong/weak split of encounter sites is random (25:32), not fixed counts; site types follow the data, not a random 7-way roll.
- The lair-race switch fall-through (ReMoM-marked bug) is fixed: death 2, chaos 2, nature 1 in five.
- Spell rewards and book/retort rewards follow the intent, not the original's muddled index arithmetic.
- Items are real: each hoard value is made into an item (`make_item`, `Grade::Any`) and goes to the armory (vault first), and the client opens the item screen after the treasure box. A prisoner is a hero offered for nothing (`Offer::Hero` with price 0, first on the offers; hired at the summoning circle, where the original's bug moves it to the lair). A book or retort hoard that gives nothing pays an item of 1200 or 1000 mana instead.
- The client shows the original's boxes (`site_prompt.rs`): the Yes/No question with the site's picture, and the treasure box (`--site ask|found|treasure` for screenshots). Missing: the discovery music and the colour slide of the node pictures.
