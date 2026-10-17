//! The box that announces a finished building (`City_Built_Building_Message` in ReMoM
//! `CityScr.c`): the green `Notify2` strip over the map, with the grass picture of `backgrnd`
//! 10 in the picture box and the building's own picture centred on it. Walls have the big wall
//! picture and lose the plural "s" of their name. One box per building, oldest first, after the
//! Chancellor's scroll.

use f4x_core::rules::BuildingId;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::site_prompt::an;
use crate::view::{App, HUMAN};

/// The top of the box (`Notify2(160, 60, ...)`).
const TOP: f32 = 60.0;
/// The area the building is centred in, inside the picture box (`41 x 43`, below the 8 pixel
/// top margin).
const CELL: (f32, f32) = (41.0, 43.0);
/// `cityscap.lbx` entry of the big wall picture.
const WALL_PICTURE: usize = 114;

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
        self.built_notices.push_back((city, building));
    }

    /// Remember a finished building of the human wizard's, to be announced.
    pub(crate) fn announce_built(&mut self, city: hecs::Entity, building: BuildingId) {
        if self.game.economy().city_owner(city) == Some(HUMAN) {
            self.built_notices.push_back((city, building));
        }
    }

    /// The oldest finished building not yet announced, over the map until it is closed.
    /// Returns whether the box took the frame.
    pub(crate) fn built_popup_frame(&mut self) -> bool {
        let Some(&(city, building)) = self.built_notices.front() else {
            return false;
        };
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
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
            crate::cityscape::sprite_of(3 + building.index() as i16).map(|(entry, _)| entry)
        };
        let text = format!(
            "The {} of {} has completed the construction of {} {name}.",
            self.city_size_name(city),
            self.city_name(city),
            an(&name),
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
        );
        if closed {
            self.built_notices.pop_front();
        }
        true
    }

    /// The part of a picture's first frame that is not transparent: left, top, width, height
    /// (`Get_Bitmap_Actual_Size`).
    fn picture_extent(&mut self, lbx: &str, entry: usize) -> (f32, f32, f32, f32) {
        let Some(tex) = self.art.picture(lbx, entry, 0) else {
            return (0.0, 0.0, 0.0, 0.0);
        };
        let img = tex.get_texture_data();
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
        if x1 <= x0 {
            return (0.0, 0.0, w as f32, h as f32);
        }
        (x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32)
    }
}
