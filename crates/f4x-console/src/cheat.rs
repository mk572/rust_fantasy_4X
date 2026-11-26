//! Override commands: cheating. They run only in override mode, and each use is written to
//! the game's saved override log.

use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;

use f4x_core::world::Command;

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
    cheat!("give", "give <gold|mana|fame> <amount> [player] | give retort <retort> [player] | give spell <spell> [player]", "Add (or, negative, remove) gold, mana or fame; or give a wizard a retort (the casting skill follows) or a spell.", give),
    cheat!("remove", "remove retort <retort> [player] | remove spell <spell> [player]", "Take a retort (the casting skill follows) or a known spell away from a wizard.", remove),
    cheat!("set", "set books <realm> <n> [player]", "Set the books a wizard has in a realm (none takes them away; at most the game's most for a realm); the casting skill follows.", set),
    cheat!("spawn", "spawn <unit> <x,y[,plane]> [player]", "Create a unit at no cost.", spawn),
    cheat!("kill", "kill <units>", "Remove units from the game.", kill),
    cheat!("restore", "restore <units>", "Heal units fully and refill their moves.", restore),
    cheat!("undead", "undead <units>", "Make units undead: Death creatures that are not healed, cost no gold or food, and are immune to death, cold, poison and illusions.", undead),
    cheat!("teleport", "teleport <units> <x,y[,plane]>", "Put units on a square, whatever is there.", teleport),
    cheat!("fortress", "fortress <x,y[,plane]> [name] [race=<id>] [player=<n>]", "Place a wizard's first city with the fortress and summoning circle. A game-start action: the game refuses it once the wizard has a fortress, is banished or is defeated.", fortress),
    cheat!("newcity", "newcity <x,y[,plane]> <name> [player] [race=<id>] [pop=<n>]", "Found a city anywhere the map allows, at no cost.", newcity),
    cheat!("build", "build <city> <building>", "Give a city a building, ignoring its requirements and cost.", build),
    cheat!("terrain", "terrain <x,y[,plane]> <terrain-id>", "Change a square's terrain.", terrain),
    cheat!("lair", "lair <feature> <x,y[,plane]> [strong]", "Put an encounter site or a tower on a square with guards and treasure made as a new world makes them (a strong or, by default, a weak site), replacing what stands there.", lair),
    cheat!("unlair", "unlair <x,y[,plane]>", "Remove the site with a lair from a square, guards and treasure with it.", unlair),
    cheat!("feature", "feature <x,y[,plane]> <feature|none>", "Put a terrain feature (a mineral, a tower, a node, an encounter site, with no guards, treasure or aura) on a square, or with `none` take the feature off.", feature),
    cheat!("hire", "hire <hero> [player] [x,y[,plane]]", "Hire a hero for nothing: it appears on any square of the map you give, else on the wizard's fortress (or in a city). The hero must be free or dead.", hire),
    cheat!("dismiss", "dismiss <hero>", "Let a hero in service go; it is free to be hired again and its items go to the armory.", dismiss),
    cheat!("reroll", "reroll <hero>", "Roll a hero's abilities and spells anew; its name, level and items stay.", reroll),
    cheat!("treasury", "treasury [player]", "Show a wizard's gold, mana and fame, and their income.", treasury),
    cheat!("enchant", "enchant <units> <spell> [player]", "Put a lasting spell on units at no cost, as if that wizard had cast it.", enchant),
    cheat!("citizens", "citizens <city> <n>", "Set a city's citizens.", citizens),
    cheat!("learn", "learn <spell> [player]", "Teach a wizard a spell.", learn),
    cheat!("reveal", "reveal [player]", "Explore the whole map for a player.", reveal),
    cheat!("contact", "contact [player]", "Meet a wizard (or, with no argument, every wizard) as the acting player, as if their cities or units had come into view.", contact),
    cheat!("event", "event <name> [player]", "Make a random event happen to a wizard now (great_meteor, gift, plague, good_moon, ...); the vetoes apply, except the wait until turn 150.", event),
    cheat!("offer", "offer <hero|mercenaries|item> [player]", "Make a wizard the offer now, whatever the chance (it still needs a free slot, turn 30 or gold).", offer),
    cheat!("eliminate", "eliminate <player>", "Defeat a wizard on the spot (their cities turn neutral).", eliminate),
    cheat!("win", "win <player>", "End the game with this wizard winning by the Spell of Mastery.", win),
    cheat!("as", "as <player>", "Play as another wizard from now on.", act_as),
];

fn player_arg(ctx: &Ctx, a: &Args, i: usize) -> Result<PlayerId, ConsoleError> {
    let id = match a.get(i) {
        Some(p) => parse_player(p)?,
        None => ctx.session.player,
    };
    ctx.world
        .player_entity(id)
        .map(|_| id)
        .ok_or_else(|| ConsoleError::NoSuch(format!("player {}", id.0)))
}

/// The words after the keyword as a name and, when the last of several is a number, the wizard.
fn name_and_wizard<'a>(
    ctx: &Ctx,
    words: &'a [&'a str],
) -> Result<(String, PlayerId), ConsoleError> {
    match words.split_last() {
        Some((last, rest)) if !rest.is_empty() && last.parse::<u8>().is_ok() => {
            Ok((rest.join(" "), parse_player(last)?))
        }
        _ => Ok((words.join(" "), ctx.session.player)),
    }
}

fn spell_of(ctx: &Ctx, name: &str) -> Result<f4x_core::rules::SpellId, ConsoleError> {
    resolve(&ctx.world.content().rules.spells, name, |d| &d.id, "spell")
}

fn retort_of(ctx: &Ctx, name: &str) -> Result<f4x_core::wizard::magic::Retort, ConsoleError> {
    crate::setup::parse_retort(&ctx.world.content().rules, name)
        .ok_or_else(|| ConsoleError::NoSuch(format!("retort {name:?}")))
}

fn give_kept(ctx: &mut Ctx, a: &Args, what: &str) -> Result<(), ConsoleError> {
    let (plain, _) = a.split_options();
    let (name, id) = name_and_wizard(ctx, plain.get(1..).unwrap_or(&[]))?;
    if what == "spell" {
        let spell = spell_of(ctx, &name)?;
        if !ctx.world.cheat_learn_spell(id, spell) {
            return Err(ConsoleError::NoSuch(format!("wizard {}", id.0)));
        }
        ctx.say(format!(
            "{} learned {}",
            describe::player(ctx.world, id),
            describe::spell(ctx.world, spell)
        ));
    } else {
        let retort = retort_of(ctx, &name)?;
        if !ctx.world.cheat_set_retort(id, retort, true) {
            return Err(ConsoleError::Failed(format!(
                "wizard {} has no sheet, or has {name:?} already",
                id.0
            )));
        }
        ctx.say(format!(
            "{} now has the retort {name}",
            describe::player(ctx.world, id)
        ));
    }
    Ok(())
}

fn remove(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let what = a.need(0, "retort or spell")?;
    let (plain, _) = a.split_options();
    let (name, id) = name_and_wizard(ctx, plain.get(1..).unwrap_or(&[]))?;
    match what {
        "spell" => {
            let spell = spell_of(ctx, &name)?;
            if !ctx.world.cheat_forget_spell(id, spell) {
                return Err(ConsoleError::Failed(format!(
                    "wizard {} does not know {name:?}",
                    id.0
                )));
            }
            ctx.say(format!(
                "{} forgot {}",
                describe::player(ctx.world, id),
                describe::spell(ctx.world, spell)
            ));
        }
        "retort" => {
            let retort = retort_of(ctx, &name)?;
            if !ctx.world.cheat_set_retort(id, retort, false) {
                return Err(ConsoleError::Failed(format!(
                    "wizard {} does not have the retort {name:?}",
                    id.0
                )));
            }
            ctx.say(format!(
                "{} no longer has the retort {name}",
                describe::player(ctx.world, id)
            ));
        }
        other => {
            return Err(ConsoleError::Usage(format!(
                "remove retort or spell, not {other:?}"
            )))
        }
    }
    Ok(())
}

fn set(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    match a.need(0, "books")? {
        "books" => {
            let realm = resolve(
                &ctx.world.content().rules.realms,
                a.need(1, "realm")?,
                |d| &d.id,
                "realm",
            )?;
            let books = u8::try_from(a.int(2, "books")?)
                .map_err(|_| ConsoleError::Parse("books are 0 to 255".into()))?;
            let id = player_arg(ctx, a, 3)?;
            if !ctx.world.cheat_set_books(id, realm, books) {
                return Err(ConsoleError::NoSuch(format!("wizard {}", id.0)));
            }
            let now = ctx.world.wizard(id).map_or(0, |w| w.books_in(realm));
            ctx.say(format!(
                "{} now has {now} books in {}",
                describe::player(ctx.world, id),
                a.need(1, "realm")?
            ));
            Ok(())
        }
        other => Err(ConsoleError::Usage(format!("set books, not {other:?}"))),
    }
}

fn give(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let what = a.need(0, "gold, mana or fame")?;
    if what == "retort" || what == "spell" {
        return give_kept(ctx, a, what);
    }
    let amount = a.int(1, "amount")? as i32;
    let id = player_arg(ctx, a, 2)?;
    let t = ctx.world.treasury_mut(id).expect("checked");
    let now = match what {
        "gold" => {
            t.gold += amount;
            t.gold.to_string()
        }
        "mana" => {
            t.mana += amount;
            t.mana.to_string()
        }
        "fame" => {
            ctx.world.add_fame(id, amount);
            ctx.world.wizard(id).map_or(0, |w| w.fame()).to_string()
        }
        other => {
            return Err(ConsoleError::Usage(format!(
                "give gold, mana or fame, not {other:?}"
            )))
        }
    };
    ctx.say(format!(
        "{} now has {now} {what}",
        describe::player(ctx.world, id)
    ));
    Ok(())
}

fn spawn(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let unit = resolve(
        &ctx.world.content().rules.units,
        a.need(0, "unit")?,
        |d| &d.id,
        "unit",
    )?;
    let pos = parse_pos(a.need(1, "position")?, Plane::Arcanus)?;
    let owner = player_arg(ctx, a, 2)?;
    ctx.apply(Command::CreateUnit { owner, pos, unit })
}

fn kill(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = parse_units(ctx.world, a.need(0, "units")?)?;
    for u in units {
        let line = describe::unit(ctx.world, u);
        ctx.world.cheat_remove_unit(u);
        ctx.say(format!("removed {line}"));
    }
    Ok(())
}

fn restore(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    for u in parse_units(ctx.world, a.need(0, "units")?)? {
        ctx.world.cheat_restore_unit(u);
        let line = describe::unit(ctx.world, u);
        ctx.say(format!("restored {line}"));
    }
    Ok(())
}

fn undead(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    for u in parse_units(ctx.world, a.need(0, "units")?)? {
        ctx.world.make_undead(u);
        let line = describe::unit(ctx.world, u);
        ctx.say(format!("made undead: {line}"));
    }
    Ok(())
}

fn teleport(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = parse_units(ctx.world, a.need(0, "units")?)?;
    let first = units
        .first()
        .and_then(|&u| ctx.world.units().unit_position(u))
        .map_or(Plane::Arcanus, |p| p.plane);
    let to = parse_pos(a.need(1, "position")?, first)?;
    for u in units {
        ctx.world.cheat_move_unit(u, to);
    }
    ctx.explore_for_units(to);
    ctx.say(format!("moved to {}", describe::pos(to)));
    Ok(())
}

impl Ctx<'_> {
    fn explore_for_units(&mut self, at: Position) {
        let me = self.session.player;
        self.world
            .explore(me, at, f4x_def::half_int::HalfInt::from_int(2));
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
                race = Some(resolve(
                    &ctx.world.content().rules.races,
                    v,
                    |d| &d.id,
                    "race",
                )?)
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
            .world
            .content()
            .rules
            .races
            .iter()
            .next()
            .map(|(id, _)| id);
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
    let city = city_by_serial(ctx.world, a.int(0, "city number")? as u64)?;
    let b = resolve(
        &ctx.world.content().rules.buildings,
        &a.rest(1),
        |d| &d.id,
        "building",
    )?;
    if !ctx.world.cheat_add_building(city, b) {
        return Err(ConsoleError::Failed("could not add the building".into()));
    }
    ctx.say(format!(
        "{} now has {}",
        describe::city_name(ctx.world, city),
        ctx.world.content().rules.buildings.get(b).id
    ));
    Ok(())
}

fn terrain(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let pos = parse_pos(a.need(0, "position")?, Plane::Arcanus)?;
    let name = a.need(1, "terrain id")?;
    let terrains = &ctx.world.content().terrains;
    let id = terrains
        .defs()
        .iter()
        .find(|d| d.id == name)
        .and_then(|d| terrains.id_numbered(d.num))
        .ok_or_else(|| ConsoleError::NoSuch(format!("terrain {name:?}")))?;
    if !ctx.world.cheat_set_terrain(pos, id) {
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
        &ctx.world.content().rules.spells,
        &spell_text,
        |d| &d.id,
        "spell",
    )?;
    let id = match who {
        Some(p) => parse_player(p)?,
        None => ctx.session.player,
    };
    if !ctx.world.cheat_learn_spell(id, spell) {
        return Err(ConsoleError::NoSuch(format!("wizard {}", id.0)));
    }
    ctx.say(format!(
        "{} learned {}",
        describe::player(ctx.world, id),
        describe::spell(ctx.world, spell)
    ));
    Ok(())
}

fn reveal(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = player_arg(ctx, a, 0)?;
    ctx.world.cheat_reveal(id);
    ctx.say("map revealed");
    Ok(())
}

fn contact(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    let others = match a.get(0) {
        Some(p) => {
            let id = parse_player(p)?;
            ctx.world
                .player_entity(id)
                .ok_or_else(|| ConsoleError::NoSuch(format!("player {}", id.0)))?;
            vec![id]
        }
        None => ctx
            .world
            .wizards()
            .into_iter()
            .filter(|&w| w != me)
            .collect(),
    };
    let (mut first, mut already, mut refused) = (0, 0, 0);
    for id in others {
        if ctx.world.make_contact(me, id) {
            first += 1;
        } else if ctx.world.have_met(me, id) {
            already += 1;
        } else {
            refused += 1;
        }
    }
    if first + already == 0 {
        return Err(ConsoleError::Refused(
            "no wizard to meet (only wizards that take part in diplomacy can be met)".into(),
        ));
    }
    ctx.say(format!(
        "met {first} wizard(s) for the first time, {already} already met{}",
        if refused > 0 {
            format!(", {refused} cannot be met")
        } else {
            String::new()
        }
    ));
    Ok(())
}

fn eliminate(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = parse_player(a.need(0, "player")?)?;
    let by = ctx.session.player;
    let events = ctx.world.cheat_eliminate(id, by);
    if events.is_empty() {
        return Err(ConsoleError::Refused("no such active wizard".into()));
    }
    for e in &events {
        let line = describe::event(ctx.world, e);
        ctx.say(line);
    }
    ctx.events.extend(events);
    Ok(())
}

fn win(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = parse_player(a.need(0, "player")?)?;
    let events = ctx.world.win_by_mastery(id);
    if events.is_empty() {
        return Err(ConsoleError::Refused("the game is already over".into()));
    }
    for e in &events {
        let line = describe::event(ctx.world, e);
        ctx.say(line);
    }
    ctx.events.extend(events);
    Ok(())
}

fn act_as(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = player_arg(ctx, a, 0)?;
    ctx.session.player = id;
    ctx.say(format!("now playing {}", describe::player(ctx.world, id)));
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
    let rules = &ctx.world.content().rules;
    let race = match opts.iter().find(|(k, _)| *k == "race") {
        Some((_, id)) => resolve(&rules.races, id, |d| &d.id, "race")?,
        None => rules
            .races
            .iter()
            .next()
            .map(|(id, _)| id)
            .ok_or(ConsoleError::NoSuch("race".into()))?,
    };
    let name = if plain.len() > 1 {
        plain[1..].join(" ")
    } else {
        "Capital".to_string()
    };
    let race = Some(race);
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

fn event(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    use f4x_core::world_event::EventKind;
    let name = a.need(0, "an event name")?;
    let kind = EventKind::from_id(name).ok_or_else(|| {
        ConsoleError::Usage(format!(
            "event {name:?} is not one of {}",
            EventKind::all()
                .map(|k| k.id())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;
    let id = player_arg(ctx, a, 1)?;
    ctx.world
        .cheat_event(kind, id)
        .map_err(ConsoleError::Failed)?;
    ctx.say(format!(
        "{} happens to {}",
        name.replace('_', " "),
        describe::player(ctx.world, id)
    ));
    Ok(())
}

fn offer(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    use f4x_core::player::offer::OfferKind;
    let what = match a.need(0, "hero, mercenaries or item")? {
        "hero" => OfferKind::Hero,
        "mercenaries" | "merc" => OfferKind::Mercenaries,
        "item" => OfferKind::Item,
        other => {
            return Err(ConsoleError::Usage(format!(
                "offer hero, mercenaries or item, not {other:?}"
            )))
        }
    };
    let id = player_arg(ctx, a, 1)?;
    match ctx.world.cheat_offer(id, what) {
        Some(o) => {
            let line = describe::offer(ctx.world, &o);
            ctx.say(format!(
                "{} is offered: {line}",
                describe::player(ctx.world, id)
            ));
            Ok(())
        }
        None => Err(ConsoleError::Failed("there is nothing to offer".into())),
    }
}

fn enchant(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = parse_units(ctx.world, a.need(0, "units")?)?;
    let spell = resolve(
        &ctx.world.content().rules.spells,
        a.need(1, "spell")?,
        |d| &d.id,
        "spell",
    )?;
    let caster = player_arg(ctx, a, 2)?;
    for u in units {
        let line = describe::unit(ctx.world, u);
        if !ctx.world.cheat_enchant_unit(u, spell, caster) {
            return Err(ConsoleError::NoSuch(line));
        }
        ctx.say(format!(
            "{line} now has {}",
            describe::spell(ctx.world, spell)
        ));
    }
    Ok(())
}

fn citizens(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let city = city_by_serial(ctx.world, a.int(0, "city number")? as u64)?;
    let n = a.int(1, "citizens")?;
    let n = u32::try_from(n).map_err(|_| ConsoleError::Parse(format!("{n} is not a size")))?;
    if !ctx.world.cheat_set_citizens(city, n) {
        return Err(ConsoleError::Failed("that is not a city".into()));
    }
    ctx.say(format!(
        "{} now has {n} citizens",
        describe::city_name(ctx.world, city)
    ));
    Ok(())
}

fn feature_id(ctx: &Ctx, name: &str) -> Result<f4x_core::map::FeatureId, ConsoleError> {
    let features = &ctx.world.content().features;
    features
        .defs()
        .iter()
        .find(|d| d.id == name)
        .and_then(|d| features.id_numbered(d.num))
        .ok_or_else(|| ConsoleError::NoSuch(format!("feature {name:?}")))
}

fn lair(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = feature_id(ctx, a.need(0, "feature id")?)?;
    let pos = parse_pos(a.need(1, "position")?, Plane::Arcanus)?;
    let strong = match a.get(2) {
        None | Some("weak") => false,
        Some("strong") => true,
        Some(other) => {
            return Err(ConsoleError::Usage(format!(
                "lair <feature> <pos> [strong|weak], not {other:?}"
            )))
        }
    };
    ctx.world
        .cheat_add_lair(pos, id, strong)
        .map_err(ConsoleError::Failed)?;
    ctx.say(format!("a lair stands at {}", describe::pos(pos)));
    Ok(())
}

fn unlair(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let pos = parse_pos(a.need(0, "position")?, Plane::Arcanus)?;
    if !ctx.world.cheat_remove_lair(pos) {
        return Err(ConsoleError::Failed(
            "there is no lair on that square".into(),
        ));
    }
    ctx.say(format!("the lair at {} is gone", describe::pos(pos)));
    Ok(())
}

fn feature(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let pos = parse_pos(a.need(0, "position")?, Plane::Arcanus)?;
    let name = a.need(1, "feature id or none")?;
    let id = match name {
        "none" => None,
        other => Some(feature_id(ctx, other)?),
    };
    if !ctx.world.cheat_set_feature(pos, id) {
        return Err(ConsoleError::Failed("that square is off the map".into()));
    }
    ctx.say(format!("{} now has {name}", describe::pos(pos)));
    Ok(())
}

/// The hero type a hero's id or number names (a hero type is also its unit's number).
fn hero_kind(ctx: &Ctx, text: &str) -> Result<usize, ConsoleError> {
    let rules = &ctx.world.content().rules;
    let unit = resolve(&rules.units, text, |d| &d.id, "hero")?;
    f4x_core::hero::kind::kind_of(rules, unit)
        .ok_or_else(|| ConsoleError::NoSuch(format!("hero {text:?}")))
}

fn hire(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let kind = hero_kind(ctx, a.need(0, "hero")?)?;
    // The words after the hero: a square (`x,y[,plane]`) and a player, in either order.
    let rest: Vec<&str> = (1..a.len()).filter_map(|i| a.get(i)).collect();
    let square = rest.iter().find(|w| w.contains(','));
    let player_word = rest.iter().find(|w| !w.contains(','));
    let id = match player_word {
        Some(p) => {
            let id = parse_player(p)?;
            ctx.world
                .player_entity(id)
                .map(|_| id)
                .ok_or_else(|| ConsoleError::NoSuch(format!("player {}", id.0)))?
        }
        None => player_arg(ctx, a, usize::MAX)?,
    };
    let at = match square {
        Some(s) => Some(crate::args::parse_pos(s, f4x_core::map::Plane::Arcanus)?),
        None => None,
    };
    let unit = ctx
        .world
        .cheat_hire_hero(id, kind, at)
        .map_err(ConsoleError::Failed)?;
    let line = describe::unit(ctx.world, unit);
    ctx.say(format!("hired: {line}"));
    Ok(())
}

fn dismiss(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let kind = hero_kind(ctx, a.need(0, "hero")?)?;
    let hero = ctx
        .world
        .units()
        .hero_entity(kind)
        .filter(|&h| ctx.world.units().unit_owner(h).is_some())
        .ok_or_else(|| ConsoleError::Failed("that hero is not in anyone's service".into()))?;
    let line = describe::unit(ctx.world, hero);
    ctx.world.dismiss_unit(hero);
    ctx.say(format!("dismissed: {line}"));
    Ok(())
}

fn reroll(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let kind = hero_kind(ctx, a.need(0, "hero")?)?;
    ctx.world
        .cheat_reroll_hero(kind)
        .map_err(ConsoleError::Failed)?;
    let hero = ctx.world.units().hero_entity(kind);
    let line = hero.map_or_else(String::new, |h| describe::unit(ctx.world, h));
    ctx.say(format!("rerolled: {line}"));
    Ok(())
}

fn treasury(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let id = player_arg(ctx, a, 0)?;
    let wallet = ctx.world.player(id).map(|p| p.treasury());
    let (gold, mana) = wallet.map_or_else(
        || ("0".to_string(), "0".to_string()),
        |w| (w.gold.to_string(), w.mana.to_string()),
    );
    let net = ctx.world.budget(id).net_gold();
    let fame = ctx.world.fame(id);
    ctx.say(format!(
        "{}: gold {gold} (net {net:+}), mana {mana}, fame {fame}",
        describe::player(ctx.world, id)
    ));
    Ok(())
}
