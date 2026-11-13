//! A computer wizard's guaranteed spell of its main realm (`spell_library.ai_main_realm_known`).
//!
//! Tests: below.

use f4x_core::game::Game;
use f4x_core::player::PlayerId;
use f4x_core::rules::RealmId;
use f4x_core::wizard::sheet::KnownSpells;

/// A computer wizard also knows one spell of its main realm, the realm with most books (the
/// first in the realms' order on a tie), from `spell_library.ai_main_realm_known`. The
/// human wizard and the independent players get none.
pub fn give_main_realm_spell(game: &mut Game, player: PlayerId) {
    if game.human() == Some(player) {
        return;
    }
    let guaranteed = game.settings().ai.wizards.guaranteed_summoning_spell;
    let Some(w) = game.wizard(player) else {
        return;
    };
    let books = w.books();
    let main = game
        .content()
        .rules
        .realms
        .defs()
        .iter()
        .filter_map(|r| {
            let id = RealmId::new(r.num);
            Some((id, *books.get(&id)?))
        })
        .fold(None, |best: Option<(RealmId, u8)>, (id, n)| match best {
            Some((_, b)) if b >= n => best,
            _ => Some((id, n)),
        });
    let Some((realm, _)) = main else {
        return;
    };
    let library = &game.content().settings.spell_library;
    let known = guaranteed
        .then(|| library.ai_main_realm_known.get(&realm).copied())
        .flatten();
    let open = (game.difficulty() >= library.ai_main_realm_researchable_from)
        .then(|| library.ai_main_realm_researchable.get(&realm).copied())
        .flatten();
    if let Some(open) = open {
        let known = game.wizard(player).is_some_and(|w| w.knows(open));
        if let Some(mut w) = game.wizard_mut(player).filter(|_| !known) {
            w.get_mut::<KnownSpells>().researchable.insert(open);
        }
    }
    match known {
        Some(spell) => {
            if let Some(mut w) = game.wizard_mut(player) {
                w.learn(spell);
            }
            game.refill_research(player);
        }
        None => game.refill_research(player),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::game::setting::GameSettings;
    use f4x_core::map::{Grid, Map};
    use f4x_core::wizard::magic::Retort;
    use f4x_core::wizard::WizardBuild;
    use f4x_def::testdir::{shipped, TestResult};

    const P0: PlayerId = PlayerId(0);

    /// A game on the shipped content with one player, the settings edited by `edit`.
    fn game(edit: impl FnOnce(&mut GameSettings)) -> Game {
        let c = shipped();
        let grass = c
            .terrains
            .id_numbered(4 /* grassland */)
            .expect("the shipped content has grassland");
        let mut settings = GameSettings::default();
        edit(&mut settings);
        let map = Map::new(Grid::new(14, 8, grass), Grid::new(14, 8, grass));
        let mut g = Game::begin_world(5, settings, c, Default::default(), map, Vec::new());
        g.add_player(P0, "W");
        g
    }

    fn preset(books: &[(RealmId, u8)], retorts: &[Retort]) -> WizardBuild {
        WizardBuild {
            name: "Test".into(),
            preset: true,
            books: books.iter().copied().collect(),
            retorts: retorts.iter().copied().collect(),
            ..Default::default()
        }
    }

    fn spell_id(g: &Game, id: u16) -> f4x_core::rules::SpellId {
        g.content()
            .rules
            .spells
            .id_numbered(id)
            .expect("a shipped spell")
    }

    #[test]
    fn a_computer_wizard_knows_a_spell_of_its_main_realm_and_the_human_does_not() -> TestResult {
        // Most books wins; on a tie the first realm in the order nature, sorcery, chaos, life, death.
        for (books, spell) in [
            (
                vec![
                    (RealmId::new(0 /* nature */), 4),
                    (RealmId::new(3 /* life */), 2),
                ],
                9, /* sprites */
            ),
            (
                vec![
                    (RealmId::new(1 /* sorcery */), 3),
                    (RealmId::new(2 /* chaos */), 3),
                ],
                49, /* nagas */
            ),
            (
                vec![
                    (RealmId::new(2 /* chaos */), 1),
                    (RealmId::new(3 /* life */), 5),
                ],
                129, /* guardian_spirit */
            ),
            (
                vec![
                    (RealmId::new(4 /* death */), 2),
                    (RealmId::new(3 /* life */), 2),
                ],
                129, /* guardian_spirit */
            ),
            (
                vec![
                    (RealmId::new(4 /* death */), 3),
                    (RealmId::new(3 /* life */), 2),
                ],
                166, /* ghouls */
            ),
        ] {
            let mut g = game(|s| s.ai.wizards.guaranteed_summoning_spell = true);
            g.create_wizard(P0, &preset(&books, &[]))
                .map_err(|e| format!("{e:?}"))?;
            give_main_realm_spell(&mut g, P0);
            assert!(
                g.wizard(P0)
                    .ok_or("wizard is missing")?
                    .known()
                    .contains(&spell_id(&g, spell)),
                "{spell}"
            );
        }
        let mut g = game(|_| {});
        g.create_wizard(P0, &preset(&[(RealmId::new(0 /* nature */), 1)], &[]))
            .map_err(|e| format!("{e:?}"))?;
        g.set_human(Some(P0));
        give_main_realm_spell(&mut g, P0);
        assert!(!g
            .wizard(P0)
            .ok_or("wizard is missing")?
            .known()
            .contains(&spell_id(&g, 9 /* sprites */)));
        Ok(())
    }

    #[test]
    fn from_hard_on_a_computer_wizard_may_research_an_uncommon_summoning_spell() -> TestResult {
        for (difficulty, open) in [(2, false), (3, true), (4, true)] {
            let mut g = game(|s| s.basic.difficulty = difficulty);
            g.create_wizard(P0, &preset(&[(RealmId::new(0 /* nature */), 1)], &[]))
                .map_err(|e| format!("{e:?}"))?;
            give_main_realm_spell(&mut g, P0);
            let m = g.wizard(P0).ok_or("wizard is missing")?;
            let basilisk = spell_id(&g, 20 /* basilisk */);
            assert_eq!(m.can_research(basilisk), open, "difficulty {difficulty}");
            assert!(!m.knows(basilisk));
        }
        Ok(())
    }
}
