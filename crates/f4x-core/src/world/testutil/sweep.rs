//! The commands of a sweep: made from what a world holds, in every pairing that makes sense and
//! some that do not, so that most of them are refused.
//!
//! Tests: through the tests that use it (class 1 here and in `f4x-master`).

use crate::item::armory::{ItemSpot, Spot};
use crate::item::crafting::{Craft, ItemDesign};
use crate::map::{Plane, Position};
use crate::player::PlayerId;
use crate::rules::{BuildingId, SpellId, UnitId};
use crate::world::{Command, World};
use f4x_def::rules::ItemKind;

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
            squares.extend(around(w, at).into_iter().take(3));
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
        out.push(Command::Task {
            player: p,
            change: crate::unit::task::Change::TidyMusters,
        });
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
        target: crate::rules::exec::Target::Player(wizard),
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
            Some(crate::unit::Standby::Done),
            Some(crate::unit::Standby::Patrol),
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
            for errand in [
                crate::unit::Errand::Scout {
                    to: Some(to),
                    avoid: Vec::new(),
                    favour: Vec::new(),
                },
                crate::unit::Errand::Meld { to },
                crate::unit::Errand::Settle { to, name: None },
                crate::unit::Errand::Reinforce { to },
            ] {
                out.push(Command::GiveOrder {
                    units: vec![u],
                    errand,
                });
            }
        }
        for &charge in &units {
            out.push(Command::GiveOrder {
                units: vec![u],
                errand: crate::unit::Errand::Escort { charge },
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
        for auto in [false, true] {
            out.push(Command::Defend {
                attackers: vec![pair[0]],
                fighters: vec![pair[1]],
                auto,
                mana: None,
            });
        }
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
        use crate::diplomacy::trade::Goods;
        use crate::diplomacy::{DiplomacyCommand as D, ProposalKind as K, Remark};
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
                item: crate::city::production::ProductionItem::Building(b),
            });
        }
        for &u in &unit_ids {
            out.push(Command::SetProduction {
                city: c,
                item: crate::city::production::ProductionItem::Unit(u),
            });
        }
        out.push(Command::SetProduction {
            city: c,
            item: crate::city::production::ProductionItem::TradeGoods,
        });
        out.push(Command::SetProduction {
            city: c,
            item: crate::city::production::ProductionItem::Housing,
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
