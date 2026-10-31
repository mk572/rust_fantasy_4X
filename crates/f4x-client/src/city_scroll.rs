//! The scroll of a city spell (`Cast_Spell_City_Enchantment_Animation_1`, `Spells137.c`): the
//! city's cityscape in a framed window, "<size> of <city>" above it and "You cast <spell>" (or
//! "<wizard> casts <spell>") below it, with the realm's good or bad music. Earthquake shakes
//! the window. With Spell Animations off it is a short still. The building the original lets
//! appear in the cityscape (Move Fortress, Earth Gate and the like) is not drawn: the city's
//! own buildings are shown as they are.

use crate::audio::Track;
use crate::font::Style;
use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::{App, HUMAN};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::rules::{SpellId, SpellTarget};
use f4x_core::text::Args;
use macroquad::prelude::*;

/// Where the scroll goes, and the cityscape inside it.
const FRAME: (f32, f32) = (53.0, 38.0);
const CITYSCAPE: (f32, f32) = (58.0, 65.0);
/// Two ticks of the original's 55 ms timer a stage.
const STAGE: f64 = 0.11;
/// The animated scroll stays this many stages (the original waits for a click after eight).
const STAGES_ANIMATED: usize = 40;
const STAGES_PLAIN: usize = 12;
/// `MUSIC.LBX`: the good tunes of arcane, chaos, death, life, nature and sorcery; the bad ones
/// follow six on.
const GOOD_MUSIC: [u16; 6] = [0, 1, 2, 3, 4, 5];
const BAD_OFFSET: u16 = 6;

#[derive(Debug, Clone)]
pub(crate) struct CityScroll {
    player: PlayerId,
    spell: SpellId,
    city: hecs::Entity,
    music: Track,
    shakes: bool,
    started: Option<f64>,
}

impl Modal for CityScroll {
    fn kind(&self) -> &'static str {
        kind::CITY_SCROLL
    }
    fn music(&self) -> Option<Track> {
        Some(self.music)
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.city_scroll_frame(*self, input))
    }
}

fn realm_slot(realm: &str) -> usize {
    match realm {
        "chaos" => 1,
        "death" => 2,
        "life" => 3,
        "nature" => 4,
        "sorcery" => 5,
        _ => 0,
    }
}

impl App {
    /// Queue the scroll of a spell that landed on the city at `at`: with City Spell Events on,
    /// for the human's casts, and for a rival's with Enemy Spells on and the city explored.
    pub(crate) fn announce_city_spell(&mut self, player: PlayerId, spell: SpellId, at: Position) {
        if !self.prefs.get(Pref::CitySpellEvents) {
            return;
        }
        let Some(city) = self.game.economy().city_at(at) else {
            return;
        };
        // A cast on the human's own city is always shown.
        let own = self.game.economy().city_owner(city) == Some(HUMAN);
        if player != HUMAN && !own && !self.rival_spell_seen(player, Some(at), false) {
            return;
        }
        if let Some(scroll) = self.city_scroll_for(player, spell, city, None) {
            self.modals.enqueue(scroll);
        }
    }

    fn city_scroll_for(
        &self,
        player: PlayerId,
        spell: SpellId,
        city: hecs::Entity,
        started: Option<f64>,
    ) -> Option<CityScroll> {
        let def = self.game.content().rules.spells.get(spell);
        if !matches!(def.target, SpellTarget::OwnCity | SpellTarget::EnemyCity) {
            return None;
        }
        let bad = if def.target == SpellTarget::EnemyCity {
            BAD_OFFSET
        } else {
            0
        };
        Some(CityScroll {
            player,
            spell,
            city,
            music: Track::piece(GOOD_MUSIC[realm_slot(&def.realm)] + bad),
            shakes: def.id == "earthquake",
            started,
        })
    }

    /// For screenshots: the scroll of `id` cast by player `by` on the first city.
    pub fn open_test_city_scroll(&mut self, id: &str, by: u8, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            eprintln!("unknown spell {id}");
            return;
        };
        let Some(city) = self.game.economy().cities().first().map(|c| c.0) else {
            return;
        };
        match self.city_scroll_for(PlayerId(by), spell, city, Some(get_time() - seconds)) {
            Some(scroll) => self.modals.enqueue(scroll),
            None => eprintln!("{id} is not a city spell"),
        }
    }

    pub(crate) fn city_scroll_frame(
        &mut self,
        mut scroll: CityScroll,
        input: &Input,
    ) -> Option<CityScroll> {
        let animated = self.prefs.get(Pref::SpellAnimations);
        let started = *scroll.started.get_or_insert_with(get_time);
        let stage = ((get_time() - started) / STAGE) as usize;
        let stages = if animated {
            STAGES_ANIMATED
        } else {
            STAGES_PLAIN
        };
        if stage >= stages || input.dismisses() {
            return None;
        }
        let mut lay = Layout::new();
        // Earthquake: the window jumps about for the first thirty stages, less each time.
        if scroll.shakes && animated && stage < 30 {
            let reach = (35 - stage) as f32 / 5.0;
            let wobble = |n: f32| ((get_time() as f32 * 97.0 + n).sin() * reach).round();
            lay.origin += vec2(wobble(0.0), wobble(2.0)) * lay.scale;
        }
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, y) = FRAME;
        self.draw_pic(&lay, "spellscr", 73, 0, x, y);
        let size = self.city_size_name(scroll.city);
        let name = self.city_name(scroll.city);
        let title = self.text_with(
            "spell.city_scroll_title",
            &Args::new().text("size", size).text("city", name),
        );
        self.draw_styled(view, &Style::outlined(5).center(), &title, 160.0, 44.0);
        let spell_name = self
            .game
            .content()
            .rules
            .spells
            .get(scroll.spell)
            .name
            .clone();
        let line = if scroll.player == HUMAN {
            self.text_with("spell.you_cast", &Args::new().text("spell", spell_name))
        } else {
            let args = self
                .wizard_arg(Args::new(), "wizard", scroll.player)
                .text("spell", spell_name);
            self.text_with("spell.rival_casts", &args)
        };
        self.draw_styled(view, &Style::outlined(4).center(), &line, 160.0, 164.0);
        self.clipped((CITYSCAPE.0, CITYSCAPE.1, 205.0, 96.0), |app| {
            let city_lay = crate::city_screen::Layout {
                ox: lay.origin.x,
                oy: lay.origin.y,
                scale: lay.scale,
            };
            app.draw_city_buildings_at(scroll.city, city_lay, CITYSCAPE.0, CITYSCAPE.1, input);
        });
        Some(scroll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curse_on_an_enemy_city_gets_the_bad_tune_of_its_realm() {
        let app = App::for_test();
        let city = app
            .game
            .economy()
            .cities()
            .first()
            .map(|c| c.0)
            .expect("the new game has a city");
        let spells = &app.game.content().rules.spells;
        let scroll =
            |id: &str| app.city_scroll_for(HUMAN, spells.id_of(id).expect("spell"), city, None);
        let good = scroll("natures_eye").expect("a city spell");
        assert_eq!(good.music, Track::piece(4));
        assert!(scroll("stone_skin").is_none(), "a unit spell has no scroll");
        let curse = scroll("earthquake").expect("a city spell");
        assert!(curse.shakes);
        assert!(curse.music.entry >= BAD_OFFSET);
    }
}
