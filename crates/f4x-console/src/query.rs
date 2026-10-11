//! Read-only commands. In legal mode they show only what the console's player can see.

use f4x_core::city::Position;
use f4x_core::economy::Population;
use f4x_core::map::Plane;
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
        name: "score",
        kind: Kind::Query,
        usage: "score [player]",
        help: "The score breakdown as the game stands.",
        run: score,
    },
];

impl Ctx<'_> {
    /// Can the console's player see this square? (Override mode sees everything.)
    fn sees(&self, p: Position) -> bool {
        if self.overriding() {
            return true;
        }
        let Some(map) = self.game.map.as_ref() else {
            return false;
        };
        if p.x >= map.width() || p.y >= map.height() {
            return false;
        }
        self.game
            .visibility(self.session.player, p.plane)
            .get((p.y * map.width() + p.x) as usize)
            .copied()
            .unwrap_or(false)
    }

    fn explored(&self, p: Position) -> bool {
        self.overriding()
            || self
                .game
                .fog
                .is_explored(self.session.player, p.plane, p.x, p.y)
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
    if let Some(p) = ctx.game.player(me) {
        let t = ctx.game.treasury(me);
        ctx.say(format!(
            "gold {} (net {:+}), mana {}, fame {}, tax rate {}",
            p.gold,
            t.net_gold(),
            p.mana,
            p.fame,
            p.tax_rate
        ));
        let mut lines = Vec::new();
        if let Some(m) = ctx.game.magic(me) {
            let split = ctx.game.power_split(me);
            lines.push(format!(
                "power split mana {}% research {}% skill {}% (yielding {} mana, {} research, {} skill)",
                m.mana_pct, m.research_pct, m.skill_pct, split.mana, split.research, split.skill
            ));
            let research = m
                .researching
                .map(|s| format!("{} ({} to go)", describe::spell(ctx.game, s), m.research_remaining))
                .unwrap_or_else(|| "nothing".into());
            lines.push(format!("researching {research}"));
            if let Some(c) = m.casting {
                lines.push(format!(
                    "casting {} ({} mana to go)",
                    describe::spell(ctx.game, c.spell),
                    c.remaining
                ));
            }
        }
        for l in lines {
            ctx.say(l);
        }
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
    let n = ctx.game.override_log().len();
    if n > 0 {
        ctx.say(format!("override commands used in this game: {n}"));
    }
    Ok(())
}

fn players(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let rows: Vec<String> = ctx
        .game
        .players
        .iter()
        .filter(|p| p.id.0 != u8::MAX && !p.neutral)
        .map(|p| {
            let state = if p.defeated {
                "defeated"
            } else if p.banished {
                "banished"
            } else {
                "active"
            };
            let me = if p.id == ctx.session.player { " (you)" } else { "" };
            format!("{} {}{me}: {state}", p.id.0, p.name)
        })
        .collect();
    for r in rows {
        ctx.say(r);
    }
    Ok(())
}

fn unit_line(ctx: &Ctx, u: hecs::Entity) -> String {
    let (hits, max) = ctx.game.unit_hits(u);
    let owner = ctx.game.unit_owner(u).map_or(0, |o| o.0);
    format!(
        "{} owner {owner} hits {hits}/{max} moves {}/{}",
        describe::unit(ctx.game, u),
        ctx.game.unit_moves_left(u),
        ctx.game.unit_max_moves(u)
    )
}

fn units(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    let mut list = match a.get(0) {
        None | Some("mine") => ctx.game.player_units(me),
        Some("all") => ctx.game.all_units(),
        Some(p) => ctx.game.units_at(parse_pos(p, Plane::Arcanus)?),
    };
    ctx.game.sort_by_serial(&mut list);
    let mut shown = 0;
    for u in list {
        let visible = ctx.game.unit_owner(u) == Some(me)
            || (ctx.game.unit_position(u).is_some_and(|p| ctx.sees(p))
                && ctx.game.unit_visible_to(u, me))
            || ctx.overriding();
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
    let at = ctx.game.unit_position(u).ok_or(ConsoleError::NoSuch("unit".into()))?;
    if ctx.game.unit_owner(u) != Some(me)
        && !(ctx.sees(at) && ctx.game.unit_visible_to(u, me))
        && !ctx.overriding()
    {
        return Err(ConsoleError::NoSuch(format!("unit {}", a.get(0).unwrap_or(""))));
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
    .map(|(n, s)| format!("{n} {}", ctx.game.unit_stat(u, *s)))
    .collect();
    ctx.say(format!(
        "{}, level {}",
        stats.join(", "),
        ctx.game.unit_level(u)
    ));
    if let Some(def) = ctx.game.unit_def(u) {
        if !def.abilities.is_empty() {
            let list = def.abilities.join(", ");
            ctx.say(format!("abilities: {list}"));
        }
    }
    Ok(())
}

fn cities(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let all = a.get(0) == Some("all");
    let me = ctx.session.player;
    let mut rows = Vec::new();
    for (e, c, p) in ctx.game.cities() {
        if c.owner == me || (all && ctx.sees(p)) || ctx.overriding() && all {
            let pop = ctx.game.world.get::<&Population>(e).map_or(0, |p| p.figures);
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
    let owner = ctx.game.city_owner(c);
    let at = ctx
        .game
        .cities()
        .into_iter()
        .find(|(e, ..)| *e == c)
        .map(|(_, _, p)| p)
        .ok_or(ConsoleError::NoSuch("city".into()))?;
    if owner != Some(ctx.session.player) && !ctx.sees(at) && !ctx.overriding() {
        return Err(ConsoleError::NoSuch(format!("city {}", a.get(0).unwrap_or(""))));
    }
    ctx.say(format!(
        "{} owner {} at {}",
        describe::city_name(ctx.game, c),
        owner.map_or(0, |o| o.0),
        describe::pos(at)
    ));
    if owner != Some(ctx.session.player) && !ctx.overriding() {
        return Ok(()); // a rival's city: only what shows from outside
    }
    if let Some(eco) = ctx.game.city_economy(c) {
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
        .world
        .get::<&f4x_core::economy::Buildings>(c)
        .map(|b| {
            b.0.iter()
                .map(|&id| ctx.game.content.rules.buildings.get(id).name.clone())
                .collect()
        })
        .unwrap_or_default();
    ctx.say(format!(
        "buildings: {}",
        if built.is_empty() { "none".into() } else { built.join(", ") }
    ));
    let prod = ctx.game.city_production(c);
    match prod.item {
        Some(item) => {
            let left = ctx.game.production_remaining(c);
            let buy = ctx
                .game
                .buy_cost(c)
                .map_or(String::new(), |g| format!(", buy for {g} gold"));
            ctx.say(format!(
                "producing {} ({left} to go{buy})",
                describe::item(ctx.game, item)
            ));
        }
        None => ctx.say("producing nothing"),
    }
    let mut garrison = ctx.game.garrison(at);
    ctx.game.sort_by_serial(&mut garrison);
    for u in garrison {
        let line = unit_line(ctx, u);
        ctx.say(format!("  {line}"));
    }
    Ok(())
}

fn options(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    use f4x_core::production::ProductionItem;
    let c = city_by_serial(ctx.game, a.int(0, "city number")? as u64)?;
    if ctx.game.city_owner(c) != Some(ctx.session.player) && !ctx.overriding() {
        return Err(ConsoleError::NotAllowed("not your city".into()));
    }
    let rules = &ctx.game.content.rules;
    let mut items: Vec<ProductionItem> = rules
        .buildings
        .iter()
        .map(|(id, _)| ProductionItem::Building(id))
        .chain(rules.units.iter().map(|(id, _)| ProductionItem::Unit(id)))
        .chain([ProductionItem::TradeGoods, ProductionItem::Housing])
        .collect();
    items.retain(|&i| ctx.game.can_build(c, i).is_ok());
    for i in items {
        let (name, id) = match i {
            ProductionItem::Building(b) => {
                let d = ctx.game.content.rules.buildings.get(b);
                (d.name.clone(), d.id.clone())
            }
            ProductionItem::Unit(u) => {
                let d = ctx.game.content.rules.units.get(u);
                (d.name.clone(), d.id.clone())
            }
            ProductionItem::TradeGoods => ("Trade Goods".into(), "trade".into()),
            ProductionItem::Housing => ("Housing".into(), "housing".into()),
        };
        let cost = ctx.game.item_cost(c, i);
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
    let map = ctx.game.map.as_ref().ok_or(ConsoleError::NoSuch("map".into()))?;
    let grid = map.plane(p.plane);
    let t = grid
        .get(p.x as i32, p.y as i32)
        .and_then(|t| ctx.game.content.terrains.get(t))
        .map_or("outside the map".to_string(), |d| d.name.clone());
    let mut line = format!("{}: {t}", describe::pos(p));
    if let Some(f) = grid
        .feature(p.x as i32, p.y as i32)
        .and_then(|f| ctx.game.content.features.get(f))
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
    if let Some(c) = ctx.game.city_at(p) {
        let line = describe::city_name(ctx.game, c);
        ctx.say(format!("  {line}"));
    }
    if ctx.sees(p) {
        let mut here = ctx.game.units_at(p);
        ctx.game.sort_by_serial(&mut here);
        for u in here {
            if ctx.game.unit_visible_to(u, ctx.session.player) || ctx.overriding() {
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
            .cities()
            .into_iter()
            .find(|(_, c, _)| c.owner == me)
            .map(|(_, _, p)| p)
            .unwrap_or(Position { plane: Plane::Arcanus, x: 0, y: 0 });
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
    let map = ctx.game.map.as_ref().ok_or(ConsoleError::NoSuch("map".into()))?;
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
            let p = Position { plane: centre.plane, x, y: y as u32 };
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
            .units_at(p)
            .into_iter()
            .any(|u| ctx.game.unit_owner(u) == Some(ctx.session.player));
        if mine {
            return '*';
        }
        if !ctx.game.units_at(p).is_empty() {
            return 'u';
        }
    }
    if ctx.game.city_at(p).is_some() {
        return 'C';
    }
    use f4x_core::content::TerrainClass as C;
    match ctx.game.terrain_class_at(p) {
        Some(C::Ocean) => '~',
        Some(C::Hills) | Some(C::Mountains) => '^',
        Some(_) => '.',
        None => ' ',
    }
}

fn spells(ctx: &mut Ctx, _: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    let m = ctx
        .game
        .magic(me)
        .ok_or_else(|| ConsoleError::NoSuch("wizard".into()))?;
    let rules = &ctx.game.content.rules;
    let line = |ids: &mut dyn Iterator<Item = &f4x_core::rules::SpellId>, cost_research: bool| {
        ids.map(|&s| {
            let d = rules.spells.get(s);
            let c = if cost_research { d.research_cost } else { d.cost };
            format!("{} ({}, {c})", d.id, d.name)
        })
        .collect::<Vec<_>>()
    };
    let known = line(&mut m.known.iter(), false);
    let researchable = line(&mut m.researchable.iter(), true);
    ctx.say(format!("known (id, name, mana): {}", join(&known)));
    ctx.say(format!("researchable (id, name, research): {}", join(&researchable)));
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
        b.winner.map_or(String::new(), |w| format!(", won by {w:?}"))
    ))
    .chain(b.units.iter().map(|u| {
        format!(
            "{} {:?} {} at {},{} figures {}/{} melee {} ranged {} defense {} moves {}/{} ammo {}",
            u.id, u.side, u.name, u.cell.0, u.cell.1, u.figures, u.figures_start, u.melee,
            u.ranged, u.defense, u.moves_left, u.moves, u.ammo
        )
    }))
    .collect();
    for l in lines {
        ctx.say(l);
    }
    Ok(())
}

fn score(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let who = match a.get(0) {
        Some(p) => crate::args::parse_player(p)?,
        None => ctx.session.player,
    };
    let s = ctx.game.score(who);
    ctx.say(format!(
        "spells {}, townsfolk {}, wizards defeated {}, fame {}, time {}, mastery {}; total {}",
        s.spells,
        s.townsfolk,
        s.conquests,
        s.fame,
        s.time,
        s.mastery,
        s.total()
    ));
    Ok(())
}
