//! A read-only view of the game for the domains (diplomacy, magic and the rest).
//!
//! The base rule (`design.md` section 3) is that domain logic takes the pieces it needs and not
//! `&mut Game`. Where a domain needs a few facts about the rest of the world (who is in the
//! game, what a wizard can see, how much an army costs) it asks them through [`WorldView`]. The
//! trait is small on purpose: a question only one domain asks stays out of it, and it has no
//! entity or component in its signatures, so a test can answer it with [`FakeView`]
//! (`game/testutil.rs`) without a world, a map or a fog.
//!
//! [`Game`] implements it by answering from what it has. Add a question here when two domains
//! need it, with a plain-data answer.

use crate::content::Content;
use crate::map::Position;
use crate::player::{MagicRef, PlayerHandle, PlayerId};

use crate::game::Game;

/// Something of another player that a wizard can see: one of its cities, or a unit the wizard
/// can make out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sighting {
    pub owner: PlayerId,
    pub pos: Position,
}

/// A unit as the domains need to know it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitInfo {
    pub pos: Position,
    /// What the unit cost to make (the original's measure of its strength).
    pub cost: u32,
}

pub trait WorldView {
    /// The turn number.
    fn turn(&self) -> u32;
    /// The human wizard, if there is one.
    fn human(&self) -> Option<PlayerId>;
    /// The game's difficulty: 0 intro, 1 easy, 2 normal, 3 hard, 4 impossible.
    fn difficulty(&self) -> u8;
    /// The definitions of the game.
    fn content(&self) -> &Content;
    /// Every player's id: wizards, independent cities' players, ascending.
    fn player_ids(&self) -> Vec<PlayerId>;
    /// A player's data.
    fn player(&self, id: PlayerId) -> Option<PlayerHandle<'_>>;
    /// Whether the player is neutral: the monsters' owner or an independent city's player.
    fn is_neutral(&self, id: PlayerId) -> bool;

    /// Another player's cities and units that `viewer` sees now: on squares it has in sight,
    /// the units only when it can make them out (an invisible one is hidden).
    fn foreign_sightings(&self, viewer: PlayerId) -> Vec<Sighting>;
    /// Another player's cities and units within `radius` squares of `at` (the same plane),
    /// the units only when `viewer` can make them out. The squares need not be in sight.
    fn foreign_sightings_near(&self, viewer: PlayerId, at: Position, radius: u32) -> Vec<Sighting>;
    /// The units a player has.
    fn units_of(&self, player: PlayerId) -> Vec<UnitInfo>;
    /// How many cities (outposts too) a player has.
    fn city_count(&self, player: PlayerId) -> usize;

    /// A wizard's magic.
    fn magic(&self, id: PlayerId) -> Option<MagicRef<'_>> {
        self.player(id).map(PlayerHandle::into_magic)
    }

    /// A player that is a wizard: neither the monsters nor an independent city.
    fn is_wizard(&self, id: PlayerId) -> bool {
        self.player(id).is_some() && !self.is_neutral(id)
    }

    /// Wizards that have not been defeated, in the order of the players.
    fn living_wizards(&self) -> Vec<PlayerId> {
        self.player_ids()
            .into_iter()
            .filter(|&id| self.is_wizard(id) && self.player(id).is_some_and(|p| !p.defeated))
            .collect()
    }

    /// Does the wizard keep the global spell with this id going?
    fn has_global(&self, player: PlayerId, spell: &str) -> bool {
        self.magic(player).is_some_and(|m| {
            m.global
                .iter()
                .any(|&s| self.content().rules.spells.get(s).id == spell)
        })
    }
}

impl WorldView for Game {
    fn turn(&self) -> u32 {
        self.turn
    }

    fn human(&self) -> Option<PlayerId> {
        self.human
    }

    fn difficulty(&self) -> u8 {
        self.settings.basic.difficulty
    }

    fn content(&self) -> &Content {
        &self.content
    }

    fn player_ids(&self) -> Vec<PlayerId> {
        Game::player_ids(self)
    }

    fn player(&self, id: PlayerId) -> Option<PlayerHandle<'_>> {
        Game::player(self, id).map(PlayerHandle::Held)
    }

    fn is_neutral(&self, id: PlayerId) -> bool {
        self.is_neutral_player(id)
    }

    fn foreign_sightings(&self, viewer: PlayerId) -> Vec<Sighting> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let width = map.width() as usize;
        let seen: Vec<Vec<bool>> = crate::map::Plane::ALL
            .iter()
            .map(|&plane| self.economy().visibility(viewer, plane))
            .collect();
        let sees = |p: Position| {
            seen.get(p.plane.index())
                .and_then(|grid| grid.get(p.y as usize * width + p.x as usize))
                .copied()
                .unwrap_or(false)
        };
        self.sightings_where(viewer, sees)
    }

    fn foreign_sightings_near(&self, viewer: PlayerId, at: Position, radius: u32) -> Vec<Sighting> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let grid = map.plane(at.plane);
        self.sightings_where(viewer, |p| {
            p.plane == at.plane && grid.chebyshev((p.x, p.y), (at.x, at.y)) <= radius
        })
    }

    fn units_of(&self, player: PlayerId) -> Vec<UnitInfo> {
        self.units()
            .player_units(player)
            .into_iter()
            .filter_map(|u| {
                Some(UnitInfo {
                    pos: self.units().unit_position(u)?,
                    cost: self.units().unit_def(u)?.cost,
                })
            })
            .collect()
    }

    fn city_count(&self, player: PlayerId) -> usize {
        self.economy()
            .cities()
            .iter()
            .filter(|(_, c, _)| c.owner == player)
            .count()
    }
}

impl Game {
    /// The cities and visible units of other players on the squares `keep` accepts.
    fn sightings_where(&self, viewer: PlayerId, keep: impl Fn(Position) -> bool) -> Vec<Sighting> {
        let mut out = Vec::new();
        for (_, city, pos) in self.economy().cities() {
            if city.owner != viewer && keep(pos) {
                out.push(Sighting {
                    owner: city.owner,
                    pos,
                });
            }
        }
        for unit in self.units().all_units() {
            let (Some(owner), Some(pos)) = (
                self.units().unit_owner(unit),
                self.units().unit_position(unit),
            ) else {
                continue;
            };
            if owner != viewer && keep(pos) && self.economy().unit_visible_to(unit, viewer) {
                out.push(Sighting { owner, pos });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplomacy::{army_strength, met_in_sight, met_near};
    use crate::game::testutil::{content_with_magic, FakeView};
    use crate::map::Plane;
    use crate::map::Position;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);
    const P2: PlayerId = PlayerId(2);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn sighting(owner: PlayerId, x: u32, y: u32) -> Sighting {
        Sighting {
            owner,
            pos: at(x, y),
        }
    }

    #[test]
    fn a_domain_can_be_tested_against_a_fake_view() {
        let mut view = FakeView::new(&[P0, P1, P2]);
        view.sightings = vec![
            (P0, sighting(P1, 12, 10)),
            (P0, sighting(P1, 14, 10)),
            (P0, sighting(P2, 30, 10)),
            (P0, sighting(P0, 11, 10)),
            (P1, sighting(P0, 12, 10)),
        ];
        assert_eq!(met_in_sight(&view, P0), vec![P1, P2], "sorted, once each");
        assert_eq!(met_in_sight(&view, P2), vec![]);
        assert_eq!(met_near(&view, P0, at(10, 10), 2), vec![P1]);
        assert_eq!(met_near(&view, P0, at(10, 10), 25), vec![P1, P2]);
        let other_plane = Position {
            plane: Plane::Myrror,
            ..at(10, 10)
        };
        assert_eq!(met_near(&view, P0, other_plane, 25), vec![]);
        view.units = vec![
            (
                P0,
                UnitInfo {
                    pos: at(1, 1),
                    cost: 30,
                },
            ),
            (
                P0,
                UnitInfo {
                    pos: at(2, 1),
                    cost: 50,
                },
            ),
            (
                P1,
                UnitInfo {
                    pos: at(3, 1),
                    cost: 7,
                },
            ),
        ];
        assert_eq!(army_strength(&view, P0), 80);
        assert_eq!(army_strength(&view, P2), 0);
    }

    #[test]
    fn the_provided_questions_follow_from_the_players() {
        let mut view = FakeView::new(&[P0, P1, P2]).with_content(content_with_magic());
        view.neutral.push(P1);
        view.players[2].1.defeated = true;
        assert!(view.is_wizard(P0) && !view.is_wizard(P1));
        assert!(!view.is_wizard(crate::player::MONSTERS) && !view.is_wizard(PlayerId(9)));
        assert_eq!(view.living_wizards(), vec![P0]);
        let spell = view
            .content
            .rules
            .spells
            .iter()
            .map(|(id, d)| (id, d.id.clone()))
            .next()
            .unwrap();
        assert!(!view.has_global(P0, &spell.1));
        view.players[0].1.magic.global.push(spell.0);
        assert!(view.has_global(P0, &spell.1));
        assert!(!view.has_global(P1, &spell.1));
    }

    #[test]
    fn the_game_answers_what_a_wizard_sees_and_has() {
        let mut g = Game::new(3);
        g.content = content_with_magic();
        let grass = g.content.terrains.id_of("grassland").unwrap();
        g.map = Some(crate::map::Map::new(
            crate::map::Grid::new(30, 20, grass),
            crate::map::Grid::new(30, 20, grass),
        ));
        for id in [P0, P1, P2] {
            g.add_player(id, "W", crate::player::Player::new());
        }
        let walker = g.content.rules.units.id_of("walker").unwrap();
        g.spawn_unit(P1, at(11, 10), walker);
        g.spawn_unit(P2, at(25, 15), walker);
        let mine = g.spawn_unit(P0, at(10, 10), walker);
        let cost = g.units().unit_def(mine).unwrap().cost;
        // Within two squares of (10, 10), whether or not it is in sight.
        let around = g.foreign_sightings_near(P0, at(10, 10), 2);
        assert_eq!(
            around,
            vec![Sighting {
                owner: P1,
                pos: at(11, 10)
            }]
        );
        // What P0 sees now: its units see around them, so the near one and not the far one.
        let seen = g.foreign_sightings(P0);
        assert_eq!(
            seen,
            vec![Sighting {
                owner: P1,
                pos: at(11, 10)
            }],
            "{seen:?}"
        );
        assert_eq!(
            g.units_of(P0),
            vec![UnitInfo {
                pos: at(10, 10),
                cost
            }]
        );
        assert_eq!(army_strength(&g, P0), cost as i32);
        // Another viewpoint: P1 sees its own side's neighbours, not itself.
        assert!(g.foreign_sightings(P1).iter().all(|s| s.owner != P1));
        assert_eq!(WorldView::turn(&g), g.turn);
    }
}
