//! Words for things: units, cities, positions and events, as the console prints them.

use f4x_core::city::production::ProductionItem;
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;

use f4x_core::game::{Event, Game};

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
    if let Some(p) = game.player(id).filter(|_| game.has_player(id)) {
        format!("{} ({})", p.name(), id.0)
    } else {
        format!("player {}", id.0)
    }
}

pub fn unit(game: &Game, u: hecs::Entity) -> String {
    let name = game.units().unit_def(u).map_or("?", |d| d.name.as_str());
    let at = game
        .units()
        .unit_position(u)
        .map_or(String::new(), |p| format!(" at {}", pos(p)));
    format!("unit {} {name}{at}", game.serial_of(u).0)
}

pub fn city_name(game: &Game, c: hecs::Entity) -> String {
    let name = game
        .economy()
        .cities()
        .into_iter()
        .find(|(e, ..)| *e == c)
        .map_or("?".to_string(), |(_, c, _)| c.name);
    format!("city {} {name}", game.serial_of(c).0)
}

pub fn item(game: &Game, item: ProductionItem) -> String {
    let rules = &game.content().rules;
    match item {
        ProductionItem::Building(b) => rules.buildings.get(b).name.clone(),
        ProductionItem::Unit(u) => rules.units.get(u).name.clone(),
        ProductionItem::TradeGoods => "Trade Goods".into(),
        ProductionItem::Housing => "Housing".into(),
    }
}

pub fn spell(game: &Game, s: f4x_core::rules::SpellId) -> String {
    game.content().rules.spells.get(s).name.clone()
}

fn units(game: &Game, list: &[hecs::Entity]) -> String {
    list.iter()
        .map(|&u| game.serial_of(u).0.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// One line for an offer waiting for an answer.
pub fn offer(game: &Game, o: &f4x_core::player::offer::Offer) -> String {
    use f4x_core::player::offer::Offer;
    let rules = &game.content().rules;
    match o {
        Offer::Hero { kind, price } => {
            let name = rules
                .hero_templates
                .get(*kind)
                .and_then(|t| rules.units.by_name(&t.unit))
                .map_or("a hero", |u| u.name.as_str());
            format!("{name} asks {price} gold to join you")
        }
        Offer::Mercenaries {
            unit,
            count,
            level,
            price,
        } => {
            let name = rules
                .units
                .by_name(unit)
                .map_or(unit.as_str(), |u| u.name.as_str());
            format!("{count} {name} of level {} for {price} gold", level + 1)
        }
        Offer::Item { item, price } => format!(
            "the item {} for {price} gold",
            item.display_name(game.content())
        ),
        Offer::Resale { item, price, .. } => format!(
            "the item {} for {price} gold, sold by another wizard",
            item.display_name(game.content())
        ),
    }
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
        Event::ProductionBlocked { city, item: i } => {
            format!(
                "{} cannot finish {} yet: the city is full or too small",
                city_name(game, *city),
                item(game, *i)
            )
        }
        Event::ProductionImpossible { city, item: i } => format!(
            "{} can no longer produce {}",
            city_name(game, *city),
            item(game, *i)
        ),
        Event::ChaosRiftStrikes {
            city,
            units_lost,
            building,
        } => {
            let mut line = format!("The chaos rift over {} strikes", city_name(game, *city));
            if *units_lost > 0 {
                line += &format!(": {units_lost} unit(s) destroyed");
            }
            if let Some(b) = building {
                line += &format!(", {} falls", game.content().rules.buildings.get(*b).name);
            }
            line
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
        Event::ShipLanded {
            player: p,
            units,
            at,
        } => format!(
            "a ship of {} landed {units} units at {}",
            player(game, *p),
            pos(*at)
        ),
        Event::FortressPlaced { player: p, pos: at } => {
            format!("{} placed a fortress at {}", player(game, *p), pos(*at))
        }
        Event::WizardBanished { player: p, by } => {
            format!("{} is banished by {}", player(game, *p), player(game, *by))
        }
        Event::WizardReturned { player: p, .. } => format!("{} has returned", player(game, *p)),
        Event::ConquestSpells {
            winner,
            loser,
            spells,
        } => {
            let names: Vec<_> = spells
                .iter()
                .map(|&s| game.content().rules.spells.get(s).name.clone())
                .collect();
            format!(
                "{} takes {} from {}",
                player(game, *winner),
                if names.is_empty() {
                    "no spells".to_string()
                } else {
                    names.join(" and ")
                },
                player(game, *loser)
            )
        }
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
        Event::CityLooted {
            city,
            gold,
            citizens_lost,
            buildings_lost,
            ..
        } => format!(
            "{} is looted of {gold} gold; {citizens_lost} citizens and {} buildings are lost",
            city_name(game, *city),
            buildings_lost.len()
        ),
        Event::CityRazed {
            player: p, gold, ..
        } => format!(
            "{} razes the city and loots {gold} gold",
            player(game, *p)
        ),
        Event::SpellLearned {
            player: p,
            spell: s,
        } => {
            format!("{} learned {}", player(game, *p), spell(game, *s))
        }
        Event::SpellCast {
            player: p,
            spell: s,
        } => {
            format!("{} cast {}", player(game, *p), spell(game, *s))
        }
        Event::SpellFizzled {
            player: p,
            spell: s,
            city,
        } => format!(
            "{}'s {} fizzles: nightshade protects {}",
            player(game, *p),
            spell(game, *s),
            city_name(game, *city)
        ),
        Event::Dispelled { player: p, removed } => {
            format!(
                "{}'s dispel removes {removed} enchantment(s)",
                player(game, *p)
            )
        }
        Event::DispelFailed {
            player: p,
            spell: s,
        } => {
            format!("{}'s dispel of {} fails", player(game, *p), spell(game, *s))
        }
        Event::GlobalSpellRemoved {
            player: p,
            victim,
            spell: s,
        } => format!(
            "{} ends {}'s {}",
            player(game, *p),
            player(game, *victim),
            spell(game, *s)
        ),
        Event::GlobalSpellTaken {
            player: p,
            victim,
            spell: s,
        } => format!(
            "{} takes {}'s {}",
            player(game, *p),
            player(game, *victim),
            spell(game, *s)
        ),
        Event::SpellBlasted {
            player: p,
            victim,
            spell: s,
            mana,
        } => format!(
            "{} blasts {}'s {} for {mana} mana",
            player(game, *p),
            player(game, *victim),
            spell(game, *s)
        ),
        Event::PowerDrained {
            player: p,
            victim,
            amount,
        } => format!(
            "{} drains {amount} mana from {}",
            player(game, *p),
            player(game, *victim)
        ),
        Event::SkillDestroyed {
            player: p,
            victim,
            amount,
        } => format!(
            "{} destroys {amount} points of {}'s casting skill",
            player(game, *p),
            player(game, *victim)
        ),
        Event::UnitsSlain {
            player: p,
            spell: s,
            count,
        } => format!("{}'s {} slays {count} units", player(game, *p), spell(game, *s)),
        Event::UnitTransformed { from, to, .. } => format!(
            "A {} becomes {}",
            game.content().rules.units.get(*from).name,
            game.content().rules.units.get(*to).name
        ),
        Event::EarthquakeStruck {
            units_lost,
            buildings_lost,
            ..
        } => format!("An earthquake kills {units_lost} units and destroys {buildings_lost} buildings"),
        Event::StasisEnded { .. } => "A unit breaks free of stasis".to_string(),
        Event::UnitMutated { mutation, .. } => format!("A unit is mutated by chaos: {mutation:?}"),
        Event::MasteryStarted { caster } => format!(
            "{} has started casting the Spell of Mastery",
            player(game, *caster)
        ),
        Event::WorldSpellStruck {
            spell: s,
            units_lost,
            buildings_lost,
            population_lost,
            ..
        } => format!(
            "{} kills {units_lost} units and {population_lost} citizens and destroys {buildings_lost} buildings",
            spell(game, *s)
        ),
        Event::SpellSuppressed {
            player: p,
            spell: s,
            by,
        } => format!(
            "{}'s {} is stopped by {}'s Suppress Magic",
            player(game, *p),
            spell(game, *s),
            player(game, *by)
        ),
        Event::NoTargetForCast { spell: s, .. } => {
            format!(
                "There are no targets for {}: it is not paid for until there is one (`cancelcast` stops it)",
                spell(game, *s)
            )
        }
        Event::SpellReady { spell: s, .. } => {
            format!(
                "{} is paid for: choose a target with `target`",
                spell(game, *s)
            )
        }
        Event::HeroJoined { player: p, kind, .. } => {
            let name = game
                .units()
                .hero_type_name(*kind)
                .unwrap_or_else(|| "A hero".to_string());
            format!("{name} joins {}", player(game, *p))
        }
        Event::HeroSpellFailed { spell: s, .. } => {
            format!("{} finds no hero to bring", spell(game, *s))
        }
        Event::SpellHasNoEffectYet { spell: s } => {
            format!("{} has no effect yet", spell(game, *s))
        }
        Event::BattleStarted {
            attacker,
            defender,
            site,
        } => format!(
            "Battle at {}: {} attacks {}",
            pos(*site),
            player(game, *attacker),
            player(game, *defender)
        ),
        Event::Battle(e) => battle_event(game, e),
        Event::BattleEnded { winner, loser } => format!(
            "Battle over: {} beats {}",
            player(game, *winner),
            player(game, *loser)
        ),
        Event::BattleSpoils {
            player: p,
            experience,
            fame,
            ..
        } => {
            let fame = if *fame > 0 {
                format!(" and {fame} fame")
            } else {
                String::new()
            };
            format!(
                "{}'s survivors gain {experience} experience{fame}",
                player(game, *p)
            )
        }
        Event::UnitCreated { unit: u, .. } => format!("Created {}", unit(game, *u)),
        Event::UnitsMoved {
            units: us,
            from,
            to,
        } => {
            format!(
                "Units {} moved {} -> {}",
                units(game, us),
                pos(*from),
                pos(*to)
            )
        }
        Event::AttackRequested {
            attackers,
            to,
            defenders,
            ..
        } => format!(
            "Units {} attack {} (defenders {})",
            units(game, attackers),
            pos(*to),
            units(game, defenders)
        ),
        Event::SiteEncounter { units: us, to, .. } => {
            format!(
                "Units {} reach a guarded site at {}",
                units(game, us),
                pos(*to)
            )
        }
        Event::RaidersRaised { owner, count, .. } => {
            format!("{} raises {count} raiders", player(game, *owner))
        }
        Event::MonstersSettled {
            pos: at,
            site,
            count,
            evicted,
        } => format!(
            "{count} monsters settle in the {site} at {} and drive out {evicted}",
            pos(*at)
        ),
        Event::CityRampaged {
            pos: at,
            name,
            owner,
            ruined,
        } => format!(
            "Monsters {} {name} of {} at {}",
            if *ruined { "raze" } else { "wreck" },
            player(game, *owner),
            pos(*at)
        ),
        Event::SiteCleared {
            player: p,
            pos: at,
            loot,
            ..
        } => {
            format!(
                "{} cleared the site at {} ({loot:?})",
                player(game, *p),
                pos(*at)
            )
        }
        Event::OfferMade { player: p, offer: o } => {
            format!("{} is offered: {}", player(game, *p), offer(game, o))
        }
        Event::ItemSold { player: p, name, gold } => {
            format!("{} sells the item {name} for {gold} gold", player(game, *p))
        }
        Event::MercenariesHired { player: p, units } => {
            format!("{} hires {} mercenaries", player(game, *p), units.len())
        }
        Event::ItemMade { player: p, name, .. } => {
            format!("{} gets the item {name}", player(game, *p))
        }
        Event::Rejected(why) => format!("Refused: {why:?}"),
        Event::Diplomacy(d) => diplomacy(game, d),
        other => format!("{other:?}"),
    }
}

/// The spell events of a battle; the other battle events are shown plainly.
fn battle_event(game: &Game, e: &f4x_core::battle::BattleEvent) -> String {
    use f4x_core::battle::BattleEvent as B;
    match e {
        B::SpellCast {
            side,
            spell: s,
            skill,
            mana,
        } => format!(
            "{side:?} wizard casts {} (skill {skill}, mana {mana})",
            spell(game, *s)
        ),
        B::UnitSpellCast {
            unit,
            spell: s,
            mana,
        } => format!("Unit {unit} casts {} (mana {mana})", spell(game, *s)),
        B::SpellCountered { side, spell: s } => {
            format!(
                "Counter Magic stops the {side:?} wizard's {}",
                spell(game, *s)
            )
        }
        B::SpellDamaged { unit, dealt, kills } => {
            format!("Spell hits unit {unit} for {dealt}, {kills} figures lost")
        }
        B::SpellImmune { unit } => format!("Unit {unit} takes no harm from the spell"),
        B::SpellHealed { unit, healed } => format!("Unit {unit} heals {healed}"),
        B::HeroRecalled { unit } => format!("Unit {unit} is recalled to the summoning circle"),
        B::SpellEnchanted { unit, spell: s } => format!("Unit {unit} is under {}", spell(game, *s)),
        B::SpellDispelled { unit, removed, .. } => {
            format!("Dispel takes {removed} spells off unit {unit}")
        }
        B::Summoned { unit } => format!("Unit {unit} is summoned"),
        B::Raised { unit } => format!("Unit {unit} rises again as undead"),
        B::FieldDispelled { side, spell: s } => {
            format!("{side:?}'s {} is dispelled from the field", spell(game, *s))
        }
        B::FieldRaised { side, spell: s } => {
            format!("{side:?} raises {} over the field", spell(game, *s))
        }
        B::WallRaised { side, spell: s } => {
            format!("{side:?} raises {} at the city", spell(game, *s))
        }
        B::SpellResisted { unit } => format!("Unit {unit} resists the spell"),
        B::SpellSlew { unit, kills } => format!("Spell destroys {kills} figures of unit {unit}"),
        B::FieldStruck { unit, dealt, kills } => {
            format!("Unit {unit} is struck for {dealt}, {kills} figures lost")
        }
        B::Terrified { unit } => format!("Unit {unit} cowers in terror"),
        B::Confused { unit, act } => match act {
            f4x_core::battle::Confusion::Idle => format!("Unit {unit} stands about, confused"),
            f4x_core::battle::Confusion::Wandered => format!("Unit {unit} wanders off, confused"),
            f4x_core::battle::Confusion::Seized => {
                format!("Unit {unit} falls under the enemy's orders")
            }
        },
        B::ManaLeaked { side } => format!("The {side:?} wizard's mana leaks away"),
        B::CounterMagicRaised { side, strength } => {
            format!("{side:?} raises Counter Magic {strength}")
        }
        other => format!("{other:?}"),
    }
}

/// What a proposal asks, in words.
pub fn proposal(game: &Game, kind: f4x_core::diplomacy::ProposalKind) -> String {
    use f4x_core::diplomacy::ProposalKind as K;
    match kind {
        K::Pact => "a wizard pact".into(),
        K::Alliance => "an alliance".into(),
        K::Peace => "peace".into(),
        K::WarOn(t) => format!("war on {}", player(game, t)),
        K::BreakAllianceWith(t) => format!("breaking the alliance with {}", player(game, t)),
        K::Exchange { give, get } => format!(
            "a swap: their {} for {}",
            spell(game, get),
            spell(game, give)
        ),
        K::Threat => "a threat to attack".into(),
        K::Demand { gold } => format!("{gold} gold as tribute"),
    }
}

fn diplomacy(game: &Game, d: &f4x_core::diplomacy::DiplomacyEvent) -> String {
    use f4x_core::diplomacy::relation_key;
    use f4x_core::diplomacy::DiplomacyEvent as D;
    match d {
        D::ContactMade { a, b } => format!("{} and {} meet", player(game, *a), player(game, *b)),
        D::RelationChanged {
            attacker,
            defender,
            change,
            ..
        } => {
            let now = game.relation(*attacker, *defender);
            format!(
                "relations of {} with {} change by {change:+}: now {} ({now})",
                player(game, *defender),
                player(game, *attacker),
                game.content()
                    .texts
                    .get(relation_key(now), &f4x_core::text::Args::new())
            )
        }
        D::TreatyChanged { a, b, treaty } => format!(
            "{} and {}: {}",
            player(game, *a),
            player(game, *b),
            crate::talk::treaty_word(*treaty)
        ),
        D::WarDeclared { attacker, defender } => format!(
            "{} declares war on {}",
            player(game, *attacker),
            player(game, *defender)
        ),
        D::TreatyBroken {
            breaker,
            victim,
            treaty,
        } => format!(
            "{} breaks the {} with {}",
            player(game, *breaker),
            crate::talk::treaty_word(*treaty),
            player(game, *victim)
        ),
        D::ProposalMade(p) => format!(
            "{} proposes to {}: {}",
            player(game, p.from),
            player(game, p.to),
            proposal(game, p.kind)
        ),
        D::ProposalAnswered {
            proposal: p,
            accepted,
            lapsed,
        } => format!(
            "{} {} {}'s proposal of {}",
            player(game, p.to),
            match (accepted, lapsed) {
                (true, _) => "accepts",
                (false, true) => "does not answer",
                (false, false) => "refuses",
            },
            player(game, p.from),
            proposal(game, p.kind)
        ),
        D::GoldGiven { from, to, gold } => format!(
            "{} gives {} {gold} gold",
            player(game, *from),
            player(game, *to)
        ),
        D::Warned { from, to } => format!(
            "{} warns {} that its patience is running out",
            player(game, *from),
            player(game, *to)
        ),
        D::Trespassing { owner, intruder } => format!(
            "{} warns {} to keep its units away from its cities",
            player(game, *owner),
            player(game, *intruder)
        ),
        D::SpellGiven { from, to, spell: s } => format!(
            "{} gives {} {}",
            player(game, *from),
            player(game, *to),
            spell(game, *s)
        ),
    }
}

/// An item in a line: its name, kind, bonuses, powers and charges.
pub fn magic_item(game: &Game, item: &f4x_core::item::Item) -> String {
    let rules = &game.content().rules;
    let b = item.bonus;
    let mut parts: Vec<String> = [
        ("attack", b.attack),
        ("to hit", b.to_hit),
        ("defense", b.defense),
        ("half-moves", b.moves),
        ("resistance", b.resistance),
        ("spell skill", b.spell_skill),
        ("spell save", b.spell_save),
    ]
    .into_iter()
    .filter(|(_, v)| *v != 0)
    .map(|(n, v)| format!("{v:+} {n}"))
    .collect();
    parts.extend(item.powers.iter().map(|p| {
        rules
            .item_powers
            .by_name(p)
            .map_or(p.clone(), |d| d.name.clone())
    }));
    if let Some((spell, n)) = &item.charges {
        let name = rules
            .spells
            .by_name(spell)
            .map_or(spell.as_str(), |s| s.name.as_str());
        parts.push(format!("{name} x{n}"));
    }
    format!(
        "{} ({:?}, worth {}): {}",
        item.display_name(game.content()),
        item.kind,
        item.cost,
        if parts.is_empty() {
            "no powers".to_string()
        } else {
            parts.join(", ")
        }
    )
}
