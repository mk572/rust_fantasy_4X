//! Class 9, the sites: lairs, magic nodes and towers. Who guards a site and what it holds, what a
//! fight for it changes, melding and node power, and who owns a tower (`docs/lairs.md`). The
//! numbers are the content's (`lairs.ron`, `units.ron`, `features.ron`).
//!
//! The sites are put on the fresh worlds by `cheat_add_lair`, which makes them as a new world does.
//! Battles are decided by felling a side, so what a test says does not hang on how a battle goes.

use std::collections::{BTreeSet, HashMap};

use crate::support::{around, state, worlds, WORLDS};
use f4x_battle::battle::{BattleCommand, Side};
use f4x_core::feature::def::FeatureKind;
use f4x_core::feature::entity::{LairInfo, NodeInfo};
use f4x_core::feature::lair::create::{create, node_budget, site_budget, SiteType};
use f4x_core::feature::lair::{Loot, SiteError};
use f4x_core::feature::node::MeldError;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{FeatureId, Plane, Position};
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::rules::exec::Target;
use f4x_core::rules::lair::LairStyle;
use f4x_core::rules::{RealmId, SpellId, UnitId, UnitKind};
use f4x_core::wizard::magic::Retort;
use f4x_core::world::testutil::{fresh, Table};
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::rules::Stat;
use f4x_def::testdir::{deep, sample};
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

const SPEARMEN: u16 = 104; // high_men_spearmen
const MAGIC_SPIRIT: u16 = 154; // magic_spirit
const GUARDIAN_SPIRIT: u16 = 176; // guardian_spirit
const NATURE_AWARENESS: u16 = 34; // nature_awareness
const NODE_MASTERY: u16 = 12; // node_mastery
const RAISE_VOLCANO: u16 = 98; // raise_volcano
const ARCANE: u16 = 5; // arcane
const LIFE: u16 = 3; // life
const DEATH: u16 = 4; // death

/// A world with an empty field for a site and a square beside it for the army that goes there.
struct Scene {
    name: String,
    w: Table,
    p: PlayerId,
    other: PlayerId,
    from: Position,
    site: Position,
}

/// Whether a square is open grassland: no unit, city or feature.
fn open(w: &World, at: Position) -> bool {
    w.economy().terrain_class_at(at) == Some(TerrainClass::Grassland)
        && w.units().units_at(at).is_empty()
        && w.economy().city_at(at).is_none()
        && w.economy().feature_entity_at(at).is_none()
}

fn scene(name: String, w: Table) -> Option<Scene> {
    let ids = w.wizards();
    let (p, other) = (ids[0], ids[1]);
    let home = w.economy().cities_of(p).first()?.1;
    let near = (-10i32..=10).flat_map(|dy| (-10i32..=10).map(move |dx| (dx, dy)));
    let (site, from) = near.into_iter().find_map(|(dx, dy)| {
        let site = Position {
            x: u32::try_from(home.x as i32 + dx).ok()?,
            y: u32::try_from(home.y as i32 + dy).ok()?,
            plane: home.plane,
        };
        if !open(&w, site) {
            return None;
        }
        let from = around(&w, site).into_iter().find(|&q| open(&w, q))?;
        Some((site, from))
    })?;
    Some(Scene {
        name,
        w,
        p,
        other,
        from,
        site,
    })
}

fn scenes() -> Vec<Scene> {
    let all: Vec<Scene> = worlds()
        .into_iter()
        .filter_map(|(name, w)| scene(name, w))
        .collect();
    assert!(!all.is_empty(), "no world had room for a site");
    all
}

/// The first scene afresh, with the generator moved on `skip` draws so that what is drawn next
/// differs from one `skip` to another.
fn scene_at(skip: usize) -> Scene {
    let (wizards, seed) = WORLDS[0];
    let mut w = fresh(wizards, seed).expect("a fresh game");
    for _ in 0..skip {
        w.next_u64();
    }
    scene(format!("{wizards} wizards, seed {seed}, draw {skip}"), w).expect("room for a site")
}

/// The features of a kind that holds a lair made on demand.
fn site_features(w: &World, kind: FeatureKind) -> Vec<(FeatureId, bool)> {
    let features = &w.content().features;
    features
        .defs()
        .iter()
        .filter(|d| d.kind == kind)
        .map(|d| (features.id_numbered(d.num).expect("a feature"), d.holy))
        .collect()
}

fn encounter(w: &World) -> FeatureId {
    site_features(w, FeatureKind::Encounter)[0].0
}

fn tower(w: &World) -> FeatureId {
    site_features(w, FeatureKind::Tower)[0].0
}

fn put_lair(w: &mut Table, at: Position, feature: FeatureId, strong: bool) -> LairInfo {
    w.cheat_add_lair(at, feature, strong).expect("a lair");
    w.economy().lair_at(at).expect("the lair put there")
}

/// What the guards of a lair cost together.
fn guard_value(w: &World, lair: &LairInfo) -> u32 {
    lair.guards
        .iter()
        .map(|&(u, n)| w.content().rules.units.get(u).cost * n)
        .sum()
}

fn treasure_value(lair: &LairInfo) -> u32 {
    lair.gold + lair.mana + lair.items.iter().sum::<u32>()
}

/// The strongest ordinary unit that walks, by price.
fn soldier(w: &World) -> UnitId {
    w.content()
        .rules
        .units
        .defs()
        .iter()
        .filter(|d| d.kind == UnitKind::Normal)
        .filter(|d| d.stats.get(&Stat::Transport).map_or(0, |v| v.floor()) == 0)
        .max_by_key(|d| d.cost)
        .map(|d| UnitId::new(d.num))
        .expect("a soldier")
}

fn create_at(w: &mut Table, owner: PlayerId, pos: Position, unit: UnitId) -> hecs::Entity {
    let before = w.units().units_at(pos);
    let events = w.apply(Command::CreateUnit { owner, pos, unit });
    let made = w
        .units()
        .units_at(pos)
        .into_iter()
        .find(|u| !before.contains(u));
    made.unwrap_or_else(|| panic!("no unit was made: {events:?}"))
}

fn army(s: &mut Scene) -> Vec<hecs::Entity> {
    let kind = soldier(&s.w);
    (0..3)
        .map(|_| create_at(&mut s.w, s.p, s.from, kind))
        .collect()
}

/// Every unit of `side` falls, and the next command lets the battle see it.
fn wipe_out(w: &mut Table, side: Side) -> Vec<Event> {
    w.scenario_edit_battle(|b| {
        for u in b.units.iter_mut().filter(|u| u.side == side) {
            u.kill();
            u.status = f4x_battle::unit::Status::Dead;
        }
    });
    w.apply(Command::Battle(BattleCommand::EndTurn))
}

fn refusal(events: &[Event]) -> Option<&Rejection> {
    events.iter().find_map(|e| match e {
        Event::Rejected(why) => Some(why),
        _ => None,
    })
}

fn cleared(events: &[Event]) -> Option<(bool, Loot)> {
    events.iter().find_map(|e| match e {
        Event::SiteCleared { guarded, loot, .. } => Some((*guarded, loot.clone())),
        _ => None,
    })
}

// ---- what a new site holds ----

#[test]
fn a_new_site_is_guarded_by_creatures_of_one_realm_its_kind_allows() {
    let draws = sample(400, 60);
    let mut s = scene_at(0);
    let rules = s.w.content().rules.lairs.clone();
    let mut features = site_features(&s.w, FeatureKind::Encounter);
    features.extend(site_features(&s.w, FeatureKind::Tower));
    for stronger in [false, true] {
        s.w.scenario_change_settings(|g| g.rules.stronger_lairs = stronger);
        let max_first = if stronger {
            rules.patch_150.max_first_guards
        } else {
            rules.max_first_guards
        };
        // The realms seen, by the list of realms the site's kind allows.
        let mut seen: HashMap<&str, BTreeSet<u16>> = HashMap::new();
        for &(feature, holy) in &features {
            let is_tower =
                s.w.content().features.get(feature).map(|d| d.kind) == Some(FeatureKind::Tower);
            let (list, weights) = match (is_tower, holy) {
                (true, _) => ("tower", &rules.tower_realms),
                (false, true) => ("holy", &rules.holy_realms),
                (false, false) => ("wild", &rules.wild_realms),
            };
            for i in 0..draws {
                let at = Position {
                    plane: Plane::all().nth(i % 2).expect("two planes"),
                    ..s.site
                };
                let lair = put_lair(&mut s.w, at, feature, i % 4 < 2);
                let what = format!("feature {feature:?}, draw {i}, stronger {stronger}: {lair:?}");
                assert!(lair.intact, "{what}");
                assert!(lair.fought_by.is_empty(), "{what}");
                assert!(lair.guard_count() <= rules.max_guards, "{what}");
                assert!(lair.guards.len() <= 2, "{what}: two kinds at most");
                assert!(lair.items.len() <= rules.max_items, "{what}");
                if let [first, rest @ ..] = &lair.guards[..] {
                    assert!(first.1 as i32 <= max_first, "{what}: the first kind");
                    assert!(
                        rest.iter().all(|g| g.0 != first.0),
                        "{what}: one kind twice"
                    );
                }
                let units = &s.w.content().rules.units;
                let realms: BTreeSet<u16> = lair
                    .guards
                    .iter()
                    .map(|&(u, n)| {
                        let d = units.get(u);
                        assert!(n > 0, "{what}");
                        assert_eq!(d.kind, UnitKind::Summoned, "{what}");
                        let carries = d.stats.get(&Stat::Transport).map_or(0, |v| v.floor());
                        assert_eq!(carries, 0, "{what}: a transport on guard");
                        d.realm.expect("a summoned creature has a realm").num()
                    })
                    .collect();
                assert!(realms.len() <= 1, "{what}: of more than one realm");
                for r in realms {
                    assert!(
                        weights.iter().any(|&(n, w)| n == r && w > 0),
                        "{what}: realm {r} does not guard such a site"
                    );
                    seen.entry(list).or_default().insert(r);
                }
            }
        }
        for (list, weights) in [
            ("tower", &rules.tower_realms),
            ("holy", &rules.holy_realms),
            ("wild", &rules.wild_realms),
        ] {
            let allowed: BTreeSet<u16> = weights.iter().filter(|w| w.1 > 0).map(|w| w.0).collect();
            assert_eq!(
                seen.get(list),
                Some(&allowed),
                "{list}, stronger {stronger}: every realm allowed turns up"
            );
        }
    }
}

#[test]
fn a_hoard_of_the_first_treasure_rules_that_holds_a_book_or_retort_holds_nothing_else() {
    let mut s = scene_at(0);
    s.w.scenario_change_settings(|g| {
        g.rules.treasure_patch_150 = false;
        g.world.lair_treasure_percent = 500;
    });
    let total = s.w.content().rules.lairs.spell_total;
    let tower = tower(&s.w);
    let mut special = 0;
    for i in 0..sample(600, 200) {
        let lair = put_lair(&mut s.w, s.site, tower, true);
        // A tower always holds a spell, or the book or retort that took its place.
        assert!(lair.special >= 1, "draw {i}: {lair:?}");
        if i32::from(lair.special) > total {
            special += 1;
            assert_eq!(treasure_value(&lair), 0, "draw {i}: {lair:?}");
            assert!(!lair.prisoner, "draw {i}: {lair:?}");
        }
    }
    assert!(special > 0, "no tower held a book or a retort");
}

#[test]
fn a_node_is_guarded_by_its_own_realm_or_the_one_that_stands_in() {
    let (_, w) = worlds().remove(0);
    let content = w.content();
    let rules = &content.rules.lairs;
    let style = LairStyle {
        stronger: false,
        treasure: false,
    };
    let nodes: Vec<RealmId> = content
        .features
        .defs()
        .iter()
        .filter(|d| d.kind == FeatureKind::Node)
        .map(|d| d.realm.expect("a node has a realm"))
        .collect();
    assert!(!nodes.is_empty());
    for realm in nodes {
        let wanted = if rules.node_own_realms.contains(&realm.num()) {
            realm.num()
        } else {
            rules.node_other
        };
        let mut guarded = 0;
        for seed in 0..sample(200, 40) {
            let mut rng = Pcg64Mcg::seed_from_u64(seed as u64);
            let budget = node_budget(&mut rng, content, 10, 1);
            let lair = create(
                &mut rng,
                content,
                Plane::Arcanus,
                SiteType::Node(realm),
                budget,
                100,
                100,
                style,
            );
            for &(u, _) in &lair.guards {
                guarded += 1;
                let of = content.rules.units.get(u).realm.map(RealmId::num);
                assert_eq!(of, Some(wanted), "node of realm {}", realm.num());
            }
        }
        assert!(guarded > 0, "no node of realm {} had a guard", realm.num());
    }
}

#[test]
fn a_node_budget_grows_with_power_and_magic_up_to_the_cap() {
    let (_, w) = worlds().remove(0);
    let content = w.content();
    let cap = content.rules.lairs.node_cap;
    let mut capped = false;
    for seed in 0..sample(100, 20) {
        // The same draw for each power and magic setting.
        let budget = |power: i32, magic: i32| {
            node_budget(
                &mut Pcg64Mcg::seed_from_u64(seed as u64),
                content,
                power,
                magic,
            )
        };
        for magic in 0..=2 {
            for power in 1..=60 {
                let b = budget(power, magic);
                assert!(b <= cap, "power {power}, magic {magic}: {b}");
                assert!(
                    b <= budget(power + 1, magic),
                    "power {power}, magic {magic}"
                );
                if magic < 2 {
                    assert!(
                        b <= budget(power, magic + 1),
                        "power {power}, magic {magic}"
                    );
                }
                capped |= b == cap;
            }
        }
        assert!(budget(5, 2) > budget(5, 0), "magic adds nothing");
    }
    assert!(capped, "no power reached the cap");
}

#[test]
fn an_encounter_budget_stays_in_the_range_of_its_plane_and_strength() {
    let (_, w) = worlds().remove(0);
    let content = w.content();
    let rules = &content.rules.lairs;
    for stronger in [false, true] {
        let style = LairStyle {
            stronger,
            treasure: false,
        };
        for plane in Plane::all() {
            for strong in [false, true] {
                let table = match (stronger, strong) {
                    (false, false) => &rules.weak_budget,
                    (false, true) => &rules.strong_budget,
                    (true, false) => &rules.patch_150.weak_budget,
                    (true, true) => &rules.patch_150.strong_budget,
                };
                let roll = table[plane.index()];
                let range = roll.base..=roll.base + roll.rolls * roll.step;
                for seed in 0..sample(500, 100) {
                    let mut rng = Pcg64Mcg::seed_from_u64(seed as u64);
                    let b = site_budget(&mut rng, content, plane, strong, style);
                    assert!(
                        range.contains(&b),
                        "{plane:?}, strong {strong}, stronger {stronger}: {b} not in {range:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_guard_and_treasure_settings_scale_what_new_sites_get() {
    let draws = sample(300, 80);
    // What `draws` new sites get altogether, the same draws for every setting.
    let total = |guards: u16, treasure: u16| {
        let mut s = scene_at(0);
        s.w.scenario_change_settings(|g| {
            g.rules.stronger_lairs = false;
            g.rules.treasure_patch_150 = false;
            g.world.lair_guard_percent = guards;
            g.world.lair_treasure_percent = treasure;
        });
        let feature = encounter(&s.w);
        (0..draws).fold((0, 0), |(g, t), _| {
            let lair = put_lair(&mut s.w, s.site, feature, true);
            (g + guard_value(&s.w, &lair), t + treasure_value(&lair))
        })
    };
    let (none, little, usual, much) = (
        total(0, 100),
        total(50, 100),
        total(100, 100),
        total(300, 100),
    );
    assert_eq!(none.0, 0, "guards with the setting at nothing");
    assert!(
        little.0 < usual.0 && usual.0 < much.0,
        "guards: {little:?}, {usual:?}, {much:?}"
    );
    let (poor, rich) = (total(100, 25), total(100, 500));
    assert!(
        poor.1 < usual.1 && usual.1 < rich.1,
        "treasure: {poor:?}, {usual:?}, {rich:?}"
    );
}

// ---- going in ----

#[test]
fn entering_is_refused_where_there_is_nothing_to_fight_or_no_step_to_it() {
    for mut s in scenes() {
        let units = army(&mut s);
        let before = state(&s.w);
        let events = s.w.apply(Command::EnterSite {
            units: units.clone(),
            to: s.site,
        });
        assert_eq!(
            refusal(&events),
            Some(&Rejection::Site(SiteError::NoSite)),
            "{}",
            s.name
        );
        assert_eq!(state(&s.w), before, "{}", s.name);

        let feature = encounter(&s.w);
        put_lair(&mut s.w, s.site, feature, true);
        // A square the army does not stand next to.
        let far = around(&s.w, s.from)
            .into_iter()
            .flat_map(|q| around(&s.w, q))
            .find(|&q| q != s.site && !around(&s.w, s.site).contains(&q) && open(&s.w, q));
        let Some(far) = far else {
            continue;
        };
        for &u in &units {
            s.w.cheat_move_unit(u, far);
        }
        let before = state(&s.w);
        let events = s.w.apply(Command::EnterSite { units, to: s.site });
        assert!(
            matches!(refusal(&events), Some(Rejection::Site(SiteError::Move(_)))),
            "{}: {events:?}",
            s.name
        );
        assert_eq!(state(&s.w), before, "{}", s.name);
    }
}

#[test]
fn a_move_onto_a_guarded_site_stops_and_tells_of_it() {
    for mut s in scenes() {
        let units = army(&mut s);
        let feature = encounter(&s.w);
        let lair = put_lair(&mut s.w, s.site, feature, true);
        assert!(!lair.guards.is_empty(), "{}", s.name);
        let events = s.w.apply(Command::MoveStack {
            units: units.clone(),
            to: s.site,
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::SiteEncounter { to, lair: l, .. }
                if *to == s.site && *l == lair.entity)),
            "{}: {events:?}",
            s.name
        );
        for &u in &units {
            assert_eq!(s.w.units().unit_position(u), Some(s.from), "{}", s.name);
        }
        assert_eq!(s.w.economy().lair_at(s.site), Some(lair), "{}", s.name);
        assert!(!s.w.in_battle(), "{}", s.name);
    }
}

#[test]
fn the_guards_stand_for_the_fight_and_a_lost_assault_leaves_the_site() {
    for mut s in scenes() {
        let units = army(&mut s);
        let feature = encounter(&s.w);
        let lair = put_lair(&mut s.w, s.site, feature, true);
        assert_eq!(
            s.w.economy().lair_guards_known_to(s.site, s.p),
            None,
            "{}: known before anyone fought",
            s.name
        );
        let events = s.w.apply(Command::EnterSite { units, to: s.site });
        assert_eq!(refusal(&events), None, "{}: {events:?}", s.name);
        assert!(s.w.in_battle(), "{}", s.name);
        let guards = s.w.units().units_at(s.site);
        let wanted = (lair.guard_count() as usize).min(s.w.max_battle_side());
        assert_eq!(guards.len(), wanted, "{}", s.name);
        for &g in &guards {
            assert_eq!(s.w.units().unit_owner(g), Some(MONSTERS), "{}", s.name);
            let kind = s.w.units().unit_kind_id(g).expect("a kind");
            assert!(lair.guards.iter().any(|x| x.0 == kind), "{}", s.name);
        }
        wipe_out(&mut s.w, Side::Attacker);
        assert!(!s.w.in_battle(), "{}", s.name);
        assert!(s.w.units().units_at(s.site).is_empty(), "{}", s.name);
        let now = s.w.economy().lair_at(s.site).expect("the site stands");
        assert_eq!(now.guards, lair.guards, "{}", s.name);
        assert_eq!(treasure_value(&now), treasure_value(&lair), "{}", s.name);
        // Whoever fought knows what is there; nobody else does.
        assert_eq!(
            s.w.economy().lair_guards_known_to(s.site, s.p),
            Some(lair.guards.clone()),
            "{}",
            s.name
        );
        assert_eq!(
            s.w.economy().lair_guards_known_to(s.site, s.other),
            None,
            "{}",
            s.name
        );
    }
}

#[test]
fn nature_awareness_shows_the_guards_of_every_site() {
    for mut s in scenes() {
        let feature = encounter(&s.w);
        let lair = put_lair(&mut s.w, s.site, feature, true);
        let spell = SpellId::new(NATURE_AWARENESS);
        s.w.scenario_add_global(s.p, spell);
        assert_eq!(
            s.w.economy().lair_guards_known_to(s.site, s.p),
            Some(lair.guards),
            "{}",
            s.name
        );
        assert_eq!(
            s.w.economy().lair_guards_known_to(s.site, s.other),
            None,
            "{}",
            s.name
        );
    }
}

// ---- the treasure ----

/// What a wizard has that a hoard can add to.
struct Holdings {
    gold: f4x_def::half_int::HalfInt,
    mana: f4x_def::half_int::HalfInt,
    items: usize,
    heroes: usize,
    known: BTreeSet<SpellId>,
    books: HashMap<RealmId, i32>,
    retorts: usize,
}

fn holdings(w: &World, p: PlayerId) -> Holdings {
    let player = w.player(p).expect("a player");
    let treasury = player.treasury();
    let armory = w.items().armory(p);
    let wizard = w.wizard(p).expect("a wizard");
    Holdings {
        gold: treasury.gold,
        mana: treasury.mana,
        items: armory.vault.iter().flatten().count() + armory.awards.len(),
        heroes: w.units().hero_units(p).len(),
        known: w
            .content()
            .rules
            .spells
            .iter()
            .map(|(s, _)| s)
            .filter(|&s| wizard.knows(s))
            .collect(),
        books: w
            .content()
            .rules
            .realms
            .iter()
            .map(|(r, _)| (r, wizard.books_in(r)))
            .collect(),
        retorts: wizard.retorts().len(),
    }
}

/// A new rich site at the first scene, drawn after `skip` draws.
fn rich_site(skip: usize, patch: bool) -> (Scene, LairInfo) {
    let mut s = scene_at(skip);
    s.w.scenario_change_settings(|g| {
        g.rules.treasure_patch_150 = patch;
        g.world.lair_treasure_percent = 500;
        g.world.lair_guard_percent = 300;
    });
    let feature = encounter(&s.w);
    let lair = put_lair(&mut s.w, s.site, feature, true);
    (s, lair)
}

/// The wizard's army goes into the site and the guards fall.
fn win(s: &mut Scene) -> (Holdings, Vec<Event>) {
    let units = army(s);
    let before = holdings(&s.w, s.p);
    let mut events = s.w.apply(Command::EnterSite { units, to: s.site });
    assert_eq!(refusal(&events), None, "{}: {events:?}", s.name);
    events.extend(wipe_out(&mut s.w, Side::Defender));
    (before, events)
}

#[test]
fn winning_pays_the_winner_what_the_event_tells_and_clears_the_site() {
    // How often each kind of reward turned up.
    let (mut spells, mut books, mut retorts, mut items, mut prisoners) = (0, 0, 0, 0, 0);
    let total = if deep() { 600 } else { 150 };
    for i in 0..total {
        let (mut s, lair) = rich_site(i, i % 2 == 1);
        if lair.guards.is_empty() {
            continue;
        }
        let (before, events) = win(&mut s);
        let name = format!("{}: {lair:?}", s.name);
        let (guarded, loot) = cleared(&events).unwrap_or_else(|| panic!("{name}: {events:?}"));
        assert!(guarded, "{name}");
        let after = holdings(&s.w, s.p);
        assert_eq!(
            after.gold - before.gold,
            f4x_def::half_int::HalfInt::from_int(loot.gold as i32),
            "{name}: gold"
        );
        assert_eq!(
            after.mana - before.mana,
            f4x_def::half_int::HalfInt::from_int(loot.mana as i32),
            "{name}: mana, told {loot:?}"
        );
        assert_eq!(
            after.items - before.items,
            loot.items.len(),
            "{name}: items"
        );
        assert!(loot.items.len() >= lair.items.len(), "{name}: items lost");
        assert_eq!(
            after.heroes - before.heroes,
            usize::from(loot.prisoner),
            "{name}: heroes"
        );
        assert!(
            lair.prisoner || !loot.prisoner,
            "{name}: a hero from nowhere"
        );

        // A spell only from a hoard of a spell, and one the wizard had not and can use.
        let learned: Vec<SpellId> = after.known.difference(&before.known).copied().collect();
        if let Some(spell) = loot.spell {
            spells += 1;
            let spells_up_to = s.w.content().rules.lairs.spell_total;
            assert!(
                (1..=spells_up_to).contains(&i32::from(lair.special)),
                "{name}"
            );
            assert!(learned.contains(&spell), "{name}: {spell:?}");
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, Event::SpellLearned { spell: x, .. } if *x == spell)),
                "{name}"
            );
            let realm = s.w.content().rules.spells.get(spell).realm;
            assert!(
                realm.num() == ARCANE || before.books[&realm] > 0,
                "{name}: a spell of a realm without books"
            );
        }
        // Books and retorts only from a hoard of them.
        let gained: i32 = after.books.iter().map(|(r, n)| n - before.books[r]).sum();
        assert_eq!(gained as usize, loot.books.len(), "{name}: books");
        assert_eq!(
            after.retorts - before.retorts,
            loot.retorts.len(),
            "{name}: retorts"
        );
        // A `special` above the spells' total rarity is that many books, or a retort.
        let spells_up_to = s.w.content().rules.lairs.spell_total;
        if i32::from(lair.special) <= spells_up_to {
            assert!(loot.books.is_empty() && loot.retorts.is_empty(), "{name}");
        } else {
            let over = i32::from(lair.special) - spells_up_to;
            assert!(loot.books.len() as i32 <= over, "{name}");
            assert!(loot.retorts.len() <= 1, "{name}");
            let has = |num: u16| after.books[&RealmId::new(num)] > 0;
            assert!(!(has(LIFE) && has(DEATH)), "{name}: life beside death");
        }
        books += loot.books.len();
        retorts += loot.retorts.len();
        items += loot.items.len();
        prisoners += usize::from(loot.prisoner);

        assert_eq!(s.w.economy().lair_at(s.site), None, "{name}");
        assert_eq!(
            s.w.economy().feature_entity_at(s.site),
            None,
            "{name}: an encounter site vanishes"
        );
        let here = s.w.units().units_at(s.site);
        assert!(!here.is_empty(), "{name}: the victors move in");
        assert!(
            here.iter().all(|&u| s.w.units().unit_owner(u) == Some(s.p)),
            "{name}"
        );
    }
    assert!(
        spells > 0 && books > 0 && retorts > 0 && items > 0 && prisoners > 0,
        "spells {spells}, books {books}, retorts {retorts}, items {items}, prisoners {prisoners}"
    );
}

/// A wizard who can take neither a book nor a retort is paid something else for such a hoard: an
/// item, or mana when the hoard is full of items.
#[test]
fn a_book_or_retort_nobody_can_take_pays_an_item_or_mana() {
    let (mut by_item, mut by_mana) = (0, 0);
    let wanted = sample(8, 2);
    // The sites one world draws one after the other are those of `rich_site(0)`, `rich_site(1)` and
    // so on: they are looked through in one world, and the few that fit are fought for.
    let (mut drawn, _) = rich_site(0, true);
    let feature = encounter(&drawn.w);
    for i in 1..20_000 {
        if by_item >= wanted && by_mana >= wanted {
            break;
        }
        let seen = put_lair(&mut drawn.w, drawn.site, feature, true);
        let spells_up_to = drawn.w.content().rules.lairs.spell_total;
        if i32::from(seen.special) <= spells_up_to || seen.guards.is_empty() {
            continue;
        }
        let full = seen.items.len() == drawn.w.content().rules.lairs.max_items;
        // A full hoard counts when it has mana too: the mana told must hold both.
        let enough = if full {
            by_mana >= wanted || seen.mana == 0
        } else {
            by_item >= wanted
        };
        if enough {
            continue;
        }
        let (mut s, lair) = rich_site(i, true);
        assert_eq!(
            (&lair.guards, lair.mana, &lair.items),
            (&seen.guards, seen.mana, &seen.items),
            "{}: the same site",
            s.name
        );
        let rules = &s.w.content().rules;
        let most = rules.config.wizard.max_retorts;
        let retorts: Vec<Retort> = rules.retorts.iter().map(|(r, _)| r).take(most).collect();
        for r in retorts {
            s.w.cheat_set_retort(s.p, r, true);
        }
        let most = s.w.settings().wizards.max_books_per_realm;
        for &realm in f4x_core::rules::BOOK_REALMS.iter() {
            s.w.cheat_set_books(s.p, realm, most);
        }
        let (before, events) = win(&mut s);
        let name = format!("{}: {lair:?}", s.name);
        let (_, loot) = cleared(&events).unwrap_or_else(|| panic!("{name}: {events:?}"));
        assert!(loot.books.is_empty() && loot.retorts.is_empty(), "{name}");
        let after = holdings(&s.w, s.p);
        assert_eq!(
            after.mana - before.mana,
            f4x_def::half_int::HalfInt::from_int(loot.mana as i32),
            "{name}: mana, told {loot:?}"
        );
        assert_eq!(
            after.items - before.items,
            loot.items.len(),
            "{name}: items"
        );
        if full {
            by_mana += 1;
            assert!(
                loot.mana > lair.mana,
                "{name}: nothing for the book, {loot:?}"
            );
        } else {
            by_item += 1;
            assert!(loot.items.len() > lair.items.len(), "{name}: {loot:?}");
        }
    }
    assert!(by_item > 0 && by_mana > 0, "item {by_item}, mana {by_mana}");
}

/// A spell the winner has no use for is paid in other treasure.
#[test]
fn a_spell_the_winner_knows_already_pays_other_treasure_in_its_place() {
    let mut seen = 0;
    for i in 0..4000 {
        if seen >= sample(20, 4) {
            break;
        }
        let (mut s, lair) = rich_site(i, i % 2 == 0);
        let spells_up_to = s.w.content().rules.lairs.spell_total;
        if !(1..=spells_up_to).contains(&i32::from(lair.special)) || lair.guards.is_empty() {
            continue;
        }
        seen += 1;
        let all: Vec<SpellId> = s.w.content().rules.spells.iter().map(|(s, _)| s).collect();
        for spell in all {
            s.w.cheat_learn_spell(s.p, spell);
        }
        let (_, events) = win(&mut s);
        let name = format!("{}: {lair:?}", s.name);
        let (_, loot) = cleared(&events).unwrap_or_else(|| panic!("{name}: {events:?}"));
        assert_eq!(loot.spell, None, "{name}");
        assert!(
            loot.gold >= lair.gold
                && loot.mana >= lair.mana
                && loot.items.len() >= lair.items.len(),
            "{name}: {loot:?}"
        );
        assert!(
            loot.gold > lair.gold || loot.mana > lair.mana || loot.items.len() > lair.items.len(),
            "{name}: nothing for the spell, {loot:?}"
        );
    }
    assert!(seen > 0, "no site held a spell");
}

#[test]
fn a_site_nobody_guards_pays_out_to_whoever_walks_in() {
    for mut s in scenes() {
        s.w.scenario_change_settings(|g| {
            g.rules.stronger_lairs = false;
            g.world.lair_guard_percent = 0;
        });
        let units = army(&mut s);
        let feature = encounter(&s.w);
        let lair = put_lair(&mut s.w, s.site, feature, true);
        assert!(lair.guards.is_empty(), "{}", s.name);
        let before = holdings(&s.w, s.p);
        let events = s.w.apply(Command::MoveStack {
            units: units.clone(),
            to: s.site,
        });
        let (guarded, loot) = cleared(&events).unwrap_or_else(|| panic!("{}: {events:?}", s.name));
        assert!(!guarded, "{}", s.name);
        let after = holdings(&s.w, s.p);
        assert_eq!(
            after.gold - before.gold,
            f4x_def::half_int::HalfInt::from_int(loot.gold as i32),
            "{}",
            s.name
        );
        assert_eq!(s.w.economy().lair_at(s.site), None, "{}", s.name);
        assert_eq!(
            s.w.units().unit_position(units[0]),
            Some(s.site),
            "{}",
            s.name
        );
    }
}

// ---- towers ----

#[test]
fn a_cleared_tower_stays_and_belongs_to_whoever_last_walked_into_it() {
    for mut s in scenes() {
        let units = army(&mut s);
        let feature = tower(&s.w);
        put_lair(&mut s.w, s.site, feature, true);
        let across = s.site.on_other_plane();
        assert_eq!(s.w.economy().tower_owner(s.site), None, "{}", s.name);
        assert!(
            s.w.economy().lair_at(across).is_some(),
            "{}: one tower, two planes",
            s.name
        );
        s.w.apply(Command::EnterSite { units, to: s.site });
        let events = wipe_out(&mut s.w, Side::Defender);
        assert!(cleared(&events).is_some(), "{}: {events:?}", s.name);
        assert_eq!(s.w.economy().lair_at(s.site), None, "{}", s.name);
        assert_eq!(s.w.economy().lair_at(across), None, "{}", s.name);
        assert_eq!(
            s.w.economy().feature_kind_at(across),
            Some(FeatureKind::Tower),
            "{}: the tower stays",
            s.name
        );
        for at in [s.site, across] {
            assert_eq!(s.w.economy().tower_owner(at), Some(s.p), "{}", s.name);
            assert!(s.w.is_explored(s.p, at), "{}: {at:?}", s.name);
        }

        // The victors leave and a rival walks in: the tower is the rival's.
        let aside = around(&s.w, s.from)
            .into_iter()
            .find(|&q| q != s.site && open(&s.w, q));
        let Some(aside) = aside else {
            continue;
        };
        for u in s.w.units().units_at(s.site) {
            s.w.cheat_move_unit(u, aside);
        }
        for u in s.w.units().units_at(s.from) {
            s.w.cheat_move_unit(u, aside);
        }
        let kind = soldier(&s.w);
        let rival = create_at(&mut s.w, s.other, s.from, kind);
        let events = s.w.apply(Command::MoveStack {
            units: vec![rival],
            to: s.site,
        });
        assert_eq!(refusal(&events), None, "{}: {events:?}", s.name);
        assert_eq!(s.w.units().unit_position(rival), Some(s.site), "{}", s.name);
        assert_eq!(
            s.w.economy().tower_owner(s.site),
            Some(s.other),
            "{}",
            s.name
        );
    }
}

// ---- nodes ----

/// A node of the world that nobody owns.
fn free_node(w: &World) -> NodeInfo {
    w.economy()
        .nodes()
        .into_iter()
        .find(|n| n.owner.is_none() && w.units().units_at(n.pos).is_empty())
        .expect("a free node")
}

fn meld(w: &mut Table, units: Vec<hecs::Entity>) -> Vec<Event> {
    w.apply(Command::Meld { units })
}

#[test]
fn a_meld_is_refused_without_a_spirit_or_a_node() {
    for mut s in scenes() {
        let node = free_node(&s.w);
        let spirit = create_at(&mut s.w, s.p, node.pos, UnitId::new(MAGIC_SPIRIT));
        let spearmen = create_at(&mut s.w, s.p, node.pos, UnitId::new(SPEARMEN));
        let stray = create_at(&mut s.w, s.p, s.from, UnitId::new(MAGIC_SPIRIT));
        for (units, why) in [
            (vec![], MeldError::BadSelection),
            (vec![spirit, stray], MeldError::BadSelection),
            (vec![stray], MeldError::NoNode),
            (vec![spearmen], MeldError::NoMelder),
        ] {
            let before = state(&s.w);
            let events = meld(&mut s.w, units);
            assert_eq!(
                refusal(&events),
                Some(&Rejection::Meld(why)),
                "{}: {why:?}",
                s.name
            );
            assert_eq!(state(&s.w), before, "{}: {why:?}", s.name);
        }
    }
}

#[test]
fn a_guardian_spirit_goes_before_a_magic_spirit_and_guards_the_node() {
    for mut s in scenes() {
        let node = free_node(&s.w);
        let guardian = create_at(&mut s.w, s.p, node.pos, UnitId::new(GUARDIAN_SPIRIT));
        let spirit = create_at(&mut s.w, s.p, node.pos, UnitId::new(MAGIC_SPIRIT));
        let events = meld(&mut s.w, vec![spirit, guardian]);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::NodeMelded { player, .. } if *player == s.p)),
            "{}: {events:?}",
            s.name
        );
        assert_eq!(
            s.w.units().units_at(node.pos),
            vec![spirit],
            "{}: the guardian is used up",
            s.name
        );
        let held = s.w.economy().node_at(node.pos).expect("the node");
        assert_eq!((held.owner, held.guardian), (Some(s.p), true), "{}", s.name);
        let events = meld(&mut s.w, vec![spirit]);
        assert_eq!(
            refusal(&events),
            Some(&Rejection::Meld(MeldError::AlreadyGuarded)),
            "{}",
            s.name
        );
    }
}

#[test]
fn a_rivals_node_falls_to_a_spirit_unless_a_guardian_holds_it_off() {
    for guarded in [false, true] {
        let (mut taken, mut held_off) = (0, 0);
        for i in 0..sample(200, 40) {
            let mut s = scene_at(i);
            let node = free_node(&s.w);
            let first = if guarded {
                GUARDIAN_SPIRIT
            } else {
                MAGIC_SPIRIT
            };
            let theirs = create_at(&mut s.w, s.other, node.pos, UnitId::new(first));
            meld(&mut s.w, vec![theirs]);
            let mine = create_at(&mut s.w, s.p, node.pos, UnitId::new(MAGIC_SPIRIT));
            let events = meld(&mut s.w, vec![mine]);
            assert!(
                s.w.units().units_at(node.pos).is_empty(),
                "{}: the spirit is spent either way",
                s.name
            );
            let now = s.w.economy().node_at(node.pos).expect("the node");
            if events.iter().any(|e| matches!(e, Event::NodeMelded { .. })) {
                taken += 1;
                assert_eq!((now.owner, now.guardian), (Some(s.p), false), "{}", s.name);
            } else {
                held_off += 1;
                assert!(
                    events.iter().any(|e| matches!(e, Event::MeldFailed { .. })),
                    "{}: {events:?}",
                    s.name
                );
                assert_eq!(
                    (now.owner, now.guardian),
                    (Some(s.other), guarded),
                    "{}",
                    s.name
                );
            }
        }
        if guarded {
            // A guardian holds off most of the tries and not all of them.
            assert!(
                taken > 0 && held_off > taken,
                "taken {taken}, held off {held_off}"
            );
        } else {
            assert_eq!(held_off, 0, "a node with no guardian held a spirit off");
        }
    }
}

#[test]
fn a_node_pays_its_owner_more_with_stronger_magic_and_with_node_mastery() {
    for mut s in scenes() {
        let node = free_node(&s.w);
        assert_eq!(s.w.node_power(&node), 0, "{}: nobody's node", s.name);
        let spirit = create_at(&mut s.w, s.p, node.pos, UnitId::new(MAGIC_SPIRIT));
        meld(&mut s.w, vec![spirit]);
        let mine = |w: &World| w.economy().node_at(node.pos).expect("the node");
        let mut last = 0;
        for magic in 0..=2 {
            s.w.scenario_change_settings(|g| g.basic.magic_strength = magic);
            let power = s.w.node_power(&mine(&s.w));
            assert!(power > last, "{}: magic {magic} gives {power}", s.name);
            assert_eq!(s.w.node_income(s.p), power, "{}", s.name);
            assert_eq!(s.w.node_income(s.other), 0, "{}", s.name);
            last = power;
        }
        assert!(
            s.w.cheat_set_retort(s.p, Retort::new(NODE_MASTERY), true),
            "{}",
            s.name
        );
        assert_eq!(s.w.node_power(&mine(&s.w)), last * 2, "{}", s.name);
    }
}

// ---- volcanoes ----

#[test]
fn a_raised_volcano_gives_its_wizard_power_until_it_goes_out() {
    for mut s in scenes() {
        s.w.cheat_reveal(s.p);
        let spell = SpellId::new(RAISE_VOLCANO);
        s.w.cheat_learn_spell(s.p, spell);
        s.w.treasury_mut(s.p).expect("a treasury").mana =
            f4x_def::half_int::HalfInt::from_int(10_000);
        let power = |w: &World, p| w.income(p).power_base;
        let before = (power(&s.w, s.p), power(&s.w, s.other));
        s.w.scenario_casting_with(s.p, spell, 0, 0);
        let events = s.w.apply(Command::ResolveCast {
            player: s.p,
            target: Target::Tile(s.site),
        });
        assert_eq!(refusal(&events), None, "{}: {events:?}", s.name);
        assert_eq!(s.w.economy().volcano_at(s.site), Some(s.p), "{}", s.name);
        assert!(
            s.w.economy().volcanoes().contains(&(s.site, s.p)),
            "{}",
            s.name
        );
        assert!(power(&s.w, s.p) > before.0, "{}", s.name);
        assert_eq!(power(&s.w, s.other), before.1, "{}", s.name);
        let chance = s.w.content().rules.config.spell_effects.cool_percent;
        assert!(chance > 0, "the content lets volcanoes go out");
        // Far more rounds than the chance asks for.
        let rounds = 2000 / chance;
        let out = (0..rounds).any(|_| {
            s.w.end_round();
            s.w.economy().volcano_at(s.site).is_none()
        });
        assert!(out, "{}: burning after {rounds} rounds", s.name);
        assert_eq!(
            s.w.economy().terrain_class_at(s.site),
            Some(TerrainClass::Mountains),
            "{}: mountains stay",
            s.name
        );
    }
}

#[test]
fn the_squares_of_the_sites_are_those_of_every_node_and_lair() {
    for (name, w) in worlds() {
        let sites = w.economy().site_squares();
        for node in w.economy().nodes() {
            assert!(sites.contains(&node.pos), "{name}: node at {:?}", node.pos);
        }
        let lairs = w.economy().lairs();
        assert!(!lairs.is_empty(), "{name}");
        for lair in lairs {
            let at = lair.pos;
            assert!(sites.contains(&at), "{name}: lair at {at:?}");
        }
    }
}
