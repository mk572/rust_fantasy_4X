//! Override commands: cheating. They run only in override mode, and each use is written to
//! the game's saved override log.

use f4x_core::city::{PlayerId, Position};
use f4x_core::map::Plane;
use f4x_core::Command;

use crate::args::{city_by_serial, parse_player, parse_pos, parse_units, resolve, Args};
use crate::describe;
use crate::{CommandDef, ConsoleError, Ctx, Kind};

macro_rules! cheat {
    ($name:literal, $usage:literal, $help:literal, $run:ident) => {
        CommandDef {
            name: $name,
            kind: Kind::Override,
            usage: $usage,
            help: $help,
            run: $run,
        }
    };
}

pub const TABLE: &[CommandDef] = &[
    cheat!("give", "give <gold|mana|fame> <amount> [player]", "Add (or, negative, remove) gold, mana or fame.", give),
    cheat!("spawn", "spawn <unit> <x,y[,plane]> [player]", "Create a unit at no cost.", spawn),
    cheat!("kill", "kill <units>", "Remove units from the game.", kill),
    cheat!("restore", "restore <units>", "Heal units fully and refill their moves.", restore),
    cheat!("undead", "undead <units>", "Make units undead: Death creatures that are not healed, cost no gold or food, and are immune to death, cold, poison and illusions.", undead),
    cheat!("teleport", "teleport <units> <x,y[,plane]>", "Put units on a square, whatever is there.", teleport),
    cheat!("fortress", "fortress <x,y[,plane]> [name] [race=<id>] [player=<n>]", "Place a wizard's first city with the fortress and summoning circle. A game-start action: the game refuses it once the wizard has a fortress, is banished or is defeated.", fortress),
    cheat!("newcity", "newcity <x,y[,plane]> <name> [player] [race=<id>] [pop=<n>]", "Found a city anywhere the map allows, at no cost.", newcity),
    cheat!("build", "build <city> <building>", "Give a city a building, ignoring its requirements and cost.", build),
    cheat!("terrain", "terrain <x,y[,plane]> <terrain-id>", "Change a square's terrain.", terrain),
    cheat!("learn", "learn <spell> [player]", "Teach a wizard a spell.", learn),
    cheat!("reveal", "reveal [player]", "Explore the whole map for a player.", reveal),
    cheat!("eliminate", "eliminate <player>", "Defeat a wizard on the spot (their cities turn neutral).", eliminate),
    cheat!("win", "win <player>", "End the game with this wizard winning by the Spell of Mastery.", win),
    cheat!("as", "as <player>", "Play as another wizard from now on.", act_as),
];

fn player_arg(ctx: &Ctx, a: &Args, i: usize) -> Result<PlayerId, ConsoleError> {
    let id = match a.get(i) {
        Some(p) => parse_player(p)?,
        None => ctx.session.player,
    };
    ctx.game
        .player(id)
        .map(|p| p.id)
        .ok_or_else(|| ConsoleError::NoSuch(format!("player {}", id.0)))
}

fn give(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let what = a.need(0, "gold, mana or fame")?;
    let amount = a.int(1, "amount")? as i32;
    let id = player_arg(ctx, a, 2)?;
    let p = ctx.game.player_mut(id).expect("checked");
    let now = match what {
        "gold" => {
            p.gold += amount;
            p.gold
        }
        "mana" => {
            p.mana += amount;
            p.mana
        }
        "fame" => {
            p.fame += amount;
            p.fame
        }
        other => {
            return Err(ConsoleError::Usage(format!(
                "give gold, mana or fame, not {other:?}"
            )))
        }
    };
    ctx.say(format!(
        "{} now has {now} {what}",
        describe::player(ctx.game, id)
    ));
    Ok(())
}

fn spawn(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let unit = resolve(
        &ctx.game.content().rules.units,
        a.need(0, "unit")?,
        |d| &d.name,
        "unit",
    )?;
    let pos = parse_pos(a.need(1, "position")?, Plane::Arcanus)?;
    let owner = player_arg(ctx, a, 2)?;
    ctx.apply(Command::CreateUnit { owner, pos, unit })
}

fn kill(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = parse_units(ctx.game, a.need(0, "units")?)?;
    for u in units {
        let line = describe::unit(ctx.game, u);
        ctx.game.cheat_remove_unit(u);
        ctx.say(format!("removed {line}"));
    }
    Ok(())
}

fn restore(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    for u in parse_units(ctx.game, a.need(0, "units")?)? {
        ctx.game.cheat_restore_unit(u);
        let line = describe::unit(ctx.game, u);
        ctx.say(format!("restored {line}"));
    }
    Ok(())
}

fn undead(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    for u in parse_units(ctx.game, a.need(0, "units")?)? {
        ctx.game.make_undead(u);
        let line = describe::unit(ctx.game, u);
        ctx.say(format!("made undead: {line}"));
    }
    Ok(())
}

fn teleport(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = parse_units(ctx.game, a.need(0, "units")?)?;
    let first = units
        .first()
        .and_then(|&u| ctx.game.unit_position(u))
        .map_or(Plane::Arcanus, |p| p.plane);
    let to = parse_pos(a.need(1, "position")?, first)?;
    for u in units {
        ctx.game.cheat_move_unit(u, to);
    }
    ctx.explore_for_units(to);
    ctx.say(format!("moved to {}", describe::pos(to)));
    Ok(())
}

impl Ctx<'_> {
    fn explore_for_units(&mut self, at: Position) {
        let me = self.session.player;
        self.game.explore(me, at, 2);
    }
}

fn newcity(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let (plain, opts) = a.split_options();
    let pos = parse_pos(
        plain
            .first()
            .ok_or(ConsoleError::Usage("missing position".into()))?,
        Plane::Arcanus,
    )?;
    let name = plain
        .get(1)
        .ok_or(ConsoleError::Usage("missing name".into()))?
        .to_string();
    let mut owner = ctx.session.player;
    let (mut race, mut population) = (None, 3);
    for (k, v) in opts {
        match k {
            "race" => {
                race = Some(
                    ctx.game
                        .content()
                        .rules
                        .races
                        .id_of(v)
                        .ok_or_else(|| ConsoleError::NoSuch(format!("race {v:?}")))?,
                )
            }
            "pop" => {
                population = v
                    .parse()
                    .map_err(|_| ConsoleError::Parse(format!("{v:?} is not a size")))?
            }
            other => return Err(ConsoleError::Usage(format!("unknown option {other:?}"))),
        }
    }
    if let Some(p) = plain.get(2) {
        owner = parse_player(p)?;
    }
    if race.is_none() {
        race = ctx
            .game
            .content()
            .rules
            .races
            .defs()
            .first()
            .map(|r| r.id.clone())
            .and_then(|id| ctx.game.content().rules.races.id_of(&id));
    }
    ctx.apply(Command::FoundCity {
        owner,
        pos,
        name,
        race,
        population,
    })
}

fn build(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let city = city_by_serial(ctx.game, a.int(0, "city number")? as u64)?;
    let b = resolve(
        &ctx.game.content().rules.buildings,
        &a.rest(1),
        |d| &d.name,
        "building",
    )?;
    if !ctx.game.cheat_add_building(city, b) {
        return Err(ConsoleError::Failed("could not add the building".into()));
    }
    ctx.say(format!(
        "{} now has {}",
        describe::city_name(ctx.game, city),
        ctx.game.content().rules.buildings.get(b).name
    ));
    Ok(())
}

fn terrain(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let pos = parse_pos(a.need(0, "position")?, Plane::Arcanus)?;
    let name = a.need(1, "terrain id")?;
    let id = ctx
        .game
        .content()
        .terrains
        .id_of(name)
        .ok_or_else(|| ConsoleError::NoSuch(format!("terrain {name:?}")))?;
    if !ctx.game.cheat_set_terrain(pos, id) {
        return Err(ConsoleError::Failed("that square is off the map".into()));
    }
    ctx.say(format!("{} is now {name}", describe::pos(pos)));
    Ok(())
}

fn learn(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let (plain, _) = a.split_options();
    let (spell_text, who) = match plain.split_last() {
        Some((last, rest)) if !rest.is_empty() && last.parse::<u8>().is_ok() => {
            (rest.join(" "), Some(*last))
        }
        _ => (plain.join(" "), None),
    };
    let spell = resolve(
        &ctx.game.content().rules.spells,
        &spell_text,
        |d| &d.name,
        "spell",
    )?;
    let id = match who {
        Some(p) => parse_player(p)?,
        None => ctx.session.player,
    };
    if !ctx.game.cheat_learn_spell(id, spell) {
        return Err(ConsoleError::NoSuch(format!("wizard {}", id.0)));
    }
    ctx.say(format!(
        "{} learned {}",
        describe::player(ctx.game, id),
        describe::spell(ctx.game, spell)
    ));
    Ok(())
}

fn reveal(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = player_arg(ctx, a, 0)?;
    ctx.game.cheat_reveal(id);
    ctx.say("map revealed");
    Ok(())
}

fn eliminate(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = parse_player(a.need(0, "player")?)?;
    let by = ctx.session.player;
    let events = ctx.game.cheat_eliminate(id, by);
    if events.is_empty() {
        return Err(ConsoleError::Refused("no such active wizard".into()));
    }
    for e in &events {
        let line = describe::event(ctx.game, e);
        ctx.say(line);
    }
    ctx.events.extend(events);
    Ok(())
}

fn win(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = parse_player(a.need(0, "player")?)?;
    let events = ctx.game.win_by_mastery(id);
    if events.is_empty() {
        return Err(ConsoleError::Refused("the game is already over".into()));
    }
    for e in &events {
        let line = describe::event(ctx.game, e);
        ctx.say(line);
    }
    ctx.events.extend(events);
    Ok(())
}

fn act_as(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = player_arg(ctx, a, 0)?;
    ctx.session.player = id;
    ctx.say(format!("now playing {}", describe::player(ctx.game, id)));
    Ok(())
}

fn fortress(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let (plain, opts) = a.split_options();
    let pos = parse_pos(
        plain
            .first()
            .ok_or(ConsoleError::Usage("missing position".into()))?,
        Plane::Arcanus,
    )?;
    let rules = &ctx.game.content().rules;
    let race_def = match opts.iter().find(|(k, _)| *k == "race") {
        Some((_, id)) => rules
            .races
            .by_name(id)
            .ok_or_else(|| ConsoleError::NoSuch(format!("race {id:?}")))?,
        None => rules
            .races
            .defs()
            .iter()
            .find(|r| !r.city_names.is_empty())
            .ok_or(ConsoleError::NoSuch("race".into()))?,
    };
    let name = if plain.len() > 1 {
        plain[1..].join(" ")
    } else {
        race_def
            .city_names
            .first()
            .cloned()
            .unwrap_or_else(|| "Capital".into())
    };
    let race = rules.races.id_of(&race_def.id);
    let player = match opts.iter().find(|(k, _)| *k == "player") {
        Some((_, p)) => parse_player(p)?,
        None => ctx.session.player,
    };
    ctx.apply(Command::PlaceFortress {
        player,
        pos,
        name,
        race,
        population: 3,
    })
}
