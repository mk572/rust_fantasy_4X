//! Timed spells: an enchantment cast with `Duration::Turns(n)` is removed after `n` turns.
//!
//! The enchantment itself lives where lasting spells always live (on a unit or a city, or on
//! the wizard); a timer in `Game::timers` counts it down. A timer whose enchantment is gone
//! (dispelled, or its bearer destroyed) is dropped without a message.

use crate::city::PlayerId;
use crate::economy::Enchantments;
use crate::rules::exec::Target;
use crate::rules::SpellId;
use crate::serial::Serial;
use crate::units::UnitEnchantments;
use crate::{Event, Game};

/// What carries a timed enchantment; units and cities are named by creation number so
/// that a save can find them again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerTarget {
    Unit(Serial),
    City(Serial),
    Wizard(PlayerId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timer {
    pub target: TimerTarget,
    pub spell: SpellId,
    pub caster: PlayerId,
    /// Turns left, counted down at the start of each turn's magic step.
    pub left: u16,
}

impl Game {
    /// Start (or restart) a countdown for a lasting spell just attached to `who`.
    pub(crate) fn start_timer(
        &mut self,
        who: Target,
        spell: SpellId,
        caster: PlayerId,
        turns: u16,
    ) {
        let target = match who {
            Target::Unit(u) if self.world.contains(u) => TimerTarget::Unit(self.serial_of(u)),
            Target::City(c) if self.world.contains(c) => TimerTarget::City(self.serial_of(c)),
            Target::Player(p) => TimerTarget::Wizard(p),
            _ => return,
        };
        self.timers
            .retain(|t| !(t.target == target && t.spell == spell && t.caster == caster));
        self.timers.push(Timer {
            target,
            spell,
            caster,
            left: turns,
        });
    }

    /// The entity with this creation number, if it is still alive.
    pub(crate) fn entity_of_serial(&self, serial: Serial) -> Option<hecs::Entity> {
        self.world
            .query::<&Serial>()
            .iter()
            .find(|(_, s)| **s == serial)
            .map(|(e, _)| e)
    }

    /// Turns left on a timed enchantment, if it is one.
    pub fn enchantment_turns_left(&self, who: Target, spell: SpellId) -> Option<u16> {
        let target = match who {
            Target::Unit(u) => TimerTarget::Unit(self.serial_of(u)),
            Target::City(c) => TimerTarget::City(self.serial_of(c)),
            Target::Player(p) => TimerTarget::Wizard(p),
            Target::Tile(_) => return None,
        };
        self.timers
            .iter()
            .find(|t| t.target == target && t.spell == spell)
            .map(|t| t.left)
    }

    fn still_attached(&self, t: &Timer) -> bool {
        match t.target {
            TimerTarget::Wizard(p) => self.magic(p).is_some_and(|m| m.global.contains(&t.spell)),
            TimerTarget::City(s) => self.entity_of_serial(s).is_some_and(|e| {
                self.world
                    .get::<&Enchantments>(e)
                    .is_ok_and(|l| l.0.contains(&(t.spell, t.caster)))
            }),
            TimerTarget::Unit(s) => self.entity_of_serial(s).is_some_and(|e| {
                self.world
                    .get::<&UnitEnchantments>(e)
                    .is_ok_and(|l| l.0.contains(&(t.spell, t.caster)))
            }),
        }
    }

    /// Count every timer down; remove the enchantments whose time is up.
    pub(crate) fn tick_timers(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let timers = std::mem::take(&mut self.timers);
        let mut kept = Vec::new();
        for mut t in timers {
            if !self.still_attached(&t) {
                continue;
            }
            t.left = t.left.saturating_sub(1);
            if t.left > 0 {
                kept.push(t);
                continue;
            }
            match t.target {
                TimerTarget::Wizard(p) => {
                    if let Some(m) = self.magic_mut(p) {
                        m.global.retain(|&s| s != t.spell);
                    }
                }
                TimerTarget::City(s) => {
                    if let Some(e) = self.entity_of_serial(s) {
                        if let Ok(mut l) = self.world.get::<&mut Enchantments>(e) {
                            l.0.retain(|&x| x != (t.spell, t.caster));
                        }
                    }
                }
                TimerTarget::Unit(s) => {
                    if let Some(e) = self.entity_of_serial(s) {
                        if let Ok(mut l) = self.world.get::<&mut UnitEnchantments>(e) {
                            l.0.retain(|&x| x != (t.spell, t.caster));
                        }
                    }
                }
            }
            events.push(Event::EnchantmentExpired {
                player: t.caster,
                spell: t.spell,
            });
        }
        self.timers = kept;
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::Stat;
    use crate::testutil::content_with_magic;
    use crate::Command;

    const P0: PlayerId = PlayerId(0);

    fn world() -> Game {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        let mut p = Player::new(P0, "W");
        p.mana = 1000;
        g.players.push(p);
        g.init_wizard(P0, &[("life", 2)], &[]).unwrap();
        g
    }

    fn timed_unit(g: &mut Game) -> (hecs::Entity, SpellId) {
        let pos = Position {
            plane: Plane::Arcanus,
            x: 5,
            y: 5,
        };
        let call = g.content.rules.spells.id_of("call_spears").unwrap();
        let brief = g.content.rules.spells.id_of("brief_strength").unwrap();
        g.magic_mut_for_tests(P0).known.insert(call);
        g.magic_mut_for_tests(P0).known.insert(brief);
        g.magic_mut_for_tests(P0).skill_left = 500;
        g.apply(Command::CastSpell {
            player: P0,
            spell: call,
            target: Some(Target::Tile(pos)),
        });
        let unit = g.units_at(pos)[0];
        g.apply(Command::CastSpell {
            player: P0,
            spell: brief,
            target: Some(Target::Unit(unit)),
        });
        (unit, brief)
    }

    #[test]
    fn a_timed_spell_ends_after_its_turns() {
        let mut g = world();
        let (unit, brief) = timed_unit(&mut g);
        let melee = g.unit_stat(unit, Stat::Melee);
        assert_eq!(g.enchantment_turns_left(Target::Unit(unit), brief), Some(2));
        g.tick_timers();
        assert_eq!(g.enchantment_turns_left(Target::Unit(unit), brief), Some(1));
        let events = g.tick_timers();
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::EnchantmentExpired { .. })));
        assert_eq!(g.enchantment_turns_left(Target::Unit(unit), brief), None);
        assert_eq!(g.unit_stat(unit, Stat::Melee), melee - 1);
    }

    #[test]
    fn dispelled_or_dead_bearers_drop_their_timers_quietly() {
        let mut g = world();
        let (unit, _) = timed_unit(&mut g);
        assert_eq!(g.timers.len(), 1);
        let _ = g.world.despawn(unit);
        let events = g.tick_timers();
        assert!(g.timers.is_empty());
        assert!(!events
            .iter()
            .any(|e| matches!(e, Event::EnchantmentExpired { .. })));
    }

    #[test]
    fn timers_survive_a_save() {
        let mut g = world();
        let (unit, brief) = timed_unit(&mut g);
        g.tick_timers();
        let content = g.content.clone();
        let loaded = g.snapshot().restore(content).unwrap();
        let unit = loaded
            .entity_of_serial(g.serial_of(unit))
            .expect("the unit is back");
        assert_eq!(
            loaded.enchantment_turns_left(Target::Unit(unit), brief),
            Some(1)
        );
    }
}
