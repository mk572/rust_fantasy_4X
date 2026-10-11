//! Splitting a line into words and reading the things commands take: positions, unit
//! lists, targets and definitions by id or name.

use f4x_core::city::{PlayerId, Position};
use f4x_core::map::Plane;
use f4x_core::rules::exec::Target;
use f4x_core::rules::ids::Id;
use f4x_core::rules::{Def, Registry};
use f4x_core::serial::Serial;
use f4x_core::Game;

use crate::ConsoleError;

/// Split a line into words; double quotes keep spaces together.
pub fn tokenize(line: &str) -> Result<Vec<String>, ConsoleError> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    words.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if quoted {
        return Err(ConsoleError::Parse("unclosed quote".into()));
    }
    if any {
        words.push(cur);
    }
    Ok(words)
}

/// The words after the command name.
pub struct Args<'a> {
    pub words: &'a [String],
}

impl<'a> Args<'a> {
    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&'a str> {
        self.words.get(i).map(String::as_str)
    }

    pub fn need(&self, i: usize, what: &str) -> Result<&'a str, ConsoleError> {
        self.get(i)
            .ok_or_else(|| ConsoleError::Usage(format!("missing {what}")))
    }

    pub fn int(&self, i: usize, what: &str) -> Result<i64, ConsoleError> {
        let w = self.need(i, what)?;
        w.parse()
            .map_err(|_| ConsoleError::Parse(format!("{what}: {w:?} is not a number")))
    }

    /// Words from `i` on, joined with spaces (a name).
    pub fn rest(&self, i: usize) -> String {
        self.words.get(i..).unwrap_or(&[]).join(" ")
    }

    /// `key=value` words (anywhere) as pairs; the others are returned in order.
    pub fn split_options(&self) -> (Vec<&'a str>, Vec<(&'a str, &'a str)>) {
        let (mut plain, mut opts) = (Vec::new(), Vec::new());
        for w in self.words {
            match w.split_once('=') {
                Some((k, v)) if !k.is_empty() => opts.push((k, v)),
                _ => plain.push(w.as_str()),
            }
        }
        (plain, opts)
    }
}

pub fn parse_plane(text: &str) -> Option<Plane> {
    match text.to_lowercase().as_str() {
        "a" | "arcanus" | "0" => Some(Plane::Arcanus),
        "m" | "myrror" | "1" => Some(Plane::Myrror),
        _ => None,
    }
}

/// `x,y` or `x,y,plane` (`a` or `m`); `default` supplies a missing plane.
pub fn parse_pos(text: &str, default: Plane) -> Result<Position, ConsoleError> {
    let bad = || ConsoleError::Parse(format!("{text:?} is not a position (x,y or x,y,a|m)"));
    let parts: Vec<&str> = text.split(',').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(bad());
    }
    let x = parts[0].parse().map_err(|_| bad())?;
    let y = parts[1].parse().map_err(|_| bad())?;
    let plane = match parts.get(2) {
        Some(p) => parse_plane(p).ok_or_else(bad)?,
        None => default,
    };
    Ok(Position { plane, x, y })
}

pub fn parse_player(text: &str) -> Result<PlayerId, ConsoleError> {
    text.parse::<u8>()
        .map(PlayerId)
        .map_err(|_| ConsoleError::Parse(format!("{text:?} is not a player number")))
}

pub fn unit_by_serial(game: &Game, serial: u64) -> Result<hecs::Entity, ConsoleError> {
    game.entity_of_serial(Serial(serial))
        .filter(|&e| game.unit_position(e).is_some())
        .ok_or_else(|| ConsoleError::NoSuch(format!("unit {serial}")))
}

pub fn city_by_serial(game: &Game, serial: u64) -> Result<hecs::Entity, ConsoleError> {
    game.entity_of_serial(Serial(serial))
        .filter(|&e| game.city_owner(e).is_some())
        .ok_or_else(|| ConsoleError::NoSuch(format!("city {serial}")))
}

/// A unit list: creation numbers separated by commas (`12,13`), or `@x,y[,plane]` for every
/// unit on that square (the owner is checked by the caller).
pub fn parse_units(game: &Game, text: &str) -> Result<Vec<hecs::Entity>, ConsoleError> {
    if let Some(at) = text.strip_prefix('@') {
        let pos = parse_pos(at, Plane::Arcanus)?;
        let mut units = game.units_at(pos);
        game.sort_by_serial(&mut units);
        if units.is_empty() {
            return Err(ConsoleError::NoSuch(format!("units at {at}")));
        }
        return Ok(units);
    }
    let mut units = Vec::new();
    for part in text.split(',') {
        let n: u64 = part
            .parse()
            .map_err(|_| ConsoleError::Parse(format!("{part:?} is not a unit number")))?;
        units.push(unit_by_serial(game, n)?);
    }
    Ok(units)
}

/// A spell target: `unit:12`, `city:3`, `ward:3:chaos` (a Spell Ward's city and realm),
/// `tile:x,y[,plane]` or `player:0`.
pub fn parse_target(game: &Game, text: &str) -> Result<Target, ConsoleError> {
    let (kind, rest) = text.split_once(':').ok_or_else(|| {
        ConsoleError::Parse(
            "a target is unit:N, city:N, ward:N:<realm>, tile:x,y, player:N or global:N:<spell>"
                .into(),
        )
    })?;
    let num = || {
        rest.parse::<u64>()
            .map_err(|_| ConsoleError::Parse(format!("{rest:?} is not a number")))
    };
    Ok(match kind {
        "unit" => Target::Unit(unit_by_serial(game, num()?)?),
        "city" => Target::City(city_by_serial(game, num()?)?),
        // A Spell Ward's target: the city and the realm it is set against, as `ward:3:chaos`.
        "ward" => {
            let (city, name) = rest.split_once(':').ok_or_else(|| {
                ConsoleError::Parse("a ward target is ward:<city>:<realm>".into())
            })?;
            let city = city_by_serial(
                game,
                city.parse::<u64>()
                    .map_err(|_| ConsoleError::Parse(format!("{city:?} is not a number")))?,
            )?;
            let realm = resolve(&game.content.rules.realms, name, |d| &d.name, "realm")?;
            Target::CityRealm(city, realm)
        }
        "tile" => Target::Tile(parse_pos(rest, Plane::Arcanus)?),
        "player" => Target::Player(parse_player(rest)?),
        // An enemy wizard's global enchantment: the wizard and the spell, as `global:1:awareness`.
        "global" => {
            let (who, name) = rest.split_once(':').ok_or_else(|| {
                ConsoleError::Parse("a global target is global:<player>:<spell>".into())
            })?;
            let spell = resolve(&game.content.rules.spells, name, |d| &d.name, "spell")?;
            Target::Global(parse_player(who)?, spell)
        }
        other => {
            return Err(ConsoleError::Parse(format!(
                "unknown target kind {other:?}"
            )))
        }
    })
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Find a definition by its id, its display name (spaces, case and punctuation ignored) or a
/// unique prefix of its id.
pub fn resolve<T: Def>(
    reg: &Registry<T>,
    text: &str,
    name_of: impl Fn(&T) -> &str,
    what: &str,
) -> Result<Id<T>, ConsoleError> {
    if let Some(id) = reg.id_of(text) {
        return Ok(id);
    }
    let key = squash(text);
    if let Some((id, _)) = reg
        .iter()
        .find(|(_, d)| squash(name_of(d)) == key || squash(d.id()) == key)
    {
        return Ok(id);
    }
    let hits: Vec<_> = reg
        .iter()
        .filter(|(_, d)| d.id().starts_with(text))
        .collect();
    match hits.as_slice() {
        [(id, _)] => Ok(*id),
        [] => Err(ConsoleError::NoSuch(format!("{what} {text:?}"))),
        many => Err(ConsoleError::Parse(format!(
            "{what} {text:?} is ambiguous: {}",
            many.iter()
                .take(6)
                .map(|(_, d)| d.id())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_on_spaces_and_quotes() {
        assert_eq!(
            tokenize(r#"found 12 "New York" x"#).unwrap(),
            ["found", "12", "New York", "x"]
        );
        assert!(tokenize(r#"say "oops"#).is_err());
        assert!(tokenize("   ").unwrap().is_empty());
    }

    #[test]
    fn positions_take_an_optional_plane() {
        let p = parse_pos("4,5,m", Plane::Arcanus).unwrap();
        assert_eq!((p.x, p.y, p.plane), (4, 5, Plane::Myrror));
        assert_eq!(
            parse_pos("4,5", Plane::Myrror).unwrap().plane,
            Plane::Myrror
        );
        assert!(parse_pos("4", Plane::Arcanus).is_err());
        assert!(parse_pos("a,b", Plane::Arcanus).is_err());
    }
}
