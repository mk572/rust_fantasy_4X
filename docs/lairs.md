# Lairs (guarded sites)

Source: ReMoM `Create_Lair`, `Generate_Lairs`, `Lair_Make_Guardians`, `Lair.c`. Code: `crates/f4x-core/src/lair.rs`.

- Every tower, node and encounter site gets a `Lair` (guards + hoard) at map generation; towers share one on both planes.
- Guards are neutral units made on entry (`Command::EnterSite`), fought with the battle model, then removed. Winning takes the hoard and clears the site (encounter sites vanish; towers and nodes stay).
- Moving onto a guarded site gives `Event::SiteEncounter` and nothing moves; an unguarded site pays out on arrival.

Differences from the original (gap tasks in TASKS.md):
- Strong/weak split of encounter sites is random (25:32), not fixed counts; site types follow the data, not a random 7-way roll.
- The lair-race switch fall-through (ReMoM-marked bug) is fixed: death 2, chaos 2, nature 1 in five.
- Spell rewards and book/retort rewards follow the intent, not the original's muddled index arithmetic.
- Items are real: each hoard value is made into an item (`make_item`, `Grade::Any`) and goes to the armory (vault first), and the client opens the item screen after the treasure box. A prisoner is a hero offered for nothing (`Offer::Hero` with price 0, first on the offers; hired at the summoning circle, where the original's bug moves it to the lair). A book or retort hoard that gives nothing pays an item of 1200 or 1000 mana instead.
- The client shows the original's boxes (`site_prompt.rs`): the Yes/No question with the site's picture, and the treasure box (`--site ask|found|treasure` for screenshots). Missing: the discovery music and the colour slide of the node pictures.
- Checked against `Create_Lair` and `Lair_Make_Guardians` (17 October 2026): the guard choice (dearest unit under a share of the budget, first of equals, up to 8 of the first kind, the second kind from what is left, nine in all), the hoard rolls and the treasure box texts follow the original; the differences are the ones above. The battlefield of a site has its building in the middle (`Structure`), which nobody can enter.
- A winner takes only what it can use (task 353, `claim_lair`): what it cannot (a spell, book or retort for a neutral city's player, items for a player with no hero, a freed prisoner when the hero roster is full or the neutral city has no room) is rerolled from its value with the hoard's table (`reroll_hoard`), as gold, mana and (with a hero) items. A prisoner joins a neutral winner as a hero in its nearest city with room; a wizard still gets the free offer.
- Neutral cities raid (task 354, `f4x_ai::neutral`): a city with a site it has explored within 8 squares builds a party of 3 beyond its garrison, sends it to a free square next to the site, assaults it with the battle AI playing both sides, and goes home. Caveat: a city sees only about 2 squares, so raids are rare until neutrals scout.
