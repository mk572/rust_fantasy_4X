//! Legal commands: what a player may do in normal play, one per action of the client.
//! They build the same `Command`s the client does and send them through `Game::apply`.

use f4x_core::battle::BattleCommand;
use f4x_core::city::Position;
use f4x_core::map::Plane;
use f4x_core::production::ProductionItem;
use f4x_core::Command;

use crate::args::{city_by_serial, parse_pos, parse_target, parse_units, resolve, Args};
use crate::{CommandDef, ConsoleError, Ctx, Kind};

macro_rules! legal {
    ($name:literal, $usage:literal, $help:literal, $run:ident) => {
        CommandDef {
            name: $name,
            kind: Kind::Legal,
            usage: $usage,
            help: $help,
            run: $run,
        }
    };
}

pub const TABLE: &[CommandDef] = &[
    legal!("end", "end", "End your turn (the other wizards play, then the next turn begins).", end),
    legal!("undo", "undo", "Take back your last reversible action.", undo),
    legal!("move", "move <units> <x,y[,plane]>", "Move a stack one square. Units are numbers (12,13) or @x,y for the whole stack there. Moving into an enemy stack or city attacks it (a battle starts), and onto a guarded site assaults it.", mv),
    legal!("attack", "attack <units> <x,y[,plane]>", "The same as `move`, for attacking an adjacent square.", mv),
    legal!("auto", "auto <units> <x,y[,plane]>", "Fight the hostile stack on an adjacent square to the end without the tactical map (the battle AI plays both sides). The winner moves in and takes a city.", auto),
    legal!("goto", "goto <units> <x,y[,plane]>", "Give a stack a standing order to walk to a square, a step each turn.", goto),
    legal!("stop", "stop <units>", "Cancel standing orders.", stop),
    legal!("outpost", "outpost <unit> [name]", "A settler founds an outpost where it stands.", outpost),
    legal!("road", "road <units> [x,y ...]", "Engineers build a road where they stand, then along the squares given.", road),
    legal!("purify", "purify <units>", "Cleanse the corrupted square the stack stands on.", purify),
    legal!("meld", "meld <units>", "Meld a spirit of the stack into the magic node it stands on.", meld),
    legal!("shift", "shift <units>", "Move the stack to the same square of the other plane.", shift),
    legal!("gate", "gate <units> <x,y[,plane]>", "Teleport the stack through Earth Gates to another of your cities that has one (one move).", gate),
    legal!("enter", "enter <units> <x,y[,plane]>", "Assault a guarded site (tower, node or lair) the stack stands next to.", enter),
    legal!("produce", "produce <city> <building|unit|trade|housing>", "Set what a city builds. Ids or names; `options` lists what is possible.", produce),
    legal!("raze", "raze <city>", "Destroy the city you have just taken (before the turn ends): more fame lost, a tenth of its buildings' cost looted.", raze),
    legal!("buy", "buy <city>", "Pay gold to finish the current item at the next turn change.", buy),
    legal!("farmers", "farmers <city> <number>", "Set how many citizens farm.", farmers),
    legal!("sell", "sell <city> <building>", "Sell a building for its cost in gold.", sell),
    legal!("tax", "tax <rate>", "Set the tax rate (an index into the rate table).", tax),
    legal!("accept", "accept <offer> [name]", "Take an offer (numbers are listed by `offers`); a hired hero gets the name if you give one.", accept),
    legal!("reject", "reject <offer>", "Turn an offer down.", reject),
    legal!("vizier", "vizier <on|off>", "The Grand Vizier chooses what your cities build when they finish a building.", vizier),
    legal!("power", "power <mana> <research> <skill>", "Split your power base between mana, research and casting skill (percent, summing to 100).", power),
    legal!("research", "research <spell>", "Choose the spell to research.", research),
    legal!("cast", "cast <spell> [target]", "Cast an overland spell. A target is unit:N, city:N, tile:x,y or player:N (a Spell Ward: ward:<city>:<realm>); give it now if the spell is paid in full this turn.", cast),
    legal!("target", "target <target>", "Aim a spell that has been paid for over several turns.", target),
    legal!("cancelcast", "cancelcast", "Give up the spell being cast.", cancelcast),
    legal!("turnoff", "turnoff <spell>", "Turn off one of your own lasting spells that is on the wizard (a global enchantment).", turnoff),
    legal!("alchemy", "alchemy <amount> <gold|mana>", "Convert that much gold into mana (or mana into gold): two for one, one for one with Alchemy.", alchemy),
    legal!("bmove", "bmove <unit> <x,y>", "In battle: walk a unit to a square (`battle` lists unit numbers and squares).", bmove),
    legal!("battack", "battack <unit> <target>", "In battle: attack an adjacent enemy in melee.", battack),
    legal!("bshoot", "bshoot <unit> <target>", "In battle: fire at an enemy at a distance.", bshoot),
    legal!("bwall", "bwall <unit> <x,y>", "In battle: a wall crusher strikes the wall section at that square, next to it.", bwall),
    legal!("bshootwall", "bshootwall <unit> <x,y>", "In battle: a wall crusher fires at the wall section at that square.", bshootwall),
    legal!("bcast", "bcast <spell> [unit:N | tile:x,y] [mana:N]", "In battle: your wizard casts a spell (once a turn). Aim it at a battle unit or a square; extra mana makes a bolt, a dispel or Counter Magic stronger (`bspells` lists what you can cast).", bcast),
    legal!("bdone", "bdone <unit>", "In battle: give up the rest of a unit's turn.", bdone),
    legal!("bend", "bend", "In battle: end your side's turn.", bend),
    legal!("bflee", "bflee", "In battle: leave; the other side wins.", bflee),
];

impl Ctx<'_> {
    /// Units from a list word, all of which must be the player's (override mode may act for
    /// anyone, and the line is then logged as an override).
    fn my_units(&mut self, a: &Args, i: usize) -> Result<Vec<hecs::Entity>, ConsoleError> {
        let units = parse_units(self.game, a.need(i, "units")?)?;
        let me = self.session.player;
        if units
            .iter()
            .any(|&u| self.game.units().unit_owner(u) != Some(me))
        {
            if !self.overriding() {
                return Err(ConsoleError::NotAllowed("those are not your units".into()));
            }
            self.bypassed = true;
        }
        Ok(units)
    }

    fn my_city(&mut self, a: &Args, i: usize) -> Result<hecs::Entity, ConsoleError> {
        let c = city_by_serial(self.game, a.int(i, "city number")? as u64)?;
        if self.game.economy().city_owner(c) != Some(self.session.player) {
            if !self.overriding() {
                return Err(ConsoleError::NotAllowed("that is not your city".into()));
            }
            self.bypassed = true;
        }
        Ok(c)
    }

    fn plane_of(&self, units: &[hecs::Entity]) -> Plane {
        units
            .first()
            .and_then(|&u| self.game.units().unit_position(u))
            .map_or(Plane::Arcanus, |p| p.plane)
    }
}

fn end(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    ctx.apply(Command::EndTurn)
}

fn undo(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let label = ctx
        .game
        .undo()
        .map_err(|e| ConsoleError::Refused(format!("{e:?}")))?;
    ctx.say(format!("undone: {label}"));
    Ok(())
}

fn mv(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    let to = parse_pos(a.need(1, "destination")?, ctx.plane_of(&units))?;
    ctx.apply(Command::MoveStack { units, to })
}

fn auto(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let attackers = ctx.my_units(a, 0)?;
    let at = parse_pos(a.need(1, "target square")?, ctx.plane_of(&attackers))?;
    let me = ctx.session.player;
    let defenders = ctx
        .game
        .economy()
        .hostile_at(at, me)
        .filter(|d| !d.is_empty())
        .ok_or_else(|| {
            ConsoleError::NoSuch(format!("hostile units at {}", crate::describe::pos(at)))
        })?;
    ctx.apply(Command::AutoResolve {
        attackers,
        defenders,
    })
}

fn goto(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    let to = parse_pos(a.need(1, "destination")?, ctx.plane_of(&units))?;
    ctx.apply(Command::GoTo { units, to })
}

fn stop(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    ctx.apply(Command::ClearOrders { units })
}

fn outpost(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    let &[unit] = units.as_slice() else {
        return Err(ConsoleError::Usage("outpost takes one unit".into()));
    };
    let name = if a.len() > 1 {
        a.rest(1)
    } else {
        let used = ctx.game.economy().cities().len();
        ctx.game
            .units()
            .unit_def(unit)
            .and_then(|d| d.race.as_deref())
            .and_then(|r| ctx.game.content().rules.races.by_name(r))
            .and_then(|r| r.city_names.get(used % r.city_names.len().max(1)).cloned())
            .unwrap_or_else(|| format!("Outpost {}", used + 1))
    };
    ctx.apply(Command::SettleOutpost { unit, name })
}

fn road(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    let plane = ctx.plane_of(&units);
    let path = a.words[1..]
        .iter()
        .map(|w| parse_pos(w, plane))
        .collect::<Result<Vec<Position>, _>>()?;
    ctx.apply(Command::BuildRoad { units, path })
}

fn purify(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    ctx.apply(Command::Purify { units })
}

fn meld(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    ctx.apply(Command::Meld { units })
}

fn shift(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    ctx.apply(Command::ShiftPlane { units })
}

fn gate(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    let to = parse_pos(a.need(1, "city")?, ctx.plane_of(&units))?;
    ctx.apply(Command::EarthGate { units, to })
}

fn enter(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let units = ctx.my_units(a, 0)?;
    let to = parse_pos(a.need(1, "site")?, ctx.plane_of(&units))?;
    ctx.apply(Command::EnterSite { units, to })
}

fn production_item(ctx: &Ctx, text: &str) -> Result<ProductionItem, ConsoleError> {
    let rules = &ctx.game.content().rules;
    match text.to_lowercase().as_str() {
        "trade" | "tradegoods" | "trade_goods" => return Ok(ProductionItem::TradeGoods),
        "housing" => return Ok(ProductionItem::Housing),
        _ => {}
    }
    if let Ok(b) = resolve(&rules.buildings, text, |d| &d.name, "building") {
        return Ok(ProductionItem::Building(b));
    }
    resolve(&rules.units, text, |d| &d.name, "building or unit").map(ProductionItem::Unit)
}

fn produce(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let city = ctx.my_city(a, 0)?;
    let item = production_item(ctx, &a.rest(1))?;
    ctx.apply(Command::SetProduction { city, item })
}

fn buy(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let city = ctx.my_city(a, 0)?;
    ctx.apply(Command::BuyProduction { city })
}

fn raze(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let city = ctx.my_city(a, 0)?;
    let player = ctx.session.player;
    ctx.apply(Command::RazeCity { player, city })
}

fn farmers(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let city = ctx.my_city(a, 0)?;
    let farmers = a.int(1, "number of farmers")?.max(0) as u32;
    ctx.apply(Command::SetFarmers { city, farmers })
}

fn sell(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let city = ctx.my_city(a, 0)?;
    let building = resolve(
        &ctx.game.content().rules.buildings,
        &a.rest(1),
        |d| &d.name,
        "building",
    )?;
    ctx.apply(Command::SellBuilding { city, building })
}

fn tax(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let rate = a.int(0, "rate")?.clamp(0, 255) as u8;
    let player = ctx.session.player;
    ctx.apply(Command::SetTaxRate { player, rate })
}

fn vizier(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let on = match a.need(0, "on or off")?.to_lowercase().as_str() {
        "on" | "yes" | "1" => true,
        "off" | "no" | "0" => false,
        other => return Err(ConsoleError::Parse(format!("{other:?} is not on or off"))),
    };
    let player = ctx.session.player;
    ctx.apply(Command::SetGrandVizier { player, on })?;
    ctx.say(format!("Grand Vizier {}", if on { "on" } else { "off" }));
    Ok(())
}

fn power(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let part = |i, what| a.int(i, what).map(|v| v.clamp(0, 100) as u8);
    let (mana, research, skill) = (part(0, "mana")?, part(1, "research")?, part(2, "skill")?);
    let player = ctx.session.player;
    ctx.apply(Command::SetPowerSplit {
        player,
        mana,
        research,
        skill,
    })
}

fn spell_arg(ctx: &Ctx, text: &str) -> Result<f4x_core::rules::SpellId, ConsoleError> {
    resolve(&ctx.game.content().rules.spells, text, |d| &d.name, "spell")
}

fn research(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let spell = spell_arg(ctx, &a.rest(0))?;
    let player = ctx.session.player;
    ctx.apply(Command::SetResearch { player, spell })
}

fn cast(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let target = match a.words.last().filter(|w| w.contains(':')) {
        Some(t) => Some(parse_target(ctx.game, t)?),
        None => None,
    };
    let name_words = a.len() - usize::from(target.is_some());
    let spell = spell_arg(ctx, &a.words[..name_words].join(" "))?;
    let player = ctx.session.player;
    ctx.apply(Command::CastSpell {
        player,
        spell,
        target,
    })
}

fn target(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let target = parse_target(ctx.game, a.need(0, "target")?)?;
    let player = ctx.session.player;
    ctx.apply(Command::ResolveCast { player, target })
}

fn cancelcast(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let player = ctx.session.player;
    ctx.apply(Command::CancelCast { player })
}

fn turnoff(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let spell = crate::args::resolve(
        &ctx.game.content().rules.spells,
        a.need(0, "spell")?,
        |d| &d.name,
        "spell",
    )?;
    let player = ctx.session.player;
    ctx.apply(Command::TurnOffSpell { player, spell })
}

fn alchemy(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let amount = a.int(0, "amount")? as i32;
    let gold_to_mana = match a.need(1, "gold or mana")? {
        "gold" => true,
        "mana" => false,
        other => {
            return Err(ConsoleError::Usage(format!(
                "convert gold or mana, not {other:?}"
            )))
        }
    };
    let player = ctx.session.player;
    ctx.apply(Command::Alchemy {
        player,
        amount,
        gold_to_mana,
    })
}

fn battle_unit(a: &Args, i: usize) -> Result<usize, ConsoleError> {
    Ok(a.int(i, "battle unit number")?.max(0) as usize)
}

fn battle_pos(text: &str) -> Result<(i32, i32), ConsoleError> {
    let bad = || ConsoleError::Parse(format!("{text:?} is not a battle square (x,y)"));
    let (x, y) = text.split_once(',').ok_or_else(bad)?;
    Ok((x.parse().map_err(|_| bad())?, y.parse().map_err(|_| bad())?))
}

/// A battle order is only the console player's when that player's side is the one to play.
fn battle_order(ctx: &mut Ctx, order: BattleCommand) -> Result<(), ConsoleError> {
    let b = ctx
        .game
        .battle()
        .ok_or_else(|| ConsoleError::NoSuch("battle in progress".into()))?;
    if b.player_of(b.active) != ctx.session.player {
        if !ctx.overriding() {
            return Err(ConsoleError::NotAllowed(
                "it is not your side's turn".into(),
            ));
        }
        ctx.bypassed = true;
    }
    ctx.apply(Command::Battle(order))
}

fn bmove(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let order = BattleCommand::Move {
        unit: battle_unit(a, 0)?,
        to: battle_pos(a.need(1, "square")?)?,
    };
    battle_order(ctx, order)
}

fn battack(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let order = BattleCommand::Attack {
        unit: battle_unit(a, 0)?,
        target: battle_unit(a, 1)?,
    };
    battle_order(ctx, order)
}

fn bshoot(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let order = BattleCommand::Shoot {
        unit: battle_unit(a, 0)?,
        target: battle_unit(a, 1)?,
    };
    battle_order(ctx, order)
}

fn bwall(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let order = BattleCommand::AttackWall {
        unit: battle_unit(a, 0)?,
        cell: battle_pos(a.need(1, "square")?)?,
    };
    battle_order(ctx, order)
}

fn bshootwall(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let order = BattleCommand::ShootWall {
        unit: battle_unit(a, 0)?,
        cell: battle_pos(a.need(1, "square")?)?,
    };
    battle_order(ctx, order)
}

fn bcast(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    use f4x_core::battle::SpellAim;
    let mut aim = SpellAim::None;
    let mut mana = 0;
    let mut name = Vec::new();
    for word in a.words {
        if let Some(n) = word.strip_prefix("mana:") {
            mana = n
                .parse()
                .map_err(|_| ConsoleError::Parse(format!("{n:?} is not an amount of mana")))?;
        } else if let Some(n) = word.strip_prefix("unit:") {
            aim = SpellAim::Unit(
                n.parse()
                    .map_err(|_| ConsoleError::Parse(format!("{n:?} is not a battle unit")))?,
            );
        } else if let Some(c) = word.strip_prefix("tile:") {
            aim = SpellAim::Tile(battle_pos(c)?);
        } else {
            name.push(word.as_str());
        }
    }
    let spell = spell_arg(ctx, &name.join(" "))?;
    battle_order(ctx, BattleCommand::Cast { spell, aim, mana })
}

fn bdone(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let order = BattleCommand::Done {
        unit: battle_unit(a, 0)?,
    };
    battle_order(ctx, order)
}

fn bend(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    battle_order(ctx, BattleCommand::EndTurn)
}

fn bflee(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    battle_order(ctx, BattleCommand::Flee)
}

/// The 0-based index of the offer a player numbered from 1.
fn offer_index(a: &Args) -> Result<usize, ConsoleError> {
    let n = a.int(0, "offer number")?;
    usize::try_from(n - 1).map_err(|_| ConsoleError::Parse(format!("no offer number {n}")))
}

fn accept(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let index = offer_index(a)?;
    let name = (a.len() > 1).then(|| a.words[1..].join(" "));
    let player = ctx.session.player;
    ctx.apply(Command::AcceptOffer {
        player,
        index,
        name,
    })
}

fn reject(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let index = offer_index(a)?;
    let player = ctx.session.player;
    ctx.apply(Command::RejectOffer { player, index })
}
