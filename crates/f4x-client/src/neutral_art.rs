//! The pictures of the neutral players (task 522, 523): one portrait, gem and hall picture for each
//! race, new art that ships with the client (`assets/neutral/<race id>/`, installed by
//! `scripts/neutral_art.py`). The diplomacy screen shows the portrait; the gem and the hall
//! picture are the sizes of a wizard's `lilwiz` and `halofam` pictures.

use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::frame::Layout;
use crate::view::App;

/// Which picture of a race. The gem and the hall picture are for the lists (task 537).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// 109x109, for the diplomacy screen.
    Portrait,
    /// 41x43, for lists.
    Gem,
    /// 30x32, for the hall of fame.
    Hall,
}

/// The pictures of each race by its number in `races.ron`: portrait, gem, hall.
const PICTURES: &[(u16, [&[u8]; 3])] = &[
    (
        0,
        [
            include_bytes!("../assets/neutral/barbarian/portrait.png"),
            include_bytes!("../assets/neutral/barbarian/gem.png"),
            include_bytes!("../assets/neutral/barbarian/hall.png"),
        ],
    ),
    (
        1,
        [
            include_bytes!("../assets/neutral/beastmen/portrait.png"),
            include_bytes!("../assets/neutral/beastmen/gem.png"),
            include_bytes!("../assets/neutral/beastmen/hall.png"),
        ],
    ),
    (
        2,
        [
            include_bytes!("../assets/neutral/dark_elf/portrait.png"),
            include_bytes!("../assets/neutral/dark_elf/gem.png"),
            include_bytes!("../assets/neutral/dark_elf/hall.png"),
        ],
    ),
    (
        3,
        [
            include_bytes!("../assets/neutral/draconian/portrait.png"),
            include_bytes!("../assets/neutral/draconian/gem.png"),
            include_bytes!("../assets/neutral/draconian/hall.png"),
        ],
    ),
    (
        4,
        [
            include_bytes!("../assets/neutral/dwarven/portrait.png"),
            include_bytes!("../assets/neutral/dwarven/gem.png"),
            include_bytes!("../assets/neutral/dwarven/hall.png"),
        ],
    ),
    (
        5,
        [
            include_bytes!("../assets/neutral/gnoll/portrait.png"),
            include_bytes!("../assets/neutral/gnoll/gem.png"),
            include_bytes!("../assets/neutral/gnoll/hall.png"),
        ],
    ),
    (
        6,
        [
            include_bytes!("../assets/neutral/halfling/portrait.png"),
            include_bytes!("../assets/neutral/halfling/gem.png"),
            include_bytes!("../assets/neutral/halfling/hall.png"),
        ],
    ),
    (
        7,
        [
            include_bytes!("../assets/neutral/high_elf/portrait.png"),
            include_bytes!("../assets/neutral/high_elf/gem.png"),
            include_bytes!("../assets/neutral/high_elf/hall.png"),
        ],
    ),
    (
        8,
        [
            include_bytes!("../assets/neutral/high_men/portrait.png"),
            include_bytes!("../assets/neutral/high_men/gem.png"),
            include_bytes!("../assets/neutral/high_men/hall.png"),
        ],
    ),
    (
        9,
        [
            include_bytes!("../assets/neutral/klackon/portrait.png"),
            include_bytes!("../assets/neutral/klackon/gem.png"),
            include_bytes!("../assets/neutral/klackon/hall.png"),
        ],
    ),
    (
        10,
        [
            include_bytes!("../assets/neutral/lizardman/portrait.png"),
            include_bytes!("../assets/neutral/lizardman/gem.png"),
            include_bytes!("../assets/neutral/lizardman/hall.png"),
        ],
    ),
    (
        11,
        [
            include_bytes!("../assets/neutral/nomad/portrait.png"),
            include_bytes!("../assets/neutral/nomad/gem.png"),
            include_bytes!("../assets/neutral/nomad/hall.png"),
        ],
    ),
    (
        12,
        [
            include_bytes!("../assets/neutral/orc/portrait.png"),
            include_bytes!("../assets/neutral/orc/gem.png"),
            include_bytes!("../assets/neutral/orc/hall.png"),
        ],
    ),
    (
        13,
        [
            include_bytes!("../assets/neutral/troll/portrait.png"),
            include_bytes!("../assets/neutral/troll/gem.png"),
            include_bytes!("../assets/neutral/troll/hall.png"),
        ],
    ),
];

thread_local! {
    static TEXTURES: RefCell<HashMap<(u16, Kind), Texture2D>> = RefCell::new(HashMap::new());
}

/// The picture of a race, made into a texture the first time.
pub fn texture(race: u16, kind: Kind) -> Option<Texture2D> {
    TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(t) = cache.get(&(race, kind)) {
            return Some(t.clone());
        }
        let bytes = PICTURES.iter().find(|(n, _)| *n == race)?.1[kind as usize];
        let tex = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
        tex.set_filter(FilterMode::Nearest);
        cache.insert((race, kind), tex.clone());
        Some(tex)
    })
}

impl App {
    /// Draw a race's neutral picture with its top left at (`x`, `y`) in the original's pixels.
    pub(crate) fn draw_neutral(&self, lay: &Layout, race: u16, kind: Kind, x: f32, y: f32) {
        let Some(tex) = texture(race, kind) else {
            return;
        };
        let at = lay.point(x, y);
        draw_texture_ex(
            &tex,
            at.x,
            at.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(tex.width(), tex.height()) * lay.scale),
                ..Default::default()
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The size a PNG says in its header.
    fn size(png: &[u8]) -> (u32, u32) {
        let at = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
        (at(16), at(20))
    }

    #[test]
    fn every_race_has_its_three_pictures_at_the_sizes_of_a_wizards() {
        let rules = &f4x_def::testdir::shipped().rules;
        for (_, race) in rules.races.iter() {
            let pictures = PICTURES
                .iter()
                .find(|(n, _)| *n == race.num)
                .unwrap_or_else(|| panic!("no pictures for {}", race.id));
            assert_eq!(
                size(pictures.1[Kind::Portrait as usize]),
                (109, 109),
                "{}",
                race.id
            );
            assert_eq!(
                size(pictures.1[Kind::Gem as usize]),
                (41, 43),
                "{}",
                race.id
            );
            assert_eq!(
                size(pictures.1[Kind::Hall as usize]),
                (30, 32),
                "{}",
                race.id
            );
        }
        assert_eq!(PICTURES.len(), rules.races.len());
    }
}
