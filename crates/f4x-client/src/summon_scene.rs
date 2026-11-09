//! The scene of a summoning (`Summon_Animation`, `Spells137.c`): a framed window over the map
//! with the caster's figure behind a flame, the summoned creature rising out of it, and
//! "<Creature> Summoned" under it. It plays for Summon Hero, Summon Champion and the creature
//! summons. With Spell Animations off it is 40 stages instead of 130 and the creature does not
//! rise. The frame, the flames and the caster take the colours of the creature's realm
//! (`Summon_Animation_Load`: the palette of `SPELLSCR` 61 to 66); the creature's own picture uses
//! none of the recoloured colours.

use crate::audio::Track;
use crate::font::{rgb, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::text::Args;
use crate::view::{App, HUMAN};
use f4x_core::item::crafting::CRAFT_ITEM;
use f4x_core::player::PlayerId;
use f4x_core::spell::hero::{HeroSummon, SUMMON_HERO};
use f4x_def::rules::effects::Effect;
use f4x_def::rules::SpellDef;
use f4x_def::rules::{Rarity, SpellId, UnitId};
use f4x_def::text::Gender;
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
/// `SPELLSCR.LBX` 61: the grey palette of an arcane creature, a hero, a champion and an item.
const PALETTE_GREY: usize = 61;
/// `SPELLSCR.LBX` 62 to 66: the palettes of nature, sorcery, chaos, life and death, in the order
/// of the realm numbers 0 to 4.
const PALETTE_NATURE: usize = 62;

/// What stands in the flame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Summoned {
    /// A creature: its unit id.
    Creature(UnitId),
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
    /// `SPELLSCR.LBX` entry (61 to 66) of the palette the scene is drawn in.
    palette: usize,
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
pub(crate) fn summoned_by(def: &SpellDef) -> Option<Summoned> {
    def.effects.iter().find_map(|e| match &e.effect {
        Effect::Summon { unit, .. } => Some(Summoned::Creature(*unit)),
        Effect::Custom { name, .. } if name == SUMMON_HERO => match HeroSummon::of(def) {
            Some(HeroSummon::Hero) => Some(Summoned::Hero),
            Some(HeroSummon::Champion) => Some(Summoned::Champion),
            // Incarnation shows no scene of its own.
            _ => None,
        },
        Effect::Custom { name, .. } if name == CRAFT_ITEM => Some(Summoned::Item),
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

/// The `SPELLSCR.LBX` palette of a creature of the realm numbered `realm` (`Summon_Animation_Load`
/// adds the creature's race type to 46: nature to death are 16 to 20, arcane 15).
fn palette_entry(realm: Option<u16>) -> usize {
    match realm {
        Some(num @ 0..=4) => PALETTE_NATURE + usize::from(num),
        _ => PALETTE_GREY,
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
        let what = summoned_by(rules.spells.get(spell));
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
        let what = summoned_by(def)?;
        let palette = match &what {
            Summoned::Creature(unit) => palette_entry(
                rules
                    .units
                    .get(*unit)
                    .realm
                    .map(|realm| rules.realms.get(realm).num),
            ),
            Summoned::Hero | Summoned::Champion | Summoned::Item => PALETTE_GREY,
        };
        let (figure, label) = match &what {
            Summoned::Creature(unit) => {
                let first = UnitId::new(154) /* magic_spirit */;
                let figure = (unit.num() as usize).checked_sub(first.num() as usize)?;
                let args = Args::new().text(
                    "unit",
                    self.names.unit(rules.units.get(*unit).num).to_string(),
                );
                (figure, self.text_with("spell.summoned", &args))
            }
            Summoned::Hero => (hero_figure(hero), self.text("spell.hero_summoned")),
            Summoned::Champion => (hero_figure(hero), self.text("spell.champion_summoned")),
            Summoned::Item => (ITEM_FIGURE, self.text("spell.item_enchanted")),
        };
        let slot = self.game.wizard(player).map_or(0, |w| w.portrait());
        Some(SummonScene {
            player,
            figure,
            caster: self.art.wizard(slot).summon,
            palette,
            music: Track::piece(tune(def.rarity)),
            label,
            started,
        })
    }

    /// For screenshots: show the summoning of `id` (a spell) cast by player `by`, `seconds` in.
    pub fn open_test_summon(&mut self, id: u16, by: u8, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_numbered(id) else {
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

    /// Draw a picture of the scene in the colours of `palette`, top-left at the original's `(x, y)`.
    fn draw_summon_pic(
        &mut self,
        lay: &Layout,
        entry: usize,
        frame: usize,
        palette: usize,
        x: f32,
        y: f32,
    ) {
        let Some(tex) = self.art.summon_picture(entry, frame, palette).cloned() else {
            return;
        };
        let at = lay.point(x, y);
        draw_texture_ex(
            &tex,
            at.x,
            at.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(crate::resolution::logical_size(&tex) * lay.scale),
                ..Default::default()
            },
        );
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
        let palette = scene.palette;
        self.draw_summon_pic(&lay, 9, 0, palette, x, y);
        let caster = scene.caster;
        self.clipped((x + 7.0, y + 8.0, 150.0, 97.0), |app| {
            app.draw_summon_pic(&lay, caster, 0, palette, x + 8.0, y + 3.0);
            let back = app.art.frames("spellscr", 10).max(1);
            app.draw_summon_pic(&lay, 10, stage % back, palette, x + 55.0, y + 54.0);
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
        self.draw_summon_pic(&lay, 11, stage % front, palette, x + 64.0, y + 80.0);
        self.draw_styled(
            view,
            &Style::set(4, 4)
                .shadow()
                .edge(rgb(0x38, 0x20, 0x1C))
                .center(),
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
        let summons = |id: u16| {
            let spell = rules.spells.by_num(id).expect("spell");
            summoned_by(spell)
        };
        assert_eq!(
            summons(6 /* war_bears */),
            Some(Summoned::Creature(UnitId::new(179))) // war_bears
        );
        assert_eq!(summons(208 /* summon_hero */), Some(Summoned::Hero));
        assert_eq!(summons(212 /* summon_champion */), Some(Summoned::Champion));
        assert_eq!(summons(207 /* enchant_item */), Some(Summoned::Item));
        assert_eq!(summons(211 /* create_artifact */), Some(Summoned::Item));
        assert_eq!(summons(7 /* stone_skin */), None);
    }

    #[test]
    fn a_summoned_hero_shows_the_figure_of_its_gender() {
        assert_eq!(hero_figure(Gender::Feminine), 44);
        assert_eq!(hero_figure(Gender::Masculine), 45);
        assert_eq!(hero_figure(Gender::Neuter), 45);
    }

    #[test]
    fn a_summon_takes_the_palette_of_its_realm() {
        assert_eq!(palette_entry(Some(0)), 62); // nature, green
        assert_eq!(palette_entry(Some(1)), 63); // sorcery, blue
        assert_eq!(palette_entry(Some(2)), 64); // chaos, red
        assert_eq!(palette_entry(Some(3)), 65); // life, white
        assert_eq!(palette_entry(Some(4)), 66); // death, purple
        assert_eq!(palette_entry(Some(5)), 61); // arcane, grey
        assert_eq!(palette_entry(None), 61);
    }

    #[test]
    fn the_shipped_creatures_get_the_palette_of_their_realm() {
        let app = App::for_test();
        let rules = &app.game.content().rules;
        let palette_of = |spell: u16| {
            let def = rules.spells.by_num(spell).expect("spell");
            let Some(Summoned::Creature(unit)) = summoned_by(def) else {
                panic!("spell {spell} summons no creature");
            };
            palette_entry(
                rules
                    .units
                    .get(unit)
                    .realm
                    .map(|realm| rules.realms.get(realm).num),
            )
        };
        assert_eq!(palette_of(6 /* war_bears */), 62);
        // Every creature summon lands on one of the six palettes, and the book realms differ.
        let palettes: std::collections::BTreeSet<usize> = rules
            .spells
            .iter()
            .filter_map(|(_, def)| match summoned_by(def) {
                Some(Summoned::Creature(unit)) => Some(palette_entry(
                    rules.units.get(unit).realm.map(|r| rules.realms.get(r).num),
                )),
                _ => None,
            })
            .collect();
        assert!(palettes.iter().all(|p| (61..=66).contains(p)));
        assert!(palettes.len() >= 5, "{palettes:?}");
    }

    #[test]
    fn the_summon_music_follows_the_rarity() {
        assert_eq!(tune(Rarity::Common), 13);
        assert_eq!(tune(Rarity::VeryRare), 16);
    }
}
