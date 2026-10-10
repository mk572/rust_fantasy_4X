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

use crate::city::{PlayerId, Position};
use crate::content::{Content, FeatureKind};
use crate::map::{Lair, Map, Plane};
use crate::mapgen::GenParams;
use crate::movement::MoveError;
use crate::rules::{Rarity, SpellId, Stat, UnitKind};
use crate::units::NEUTRAL;
use crate::wizard::{retort_available, retort_cost, ALL_RETORTS};
use crate::{Event, Game};

/// Most guards a site holds in total.
const MAX_GUARDS: u32 = 9;
/// Most guards of the first kind.
const MAX_FIRST_GUARDS: i32 = 8;
/// The original's two kinds of random encounter site: 25 strong ones to 32 weak ones.
const STRONG_SHARE: (u32, u32) = (25, 57);

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
    difficulty: i32,
) {
    let mut loot = guard_value + second_value / 2;
    if difficulty != 4 {
        loot = loot * 4 / (difficulty + 1);
    }
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

/// Make one lair from its budget (the original's `Create_Lair`).
#[allow(clippy::too_many_arguments)]
fn make_lair(
    rng: &mut Pcg64Mcg,
    content: &Content,
    plane: Plane,
    at: (u32, u32),
    site_id: &str,
    site: SiteType,
    budget: i32,
    difficulty: i32,
) -> Lair {
    let budget = budget * (difficulty + 1) / 4;
    let realm = guard_realm(rng, site);
    let list = candidates(content, realm);
    let (guards, first, second) = pick_guards(rng, &list, budget);
    let mut lair = Lair {
        plane,
        x: at.0,
        y: at.1,
        site: site_id.to_string(),
        intact: true,
        guards,
        gold: 0,
        mana: 0,
        items: Vec::new(),
        prisoner: false,
        special: 0,
    };
    fill_hoard(rng, &mut lair, site, plane, first, second, difficulty);
    lair
}

/// Give every tower, node and encounter site on the map its guards and treasure.
pub fn generate_lairs(seed: u64, params: &GenParams, map: &mut Map, content: &Content) {
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x1A12_5EED_600D_5CA1);
    let difficulty = i32::from(params.difficulty.min(4));
    let magic = i32::from(params.magic_strength.min(2));
    let mut lairs: Vec<Lair> = Vec::new();
    // Towers stand on both planes and share one set of guards and treasure.
    let mut seen_towers: Vec<(u32, u32)> = Vec::new();
    for plane in Plane::ALL {
        let grid = map.plane(plane);
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let Some(def) = grid
                    .feature(x as i32, y as i32)
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
                    (x, y),
                    &def.id,
                    SiteType::Tower,
                    budget,
                    difficulty,
                );
                lairs.push(Lair {
                    plane: Plane::Myrror,
                    ..lair.clone()
                });
                lairs.push(lair);
            }
        }
    }
    for node in map.nodes().to_vec() {
        let power = i32::from(node.power);
        let budget = (power * power * (4 + random(&mut rng, 11)) * (magic + 1) / 2).min(32767);
        let id = format!("{}_node", node.realm);
        let lair = make_lair(
            &mut rng,
            content,
            node.plane,
            (node.x, node.y),
            &id,
            SiteType::Node(&node.realm),
            budget,
            difficulty,
        );
        lairs.push(lair);
    }
    for plane in Plane::ALL {
        let grid = map.plane(plane);
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let Some(def) = grid
                    .feature(x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                else {
                    continue;
                };
                if def.kind != FeatureKind::Encounter {
                    continue;
                }
                let strong = rng.gen_range(0..STRONG_SHARE.1) < STRONG_SHARE.0;
                let budget = match (strong, plane) {
                    (true, Plane::Arcanus) => 50 + random(&mut rng, 29) * 50,
                    (true, Plane::Myrror) => 100 + random(&mut rng, 24) * 100,
                    (false, Plane::Arcanus) => random(&mut rng, 10) * 10,
                    (false, Plane::Myrror) => random(&mut rng, 20) * 10,
                };
                let lair = make_lair(
                    &mut rng,
                    content,
                    plane,
                    (x, y),
                    &def.id,
                    site_type(&def.id, def.kind),
                    budget,
                    difficulty,
                );
                lairs.push(lair);
            }
        }
    }
    map.set_lairs(lairs);
}

/// What a cleared site gave its conqueror.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loot {
    pub gold: u32,
    pub mana: u32,
    /// Values of magic items found; the items themselves wait for the hero tasks.
    pub items: Vec<u32>,
    /// A prisoner hero was found (hiring waits for the hero tasks).
    pub prisoner: bool,
    pub spell: Option<SpellId>,
    /// Spell books gained, by realm.
    pub books: Vec<String>,
    pub retorts: Vec<crate::magic::Retort>,
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

impl Game {
    /// The intact lair on a square.
    pub fn lair_at(&self, pos: Position) -> Option<(usize, &Lair)> {
        let map = self.map.as_ref()?;
        let i = map.lair_at(pos.plane, pos.x, pos.y)?;
        Some((i, &map.lairs()[i]))
    }

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
        let (index, lair) = self.lair_at(to).ok_or(SiteError::NoSite)?;
        if lair.guards.is_empty() {
            return Err(SiteError::NoSite);
        }
        // The stack must be able to make the step (adjacent, moves left, and so on).
        let (_, entry) = self.plan_step(units, to).map_err(SiteError::Move)?;
        if !matches!(entry, crate::movement::Entry::Site(i) if i == index) {
            return Err(SiteError::NoSite);
        }
        let guards: Vec<(String, u32)> = lair.guards.clone();
        let mut defenders = Vec::new();
        for (id, count) in guards {
            let Some(unit) = self.content.rules.units.id_of(&id) else {
                continue;
            };
            for _ in 0..count {
                if defenders.len() < crate::units::MAX_STACK {
                    defenders.push(self.spawn_unit(NEUTRAL, to, unit));
                }
            }
        }
        match self.start_battle(units, &defenders) {
            Ok(events) => {
                if let Some(active) = self.battle.as_mut() {
                    active.site = Some(index);
                }
                Ok(events)
            }
            Err(_) => {
                for d in defenders {
                    let _ = self.world.despawn(d);
                }
                Err(SiteError::Busy)
            }
        }
    }

    /// A stack that has just arrived on a square with an unguarded site takes the treasure.
    pub(crate) fn collect_site(&mut self, units: &[hecs::Entity], to: Position) -> Vec<Event> {
        let Some(owner) = units.first().and_then(|&u| self.unit_owner(u)) else {
            return Vec::new();
        };
        match self.lair_at(to) {
            Some((i, l)) if l.guards.is_empty() && owner != NEUTRAL => self.claim_lair(i, owner),
            _ => Vec::new(),
        }
    }

    /// Clear a lair and give `player` its treasure.
    pub(crate) fn claim_lair(&mut self, index: usize, player: PlayerId) -> Vec<Event> {
        let Some(lair) = self
            .map
            .as_ref()
            .and_then(|m| m.lairs().get(index))
            .cloned()
        else {
            return Vec::new();
        };
        if !lair.intact {
            return Vec::new();
        }
        let pos = Position {
            plane: lair.plane,
            x: lair.x,
            y: lair.y,
        };
        // Clear it, and a tower's twin on the other plane.
        if let Some(map) = self.map.as_mut() {
            let is_tower = lair.site == "tower";
            for l in (0..map.lairs().len()).collect::<Vec<_>>() {
                let Some(other) = map.lair_mut(l) else {
                    continue;
                };
                let same = l == index
                    || (is_tower
                        && other.site == "tower"
                        && (other.x, other.y) == (lair.x, lair.y));
                if same {
                    other.intact = false;
                }
            }
        }
        // An encounter site vanishes from the map; towers and nodes stay.
        let is_encounter = self
            .content
            .features
            .id_of(&lair.site)
            .and_then(|f| self.content.features.get(f).map(|d| d.kind))
            == Some(FeatureKind::Encounter);
        if is_encounter {
            if let Some(map) = self.map.as_mut() {
                map.plane_mut(lair.plane).set_feature(lair.x, lair.y, None);
            }
        }
        let mut loot = Loot {
            gold: lair.gold,
            mana: lair.mana,
            items: lair.items.clone(),
            prisoner: lair.prisoner,
            ..Loot::default()
        };
        if let Some(p) = self.player_mut(player) {
            p.gold += lair.gold as i32;
            p.mana += lair.mana as i32;
        }
        match lair.special {
            1..=4 => loot.spell = self.award_spell(player, i32::from(lair.special)),
            5 | 6 => {
                let picks = i32::from(lair.special) - 4;
                self.award_special(player, &lair, picks, &mut loot);
            }
            _ => {}
        }
        let mut events = vec![Event::SiteCleared { player, pos, loot }];
        if let Some(Event::SiteCleared { loot, .. }) = events.first() {
            if let Some(spell) = loot.spell {
                events.push(Event::SpellLearned { player, spell });
            }
        }
        events
    }

    /// A random spell of about `rarity` from a realm the wizard has no books in (arcane
    /// counts as such a realm), trying dearer rarities when the first has nothing new. The
    /// original's selection is muddled (ReMoM calls its index arithmetic wrong); this follows
    /// its intent.
    fn award_spell(&mut self, player: PlayerId, rarity: i32) -> Option<SpellId> {
        let books = self.magic(player)?.books.clone();
        let mut realms: Vec<String> = ["nature", "sorcery", "chaos", "life", "death"]
            .iter()
            .filter(|r| books.get(**r).copied().unwrap_or(0) == 0)
            .map(|r| r.to_string())
            .collect();
        realms.push("arcane".to_string());
        while !realms.is_empty() {
            let r = self.roll(realms.len() as u32) as usize - 1;
            let realm = realms.remove(r);
            for step in 0..4 {
                let level = match (rarity + step).min(4) {
                    1 => Rarity::Common,
                    2 => Rarity::Uncommon,
                    3 => Rarity::Rare,
                    _ => Rarity::VeryRare,
                };
                let known = self.magic(player)?.known.clone();
                let pool: Vec<SpellId> = self
                    .spells_of(&realm, level)
                    .into_iter()
                    .filter(|s| !known.contains(s))
                    .filter(|s| {
                        !matches!(
                            self.content.rules.spells.get(*s).id.as_str(),
                            "spell_of_mastery" | "spell_of_return"
                        )
                    })
                    .collect();
                if !pool.is_empty() {
                    let spell = pool[self.roll(pool.len() as u32) as usize - 1];
                    if let Some(m) = self.magic_mut(player) {
                        m.researchable.remove(&spell);
                        m.known.insert(spell);
                    }
                    return Some(spell);
                }
            }
        }
        None
    }

    /// Spell book and retort treasure: three times in four a book of the site's realm (never
    /// life with death or the reverse, nor past twelve books), otherwise a retort. A two-pick
    /// hoard gives a two-pick retort or two books. The original's version loops and awards
    /// oddly (its own comments call these bugs); this keeps the odds and the restrictions.
    fn award_special(&mut self, player: PlayerId, lair: &Lair, picks: i32, loot: &mut Loot) {
        let realm = match lair.site.as_str() {
            "nature_node" => "nature".to_string(),
            "sorcery_node" => "sorcery".to_string(),
            "chaos_node" => "chaos".to_string(),
            s if s.contains("temple") => "life".to_string(),
            "dungeon" | "abandoned_keep" | "ruins" => "death".to_string(),
            _ => ["nature", "sorcery", "chaos", "life", "death"][self.roll(5) as usize - 1]
                .to_string(),
        };
        let wants_retort = self.roll(100) > 75;
        if wants_retort {
            let Some(m) = self.magic(player) else {
                return;
            };
            let double = |r| retort_cost(r) == 2;
            let options: Vec<_> = ALL_RETORTS
                .iter()
                .copied()
                .filter(|r| !m.has(*r) && retort_available(*r, &m.books))
                .filter(|r| *r != crate::magic::Retort::Myrran)
                .filter(|&r| if picks == 2 { double(r) } else { !double(r) })
                .collect();
            if !options.is_empty() {
                let r = options[self.roll(options.len() as u32) as usize - 1];
                if let Some(m) = self.magic_mut(player) {
                    m.retorts.insert(r);
                }
                loot.retorts.push(r);
                return;
            }
        }
        for _ in 0..picks {
            let Some(m) = self.magic(player) else {
                return;
            };
            let clash = (realm == "life" && m.books_in("death") > 0)
                || (realm == "death" && m.books_in("life") > 0);
            if clash || m.total_books() >= 12 {
                continue;
            }
            if let Some(m) = self.magic_mut(player) {
                *m.books.entry(realm.clone()).or_insert(0) += 1;
            }
            loot.books.push(realm.clone());
        }
    }

    /// After a battle at a site: the guards go, and a win clears the site.
    pub(crate) fn finish_site_battle(
        &mut self,
        index: usize,
        winner: Option<PlayerId>,
        attackers: &[hecs::Entity],
    ) -> Vec<Event> {
        let Some(lair) = self
            .map
            .as_ref()
            .and_then(|m| m.lairs().get(index))
            .cloned()
        else {
            return Vec::new();
        };
        let pos = Position {
            plane: lair.plane,
            x: lair.x,
            y: lair.y,
        };
        for u in self.units_at(pos) {
            if self.unit_owner(u) == Some(NEUTRAL) {
                let _ = self.world.despawn(u);
            }
        }
        let Some(winner) = winner.filter(|&w| w != NEUTRAL) else {
            return Vec::new();
        };
        let survivors: Vec<hecs::Entity> = attackers
            .iter()
            .copied()
            .filter(|&u| self.world.contains(u))
            .collect();
        let mut events = self.claim_lair(index, winner);
        if !survivors.is_empty() {
            let from = self.unit_position(survivors[0]);
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
mod tests {
    use super::*;
    use crate::battle::{BattleCommand, Side};
    use crate::city::PlayerId;
    use crate::map::{Grid, Plane};
    use crate::player::Player;
    use crate::rules::{MovementType, Registry, UnitDef};
    use crate::testutil::{content_with_magic, unit};
    use crate::Command;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn summoned(id: &str, realm: &str, cost: u32) -> UnitDef {
        let mut u = unit(id, MovementType::Walking, 2, &[]);
        u.kind = UnitKind::Summoned;
        u.realm = Some(realm.into());
        u.cost = cost;
        u
    }

    /// Magic test content plus guards of death and chaos, and a weak one to beat.
    fn content() -> Content {
        let mut c = content_with_magic();
        let mut units = c.rules.units.defs().to_vec();
        units.extend([
            summoned("skeletons", "death", 25),
            summoned("ghouls", "death", 80),
            summoned("wraiths", "death", 500),
            summoned("hell_hounds", "chaos", 40),
            summoned("great_drake", "chaos", 900),
            summoned("nature_guard", "nature", 70),
            summoned("angel", "life", 550),
            summoned("djinn", "sorcery", 650),
        ]);
        let mut ship = summoned("floating_island", "death", 9999);
        ship.stats.insert(Stat::Transport, 8);
        units.push(ship);
        c.rules.units = Registry::new(units);
        c
    }

    fn world(seed: u64) -> Game {
        let c = content();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(seed);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.fog = crate::fog::Fog::new(24, 14);
        for id in [P0, PlayerId(1)] {
            let mut p = Player::new(id, "W");
            p.gold = 100;
            p.mana = 100;
            g.players.push(p);
        }
        g.init_wizard(P0, &[("life", 2)], &[]).unwrap();
        g
    }

    fn put_feature(g: &mut Game, plane: Plane, x: u32, y: u32, id: &str) {
        let f = g.content.features.id_of(id).unwrap();
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(plane)
            .set_feature(x, y, Some(f));
    }

    fn lair(x: u32, y: u32, guards: Vec<(&str, u32)>) -> Lair {
        Lair {
            plane: Plane::Arcanus,
            x,
            y,
            site: "lair".into(),
            intact: true,
            guards: guards
                .into_iter()
                .map(|(g, n)| (g.to_string(), n))
                .collect(),
            gold: 120,
            mana: 30,
            items: vec![400],
            prisoner: true,
            special: 0,
        }
    }

    #[test]
    fn only_summoned_non_transport_units_of_the_realm_can_guard() {
        let c = content();
        let death: Vec<String> = candidates(&c, "death")
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            death,
            ["skeletons", "ghouls", "wraiths"],
            "no ship, no other realm"
        );
        assert!(candidates(&c, "arcane").is_empty());
    }

    #[test]
    fn the_dearest_affordable_unit_is_chosen_and_the_first_of_equals() {
        let list = vec![
            ("a".to_string(), 25),
            ("b".to_string(), 80),
            ("c".to_string(), 80),
            ("d".to_string(), 500),
        ];
        assert_eq!(dearest_under(&list, 501, None).unwrap().0, "d");
        assert_eq!(
            dearest_under(&list, 500, None).unwrap().0,
            "b",
            "strictly cheaper"
        );
        assert_eq!(dearest_under(&list, 500, Some("b")).unwrap().0, "c");
        assert_eq!(dearest_under(&list, 25, None), None);
    }

    #[test]
    fn guards_fit_the_budget_and_the_stack() {
        let list = candidates(&content(), "death");
        let mut rng = Pcg64Mcg::seed_from_u64(1);
        for budget in [0, 20, 60, 200, 700, 1500, 4000] {
            for _ in 0..50 {
                let (guards, first, second) = pick_guards(&mut rng, &list, budget);
                let total: u32 = guards.iter().map(|g| g.1).sum();
                assert!(total <= MAX_GUARDS, "{total} guards for {budget}");
                assert!(first + second <= budget, "{first}+{second} over {budget}");
                if guards.len() == 2 {
                    assert_ne!(guards[0].0, guards[1].0, "two different kinds");
                    assert!(guards[0].1 <= MAX_FIRST_GUARDS as u32);
                }
            }
        }
        // A budget below the cheapest unit buys nothing.
        let (guards, ..) = pick_guards(&mut rng, &list, 20);
        assert!(guards.is_empty());
    }

    #[test]
    fn a_site_type_decides_the_guards_realm() {
        let mut rng = Pcg64Mcg::seed_from_u64(2);
        for _ in 0..100 {
            assert_eq!(guard_realm(&mut rng, SiteType::Node("sorcery")), "sorcery");
            assert_eq!(guard_realm(&mut rng, SiteType::Node("nature")), "nature");
            assert_eq!(guard_realm(&mut rng, SiteType::Node("chaos")), "chaos");
            assert!(["death", "life"].contains(&guard_realm(&mut rng, SiteType::Holy)));
            assert!(["death", "chaos", "nature"].contains(&guard_realm(&mut rng, SiteType::Wild)));
        }
        let seen: std::collections::BTreeSet<_> = (0..300)
            .map(|_| guard_realm(&mut rng, SiteType::Tower))
            .collect();
        assert_eq!(seen.len(), 5, "towers draw on every realm");
        assert_eq!(
            site_type("ancient_temple", FeatureKind::Encounter),
            SiteType::Holy
        );
        assert_eq!(site_type("ruins", FeatureKind::Encounter), SiteType::Holy);
        assert_eq!(site_type("dungeon", FeatureKind::Encounter), SiteType::Wild);
        assert_eq!(
            site_type("chaos_node", FeatureKind::Node),
            SiteType::Node("chaos")
        );
    }

    #[test]
    fn every_site_gets_a_lair_and_towers_share_one_on_both_planes() {
        let mut g = world(1);
        put_feature(&mut g, Plane::Arcanus, 3, 3, "tower");
        put_feature(&mut g, Plane::Myrror, 3, 3, "tower");
        put_feature(&mut g, Plane::Arcanus, 8, 5, "lair");
        put_feature(&mut g, Plane::Myrror, 9, 5, "lair");
        put_feature(&mut g, Plane::Myrror, 12, 9, "lair");
        let params = GenParams::default();
        let mut map = g.map.take().unwrap();
        generate_lairs(11, &params, &mut map, &g.content);
        let lairs = map.lairs();
        assert_eq!(lairs.len(), 2 + 3, "tower twice, three encounter sites");
        assert!(lairs.iter().all(|l| l.intact));
        let towers: Vec<_> = lairs.iter().filter(|l| l.site == "tower").collect();
        assert_eq!(towers.len(), 2);
        assert_ne!(towers[0].plane, towers[1].plane);
        assert_eq!(towers[0].guards, towers[1].guards);
        assert_eq!(
            (towers[0].gold, towers[0].special),
            (towers[1].gold, towers[1].special)
        );
        assert!(towers[0].special >= 1, "a tower always holds a spell");
        // The same seed gives the same lairs.
        let mut again = map.clone();
        generate_lairs(11, &params, &mut again, &g.content);
        assert_eq!(again.lairs(), map.lairs());
        generate_lairs(12, &params, &mut again, &g.content);
        assert_ne!(again.lairs(), map.lairs());
    }

    #[test]
    fn hoards_follow_the_original_rules() {
        let g = world(1);
        let mut rng = Pcg64Mcg::seed_from_u64(5);
        let mut saw_items = false;
        let mut saw_big = false;
        for i in 0..3000 {
            let plane = if i % 2 == 0 {
                Plane::Arcanus
            } else {
                Plane::Myrror
            };
            let l = make_lair(
                &mut rng,
                &g.content,
                plane,
                (1, 1),
                "lair",
                SiteType::Wild,
                200 + (i % 40) * 100,
                2,
            );
            assert_eq!(l.gold % 10, 0);
            assert_eq!(l.mana % 10, 0);
            assert!(l.items.len() <= 3);
            assert!(l.items.iter().all(|&v| v >= 100 && v % 10 == 0));
            assert!(l.special <= 6);
            if l.special > 4 {
                saw_big = true;
                assert_eq!(
                    (l.gold, l.mana, l.items.len(), l.prisoner),
                    (0, 0, 0, false)
                );
            }
            saw_items |= !l.items.is_empty();
        }
        assert!(saw_items && saw_big);
        // Impossible difficulty keeps the whole budget: more guards than on easy.
        let value = |difficulty| {
            let mut rng = Pcg64Mcg::seed_from_u64(9);
            (0..200)
                .map(|_| {
                    make_lair(
                        &mut rng,
                        &g.content,
                        Plane::Arcanus,
                        (1, 1),
                        "lair",
                        SiteType::Wild,
                        900,
                        difficulty,
                    )
                    .guard_count()
                })
                .sum::<u32>()
        };
        assert!(value(4) > value(0));
    }

    #[test]
    fn a_stack_is_stopped_by_a_guarded_site_and_nothing_moves() {
        let mut g = world(2);
        put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
        g.map
            .as_mut()
            .unwrap()
            .set_lairs(vec![lair(5, 5, vec![("skeletons", 3)])]);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let w = g.spawn_unit(P0, at(4, 5), walker);
        let events = g.apply(Command::MoveStack {
            units: vec![w],
            to: at(5, 5),
        });
        assert_eq!(
            events,
            vec![Event::SiteEncounter {
                units: vec![w],
                from: at(4, 5),
                to: at(5, 5),
                lair: 0
            }]
        );
        assert_eq!(g.unit_position(w), Some(at(4, 5)));
        assert_eq!(g.unit_moves_left(w), 4, "no moves spent");
    }

    #[test]
    fn walking_into_an_unguarded_site_takes_the_treasure_and_clears_it() {
        let mut g = world(3);
        put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
        g.map.as_mut().unwrap().set_lairs(vec![lair(5, 5, vec![])]);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let w = g.spawn_unit(P0, at(4, 5), walker);
        let events = g.apply(Command::MoveStack {
            units: vec![w],
            to: at(5, 5),
        });
        assert!(matches!(events[0], Event::UnitsMoved { .. }));
        let Some(Event::SiteCleared { player, loot, .. }) = events.get(1) else {
            panic!("expected the site to be cleared: {events:?}");
        };
        assert_eq!(*player, P0);
        assert_eq!((loot.gold, loot.mana), (120, 30));
        assert_eq!(loot.items, vec![400]);
        assert!(loot.prisoner);
        assert_eq!(
            (g.player(P0).unwrap().gold, g.player(P0).unwrap().mana),
            (220, 130)
        );
        let map = g.map.as_ref().unwrap();
        assert!(map.lair_at(Plane::Arcanus, 5, 5).is_none());
        assert_eq!(
            map.plane(Plane::Arcanus).feature(5, 5),
            None,
            "the site is gone"
        );
        // Nothing to take the second time.
        let again = g.claim_lair(0, P0);
        assert!(again.is_empty());
    }

    #[test]
    fn the_hoard_can_hold_a_spell_book_a_retort_or_a_spell() {
        let mut g = world(4);
        let mut l = lair(5, 5, vec![]);
        l.gold = 0;
        l.mana = 0;
        l.items.clear();
        l.prisoner = false;
        // A spell: from a realm the wizard has no books in, newly known.
        l.special = 2;
        g.map.as_mut().unwrap().set_lairs(vec![l.clone()]);
        let known_before = g.magic(P0).unwrap().known.len();
        let events = g.claim_lair(0, P0);
        let Some(Event::SiteCleared { loot, .. }) = events.first() else {
            panic!("{events:?}");
        };
        let spell = loot.spell.expect("a spell");
        assert!(g.magic(P0).unwrap().known.contains(&spell));
        assert_eq!(g.magic(P0).unwrap().known.len(), known_before + 1);
        assert_ne!(
            g.content.rules.spells.get(spell).realm,
            "life",
            "a realm without books"
        );
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::SpellLearned { .. })));
        // A book of a dungeon's realm (death): refused while the wizard has life books.
        let mut l = lair(6, 6, vec![]);
        l.site = "dungeon".into();
        l.special = 5;
        let mut loot = Loot::default();
        g.award_special(P0, &l, 1, &mut loot);
        assert!(
            !loot.books.contains(&"death".to_string()),
            "life and death exclude each other"
        );
        // A temple gives a life book, up to the twelve-book limit.
        let mut l = lair(7, 7, vec![]);
        l.site = "ancient_temple".into();
        let mut books = 0;
        for _ in 0..40 {
            let mut loot = Loot::default();
            g.award_special(P0, &l, 1, &mut loot);
            books += loot.books.len();
            assert!(loot.books.iter().all(|b| b == "life"));
        }
        assert!(books > 0);
        assert!(g.magic(P0).unwrap().total_books() <= 12);
    }

    fn fight_to_the_end(g: &mut Game) {
        use crate::battle::map::distance;
        for _ in 0..300 {
            if !g.in_battle() {
                return;
            }
            let (side, ids): (Side, Vec<usize>) = {
                let b = g.battle().unwrap();
                (b.active, b.active_units(b.active).map(|u| u.id).collect())
            };
            for id in ids {
                if !g.in_battle() {
                    return;
                }
                let target = {
                    let b = g.battle().unwrap();
                    b.active_units(side.other()).next().map(|t| (t.id, t.cell))
                };
                let Some((t, cell)) = target else { break };
                let step = {
                    let b = g.battle().unwrap();
                    b.reachable(id)
                        .into_iter()
                        .min_by_key(|(p, _)| distance(*p, cell))
                        .map(|(p, _)| p)
                };
                if let Some(to) = step {
                    g.apply(Command::Battle(BattleCommand::Move { unit: id, to }));
                }
                g.apply(Command::Battle(BattleCommand::Attack {
                    unit: id,
                    target: t,
                }));
            }
            if g.in_battle() {
                g.apply(Command::Battle(BattleCommand::EndTurn));
            }
        }
        panic!("the battle did not finish");
    }

    #[test]
    fn beating_the_guards_clears_the_site_pays_out_and_moves_in() {
        let mut g = world(5);
        put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
        g.map
            .as_mut()
            .unwrap()
            .set_lairs(vec![lair(5, 5, vec![("skeletons", 1)])]);
        // Make the guard feeble and the attackers strong.
        let mut units = g.content.rules.units.defs().to_vec();
        for u in &mut units {
            match u.id.as_str() {
                "skeletons" => {
                    u.stats.insert(Stat::Melee, 0);
                    u.stats.insert(Stat::Defense, 0);
                    u.stats.insert(Stat::MaxHp, 1);
                    u.stats.insert(Stat::Figures, 1);
                }
                "walker" => {
                    u.stats.insert(Stat::Melee, 10);
                    u.stats.insert(Stat::Defense, 10);
                }
                _ => {}
            }
        }
        g.content.rules.units = Registry::new(units);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let w = g.spawn_unit(P0, at(4, 5), walker);
        let events = g.apply(Command::EnterSite {
            units: vec![w],
            to: at(5, 5),
        });
        assert!(matches!(events[0], Event::BattleStarted { defender, .. } if defender == NEUTRAL));
        assert!(g.in_battle());
        assert_eq!(
            g.units_at(at(5, 5)).len(),
            1,
            "one guard stands on the site"
        );
        fight_to_the_end(&mut g);
        assert!(g.world.contains(w));
        assert!(
            g.map
                .as_ref()
                .unwrap()
                .lair_at(Plane::Arcanus, 5, 5)
                .is_none(),
            "cleared"
        );
        assert_eq!(
            g.map.as_ref().unwrap().plane(Plane::Arcanus).feature(5, 5),
            None
        );
        assert_eq!(g.player(P0).unwrap().gold, 220);
        assert_eq!(g.unit_position(w), Some(at(5, 5)), "the victors moved in");
        assert!(
            g.units_at(at(5, 5))
                .iter()
                .all(|&u| g.unit_owner(u) == Some(P0)),
            "no guard is left"
        );
    }

    #[test]
    fn losing_or_fleeing_leaves_the_site_intact_and_resets_the_guards() {
        let mut g = world(6);
        put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
        g.map
            .as_mut()
            .unwrap()
            .set_lairs(vec![lair(5, 5, vec![("skeletons", 3)])]);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let w = g.spawn_unit(P0, at(4, 5), walker);
        g.apply(Command::EnterSite {
            units: vec![w],
            to: at(5, 5),
        });
        assert_eq!(g.units_at(at(5, 5)).len(), 3);
        g.apply(Command::Battle(BattleCommand::Flee));
        assert!(!g.in_battle());
        assert!(g
            .map
            .as_ref()
            .unwrap()
            .lair_at(Plane::Arcanus, 5, 5)
            .is_some());
        assert!(
            g.units_at(at(5, 5)).is_empty(),
            "the guards are made anew on each visit"
        );
        assert_eq!(g.player(P0).unwrap().gold, 100, "no treasure");
        assert_eq!(g.unit_position(w), Some(at(4, 5)));
    }

    #[test]
    fn only_a_guarded_site_next_to_the_stack_can_be_entered() {
        let mut g = world(7);
        put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
        g.map
            .as_mut()
            .unwrap()
            .set_lairs(vec![lair(5, 5, vec![("skeletons", 1)]), lair(9, 9, vec![])]);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let w = g.spawn_unit(P0, at(2, 2), walker);
        let reject = |g: &mut Game, to| g.apply(Command::EnterSite { units: vec![w], to });
        assert_eq!(
            reject(&mut g, at(5, 5)),
            vec![Event::Rejected(crate::Rejection::Site(SiteError::Move(
                MoveError::NotAdjacent
            )))]
        );
        assert_eq!(
            reject(&mut g, at(3, 3)),
            vec![Event::Rejected(crate::Rejection::Site(SiteError::NoSite))]
        );
        assert!(!g.in_battle());
    }

    #[test]
    fn clearing_a_tower_clears_it_on_both_planes() {
        let mut g = world(8);
        put_feature(&mut g, Plane::Arcanus, 5, 5, "tower");
        put_feature(&mut g, Plane::Myrror, 5, 5, "tower");
        let mut a = lair(5, 5, vec![]);
        a.site = "tower".into();
        let b = Lair {
            plane: Plane::Myrror,
            ..a.clone()
        };
        g.map.as_mut().unwrap().set_lairs(vec![b, a]);
        g.claim_lair(1, P0);
        let map = g.map.as_ref().unwrap();
        assert!(map.lairs().iter().all(|l| !l.intact));
        assert!(
            map.plane(Plane::Arcanus).feature(5, 5).is_some(),
            "the tower itself stays"
        );
    }

    #[test]
    fn lairs_survive_a_save() {
        let mut g = world(9);
        g.map
            .as_mut()
            .unwrap()
            .set_lairs(vec![lair(5, 5, vec![("skeletons", 2), ("ghouls", 1)])]);
        let snap = g.snapshot();
        let back = snap.restore(g.content.clone()).unwrap();
        assert_eq!(
            back.map.as_ref().unwrap().lairs(),
            g.map.as_ref().unwrap().lairs()
        );
    }
}
