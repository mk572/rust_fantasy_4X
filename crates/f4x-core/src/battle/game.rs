//! Joining battles to the game: building one from overland units, running it through
//! commands, and writing its results back.

use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::{MovementType, Stat};
use crate::unit::Hurt;

use super::field::Field;
use super::immunity::Realm;
use super::map::{city_deployment, deployment, layout, BattleMap, Pos};
use super::{Battle, BattleCommand, BattleError, BattleUnit, RangedKind, Side, Status};

/// A battle in progress and the overland units its fighters were copied from.
pub struct ActiveBattle {
    pub battle: Battle,
    /// `BattleUnit::source` indexes this list.
    pub sources: Vec<hecs::Entity>,
    /// Units that stayed out of a fight on the sea (land units aboard a ship): they share the
    /// fate of their side.
    pub left_out: Vec<(Side, hecs::Entity)>,
    /// The lair being assaulted, if this is a fight against a site's guards (`Battle::site` is
    /// the square the fight is on).
    pub lair: Option<hecs::Entity>,
}

/// The ships of a sea battle that got a deck, with its squares (the rules' frame).
#[derive(Debug, Default)]
struct ShipDeckPlan {
    attackers: Vec<(hecs::Entity, Vec<Pos>)>,
    defenders: Vec<(hecs::Entity, Vec<Pos>)>,
}

impl ShipDeckPlan {
    fn of(&self, side: Side) -> &[(hecs::Entity, Vec<Pos>)] {
        match side {
            Side::Attacker => &self.attackers,
            Side::Defender => &self.defenders,
        }
    }

    fn cells(&self) -> Vec<Pos> {
        self.attackers
            .iter()
            .chain(&self.defenders)
            .flat_map(|(_, c)| c.iter().copied())
            .collect()
    }

    fn covers(&self, cell: Pos) -> bool {
        self.attackers
            .iter()
            .chain(&self.defenders)
            .any(|(_, c)| c.contains(&cell))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleStartError {
    /// A battle is already being fought.
    InProgress,
    /// A side is empty, or its units are not one player's on one square.
    BadSides,
    /// An auto-resolved attack must come from an adjacent square.
    NotAdjacent,
}

impl BattleStartError {
    /// The key of its sentence in `content/text` (task 348).
    pub fn key(self) -> &'static str {
        match self {
            Self::InProgress => "refuse_map.battle_start_error_in_progress",
            Self::BadSides => "refuse_map.battle_start_error_bad_sides",
            Self::NotAdjacent => "refuse_map.battle_start_error_not_adjacent",
        }
    }
}

/// What kind of ranged attack the unit's abilities give it.
pub fn ranged_kind_of(abilities: &[String]) -> RangedKind {
    let has = |names: &[&str]| abilities.iter().any(|a| names.contains(&a.as_str()));
    if has(&["bow", "sling", "rock_throwing"]) {
        RangedKind::Missile
    } else if has(&[
        "thrown",
        "fire_breath",
        "lightning_breath",
        "stoning_gaze",
        "death_gaze",
        "multi_gaze",
    ]) {
        RangedKind::Short
    } else if has(&[
        "fireball_attack",
        "lightning_bolt",
        "ice_bolt",
        "nature_bolt",
        "sorcery_bolt",
        "sprite_bolt",
        "drow_bolt",
        "shaman_bolt",
        "death_bolt",
    ]) {
        RangedKind::Magic
    } else {
        RangedKind::None
    }
}

/// Something that plays a wizard's side of a battle: it is asked for one command at a time.
/// The core's own player is [`super::ai::next_command`]; a learned one (`f4x-learn`) is plugged
/// in with [`Game::set_battle_brain`]. The brain sees the whole game, as a player does.
pub trait BattleBrain {
    /// The next command for the side to play in the battle in progress.
    fn command(&mut self, game: &Game) -> BattleCommand;
}

impl Game {
    /// Let `brain` play the battles of `player` (an AI wizard) from now on; `None` goes back to
    /// the plain battle AI.
    pub fn set_battle_brain(&mut self, player: PlayerId, brain: Option<Box<dyn BattleBrain>>) {
        let Some(player) = self.player_entity(player) else {
            return;
        };
        self.battle_brains.retain(|(p, _)| *p != player);
        if let Some(b) = brain {
            self.battle_brains.push((player, b));
        }
    }

    /// The next command for the side to play: from the brain of its wizard if it has one, else
    /// from the plain battle AI. `None` when no battle is on.
    pub fn next_battle_command(&mut self) -> Option<BattleCommand> {
        let player = {
            let b = self.battle()?;
            b.player_of(b.active)
        };
        let owner = self.player_entity(player);
        if let Some(k) = self
            .battle_brains
            .iter()
            .position(|(p, _)| Some(*p) == owner)
        {
            // Take the brain out while it looks at the game.
            let (p, mut brain) = self.battle_brains.swap_remove(k);
            let c = brain.command(self);
            self.battle_brains.push((p, brain));
            return Some(c);
        }
        super::brain::next_command(self)
    }

    /// Is a battle being fought?
    pub fn in_battle(&self) -> bool {
        self.battle.is_some()
    }

    /// Limit the mana the wizard of `side` may spend in the battle in progress (`None`: no
    /// limit). The wizard's own casting skill and reserve still apply.
    pub fn set_battle_mana_budget(&mut self, side: super::Side, budget: Option<u32>) {
        if let Some(a) = self.battle.as_mut() {
            a.battle.set_mana_budget(side, budget);
        }
    }

    /// What a strategic battle (the battle AI playing for `player` in the background) may spend
    /// of the mana reserve unless the player says otherwise: the lower of the casting skill and
    /// half the reserve, at least one. `None` when the wizard has no mana or knows no spell
    /// that can be cast in a battle: there is nothing to ask.
    pub fn strategic_mana_default(&self, player: PlayerId) -> Option<u32> {
        let mana = u32::try_from(self.player(player)?.treasury().mana).ok()?;
        if mana == 0 || self.battle_spells(player).is_empty() {
            return None;
        }
        let skill = self.wizard(player)?.casting_skill().max(0) as u32;
        Some(skill.min(mana / 2).max(1).min(mana))
    }

    /// The battle in progress.
    pub fn battle(&self) -> Option<&Battle> {
        self.battle.as_ref().map(|b| &b.battle)
    }

    /// Copy an overland unit into battle form.
    /// What a unit can cast in a battle on its own: a hero's innate spells, the charges of its
    /// items' spells and its spell save, and the spells its abilities cast (once a battle).
    fn unit_casting(&self, unit: hecs::Entity, abilities: &[String]) -> super::UnitCasting {
        let spells = &self.content.rules.spells;
        let mut casting = super::UnitCasting::default();
        if let Ok(hero) = self.world.get::<&crate::hero::Hero>(unit) {
            if hero.casting_skill > 0 {
                casting.innate = hero.spells.iter().filter_map(|s| spells.id_of(s)).collect();
            }
        }
        for (entity, item) in self.items().carried_entities(unit) {
            casting.spell_save += item.bonus.spell_save;
            if let Some((spell, charges)) = &item.charges {
                if let (Some(id), true) = (spells.id_of(spell), *charges > 0) {
                    casting.item_charges.push(super::ItemCharge {
                        item: entity,
                        serial: self.serial_of(entity),
                        spell: id,
                        charges: *charges,
                    });
                }
            }
        }
        if abilities.iter().any(|a| a == "summon_demon") {
            casting.demons = 3;
        }
        for ability in abilities {
            let spell = match ability.as_str() {
                "casts_doom_bolt" => "doom_bolt",
                "casts_fireball" => "fireball",
                "casts_healing" => "healing",
                "casts_web" => "web",
                _ => continue,
            };
            casting.abilities.extend(spells.id_of(spell));
        }
        casting
    }

    pub(super) fn battle_unit_from(
        &self,
        unit: hecs::Entity,
        side: Side,
        index: usize,
    ) -> Option<BattleUnit> {
        let def = self.units().unit_def(unit)?;
        let hits = self.units().unit_stat(unit, Stat::MaxHp).max(1);
        let figures = self.units().unit_stat(unit, Stat::Figures).max(1) as u32;
        let total = hits * figures as i32;
        let hurt = self
            .world
            .get::<&Hurt>(unit)
            .map_or(0, |h| h.0 as i32)
            .min(total - 1);
        let current = total - hurt;
        let alive = ((current + hits - 1) / hits) as u32;
        let abilities = self.units().unit_abilities(unit);
        let flies = def.movement == MovementType::Flying || abilities.iter().any(|a| a == "flying");
        let has = |a: &str| abilities.iter().any(|x| x == a);
        // Mithril adds one and adamantium two to the attacks and the defence.
        let quality = self.units().unit_weapon_quality(unit);
        let extra = i32::from(quality.max(1)) - 1;
        let ranged_kind = ranged_kind_of(&abilities);
        let physical_shot = ranged_kind == RangedKind::Missile || has("thrown");
        let melee = self.units().unit_stat(unit, Stat::Melee);
        let ranged = self.units().unit_stat(unit, Stat::Ranged);
        // A spell caster has mana (20, or 40 for a major one), and shoots its spells with it.
        // A hero's mana comes from its casting skill and level, and its items.
        let hero_mana = self.units().hero_mana(unit);
        let mana = if hero_mana > 0 {
            hero_mana
        } else if has("spellcaster_major") {
            40
        } else if has("spellcaster") {
            20
        } else {
            0
        };
        let lucky = i32::from(has("lucky"));
        let hero = self.world.get::<&crate::hero::Hero>(unit).ok();
        let level = self.units().unit_level(unit);
        let mut ammo = self.units().unit_stat(unit, Stat::Ammo);
        if ranged_kind == RangedKind::Magic && mana > 0 {
            ammo = mana / super::SHOT_MANA;
        }
        Some(BattleUnit {
            id: 0,
            side,
            source: Some(index),
            unit_id: def.id.clone(),
            name: def.name.clone(),
            cell: (0, 0),
            melee: if melee > 0 { melee + extra } else { melee },
            ranged: if ranged > 0 && physical_shot {
                ranged + extra
            } else {
                ranged
            },
            defense: self.units().unit_stat(unit, Stat::Defense)
                + if quality > 0 { extra } else { 0 },
            resistance: self.units().unit_stat(unit, Stat::Resistance) + lucky,
            to_hit: self.units().unit_stat(unit, Stat::ToHit) + lucky,
            melee_plus: self.units().unit_stat(unit, Stat::MeleeToHit),
            ranged_plus: self.units().unit_stat(unit, Stat::RangedToHit),
            to_block: lucky,
            hits,
            extra_hits: 0,
            figures_start: figures,
            figures: alive,
            front_damage: alive as i32 * hits - current,
            moves: self.units().unit_max_moves(unit),
            moves_left: self.units().unit_max_moves(unit),
            ammo,
            mana,
            hero_caster: hero_mana > 0,
            casting: self.unit_casting(unit, &abilities),
            ranged_kind,
            flies,
            sails: def.movement == MovementType::Sailing,
            swims: def.movement == MovementType::Swimming,
            first_strike: has("first_strike"),
            negate_first_strike: has("negate_first_strike"),
            long_range: has("long_range"),
            magical_weapon: self.units().unit_has_magical_weapon(unit),
            weapon_quality: quality,
            special_level: self.units().unit_stat(unit, Stat::SpecialLevel),
            leadership: hero.as_ref().map_or(0, |h| {
                crate::hero::leadership(h.abilities.leadership, level)
            }),
            prayermaster: hero.as_ref().map_or(0, |h| {
                crate::hero::prayermaster(h.abilities.prayermaster, level)
            }),
            // A unit held in Stasis cannot act: it is as if asleep.
            enchantments: {
                let mut list = self.units().unit_enchantment_ids(unit);
                if self.unit_in_stasis(unit) {
                    list.push("black_sleep".to_string());
                }
                list
            },
            realm: self
                .units()
                .unit_realm(unit)
                .as_deref()
                .and_then(Realm::from_id),
            undead: self.units().unit_counts_as_undead(unit),
            abilities: abilities.clone(),
            granted: self.units().unit_granted_abilities(unit),
            status: Status::Active,
            damage_kinds: [0; 3],
            attacked: self.unit_in_stasis(unit),
            revealed: false,
            was_invisible: false,
            combat_spells: Vec::new(),
            base: None,
            field: Default::default(),
            seized: false,
            turned_from: None,
            bound_from: None,
        })
    }

    /// Hurt an overland unit with a damage spell of `strength`, by the dice of a battle: its
    /// figures and wounds are those of the unit, and a unit that loses its last figure is
    /// destroyed. Returns whether it was.
    pub(crate) fn spell_hit_overland_unit(
        &mut self,
        unit: hecs::Entity,
        strength: i32,
        traits: &crate::rules::effects::DamageTraits,
        nature: super::immunity::AttackNature,
        rng: &mut dyn rand::RngCore,
    ) -> bool {
        let Some(mut bu) = self.battle_unit_from(unit, Side::Defender, 0) else {
            return false;
        };
        let _ = super::casting::damage_unit_with_spell(&mut bu, strength, traits, nature, rng);
        if bu.figures == 0 {
            self.remove_unit(unit, crate::hero::Fate::Slain);
            return true;
        }
        let hurt = (bu.max_hit_points() - bu.hit_points()).max(0);
        let _ = self.world.insert_one(unit, Hurt(hurt as u32));
        false
    }

    /// Begin a battle between two stacks. The defenders' square decides the terrain.
    pub fn start_battle(
        &mut self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
    ) -> Result<Vec<Event>, BattleStartError> {
        if self.battle.is_some() {
            return Err(BattleStartError::InProgress);
        }
        let side_info = |game: &Game, units: &[hecs::Entity]| -> Option<(PlayerId, Position)> {
            let first = *units.first()?;
            let pos = game.units().unit_position(first)?;
            let owner = game.units().unit_owner(first)?;
            units
                .iter()
                .all(|&u| {
                    game.units().unit_position(u) == Some(pos)
                        && game.units().unit_owner(u) == Some(owner)
                })
                .then_some((owner, pos))
        };
        let (a_owner, _) = side_info(self, attackers).ok_or(BattleStartError::BadSides)?;
        let (d_owner, site) = side_info(self, defenders).ok_or(BattleStartError::BadSides)?;
        if a_owner == d_owner {
            return Err(BattleStartError::BadSides);
        }
        let mut units = Vec::new();
        let mut sources = Vec::new();
        // A fight at a city is fought round its walls, if it has them.
        let city = self.economy().city_at(site);
        let walled = city.is_some_and(|c| self.economy().city_has_building(c, "city_walls"));
        let fortress = city.is_some_and(|c| self.economy().is_fortress_city(c));
        // A fight on the sea: only ships, fliers and swimmers take part, unless the side has a
        // Floating Island or Wind Walking, which carry the rest (`Undeployable_Battle_Units_On_Water`).
        let ocean = city.is_none()
            && self.economy().terrain_class_at(site)
                == Some(crate::map::terrain::TerrainClass::Ocean);
        // Ships fight as decks on the sea, and a Floating Island is a ship here: the land units
        // aboard stand on its deck (tasks 233 and 352).
        let decks = if ocean {
            self.ship_deck_plan(attackers, defenders)
        } else {
            ShipDeckPlan::default()
        };
        let mut left_out = Vec::new();
        for (side, group) in [(Side::Attacker, attackers), (Side::Defender, defenders)] {
            let ship_decks = decks.of(side);
            let carried = ocean
                && group
                    .iter()
                    .any(|&u| self.units().unit_has_ability(u, "wind_walking"));
            let fighters: Vec<hecs::Entity> = group
                .iter()
                .copied()
                .filter(|&u| {
                    let on_deck = self
                        .economy()
                        .carrier_of(u)
                        .is_some_and(|c| ship_decks.iter().any(|(ship, _)| *ship == c));
                    let keep = !ocean || carried || on_deck || self.can_fight_at_sea(u);
                    if !keep {
                        left_out.push((side, u));
                    }
                    keep
                })
                .collect();
            let cells = if city.is_some() {
                city_deployment(side == Side::Attacker, fighters.len(), walled, fortress)
            } else {
                deployment(side == Side::Attacker, fighters.len())
            };
            let mut bus = Vec::new();
            for (n, &unit) in fighters.iter().enumerate() {
                let mut bu = self
                    .battle_unit_from(unit, side, sources.len() + n)
                    .ok_or(BattleStartError::BadSides)?;
                // Wind Walking carries the whole stack through the air.
                bu.flies |= carried;
                bus.push(bu);
            }
            // Melee units take the front squares, shooters the ones behind them.
            let order = deployment_order(&bus);
            let mut placed = vec![false; bus.len()];
            let mut taken: Vec<Pos> = Vec::new();
            // A ship is its deck: the ship stays at its bow and does not sail, and the land
            // units it carries stand on the rest, the middle first.
            for (ship, cells) in ship_decks {
                let Some(si) = fighters.iter().position(|f| f == ship) else {
                    continue;
                };
                bus[si].cell = cells[0];
                bus[si].moves = 0;
                bus[si].moves_left = 0;
                placed[si] = true;
                taken.push(cells[0]);
                let centre = (
                    cells.iter().map(|c| c.0).sum::<i32>() / cells.len() as i32,
                    cells.iter().map(|c| c.1).sum::<i32>() / cells.len() as i32,
                );
                let mut rest: Vec<Pos> = cells[1..].to_vec();
                rest.sort_by_key(|c| (c.0 - centre.0).abs() + (c.1 - centre.1).abs());
                let mut rest = rest.into_iter();
                for (i, f) in fighters.iter().enumerate() {
                    if self.economy().carrier_of(*f) == Some(*ship) {
                        if let Some(cell) = rest.next() {
                            bus[i].cell = cell;
                            placed[i] = true;
                            taken.push(cell);
                        }
                    }
                }
            }
            let mut free = cells
                .into_iter()
                .filter(|c| !taken.contains(c) && !decks.covers(*c));
            for &i in &order {
                if !placed[i] {
                    if let Some(cell) = free.next() {
                        bus[i].cell = cell;
                        placed[i] = true;
                    }
                }
            }
            for ((&unit, bu), placed) in fighters.iter().zip(bus).zip(placed) {
                if placed {
                    sources.push(unit);
                    units.push(bu);
                }
            }
        }
        let seed = self.next_u64();
        // The field is built from the square alone, as the original seeds it from the square's
        // place, so a fight on the same square is fought on the same ground.
        let mut map = self.battle_field(site, city, walled, &decks.cells());
        let has = |name: &str| city.is_some_and(|c| self.economy().city_has_enchantment(c, name));
        if city.is_some() {
            map = map.with_city(walled);
            if has("flying_fortress") {
                map = map.with_flying_fortress();
            }
        }
        let mut battle = Battle::new(a_owner, d_owner, map, units, seed);
        battle.site = Some(site);
        (battle.node_aura, battle.node_site) = self.node_realms_at(site, city.is_none());
        battle.refresh_fields();
        battle.wall_of_fire = has("wall_of_fire");
        battle.wall_of_darkness = has("wall_of_darkness");
        if let Some(c) = city {
            battle.warded_realms = self.economy().city_wards(c);
        }
        // A Cloud of Shadow treats every fight at the city as if its owner had cast Darkness.
        if let Some(cloud) = self.content.rules.spells.id_of("cloud_of_shadow") {
            if has("cloud_of_shadow") {
                battle.raise_field(Side::Defender, Field::Darkness, cloud);
            }
        }
        // Heavenly Light gives the defender True Light in every fight at the city.
        if let Some(light) = self.content.rules.spells.id_of("heavenly_light") {
            if has("heavenly_light") {
                battle.raise_field(Side::Defender, Field::TrueLight, light);
            }
        }
        // Eternal Night covers every fight in Darkness, except at a city with Heavenly Light.
        if let Some(night) = self.content.rules.spells.id_of("eternal_night") {
            if self.any_global("eternal_night") && !has("heavenly_light") {
                battle.raise_field(Side::Defender, Field::Darkness, night);
            }
        }
        self.battle_casters(&mut battle, site);
        self.battle = Some(ActiveBattle {
            battle,
            sources,
            lair: None,
            left_out,
        });
        self.attack_relations(a_owner, d_owner);
        Ok(vec![Event::BattleStarted {
            attacker: a_owner,
            defender: d_owner,
            site,
        }])
    }

    /// The decks of the ships of both sides in a sea battle: each ship with a deck square set that
    /// fits, largest first; a ship that does not fit sails as a unit, and its passengers sit the
    /// fight out. Nothing is laid out unless the first ship of each side can face the other.
    fn ship_deck_plan(
        &self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
    ) -> ShipDeckPlan {
        let ships = |group: &[hecs::Entity]| -> Vec<(hecs::Entity, (i32, i32))> {
            let mut out: Vec<_> = group
                .iter()
                .copied()
                .filter(|&u| self.economy().is_ship(u))
                .map(|u| {
                    let (w, l) = self
                        .units()
                        .unit_def(u)
                        .and_then(|d| d.deck)
                        .unwrap_or((3, 7));
                    (u, (i32::from(w), i32::from(l)))
                })
                .collect();
            out.sort_by_key(|&(_, (w, l))| std::cmp::Reverse(w * l));
            out
        };
        let (a, d) = (ships(attackers), ships(defenders));
        let sizes = |v: &[(hecs::Entity, (i32, i32))]| v.iter().map(|s| s.1).collect::<Vec<_>>();
        let laid = layout::ship_decks(&sizes(&a), &sizes(&d));
        let pick = |v: &[(hecs::Entity, (i32, i32))], cells: Vec<Option<Vec<Pos>>>| {
            v.iter()
                .zip(cells)
                .filter_map(|(s, c)| c.map(|c| (s.0, c)))
                .collect::<Vec<_>>()
        };
        ShipDeckPlan {
            attackers: pick(&a, laid.attackers),
            defenders: pick(&d, laid.defenders),
        }
    }

    /// Whether a unit may fight on the sea: a ship (it carries others or sails), a flier or a
    /// swimmer.
    fn can_fight_at_sea(&self, unit: hecs::Entity) -> bool {
        let abilities = self.units().unit_abilities(unit);
        self.units().unit_def(unit).is_some_and(|d| {
            matches!(
                d.movement,
                MovementType::Flying | MovementType::Sailing | MovementType::Swimming
            )
        }) || abilities.iter().any(|a| a == "flying")
            || self.units().unit_stat(unit, Stat::Transport) > 0
    }

    /// The realms of the node a fight on `site` is under: the one whose aura covers the square,
    /// and (when `open_ground`, no city) the one that stands on it (`Combat_Node_Type` and the
    /// centre structure of `Build_Battlefield`).
    fn node_realms_at(&self, site: Position, open_ground: bool) -> (Option<Realm>, Option<Realm>) {
        let aura = self
            .economy()
            .node_covering(site)
            .and_then(|n| Realm::from_id(&n.realm));
        let on = open_ground
            .then(|| self.node_at(site))
            .flatten()
            .and_then(|n| Realm::from_id(&n.realm));
        (aura, on)
    }

    /// The ground of a fight on `site`: the original's field for the square's terrain, roads,
    /// city and walls (`Build_Battlefield`).
    fn battle_field(
        &self,
        site: Position,
        city: Option<hecs::Entity>,
        walled: bool,
        ship_cells: &[Pos],
    ) -> BattleMap {
        use super::map::layout::{Site, Spec};
        let economy = self.economy();
        let mut spec = Spec::open(super::map::set_of(economy.terrain_class_at(site)));
        spec.ship_decks = ship_cells.iter().map(|&(x, y)| (20 - x, y)).collect();
        if let Some(c) = city {
            let pop = self.city_population(c);
            spec.site = if pop.figures == 0 {
                Site::Outpost
            } else {
                Site::City
            };
            spec.population = pop.figures;
            spec.walled = walled && spec.site == Site::City;
            // The wizard's capital has the fortress on its structure square.
            if spec.site == Site::City && self.economy().is_fortress_city(c) {
                spec.structure = layout::Structure::Fortress;
            }
            spec.house_type_from(
                self.city_race(c)
                    .map_or(0, |r| self.content.rules.races.get(r).house_type),
            );
        }
        // A tower, node or encounter site on the square puts its building in the middle of the
        // field (`Build_Battlefield`), which nobody can enter.
        // (A node's own square is a forest, mountain or plains by its terrain: `set_of`.)
        if city.is_none() && spec.set != layout::Set::Water {
            let feature = self.economy().feature_at(site);
            if let Some(structure) = feature.and_then(|d| layout::Structure::of_feature(&d.id)) {
                spec.site = Site::Outpost;
                spec.structure = structure;
            }
        }
        if let Some(map) = self.map() {
            let grid = map.plane(site.plane);
            for (i, (dx, dy)) in [
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (0, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ]
            .into_iter()
            .enumerate()
            {
                let (x, y) = (site.x as i32 + dx, site.y as i32 + dy);
                spec.roads[i] = y >= 0 && grid.has_road(x, y);
                spec.rivers[i] = y >= 0 && grid.has_river(x, y);
            }
            spec.enchanted_roads = grid.has_enchanted_road(site.x as i32, site.y as i32);
        }
        let place = i64::from(site.x) * i64::from(site.y) * (site.plane.index() as i64 + 5) + 10039;
        let mut rng = <rand_pcg::Pcg64Mcg as rand::SeedableRng>::seed_from_u64(place as u64);
        BattleMap::from_layout(layout::generate(&spec, &mut rng))
    }

    /// Carry out a battle command; when it ends the battle, apply the results.
    pub fn battle_command(&mut self, command: BattleCommand) -> Result<Vec<Event>, BattleError> {
        if let BattleCommand::Cast { spell, aim, mana } = command {
            return self.battle_cast(spell, aim, mana);
        }
        if let BattleCommand::SummonDemon { unit } = command {
            return self.battle_summon_demon(unit);
        }
        if let BattleCommand::UnitCast {
            unit,
            spell,
            aim,
            mana,
        } = command
        {
            return self.battle_unit_cast(unit, spell, aim, mana);
        }
        let active = self.battle.as_mut().ok_or(BattleError::Over)?;
        let events = active.battle.apply(command)?;
        // Mana Leak and the like change the wizards' reserves.
        self.sync_battle_mana();
        let mut out: Vec<Event> = events.iter().cloned().map(Event::Battle).collect();
        self.finish_if_over(&mut out);
        Ok(out)
    }

    /// A battle whose winner is settled is written back, whatever settled it. (A cast, an end of
    /// turn effect or the start of the battle can settle it without the command's own events
    /// saying so, and a battle left open this way blocks every later fight.)
    pub(crate) fn finish_if_over(&mut self, out: &mut Vec<Event>) {
        if self
            .battle
            .as_ref()
            .is_some_and(|a| a.battle.winner.is_some())
        {
            out.extend(self.finish_battle());
        }
    }

    /// Write a finished battle back: wounds, deaths, and who won.
    /// A wizard whose units fell or fled holds it against the attacker (`End_Of_Combat`): a
    /// hero counts 20, another unit up to 20, and a settler up to 10 more. The units of the
    /// field that were summoned in the fight do not count.
    fn battle_loss_relations(&mut self, battle: &Battle, sources: &[hecs::Entity]) {
        let (attacker, defender) = (
            battle.player_of(Side::Attacker),
            battle.player_of(Side::Defender),
        );
        let lost: Vec<(bool, bool)> = battle
            .units
            .iter()
            .filter(|u| u.side == Side::Defender && u.source.is_some())
            .filter(|u| matches!(u.status, Status::Dead | Status::Fled))
            .map(|u| {
                let hero = u
                    .source
                    .and_then(|i| sources.get(i).copied())
                    .and_then(|e| self.units().unit_def(e))
                    .is_some_and(|d| d.kind == crate::rules::UnitKind::Hero);
                (hero, u.has("create_outpost"))
            })
            .collect();
        let mut value = 0;
        for (hero, settler) in lost {
            value += if hero { 20 } else { self.roll(20) as i32 };
            if settler {
                value += self.roll(10) as i32;
            }
        }
        self.change_relations(
            -value,
            attacker,
            defender,
            crate::diplomacy::Action::BattleLosses,
            None,
        );
    }

    pub(crate) fn finish_battle(&mut self) -> Vec<Event> {
        let Some(active) = self.battle.take() else {
            return Vec::new();
        };
        let ActiveBattle {
            battle,
            sources,
            lair,
            left_out,
        } = active;
        // The units that sat the fight out go down with a side that lost it.
        if let Some(winner) = battle.winner {
            for (side, unit) in left_out {
                if side != winner {
                    self.remove_unit_to(
                        unit,
                        crate::hero::Fate::Slain,
                        Some(battle.player_of(winner)),
                    );
                }
            }
        }
        let mut events = Vec::new();
        let spoils = self.battle_spoils(&battle, &sources);
        self.battle_loss_relations(&battle, &sources);
        let survivors: Vec<hecs::Entity> = battle
            .units
            .iter()
            .filter(|u| {
                Some(u.side) == battle.winner && (u.status == Status::Active || revived(&battle, u))
            })
            .filter_map(|u| u.source.and_then(|i| sources.get(i).copied()))
            .collect();
        // Units drained to death on the losing side join the winner as undead, in a fight at a
        // lair as on the map.
        let mut drained = Vec::new();
        for u in &battle.units {
            let Some(entity) = u.source.and_then(|i| sources.get(i).copied()) else {
                continue;
            };
            // The charges an item's spell has left stay on the item.
            if !u.casting.item_charges.is_empty() {
                for c in &u.casting.item_charges {
                    if let Ok(mut item) = self.world.get::<&mut crate::item::Item>(c.item) {
                        if let Some((_, charges)) = item.charges.as_mut() {
                            *charges = c.charges;
                        }
                    }
                }
            }
            // Regeneration heals the unit completely after the fight, and brings back one that
            // was slain (not drained or destroyed) if its side won.
            let regenerates = u.regenerates();
            let revived = revived(&battle, u);
            if regenerates && (revived || u.status != Status::Dead) {
                let _ = self.world.insert_one(entity, Hurt(0));
            } else if u.status == Status::Dead {
                if self.is_raised_by(&battle, u, entity) {
                    drained.push(entity);
                } else {
                    // Disintegrate and the other spells that destroy leave a hero gone for good,
                    // with its items; a slain one's items go to the winner.
                    let fate = if u.fate() == super::Fate::Gone {
                        crate::hero::Fate::Gone
                    } else {
                        crate::hero::Fate::Slain
                    };
                    let heir = battle.winner.map(|side| battle.player_of(side));
                    self.remove_unit_to(entity, fate, heir);
                }
            } else {
                let mut hurt = (u.max_hit_points() - u.hit_points()).max(0);
                // Hit points a lasting spell added (Lionheart) are gone with it: the unit
                // is hurt by at most what it can lose and live.
                if let Some(base) = &u.base {
                    hurt = hurt.min(base.hits * u.figures_start as i32 - 1);
                }
                let _ = self.world.insert_one(entity, Hurt(hurt as u32));
            }
        }
        // Passengers of a ship that went down in the fight scramble ashore or drown (task 235).
        events.extend(self.drown_stranded());
        if let Some(side) = battle.winner {
            events.push(Event::BattleEnded {
                winner: battle.player_of(side),
                loser: battle.player_of(side.other()),
            });
        }
        events.extend(self.give_spoils(&battle, &spoils, &survivors));
        if lair.is_none() {
            // A city that held against a wizard's attack costs the attacker its owner's goodwill.
            if let (Some(Side::Defender), Some(pos)) = (battle.winner, battle.site) {
                if let Some(city) = self.economy().city_at(pos) {
                    let (attacker, defender) = (
                        battle.player_of(Side::Attacker),
                        battle.player_of(Side::Defender),
                    );
                    if self.economy().citizens(city) > 0
                        && self.economy().city_owner(city) == Some(defender)
                    {
                        let fortress = self.economy().is_fortress_city(city);
                        self.city_held_relations(attacker, defender, fortress);
                    }
                }
            }
            events.extend(self.overland_aftermath(&battle, &sources));
            events.extend(self.raise_drained(&battle, drained));
        } else {
            // They stand on the site when the winner's survivors move in.
            events.extend(self.raise_drained(&battle, drained));
        }
        if let Some(index) = lair {
            let attackers: Vec<hecs::Entity> = battle
                .units
                .iter()
                .filter(|u| u.side == Side::Attacker)
                .filter_map(|u| u.source.and_then(|i| sources.get(i).copied()))
                .collect();
            let winner = battle
                .winner
                .filter(|&s| s == Side::Attacker)
                .map(|s| battle.player_of(s));
            let guards: Vec<hecs::Entity> = battle
                .units
                .iter()
                .filter(|u| u.side == Side::Defender)
                .filter_map(|u| u.source.and_then(|i| sources.get(i).copied()))
                .collect();
            events.extend(self.finish_site_battle(index, winner, &attackers, &guards));
        }
        events
    }

    /// Whether the winner takes this dead unit as an undead one: the loser's, drained to death
    /// (life steal, a creator of undead), not a creature of the Death realm and not a hero; or,
    /// with the winner's Zombie Mastery, any normal unit that fell on either side but was not
    /// destroyed.
    fn is_raised_by(&self, battle: &Battle, u: &BattleUnit, entity: hecs::Entity) -> bool {
        use crate::rules::UnitKind;
        let Some(winner) = battle.winner else {
            return false;
        };
        let kind = self.units().unit_def(entity).map(|d| d.kind);
        let zombies = u.fate() != super::Fate::Gone
            && kind == Some(UnitKind::Normal)
            && self.has_global(battle.player_of(winner), "zombie_mastery");
        zombies
            || (winner != u.side
                && u.fate() == super::Fate::Drained
                && u.realm != Some(Realm::Death)
                && kind.is_some_and(|k| k != UnitKind::Hero))
    }

    /// Put the drained dead on the winner's square as undead units, as many as it has room for.
    fn raise_drained(&mut self, battle: &Battle, mut dead: Vec<hecs::Entity>) -> Vec<Event> {
        let winner = battle.winner.map(|side| (side, battle.player_of(side)));
        let heir = winner.map(|(_, player)| player);
        let (Some((side, player)), Some(pos)) = (winner, battle.site) else {
            for u in dead {
                self.remove_unit_to(u, crate::hero::Fate::Slain, heir);
            }
            return Vec::new();
        };
        self.sort_by_serial(&mut dead);
        // As in the original, the winner's units that stand count against the stack's nine.
        let mut room = crate::unit::MAX_STACK.saturating_sub(battle.active_units(side).count());
        let mut raised = 0;
        for unit in dead {
            if room == 0 {
                self.remove_unit_to(unit, crate::hero::Fate::Slain, heir);
                continue;
            }
            room -= 1;
            self.make_undead(unit);
            let _ = self.world.remove_one::<Hurt>(unit);
            let _ = self.world.remove_one::<crate::unit::Order>(unit);
            self.set_owner(unit, player);
            let _ = self.world.insert(unit, (pos, crate::unit::MovesLeft(0)));
            raised += 1;
        }
        if raised > 0 {
            vec![Event::UndeadRaised {
                player,
                units: raised,
            }]
        } else {
            Vec::new()
        }
    }

    /// What a battle on the overland map leaves behind: units that fled lose some of their
    /// number, and an attacker who won and cleared the square moves in and takes a city.
    fn overland_aftermath(&mut self, battle: &Battle, sources: &[hecs::Entity]) -> Vec<Event> {
        use crate::rules::UnitKind;
        let mut events = Vec::new();
        let source_of = |u: &BattleUnit| u.source.and_then(|i| sources.get(i).copied());

        // Fleeing costs half the units and a quarter of the heroes (task 42). Units that are
        // asleep, confused or webbed are always lost, and on Intro and Easy the player whose
        // turn it is (the attacker: a battle starts on its mover's turn) loses nothing else
        // (`Combat.c`, task 250).
        const HELD: [&str; 3] = ["black_sleep", "confusion", "web"];
        // Heroes taken off the field by Recall Hero are at the summoning circle now.
        for u in battle.units.iter().filter(|u| u.status == Status::Recalled) {
            let to = self.summon_point(battle.player_of(u.side));
            if let (Some(unit), Some(to)) = (source_of(u), to) {
                let _ = self.world.insert_one(unit, to);
            }
        }
        for side in [Side::Attacker, Side::Defender] {
            let mut fled: Vec<(hecs::Entity, bool)> = battle
                .units
                .iter()
                .filter(|u| u.side == side && u.status == Status::Fled)
                .filter_map(|u| {
                    let held = u.enchantments.iter().any(|e| HELD.contains(&e.as_str()));
                    source_of(u).map(|e| (e, held))
                })
                .collect();
            if fled.is_empty() {
                continue;
            }
            fled.sort_by_key(|&(e, _)| (self.serial_of(e), e.id()));
            let human = battle.player_of(side) == self.human().unwrap_or(PlayerId(u8::MAX));
            let safe = side == Side::Attacker && self.settings.rules.safe_flee.of(human);
            let mut lost = 0;
            for (unit, held) in fled {
                let percent = match self.units().unit_def(unit).map(|d| d.kind) {
                    Some(UnitKind::Hero) => 25,
                    _ => 50,
                };
                if held || (!safe && self.roll(100) <= percent) {
                    let heir = battle.winner.map(|w| battle.player_of(w));
                    self.remove_unit_to(unit, crate::hero::Fate::Slain, heir);
                    lost += 1;
                }
            }
            if lost > 0 {
                events.push(Event::UnitsLostFleeing {
                    player: battle.player_of(side),
                    units: lost,
                });
            }
        }

        // The winner takes the square if no enemy is left on it.
        let (Some(Side::Attacker), Some(pos)) = (battle.winner, battle.site) else {
            return events;
        };
        let attacker = battle.player_of(Side::Attacker);
        let hostile_left = self
            .units()
            .units_at(pos)
            .into_iter()
            .any(|u| self.units().unit_owner(u).is_some_and(|o| o != attacker));
        if hostile_left {
            return events;
        }
        let movers: Vec<hecs::Entity> = battle
            .units
            .iter()
            .filter(|u| {
                u.side == Side::Attacker && (u.status == Status::Active || revived(battle, u))
            })
            .filter_map(source_of)
            .filter(|&u| self.units().unit_position(u).is_some())
            .collect();
        // Monsters that have beaten a city's defenders wreck it and stay outside.
        if attacker == crate::player::MONSTERS {
            if let Some(city) = self.economy().city_at(pos) {
                if self
                    .economy()
                    .city_owner(city)
                    .is_some_and(|o| o != attacker)
                {
                    events.extend(self.rampage_city(city, battle.city_damage, &movers));
                    return events;
                }
            }
        }
        if let Some(&first) = movers.first() {
            let from = self.units().unit_position(first).unwrap_or(pos);
            for &u in &movers {
                let _ = self.world.insert_one(u, pos);
            }
            self.explore(attacker, pos, crate::map::fog::sight_for(2));
            events.push(Event::UnitsMoved {
                units: movers,
                from,
                to: pos,
            });
        }
        if let Some(city) = self.economy().city_at(pos) {
            if self
                .economy()
                .city_owner(city)
                .is_some_and(|o| o != attacker)
            {
                events.extend(self.conquer_city(city, attacker, battle.city_damage));
            }
        }
        events
    }

    /// Fight a battle to the end with the battle AI playing both sides (the original's
    /// strategic combat, and what an AI player uses against another player's stack).
    pub fn auto_resolve(
        &mut self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
    ) -> Vec<Event> {
        self.auto_resolve_with(attackers, defenders, None)
    }

    /// [`Game::auto_resolve`] with a limit on the mana one wizard may spend from the reserve
    /// (strategic combat, `Command::AutoResolve::mana_budget`).
    pub fn auto_resolve_with(
        &mut self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
        mana_budget: Option<(PlayerId, u32)>,
    ) -> Vec<Event> {
        let near = |game: &Game| -> Option<bool> {
            let a = game.units().unit_position(*attackers.first()?)?;
            let d = game.units().unit_position(*defenders.first()?)?;
            let map = game.map.as_ref()?;
            Some(a.plane == d.plane && map.plane(a.plane).chebyshev((a.x, a.y), (d.x, d.y)) <= 1)
        };
        if near(self) == Some(false) {
            return vec![Event::Rejected(crate::game::Rejection::BattleStart(
                BattleStartError::NotAdjacent,
            ))];
        }
        let mut events = match self.start_battle(attackers, defenders) {
            Ok(e) => e,
            Err(e) => return vec![Event::Rejected(crate::game::Rejection::BattleStart(e))],
        };
        if let Some((player, mana)) = mana_budget {
            let side = self.battle().and_then(|b| b.side_of(player));
            if let Some(side) = side {
                self.set_battle_mana_budget(side, Some(mana));
            }
        }
        for _ in 0..20_000 {
            let Some(command) = self.next_battle_command() else {
                break;
            };
            match self.battle_command(command) {
                Ok(e) => events.extend(e),
                Err(_) => break,
            }
        }
        // A battle that somehow stalls ends with the active side fleeing.
        while self.battle.is_some() {
            match self.battle_command(BattleCommand::Flee) {
                Ok(e) => events.extend(e),
                Err(_) => break,
            }
        }
        self.finish_if_over(&mut events);
        events
    }
}

/// A unit that regenerates and was slain comes back to life if its side won (a unit drained or
/// destroyed does not).
fn revived(battle: &Battle, u: &BattleUnit) -> bool {
    u.status == Status::Dead
        && u.regenerates()
        && battle.winner == Some(u.side)
        && u.fate() == super::Fate::Slain
}

/// The order a side's units take their squares in, front first (`Deploy_Battle_Units`): the
/// units without a ranged attack, then the shooters. With more than five shooters the strongest
/// in melee move forward, until the front has four units or four shooters are left.
pub(super) fn deployment_order(units: &[BattleUnit]) -> Vec<usize> {
    let shoots = |u: &BattleUnit| u.ranged > 0 && u.ammo > 0;
    let mut melee: Vec<usize> = (0..units.len()).filter(|&i| !shoots(&units[i])).collect();
    let mut ranged: Vec<usize> = (0..units.len()).filter(|&i| shoots(&units[i])).collect();
    if ranged.len() > 5 {
        let to_move = ranged.len() - 4;
        for _ in 0..to_move {
            if melee.len() >= 4 {
                break;
            }
            // The original looks the strongest up in the wrong list; this takes it from the
            // shooters, as meant. The first of equals goes.
            let best = (0..ranged.len())
                .max_by_key(|&k| (units[ranged[k]].melee, std::cmp::Reverse(k)))
                .unwrap_or(0);
            melee.push(ranged.remove(best));
        }
    }
    melee.extend(ranged);
    melee
}
