//! The magic screen: power and its split, research, the spell book and casting.

use f4x_core::rules::exec::Target;
use f4x_core::rules::{CastContext, Rarity, SpellId, SpellTarget};
use f4x_core::Command;
use macroquad::prelude::*;

use crate::ui::{bar, button, Lines, GOLD, PANEL};
use crate::view::{App, Screen, HUMAN};

/// A spell waiting for the player to click its target on the map.
#[derive(Clone, Copy)]
pub(crate) struct Targeting {
    pub spell: SpellId,
    /// The spell is already paid for (cast over several turns) rather than starting now.
    pub resolve: bool,
}

const ROW_H: f32 = 24.0;

/// Move one share of the split to `new` percent and spread the rest over the other two in
/// proportion to what they had.
pub(crate) fn rebalance(split: [u8; 3], index: usize, new: u8) -> [u8; 3] {
    let new = new.min(100);
    let others: Vec<usize> = (0..3).filter(|&i| i != index).collect();
    let old_total: u32 = others.iter().map(|&i| u32::from(split[i])).sum();
    let rest = u32::from(100 - new);
    let mut out = split;
    out[index] = new;
    let first = match (u32::from(split[others[0]]) * rest).checked_div(old_total) {
        Some(first) => first,
        None => rest / 2,
    };
    out[others[0]] = first as u8;
    out[others[1]] = (rest - first) as u8;
    out
}

fn rarity_name(r: Rarity) -> &'static str {
    match r {
        Rarity::Common => "common",
        Rarity::Uncommon => "uncommon",
        Rarity::Rare => "rare",
        Rarity::VeryRare => "very rare",
    }
}

impl App {
    pub fn open_magic(&mut self) {
        self.screen = Screen::Magic;
    }

    fn realm_color(&self, realm: &str) -> Color {
        self.game
            .content
            .rules
            .realms
            .by_name(realm)
            .map_or(WHITE, |r| {
                Color::from_rgba(r.color.0, r.color.1, r.color.2, 255)
            })
    }

    pub(crate) fn magic_screen(&mut self) {
        if is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::M)
            || is_mouse_button_pressed(MouseButton::Right)
        {
            self.screen = Screen::Map;
            return;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, PANEL);
        let col = (w - 80.0) / 3.0;
        let (x0, x1, x2) = (30.0, 40.0 + col, 50.0 + 2.0 * col);
        self.magic_power_column(x0, col);
        self.magic_research_column(x1, col, h);
        self.magic_spell_column(x2, col, h);
        draw_text("M or Esc: back to the map", 30.0, h - 12.0, 18.0, LIGHTGRAY);
    }

    fn magic_power_column(&mut self, x: f32, width: f32) {
        let Some(magic) = self.game.magic(HUMAN).cloned() else {
            return;
        };
        let mana = self.game.player(HUMAN).map_or(0, |p| p.mana);
        let gold = self.game.player(HUMAN).map_or(0, |p| p.gold);
        let split = self.game.power_split(HUMAN);
        let upkeep = self.game.mana_upkeep(HUMAN);
        let mut lines = Lines::new(x, 40.0);
        lines.line("Wizard", GOLD);
        let books: Vec<String> = magic
            .books
            .iter()
            .map(|(r, n)| format!("{n} {r}"))
            .collect();
        lines.line(&format!("Books: {}", books.join(", ")), WHITE);
        let retorts: Vec<String> = magic.retorts.iter().map(|r| format!("{r:?}")).collect();
        if !retorts.is_empty() {
            lines.line(&format!("Retorts: {}", retorts.join(", ")), WHITE);
        }
        let fortress = match self.game.player(HUMAN) {
            Some(p) if p.defeated => "Defeated".to_string(),
            Some(p) if p.banished => "BANISHED: casting the Spell of Return".to_string(),
            Some(p) => match self.game.fortress_city(HUMAN) {
                Some(c) => format!(
                    "Fortress: {}   circle: {}",
                    self.city_name(c),
                    p.summon_at
                        .and_then(|pos| self.game.city_at(pos))
                        .map_or("-".to_string(), |c| self.city_name(c))
                ),
                None => "No fortress".to_string(),
            },
            None => String::new(),
        };
        lines.line(&fortress, WHITE);
        lines.gap(8.0);
        lines.line(
            &format!(
                "Mana reserve {mana}   income +{}   upkeep -{upkeep}",
                split.mana
            ),
            WHITE,
        );
        lines.line(
            &format!(
                "Casting skill {}   (+{} points a turn)",
                self.game.casting_skill(HUMAN),
                split.skill
            ),
            WHITE,
        );
        lines.line(
            &format!("Power base {}", self.game.power_base(HUMAN)),
            WHITE,
        );
        lines.gap(8.0);
        lines.line("Power split (click a bar)", GOLD);
        let shares = [magic.mana_pct, magic.research_pct, magic.skill_pct];
        let names = ["Mana", "Research", "Skill"];
        let colors = [
            Color::new(0.3, 0.5, 0.95, 1.0),
            Color::new(0.35, 0.8, 0.4, 1.0),
            Color::new(0.9, 0.7, 0.25, 1.0),
        ];
        for i in 0..3 {
            let y = lines.y;
            draw_text(names[i], x, y + 16.0, 20.0, WHITE);
            let r = Rect::new(x + 90.0, y, width - 170.0, 20.0);
            bar(r, f32::from(shares[i]) / 100.0, colors[i]);
            draw_text(
                format!("{}%", shares[i]),
                r.x + r.w + 8.0,
                y + 16.0,
                20.0,
                WHITE,
            );
            if is_mouse_button_pressed(MouseButton::Left) && r.contains(mouse_position().into()) {
                let frac = ((mouse_position().0 - r.x) / r.w).clamp(0.0, 1.0);
                let pct = ((frac * 20.0).round() as u8) * 5;
                let s = rebalance(shares, i, pct);
                let events = self.game.apply(Command::SetPowerSplit {
                    player: HUMAN,
                    mana: s[0],
                    research: s[1],
                    skill: s[2],
                });
                self.note(events);
            }
            lines.gap(30.0);
        }
        lines.gap(6.0);
        lines.line(
            &format!(
                "Research +{}/turn   city research {}",
                self.game.research_income(HUMAN),
                self.game.city_research(HUMAN)
            ),
            WHITE,
        );
        lines.gap(8.0);
        // Alchemy.
        let y = lines.y;
        let alchemy = |gold_to_mana, label: &str, rect: Rect, game: &f4x_core::Game| {
            let enough = if gold_to_mana { gold >= 10 } else { mana >= 10 };
            let _ = game;
            (button(rect, label, enough), gold_to_mana)
        };
        let a = alchemy(
            true,
            "10 gold > mana",
            Rect::new(x, y, width / 2.0 - 6.0, 28.0),
            &self.game,
        );
        let b = alchemy(
            false,
            "10 mana > gold",
            Rect::new(x + width / 2.0, y, width / 2.0 - 6.0, 28.0),
            &self.game,
        );
        for (clicked, gold_to_mana) in [a, b] {
            if clicked {
                let events = self.game.apply(Command::Alchemy {
                    player: HUMAN,
                    amount: 10,
                    gold_to_mana,
                });
                self.note(events);
            }
        }
        lines.y += 40.0;
        lines.line(
            &format!(
                "Alchemy rate {}",
                if magic.has(f4x_core::magic::Retort::Alchemy) {
                    "1 for 1"
                } else {
                    "2 for 1"
                }
            ),
            GRAY,
        );
        // Casting in progress.
        lines.gap(10.0);
        lines.line("Casting", GOLD);
        match magic.casting {
            None => lines.line("Nothing in progress", GRAY),
            Some(c) => {
                let name = self.game.content.rules.spells.get(c.spell).name.clone();
                if c.remaining > 0 {
                    lines.line(&format!("{name}: {} mana to go", c.remaining), WHITE);
                } else {
                    lines.line(&format!("{name} is ready: choose a target"), WHITE);
                }
                let y = lines.y;
                if c.remaining <= 0
                    && self.game.spell_needs_target(HUMAN, c.spell)
                    && button(Rect::new(x, y, 130.0, 28.0), "Choose target", true)
                {
                    self.targeting = Some(Targeting {
                        spell: c.spell,
                        resolve: true,
                    });
                    self.screen = Screen::Map;
                }
                if button(Rect::new(x + 140.0, y, 110.0, 28.0), "Cancel cast", true) {
                    let events = self.game.apply(Command::CancelCast { player: HUMAN });
                    self.note(events);
                }
            }
        }
    }

    fn magic_research_column(&mut self, x: f32, width: f32, height: f32) {
        let mut lines = Lines::new(x, 40.0);
        let magic = self.game.magic(HUMAN).cloned().unwrap_or_default();
        lines.line("Research", GOLD);
        match magic.researching {
            Some(s) => {
                let name = self.game.content.rules.spells.get(s).name.clone();
                let income = self.game.research_income(HUMAN).max(1);
                let turns = (magic.research_remaining + income - 1) / income;
                lines.line(
                    &format!(
                        "{name}: {} to go (~{turns} turns)",
                        magic.research_remaining
                    ),
                    WHITE,
                );
                lines.gap(10.0);
                lines.line("The choice stands until learned.", GRAY);
                return;
            }
            None => lines.line("Choose a spell to research:", WHITE),
        }
        let choices = self.game.research_choices(HUMAN);
        let top = lines.y;
        let rows = ((height - top - 50.0) / ROW_H) as usize;
        self.scroll_list(0, x, top, width, choices.len(), rows);
        let first = self.magic_scroll[0];
        for (i, &spell) in choices.iter().skip(first).take(rows).enumerate() {
            let def = self.game.content.rules.spells.get(spell);
            let bonus = self.game.research_bonus(HUMAN, spell);
            let cost = def.research_cost as i32 * 100 / (100 + bonus);
            let r = Rect::new(x, top + i as f32 * ROW_H, width, ROW_H - 2.0);
            let hover = r.contains(mouse_position().into());
            if hover {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.3, 0.22, 0.12, 1.0));
            }
            let label = format!(
                "{} ({}, {}) {}",
                def.name,
                def.realm,
                rarity_name(def.rarity),
                cost
            );
            draw_text(
                &label,
                r.x + 4.0,
                r.y + 17.0,
                18.0,
                self.realm_color(&def.realm),
            );
            if hover && is_mouse_button_pressed(MouseButton::Left) {
                let events = self.game.apply(Command::SetResearch {
                    player: HUMAN,
                    spell,
                });
                self.note(events);
            }
        }
    }

    fn magic_spell_column(&mut self, x: f32, width: f32, height: f32) {
        let mut lines = Lines::new(x, 40.0);
        lines.line("Spell book", GOLD);
        let magic = self.game.magic(HUMAN).cloned().unwrap_or_default();
        let mana = self.game.player(HUMAN).map_or(0, |p| p.mana);
        let mut known: Vec<SpellId> = magic.known.iter().copied().collect();
        known.sort_by_key(|&s| {
            let d = self.game.content.rules.spells.get(s);
            (d.realm.clone(), d.cost)
        });
        let castable: Vec<SpellId> = known
            .iter()
            .copied()
            .filter(|&s| {
                self.game
                    .content
                    .rules
                    .spells
                    .get(s)
                    .contexts
                    .contains(&CastContext::Overland)
            })
            .collect();
        lines.line(
            &format!(
                "{} known, {} castable on the map (click to cast)",
                known.len(),
                castable.len()
            ),
            WHITE,
        );
        let top = lines.y;
        let rows = ((height - top - 50.0) / ROW_H) as usize;
        self.scroll_list(1, x, top, width, castable.len(), rows);
        let first = self.magic_scroll[1];
        for (i, &spell) in castable.iter().skip(first).take(rows).enumerate() {
            let def = self.game.content.rules.spells.get(spell);
            let cost = self.game.casting_cost(HUMAN, spell, false);
            let r = Rect::new(x, top + i as f32 * ROW_H, width, ROW_H - 2.0);
            let hover = r.contains(mouse_position().into());
            let affordable = magic.casting.is_none() && mana > 0;
            if hover {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.3, 0.22, 0.12, 1.0));
            }
            let upkeep = if def.upkeep > 0 {
                format!("  upkeep {}", def.upkeep)
            } else {
                String::new()
            };
            let color = if affordable {
                self.realm_color(&def.realm)
            } else {
                GRAY
            };
            draw_text(
                format!("{}  {cost}{upkeep}", def.name),
                r.x + 4.0,
                r.y + 17.0,
                18.0,
                color,
            );
            if hover && affordable && is_mouse_button_pressed(MouseButton::Left) {
                self.begin_cast(spell);
            }
        }
    }

    /// Mouse-wheel scrolling for one of the two lists.
    fn scroll_list(&mut self, which: usize, x: f32, top: f32, width: f32, len: usize, rows: usize) {
        let area = Rect::new(x, top, width, rows as f32 * ROW_H);
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 && area.contains(mouse_position().into()) {
            let step = 3 * wheel.signum() as i32;
            self.magic_scroll[which] = (self.magic_scroll[which] as i32 - step)
                .clamp(0, len.saturating_sub(rows) as i32)
                as usize;
        }
        self.magic_scroll[which] = self.magic_scroll[which].min(len.saturating_sub(rows));
    }

    /// Start casting: a spell without a target goes straight to the game, one with a target
    /// asks for it on the map.
    fn begin_cast(&mut self, spell: SpellId) {
        let needs = self.game.spell_needs_target(HUMAN, spell);
        let cost = self.game.casting_cost(HUMAN, spell, false);
        let instant = self.game.magic(HUMAN).map_or(0, |m| m.skill_left);
        let mana = self.game.player(HUMAN).map_or(0, |p| p.mana);
        if needs && cost <= instant.min(mana) {
            self.targeting = Some(Targeting {
                spell,
                resolve: false,
            });
            self.screen = Screen::Map;
            return;
        }
        let events = self.game.apply(Command::CastSpell {
            player: HUMAN,
            spell,
            target: None,
        });
        self.note(events);
    }

    /// The player clicked `pos` while choosing a target.
    pub(crate) fn aim_at(&mut self, t: Targeting, pos: f4x_core::city::Position) {
        let def = self.game.content.rules.spells.get(t.spell);
        let target = match def.target {
            SpellTarget::OwnUnit | SpellTarget::EnemyUnit | SpellTarget::AnyUnit => {
                let units = self.game.units_at(pos);
                let wanted = units.into_iter().find(|&u| {
                    let mine = self.game.unit_owner(u) == Some(HUMAN);
                    match def.target {
                        SpellTarget::OwnUnit => mine,
                        SpellTarget::EnemyUnit => !mine,
                        _ => true,
                    }
                });
                wanted.map(Target::Unit)
            }
            SpellTarget::OwnCity | SpellTarget::EnemyCity => {
                self.game.city_at(pos).map(Target::City)
            }
            _ => Some(Target::Tile(pos)),
        };
        let Some(target) = target else {
            self.log.push("No valid target there".into());
            self.targeting = Some(t);
            return;
        };
        let cmd = if t.resolve {
            Command::ResolveCast {
                player: HUMAN,
                target,
            }
        } else {
            Command::CastSpell {
                player: HUMAN,
                spell: t.spell,
                target: Some(target),
            }
        };
        let events = self.game.apply(cmd);
        let rejected = events
            .iter()
            .any(|e| matches!(e, f4x_core::Event::Rejected(_)));
        self.note(events);
        if rejected {
            self.targeting = Some(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rebalance;

    #[test]
    fn rebalancing_keeps_the_total_at_one_hundred() {
        assert_eq!(rebalance([33, 34, 33], 0, 50), [50, 25, 25]);
        assert_eq!(rebalance([100, 0, 0], 1, 40), [60, 40, 0]);
        assert_eq!(rebalance([20, 80, 0], 2, 100), [0, 0, 100]);
        for new in 0..=100u8 {
            let s = rebalance([10, 30, 60], 1, new);
            assert_eq!(s.iter().map(|&v| u32::from(v)).sum::<u32>(), 100);
        }
    }
}
