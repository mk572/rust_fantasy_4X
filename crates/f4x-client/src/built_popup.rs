//! The box that announces a finished building (`City_Built_Building_Message` in ReMoM
//! `CityScr.c`): the green `Notify2` strip over the map, with the grass picture of `backgrnd`
//! 10 in the picture box and the building's own picture centred on it. Walls have the big wall
//! picture and lose the plural "s" of their name. One box per building, oldest first, after the
//! Chancellor's scroll.
//!
//! Tests: none, because it only draws and needs a window.

use crate::input::Input;
use f4x_core::rules::BuildingId;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::audio::{music, Track};
use crate::frame::Layout;
use crate::modal::{keep, kind, Modal};
use crate::view::{App, HUMAN};

/// The top of the box (`Notify2(160, 60, ...)`).
const TOP: f32 = 60.0;
/// The area the building is centred in, inside the picture box (`41 x 43`, below the 8 pixel
/// top margin).
const CELL: (f32, f32) = (41.0, 43.0);
/// `cityscap.lbx` entry of the big wall picture.
const WALL_PICTURE: usize = 114;

/// A finished building waiting to fade in on the city screen (`Cityscape_Build_Anim_Reset`):
/// the picture starts hidden and shows more of itself ten times in a second.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BuildIn {
    pub city: hecs::Entity,
    /// The picture's place in the cityscape's table (`cityscape::sprite_of`).
    pub idx: i16,
    /// When the city screen first drew it; a time still to come holds the picture back.
    pub started: Option<f64>,
}

/// A finished building of the human wizard's, to be announced.
pub(crate) struct BuiltNotice {
    city: hecs::Entity,
    building: BuildingId,
}

impl Modal for BuiltNotice {
    fn kind(&self) -> &'static str {
        kind::BUILT
    }
    fn music(&self) -> Option<Track> {
        Some(Track::piece(music::BUILDING_FINISHED))
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.built_popup_frame(*self, input))
    }
}

impl App {
    /// For screenshots and checks: announce this building (an id) as if the first city had just
    /// finished it.
    pub fn open_test_built(&mut self, id: &str) {
        let Some(building) = self.game.content().rules.buildings.id_of(id) else {
            eprintln!("unknown building {id}");
            return;
        };
        let Some(city) = self.game.economy().cities().first().map(|c| c.0) else {
            return;
        };
        self.modals.enqueue(BuiltNotice { city, building });
    }

    /// Remember a finished building of the human wizard's, to be announced.
    pub(crate) fn announce_built(&mut self, city: hecs::Entity, building: BuildingId) {
        if self.game.economy().city_owner(city) == Some(HUMAN) {
            self.modals.enqueue(BuiltNotice { city, building });
        }
    }

    /// The box of a finished building over the screen until it is closed.
    pub(crate) fn built_popup_frame(
        &mut self,
        BuiltNotice { city, building }: BuiltNotice,
        input: &Input,
    ) -> Option<BuiltNotice> {
        let lay = Layout::new();
        let def = self.game.content().rules.buildings.get(building);
        let walls = def.id == "city_walls";
        let mut name = def.name.clone();
        if walls {
            name.pop();
        }
        let entry = if walls {
            Some(WALL_PICTURE)
        } else {
            crate::cityscape::sprite_of(building.num() as i16).map(|(entry, _)| entry)
        };
        let text = self.text_with(
            "city.built",
            &Args::new()
                .text("size", self.city_size_name(city))
                .text("city", self.city_name(city))
                .text("building", name),
        );
        let closed = self.notify2_box(
            &lay,
            TOP,
            crate::event_popup::GREEN,
            &text,
            |app, lay, x| {
                app.draw_pic(lay, "backgrnd", 10, 0, x, TOP + 8.0);
                let Some(entry) = entry else {
                    return;
                };
                let (left, top, width, height) = app.picture_extent("cityscap", entry);
                let at_x = x + (CELL.0 - width) / 2.0 - left;
                let at_y = TOP + 8.0 + (CELL.1 - height) / 2.0 - top;
                app.draw_pic(lay, "cityscap", entry, 0, at_x, at_y);
            },
            input,
        );
        if closed {
            self.build_in = Some(BuildIn {
                city,
                idx: building.num() as i16,
                started: None,
            });
            return None;
        }
        Some(BuiltNotice { city, building })
    }

    /// The part of a picture's first frame that is not transparent: left, top, width, height
    /// (`Get_Bitmap_Actual_Size`).
    fn picture_extent(&mut self, lbx: &str, entry: usize) -> (f32, f32, f32, f32) {
        let Some(img) = self.art.pixels(lbx, entry, 0) else {
            return (0.0, 0.0, 0.0, 0.0);
        };
        let (w, h) = (usize::from(img.width), usize::from(img.height));
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if img.bytes[(y * w + x) * 4 + 3] > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        // In the squares of the design grid, whatever the art scale.
        let k = f32::from(crate::resolution::art_scale());
        if x1 <= x0 {
            return (0.0, 0.0, w as f32 / k, h as f32 / k);
        }
        (
            x0 as f32 / k,
            y0 as f32 / k,
            (x1 - x0) as f32 / k,
            (y1 - y0) as f32 / k,
        )
    }
}
