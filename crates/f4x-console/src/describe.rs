//! Words for things: units, cities, positions and events, as the console prints them.

use f4x_core::city::{PlayerId, Position};
use f4x_core::map::Plane;
use f4x_core::production::ProductionItem;
use f4x_core::{Event, Game};

pub fn plane_letter(p: Plane) -> char {
    match p {
        Plane::Arcanus => 'a',
        Plane::Myrror => 'm',
    }
}

pub fn pos(p: Position) -> String {
    format!("{},{},{}", p.x, p.y, plane_letter(p.plane))
}

pub fn player(game: &Game, id: PlayerId) -> String {
    match game.player(id) {
        Some(p) => format!("{} ({})", p.name, id.0),
        None => format!("player {}", id.0),
    }
}

pub fn unit(game: &Game, u: hecs::Entity) -> String {
    let name = game.unit_def(u).map_or("?", |d| d.name.as_str());
    let at = game.unit_position(u).map_or(String::new(), |p| format!(" at {}", pos(p)));
    format!("unit {} {name}{at}", game.serial_of(u).0)
}

pub fn city_name(game: &Game, c: hecs::Entity) -> String {
    let name = game
        .cities()
        .into_iter()
        .find(|(e, ..)| *e == c)
        .map_or("?".to_string(), |(_, c, _)| c.name);
    format!("city {} {name}", game.serial_of(c).0)
}

pub fn item(game: &Game, item: ProductionItem) -> String {
    let rules = &game.content.rules;
    match item {
        ProductionItem::Building(b) => rules.buildings.get(b).name.clone(),
        ProductionItem::Unit(u) => rules.units.get(u).name.clone(),
        ProductionItem::TradeGoods => "Trade Goods".into(),
        ProductionItem::Housing => "Housing".into(),
    }
}

pub fn spell(game: &Game, s: f4x_core::rules::SpellId) -> String {
    game.content.rules.spells.get(s).name.clone()
}

fn units(game: &Game, list: &[hecs::Entity]) -> String {
    list.iter()
        .map(|&u| game.serial_of(u).0.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// One line for an event.
pub fn event(game: &Game, e: &Event) -> String {
    match e {
        Event::TurnStarted { turn } => format!("Turn {turn} begins"),
        Event::CityFounded { city, pos: p } => {
            format!("Founded {} at {}", city_name(game, *city), pos(*p))
        }
        Event::ProductionCompleted { city, item: i } => {
            format!("{} finished {}", city_name(game, *city), item(game, *i))
        }
        Event::CityGrew { city, figures } => {
            format!("{} grew to {figures}", city_name(game, *city))
        }
        Event::CityShrank { city, figures } => {
            format!("{} shrank to {figures}", city_name(game, *city))
        }
        Event::UnitsDisbanded { player: p, units } => {
            format!("{} disbanded {units} units", player(game, *p))
        }
        Event::FortressPlaced { player: p, pos: at } => {
            format!("{} placed a fortress at {}", player(game, *p), pos(*at))
        }
        Event::WizardBanished { player: p, by } => {
            format!("{} is banished by {}", player(game, *p), player(game, *by))
        }
        Event::WizardReturned { player: p, .. } => format!("{} has returned", player(game, *p)),
        Event::WizardDefeated { player: p } => format!("{} is defeated", player(game, *p)),
        Event::GameOver { winner, kind, turn } => format!(
            "GAME OVER: {} wins by {kind:?} on turn {turn}",
            player(game, *winner)
        ),
        Event::CityCaptured { city, from, to } => format!(
            "{} taken from {} by {}",
            city_name(game, *city),
            player(game, *from),
            player(game, *to)
        ),
        Event::SpellLearned { player: p, spell: s } => {
            format!("{} learned {}", player(game, *p), spell(game, *s))
        }
        Event::SpellCast { player: p, spell: s } => {
            format!("{} cast {}", player(game, *p), spell(game, *s))
        }
        Event::SpellReady { spell: s, .. } => {
            format!("{} is paid for: choose a target with `target`", spell(game, *s))
        }
        Event::SpellHasNoEffectYet { spell: s } => {
            format!("{} has no effect yet", spell(game, *s))
        }
        Event::BattleStarted { attacker, defender, site } => format!(
            "Battle at {}: {} attacks {}",
            pos(*site),
            player(game, *attacker),
            player(game, *defender)
        ),
        Event::BattleEnded { winner, loser } => format!(
            "Battle over: {} beats {}",
            player(game, *winner),
            player(game, *loser)
        ),
        Event::UnitCreated { unit: u, .. } => format!("Created {}", unit(game, *u)),
        Event::UnitsMoved { units: us, from, to } => {
            format!("Units {} moved {} -> {}", units(game, us), pos(*from), pos(*to))
        }
        Event::AttackRequested { attackers, to, defenders, .. } => format!(
            "Units {} attack {} (defenders {})",
            units(game, attackers),
            pos(*to),
            units(game, defenders)
        ),
        Event::SiteEncounter { units: us, to, .. } => {
            format!("Units {} reach a guarded site at {}", units(game, us), pos(*to))
        }
        Event::SiteCleared { player: p, pos: at, loot } => {
            format!("{} cleared the site at {} ({loot:?})", player(game, *p), pos(*at))
        }
        Event::Rejected(why) => format!("Refused: {why:?}"),
        other => format!("{other:?}"),
    }
}
