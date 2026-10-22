//! Read-only commands. In legal mode they show only what the console's player can see.

use f4x_core::map::{Plane, Position};

use f4x_core::rules::Stat;

use crate::args::{city_by_serial, parse_pos, unit_by_serial, Args};
use crate::describe;
use crate::{CommandDef, ConsoleError, Ctx, Kind};

pub const TABLE: &[CommandDef] = &[
    CommandDef {
        name: "status",
        kind: Kind::Query,
        usage: "status",
        help: "Turn, your gold, mana, research and spell being cast, and whether the game is over.",
        run: status,
    },
    CommandDef {
        name: "players",
        kind: Kind::Query,
        usage: "players",
        help: "The wizards and whether they are banished or defeated.",
        run: players,
    },
    CommandDef {
        name: "units",
        kind: Kind::Query,
        usage: "units [mine|all|x,y[,plane]]",
        help: "List units: yours (default), every visible one, or those on a square.",
        run: units,
    },
    CommandDef {
        name: "unit",
        kind: Kind::Query,
        usage: "unit <number>",
        help: "One unit's statistics, wounds and moves.",
        run: unit,
    },
    CommandDef {
        name: "cities",
        kind: Kind::Query,
        usage: "cities [all]",
        help: "Your cities (or every visible one).",
        run: cities,
    },
    CommandDef {
        name: "city",
        kind: Kind::Query,
        usage: "city <number>",
        help: "A city's population, income, buildings, production and garrison.",
        run: city,
    },
    CommandDef {
        name: "options",
        kind: Kind::Query,
        usage: "options <city>",
        help: "What the city can build now, with costs.",
        run: options,
    },
    CommandDef {
        name: "tile",
        kind: Kind::Query,
        usage: "tile <x,y[,plane]>",
        help: "Terrain, feature and what stands on a square.",
        run: tile,
    },
    CommandDef {
        name: "map",
        kind: Kind::Query,
        usage: "map [x,y[,plane]] [radius]",
        help: "A text map around a square (default: your first city, radius 8). # unexplored, ~ water, . land, C city, u unit, * your unit.",
        run: map,
    },
    CommandDef {
        name: "offers",
        kind: Kind::Query,
        usage: "offers",
        help: "The hero, mercenaries and merchant's item offered to you this turn; `accept` or `reject` one by its number.",
        run: offers,
    },
    CommandDef {
        name: "spells",
        kind: Kind::Query,
        usage: "spells",
        help: "Spells you can cast, what you may research, and the research in progress.",
        run: spells,
    },
    CommandDef {
        name: "battle",
        kind: Kind::Query,
        usage: "battle",
        help: "The battle being fought: whose turn, and each unit with its square, figures and moves.",
        run: battle,
    },
    CommandDef {
        name: "blegal",
        kind: Kind::Query,
        usage: "blegal [unit]",
        help: "Everything your side may do in the battle now, as the game itself lists it: for each unit its moves, melee attacks, shots and wall attacks, then the casts (spell, aims, mana). Give a unit number for one unit.",
        run: blegal,
    },
    CommandDef {
        name: "uspells",
        kind: Kind::Query,
        usage: "uspells <unit>",
        help: "The spells a battle unit can cast on its own (a hero's spells and items, a creature caster, an ability), where each comes from and what it must be aimed at.",
        run: uspells,
    },
    CommandDef {
        name: "bspells",
        kind: Kind::Query,
        usage: "bspells",
        help: "The spells you could cast in a battle, what each costs in casting skill, and what it must be aimed at.",
        run: bspells,
    },
    CommandDef {
        name: "score",
        kind: Kind::Query,
        usage: "score [player]",
        help: "The score breakdown as the game stands.",
        run: score,
    },
];

impl Ctx<'_> {
    /// Can the console's player see this square?
    fn sees(&self, p: Position) -> bool {
        let Some(map) = self.game.map() else {
            return false;
        };
        if p.x >= map.width() || p.y >= map.height() {
            return false;
        }
        self.game
            .economy()
            .visibility(self.session.player, p.plane)
            .get((p.y * map.width() + p.x) as usize)
            .copied()
            .unwrap_or(false)
    }

    fn explored(&self, p: Position) -> bool {
        self.game.is_explored(self.session.player, p)
    }
}

fn status(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    ctx.say(format!(
        "turn {}, playing {}, mode {}",
        ctx.game.turn,
        describe::player(ctx.game, me),
        ctx.session.mode.name()
    ));
    let game = &*ctx.game;
    let mut lines = Vec::new();
    if let (Some(wallet), Some(tax)) = (
        game.treasury(me),
        game.player_component::<f4x_core::player::Tax>(me),
    ) {
        let t = game.budget(me);
        lines.push(format!(
            "gold {} (net {:+}), mana {}, fame {}, tax rate {}",
            wallet.gold,
            t.net_gold(),
            wallet.mana,
            game.fame(me),
            tax.0
        ));
        if let Some(m) = game.wizard(me) {
            let split = game.income(me).split;
            lines.push(format!(
                "power split mana {}% research {}% skill {}% (yielding {} mana, {} research, {} skill)",
                m.power_split().mana, m.power_split().research, m.power_split().skill, split.mana, split.research, split.skill
            ));
            let research = m
                .researching()
                .map(|s| {
                    format!(
                        "{} ({} to go)",
                        describe::spell(game, s),
                        m.research_remaining()
                    )
                })
                .unwrap_or_else(|| "nothing".into());
            lines.push(format!("researching {research}"));
            if let Some(c) = m.casting() {
                lines.push(format!(
                    "casting {} ({} mana to go)",
                    describe::spell(game, c.spell),
                    c.remaining
                ));
            }
        }
    }
    for l in lines {
        ctx.say(l);
    }
    if ctx.game.in_battle() {
        ctx.say("a battle is being fought (see `battle`)");
    }
    if let Some(over) = ctx.game.game_over {
        ctx.say(format!(
            "GAME OVER: {} won by {:?} on turn {}",
            describe::player(ctx.game, over.winner),
            over.kind,
            over.turn
        ));
    }
    Ok(())
}

fn players(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let rows: Vec<String> = ctx
        .game
        .player_ids()
        .into_iter()
        .filter(|id| !ctx.game.is_neutral_player(*id))
        .map(|id| {
            let state = if ctx.game.is_defeated(id) {
                "defeated"
            } else if ctx.game.wizard(id).is_some_and(|w| w.is_banished()) {
                "banished"
            } else {
                "active"
            };
            let me = if id == ctx.session.player {
                " (you)"
            } else {
                ""
            };
            format!("{} {}{me}: {state}", id.0, ctx.game.player_name(id))
        })
        .collect();
    for r in rows {
        ctx.say(r);
    }
    Ok(())
}

fn unit_line(ctx: &Ctx, u: hecs::Entity) -> String {
    let (hits, max) = ctx.game.units().unit_hits(u);
    let owner = ctx.game.units().unit_owner(u).map_or(0, |o| o.0);
    format!(
        "{} owner {owner} hits {hits}/{max} moves {}/{}",
        describe::unit(ctx.game, u),
        ctx.game.units().unit_moves_left(u),
        ctx.game.units().unit_max_moves(u)
    )
}

fn units(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    let mut list = match a.get(0) {
        None | Some("mine") => ctx.game.units().player_units(me),
        Some("all") => ctx.game.units().all_units(),
        Some(p) => ctx.game.units().units_at(parse_pos(p, Plane::Arcanus)?),
    };
    ctx.game.sort_by_serial(&mut list);
    let mut shown = 0;
    for u in list {
        let visible = ctx.game.units().unit_owner(u) == Some(me)
            || (ctx
                .game
                .units()
                .unit_position(u)
                .is_some_and(|p| ctx.sees(p))
                && ctx.game.economy().unit_visible_to(u, me));
        if visible {
            let line = unit_line(ctx, u);
            ctx.say(line);
            shown += 1;
        }
    }
    if shown == 0 {
        ctx.say("no units");
    }
    Ok(())
}

fn unit(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let u = unit_by_serial(ctx.game, a.int(0, "unit number")? as u64)?;
    let me = ctx.session.player;
    let at = ctx
        .game
        .units()
        .unit_position(u)
        .ok_or(ConsoleError::NoSuch("unit".into()))?;
    if ctx.game.units().unit_owner(u) != Some(me)
        && !(ctx.sees(at) && ctx.game.economy().unit_visible_to(u, me))
    {
        return Err(ConsoleError::NoSuch(format!(
            "unit {}",
            a.get(0).unwrap_or("")
        )));
    }
    let line = unit_line(ctx, u);
    ctx.say(line);
    let stats: Vec<String> = [
        ("melee", Stat::Melee),
        ("ranged", Stat::Ranged),
        ("defense", Stat::Defense),
        ("resistance", Stat::Resistance),
    ]
    .iter()
    .map(|(n, s)| format!("{n} {}", ctx.game.units().unit_stat(u, *s)))
    .collect();
    ctx.say(format!(
        "{}, level {}",
        stats.join(", "),
        ctx.game.units().unit_level(u)
    ));
    let abilities = ctx.game.units().unit_abilities(u);
    if !abilities.is_empty() {
        ctx.say(format!("abilities: {}", abilities.join(", ")));
    }
    if ctx.game.units().unit_is_undead(u) {
        ctx.say("undead: a Death creature, not healed".to_string());
    }
    Ok(())
}

fn cities(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let all = a.get(0) == Some("all");
    let me = ctx.session.player;
    let mut rows = Vec::new();
    for (e, c, p) in ctx.game.economy().cities() {
        if c.owner == me || (all && ctx.sees(p)) {
            let pop = ctx.game.economy().citizens(e);
            rows.push((
                ctx.game.serial_of(e),
                format!(
                    "{} owner {} at {} size {pop}",
                    describe::city_name(ctx.game, e),
                    c.owner.0,
                    describe::pos(p)
                ),
            ));
        }
    }
    rows.sort();
    if rows.is_empty() {
        ctx.say("no cities");
    }
    for (_, r) in rows {
        ctx.say(r);
    }
    Ok(())
}

fn city(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let c = city_by_serial(ctx.game, a.int(0, "city number")? as u64)?;
    let owner = ctx.game.economy().city_owner(c);
    let at = ctx
        .game
        .economy()
        .cities()
        .into_iter()
        .find(|(e, ..)| *e == c)
        .map(|(_, _, p)| p)
        .ok_or(ConsoleError::NoSuch("city".into()))?;
    if owner != Some(ctx.session.player) && !ctx.sees(at) {
        return Err(ConsoleError::NoSuch(format!(
            "city {}",
            a.get(0).unwrap_or("")
        )));
    }
    ctx.say(format!(
        "{} owner {} at {}",
        describe::city_name(ctx.game, c),
        owner.map_or(0, |o| o.0),
        describe::pos(at)
    ));
    if owner != Some(ctx.session.player) {
        return Ok(()); // a rival's city: only what shows from outside
    }
    if let Some(eco) = ctx.game.economy().city_economy(c) {
        ctx.say(format!(
            "size {} (max {}), farmers {}, workers {}, rebels {}",
            eco.figures, eco.max_size, eco.farmers, eco.workers, eco.rebels
        ));
        ctx.say(format!(
            "food {:+}, production {}, gold {:+} (upkeep {}), research {}, mana {}, growth {:+} tens",
            eco.food, eco.production, eco.gold, eco.gold_upkeep, eco.research, eco.mana, eco.growth_tens
        ));
    }
    let built: Vec<String> = ctx
        .game
        .city_buildings(c)
        .into_iter()
        .map(|id| ctx.game.content().rules.buildings.get(id).name.clone())
        .collect();
    ctx.say(format!(
        "buildings: {}",
        if built.is_empty() {
            "none".into()
        } else {
            built.join(", ")
        }
    ));
    let prod = ctx.game.economy().city_production(c);
    match prod.item {
        Some(item) => {
            let left = ctx.game.economy().production_remaining(c);
            let buy = ctx
                .game
                .economy()
                .buy_cost(c)
                .map_or(String::new(), |g| format!(", buy for {g} gold"));
            ctx.say(format!(
                "producing {} ({left} to go{buy})",
                describe::item(ctx.game, item)
            ));
        }
        None => ctx.say("producing nothing"),
    }
    let mut garrison = ctx.game.economy().garrison(at);
    ctx.game.sort_by_serial(&mut garrison);
    for u in garrison {
        let line = unit_line(ctx, u);
        ctx.say(format!("  {line}"));
    }
    Ok(())
}

fn options(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    use f4x_core::city::production::ProductionItem;
    let c = city_by_serial(ctx.game, a.int(0, "city number")? as u64)?;
    if ctx.game.economy().city_owner(c) != Some(ctx.session.player) {
        return Err(ConsoleError::NotAllowed("not your city".into()));
    }
    let rules = &ctx.game.content().rules;
    let mut items: Vec<ProductionItem> = rules
        .buildings
        .iter()
        .map(|(id, _)| ProductionItem::Building(id))
        .chain(rules.units.iter().map(|(id, _)| ProductionItem::Unit(id)))
        .chain([ProductionItem::TradeGoods, ProductionItem::Housing])
        .collect();
    items.retain(|&i| ctx.game.economy().can_build(c, i).is_ok());
    for i in items {
        let (name, id) = match i {
            ProductionItem::Building(b) => {
                let d = ctx.game.content().rules.buildings.get(b);
                (d.name.clone(), d.id.clone())
            }
            ProductionItem::Unit(u) => {
                let d = ctx.game.content().rules.units.get(u);
                (d.name.clone(), d.id.clone())
            }
            ProductionItem::TradeGoods => ("Trade Goods".into(), "trade".into()),
            ProductionItem::Housing => ("Housing".into(), "housing".into()),
        };
        let cost = ctx.game.economy().item_cost(c, i);
        ctx.say(format!("{id:<24} {name} ({cost})"));
    }
    Ok(())
}

fn tile(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let p = parse_pos(a.need(0, "position")?, Plane::Arcanus)?;
    if !ctx.explored(p) {
        ctx.say(format!("{} is unexplored", describe::pos(p)));
        return Ok(());
    }
    let map = ctx.game.map().ok_or(ConsoleError::NoSuch("map".into()))?;
    let grid = map.plane(p.plane);
    let t = grid
        .get(p.x as i32, p.y as i32)
        .and_then(|t| ctx.game.content().terrains.get(t))
        .map_or("outside the map".to_string(), |d| d.name.clone());
    let mut line = format!("{}: {t}", describe::pos(p));
    if let Some(f) = grid
        .feature(p.x as i32, p.y as i32)
        .and_then(|f| ctx.game.content().features.get(f))
    {
        line.push_str(&format!(", {}", f.name));
    }
    for (flag, name) in [
        (grid.has_river(p.x as i32, p.y as i32), "river"),
        (grid.has_road(p.x as i32, p.y as i32), "road"),
        (grid.is_corrupted(p.x as i32, p.y as i32), "corrupted"),
    ] {
        if flag {
            line.push_str(&format!(", {name}"));
        }
    }
    ctx.say(line);
    if let Some(c) = ctx.game.economy().city_at(p) {
        let line = describe::city_name(ctx.game, c);
        ctx.say(format!("  {line}"));
    }
    if ctx.sees(p) {
        let mut here = ctx.game.units().units_at(p);
        ctx.game.sort_by_serial(&mut here);
        for u in here {
            if ctx.game.economy().unit_visible_to(u, ctx.session.player) {
                let line = unit_line(ctx, u);
                ctx.say(format!("  {line}"));
            }
        }
    }
    Ok(())
}

fn map(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    let (centre, radius) = {
        let mut centre = ctx
            .game
            .economy()
            .cities()
            .into_iter()
            .find(|(_, c, _)| c.owner == me)
            .map(|(_, _, p)| p)
            .unwrap_or(Position {
                plane: Plane::Arcanus,
                x: 0,
                y: 0,
            });
        let mut radius = 8i64;
        for w in a.words {
            if w.contains(',') {
                centre = parse_pos(w, centre.plane)?;
            } else {
                radius = w
                    .parse()
                    .map_err(|_| ConsoleError::Parse(format!("{w:?} is not a radius")))?;
            }
        }
        (centre, radius.clamp(1, 20) as i32)
    };
    let map = ctx.game.map().ok_or(ConsoleError::NoSuch("map".into()))?;
    let grid = map.plane(centre.plane);
    let mut rows = Vec::new();
    for dy in -radius..=radius {
        let y = centre.y as i32 + dy;
        if y < 0 || y >= grid.height() as i32 {
            continue;
        }
        let mut row = String::new();
        for dx in -radius..=radius {
            let x = grid.wrap_x(centre.x as i32 + dx);
            let p = Position {
                plane: centre.plane,
                x,
                y: y as u32,
            };
            row.push(map_char(ctx, p));
        }
        rows.push(row);
    }
    ctx.say(format!(
        "centre {} radius {radius} (north is up; # unexplored, ~ water, . land, ^ hills/mountains, C city, u unit, * your unit)",
        describe::pos(centre)
    ));
    for r in rows {
        ctx.say(r);
    }
    Ok(())
}

fn map_char(ctx: &Ctx, p: Position) -> char {
    if !ctx.explored(p) {
        return '#';
    }
    if ctx.sees(p) {
        let mine = ctx
            .game
            .units()
            .units_at(p)
            .into_iter()
            .any(|u| ctx.game.units().unit_owner(u) == Some(ctx.session.player));
        if mine {
            return '*';
        }
        if !ctx.game.units().units_at(p).is_empty() {
            return 'u';
        }
    }
    if ctx.game.economy().city_at(p).is_some() {
        return 'C';
    }
    use f4x_core::content::TerrainClass as C;
    match ctx.game.economy().terrain_class_at(p) {
        Some(C::Ocean) => '~',
        Some(C::Hills) | Some(C::Mountains) => '^',
        Some(_) => '.',
        None => ' ',
    }
}

fn spells(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    let game = &*ctx.game;
    let (known, researchable) = {
        let m = game
            .wizard(me)
            .ok_or_else(|| ConsoleError::NoSuch("wizard".into()))?;
        let rules = &game.content().rules;
        let line = |ids: &mut dyn Iterator<Item = &f4x_core::rules::SpellId>,
                    cost_research: bool| {
            ids.map(|&s| {
                let d = rules.spells.get(s);
                let c = if cost_research {
                    d.research_cost
                } else {
                    d.cost
                };
                format!("{} ({}, {c})", d.id, d.name)
            })
            .collect::<Vec<_>>()
        };
        let known = line(&mut m.known().iter(), false);
        let researchable = line(&mut m.researchable().iter(), true);
        (known, researchable)
    };
    ctx.say(format!("known (id, name, mana): {}", join(&known)));
    ctx.say(format!(
        "researchable (id, name, research): {}",
        join(&researchable)
    ));
    Ok(())
}

fn join(v: &[String]) -> String {
    if v.is_empty() {
        "none".into()
    } else {
        v.join("; ")
    }
}

fn battle(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let b = ctx
        .game
        .battle()
        .ok_or_else(|| ConsoleError::NoSuch("battle in progress".into()))?;
    let lines: Vec<String> = std::iter::once(format!(
        "turn {}, {:?} to play ({}){}",
        b.turn,
        b.active,
        describe::player(ctx.game, b.player_of(b.active)),
        b.winner
            .map_or(String::new(), |w| format!(", won by {w:?}"))
    ))
    .chain(city_lines(b))
    .chain(wizard_lines(b))
    .chain(
        b.units
            .iter()
            // What the session's side cannot see (an invisible enemy) is not listed.
            .filter(|u| {
                b.side_of(ctx.session.player)
                    .is_none_or(|side| b.seen_by(side, u.id))
            })
            .map(|u| {
                format!(
            "{} {:?} {} at {},{} figures {}/{} melee {} ranged {} defense {} moves {}/{} ammo {}",
            u.id, u.side, u.name, u.cell.0, u.cell.1, u.figures, u.figures_start, u.melee,
            u.ranged, u.defense, u.moves_left, u.moves, u.ammo
        )
            }),
    )
    .collect();
    for l in lines {
        ctx.say(l);
    }
    Ok(())
}

fn blegal(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    use f4x_core::battle::{BattleCommand as C, SpellAim};
    let b = ctx
        .game
        .battle()
        .ok_or_else(|| ConsoleError::NoSuch("battle in progress".into()))?;
    if b.player_of(b.active) != ctx.session.player {
        return Err(ConsoleError::NotAllowed(
            "it is not your side's turn".into(),
        ));
    }
    let only: Option<usize> = match a.get(0) {
        Some(w) => Some(
            w.parse()
                .map_err(|_| ConsoleError::Parse(format!("{w:?} is not a battle unit")))?,
        ),
        None => None,
    };
    let mut lines = Vec::new();
    for u in b
        .active_units(b.active)
        .filter(|u| only.is_none_or(|o| o == u.id))
    {
        let (mut moves, mut attacks, mut shots, mut walls) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for c in b.legal_commands() {
            match c {
                C::Move { unit, to } if unit == u.id => moves.push(format!("{},{}", to.0, to.1)),
                C::Attack { unit, target } if unit == u.id => attacks.push(target.to_string()),
                C::Shoot { unit, target } if unit == u.id => shots.push(target.to_string()),
                C::AttackWall { unit, cell } | C::ShootWall { unit, cell } if unit == u.id => {
                    walls.push(format!("{},{}", cell.0, cell.1))
                }
                _ => {}
            }
        }
        let list = |name: &str, v: &[String]| {
            if v.is_empty() {
                String::new()
            } else {
                format!("; {name} {}", v.join(" "))
            }
        };
        lines.push(format!(
            "unit {} ({}): {} moves{}{}{}{}",
            u.id,
            u.name,
            moves.len(),
            list("move to", &moves),
            list("attack", &attacks),
            list("shoot", &shots),
            list("wall", &walls)
        ));
    }
    if only.is_none() {
        let rules = &ctx.game.content().rules;
        for lc in ctx.game.legal_battle_casts() {
            let aims: Vec<String> = lc
                .aims
                .iter()
                .map(|a| match a {
                    SpellAim::None => "none".to_string(),
                    SpellAim::Unit(u) => format!("unit:{u}"),
                    SpellAim::Tile(c) => format!("tile:{},{}", c.0, c.1),
                })
                .collect();
            lines.push(format!(
                "cast {}: mana {}..{}; aims {}",
                rules.spells.get(lc.spell).id,
                lc.least_mana,
                lc.most_mana,
                aims.join(" ")
            ));
        }
    }
    {
        let rules = &ctx.game.content().rules;
        for (unit, lc) in ctx.game.legal_unit_casts() {
            if only.is_some_and(|o| o != unit) {
                continue;
            }
            let aims: Vec<String> = lc
                .aims
                .iter()
                .map(|a| match a {
                    SpellAim::None => "none".to_string(),
                    SpellAim::Unit(u) => format!("unit:{u}"),
                    SpellAim::Tile(c) => format!("tile:{},{}", c.0, c.1),
                })
                .collect();
            lines.push(format!(
                "unit {unit} casts {}: mana {}..{}; aims {}",
                rules.spells.get(lc.spell).id,
                lc.least_mana,
                lc.most_mana,
                aims.join(" ")
            ));
        }
    }
    for l in lines {
        ctx.say(l);
    }
    Ok(())
}

fn uspells(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    use f4x_core::battle::spell::SpellSource;
    use f4x_core::rules::defs::SpellTarget;
    let unit = a.int(0, "battle unit number")?.max(0) as usize;
    let b = ctx
        .game
        .battle()
        .ok_or_else(|| ConsoleError::NoSuch("battle in progress".into()))?;
    let u = b
        .unit(unit)
        .ok_or_else(|| ConsoleError::NoSuch(format!("battle unit {unit}")))?;
    let mut lines = vec![format!("unit {unit} ({}) has {} mana", u.name, u.mana)];
    let rules = &ctx.game.content().rules;
    for s in ctx.game.battle_unit_spells(unit) {
        let d = rules.spells.get(s.spell);
        let aim = match s.target {
            SpellTarget::OwnUnit => "own unit",
            SpellTarget::EnemyUnit => "enemy unit",
            SpellTarget::AnyUnit => "any unit",
            SpellTarget::Tile => "a square",
            _ => "no target",
        };
        let from = match s.source {
            SpellSource::Item(slot) => format!("item charge (slot {})", slot + 1),
            SpellSource::Ability => "ability, once".to_string(),
            SpellSource::Mana => format!(
                "costs {}{} mana",
                s.cost,
                if s.takes_extra_mana { "+" } else { "" }
            ),
        };
        lines.push(format!(
            "{} ({}): {from}, aimed at {aim}{}",
            d.id,
            d.name,
            if s.castable { "" } else { " [cannot cast now]" }
        ));
    }
    for l in lines {
        ctx.say(l);
    }
    Ok(())
}

fn bspells(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    use f4x_core::rules::defs::SpellTarget;
    let me = ctx.session.player;
    let rules = &ctx.game.content().rules;
    let lines: Vec<String> = ctx
        .game
        .battle_spells(me)
        .into_iter()
        .map(|s| {
            let d = rules.spells.get(s.spell);
            let aim = match s.target {
                SpellTarget::OwnUnit => "own unit",
                SpellTarget::EnemyUnit => "enemy unit",
                SpellTarget::AnyUnit => "any unit",
                SpellTarget::Tile => "a square",
                _ => "no target",
            };
            format!(
                "{} ({}) costs {}{}, aimed at {}{}",
                d.id,
                d.name,
                s.cost,
                if s.takes_extra_mana { "+" } else { "" },
                aim,
                if s.castable { "" } else { " [cannot cast now]" }
            )
        })
        .collect();
    for l in lines {
        ctx.say(l);
    }
    Ok(())
}

/// Each side's wizard: casting skill and mana left, what casting costs, Counter Magic.
fn wizard_lines(b: &f4x_core::battle::Battle) -> Vec<String> {
    use f4x_core::battle::Side;
    [Side::Attacker, Side::Defender]
        .into_iter()
        .filter_map(|side| {
            let c = b.caster(side)?;
            Some(format!(
                "{side:?} wizard: casting skill {} left, mana {}, casting costs x{:.1}{}{}",
                c.skill_left,
                c.mana,
                c.range as f32 / 10.0,
                if c.cast_this_turn {
                    ", has cast this turn"
                } else {
                    ""
                },
                match b.counter_magic[side.index()] {
                    0 => String::new(),
                    n => format!(", Counter Magic {n}"),
                }
            ))
        })
        .collect()
}

/// What stands round a city fought over: its walls, standing and fallen, and the gate.
fn city_lines(b: &f4x_core::battle::Battle) -> Vec<String> {
    use f4x_core::battle::map::{CITY_SIZE, CITY_X0, CITY_Y0, GATE};
    if !b.map.is_walled() {
        return if b.map.is_city() {
            vec!["a city without walls".to_string()]
        } else {
            Vec::new()
        };
    }
    let squares = |down: bool| -> String {
        let mut out = Vec::new();
        for y in CITY_Y0..CITY_Y0 + CITY_SIZE {
            for x in CITY_X0..CITY_X0 + CITY_SIZE {
                let wall = b.map.wall((x, y));
                let fallen = wall == f4x_core::battle::map::Wall::Broken;
                if down == fallen && wall != f4x_core::battle::map::Wall::None {
                    out.push(format!("{x},{y}"));
                }
            }
        }
        out.join(" ")
    };
    let mut lines = vec![format!(
        "city walls stand at {} (the gate is at {},{}); damage to the city {}",
        squares(false),
        GATE.0,
        GATE.1,
        b.city_damage
    )];
    let fallen = squares(true);
    if !fallen.is_empty() {
        lines.push(format!("fallen walls at {fallen}"));
    }
    lines
}

fn score(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let who = match a.get(0) {
        Some(p) => crate::args::parse_player(p)?,
        None => ctx.session.player,
    };
    let s = ctx.game.score(who);
    ctx.say(format!(
        "spells {}, townsfolk {}, wizards defeated {}, fame {}, time {}, mastery {}; scale {}%; total {}",
        s.spells,
        s.townsfolk,
        s.conquests,
        s.fame,
        s.time,
        s.mastery,
        s.percent,
        s.total()
    ));
    Ok(())
}

fn offers(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    let mut lines: Vec<String> = ctx
        .game
        .offers(me)
        .iter()
        .enumerate()
        .map(|(i, o)| format!("{}: {}", i + 1, describe::offer(ctx.game, o)))
        .collect();
    if lines.is_empty() {
        lines.push("no offers".into());
    }
    for line in lines {
        ctx.say(line);
    }
    Ok(())
}
