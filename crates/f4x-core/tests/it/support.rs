//! What the tests share: the fresh games the matrix runs over.

use f4x_core::item::armory::{ItemSpot, Spot};
use f4x_core::item::crafting::{Craft, ItemDesign};
use f4x_core::world::testutil::fresh;
use f4x_core::world::World;
use f4x_def::rules::ItemKind;

/// The saved fresh games: (computer wizards, seed). The files are `tests/saves/fresh_w*_s*.save`.
pub use f4x_core::world::testutil::FRESH_WORLDS as WORLDS;

/// Every fresh game of the matrix, with its name for a failure message.
pub fn worlds() -> Vec<(String, World)> {
    WORLDS
        .iter()
        .map(|&(w, s)| {
            let world = fresh(w, s).unwrap_or_else(|e| panic!("{e}"));
            (format!("{w} wizards, seed {s}"), world)
        })
        .collect()
}

/// [`worlds`] as games, for the tests that let time pass.
pub fn games() -> Vec<(String, Game)> {
    worlds()
        .into_iter()
        .map(|(name, world)| (name, Game::new(world)))
        .collect()
}

use f4x_core::map::Position;
use f4x_core::world::{Command, Event};
use f4x_master::Game;

/// The content the games are loaded and restored against.
pub fn content() -> f4x_def::content::Content {
    f4x_core::world::testutil::shipped()
}

/// The text of a game: what a save would write (the checksum line aside, which only follows from it).
pub fn text(w: &World) -> String {
    w.snapshot().to_text().expect("a game writes as text")
}

/// The eight squares round `at`, east and west wrapping, those off the top or bottom left out.
pub fn around(w: &World, at: Position) -> Vec<Position> {
    let map = w.map().expect("a map");
    let mut out = Vec::new();
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let y = at.y as i32 + dy;
            if (dx, dy) != (0, 0) && (0..map.height() as i32).contains(&y) {
                out.push(Position {
                    x: (at.x as i32 + dx).rem_euclid(map.width() as i32) as u32,
                    y: y as u32,
                    plane: at.plane,
                });
            }
        }
    }
    out
}

/// One round of play without an AI: every unit of the first `limit` tries one step to a
/// neighbouring square (the first the game accepts), then the turn ends. The events come back, so two
/// games played the same way can be compared. A scripted walk, not a player: it explores, draws
/// random numbers and fights, which is what the persistence tests need to have happened.
pub fn busy_round(w: &mut Game, limit: usize) -> Vec<Event> {
    let mut events = Vec::new();
    for u in w.units().all_units().into_iter().take(limit) {
        let Some(at) = w.units().unit_position(u) else {
            continue;
        };
        for to in around(w, at) {
            let out = w.apply(Command::MoveStack { units: vec![u], to });
            let refused = out.iter().any(|e| matches!(e, Event::Rejected(_)));
            events.extend(out);
            if !refused {
                break;
            }
        }
    }
    events.extend(w.play_round());
    events
}

/// The events as text with the entity handles blanked (`170v1` becomes `E`): a loaded game numbers
/// its entities afresh, so two games that are the same differ in these alone. Creation numbers (the
/// saved text) are what is compared exactly.
pub fn shape(events: &[Event]) -> String {
    let text = format!("{events:?}");
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i > start && i < chars.len() && chars[i] == 'v' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && !chars[start.saturating_sub(1)].is_alphanumeric() {
                out.push('E');
                i = j;
                continue;
            }
        }
        i = start;
        out.push(chars[i]);
        i += 1;
    }
    out
}

use f4x_core::map::Plane;
use f4x_core::player::PlayerId;
use f4x_core::rules::{BuildingId, SpellId, UnitId};

/// The commands of a game, in a fixed order that does not depend on its entity handles, so the
/// same list is made again from a game that was restored.
pub fn commands(w: &World) -> Vec<Command> {
    let rules = &w.content().rules;
    let mut out = Vec::new();

    let mut players: Vec<PlayerId> = w.player_ids();
    players.push(PlayerId(250));
    let wizard = players
        .iter()
        .copied()
        .find(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
        .expect("a wizard");

    let all = w.units().all_units();
    let step = (all.len() / 5).max(1);
    let units: Vec<_> = all.iter().copied().step_by(step).take(6).collect();
    let cities: Vec<_> = w
        .economy()
        .cities()
        .into_iter()
        .map(|c| c.0)
        .take(4)
        .collect();

    let map = w.map().expect("a map");
    let mut squares: Vec<Position> = Vec::new();
    // The squares that are not an ordinary neighbour come first, so that cutting the list keeps them:
    // a corner, a square off the east edge, one off the south edge.
    squares.push(Position {
        x: 0,
        y: 0,
        plane: Plane::Arcanus,
    });
    squares.push(Position {
        x: map.width() + 3,
        y: 1,
        plane: Plane::Arcanus,
    });
    squares.push(Position {
        x: 1,
        y: map.height() + 3,
        plane: Plane::Myrror,
    });
    for &u in &units {
        if let Some(at) = w.units().unit_position(u) {
            squares.push(at);
            squares.extend(crate::support::around(w, at).into_iter().take(3));
            squares.push(Position {
                plane: if at.plane == Plane::Arcanus {
                    Plane::Myrror
                } else {
                    Plane::Arcanus
                },
                ..at
            });
        }
    }
    squares.dedup();
    squares.truncate(10);

    let spells: Vec<SpellId> = rules
        .spells
        .defs()
        .iter()
        .map(|d| SpellId::new(d.num))
        .collect();
    let buildings: Vec<BuildingId> = rules
        .buildings
        .defs()
        .iter()
        .map(|d| BuildingId::new(d.num))
        .collect();
    let unit_ids: Vec<UnitId> = rules
        .units
        .defs()
        .iter()
        .step_by(7)
        .map(|d| UnitId::new(d.num))
        .collect();

    for &p in &players {
        out.push(Command::Resign { player: p });
        out.push(Command::CancelCast { player: p });
        out.push(Command::BalanceFarmers { player: p });
        for rate in [0u8, 3, 255] {
            out.push(Command::SetTaxRate { player: p, rate });
        }
        out.push(Command::SetGrandVizier {
            player: p,
            on: true,
        });
        for (m, r, s) in [(0u8, 0u8, 0u8), (100, 0, 0), (50, 50, 50), (255, 255, 255)] {
            out.push(Command::SetPowerSplit {
                player: p,
                mana: m,
                research: r,
                skill: s,
            });
        }
        for (amount, gold_to_mana) in [(0, true), (5, true), (-5, false), (i32::MAX, true)] {
            out.push(Command::Alchemy {
                player: p,
                amount,
                gold_to_mana,
            });
        }
        out.push(Command::ReleaseUpkeep {
            player: p,
            items: Vec::new(),
        });
        out.push(Command::ReleaseTroops {
            player: p,
            units: units.clone(),
            scrap: Vec::new(),
        });
        out.push(Command::AcceptOffer {
            player: p,
            index: 0,
            name: None,
            at: None,
        });
        out.push(Command::RejectOffer {
            player: p,
            index: 0,
        });
        for &u in &units {
            for &s in spells.iter().step_by(40) {
                out.push(Command::TurnOffUnitSpell {
                    player: p,
                    unit: u,
                    spell: s,
                });
            }
        }
        for &c in &cities {
            out.push(Command::RazeCity { player: p, city: c });
        }
        // The item screen: the vault, the award list and the slots of a unit (none is a hero, or
        // all are the wrong ones), a slot that is not there.
        let mut spots = vec![
            ItemSpot::Vault(0),
            ItemSpot::Vault(1),
            ItemSpot::Vault(40),
            ItemSpot::Award(0),
        ];
        for &u in units.iter().take(2) {
            for slot in [0, 2, 3] {
                spots.push(ItemSpot::Hero(u, slot));
            }
        }
        for &from in &spots {
            for &to in &spots {
                for pay in [false, true] {
                    out.push(Command::MoveItem {
                        player: p,
                        from,
                        to,
                        pay,
                    });
                }
            }
        }
        for from in [
            Spot::Vault(0),
            Spot::Vault(1),
            Spot::Vault(40),
            Spot::Award(0),
        ] {
            out.push(Command::ScrapItem { player: p, from });
            out.push(Command::SellItem { player: p, from });
        }
        // Designs: none, the first power of each kind of item, one of a power that is not there,
        // the same power twice, charges of a spell the wizard does not know.
        let first_power = rules.item_powers.iter().next().map(|(id, _)| id);
        for craft in [Craft::EnchantItem, Craft::CreateArtifact] {
            for kind in [
                ItemKind::Sword,
                ItemKind::Misc,
                ItemKind::Staff,
                ItemKind::Shield,
            ] {
                let design = |powers: Vec<_>, charges| ItemDesign {
                    name: String::new(),
                    kind,
                    icon: 0,
                    powers,
                    charges,
                };
                out.push(Command::CraftItem {
                    player: p,
                    craft,
                    design: design(Vec::new(), None),
                });
                if let Some(power) = first_power {
                    out.push(Command::CraftItem {
                        player: p,
                        craft,
                        design: design(vec![power], None),
                    });
                    out.push(Command::CraftItem {
                        player: p,
                        craft,
                        design: design(vec![power, power], None),
                    });
                    out.push(Command::CraftItem {
                        player: p,
                        craft,
                        design: design(vec![power], spells.first().map(|&s| (s, 9))),
                    });
                }
            }
        }
    }
    for &s in &spells {
        out.push(Command::SetResearch {
            player: wizard,
            spell: s,
        });
        out.push(Command::CastSpell {
            player: wizard,
            spell: s,
            target: None,
            extra: 0,
        });
        out.push(Command::TurnOffSpell {
            player: wizard,
            spell: s,
        });
    }
    out.push(Command::ResolveCast {
        player: wizard,
        target: f4x_core::rules::exec::Target::Player(wizard),
    });

    for &u in &units {
        out.push(Command::ClearOrders { units: vec![u] });
        out.push(Command::Purify { units: vec![u] });
        out.push(Command::Meld { units: vec![u] });
        out.push(Command::ShiftPlane { units: vec![u] });
        out.push(Command::DismissUnit { unit: u });
        out.push(Command::SetBridge { unit: u, on: true });
        out.push(Command::SettleOutpost {
            unit: u,
            name: "Nowhere".into(),
        });
        for standby in [
            None,
            Some(f4x_core::unit::Standby::Done),
            Some(f4x_core::unit::Standby::Patrol),
        ] {
            out.push(Command::SetStandby {
                units: vec![u],
                standby,
            });
        }
        for &to in &squares {
            out.push(Command::MoveStack { units: vec![u], to });
            out.push(Command::GoTo { units: vec![u], to });
            out.push(Command::EnterSite { units: vec![u], to });
            out.push(Command::EarthGate { units: vec![u], to });
            out.push(Command::BuildRoad {
                units: vec![u],
                path: vec![to],
            });
        }
    }
    out.push(Command::MoveStack {
        units: Vec::new(),
        to: squares[0],
    });
    out.push(Command::MoveStack {
        units: units.clone(),
        to: squares[0],
    });
    for pair in units.windows(2) {
        out.push(Command::BeginBattle {
            attackers: vec![pair[0]],
            defenders: vec![pair[1]],
        });
        out.push(Command::AutoResolve {
            attackers: vec![pair[0]],
            defenders: vec![pair[1]],
            mana_budget: None,
        });
    }
    out.push(Command::BeginBattle {
        attackers: Vec::new(),
        defenders: units.clone(),
    });

    for &p in &players {
        for &at in &squares {
            out.push(Command::FoundCity {
                owner: p,
                pos: at,
                name: "Newtown".into(),
                race: None,
                population: 1,
            });
            out.push(Command::PlaceFortress {
                player: p,
                pos: at,
                name: "Keep".into(),
                race: None,
                population: 1,
            });
            for &u in unit_ids.iter().take(3) {
                out.push(Command::CreateUnit {
                    owner: p,
                    pos: at,
                    unit: u,
                });
            }
        }
    }
    // Diplomacy between each pair of players, the unknown one among them.
    {
        use f4x_core::diplomacy::trade::Goods;
        use f4x_core::diplomacy::{DiplomacyCommand as D, ProposalKind as K, Remark};
        let spell = spells.first().copied().expect("a spell");
        let parties: Vec<PlayerId> = players
            .iter()
            .copied()
            .filter(|&p| p == PlayerId(250) || (!w.is_neutral_player(p) && w.wizard(p).is_some()))
            .collect();
        for &a in &parties {
            for &b in &parties {
                let kinds = [
                    K::Pact,
                    K::Alliance,
                    K::Peace,
                    K::WarOn(b),
                    K::BreakAllianceWith(b),
                    K::Trade {
                        give: Goods::gold(10),
                        get: Goods::gold(5),
                    },
                    K::Trade {
                        give: Goods::gold(i32::MAX),
                        get: Goods::gold(i32::MIN),
                    },
                    K::Threat,
                    K::Demand { gold: 0 },
                    K::Demand { gold: i32::MAX },
                    K::Demand { gold: -5 },
                ];
                let mut diplomacy = vec![
                    D::Greeted { player: a, with: b },
                    D::Answer {
                        player: a,
                        from: b,
                        accept: true,
                    },
                    D::Answer {
                        player: a,
                        from: b,
                        accept: false,
                    },
                    D::BreakTreaty { player: a, with: b },
                    D::DeclareWar { player: a, on: b },
                    D::GiveSpell {
                        from: a,
                        to: b,
                        spell,
                    },
                    D::Warn { from: a, to: b },
                    D::Remark {
                        from: a,
                        to: b,
                        why: Remark::Coalition,
                    },
                ];
                for amount in [0, 5, -5, i32::MAX, i32::MIN] {
                    diplomacy.push(D::GiveGold {
                        from: a,
                        to: b,
                        amount,
                    });
                }
                for kind in kinds {
                    diplomacy.push(D::Propose {
                        from: a,
                        to: b,
                        kind,
                    });
                    diplomacy.push(D::Counter {
                        player: a,
                        from: b,
                        kind,
                    });
                }
                out.extend(diplomacy.into_iter().map(Command::Diplomacy));
            }
        }
    }
    for &c in &cities {
        out.push(Command::BuyProduction { city: c });
        out.push(Command::RenameCity {
            city: c,
            name: String::new(),
        });
        out.push(Command::SetAutoBuild {
            city: c,
            plan: None,
        });
        for farmers in [0u32, 1, 9999] {
            out.push(Command::SetFarmers { city: c, farmers });
        }
        for &b in buildings.iter().step_by(3) {
            out.push(Command::SellBuilding {
                city: c,
                building: b,
            });
            out.push(Command::SetProduction {
                city: c,
                item: f4x_core::city::production::ProductionItem::Building(b),
            });
        }
        for &u in &unit_ids {
            out.push(Command::SetProduction {
                city: c,
                item: f4x_core::city::production::ProductionItem::Unit(u),
            });
        }
        out.push(Command::SetProduction {
            city: c,
            item: f4x_core::city::production::ProductionItem::TradeGoods,
        });
        out.push(Command::SetProduction {
            city: c,
            item: f4x_core::city::production::ProductionItem::Housing,
        });
        for &s in spells.iter().step_by(40) {
            out.push(Command::TurnOffCitySpell {
                player: wizard,
                city: c,
                spell: s,
            });
        }
    }
    out
}

/// A city of `owner` on the first square near their first city that the game lets one be founded on.
pub fn another_city(
    w: &mut World,
    owner: f4x_core::player::PlayerId,
    population: u32,
) -> Option<hecs::Entity> {
    let (first, at) = w.economy().cities_of(owner).first().copied()?;
    let race = w.economy().city_race(first);
    for dx in -6i32..=6 {
        for dy in -6i32..=6 {
            let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
            if x < 0 || y < 0 {
                continue;
            }
            let pos = Position {
                x: x as u32,
                y: y as u32,
                plane: at.plane,
            };
            let events = w.apply(Command::FoundCity {
                owner,
                pos,
                name: "Another".to_owned(),
                race,
                population,
            });
            if events.iter().all(|e| !matches!(e, Event::Rejected(_))) {
                return w.economy().city_at(pos);
            }
        }
    }
    None
}
