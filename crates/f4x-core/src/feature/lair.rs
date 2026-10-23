//! Guarded sites: who guards towers, magic nodes and encounter sites, what treasure they hold,
//! and what clearing one does.
//!
//! The original gives every tower, node and encounter site a record with up to two kinds of
//! guard and a hoard, made from a budget when the map is generated (ReMoM `Create_Lair`).
//! The guards are not on the map: they are created as neutral units when a stack enters the
//! site (`Lair_Make_Guardians`), fight the stack, and are removed again afterwards. A stack that
//! wins takes the hoard and the site is cleared; an encounter site then disappears from the map.
//!
//! Differences from the original are listed in `docs/lairs.md`.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;
use serde::{Deserialize, Serialize};

use crate::content::{Content, FeatureKind};
use crate::feature::entity::LairInfo;
use crate::feature::node::Node;
use crate::feature::place::Placed;
use crate::item::make::{make_item, Grade};
use crate::map::mapgen::GenParams;
use crate::map::{Map, Plane, Position};
use crate::player::{PlayerId, MONSTERS};

use crate::rules::{Rarity, SpellId, Stat, UnitKind};
use crate::unit::movement::MoveError;

use crate::game::{Event, Game};
use crate::wizard::{retort_available, retort_cost, ALL_RETORTS};

/// Most guards a site holds in total.
const MAX_GUARDS: u32 = 9;
/// Most guards of the first kind.
const MAX_FIRST_GUARDS: i32 = 8;
/// What kind of site a lair guards, for choosing its guards' realm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SiteType<'a> {
    Tower,
    Node(&'a str),
    /// Temples and ruins draw on death and life.
    Holy,
    /// Caves, lairs, dungeons and keeps draw on death, chaos and nature.
    Wild,
}

fn site_type<'a>(def_id: &'a str, kind: FeatureKind) -> SiteType<'a> {
    match kind {
        FeatureKind::Tower => SiteType::Tower,
        FeatureKind::Node => SiteType::Node(def_id.strip_suffix("_node").unwrap_or(def_id)),
        _ if def_id.contains("temple") || def_id.contains("ruin") => SiteType::Holy,
        _ => SiteType::Wild,
    }
}

/// `Random(n)` of the original: `1..=n`.
fn random(rng: &mut Pcg64Mcg, n: i32) -> i32 {
    rng.gen_range(1..=n.max(1))
}

/// The realm of a site's guards.
fn guard_realm(rng: &mut Pcg64Mcg, site: SiteType) -> &'static str {
    match site {
        SiteType::Node("sorcery") => "sorcery",
        SiteType::Node("nature") => "nature",
        SiteType::Node(_) => "chaos",
        SiteType::Tower => match random(rng, 6) {
            1 | 2 => "death",
            3 => "chaos",
            4 => "life",
            5 => "nature",
            _ => "sorcery",
        },
        SiteType::Holy => {
            if random(rng, 4) <= 3 {
                "death"
            } else {
                "life"
            }
        }
        // The original's switch falls through to nature in every case; the intended split
        // (ReMoM marks it a bug) is two parts death, two chaos, one nature.
        SiteType::Wild => match random(rng, 5) {
            1 | 2 => "death",
            3 | 4 => "chaos",
            _ => "nature",
        },
    }
}

/// Units that can guard a site of `realm`: summoned creatures of that realm that are not
/// transports, with their costs, in the data's order.
fn candidates(content: &Content, realm: &str) -> Vec<(String, i32)> {
    content
        .rules
        .units
        .defs()
        .iter()
        .filter(|d| {
            d.kind == UnitKind::Summoned
                && d.realm.as_deref() == Some(realm)
                && d.stats.get(&Stat::Transport).copied().unwrap_or(0) == 0
        })
        .map(|d| (d.id.clone(), d.cost as i32))
        .collect()
}

/// The dearest candidate costing less than `limit` (the first of equals), skipping `not`.
fn dearest_under<'a>(
    list: &'a [(String, i32)],
    limit: i32,
    not: Option<&str>,
) -> Option<&'a (String, i32)> {
    let mut best: Option<&(String, i32)> = None;
    for c in list {
        if not == Some(c.0.as_str()) {
            continue;
        }
        if c.1 > best.map_or(0, |b| b.1) && c.1 < limit {
            best = Some(c);
        }
    }
    best
}

/// Choose the guards a budget buys: a first kind from a random share of the budget, then
/// a second kind from what is left, nine units at most. Returns the guards and their value.
fn pick_guards(
    rng: &mut Pcg64Mcg,
    list: &[(String, i32)],
    mut budget: i32,
) -> (Vec<(String, u32)>, i32, i32) {
    let mut guards: Vec<(String, u32)> = Vec::new();
    let draw = |rng: &mut Pcg64Mcg, budget: i32, spread: i32, not: Option<&str>| {
        for _ in 0..200 {
            let limit = budget / random(rng, spread);
            if let Some(c) = dearest_under(list, limit, not) {
                return Some(c.clone());
            }
        }
        None
    };
    let mut first_value = 0;
    let mut second_value = 0;
    let mut first_count = 0;
    if let Some((id, cost)) = draw(rng, budget, 4, None) {
        let mut count = budget / cost;
        if count > 1 && random(rng, 2) == 1 {
            count -= 1;
        }
        count = count.min(MAX_FIRST_GUARDS);
        budget -= cost * count;
        first_count = count;
        first_value = cost * count;
        guards.push((id, count as u32));
    }
    if first_count > 0 {
        let first_id = guards[0].0.clone();
        if let Some((id, cost)) = draw(rng, budget, 10 - first_count, Some(&first_id)) {
            let count = (budget / cost).min(MAX_GUARDS as i32 - first_count);
            if count > 0 {
                second_value = cost * count;
                guards.push((id, count as u32));
            }
        }
    }
    (guards, first_value, second_value)
}

/// Fill a lair's hoard from its guards' value (`Create_Lair` after the guards are chosen).
fn fill_hoard(
    rng: &mut Pcg64Mcg,
    lair: &mut Lair,
    site: SiteType,
    plane: Plane,
    guard_value: i32,
    second_value: i32,
    treasure_percent: i32,
) {
    let mut loot = guard_value + second_value / 2;
    loot = loot * treasure_percent / 100;
    loot = match plane {
        Plane::Arcanus => loot * (49 + random(rng, 76)) / 100,
        Plane::Myrror => loot * (75 + random(rng, 100)) / 100,
    };
    loot = loot.max(50);
    let mut special = 0i32;
    if site == SiteType::Tower {
        // A tower always holds a spell, and pays for it out of the rest.
        let rarity = random(rng, 4);
        loot -= rarity * rarity * 50 - 100;
        special = rarity;
    }
    let mut guard = 0;
    while loot >= 50 && guard < 2000 {
        guard += 1;
        match random(rng, 15) - 1 {
            0 | 1 => {
                let price = (random(rng, 20) * 10).min(loot) / 10 * 10;
                loot -= 200;
                lair.gold += price as u32;
            }
            2 | 3 => {
                let price = (random(rng, 20) * 10).min(loot) / 10 * 10;
                loot -= 200;
                lair.mana += price as u32;
            }
            4..=8 => {
                if lair.items.len() < 3 && loot >= 300 {
                    let price = (300 + random(rng, 27) * 100).min(loot) / 10 * 10;
                    loot -= price;
                    lair.items.push(price as u32);
                }
            }
            9 => {
                if loot >= 400 && !lair.prisoner {
                    lair.prisoner = true;
                    loot -= 1000;
                }
            }
            10..=12 => {
                let rarity = random(rng, 4);
                if rarity * rarity * 50 <= loot && special < 4 {
                    loot -= rarity * rarity * 50;
                    special = (special + rarity).min(4);
                }
            }
            _ => {
                // A spell book or retort: two picks' worth when the budget allows.
                if random(rng, 4) == 1 && loot >= 3000 || loot >= 2000 {
                    special = 6;
                    loot -= 3000;
                } else if loot >= 1000 {
                    special = 5;
                    loot -= 3000;
                }
            }
        }
    }
    if special > 4 {
        lair.gold = 0;
        lair.mana = 0;
        lair.items.clear();
        lair.prisoner = false;
    }
    lair.special = special as u8;
}

/// What the loot a winner cannot use turns into: the hoard's own table over `value`, without
/// the kinds a winner has no use for. Gold and mana are always of use; an item only when `items`
/// has room (at most that many, and each of 300 or more); no spell, book, retort or prisoner.
/// Returns the gold, the mana and the values of the items.
pub(crate) fn reroll_hoard(rng: &mut impl Rng, value: i32, items: usize) -> (u32, u32, Vec<u32>) {
    let mut loot = value;
    let (mut gold, mut mana) = (0, 0);
    let mut made = Vec::new();
    for _ in 0..2000 {
        if loot < 50 {
            break;
        }
        match rng.gen_range(0..15) {
            0 | 1 => {
                let price = rng.gen_range(1..=20) * 10;
                loot -= 200;
                gold += price.min(loot.max(0) + 200) as u32 / 10 * 10;
            }
            2 | 3 => {
                let price = rng.gen_range(1..=20) * 10;
                loot -= 200;
                mana += price.min(loot.max(0) + 200) as u32 / 10 * 10;
            }
            4..=8 if made.len() < items && loot >= 300 => {
                let price = (300 + rng.gen_range(1..=27) * 100).min(loot) / 10 * 10;
                loot -= price;
                made.push(price as u32);
            }
            _ => {}
        }
    }
    // A value too small for the table pays out as gold, so that nothing is lost to the roll.
    if gold + mana == 0 && made.is_empty() {
        gold = (value.max(0) / 4) as u32;
    }
    (gold, mana, made)
}

/// Make one lair from its budget (the original's `Create_Lair`).
#[allow(clippy::too_many_arguments)]
fn make_lair(
    rng: &mut Pcg64Mcg,
    content: &Content,
    plane: Plane,
    site: SiteType,
    budget: i32,
    guard_percent: i32,
    treasure_percent: i32,
) -> Lair {
    let budget = budget * guard_percent / 100;
    let realm = guard_realm(rng, site);
    let list = candidates(content, realm);
    let (guards, first, second) = pick_guards(rng, &list, budget);
    let mut lair = Lair {
        intact: true,
        guards,
        gold: 0,
        mana: 0,
        items: Vec::new(),
        prisoner: false,
        special: 0,
    };
    fill_hoard(rng, &mut lair, site, plane, first, second, treasure_percent);
    lair
}

/// What a guarded site holds: its guards and its treasure. Towers, magic nodes and the
/// encounter sites each have one (the original's "lair" table), a component of their entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lair {
    /// False once cleared.
    pub intact: bool,
    /// Guard unit ids with counts; empty for an unguarded site.
    pub guards: Vec<(String, u32)>,
    pub gold: u32,
    pub mana: u32,
    /// The value of each magic item inside (items themselves wait for the hero tasks).
    pub items: Vec<u32>,
    /// A hero is held prisoner here.
    pub prisoner: bool,
    /// 0 nothing, 1 to 4 a spell of that rarity (1 common), 5 a spell book or a retort,
    /// 6 a spell book or retort worth two picks.
    pub special: u8,
}

/// Give every tower, node and encounter site on the map its guards and treasure, each with the
/// square it is on. A tower is one site, on its Arcanus square.
pub fn generate_lairs(
    seed: u64,
    params: &GenParams,
    map: &Map,
    placed: &Placed,
    nodes: &[(Position, Node)],
    content: &Content,
) -> Vec<(Position, Lair)> {
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x1A12_5EED_600D_5CA1);
    let guards = i32::from(params.lair_guard_percent.min(300));
    let treasure = i32::from(params.lair_treasure_percent.min(500));
    let magic = i32::from(params.magic_strength.min(2));
    let mut lairs: Vec<(Position, Lair)> = Vec::new();
    // Towers stand on both planes and share one set of guards and treasure.
    let mut seen_towers: Vec<(u32, u32)> = Vec::new();
    for plane in Plane::ALL {
        for y in 0..map.height() {
            for x in 0..map.width() {
                let Some(def) = placed
                    .get(plane, x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                else {
                    continue;
                };
                if def.kind != FeatureKind::Tower || seen_towers.contains(&(x, y)) {
                    continue;
                }
                seen_towers.push((x, y));
                let budget = 650 + 50 * random(&mut rng, 11);
                let lair = make_lair(
                    &mut rng,
                    content,
                    Plane::Arcanus,
                    SiteType::Tower,
                    budget,
                    guards,
                    treasure,
                );
                lairs.push((
                    Position {
                        plane: Plane::Arcanus,
                        x,
                        y,
                    },
                    lair,
                ));
            }
        }
    }
    for (pos, node) in nodes {
        let power = i32::from(node.power);
        let budget = (power * power * (4 + random(&mut rng, 11)) * (magic + 1) / 2).min(32767);
        let lair = make_lair(
            &mut rng,
            content,
            pos.plane,
            SiteType::Node(&node.realm),
            budget,
            guards,
            treasure,
        );
        lairs.push((*pos, lair));
    }
    // The random sites, in map order; exactly `strong_sites` of them (picked at random) are the
    // strong ones, the rest weak.
    let mut sites = Vec::new();
    for plane in Plane::ALL {
        for y in 0..map.height() {
            for x in 0..map.width() {
                let Some(def) = placed
                    .get(plane, x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                else {
                    continue;
                };
                if def.kind == FeatureKind::Encounter {
                    sites.push((plane, x, y, def));
                }
            }
        }
    }
    let mut order: Vec<usize> = (0..sites.len()).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.gen_range(0..=i));
    }
    let strong_count = (params.strong_sites as usize).min(sites.len());
    let strong: Vec<bool> = {
        let mut flags = vec![false; sites.len()];
        for &i in &order[..strong_count] {
            flags[i] = true;
        }
        flags
    };
    for (i, &(plane, x, y, def)) in sites.iter().enumerate() {
        let budget = match (strong[i], plane) {
            (true, Plane::Arcanus) => 50 + random(&mut rng, 29) * 50,
            (true, Plane::Myrror) => 100 + random(&mut rng, 24) * 100,
            (false, Plane::Arcanus) => random(&mut rng, 10) * 10,
            (false, Plane::Myrror) => random(&mut rng, 20) * 10,
        };
        let lair = make_lair(
            &mut rng,
            content,
            plane,
            site_type(&def.id, def.kind),
            budget,
            guards,
            treasure,
        );
        lairs.push((Position { plane, x, y }, lair));
    }
    lairs
}

impl Lair {
    /// Number of guard units.
    pub fn guard_count(&self) -> u32 {
        self.guards.iter().map(|g| g.1).sum()
    }
}

/// What a cleared site gave its conqueror.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loot {
    pub gold: u32,
    pub mana: u32,
    /// The magic items found, made from the values in the hoard; they are in the armory.
    pub items: Vec<crate::item::Item>,
    /// A prisoner hero was found: a free offer of the hero waits on the wizard's offers.
    pub prisoner: bool,
    pub spell: Option<SpellId>,
    /// Spell books gained, by realm.
    pub books: Vec<String>,
    pub retorts: Vec<crate::wizard::magic::Retort>,
}

/// Why a stack could not enter a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteError {
    Move(MoveError),
    /// No guarded site on that square.
    NoSite,
    /// A battle is already being fought.
    Busy,
}

impl SiteError {
    /// The key of its sentence in `content/text` (task 348).
    pub fn key(self) -> &'static str {
        match self {
            Self::Move(e) => e.key(),
            Self::NoSite => "refuse_map.site_error_no_site",
            Self::Busy => "refuse_map.site_error_busy",
        }
    }
}

/// A random spell of about `rarity` from a realm the wizard has no books in (arcane counts as
/// such a realm), trying dearer rarities when the first has nothing new; the wizard learns it.
/// The original's selection is muddled (ReMoM calls its index arithmetic wrong); this follows
/// its intent.
pub(crate) fn award_spell(
    wizard: &mut crate::wizard::sheet::WizardMut<'_>,
    content: &Content,
    rng: &mut crate::game::rng::GameRng,
    rarity: i32,
) -> Option<SpellId> {
    let mut realms: Vec<String> = ["nature", "sorcery", "chaos", "life", "death"]
        .iter()
        .filter(|r| wizard.view().books_in(r) == 0)
        .map(|r| r.to_string())
        .collect();
    realms.push("arcane".to_string());
    while !realms.is_empty() {
        let r = rng.roll(realms.len() as u32) as usize - 1;
        let realm = realms.remove(r);
        for step in 0..4 {
            let level = match (rarity + step).min(4) {
                1 => Rarity::Common,
                2 => Rarity::Uncommon,
                3 => Rarity::Rare,
                _ => Rarity::VeryRare,
            };
            let pool: Vec<SpellId> = crate::wizard::magic::spells_of(content, &realm, level)
                .into_iter()
                .filter(|s| !wizard.view().knows(*s))
                .filter(|s| {
                    !matches!(
                        content.rules.spells.get(*s).id.as_str(),
                        "spell_of_mastery" | "spell_of_return"
                    )
                })
                .collect();
            if !pool.is_empty() {
                let spell = pool[rng.roll(pool.len() as u32) as usize - 1];
                let mut spells = wizard.get_mut::<crate::wizard::sheet::KnownSpells>();
                spells.researchable.remove(&spell);
                spells.known.insert(spell);
                return Some(spell);
            }
        }
    }
    None
}

/// Spell book and retort treasure: three times in four a book of the site's realm (never life
/// with death or the reverse, nor past twelve books), otherwise a retort. A two-pick hoard
/// gives a two-pick retort or two books. The original's version loops and awards oddly (its
/// own comments call these bugs); this keeps the odds and the restrictions.
pub(crate) fn award_special(
    wizard: &mut crate::wizard::sheet::WizardMut<'_>,
    rng: &mut crate::game::rng::GameRng,
    lair: &LairInfo,
    picks: i32,
    loot: &mut Loot,
) {
    let realm = match lair.site.as_str() {
        "nature_node" => "nature".to_string(),
        "sorcery_node" => "sorcery".to_string(),
        "chaos_node" => "chaos".to_string(),
        s if s.contains("temple") => "life".to_string(),
        "dungeon" | "abandoned_keep" | "ruins" => "death".to_string(),
        _ => ["nature", "sorcery", "chaos", "life", "death"][rng.roll(5) as usize - 1].to_string(),
    };
    // Three times in four a book; the original turns to the other kind when the wizard is at
    // the limit of six retorts or twelve books.
    let wants_retort = if wizard.view().retorts().len() >= crate::wizard::MAX_RETORTS {
        false
    } else if wizard.view().total_books() >= 12 {
        true
    } else {
        rng.roll(100) > 75
    };
    if wants_retort {
        let double = |r| retort_cost(r) == 2;
        let (view, books) = (wizard.view(), wizard.view().books());
        let options: Vec<_> = ALL_RETORTS
            .iter()
            .copied()
            .filter(|r| !view.has(*r) && retort_available(*r, &books))
            .filter(|r| *r != crate::wizard::magic::Retort::Myrran)
            .filter(|&r| if picks == 2 { double(r) } else { !double(r) })
            .collect();
        if !options.is_empty() {
            let r = options[rng.roll(options.len() as u32) as usize - 1];
            wizard
                .get_mut::<crate::wizard::sheet::Retorts>()
                .0
                .insert(r);
            loot.retorts.push(r);
            return;
        }
    }
    for _ in 0..picks {
        let clash = (realm == "life" && wizard.view().books_in("death") > 0)
            || (realm == "death" && wizard.view().books_in("life") > 0);
        if clash || wizard.view().total_books() >= 12 {
            continue;
        }
        *wizard
            .get_mut::<crate::wizard::sheet::Books>()
            .0
            .entry(realm.clone())
            .or_insert(0) += 1;
        loot.books.push(realm.clone());
    }
}

/// Where a freed prisoner serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prisoner {
    /// A free offer of the hero on the wizard's offers.
    Offer,
    /// The hero stands in this city of a neutral player.
    Join(Position),
}

impl Game {
    /// Fight the guards of the site on `to`, which a stack next to it has been stopped at.
    /// The guards are created as neutral units on the site for the battle.
    pub fn enter_site(
        &mut self,
        units: &[hecs::Entity],
        to: Position,
    ) -> Result<Vec<Event>, SiteError> {
        if self.battle.is_some() {
            return Err(SiteError::Busy);
        }
        let lair = self.economy().lair_at(to).ok_or(SiteError::NoSite)?;
        let site = lair.entity;
        if lair.guards.is_empty() {
            return Err(SiteError::NoSite);
        }
        // The stack must be able to make the step (adjacent, moves left, and so on).
        let (_, entry) = self
            .economy()
            .plan_step(units, to)
            .map_err(SiteError::Move)?;
        if !matches!(entry, crate::unit::movement::Entry::Site(e) if e == site) {
            return Err(SiteError::NoSite);
        }
        let guards: Vec<(String, u32)> = lair.guards.clone();
        let mut defenders = Vec::new();
        for (id, count) in guards {
            let Some(unit) = self.content.rules.units.id_of(&id) else {
                continue;
            };
            for _ in 0..count {
                if defenders.len() < crate::unit::MAX_STACK {
                    defenders.push(self.spawn_unit(MONSTERS, to, unit));
                }
            }
        }
        match self.start_battle(units, &defenders) {
            Ok(events) => {
                if let Some(active) = self.battle.as_mut() {
                    active.site = Some(site);
                }
                Ok(events)
            }
            Err(_) => {
                for d in defenders {
                    self.despawn_with_spells(d);
                }
                Err(SiteError::Busy)
            }
        }
    }

    /// A stack that has just arrived on a square with an unguarded site takes the treasure.
    pub(crate) fn collect_site(&mut self, units: &[hecs::Entity], to: Position) -> Vec<Event> {
        let Some(owner) = units.first().and_then(|&u| self.units().unit_owner(u)) else {
            return Vec::new();
        };
        match self.economy().lair_at(to) {
            Some(l) if l.guards.is_empty() && owner != MONSTERS => self.claim_lair(l.entity, owner),
            _ => Vec::new(),
        }
    }

    /// Clear the guarded site on an entity: it stops being intact, and an encounter site vanishes
    /// from the map (towers and nodes stay). Returns the site as it was, with its treasure;
    /// `None` if there is none or it is cleared already.
    pub(crate) fn clear_site(&mut self, entity: hecs::Entity) -> Option<LairInfo> {
        let lair = self.economy().lair_of(entity).filter(|l| l.intact)?;
        if let Ok(mut l) = self.world.get::<&mut Lair>(entity) {
            l.intact = false;
        }
        let is_encounter = self
            .content
            .features
            .id_of(&lair.site)
            .and_then(|f| self.content.features.get(f).map(|d| d.kind))
            == Some(FeatureKind::Encounter);
        if is_encounter {
            self.despawn_feature(entity);
        }
        Some(lair)
    }

    /// Where a freed prisoner can serve `player`: a wizard gets a free offer; a neutral player
    /// gets the hero in the nearest of its cities that has room. `None` when it has no free
    /// hero slot (or no room).
    fn prisoner_place(&self, player: PlayerId, near: Position) -> Option<Prisoner> {
        if self.units().open_hero_slots(player) == 0 {
            return None;
        }
        if self.wizard(player).is_some() {
            return Some(Prisoner::Offer);
        }
        let economy = self.economy();
        let mut homes: Vec<Position> = economy
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .map(|(_, _, p)| p)
            .filter(|&p| self.units().units_at(p).len() < crate::unit::MAX_STACK)
            .collect();
        homes.sort_by_key(|p| {
            (
                p.plane != near.plane,
                p.x.abs_diff(near.x) + p.y.abs_diff(near.y),
            )
        });
        homes.first().map(|&p| Prisoner::Join(p))
    }

    /// Clear a lair and give `player` its treasure. What the winner cannot use is rerolled from
    /// its value (task 353, `reroll_hoard`).
    pub(crate) fn claim_lair(&mut self, site: hecs::Entity, player: PlayerId) -> Vec<Event> {
        let Some(mut lair) = self.clear_site(site) else {
            return Vec::new();
        };
        let pos = lair.pos;
        let wizard = self.wizard(player).is_some();
        let mut unused = 0i32;
        let mut prisoner = None;
        if lair.prisoner {
            prisoner = self.prisoner_place(player, pos);
            if prisoner.is_none() {
                unused += 1000;
            }
        }
        if !wizard {
            unused += match lair.special {
                1..=4 => i32::from(lair.special) * i32::from(lair.special) * 50,
                5 | 6 => 3000,
                _ => 0,
            };
            lair.special = 0;
        }
        let has_hero = wizard
            || matches!(prisoner, Some(Prisoner::Join(_)))
            || !self.units().hero_units(player).is_empty();
        if !has_hero {
            unused += lair.items.iter().map(|&v| v as i32).sum::<i32>();
            lair.items.clear();
        }
        if unused > 0 {
            let room = if has_hero {
                3 - lair.items.len().min(3)
            } else {
                0
            };
            let (gold, mana, items) = reroll_hoard(&mut self.rng, unused, room);
            lair.gold += gold;
            lair.mana += mana;
            lair.items.extend(items);
        }
        let mut loot = Loot {
            gold: lair.gold,
            mana: lair.mana,
            ..Loot::default()
        };
        let mut items: Vec<crate::item::Item> = Vec::new();
        for &value in &lair.items {
            items.push(make_item(
                &mut self.rng,
                &self.content.rules,
                Grade::Any,
                value,
                &mut self.prefab_taken,
            ));
        }
        if let Some(t) = self.treasury_mut(player) {
            t.gold += lair.gold as i32;
            t.mana += lair.mana as i32;
        }
        if let Some(mut w) = self
            .player_index
            .get(&player)
            .and_then(|&e| crate::wizard::entity::of_player(&self.world, e))
            .map(|e| crate::wizard::sheet::WizardMut::new(&mut self.world, e))
        {
            match lair.special {
                1..=4 => {
                    loot.spell = award_spell(
                        &mut w,
                        &self.content,
                        &mut self.rng,
                        i32::from(lair.special),
                    )
                }
                5 | 6 => {
                    let picks = i32::from(lair.special) - 4;
                    award_special(&mut w, &mut self.rng, &lair, picks, &mut loot);
                    // Famous brings its ten points of fame with it.
                    if loot.retorts.contains(&crate::wizard::magic::Retort::Famous) {
                        w.get_mut::<crate::wizard::sheet::Fame>().0 += 10;
                    }
                }
                _ => {}
            }
        }
        // A hoard of a book or a retort that gave nothing (nothing left to give) pays an item
        // worth 1200 instead, or 1000 mana when the hoard already has three items (the original
        // writes the item past the end of its list: ReMoM marks it a bug).
        if matches!(lair.special, 5 | 6) && loot.books.is_empty() && loot.retorts.is_empty() {
            if items.len() < 3 {
                items.push(make_item(
                    &mut self.rng,
                    &self.content.rules,
                    Grade::Any,
                    1200,
                    &mut self.prefab_taken,
                ));
            } else {
                loot.mana = 1000;
                if let Some(t) = self.treasury_mut(player) {
                    t.mana += 1000;
                }
            }
        }
        for item in &items {
            self.give_item(player, item.clone());
        }
        loot.items = items;
        // A prisoner is a hero for nothing: offered to a wizard, in service at once for a
        // neutral player.
        if let Some(place) = prisoner {
            if let Some(kind) = self.pick_hero(player, true, crate::hero::kind::Class::Any) {
                match place {
                    Prisoner::Offer => {
                        if let Some(o) = self.player_component_mut::<crate::player::Offers>(player)
                        {
                            o.0.insert(0, crate::player::offer::Offer::Hero { kind, price: 0 });
                            loot.prisoner = true;
                        }
                    }
                    Prisoner::Join(at) => {
                        loot.prisoner = self.spawn_hero(player, at, kind).is_ok();
                    }
                }
            }
        }
        let mut events = vec![Event::SiteCleared {
            player,
            pos,
            site: lair.site.clone(),
            guarded: !lair.guards.is_empty(),
            loot,
        }];
        if let Some(Event::SiteCleared { loot, .. }) = events.first() {
            if let Some(spell) = loot.spell {
                events.push(Event::SpellLearned { player, spell });
            }
        }
        events
    }

    /// After a battle at a site: the guards go, and a win clears the site.
    pub(crate) fn finish_site_battle(
        &mut self,
        site: hecs::Entity,
        winner: Option<PlayerId>,
        attackers: &[hecs::Entity],
        guards: &[hecs::Entity],
    ) -> Vec<Event> {
        let Some(lair) = self.economy().lair_of(site) else {
            return Vec::new();
        };
        let pos = lair.pos;
        // The guards were made for the fight and go with it. Other neutral units on the site
        // (the undead raised for the monsters) stay, as do guards the victors took as undead.
        for &u in guards {
            if self.units().unit_owner(u) == Some(MONSTERS) {
                self.despawn_with_spells(u);
            }
        }
        let Some(winner) = winner.filter(|&w| w != MONSTERS) else {
            return Vec::new();
        };
        let survivors: Vec<hecs::Entity> = attackers
            .iter()
            .copied()
            .filter(|&u| self.world.contains(u))
            .collect();
        let mut events = self.claim_lair(site, winner);
        if !survivors.is_empty() {
            let from = self.units().unit_position(survivors[0]);
            if self.move_stack(&survivors, pos).is_ok() {
                if let Some(from) = from {
                    events.push(Event::UnitsMoved {
                        units: survivors,
                        from,
                        to: pos,
                    });
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests;
