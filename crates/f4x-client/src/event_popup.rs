//! The random events as the player sees them: the box that announces an event, the
//! name of the running world event under the minimap and the plague and boom marks on the city
//! screen. The box is the original's `Notify2` (`GENDRAW.c`): the green strip with the mascot of
//! the wizard's strongest realm on the left, the text, and the event's picture in a frame at
//! the right end. A click or a key closes it, and the next one follows.
//!
//! Tests: below. The box's place is `strip_at`, the mascot is `mascot_of`, and the boxes the
//! spells ask for are made by `dispelled_notice`, `conquest_notice` and `fizzle_notice`; a click
//! or key closing a box is `Input::dismisses`, and the drawing is left to the screenshots.

use crate::font::Style;
use crate::input::Input;
use f4x_core::game::DispelSite;
use f4x_core::text::Args;
use f4x_core::world_event::{compose, EventKind};
use f4x_data::spell_art::Role;
use macroquad::prelude::*;

use crate::audio::{music, Track};
use crate::frame::Layout;
use crate::modal::{keep, kind, Modal};
use crate::view::{App, HUMAN};

/// Where the mascot, the text and the box go for each realm (chaos, death, life, nature,
/// sorcery): the mascot's offset and the text's left (`notify2_mascot_x1`, `_y1`, `_text_x1`).
const MASCOT: [(f32, f32); 5] = [
    (-2.0, 2.0),
    (0.0, -20.0),
    (-22.0, -4.0),
    (8.0, -12.0),
    (2.0, -3.0),
];
const TEXT_X: [f32; 5] = [70.0, 73.0, 62.0, 68.0, 70.0];
const REALMS: [&str; 5] = ["chaos", "death", "life", "nature", "sorcery"];
/// The widest a line of the message goes.
const WIDTH: f32 = 175.0;
/// The top of the box (`Notify2(160, 70, ...)`).
const TOP: f32 = 70.0;
/// The `resource.lbx` entry of the strip of each kind of box (`Notify_Load`): the picture box is
/// two entries on. Green is the notice, red the warning of a spell that fizzled.
pub(crate) const GREEN: usize = 40;
const RED: usize = 43;
/// The top of the fizzle box (`Notify2(160, 80, 1, ...)`).
const FIZZLE_TOP: f32 = 80.0;

/// A box about a spell: where it goes, the colour of its strip and the picture in it (the role of
/// a shared picture in `content/spell_art.ron`).
#[derive(Debug, Clone)]
pub(crate) struct SpellNotice {
    text: String,
    strip: usize,
    top: f32,
    picture: Role,
    /// Where the picture sits from the picture box's left edge plus 7, and from the box's top.
    shift: f32,
    drop: f32,
}

impl SpellNotice {
    /// The box of a spell that fizzled (the red one).
    pub(crate) fn fizzled(&self) -> bool {
        self.strip == RED
    }
}

/// The box that announces a random event.
pub(crate) struct EventBox(pub(crate) f4x_core::world_event::EventNotice);

/// The realm whose mascot stands in the box, given the books in `REALMS` order: the most, life
/// first and a later realm only when it has more.
fn mascot_of(books: [i32; 5]) -> usize {
    let mut best = 2;
    for next in [0, 1, 3, 4] {
        if books[best] < books[next] {
            best = next;
        }
    }
    best
}

/// The strip's left edge and the right end of the text, for this realm's mascot and a text
/// `widest` pixels wide: a mascot that hangs out to the left pushes the strip right.
fn strip_at(realm: usize, widest: f32) -> (f32, f32) {
    let mascot_x = MASCOT[realm].0;
    let x = if mascot_x < 0.0 { 6.0 - mascot_x } else { 6.0 };
    (x, TEXT_X[realm] + x + widest)
}

impl Modal for EventBox {
    fn kind(&self) -> &'static str {
        kind::EVENT
    }
    fn music(&self) -> Option<Track> {
        Some(Track {
            entry: if self.0.kind.is_good() {
                music::EVENT_GOOD
            } else {
                music::EVENT_BAD
            },
            looped: false,
            event: true,
        })
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.event_popup_frame(*self, input))
    }
}

impl Modal for SpellNotice {
    fn kind(&self) -> &'static str {
        kind::SPELL_NOTICE
    }
    fn music(&self) -> Option<Track> {
        self.fizzled().then(|| Track::piece(music::SPELL_FAILURE))
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.spell_notice_frame(*self, input))
    }
}

impl App {
    /// For screenshots and checks: make an event happen to the human wizard (`great_meteor`,
    /// `gift`, `plague`...). The reason it cannot is printed.
    pub fn open_test_event(&mut self, kind: &str) {
        let Some(kind) = EventKind::from_id(kind) else {
            eprintln!(
                "unknown event {kind}; one of {}",
                EventKind::ALL.map(|k| k.id()).join(", ")
            );
            return;
        };
        match self.game.cheat_event(kind, HUMAN) {
            Ok(events) => {
                self.note(events);
            }
            Err(why) => eprintln!("{why}"),
        }
    }

    /// The realm whose mascot stands in the box: the human's strongest book, life first and a
    /// later realm only when it has more.
    fn mascot_realm(&self) -> usize {
        mascot_of(REALMS.map(|r| self.game.wizard(HUMAN).map_or(0, |m| m.books_in(r))))
    }

    /// The event box over the screen until a click or a key closes it.
    pub(crate) fn event_popup_frame(
        &mut self,
        EventBox(notice): EventBox,
        input: &Input,
    ) -> Option<EventBox> {
        let text = compose(&self.event_texts, &self.game.content().texts, &notice)?;
        let lay = Layout::new();
        let number = notice.kind.number() as usize - 1;
        let closed = self.notify2_box(
            &lay,
            TOP,
            GREEN,
            &text,
            |app, lay, x| {
                app.draw_pic(lay, "events", number, 0, x, TOP + 8.0);
            },
            input,
        );
        (!closed).then_some(EventBox(notice))
    }

    /// `Notify2`: the green strip with the mascot of the wizard's strongest realm on the left,
    /// the text, and the framed picture box at the right end, whose contents `pictures` draws
    /// at the box's left edge plus 7. Returns whether a click or a key closed it.
    pub(crate) fn notify2_box(
        &mut self,
        lay: &Layout,
        top: f32,
        strip: usize,
        text: &str,
        pictures: impl FnOnce(&mut Self, &Layout, f32),
        input: &Input,
    ) -> bool {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let realm = self.mascot_realm();
        let line = self.art.text_height(4) + 2.0;
        let lines = self.bm_wrap(4, text, WIDTH);
        let widest = lines
            .iter()
            .map(|l| self.art.text_width(4, l))
            .fold(0.0f32, f32::max)
            .ceil();
        let (mascot_x, mascot_y) = MASCOT[realm];
        let (x, right) = strip_at(realm, widest);
        self.clipped(
            (0.0, 0.0, x + right + 12.0, crate::resolution::DESIGN_H),
            |app| {
                app.draw_pic(lay, "resource", strip, 0, x, top);
                app.draw_pic(
                    lay,
                    "resource",
                    51 + realm,
                    0,
                    x + mascot_x - 1.0,
                    top + mascot_y,
                );
            },
        );
        let black = lay.rect((right + 2.0, top, 46.0, 56.0));
        draw_rectangle(black.x, black.y, black.w, black.h, BLACK);
        self.draw_pic(lay, "resource", strip + 2, 0, right + 1.0, top);
        pictures(self, lay, right + 7.0);
        let lines: Vec<(f32, String)> = lines.into_iter().map(|l| (0.0, l)).collect();
        self.draw_box_text(view, &lines, x + TEXT_X[realm], top + 8.0, line);
        input.dismisses()
    }

    /// The words of a spell that fizzled (`Fizzle_Notification`): the counter spell's caster
    /// and the one whose spell it was; one of them is the human wizard. `counter_spell` is the
    /// key of the counter spell's name.
    pub(crate) fn fizzle_text(
        &self,
        caster: f4x_core::player::PlayerId,
        counter: f4x_core::player::PlayerId,
        spell: &str,
        counter_spell: &str,
    ) -> String {
        let args = Args::new()
            .text("spell", spell)
            .text("counter_spell", self.text(counter_spell));
        if counter == HUMAN {
            let args = self.wizard_arg(args, "wizard", caster);
            self.text_with("spell.fizzled_by_you", &args)
        } else {
            let args = self.wizard_arg(args, "wizard", counter);
            self.text_with("spell.fizzled_yours", &args)
        }
    }

    /// A spell notice in its box over the screen (the map, or a battle) until it is closed.
    pub(crate) fn spell_notice_frame(
        &mut self,
        notice: SpellNotice,
        input: &Input,
    ) -> Option<SpellNotice> {
        let lay = Layout::new();
        let closed = self.notify2_box(
            &lay,
            notice.top,
            notice.strip,
            &notice.text,
            |app, lay, x| {
                app.draw_shared_pic(
                    lay,
                    &notice.picture,
                    0,
                    x + notice.shift,
                    notice.top + notice.drop,
                );
            },
            input,
        );
        (!closed).then_some(notice)
    }

    /// A spell of the human's own went to nothing (`Notify2` in `OverSpel.c`): "X has been
    /// dispelled." in the green box, the first four of a dispel; the fifth says the rest went
    /// too, and later ones are not announced. `count` is how many this dispel has announced.
    pub(crate) fn announce_dispelled(
        &mut self,
        spell: f4x_core::rules::SpellId,
        site: DispelSite,
        count: &mut u32,
    ) {
        if let Some(notice) = self.dispelled_notice(spell, site, count) {
            self.modals.enqueue(notice);
        }
    }

    /// The box for the next announced dispel, if the dispel has any left to announce.
    fn dispelled_notice(
        &self,
        spell: f4x_core::rules::SpellId,
        site: DispelSite,
        count: &mut u32,
    ) -> Option<SpellNotice> {
        let text = match *count {
            0..=3 => self.text_with(
                "spell.dispelled",
                &Args::new().text("spell", &self.game.content().rules.spells.get(spell).name),
            ),
            4 => self.text(match site {
                DispelSite::Unit | DispelSite::BattleUnit => "spell.dispelled_on_unit_more",
                DispelSite::City => "spell.dispelled_on_city_more",
                DispelSite::Field => "spell.dispelled_on_field_more",
            }),
            _ => return None,
        };
        // The boxes of a battle sit higher (`Combat_Dispel`: y 20, not 40).
        let base = if matches!(site, DispelSite::BattleUnit | DispelSite::Field) {
            20.0
        } else {
            40.0
        };
        let top = base + 25.0 * *count as f32;
        *count += 1;
        // The unit picture is a little right of the city's (`pict1_x` 4).
        let (picture, shift) = match site {
            DispelSite::Unit | DispelSite::BattleUnit => (Role::DispelledUnit, 4.0),
            DispelSite::City | DispelSite::Field => (Role::DispelledCity, 0.0),
        };
        Some(SpellNotice {
            text,
            strip: GREEN,
            top,
            picture,
            shift,
            drop: 8.0,
        })
    }

    /// The human has taken `spells` (none, one or two) from a wizard it banished or defeated
    /// (`Conquest_Spells`): the green box with the learn-spell picture.
    pub(crate) fn announce_conquest_spells(&mut self, spells: &[f4x_core::rules::SpellId]) {
        let notice = self.conquest_notice(spells);
        self.modals.enqueue(notice);
    }

    fn conquest_notice(&self, spells: &[f4x_core::rules::SpellId]) -> SpellNotice {
        let name = |app: &Self, spell| app.game.content().rules.spells.get(spell).name.clone();
        let text = match spells {
            [] => self.text("spell.conquest_none"),
            [one] => {
                let args = Args::new().text("first", name(self, *one));
                self.text_with("spell.conquest_one", &args)
            }
            [first, second, ..] => {
                let args = Args::new()
                    .text("first", name(self, *first))
                    .text("second", name(self, *second));
                self.text_with("spell.conquest_two", &args)
            }
        };
        SpellNotice {
            text,
            strip: GREEN,
            top: 40.0,
            picture: Role::Learned,
            shift: 0.0,
            drop: 9.0,
        }
    }

    /// Announce that `counter`'s spell made `caster`'s `spell` fizzle, when the human is one of
    /// them (the original says nothing of what two rivals do to each other).
    pub(crate) fn announce_fizzle(
        &mut self,
        caster: f4x_core::player::PlayerId,
        counter: f4x_core::player::PlayerId,
        spell: f4x_core::rules::SpellId,
        counter_spell: &str,
    ) {
        if let Some(notice) = self.fizzle_notice(caster, counter, spell, counter_spell) {
            self.modals.enqueue(notice);
        }
    }

    /// The red box, unless the human is neither the one whose spell fizzled nor the one who
    /// made it.
    fn fizzle_notice(
        &self,
        caster: f4x_core::player::PlayerId,
        counter: f4x_core::player::PlayerId,
        spell: f4x_core::rules::SpellId,
        counter_spell: &str,
    ) -> Option<SpellNotice> {
        if caster != HUMAN && counter != HUMAN {
            return None;
        }
        let name = self.game.content().rules.spells.get(spell).name.clone();
        let text = self.fizzle_text(caster, counter, &name, counter_spell);
        Some(SpellNotice {
            text,
            strip: RED,
            top: FIZZLE_TOP,
            picture: Role::Fizzle,
            shift: -1.0,
            drop: 7.0,
        })
    }

    /// For screenshots: five of the human's spells dispelled at once.
    pub fn open_test_dispelled(&mut self, site: DispelSite) {
        let spells: Vec<_> = self
            .game
            .content()
            .rules
            .spells
            .defs()
            .iter()
            .filter_map(|s| self.game.content().rules.spells.id_of(&s.id))
            .take(6)
            .collect();
        let mut count = 0;
        for spell in spells {
            self.announce_dispelled(spell, site, &mut count);
        }
    }

    /// For screenshots: an enemy wizard's Suppress Magic stops the human's spell.
    pub fn open_test_fizzle(&mut self) {
        let rival = self
            .game
            .player_ids()
            .into_iter()
            .find(|&id| id != HUMAN && !self.game.is_neutral_player(id));
        if let Some(rival) = rival {
            let text = self.fizzle_text(HUMAN, rival, "Fire Bolt", "spell.suppress_magic");
            self.modals.enqueue(SpellNotice {
                text,
                strip: RED,
                top: FIZZLE_TOP,
                picture: Role::Fizzle,
                shift: -1.0,
                drop: 7.0,
            });
        }
    }

    /// The red box that announces the items of fallen heroes before the item screen
    /// (`Player_Process_Item_Pool`). True when the box was closed this frame.
    pub(crate) fn hero_died_warning(&mut self, input: &Input) -> bool {
        let text = self.text("hero.items_to_share");
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars(input);
        let lay = Layout::new();
        self.warning_box(&lay, &text, input)
    }

    /// The name of the world event that is going on, centred under the minimap
    /// (`Main_Screen_Draw_Status_Window`).
    pub(crate) fn draw_world_event(&mut self, lay: &Layout) {
        let Some(kind) = self.game.events().world_event() else {
            return;
        };
        let (name, colour, y) = match kind {
            EventKind::ConjunctionSorcery => ("event.conjunction", "event_sorcery", 157.0),
            EventKind::ConjunctionChaos => ("event.conjunction", "event_chaos", 158.0),
            EventKind::ConjunctionNature => ("event.conjunction", "event_nature", 158.0),
            EventKind::BadMoon => ("event.bad_moon", "event_bad_moon", 158.0),
            EventKind::GoodMoon => ("event.good_moon", "event_good_moon", 158.0),
            _ => ("event.mana_short", "event_mana_short", 158.0),
        };
        let name = self.text(name);
        let colour = self.art.minimap().named(colour, WHITE);
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let black = Color::new(0.0, 0.0, 0.0, 1.0);
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0)] {
            self.draw_styled(
                view,
                &Style::nearest(0, black).center(),
                &name,
                280.0 + dx,
                y + dy,
            );
        }
        self.draw_styled(view, &Style::nearest(0, colour).center(), &name, 280.0, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::testdir::TestResult;

    fn spell(app: &App, id: &str) -> Result<f4x_core::rules::SpellId, String> {
        app.game
            .content()
            .rules
            .spells
            .id_of(id)
            .ok_or(format!("{id} is not in the content"))
    }

    #[test]
    fn the_mascot_is_the_realm_of_the_most_books_with_life_first_on_a_tie() {
        // chaos, death, life, nature, sorcery
        assert_eq!(mascot_of([0, 0, 0, 0, 0]), 2);
        assert_eq!(mascot_of([3, 3, 3, 3, 3]), 2);
        assert_eq!(mascot_of([4, 1, 3, 0, 0]), 0);
        assert_eq!(mascot_of([4, 4, 3, 0, 0]), 0, "chaos before death");
        assert_eq!(mascot_of([0, 0, 1, 2, 2]), 3, "nature before sorcery");
        assert_eq!(mascot_of([0, 0, 0, 0, 5]), 4);
    }

    #[test]
    fn a_mascot_that_hangs_out_to_the_left_pushes_the_strip_right() {
        let death = 1;
        let chaos = 0;
        assert_eq!(MASCOT[death].0, 0.0);
        assert_eq!(strip_at(death, 100.0), (6.0, TEXT_X[death] + 6.0 + 100.0));
        let life = 2;
        assert_eq!(MASCOT[life].0, -22.0);
        assert_eq!(strip_at(life, 100.0), (28.0, TEXT_X[life] + 28.0 + 100.0));
        assert_eq!(strip_at(chaos, 0.0).0, 6.0 + 2.0);
    }

    #[test]
    fn a_dispel_announces_four_spells_then_that_the_rest_went_too_then_nothing() -> TestResult {
        let app = App::for_test();
        let fire = spell(&app, "fire_bolt")?;
        let mut count = 0;
        for n in 0..4 {
            let notice = app
                .dispelled_notice(fire, DispelSite::City, &mut count)
                .ok_or("a dispelled spell is not announced")?;
            assert_eq!(count, n + 1);
            assert_eq!(notice.top, 40.0 + 25.0 * n as f32);
            assert!(notice.text.contains("Fire Bolt"), "{}", notice.text);
        }
        let more = app
            .dispelled_notice(fire, DispelSite::City, &mut count)
            .ok_or("the rest are not announced")?;
        assert_eq!(more.text, app.text("spell.dispelled_on_city_more"));
        assert!(app
            .dispelled_notice(fire, DispelSite::City, &mut count)
            .is_none());
        assert_eq!(count, 5);
        Ok(())
    }

    #[test]
    fn a_dispel_in_a_battle_sits_higher_and_a_unit_picture_sits_right_of_the_citys() -> TestResult {
        let app = App::for_test();
        let fire = spell(&app, "fire_bolt")?;
        let top = |site| -> Result<SpellNotice, &'static str> {
            app.dispelled_notice(fire, site, &mut 0).ok_or("no notice")
        };
        assert_eq!(top(DispelSite::Field)?.top, 20.0);
        assert_eq!(top(DispelSite::BattleUnit)?.top, 20.0);
        assert_eq!(top(DispelSite::Unit)?.top, 40.0);
        assert_eq!(top(DispelSite::Unit)?.shift, 4.0);
        assert_eq!(top(DispelSite::City)?.shift, 0.0);
        Ok(())
    }

    #[test]
    fn the_conquest_box_names_none_one_or_two_of_the_spells_taken() -> TestResult {
        let app = App::for_test();
        let (a, b, c) = (
            spell(&app, "fire_bolt")?,
            spell(&app, "fireball")?,
            spell(&app, "web")?,
        );
        assert_eq!(
            app.conquest_notice(&[]).text,
            app.text("spell.conquest_none")
        );
        let one = app.conquest_notice(&[a]).text;
        assert!(
            one.contains("Fire Bolt") && !one.contains("Fireball"),
            "{one}"
        );
        let two = app.conquest_notice(&[a, b, c]).text;
        assert!(
            two.contains("Fire Bolt") && two.contains("Fireball"),
            "{two}"
        );
        assert!(!two.contains("Web"), "only the first two: {two}");
        Ok(())
    }

    #[test]
    fn a_fizzle_is_told_only_when_the_human_is_one_of_the_two() -> TestResult {
        let app = App::for_test();
        let fire = spell(&app, "fire_bolt")?;
        let first = app
            .game
            .player_ids()
            .into_iter()
            .find(|&id| id != HUMAN && !app.game.is_neutral_player(id))
            .ok_or("the game has no rival wizard")?;
        let second = f4x_core::player::MONSTERS;
        let by_you = app
            .fizzle_notice(HUMAN, first, fire, "spell.suppress_magic")
            .ok_or("your spell fizzling is not told")?;
        assert!(by_you.fizzled());
        let yours = app
            .fizzle_notice(first, HUMAN, fire, "spell.suppress_magic")
            .ok_or("your counter spell is not told")?;
        assert_ne!(by_you.text, yours.text);
        assert!(app
            .fizzle_notice(first, second, fire, "spell.suppress_magic")
            .is_none());
        Ok(())
    }
}
