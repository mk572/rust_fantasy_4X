//! Joining battles to the game: building one from overland units, running it through
//! commands, and writing its results back.

use crate::city::{PlayerId, Position};
use crate::rules::{MovementType, Stat};
use crate::units::Hurt;
use crate::{Event, Game};

use super::field::Field;
use super::immunity::Realm;
use super::map::{city_deployment, deployment, BattleMap};
use super::{
    Battle, BattleCommand, BattleError, BattleEvent, BattleUnit, RangedKind, Side, Status,
};

/// A battle in progress and the overland units its fighters were copied from.
pub struct ActiveBattle {
    pub battle: Battle,
    /// `BattleUnit::source` indexes this list.
    pub sources: Vec<hecs::Entity>,
    /// The lair being assaulted, if this is a fight against a site's guards.
    pub site: Option<usize>,
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
        if let Some(k) = self.battle_brains.iter().position(|(p, _)| *p == player) {
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
        if let Ok(items) = self.world.get::<&crate::items::HeroItems>(unit) {
            for (slot, item) in items.0.iter().enumerate() {
                let Some(item) = item else { continue };
                casting.spell_save += item.bonus.spell_save;
                if let Some((spell, charges)) = &item.charges {
                    if let (Some(id), true) = (spells.id_of(spell), *charges > 0) {
                        casting.item_charges.push((slot, id, *charges));
                    }
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
            enchantments: self.units().unit_enchantment_ids(unit),
            realm: self
                .units()
                .unit_realm(unit)
                .as_deref()
                .and_then(Realm::from_id),
            undead: self.units().unit_counts_as_undead(unit),
            abilities: abilities.clone(),
            status: Status::Active,
            damage_kinds: [0; 3],
            attacked: false,
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
            self.remove_unit(unit, crate::roster::Fate::Slain);
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
        for (side, group) in [(Side::Attacker, attackers), (Side::Defender, defenders)] {
            let cells = if city.is_some() {
                city_deployment(side == Side::Attacker, group.len(), walled)
            } else {
                deployment(side == Side::Attacker, group.len())
            };
            for (&unit, cell) in group.iter().zip(cells) {
                let index = sources.len();
                let mut bu = self
                    .battle_unit_from(unit, side, index)
                    .ok_or(BattleStartError::BadSides)?;
                bu.cell = cell;
                sources.push(unit);
                units.push(bu);
            }
        }
        let seed = self.next_u64();
        // The field is built from the square alone, as the original seeds it from the square's
        // place, so a fight on the same square is fought on the same ground.
        let mut map = self.battle_field(site, city, walled);
        let has = |name: &str| city.is_some_and(|c| self.economy().city_has_enchantment(c, name));
        if city.is_some() {
            map = map.with_city(walled);
            if has("flying_fortress") {
                map = map.with_flying_fortress();
            }
        }
        let mut battle = Battle::new(a_owner, d_owner, map, units, seed);
        battle.site = Some(site);
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
        self.battle_casters(&mut battle, site);
        self.battle = Some(ActiveBattle {
            battle,
            sources,
            site: None,
        });
        self.attack_relations(a_owner, d_owner);
        Ok(vec![Event::BattleStarted {
            attacker: a_owner,
            defender: d_owner,
            site,
        }])
    }

    /// The ground of a fight on `site`: the original's field for the square's terrain, roads,
    /// city and walls (`Build_Battlefield`).
    fn battle_field(&self, site: Position, city: Option<hecs::Entity>, walled: bool) -> BattleMap {
        use super::map::layout::{self, Site, Spec};
        let economy = self.economy();
        let mut spec = Spec::open(super::map::set_of(economy.terrain_class_at(site)));
        if let Some(c) = city {
            let pop = self.city_population(c);
            spec.site = if pop.figures == 0 {
                Site::Outpost
            } else {
                Site::City
            };
            spec.population = pop.figures;
            spec.walled = walled && spec.site == Site::City;
            spec.house_type_from(
                self.city_race(c)
                    .map_or(0, |r| self.content.rules.races.get(r).house_type),
            );
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
        if events
            .iter()
            .any(|e| matches!(e, BattleEvent::Ended { .. }))
        {
            out.extend(self.finish_battle());
        }
        Ok(out)
    }

    /// Write a finished battle back: wounds, deaths, and who won.
    pub(super) fn finish_battle(&mut self) -> Vec<Event> {
        let Some(active) = self.battle.take() else {
            return Vec::new();
        };
        let ActiveBattle {
            battle,
            sources,
            site,
        } = active;
        let mut events = Vec::new();
        let spoils = self.battle_spoils(&battle, &sources);
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
                if let Ok(mut items) = self.world.get::<&mut crate::items::HeroItems>(entity) {
                    for &(slot, _, left) in &u.casting.item_charges {
                        if let Some((_, charges)) =
                            items.0[slot].as_mut().and_then(|i| i.charges.as_mut())
                        {
                            *charges = left;
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
                    self.remove_unit(entity, crate::roster::Fate::Slain);
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
        if let Some(side) = battle.winner {
            events.push(Event::BattleEnded {
                winner: battle.player_of(side),
                loser: battle.player_of(side.other()),
            });
        }
        events.extend(self.give_spoils(&battle, &spoils, &survivors));
        if site.is_none() {
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
        if let Some(index) = site {
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
    /// (life steal, a creator of undead), not a creature of the Death realm and not a hero.
    fn is_raised_by(&self, battle: &Battle, u: &BattleUnit, entity: hecs::Entity) -> bool {
        battle.winner.is_some_and(|w| w != u.side)
            && u.fate() == super::Fate::Drained
            && u.realm != Some(Realm::Death)
            && self
                .units()
                .unit_def(entity)
                .is_some_and(|d| d.kind != crate::rules::UnitKind::Hero)
    }

    /// Put the drained dead on the winner's square as undead units, as many as it has room for.
    fn raise_drained(&mut self, battle: &Battle, mut dead: Vec<hecs::Entity>) -> Vec<Event> {
        let winner = battle.winner.map(|side| (side, battle.player_of(side)));
        let (Some((side, player)), Some(pos)) = (winner, battle.site) else {
            for u in dead {
                self.remove_unit(u, crate::roster::Fate::Slain);
            }
            return Vec::new();
        };
        self.sort_by_serial(&mut dead);
        // As in the original, the winner's units that stand count against the stack's nine.
        let mut room = crate::units::MAX_STACK.saturating_sub(battle.active_units(side).count());
        let mut raised = 0;
        for unit in dead {
            if room == 0 {
                self.remove_unit(unit, crate::roster::Fate::Slain);
                continue;
            }
            room -= 1;
            self.make_undead(unit);
            let _ = self.world.remove_one::<Hurt>(unit);
            let _ = self.world.remove_one::<crate::units::Order>(unit);
            let _ = self.world.insert(
                unit,
                (crate::units::Owner(player), pos, crate::units::MovesLeft(0)),
            );
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
            let human = battle.player_of(side) == self.human.unwrap_or(PlayerId(u8::MAX));
            let safe = side == Side::Attacker && self.settings.rules.safe_flee.of(human);
            let mut lost = 0;
            for (unit, held) in fled {
                let percent = match self.units().unit_def(unit).map(|d| d.kind) {
                    Some(UnitKind::Hero) => 25,
                    _ => 50,
                };
                if held || (!safe && self.roll(100) <= percent) {
                    self.remove_unit(unit, crate::roster::Fate::Slain);
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
        if let Some(&first) = movers.first() {
            let from = self.units().unit_position(first).unwrap_or(pos);
            for &u in &movers {
                let _ = self.world.insert_one(u, pos);
            }
            self.explore(attacker, pos, 2);
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
        let near = |game: &Game| -> Option<bool> {
            let a = game.units().unit_position(*attackers.first()?)?;
            let d = game.units().unit_position(*defenders.first()?)?;
            let map = game.map.as_ref()?;
            Some(a.plane == d.plane && map.plane(a.plane).chebyshev((a.x, a.y), (d.x, d.y)) <= 1)
        };
        if near(self) == Some(false) {
            return vec![Event::Rejected(crate::Rejection::BattleStart(
                BattleStartError::NotAdjacent,
            ))];
        }
        let mut events = match self.start_battle(attackers, defenders) {
            Ok(e) => e,
            Err(e) => return vec![Event::Rejected(crate::Rejection::BattleStart(e))],
        };
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
