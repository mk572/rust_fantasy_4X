//! Whether the human sees another wizard's spell being cast on the map: the original's test
//! (`OverSpel.c`, `Spells129.c`, `Spells130.c`, `Spells132.c`) is Enemy Spells on, the human's
//! Detect Magic active, and for a summon contact with the caster, for a spell on the map the
//! square explored. The human's own casts need no test (the callers check that first).

use crate::preferences::Pref;
use crate::view::{App, HUMAN};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;

impl App {
    /// `at` is the square the spell lands on when it must be explored; `contact` asks for contact
    /// with the caster (summons).
    pub(crate) fn rival_spell_seen(
        &self,
        caster: PlayerId,
        at: Option<Position>,
        contact: bool,
    ) -> bool {
        self.prefs.get(Pref::EnemySpells)
            && self.game.has_global(HUMAN, "detect_magic")
            && at.is_none_or(|pos| self.game.is_explored(HUMAN, pos))
            && (!contact || self.game.have_met(HUMAN, caster))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rivals_spell_stays_unseen_without_detect_magic() {
        let app = App::for_test();
        assert!(app.prefs.get(Pref::EnemySpells));
        assert!(!app.game.has_global(HUMAN, "detect_magic"));
        assert!(!app.rival_spell_seen(PlayerId(1), None, false));
        assert!(!app.rival_spell_seen(PlayerId(1), None, true));
    }

    #[test]
    fn a_rivals_spell_stays_unseen_with_enemy_spells_off() {
        let mut app = App::for_test();
        app.prefs.toggle(Pref::EnemySpells);
        assert!(!app.rival_spell_seen(PlayerId(1), None, false));
    }
}
