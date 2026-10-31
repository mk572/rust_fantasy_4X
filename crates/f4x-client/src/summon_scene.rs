//! The scene of a summoning (`Summon_Animation`, `Spells137.c`): a framed window over the map
//! with the caster's figure behind a flame, the summoned creature rising out of it, and
//! "<Creature> Summoned" under it. It plays for Summon Hero, Summon Champion and the creature
//! summons. With Spell Animations off it is 40 stages instead of 130 and the creature does not
//! rise. The original recolours the frame by the creature's race (`SPELLSCR` 46 + race palette);
//! ours keeps the one palette.

use crate::audio::Track;
use crate::font::Style;
use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::{App, HUMAN};
use f4x_core::item::crafting::CRAFT_ITEM;
use f4x_core::player::PlayerId;
use f4x_core::rules::effects::{Effect, EffectRule};
use f4x_core::rules::{Rarity, SpellId};
use f4x_core::text::{Args, Gender};
use macroquad::prelude::*;

/// Where the frame goes (`start_x`, `start_y`).
const FRAME: (f32, f32) = (30.0, 42.0);
/// Two ticks of the original's 55 ms timer a stage.
const STAGE: f64 = 0.11;
/// The stages with Spell Animations on and off.
const STAGES_ANIMATED: usize = 130;
const STAGES_PLAIN: usize = 40;
/// The creature rises for this many stages, three pixels each.
const RISE: usize = 30;
/// `MUSIC.LBX` 13 to 16: the summon tunes by the spell's rarity.
const MUSIC_FIRST: u16 = 13;
/// `MONSTER.LBX` pictures of a summoned hero or champion: the female and the male one (the
/// original picks by the hero's gender; a hero of neither gets the male one).
const FEMALE_HERO_FIGURE: usize = 44;
const MALE_HERO_FIGURE: usize = 45;
/// The sword that rises for Enchant Item and Create Artifact.
const ITEM_FIGURE: usize = 46;

/// What stands in the flame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Summoned {
    /// A creature: its unit id.
    Creature(String),
    Hero,
    Champion,
    /// Enchant Item and Create Artifact: the sword (`MONSTER.LBX` 46) rises.
    Item,
}

/// A summoning cast and not yet shown.
#[derive(Debug, Clone)]
pub(crate) struct SummonScene {
    player: PlayerId,
    /// `MONSTER.LBX` entry of the figure that rises.
    figure: usize,
    /// `SPELLSCR.LBX` entry of the caster's figure.
    caster: usize,
    music: Track,
    /// The words under the figure.
    label: String,
    started: Option<f64>,
}

impl Modal for SummonScene {
    fn kind(&self) -> &'static str {
        kind::SUMMON_SCENE
    }
    fn music(&self) -> Option<Track> {
        Some(self.music)
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.summon_scene_frame(*self, input))
    }
}

/// What a spell summons, if it does.
pub(crate) fn summoned_by(effects: &[EffectRule]) -> Option<Summoned> {
    effects.iter().find_map(|e| match &e.effect {
        Effect::Summon { unit, .. } => Some(Summoned::Creature(unit.clone())),
        Effect::Custom(name) if name == "summon_hero" => Some(Summoned::Hero),
        Effect::Custom(name) if name == "summon_champion" => Some(Summoned::Champion),
        Effect::Custom(name) if name == CRAFT_ITEM => Some(Summoned::Item),
        _ => None,
    })
}

/// The `MONSTER.LBX` entry of a summoned hero.
fn hero_figure(gender: Gender) -> usize {
    match gender {
        Gender::Feminine => FEMALE_HERO_FIGURE,
        Gender::Masculine | Gender::Neuter => MALE_HERO_FIGURE,
    }
}

fn tune(rarity: Rarity) -> u16 {
    MUSIC_FIRST
        + match rarity {
            Rarity::Common => 0,
            Rarity::Uncommon => 1,
            Rarity::Rare => 2,
            Rarity::VeryRare => 3,
        }
}

impl App {
    /// Queue the scene of a cast that summons, when Summoning Events is on, the caster is the
    /// human or Enemy Spells is on.
    pub(crate) fn announce_summon(&mut self, player: PlayerId, spell: SpellId) {
        let rules = &self.game.content().rules;
        let what = summoned_by(&rules.spells.get(spell).effects);
        // Enchant Item and Create Artifact play for the human alone, whatever the settings
        // (`OverSpel.c`, the crafting case).
        if what == Some(Summoned::Item) && player != HUMAN {
            return;
        }
        if what != Some(Summoned::Item) {
            if !self.prefs.get(Pref::SummoningEvents) {
                return;
            }
            if player != HUMAN && !self.rival_spell_seen(player, None, true) {
                return;
            }
        }
        if matches!(what, Some(Summoned::Hero | Summoned::Champion)) {
            // The hero is picked as the spell takes effect: the scene waits for it.
            self.hero_summon_due = Some((player, spell));
            return;
        }
        if let Some(scene) = self.summon_scene_for(player, spell, None, Gender::Neuter) {
            self.modals.enqueue(scene);
        }
    }

    /// The hero type `kind` joined `player`: show the summoning that brought it, if one waits.
    pub(crate) fn announce_hero_summon(&mut self, player: PlayerId, kind: usize) {
        let Some((by, spell)) = self.hero_summon_due else {
            return;
        };
        if by != player {
            return;
        }
        self.hero_summon_due = None;
        let gender = self
            .game
            .content()
            .rules
            .hero_templates
            .get(kind)
            .map_or(Gender::Neuter, |t| t.gender);
        if let Some(scene) = self.summon_scene_for(player, spell, None, gender) {
            self.modals.enqueue(scene);
        }
    }

    fn summon_scene_for(
        &self,
        player: PlayerId,
        spell: SpellId,
        started: Option<f64>,
        hero: Gender,
    ) -> Option<SummonScene> {
        let rules = &self.game.content().rules;
        let def = rules.spells.get(spell);
        let what = summoned_by(&def.effects)?;
        let (figure, label) = match &what {
            Summoned::Creature(id) => {
                let unit = rules.units.id_of(id)?;
                let first = rules.units.id_of("magic_spirit")?;
                let figure = (unit.num() as usize).checked_sub(first.num() as usize)?;
                let args = Args::new().text("unit", rules.units.get(unit).name.clone());
                (figure, self.text_with("spell.summoned", &args))
            }
            Summoned::Hero => (hero_figure(hero), self.text("spell.hero_summoned")),
            Summoned::Champion => (hero_figure(hero), self.text("spell.champion_summoned")),
            Summoned::Item => (ITEM_FIGURE, self.text("spell.item_enchanted")),
        };
        let portrait = self
            .game
            .wizard(player)
            .map_or(0, |w| usize::from(w.portrait()));
        Some(SummonScene {
            player,
            figure,
            caster: 46 + portrait.min(13),
            music: Track::piece(tune(def.rarity)),
            label,
            started,
        })
    }

    /// For screenshots: show the summoning of `id` (a spell) cast by player `by`, `seconds` in.
    pub fn open_test_summon(&mut self, id: &str, by: u8, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            eprintln!("unknown spell {id}");
            return;
        };
        match self.summon_scene_for(
            PlayerId(by),
            spell,
            Some(get_time() - seconds),
            Gender::Neuter,
        ) {
            Some(scene) => self.modals.enqueue(scene),
            None => eprintln!("{id} summons nothing"),
        }
    }

    pub(crate) fn summon_scene_frame(
        &mut self,
        mut scene: SummonScene,
        input: &Input,
    ) -> Option<SummonScene> {
        let animated = self.prefs.get(Pref::SpellAnimations);
        let started = *scene.started.get_or_insert_with(get_time);
        let stage = ((get_time() - started) / STAGE) as usize;
        let stages = if animated {
            STAGES_ANIMATED
        } else {
            STAGES_PLAIN
        };
        if stage >= stages || input.dismisses() {
            return None;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, y) = FRAME;
        self.draw_pic(&lay, "spellscr", 9, 0, x, y);
        let caster = scene.caster;
        self.clipped((x + 7.0, y + 8.0, 150.0, 97.0), |app| {
            app.draw_pic(&lay, "spellscr", caster, 0, x + 8.0, y + 3.0);
            let back = app.art.frames("spellscr", 10).max(1);
            app.draw_pic(&lay, "spellscr", 10, stage % back, x + 55.0, y + 54.0);
        });
        let rise = if animated && scene.player == HUMAN && stage < RISE {
            y + 110.0 - (stage * 3) as f32
        } else {
            y + 18.0
        };
        let figure = scene.figure;
        self.clipped((0.0, 0.0, crate::resolution::DESIGN_W, y + 105.0), |app| {
            app.draw_pic(&lay, "monster", figure, 0, x + 76.0, rise);
        });
        let front = self.art.frames("spellscr", 11).max(1);
        self.draw_pic(&lay, "spellscr", 11, stage % front, x + 64.0, y + 80.0);
        self.draw_styled(
            view,
            &Style::outlined(4).center(),
            &scene.label,
            x + 90.0,
            y + 117.0,
        );
        Some(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_spells_that_summon_are_the_ones_the_scene_knows() {
        let app = App::for_test();
        let rules = &app.game.content().rules;
        let summons = |id: &str| {
            let spell = rules.spells.by_name(id).expect("spell");
            summoned_by(&spell.effects)
        };
        assert_eq!(
            summons("war_bears"),
            Some(Summoned::Creature("war_bears".into()))
        );
        assert_eq!(summons("summon_hero"), Some(Summoned::Hero));
        assert_eq!(summons("summon_champion"), Some(Summoned::Champion));
        assert_eq!(summons("enchant_item"), Some(Summoned::Item));
        assert_eq!(summons("create_artifact"), Some(Summoned::Item));
        assert_eq!(summons("stone_skin"), None);
    }

    #[test]
    fn a_summoned_hero_shows_the_figure_of_its_gender() {
        assert_eq!(hero_figure(Gender::Feminine), 44);
        assert_eq!(hero_figure(Gender::Masculine), 45);
        assert_eq!(hero_figure(Gender::Neuter), 45);
    }

    #[test]
    fn the_summon_music_follows_the_rarity() {
        assert_eq!(tune(Rarity::Common), 13);
        assert_eq!(tune(Rarity::VeryRare), 16);
    }
}
