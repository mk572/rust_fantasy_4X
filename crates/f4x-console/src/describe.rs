//! Words for things: units, cities, positions and events, as the console prints them.
//!
//! Tests: in `tests.rs`, through the console.

use f4x_core::city::production::ProductionItem;
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;

use f4x_core::world::{Event, World};

pub fn plane_letter(p: Plane) -> char {
    match p {
        Plane::Arcanus => 'a',
        Plane::Myrror => 'm',
    }
}

pub fn pos(p: Position) -> String {
    format!("{},{},{}", p.x, p.y, plane_letter(p.plane))
}

pub fn player(world: &World, id: PlayerId) -> String {
    if let Some(p) = world.player(id).filter(|_| world.has_player(id)) {
        format!("{} ({})", p.name(), id.0)
    } else {
        format!("player {}", id.0)
    }
}

pub fn unit(world: &World, u: hecs::Entity) -> String {
    let name = world.units().unit_def(u).map_or("?", |d| d.id.as_str());
    let at = world
        .units()
        .unit_position(u)
        .map_or(String::new(), |p| format!(" at {}", pos(p)));
    format!("unit {} {name}{at}", world.units().serial_of(u).0)
}

pub fn city_name(world: &World, c: hecs::Entity) -> String {
    let name = world
        .economy()
        .cities()
        .into_iter()
        .find(|(e, ..)| *e == c)
        .map_or("?".to_string(), |(_, c, _)| c.name);
    format!("city {} {name}", world.units().serial_of(c).0)
}

pub fn item(world: &World, item: ProductionItem) -> String {
    let rules = &world.content().rules;
    match item {
        ProductionItem::Building(b) => rules.buildings.get(b).id.clone(),
        ProductionItem::Unit(u) => rules.units.get(u).id.clone(),
        ProductionItem::TradeGoods => "Trade Goods".into(),
        ProductionItem::Housing => "Housing".into(),
    }
}

pub fn spell(world: &World, s: f4x_def::rules::SpellId) -> String {
    world.content().rules.spells.get(s).id.clone()
}

fn units(world: &World, list: &[hecs::Entity]) -> String {
    list.iter()
        .map(|&u| world.units().serial_of(u).0.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// One line for an offer waiting for an answer.
pub fn offer(world: &World, o: &f4x_core::player::offer::Offer) -> String {
    use f4x_core::player::offer::Offer;
    let rules = &world.content().rules;
    match o {
        Offer::Hero { kind, price } => {
            let name = rules
                .hero_templates
                .get(*kind)
                .and_then(|t| rules.units.try_get(t.unit))
                .map_or("a hero", |u| u.id.as_str());
            format!("{name} asks {price} gold to join you")
        }
        Offer::Summoned { kind, .. } => {
            let name = rules
                .hero_templates
                .get(*kind)
                .and_then(|t| rules.units.try_get(t.unit))
                .map_or("a hero", |u| u.id.as_str());
            format!("{name} is summoned and waits to be accepted")
        }
        Offer::Mercenaries {
            unit,
            count,
            level,
            price,
        } => {
            let name = rules
                .units
                .try_get(*unit)
                .map_or("a unit", |u| u.id.as_str());
            format!("{count} {name} of level {} for {price} gold", level + 1)
        }
        Offer::Item { item, price } => {
            format!("the item {} for {price} gold", item_label(world, item))
        }
        Offer::Resale { item, price, .. } => format!(
            "the item {} for {price} gold, sold by another wizard",
            item_label(world, item)
        ),
    }
}

/// One line for an event.
pub fn event(world: &World, e: &Event) -> String {
    match e {
        Event::TurnStarted { turn } => format!("Turn {turn} begins"),
        Event::CityFounded { city, pos: p } => {
            format!("Founded {} at {}", city_name(world, *city), pos(*p))
        }
        Event::ProductionCompleted { city, item: i } => {
            format!("{} finished {}", city_name(world, *city), item(world, *i))
        }
        Event::ProductionBlocked { city, item: i } => {
            format!(
                "{} cannot finish {} yet: the city is full or too small",
                city_name(world, *city),
                item(world, *i)
            )
        }
        Event::ProductionImpossible { city, item: i } => format!(
            "{} can no longer produce {}",
            city_name(world, *city),
            item(world, *i)
        ),
        Event::ChaosRiftStrikes {
            city,
            units_lost,
            building,
        } => {
            let mut line = format!("The chaos rift over {} strikes", city_name(world, *city));
            if *units_lost > 0 {
                line += &format!(": {units_lost} unit(s) destroyed");
            }
            if let Some(b) = building {
                line += &format!(", {} falls", world.content().rules.buildings.get(*b).id);
            }
            line
        }
        Event::CityGrew { city, figures } => {
            format!("{} grew to {figures}", city_name(world, *city))
        }
        Event::CityShrank { city, figures } => {
            format!("{} shrank to {figures}", city_name(world, *city))
        }
        Event::UnitsDisbanded { player: p, units } => {
            format!("{} disbanded {units} units", player(world, *p))
        }
        Event::ShipLanded {
            player: p,
            units,
            at,
        } => format!(
            "a ship of {} landed {units} units at {}",
            player(world, *p),
            pos(*at)
        ),
        Event::ShipStranded {
            player: p,
            units,
            at,
        } => format!(
            "a ship of {} was destroyed on land at {}, leaving {units} units",
            player(world, *p),
            pos(*at)
        ),
        Event::FortressPlaced { player: p, pos: at } => {
            format!("{} placed a fortress at {}", player(world, *p), pos(*at))
        }
        Event::WizardBanished { player: p, by } => {
            format!("{} is banished by {}", player(world, *p), player(world, *by))
        }
        Event::WizardReturned { player: p, .. } => format!("{} has returned", player(world, *p)),
        Event::ConquestSpells {
            winner,
            loser,
            spells,
        } => {
            let names: Vec<_> = spells
                .iter()
                .map(|&s| world.content().rules.spells.get(s).id.clone())
                .collect();
            format!(
                "{} takes {} from {}",
                player(world, *winner),
                if names.is_empty() {
                    "no spells".to_string()
                } else {
                    names.join(" and ")
                },
                player(world, *loser)
            )
        }
        Event::WizardDefeated { player: p } => format!("{} is defeated", player(world, *p)),
        Event::GameOver { winner, kind, turn } => format!(
            "GAME OVER: {} wins by {kind:?} on turn {turn}",
            player(world, *winner)
        ),
        Event::CityCaptured { city, from, to } => format!(
            "{} taken from {} by {}",
            city_name(world, *city),
            player(world, *from),
            player(world, *to)
        ),
        Event::CityLooted {
            city,
            gold,
            citizens_lost,
            buildings_lost,
            ..
        } => format!(
            "{} is looted of {gold} gold; {citizens_lost} citizens and {} buildings are lost",
            city_name(world, *city),
            buildings_lost.len()
        ),
        Event::CityRazed {
            player: p, gold, ..
        } => format!(
            "{} razes the city and loots {gold} gold",
            player(world, *p)
        ),
        Event::SpellLearned {
            player: p,
            spell: s,
        } => {
            format!("{} learned {}", player(world, *p), spell(world, *s))
        }
        Event::SpellCast {
            player: p,
            spell: s,
        } => {
            format!("{} cast {}", player(world, *p), spell(world, *s))
        }
        Event::SpellFizzled {
            player: p,
            spell: s,
            city,
        } => format!(
            "{}'s {} fizzles: nightshade protects {}",
            player(world, *p),
            spell(world, *s),
            city_name(world, *city)
        ),
        Event::Dispelled { player: p, removed } => {
            format!(
                "{}'s dispel removes {removed} enchantment(s)",
                player(world, *p)
            )
        }
        Event::DispelFailed {
            player: p,
            spell: s,
        } => {
            format!("{}'s dispel of {} fails", player(world, *p), spell(world, *s))
        }
        Event::GlobalSpellRemoved {
            player: p,
            victim,
            spell: s,
        } => format!(
            "{} ends {}'s {}",
            player(world, *p),
            player(world, *victim),
            spell(world, *s)
        ),
        Event::GlobalSpellTaken {
            player: p,
            victim,
            spell: s,
        } => format!(
            "{} takes {}'s {}",
            player(world, *p),
            player(world, *victim),
            spell(world, *s)
        ),
        Event::SpellBlasted {
            player: p,
            victim,
            spell: s,
            mana,
        } => format!(
            "{} blasts {}'s {} for {mana} mana",
            player(world, *p),
            player(world, *victim),
            spell(world, *s)
        ),
        Event::PowerDrained {
            player: p,
            victim,
            amount,
        } => format!(
            "{} drains {amount} mana from {}",
            player(world, *p),
            player(world, *victim)
        ),
        Event::SkillDestroyed {
            player: p,
            victim,
            amount,
        } => format!(
            "{} destroys {amount} points of {}'s casting skill",
            player(world, *p),
            player(world, *victim)
        ),
        Event::UnitsSlain {
            player: p,
            spell: s,
            count,
        } => format!("{}'s {} slays {count} units", player(world, *p), spell(world, *s)),
        Event::UnitTransformed { from, to, .. } => format!(
            "A {} becomes {}",
            world.content().rules.units.get(*from).id,
            world.content().rules.units.get(*to).id
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
            player(world, *caster)
        ),
        Event::WorldSpellStruck {
            spell: s,
            units_lost,
            buildings_lost,
            population_lost,
            ..
        } => format!(
            "{} kills {units_lost} units and {population_lost} citizens and destroys {buildings_lost} buildings",
            spell(world, *s)
        ),
        Event::SpellSuppressed {
            player: p,
            spell: s,
            by,
            counter,
        } => format!(
            "{}'s {} is stopped by {}'s {}",
            player(world, *p),
            spell(world, *s),
            player(world, *by),
            spell(world, *counter)
        ),
        Event::SpellFailed {
            player: p,
            spell: s,
            reason,
            refunded,
        } => format!(
            "{}'s {} fails ({reason:?}); {refunded} mana returned",
            player(world, *p),
            spell(world, *s)
        ),
        Event::NoTargetForCast { spell: s, .. } => {
            format!(
                "There are no targets for {}: it is not paid for until there is one (`cancelcast` stops it)",
                spell(world, *s)
            )
        }
        Event::SpellReady { spell: s, .. } => {
            format!(
                "{} is paid for: choose a target with `target`",
                spell(world, *s)
            )
        }
        Event::HeroJoined { player: p, kind, .. } => {
            let name = world
                .units()
                .hero_type_name(*kind)
                .unwrap_or_else(|| "A hero".to_string());
            format!("{name} joins {}", player(world, *p))
        }
        Event::HeroSpellFailed { spell: s, .. } => {
            format!("{} finds no hero to bring", spell(world, *s))
        }
        Event::SpellHasNoEffectYet { spell: s } => {
            format!("{} has no effect yet", spell(world, *s))
        }
        Event::BattleStarted {
            attacker,
            defender,
            site,
        } => format!(
            "Battle at {}: {} attacks {}",
            pos(*site),
            player(world, *attacker),
            player(world, *defender)
        ),
        Event::Battle(e) => battle_event(world, e),
        Event::BattleEnded { winner, loser } => format!(
            "Battle over: {} beats {}",
            player(world, *winner),
            player(world, *loser)
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
                player(world, *p)
            )
        }
        Event::UnitCreated { unit: u, .. } => format!("Created {}", unit(world, *u)),
        Event::UnitsMoved {
            units: us,
            from,
            to,
        } => {
            format!(
                "Units {} moved {} -> {}",
                units(world, us),
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
            units(world, attackers),
            pos(*to),
            units(world, defenders)
        ),
        Event::SiteEncounter { units: us, to, .. } => {
            format!(
                "Units {} reach a guarded site at {}",
                units(world, us),
                pos(*to)
            )
        }
        Event::RaidersRaised { owner, count, .. } => {
            format!("{} raises {count} raiders", player(world, *owner))
        }
        Event::SeaRaidLanded { owner, at, count } => format!(
            "{} lands {count} raiders and a settler from the sea at {}",
            player(world, *owner),
            pos(*at)
        ),
        Event::MonstersSettled {
            pos: at,
            site,
            count,
            evicted,
        } => format!(
            "{count} monsters settle in the {} at {} and drive out {evicted}",
            world.content()
                .features
                .get(*site)
                .map_or("site", |d| d.id.as_str()),
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
            player(world, *owner),
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
                player(world, *p),
                pos(*at)
            )
        }
        Event::HeroSummoned { player: p, kind } => {
            let name = world
                .units()
                .hero_type_name(*kind)
                .unwrap_or_else(|| "A hero".to_string());
            format!("{name} is summoned for {}: accept or reject it", player(world, *p))
        }
        Event::OfferMade { player: p, offer: o } => {
            format!("{} is offered: {}", player(world, *p), offer(world, o))
        }
        Event::ItemSold { player: p, item, gold } => {
            let name = item_label(world, item);
            format!("{} sells the item {name} for {gold} gold", player(world, *p))
        }
        Event::ItemScrapped { player: p, item, mana } => {
            let name = item_label(world, item);
            format!("{} breaks up the item {name} for {mana} mana", player(world, *p))
        }
        Event::MercenariesHired { player: p, units } => {
            format!("{} hires {} mercenaries", player(world, *p), units.len())
        }
        Event::ItemMade { player: p, item, .. } => {
            let name = item_label(world, item);
            format!("{} gets the item {name}", player(world, *p))
        }
        Event::Rejected(why) => format!("Refused: {why:?}"),
        Event::Diplomacy(d) => diplomacy(world, d),
        other => format!("{other:?}"),
    }
}

/// The spell events of a battle; the other battle events are shown plainly.
fn battle_event(world: &World, e: &f4x_battle::battle::BattleEvent) -> String {
    use f4x_battle::battle::BattleEvent as B;
    match e {
        B::SpellCast {
            side,
            spell: s,
            skill,
            mana,
        } => format!(
            "{side:?} wizard casts {} (skill {skill}, mana {mana})",
            spell(world, *s)
        ),
        B::UnitSpellCast {
            unit,
            spell: s,
            mana,
        } => format!("Unit {unit} casts {} (mana {mana})", spell(world, *s)),
        B::SpellCountered { side, spell: s } => {
            format!(
                "Counter Magic stops the {side:?} wizard's {}",
                spell(world, *s)
            )
        }
        B::SpellDamaged { unit, dealt, kills } => {
            format!("Spell hits unit {unit} for {dealt}, {kills} figures lost")
        }
        B::SpellImmune { unit, by: Some(by) } => {
            format!("Unit {unit} takes no harm from the spell: immune to {by:?}")
        }
        B::SpellImmune { unit, by: None } => format!("Unit {unit} takes no harm from the spell"),
        B::Unharmed {
            attacker,
            defender,
            by,
        } => format!(
            "Unit {defender} is immune to {by:?}: the attack of unit {attacker} does nothing"
        ),
        B::SpellHealed { unit, healed } => format!("Unit {unit} heals {healed}"),
        B::Merged { unit } => format!("Unit {unit} sinks into the ground"),
        B::Surfaced { unit, cell } => format!("Unit {unit} comes up at {},{}", cell.0, cell.1),
        B::HeroRecalled { unit } => format!("Unit {unit} is recalled to the summoning circle"),
        B::SpellEnchanted { unit, spell: s } => {
            format!("Unit {unit} is under {}", spell(world, *s))
        }
        B::SpellDispelled { unit, removed, .. } => {
            format!("Dispel takes {removed} spells off unit {unit}")
        }
        B::Summoned { unit } => format!("Unit {unit} is summoned"),
        B::Raised { unit } => format!("Unit {unit} rises again as undead"),
        B::FieldDispelled { side, spell: s } => {
            format!(
                "{side:?}'s {} is dispelled from the field",
                spell(world, *s)
            )
        }
        B::FieldRaised { side, spell: s } => {
            format!("{side:?} raises {} over the field", spell(world, *s))
        }
        B::WallRaised { side, spell: s } => {
            format!("{side:?} raises {} at the city", spell(world, *s))
        }
        B::SpellResisted { unit } => format!("Unit {unit} resists the spell"),
        B::SpellSlew { unit, kills } => format!("Spell destroys {kills} figures of unit {unit}"),
        B::FieldStruck { unit, dealt, kills } => {
            format!("Unit {unit} is struck for {dealt}, {kills} figures lost")
        }
        B::Terrified { unit } => format!("Unit {unit} cowers in terror"),
        B::WebStruck {
            unit,
            dealt,
            broken,
        } => {
            if *broken {
                format!("Unit {unit} tears the web apart")
            } else {
                format!("Unit {unit} tears at the web for {dealt}")
            }
        }
        B::Confused { unit, act } => match act {
            f4x_battle::Confusion::Idle => format!("Unit {unit} stands about, confused"),
            f4x_battle::Confusion::Wandered => format!("Unit {unit} wanders off, confused"),
            f4x_battle::Confusion::Seized => {
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

/// A bundle of goods in words: "100 gold, the spell Haste, the item Sword of Doom".
pub fn goods(world: &World, goods: f4x_core::diplomacy::Goods) -> String {
    let mut parts = Vec::new();
    if goods.gold > 0 {
        parts.push(format!("{} gold", goods.gold));
    }
    if let Some(s) = goods.spell {
        parts.push(format!("the spell {}", spell(world, s)));
    }
    if goods.item.is_some() {
        parts.push("an item".to_string());
    }
    if goods.unit.is_some() {
        parts.push("a unit".to_string());
    }
    if goods.hero.is_some() {
        parts.push("a hero".to_string());
    }
    if parts.is_empty() {
        "nothing".into()
    } else {
        parts.join(", ")
    }
}

/// What a proposal asks, in words.
pub fn proposal(world: &World, kind: f4x_core::diplomacy::ProposalKind) -> String {
    use f4x_core::diplomacy::ProposalKind as K;
    match kind {
        K::Pact => "a wizard pact".into(),
        K::Alliance => "an alliance".into(),
        K::Peace => "peace".into(),
        K::WarOn(t) => format!("war on {}", player(world, t)),
        K::BreakAllianceWith(t) => format!("breaking the alliance with {}", player(world, t)),
        K::Trade { give, get } => format!(
            "a trade: their {} for {}",
            goods(world, get),
            goods(world, give)
        ),
        K::Threat => "a threat to attack".into(),
        K::Demand { gold } => format!("{gold} gold as tribute"),
    }
}

fn diplomacy(world: &World, d: &f4x_core::diplomacy::DiplomacyEvent) -> String {
    use f4x_core::diplomacy::DiplomacyEvent as D;
    match d {
        D::ContactMade { a, b } => format!("{} and {} meet", player(world, *a), player(world, *b)),
        D::TreatyChanged { a, b, treaty } => format!(
            "{} and {}: {}",
            player(world, *a),
            player(world, *b),
            crate::talk::treaty_word(*treaty)
        ),
        D::WarDeclared { attacker, defender } => format!(
            "{} declares war on {}",
            player(world, *attacker),
            player(world, *defender)
        ),
        D::TreatyBroken {
            breaker,
            victim,
            treaty,
        } => format!(
            "{} breaks the {} with {}",
            player(world, *breaker),
            crate::talk::treaty_word(*treaty),
            player(world, *victim)
        ),
        D::ProposalMade(p) => format!(
            "{} proposes to {}: {}",
            player(world, p.from),
            player(world, p.to),
            proposal(world, p.kind)
        ),
        D::ProposalCountered { old, new } => format!(
            "{} answers {}'s proposal of {} with: {}",
            player(world, new.from),
            player(world, old.from),
            proposal(world, old.kind),
            proposal(world, new.kind)
        ),
        D::ProposalAnswered {
            proposal: p,
            accepted,
            lapsed,
        } => format!(
            "{} {} {}'s proposal of {}",
            player(world, p.to),
            match (accepted, lapsed) {
                (true, _) => "accepts",
                (false, true) => "does not answer",
                (false, false) => "refuses",
            },
            player(world, p.from),
            proposal(world, p.kind)
        ),
        D::GoldGiven { from, to, gold } => format!(
            "{} gives {} {gold} gold",
            player(world, *from),
            player(world, *to)
        ),
        D::Warned { from, to } => format!(
            "{} warns {} that its patience is running out",
            player(world, *from),
            player(world, *to)
        ),
        D::Remarked { from, to, why } => format!(
            "{} tells {} why: {why:?}",
            player(world, *from),
            player(world, *to)
        ),
        D::Trespassing { owner, intruder } => format!(
            "{} warns {} to keep its units away from its cities",
            player(world, *owner),
            player(world, *intruder)
        ),
        D::SpellGiven { from, to, spell: s } => format!(
            "{} gives {} {}",
            player(world, *from),
            player(world, *to),
            spell(world, *s)
        ),
        D::BattleLosses {
            attacker,
            defender,
            heroes,
            others,
            ..
        } => format!(
            "{} holds the losses of {} heroes and {} other units against {}",
            player(world, *defender),
            heroes,
            others,
            player(world, *attacker)
        ),
        D::AssaultFailed {
            attacker,
            defender,
            fortress,
        } => format!(
            "{}'s assault on {}'s {} failed",
            player(world, *attacker),
            player(world, *defender),
            if *fortress { "fortress city" } else { "city" }
        ),
        D::FortressTaken { winner, loser } => format!(
            "{} took the fortress of {}",
            player(world, *winner),
            player(world, *loser)
        ),
        D::WizardDestroyed { winner, loser } => format!(
            "{} destroyed {}",
            player(world, *winner),
            player(world, *loser)
        ),
        D::EnchantmentCast { caster, spell: s } => format!(
            "the other wizards take note of {} by {}",
            spell(world, *s),
            player(world, *caster)
        ),
        D::CitySpellCast {
            caster,
            owner,
            spell: s,
        } => format!(
            "{} takes note of {} by {} on its city",
            player(world, *owner),
            spell(world, *s),
            player(world, *caster)
        ),
        D::Subverted { caster, target } => format!(
            "{} subverts the standing of {}",
            player(world, *caster),
            player(world, *target)
        ),
    }
}

/// An item in a line: its name, kind, bonuses, powers and charges.
pub fn magic_item(world: &World, item: &f4x_core::item::Item) -> String {
    let rules = &world.content().rules;
    let b = item.bonus;
    let mut parts: Vec<String> = [
        ("attack", b.attack),
        ("to hit", b.to_hit),
        ("defense", b.defense),
        ("moves", b.moves.floor()),
        ("resistance", b.resistance),
        ("spell skill", b.spell_skill),
        ("spell save", b.spell_save),
    ]
    .into_iter()
    .filter(|(_, v)| *v != 0)
    .map(|(n, v)| format!("{v:+} {n}"))
    .collect();
    parts.extend(item.powers.iter().map(|&p| {
        rules
            .item_powers
            .try_get(p)
            .map_or_else(|| format!("power {}", p.num()), |d| d.id.clone())
    }));
    if let Some((spell, n)) = &item.charges {
        let name = rules
            .spells
            .try_get(*spell)
            .map_or_else(|| format!("spell {}", spell.num()), |s| s.id.clone());
        parts.push(format!("{name} x{n}"));
    }
    format!(
        "{} ({:?}, worth {}): {}",
        item_label(world, item),
        item.kind,
        item.cost,
        if parts.is_empty() {
            "no powers".to_string()
        } else {
            parts.join(", ")
        }
    )
}

/// An item as the console shows it: the name it was given, else the alias of the prefab item it
/// copies, else "made item" (the console shows aliases and numbers; the client names things).
fn item_label(world: &World, item: &f4x_core::item::Item) -> String {
    if !item.name.is_empty() {
        return item.name.clone();
    }
    match item.prefab {
        Some(id) => world.content().rules.items.get(id).id.clone(),
        None => "made item".to_string(),
    }
}
