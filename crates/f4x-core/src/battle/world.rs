//! Joining battles to the game: building one from overland units, running it through
//! commands, and writing its results back.
//!
//! Tests: in battle/tests.rs.

use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::unit_flag::{UnitFlag, UnitFlags};
use crate::rules::Ability;
use crate::rules::{MovementType, Stat, UnitId};
use crate::unit::Hurt;
use crate::world::rng::GameRng;
use crate::world::{Event, World};
use f4x_def::half_int::HalfInt;
use rand::RngCore as _;

use super::field::Field;
use super::map::{city_deployment, deployment, layout, BattleMap, Pos};
use super::{
    Battle, BattleCommand, BattleError, BattleUnit, HeroSheet, Sheet, Side, Status, WizardSheet,
};
use f4x_battle::battle::setup::{place_side, Placement};
use f4x_def::rules::realm::Realm;

/// A battle in progress and the overland units its fighters were copied from.
pub struct ActiveBattle {
    pub battle: Battle,
    /// `BattleUnit::source` indexes this list.
    pub sources: Vec<hecs::Entity>,
    /// Units that stayed out of a fight on the sea (land units aboard a ship): they share the
    /// fate of their side.
    pub left_out: Vec<(Side, hecs::Entity)>,
    /// Defenders that stood on the square but are not among the nine who fight. They stay out
    /// of the battle; if the defence loses they flee (`World::bench_flees`), if it wins they stay.
    pub benched: Vec<hecs::Entity>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleStartError {
    /// A battle is already being fought.
    InProgress,
    /// A side is empty, or its units are not one player's on one square.
    BadSides,
    /// An auto-resolved attack must come from an adjacent square.
    NotAdjacent,
    /// The sides have a pact or an alliance: it has to be broken first, by a player.
    Treaty,
    /// No attack of those units waits for these defenders' answer.
    NoSuchAttack,
}

impl BattleStartError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::InProgress => "refuse_map.battle_start_error_in_progress",
            Self::BadSides => "refuse_map.battle_start_error_bad_sides",
            Self::NotAdjacent => "refuse_map.battle_start_error_not_adjacent",
            Self::Treaty => "refuse_map.battle_start_error_treaty",
            Self::NoSuchAttack => "refuse_map.battle_start_error_no_such_attack",
        }
    }
}

pub use f4x_battle::weapon::{caster_mana_of, ranged_kind_of, weapon_attack_of, weapon_shot_of};

/// Something that plays a wizard's side of a battle: it is asked for one command at a time.
/// The scripted player is `f4x_ai::battle`, set once with [`set_battle_ai`]; a learned one
/// (`f4x-learn`) is plugged in with [`World::set_battle_brain`]. The brain sees the whole game, as
/// a player does.
pub trait BattleBrain {
    /// The next command for the side to play in the battle in progress.
    fn command(&mut self, world: &World) -> BattleCommand;
}

/// The orders of auto-resolved battles, which a replay records and gives back
/// (`f4x-master`, `game/replay.rs`). Not part of a save.
#[derive(Default)]
pub(crate) struct AutoOrders {
    /// Orders for the next auto-resolved battle, given in place of asking for them.
    script: Option<Vec<BattleCommand>>,
    /// The orders asked for in the last one.
    given: Option<Vec<BattleCommand>>,
}

/// The scripted battle player that plays every side no brain was set for.
static BATTLE_AI: std::sync::OnceLock<fn(&World) -> Option<BattleCommand>> =
    std::sync::OnceLock::new();

/// Name the scripted battle player: `f4x_ai::battle::install` calls this once. The battle AI is
/// behaviour, so it lives in the AI crate; the game only asks it for the next command. Without
/// one (and without a brain for the side) a battle has no next command.
pub fn set_battle_ai(ai: fn(&World) -> Option<BattleCommand>) {
    let _ = BATTLE_AI.set(ai);
}

impl World {
    /// Let `brain` play the battles of `player` (an AI wizard) from now on; `None` goes back to
    /// the scripted battle AI.
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
    /// from the scripted battle AI ([`set_battle_ai`]). `None` when no battle is on or there is no AI.
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
        BATTLE_AI.get().and_then(|ai| ai(self))
    }

    /// Give the next auto-resolved battle these orders, in place of asking the brains and the
    /// battle AI; when they run out, the battle ends as it does with nobody to ask. `None` goes
    /// back to asking.
    pub fn script_battle_orders(&mut self, orders: Option<Vec<BattleCommand>>) {
        self.auto_orders.script = orders;
    }

    /// The orders asked for in the last auto-resolved battle, in order, the one the rules
    /// refused included; `None` when none was fought since they were last taken.
    pub fn take_battle_orders(&mut self) -> Option<Vec<BattleCommand>> {
        self.auto_orders.given.take()
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
        let mana = u32::try_from(self.player(player)?.treasury().mana.floor()).ok()?;
        if mana == 0 || self.battle_spells(player).is_empty() {
            return None;
        }
        let skill = self
            .wizard(player)?
            .casting_skill(&self.content.rules)
            .max(0) as u32;
        Some(skill.min(mana / 2).max(1).min(mana))
    }

    /// The battle in progress.
    pub fn battle(&self) -> Option<&Battle> {
        self.battle.as_ref().map(|b| &b.battle)
    }

    /// The overland unit battle unit `id` was copied from (`None` for a unit made in the fight,
    /// such as a summon).
    pub fn battle_unit_source(&self, id: usize) -> Option<hecs::Entity> {
        let active = self.battle.as_ref()?;
        let index = active.battle.unit(id)?.source?;
        active.sources.get(index).copied()
    }

    /// What a unit can cast in a battle on its own: a hero's innate spells, the charges of its
    /// items' spells and its spell save, and the spells its abilities cast (once a battle).
    fn unit_casting(&self, unit: hecs::Entity, abilities: &[Ability]) -> super::UnitCasting {
        let mut casting = super::UnitCasting::of_abilities(abilities, &self.content().rules);
        if let Ok(hero) = self.ecs.get::<&crate::hero::Hero>(unit) {
            if hero.casting_skill > 0 {
                casting.innate = hero.spells.clone();
            }
        }
        for (entity, item) in self.items().carried_entities(unit) {
            casting.spell_save += item.bonus.spell_save;
            if let Some((spell, charges)) = &item.charges {
                if let (id, true) = (*spell, *charges > 0) {
                    casting.item_charges.push(super::ItemCharge {
                        item: entity,
                        serial: self.units().serial_of(entity),
                        spell: id,
                        charges: *charges,
                    });
                }
            }
        }
        casting
    }

    /// Copy an overland unit into battle form: what it is, and nothing worked out from its level
    /// or its owner's global enchantments, which the battle works out itself (`Sheet`).
    /// What a battle knows of a player's wizard: its global enchantments and retorts, by id.
    pub(super) fn wizard_sheet(&self, player: PlayerId) -> WizardSheet {
        self.wizard(player)
            .map(|m| WizardSheet {
                globals: m.global(),
                retorts: m.retorts().into_iter().collect(),
            })
            .unwrap_or_default()
    }

    pub(super) fn battle_unit_from(
        &self,
        unit: hecs::Entity,
        side: Side,
        index: usize,
    ) -> Option<BattleUnit> {
        let view = self.units();
        let def = view.unit_def(unit)?;
        let rules = &self.content().rules;
        let abilities = view.unit_abilities(unit);
        let mut flags = UnitFlags::NONE;
        if def.movement == MovementType::Flying {
            flags.insert(UnitFlag::Flies);
        }
        if view.unit_has_innate_magical_weapon(unit) {
            flags.insert(UnitFlag::MagicalWeapon);
        }
        let hero = self.ecs.get::<&crate::hero::Hero>(unit).ok();
        let mut sheet = Sheet::plain(side, def.stats.clone());
        sheet.modifiers = view.own_modifiers(unit);
        sheet.granted = view.unit_granted_abilities(unit);
        sheet.enchantments = view.unit_enchantment_ids(unit);
        sheet.ranged_kind = ranged_kind_of(&abilities, rules);
        sheet.abilities = abilities;
        sheet.flags = flags;
        sheet.xp = view.unit_experience(unit);
        sheet.hero = hero.as_ref().map(|h| HeroSheet {
            abilities: h.abilities,
            casting_skill: h.casting_skill,
            item_spell_skill: view
                .items()
                .carried(unit)
                .iter()
                .map(|(_, i)| i.bonus.spell_skill)
                .sum(),
        });
        // What the unit is made of overland, with all that its owner's wizard gives it: the figures
        // it starts with, and the wounds it has, which cannot be all of its hit points.
        let hits = view.unit_stat(unit, Stat::MaxHp).max(1);
        let figures = view.unit_stat(unit, Stat::Figures).max(1) as u32;
        let hurt = self
            .ecs
            .get::<&Hurt>(unit)
            .map_or(0, |h| h.0 as i32)
            .min(hits * figures as i32 - 1);
        let mut built = BattleUnit::from_sheet(side, sheet, figures);
        built.source = Some(index);
        built.unit_id = Some(UnitId::new(def.num));
        built.set_lost(hurt);
        built.weapon_quality = view.unit_weapon_quality(unit);
        built.hero_caster = hero.as_ref().is_some_and(|h| h.casting_skill > 0);
        built.hero_name = hero.as_ref().map(|h| h.name.clone()).unwrap_or_default();
        built.casting = self.unit_casting(unit, &built.sheet().abilities.clone());
        built.sails = def.movement == MovementType::Sailing;
        built.swims = def.movement == MovementType::Swimming;
        built.in_stasis = self.unit_in_stasis(unit);
        // A unit held in Stasis cannot act: it is as if asleep.
        built.attacked = built.in_stasis;
        built.realm = view.unit_realm(unit).and_then(Realm::of);
        built.undead = view.unit_counts_as_undead(unit);
        built.bind(&self.content);
        Some(built)
    }

    /// Hurt an overland unit with a damage spell of `strength`, by the dice of a battle: its
    /// figures and wounds are those of the unit, and a unit that loses its last figure is
    /// destroyed. Returns whether it was.
    pub(crate) fn spell_hit_overland_unit(
        &mut self,
        unit: hecs::Entity,
        strength: i32,
        traits: &crate::rules::effects::DamageTraits,
        nature: f4x_def::rules::attack_nature::AttackNature,
        rng: &mut dyn rand::RngCore,
    ) -> bool {
        let Some(mut state) = self.overland_defender(unit) else {
            return false;
        };
        let _ = f4x_battle::battlemagic::casting::damage_state_with_spell(
            &self.content.rules,
            &mut state,
            strength,
            traits,
            nature,
            rng,
        );
        if state.figures == 0 {
            self.remove_unit(unit, crate::hero::Fate::Slain);
            return true;
        }
        let hurt = state.figures_max as i32 * state.hits - state.hit_points();
        self.put(unit, Hurt(hurt.max(0) as u32));
        false
    }

    /// An overland unit as a defender: what its numbers are now, with the global enchantments of
    /// its owner, and the wounds it has.
    fn overland_defender(&self, unit: hecs::Entity) -> Option<super::attack::DefenderState> {
        let view = self.units();
        view.unit_def(unit)?;
        let rules = &self.content.rules;
        let abilities = view.unit_abilities(unit);
        let enchantments = view.unit_enchantment_ids(unit);
        let hits = view.unit_stat(unit, Stat::MaxHp).max(1);
        let figures = view.unit_stat(unit, Stat::Figures).max(1) as u32;
        let hurt = self
            .ecs
            .get::<&Hurt>(unit)
            .map_or(0, |h| h.0 as i32)
            .min(hits * figures as i32 - 1);
        let lucky = abilities.contains(&Ability::new(39 /* lucky */));
        let figures_left = ((hits * figures as i32 - hurt + hits - 1) / hits) as u32;
        Some(super::attack::DefenderState {
            hits,
            extra_hits: 0,
            defense: view.unit_stat(unit, Stat::Defense),
            immunities: f4x_def::rules::immunity::Immunities::of(&abilities, &enchantments, rules),
            resistance: view.unit_stat(unit, Stat::Resistance),
            effects: view.unit_effects(unit),
            realm: view.unit_realm(unit).and_then(Realm::of),
            undead: view.unit_counts_as_undead(unit),
            figures_max: figures,
            to_block: i32::from(lucky),
            figures: figures_left,
            front_damage: figures_left as i32 * hits - (figures as i32 * hits - hurt),
            lucky,
            enchantments,
        })
    }

    /// The units that fight when more than nine could, strongest first: heroes first; then units
    /// with an attack by strength, the more experienced first among equals; settlers, engineers
    /// and the other units with no attack last. The first nine fight.
    pub fn fighter_preset(&self, units: &[hecs::Entity]) -> Vec<hecs::Entity> {
        use crate::rules::{Stat, UnitKind};
        let mut sorted = units.to_vec();
        self.units().sort_by_serial(&mut sorted);
        sorted.sort_by_key(|&u| {
            let hero = self
                .units()
                .unit_def(u)
                .is_some_and(|d| d.kind == UnitKind::Hero);
            let combat = self.units().unit_stat(u, Stat::Melee) > 0
                || self.units().unit_stat(u, Stat::Ranged) > 0;
            (
                !hero,
                !combat,
                std::cmp::Reverse((self.units().unit_power(u) * 10.0) as i64),
                std::cmp::Reverse(self.units().unit_experience(u)),
            )
        });
        sorted
    }

    /// The (at most nine) units that fight out of those sent: all of them up to nine, else the
    /// preset's nine. The rest are the second list.
    fn pick_fighters(&self, units: &[hecs::Entity]) -> (Vec<hecs::Entity>, Vec<hecs::Entity>) {
        let side = self.max_battle_side();
        if units.len() <= side {
            return (units.to_vec(), Vec::new());
        }
        let mut order = self.fighter_preset(units);
        let rest = order.split_off(side);
        (order, rest)
    }

    /// The defenders who fight, and the units of the defending player on the square who do not.
    /// The caller names who fights (the pick screen); a square with more than nine units where
    /// fewer are named leaves the others out, and more than nine named are cut to the preset.
    fn pick_defenders(
        &self,
        named: &[hecs::Entity],
        owner: PlayerId,
        site: Position,
    ) -> (Vec<hecs::Entity>, Vec<hecs::Entity>) {
        let (fighters, _) = self.pick_fighters(named);
        let there: Vec<hecs::Entity> = self
            .economy()
            .army_at(site)
            .into_iter()
            .filter(|&u| self.units().unit_owner(u) == Some(owner))
            .collect();
        if there.len() <= self.max_battle_side() {
            return (named.to_vec(), Vec::new());
        }
        let rest = there
            .into_iter()
            .filter(|u| !fighters.contains(u))
            .collect();
        (fighters, rest)
    }

    /// Defenders that sat a lost battle out run as fleeing units do: half are lost and a quarter
    /// of the heroes, the rest are driven to a free neighbouring square (lost if there is none).
    fn bench_flees(&mut self, benched: &[hecs::Entity], victor: PlayerId, rng: &mut GameRng) {
        use crate::rules::UnitKind;
        let mut benched = benched.to_vec();
        self.units().sort_by_serial(&mut benched);
        for unit in benched {
            let (Some(at), Some(owner)) = (
                self.units().unit_position(unit),
                self.units().unit_owner(unit),
            ) else {
                continue;
            };
            let percent = match self.units().unit_def(unit).map(|d| d.kind) {
                Some(UnitKind::Hero) => 25,
                _ => 50,
            };
            let landing = crate::unit::pathing::neighbours(self, at)
                .into_iter()
                .find(|&next| self.free_landing(next, at, owner, &[unit]));
            match landing {
                Some(next) if rng.roll(100) > percent => {
                    self.strip::<crate::unit::Order>(unit);
                    self.put(unit, next);
                }
                _ => self.remove_unit_to(unit, crate::hero::Fate::Slain, Some(victor)),
            }
        }
    }

    /// Who attacks whom and on which square, if the two stacks can fight: each one player's on
    /// one square, the two players different and not bound by a treaty.
    fn battle_sides(
        &self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
    ) -> Result<(PlayerId, PlayerId, Position), BattleStartError> {
        let side_info = |world: &World, units: &[hecs::Entity]| -> Option<(PlayerId, Position)> {
            let first = *units.first()?;
            let pos = world.units().unit_position(first)?;
            let owner = world.units().unit_owner(first)?;
            // A stack in a tower is on both planes: its units may stand on either.
            let mirror = world.economy().tower_mirror(pos);
            units
                .iter()
                .all(|&u| {
                    let at = world.units().unit_position(u);
                    (at == Some(pos) || (mirror.is_some() && at == mirror))
                        && world.units().unit_owner(u) == Some(owner)
                })
                .then_some((owner, pos))
        };
        let (a_owner, a_pos) = side_info(self, attackers).ok_or(BattleStartError::BadSides)?;
        let (d_owner, site) = side_info(self, defenders).ok_or(BattleStartError::BadSides)?;
        // The attacker came from its own plane: the fight is on the tower square of that plane.
        let site = if site.plane == a_pos.plane {
            site
        } else {
            self.economy()
                .tower_mirror(site)
                .filter(|m| m.plane == a_pos.plane)
                .unwrap_or(site)
        };
        if a_owner == d_owner {
            return Err(BattleStartError::BadSides);
        }
        if self.bound_by_treaty(a_owner, d_owner) {
            return Err(BattleStartError::Treaty);
        }
        Ok((a_owner, d_owner, site))
    }

    /// Whether the two stacks stand on neighbouring squares (or on one); `None` when that
    /// cannot be told.
    fn stacks_near(&self, attackers: &[hecs::Entity], defenders: &[hecs::Entity]) -> Option<bool> {
        let a = self.units().unit_position(*attackers.first()?)?;
        let mut d = self.units().unit_position(*defenders.first()?)?;
        // The same tower square, seen from the attacker's plane.
        if d.plane != a.plane {
            d = self
                .economy()
                .tower_mirror(d)
                .filter(|m| m.plane == a.plane)
                .unwrap_or(d);
        }
        let map = self.map.as_ref()?;
        Some(a.plane == d.plane && map.plane(a.plane).chebyshev((a.x, a.y), (d.x, d.y)) <= 1)
    }

    /// Whether `attackers` can attack `defenders` from where they stand: the sides of a battle,
    /// next to each other. A battle that is being fought does not count.
    pub fn check_attack(
        &self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
    ) -> Result<(), BattleStartError> {
        if self.stacks_near(attackers, defenders) == Some(false) {
            return Err(BattleStartError::NotAdjacent);
        }
        self.battle_sides(attackers, defenders).map(|_| ())
    }

    /// The units are committed to an attack that waits for its defender: they move no further
    /// this turn.
    pub fn commit_attackers(&mut self, attackers: &[hecs::Entity]) {
        for &u in attackers {
            self.put(u, crate::unit::MovesLeft(HalfInt::ZERO));
        }
    }

    /// Begin a battle between two stacks. The defenders' square decides the terrain.
    pub fn start_battle(
        &mut self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
        rng: &mut GameRng,
    ) -> Result<Vec<Event>, BattleStartError> {
        if self.battle.is_some() {
            return Err(BattleStartError::InProgress);
        }
        let (a_owner, d_owner, site) = self.battle_sides(attackers, defenders)?;
        let mut units = Vec::new();
        let mut sources = Vec::new();
        // A fight at a city is fought round its walls, if it has them.
        let city = self.economy().city_at(site);
        let walled = city.is_some_and(|c| {
            self.economy()
                .city_has_building(c, crate::rules::BuildingId::new(35) /* city_walls */)
        });
        let fortress = city.is_some_and(|c| self.economy().is_fortress_city(c));
        // A fight on the sea: only ships, fliers and swimmers take part, unless the side has a
        // Floating Island or Wind Walking, which carry the rest (`Undeployable_Battle_Units_On_Water`).
        let ocean = city.is_none()
            && self.economy().terrain_class_at(site)
                == Some(crate::map::terrain::TerrainClass::Ocean);
        // Ships fight as decks on the sea, and a Floating Island is a ship here: the land units
        // aboard stand on its deck.
        let decks = if ocean {
            self.ship_deck_plan(attackers, defenders)
        } else {
            ShipDeckPlan::default()
        };
        let mut left_out = Vec::new();
        // An army and a battle side are at most nine; a defended square may hold more, and the
        // strongest nine fight (`World::fighter_preset`).
        let attackers = self.pick_fighters(attackers).0;
        let attackers = &attackers[..];
        let (defenders, benched) = self.pick_defenders(defenders, d_owner, site);
        let defenders = &defenders[..];
        for (side, group) in [(Side::Attacker, attackers), (Side::Defender, defenders)] {
            let ship_decks = decks.of(side);
            let carried = ocean
                && group
                    .iter()
                    .any(|&u| self.units().unit_has_flag(u, UnitFlag::WindWalking));
            let fighters: Vec<hecs::Entity> = group
                .iter()
                .copied()
                .filter(|&u| {
                    let on_deck = self
                        .economy()
                        .carrier_of(u)
                        .is_some_and(|c| ship_decks.iter().any(|(ship, _)| *ship == c));
                    let keep = if ocean {
                        carried || on_deck || self.can_fight_at_sea(u)
                    } else {
                        // A ship that cannot move on land (Flight, Wraith Form) is no fighter
                        // in a city or on land (1.50 note 458).
                        !self.is_landlocked_ship(u)
                    };
                    if !keep {
                        left_out.push((side, u));
                    }
                    keep
                })
                .collect();
            let cells = if city.is_some() {
                city_deployment(
                    &self.content.rules.battle.deployment,
                    side == Side::Attacker,
                    fighters.len(),
                    fortress,
                )
            } else {
                deployment(
                    &self.content.rules.battle.deployment,
                    side == Side::Attacker,
                    fighters.len(),
                )
            };
            let mut bus = Vec::new();
            for (n, &unit) in fighters.iter().enumerate() {
                let mut bu = self
                    .battle_unit_from(unit, side, sources.len() + n)
                    .ok_or(BattleStartError::BadSides)?;
                // Wind Walking carries the whole stack through the air.
                if carried {
                    bu.sheet_mut().flags.insert(UnitFlag::Flies);
                }
                bus.push(bu);
            }
            // Where the units stand is the battle's rule; the world says who is on which ship.
            let decks_here: Vec<(usize, Vec<Pos>)> = ship_decks
                .iter()
                .filter_map(|(ship, cells)| {
                    Some((fighters.iter().position(|f| f == ship)?, cells.clone()))
                })
                .collect();
            let carried_by: Vec<Option<usize>> = fighters
                .iter()
                .map(|&f| {
                    let carrier = self.economy().carrier_of(f)?;
                    fighters.iter().position(|&s| s == carrier)
                })
                .collect();
            let placed = place_side(
                &mut bus,
                Placement {
                    cells,
                    decks: &decks_here,
                    carried_by: &carried_by,
                    deck_cells: &decks.cells(),
                },
            );
            for ((&unit, bu), placed) in fighters.iter().zip(bus).zip(placed) {
                if placed {
                    sources.push(unit);
                    units.push(bu);
                }
            }
        }
        let seed = rng.next_u64();
        // The field is built from the square alone, so a fight on the same square is fought on the
        // same ground.
        let mut map = self.battle_field(site, city, walled, &decks.cells());
        if self.settings.rules.original_battlefield {
            map.shut_beyond_original();
        }
        let has = |effect: crate::spell::city::CityEffect| {
            city.is_some_and(|c| self.economy().city_has_effect(c, effect))
        };
        let has_wall = |kind: &str| city.is_some_and(|c| self.economy().city_has_wall(c, kind));
        if city.is_some() {
            map = map
                .with_city(walled)
                .with_wall_rules(self.content.rules.battle.city_wall);
            if has(crate::spell::city::CityEffect::FlyingFortress) {
                map = map.with_flying_fortress();
            }
        }
        let mut battle = Battle::new(self.content.clone(), a_owner, d_owner, map, units, seed);
        battle.site = Some(site);
        battle.apply_settings(&self.settings.rules);
        (battle.node_aura, battle.node_site) = self.node_realms_at(site, city.is_none());
        for (side, owner) in [(Side::Attacker, a_owner), (Side::Defender, d_owner)] {
            battle.set_wizard(side, self.wizard_sheet(owner));
        }
        battle.note_invisibility();
        battle.wall_of_fire = has_wall("fire");
        battle.wall_of_darkness = has_wall("darkness");
        if let Some(c) = city {
            battle.warded_realms = self.economy().city_shielded_realms(c);
        }
        // A Cloud of Shadow treats every fight at the city as if its owner had cast Darkness.
        if let Some(cloud) = self
            .economy()
            .held_city_effect(city, crate::spell::city::CityEffect::CloudOfShadow)
        {
            battle.raise_field(Side::Defender, Field::Darkness, cloud);
        }
        // Heavenly Light gives the defender True Light in every fight at the city.
        if let Some(light) = self
            .economy()
            .held_city_effect(city, crate::spell::city::CityEffect::HeavenlyLight)
        {
            battle.raise_field(Side::Defender, Field::TrueLight, light);
        }
        // Eternal Night covers every fight in Darkness, except at a city with Heavenly Light.
        if let Some(night) =
            self.held_global_effect(crate::spell::global_enchantment::GlobalEffect::EternalNight)
        {
            if !has(crate::spell::city::CityEffect::HeavenlyLight) {
                battle.raise_field(Side::Defender, Field::Darkness, night);
            }
        }
        self.battle_casters(&mut battle, site);
        self.battle = Some(ActiveBattle {
            battle,
            sources,
            lair: None,
            left_out,
            benched,
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

    /// Whether the unit is a ship that cannot move on land: only Flight or Wraith Form
    /// (non-corporeal) lets a ship stand and fight there.
    fn is_landlocked_ship(&self, unit: hecs::Entity) -> bool {
        self.units()
            .unit_def(unit)
            .is_some_and(|d| d.movement == MovementType::Sailing)
            && !self.units().unit_has_flag(unit, UnitFlag::Flies)
            && !self.units().unit_has_flag(unit, UnitFlag::NonCorporeal)
    }

    /// Whether a unit may fight on the sea: a ship (it carries others or sails), a flier or a
    /// swimmer.
    fn can_fight_at_sea(&self, unit: hecs::Entity) -> bool {
        self.units().unit_def(unit).is_some_and(|d| {
            matches!(
                d.movement,
                MovementType::Flying | MovementType::Sailing | MovementType::Swimming
            )
        }) || self.units().unit_has_flag(unit, UnitFlag::Flies)
            || self.units().unit_stat(unit, Stat::Transport) > 0
    }

    /// The realms of the node a fight on `site` is under: the one whose aura covers the square,
    /// and (when `open_ground`, no city) the one that stands on it (`Combat_Node_Type` and the
    /// centre structure of `Build_Battlefield`).
    fn node_realms_at(&self, site: Position, open_ground: bool) -> (Option<Realm>, Option<Realm>) {
        let aura = self
            .economy()
            .node_covering(site)
            .and_then(|n| Realm::of(n.realm));
        let on = open_ground
            .then(|| self.economy().node_at(site))
            .flatten()
            .and_then(|n| Realm::of(n.realm));
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
        spec.scenery = self.content.rules.battle.scenery;
        BattleMap::from_layout(layout::generate(&spec, &mut rng))
    }

    /// Carry out a battle command; when it ends the battle, apply the results.
    pub fn battle_command(
        &mut self,
        command: BattleCommand,
        rng: &mut GameRng,
    ) -> Result<Vec<Event>, BattleError> {
        if let BattleCommand::Cast { spell, aim, mana } = command {
            return self.battle_cast(spell, aim, mana, rng);
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
            return self.battle_unit_cast(unit, spell, aim, mana, rng);
        }
        let active = self.battle.as_mut().ok_or(BattleError::Over)?;
        let events = active.battle.apply(command)?;
        // Mana Leak and the like change the wizards' reserves.
        self.sync_battle_mana();
        let mut out: Vec<Event> = events.iter().cloned().map(Event::Battle).collect();
        self.finish_if_over(&mut out, rng);
        Ok(out)
    }

    /// A battle whose winner is settled is written back, whatever settled it. (A cast, an end of
    /// turn effect or the start of the battle can settle it without the command's own events
    /// saying so, and a battle left open this way blocks every later fight.)
    pub(crate) fn finish_if_over(&mut self, out: &mut Vec<Event>, rng: &mut GameRng) {
        if self
            .battle
            .as_ref()
            .is_some_and(|a| a.battle.winner.is_some())
        {
            out.extend(self.finish_battle(rng));
        }
    }

    /// What the defender lost, for the AI to weigh (`End_Of_Combat`, `f4x-ai/src/opinion.rs`): the heroes,
    /// the other units and the settlers that fell or fled. The units of the field that were summoned in the
    /// fight do not count.
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
                (hero, u.is_settler())
            })
            .collect();
        let (mut heroes, mut others, mut settlers) = (0, 0, 0);
        for &(hero, settler) in &lost {
            heroes += u32::from(hero);
            others += u32::from(!hero);
            settlers += u32::from(settler);
        }
        if heroes + others > 0 {
            self.report_diplomacy(crate::diplomacy::DiplomacyEvent::BattleLosses {
                attacker,
                defender,
                heroes,
                others,
                settlers,
            });
        }
    }

    /// Write a finished battle back: wounds, deaths, and who won.
    pub(crate) fn finish_battle(&mut self, rng: &mut GameRng) -> Vec<Event> {
        let Some(active) = self.battle.take() else {
            return Vec::new();
        };
        let ActiveBattle {
            battle,
            sources,
            lair,
            left_out,
            benched,
        } = active;
        // The defenders that did not fight flee if the defence lost (before the square is
        // taken, so that nobody is left on it).
        if battle.winner == Some(Side::Attacker) {
            self.bench_flees(&benched, battle.player_of(Side::Attacker), rng);
        }
        // The units that sat the fight out go down with a side that lost it.
        let mut fallen: std::collections::BTreeMap<crate::player::PlayerId, u32> =
            std::collections::BTreeMap::new();
        if let Some(winner) = battle.winner {
            for (side, unit) in left_out {
                if side != winner {
                    *fallen.entry(battle.player_of(side)).or_default() += 1;
                    self.remove_unit_to(
                        unit,
                        crate::hero::Fate::Slain,
                        Some(battle.player_of(winner)),
                    );
                }
            }
        }
        let mut events = Vec::new();
        // The units of the army that fell in the fight (a summoned creature is no unit of the army,
        // and one that regenerates or comes back is not lost): counted by who owned them.
        for u in &battle.units {
            if u.source.is_some()
                && u.status == Status::Dead
                && !(u.regenerates() && battle.revived(u))
            {
                *fallen.entry(battle.player_of(u.side)).or_default() += 1;
            }
        }
        events.extend(
            fallen
                .into_iter()
                .map(|(player, units)| Event::BattleCasualties { player, units }),
        );
        let spoils = battle.spoils(&self.content.rules);
        self.battle_loss_relations(&battle, &sources);
        let survivors: Vec<hecs::Entity> = battle
            .units
            .iter()
            .filter(|u| {
                Some(u.side) == battle.winner && (u.status == Status::Active || battle.revived(u))
            })
            .filter_map(|u| u.source.and_then(|i| sources.get(i).copied()))
            .collect();
        // Units drained to death on the losing side join the winner as undead, in a fight at a
        // lair as on the map.
        let mut drained = Vec::new();
        let mut zombified = std::collections::HashSet::new();
        let mastery = self.winner_has_zombie_mastery(&battle);
        for u in &battle.units {
            let Some(entity) = u.source.and_then(|i| sources.get(i).copied()) else {
                continue;
            };
            // The charges an item's spell has left stay on the item.
            if !u.casting.item_charges.is_empty() {
                for c in &u.casting.item_charges {
                    if let Ok(mut item) = self.ecs.get::<&mut crate::item::Item>(c.item) {
                        if let Some((_, charges)) = item.charges.as_mut() {
                            *charges = c.charges;
                        }
                    }
                }
            }
            // Regeneration heals the unit completely after the fight, and brings back one that
            // was slain (not drained or destroyed) if its side won.
            let regenerates = u.regenerates();
            let revived = battle.revived(u);
            if regenerates && (revived || u.status != Status::Dead) {
                self.put(entity, Hurt(0));
            } else if u.status == Status::Dead {
                let kind = self.units().unit_def(entity).map(|d| d.kind);
                if battle.is_raised_by(u, kind, mastery, &self.content.rules) {
                    drained.push(entity);
                    if battle.is_zombified(u, kind, mastery, &self.content.rules) {
                        zombified.insert(entity);
                    }
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
                // The hit points a lasting spell cast in the battle gave (Lionheart) end with
                // it, and the extras of life steal were never the unit's. The damage it took
                // stays, and a unit that cannot stand it without them is dead
                // (`divergences.md` 45): the sweep after this command removes it, as it does
                // when a spell like that is dispelled on the map.
                self.put(entity, Hurt(u.lost().max(0) as u32));
            }
        }
        // A unit that stood on hit points a spell of the battle gave is dead now.
        events.extend(self.remove_units_without_hit_points());
        // Passengers of a ship that went down in the fight scramble ashore or drown.
        events.extend(self.drown_stranded(None, rng));
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
            events.extend(self.overland_aftermath(&battle, &sources, rng));
            events.extend(self.raise_drained(&battle, drained, &zombified, &sources));
        } else {
            // They stand on the site when the winner's survivors move in.
            events.extend(self.raise_drained(&battle, drained, &zombified, &sources));
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
            events.extend(self.finish_site_battle(index, winner, &attackers, &guards, rng));
        }
        events
    }

    /// Whether the winner holds Zombie Mastery.
    fn winner_has_zombie_mastery(&self, battle: &Battle) -> bool {
        battle.winner.is_some_and(|side| {
            self.has_global_effect(
                battle.player_of(side),
                crate::spell::global_enchantment::GlobalEffect::ZombieMastery,
            )
        })
    }

    /// The winner's unit that made the undead: the costliest with Life Steal or Create Undead.
    fn undead_creator(&self, battle: &Battle, sources: &[hecs::Entity]) -> Option<UnitId> {
        let index = battle.undead_creator(|i| {
            let def = self.units().unit_def(*sources.get(i)?)?;
            Some(def.cost)
        })?;
        Some(self.ecs.get::<&crate::unit::Unit>(sources[index]).ok()?.0)
    }

    /// Put the drained dead on the winner's square as undead units. All of them rise: they are
    /// forced there, so a full square is no bar.
    fn raise_drained(
        &mut self,
        battle: &Battle,
        mut dead: Vec<hecs::Entity>,
        zombified: &std::collections::HashSet<hecs::Entity>,
        sources: &[hecs::Entity],
    ) -> Vec<Event> {
        let creator = self.undead_creator(battle, sources);
        let winner = battle.winner.map(|side| battle.player_of(side));
        let (Some(player), Some(pos)) = (winner, battle.site) else {
            for u in dead {
                self.remove_unit_to(u, crate::hero::Fate::Slain, winner);
            }
            return Vec::new();
        };
        self.units().sort_by_serial(&mut dead);
        let (mut raised, mut zombies) = (0, 0);
        for unit in dead {
            self.make_undead(unit);
            self.strip::<Hurt>(unit);
            self.strip::<crate::unit::Order>(unit);
            self.set_owner(unit, player);
            self.put(unit, pos);
            self.put(unit, crate::unit::MovesLeft(HalfInt::ZERO));
            if zombified.contains(&unit) {
                zombies += 1;
            } else {
                raised += 1;
            }
        }
        let mut events = Vec::new();
        if zombies > 0 {
            events.push(Event::UndeadRaised {
                player,
                units: zombies,
                zombies: true,
                creator: None,
            });
        }
        if raised > 0 {
            events.push(Event::UndeadRaised {
                player,
                units: raised,
                zombies: false,
                creator,
            });
        }
        events
    }

    /// What a battle on the overland map leaves behind: units that fled lose some of their
    /// number, and an attacker who won and cleared the square moves in and takes a city.
    fn overland_aftermath(
        &mut self,
        battle: &Battle,
        sources: &[hecs::Entity],
        rng: &mut GameRng,
    ) -> Vec<Event> {
        use crate::rules::UnitKind;
        let mut events = Vec::new();
        let source_of = |u: &BattleUnit| u.source.and_then(|i| sources.get(i).copied());

        // Fleeing costs half the units and a quarter of the heroes. Units that are
        // asleep, confused or webbed are always lost, and on Intro and Easy the player whose
        // turn it is (the attacker: a battle starts on its mover's turn) loses nothing else
        // (`Combat.c`).
        // Heroes taken off the field by Recall Hero are at the summoning circle now.
        for u in battle.units.iter().filter(|u| u.status == Status::Recalled) {
            let to = u
                .recalled_to
                .or_else(|| self.summon_point(battle.player_of(u.side)));
            if let (Some(unit), Some(to)) = (source_of(u), to) {
                self.put(unit, to);
            }
        }
        for side in [Side::Attacker, Side::Defender] {
            let mut fled: Vec<(hecs::Entity, bool)> = battle
                .units
                .iter()
                .filter(|u| u.side == side && u.status == Status::Fled && !u.slipped)
                .filter_map(|u| {
                    // Asleep, confused or in a web that has not been broken.
                    let effects = u.effects(&self.content().rules);
                    let webbed = effects.contains(crate::spell::unit_effect::UnitEffect::Web);
                    let held = effects.contains(crate::spell::unit_effect::UnitEffect::Holds)
                        && (!webbed || u.held_by_web(&self.content().rules));
                    source_of(u).map(|e| (e, held))
                })
                .collect();
            if fled.is_empty() {
                continue;
            }
            fled.sort_by_key(|&(e, _)| (self.units().serial_of(e), e.id()));
            let human = battle.player_of(side) == self.human().unwrap_or(PlayerId(u8::MAX));
            let safe = side == Side::Attacker && self.settings.rules.safe_flee.of(human);
            let mut lost = 0;
            for (unit, held) in fled {
                let percent = match self.units().unit_def(unit).map(|d| d.kind) {
                    Some(UnitKind::Hero) => 25,
                    _ => 50,
                };
                if held || (!safe && rng.roll(100) <= percent) {
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
            .economy()
            .army_at(pos)
            .into_iter()
            .any(|u| self.units().unit_owner(u).is_some_and(|o| o != attacker));
        if hostile_left {
            return events;
        }
        let movers: Vec<hecs::Entity> = battle
            .units
            .iter()
            .filter(|u| {
                u.side == Side::Attacker && (u.status == Status::Active || battle.revived(u))
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
                    events.extend(self.rampage_city(city, battle.city_damage, &movers, rng));
                    return events;
                }
            }
        }
        if let Some(&first) = movers.first() {
            let from = self.units().unit_position(first).unwrap_or(pos);
            for &u in &movers {
                self.put(u, pos);
            }
            self.explore(attacker, pos, crate::map::fog::sight_for(2));
            // A stack that ends its move in a tower holds it.
            self.claim_tower(&movers, pos);
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
                events.extend(self.conquer_city(city, attacker, battle.city_damage, rng));
            }
        }
        events
    }

    /// Fight a battle to the end with the battle AI playing both sides, or with the orders of
    /// [`World::script_battle_orders`].
    pub fn auto_resolve(
        &mut self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
        rng: &mut GameRng,
    ) -> Vec<Event> {
        self.auto_resolve_with(attackers, defenders, None, rng)
    }

    /// [`World::auto_resolve`] with a limit on the mana one wizard may spend from the reserve
    /// (strategic combat, `Command::AutoResolve::mana_budget`).
    pub fn auto_resolve_with(
        &mut self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
        mana_budget: Option<(PlayerId, u32)>,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        // A script is for this battle, whether it starts or not.
        let mut script = self.auto_orders.script.take().map(Vec::into_iter);
        if self.stacks_near(attackers, defenders) == Some(false) {
            return vec![Event::Rejected(crate::world::Rejection::BattleStart(
                BattleStartError::NotAdjacent,
            ))];
        }
        let mut events = match self.start_battle(attackers, defenders, rng) {
            Ok(e) => e,
            Err(e) => return vec![Event::Rejected(crate::world::Rejection::BattleStart(e))],
        };
        if let Some((player, mana)) = mana_budget {
            let side = self.battle().and_then(|b| b.side_of(player));
            if let Some(side) = side {
                self.set_battle_mana_budget(side, Some(mana));
            }
        }
        let mut given = Vec::new();
        for _ in 0..20_000 {
            let next = match script.as_mut() {
                Some(orders) => orders.next(),
                None => self.next_battle_command(),
            };
            let Some(command) = next else {
                break;
            };
            given.push(command.clone());
            match self.battle_command(command, rng) {
                Ok(e) => events.extend(e),
                Err(_) => break,
            }
        }
        // A battle that somehow stalls ends with the active side fleeing.
        while self.battle.is_some() {
            match self.battle_command(BattleCommand::Flee, rng) {
                Ok(e) => events.extend(e),
                Err(_) => break,
            }
        }
        self.finish_if_over(&mut events, rng);
        self.auto_orders.given = Some(given);
        events
    }
}
