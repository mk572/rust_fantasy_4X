//! The animation of a spell aimed at another wizard (`Target_Wizard_Screen`): the target's
//! portrait in the diplomacy room, with the spell's own picture (Spell Blast, Drain Power, Cruel
//! Unminding, Subversion) playing over it. It is skipped with Spell Animations off. A rival's
//! cast shows with Enemy Spells on, Detect Magic and contact, as the other overland scenes do.

use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::{App, HUMAN};
use f4x_core::player::PlayerId;
use f4x_core::rules::SpellId;
use f4x_data::spell_art::Role;
use macroquad::prelude::*;

/// Where the portrait goes (as in the diplomacy room).
const PORTRAIT_AT: (f32, f32) = (107.0, 13.0);
/// Two ticks of the original's 55 ms timer a stage.
const STAGE: f64 = 0.11;
/// The picture plays at least this many stages.
const MIN_STAGES: usize = 12;

#[derive(Debug, Clone)]
pub(crate) struct WizardAnim {
    portrait: usize,
    picture: (String, usize),
    started: Option<f64>,
}

impl Modal for WizardAnim {
    fn kind(&self) -> &'static str {
        kind::WIZARD_ANIM
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.wizard_anim_frame(*self, input))
    }
}

impl App {
    /// Queue the animation of a spell `player` cast on the wizard `victim`.
    pub(crate) fn announce_wizard_spell(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        victim: PlayerId,
    ) {
        if !self.prefs.get(Pref::SpellAnimations) {
            return;
        }
        if player != HUMAN && victim != HUMAN && !self.rival_spell_seen(player, None, true) {
            return;
        }
        if let Some(anim) = self.wizard_anim_for(spell, victim, None) {
            self.modals.enqueue(anim);
        }
    }

    fn wizard_anim_for(
        &self,
        spell: SpellId,
        victim: PlayerId,
        started: Option<f64>,
    ) -> Option<WizardAnim> {
        let def = self.game.content().rules.spells.get(spell);
        let picture = self.art.spell_picture(&def.id, &Role::Cast)?;
        let portrait = self
            .game
            .wizard(victim)
            .map_or(0, |w| usize::from(w.portrait()));
        Some(WizardAnim {
            portrait,
            picture,
            started,
        })
    }

    /// For screenshots: the animation of `id` on wizard `victim`.
    pub fn open_test_wizard_anim(&mut self, id: &str, victim: u8, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            eprintln!("unknown spell {id}");
            return;
        };
        match self.wizard_anim_for(spell, PlayerId(victim), Some(get_time() - seconds)) {
            Some(anim) => self.modals.enqueue(anim),
            None => eprintln!("{id} has no wizard animation"),
        }
    }

    pub(crate) fn wizard_anim_frame(
        &mut self,
        mut anim: WizardAnim,
        input: &Input,
    ) -> Option<WizardAnim> {
        let started = *anim.started.get_or_insert_with(get_time);
        let frames = self.art.frames(&anim.picture.0, anim.picture.1).max(1);
        let stages = MIN_STAGES.max(frames);
        let stage = ((get_time() - started) / STAGE) as usize;
        if stage >= stages || input.dismisses() {
            return None;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();
        let (x, y) = PORTRAIT_AT;
        self.draw_pic(&lay, "diplomac", 0, 0, 0.0, 0.0);
        self.draw_pic(&lay, "moodwiz", anim.portrait, 0, x, y);
        let (lbx, entry) = (anim.picture.0.clone(), anim.picture.1);
        self.draw_pic(&lay, &lbx, entry, stage % frames, x, y);
        Some(anim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::testdir::TestResult;

    #[test]
    fn a_spell_on_a_wizard_animates_for_the_human_and_not_when_animations_are_off() -> TestResult {
        let mut app = App::for_test();
        let spell = app
            .game
            .content()
            .rules
            .spells
            .id_of("banish")
            .ok_or("no banish")?;
        app.prefs.set(Pref::SpellAnimations, false);
        app.announce_wizard_spell(PlayerId(1), spell, HUMAN);
        assert!(!app.modals.has(kind::WIZARD_ANIM));
        app.prefs.set(Pref::SpellAnimations, true);
        app.announce_wizard_spell(PlayerId(1), spell, HUMAN);
        assert!(app.modals.has(kind::WIZARD_ANIM));
        Ok(())
    }
}
