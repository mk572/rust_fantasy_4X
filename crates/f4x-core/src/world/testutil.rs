//! Content for unit tests: [`shipped`] is the game's own `content/`. [`Table`] is a world with
//! the dice a test rolls for it, and [`fresh`] loads one that `new_game` made.
//!
//! Tests: none, because this is test support.

use crate::player::PlayerId;
use crate::world::rng::GameRng;
use crate::world::World;
use crate::world::{Command, Event};
use f4x_def::content::Content;
/// An entity, as [`drawing_ops`](crate::drawing_ops) names it for the crate that expands it.
pub use hecs::Entity;

/// The game's own `content/` folder, loaded by the real loader (`f4x_def::testdir::shipped`), as
/// a copy for a `World`, which owns its content.
pub fn shipped() -> Content {
    crate::battle::world::set_battle_ai(battle_policy);
    f4x_def::testdir::shipped().clone()
}

/// The battle player of core's own tests: every unit hits what is next to it, shoots what it can
/// or walks at the nearest enemy, with no walls and no spells. It stands in for the scripted
/// battle AI (`f4x_ai::battle`), which core cannot use, wherever a test only needs a fight to
/// get played to the end.
pub fn battle_policy(world: &crate::world::World) -> Option<crate::battle::BattleCommand> {
    use crate::battle::map::distance;
    use crate::battle::{BattleCommand, RangedKind};
    let battle = world.battle()?;
    let side = battle.active;
    for unit in battle.active_units(side) {
        if unit.moves_left == 0 {
            continue;
        }
        let Some(enemy) = battle
            .active_units(side.other())
            .filter(|e| battle.seen_by(side, e.id))
            .min_by_key(|e| (distance(unit.cell, e.cell), e.id))
        else {
            return Some(BattleCommand::EndTurn);
        };
        if unit.can_attack() {
            if distance(unit.cell, enemy.cell) == 1 {
                return Some(BattleCommand::Attack {
                    unit: unit.id,
                    target: enemy.id,
                });
            }
            let shoots = matches!(unit.ranged_kind, RangedKind::Missile | RangedKind::Magic)
                && unit.own_ranged > 0
                && unit.ammo > 0;
            if shoots && !battle.blocked_by_darkness(unit.id, enemy.id) {
                return Some(BattleCommand::Shoot {
                    unit: unit.id,
                    target: enemy.id,
                });
            }
        }
        if unit.moves_left > 0 {
            let here = distance(unit.cell, enemy.cell);
            let best = battle
                .reachable(unit.id)
                .into_iter()
                .min_by_key(|&(p, cost)| (distance(p, enemy.cell), cost, p));
            if let Some((to, _)) = best {
                if distance(to, enemy.cell) < here {
                    return Some(BattleCommand::Move { unit: unit.id, to });
                }
            }
        }
        return Some(BattleCommand::Done { unit: unit.id });
    }
    Some(BattleCommand::EndTurn)
}

/// The saved fresh games of [`fresh`]: (computer wizards, seed).
pub const FRESH_WORLDS: [(u8, u64); 4] = [(2, 7), (3, 3), (3, 7), (3, 11)];

/// Where the saved fresh games of [`fresh`] are (`crates/f4x-core/tests/saves`).
pub fn saves_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/saves")
}

/// The file of the fresh game with `wizards` computer wizards on `seed`.
pub fn fresh_file(wizards: u8, seed: u64) -> std::path::PathBuf {
    saves_dir().join(format!("fresh_w{wizards}_s{seed}.save"))
}

/// A new game of `wizards` computer wizards on `seed`, on the shipped content, loaded from its
/// save: the world `f4x_newgame::setup::new_game` made, with a real map, cities, units and wizards,
/// and the generator where the setup left it.
/// Core cannot make one itself (the crate that does sits above it); the test `fixtures` of
/// `f4x-newgame` keeps these files equal to what it makes, and rewrites them with
/// `F4X_REFRESH_FIXTURES=1 cargo test -p f4x-newgame --test it fixtures`.
pub fn fresh(wizards: u8, seed: u64) -> Result<Table, String> {
    let path = fresh_file(wizards, seed);
    let failed = |e: String| {
        format!(
            "{}: {e} (rewrite the fresh games with F4X_REFRESH_FIXTURES=1 cargo test -p f4x-newgame --test it fixtures)",
            path.display()
        )
    };
    let (world, _) =
        crate::world::save::file::load_game(&path, shipped()).map_err(|e| failed(e.to_string()))?;
    let dice =
        std::fs::read_to_string(path.with_extension("rng")).map_err(|e| failed(e.to_string()))?;
    let rng = ron::from_str(&dice).map_err(|e| failed(e.to_string()))?;
    Ok(Table { world, rng })
}

/// A world and the generator its tests draw from: what a game holds, without the flow of the
/// game. It reads and changes like its [`World`]; what draws is given the generator.
pub struct Table {
    pub world: World,
    pub rng: GameRng,
}

impl Table {
    pub fn new(world: World, rng: GameRng) -> Self {
        Self { world, rng }
    }

    /// A world with nothing in it, with dice from `seed`.
    pub fn empty(seed: u64) -> Self {
        Self::new(World::new(), GameRng::new(seed))
    }

    /// The world and the generator, for an operation that has no method here.
    pub fn world_and_rng(&mut self) -> (&mut World, &mut GameRng) {
        (&mut self.world, &mut self.rng)
    }

    pub fn seed(&self) -> u64 {
        self.rng.seed()
    }

    pub fn draws(&self) -> u64 {
        self.rng.draws()
    }

    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        self.world.apply(command, &mut self.rng)
    }

    pub fn start_round(&mut self, round: u32) -> Vec<Event> {
        self.world.start_round(round, &mut self.rng)
    }

    pub fn start_player_turn(&mut self, player: PlayerId, round: u32) -> Vec<Event> {
        self.world.start_player_turn(player, round, &mut self.rng)
    }

    pub fn end_player_turn(&mut self, player: PlayerId) -> Vec<Event> {
        self.world.end_player_turn(player, &mut self.rng)
    }

    pub fn end_round(&mut self) -> Vec<Event> {
        self.world.end_round(&mut self.rng)
    }
}

impl From<(World, GameRng)> for Table {
    fn from((world, rng): (World, GameRng)) -> Self {
        Self::new(world, rng)
    }
}

impl std::ops::Deref for Table {
    type Target = World;

    fn deref(&self) -> &World {
        &self.world
    }
}

impl std::ops::DerefMut for Table {
    fn deref_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

crate::drawing_ops!(Table);

/// For a test type with a `world_and_rng` method: the world operations that draw, as methods that
/// hand them the generator. A test then calls them as it calls any other.
#[macro_export]
macro_rules! drawing_ops {
    ($kind:ty) => {
        $crate::drawing_ops! { $kind;
            start_battle(attackers: &[$crate::world::testutil::Entity], defenders: &[$crate::world::testutil::Entity]) -> Result<Vec<$crate::world::Event>, $crate::battle::world::BattleStartError>;
            battle_command(command: ::f4x_battle::battle::BattleCommand) -> Result<Vec<$crate::world::Event>, ::f4x_battle::battle::BattleError>;
            auto_resolve(attackers: &[$crate::world::testutil::Entity], defenders: &[$crate::world::testutil::Entity]) -> Vec<$crate::world::Event>;
            auto_resolve_with(attackers: &[$crate::world::testutil::Entity], defenders: &[$crate::world::testutil::Entity], mana_budget: Option<($crate::player::PlayerId, u32)>) -> Vec<$crate::world::Event>;
            autobuild_choice(city: $crate::world::testutil::Entity, owner: $crate::player::PlayerId) -> Option<$crate::city::production::ProductionItem>;
            set_autobuild(city: $crate::world::testutil::Entity, plan: Option<$crate::city::autobuild::BuildPlan>) -> Result<(), $crate::city::production::ProductionError>;
            conquer_city(city: $crate::world::testutil::Entity, winner: $crate::player::PlayerId, city_damage: u32) -> Vec<$crate::world::Event>;
            raze_city(player: $crate::player::PlayerId, city: $crate::world::testutil::Entity) -> Vec<$crate::world::Event>;
            found_neutral_city(pos: $crate::map::Position, race: $crate::rules::RaceId) -> $crate::world::testutil::Entity;
            apply_production(city: $crate::world::testutil::Entity, production: f4x_def::half_int::HalfInt) -> Option<$crate::city::production::ProductionItem>;
            declare_peace(a: $crate::player::PlayerId, b: $crate::player::PlayerId) -> ();
            enter_site(units: &[$crate::world::testutil::Entity], to: $crate::map::Position) -> Result<Vec<$crate::world::Event>, $crate::feature::lair::SiteError>;
            meld(units: &[$crate::world::testutil::Entity]) -> Result<Vec<$crate::world::Event>, $crate::feature::node::MeldError>;
            add_hero(kind: usize) -> Option<$crate::world::testutil::Entity>;
            pick_hero(player: $crate::player::PlayerId, zero_cost: bool, class: $crate::hero::kind::Class) -> Option<usize>;
            cheat_offer(player: $crate::player::PlayerId, what: $crate::player::offer::OfferKind, round: u32) -> Option<$crate::player::offer::Offer>;
            turn_off_unit_spell(player: $crate::player::PlayerId, unit: $crate::world::testutil::Entity, spell: $crate::rules::SpellId) -> Result<Vec<$crate::world::Event>, $crate::wizard::magic::MagicError>;
            capture_city(city: $crate::world::testutil::Entity, winner: $crate::player::PlayerId) -> Vec<$crate::world::Event>;
            resign(player: $crate::player::PlayerId) -> Result<Vec<$crate::world::Event>, $crate::wizard::fortress::FortressError>;
            destroy_city(city: $crate::world::testutil::Entity) -> Result<Vec<$crate::world::Event>, $crate::wizard::fortress::FortressError>;
            init_wizard(player: $crate::player::PlayerId, books: &[($crate::rules::RealmId, u8)], retorts: &[$crate::wizard::magic::Retort], start_spells: Option<&[$crate::rules::SpellId]>) -> Result<(), $crate::wizard::magic::MagicError>;
            refill_research(player: $crate::player::PlayerId) -> ();
            start_cast(player: $crate::player::PlayerId, spell: $crate::rules::SpellId, target: Option<$crate::rules::exec::Target>) -> Result<Vec<$crate::world::Event>, $crate::wizard::magic::MagicError>;
            start_cast_with(player: $crate::player::PlayerId, spell: $crate::rules::SpellId, target: Option<$crate::rules::exec::Target>, extra: i32) -> Result<Vec<$crate::world::Event>, $crate::wizard::magic::MagicError>;
            finish_cast_with_target(player: $crate::player::PlayerId, target: $crate::rules::exec::Target) -> Result<Vec<$crate::world::Event>, $crate::wizard::magic::MagicError>;
            create_wizard(player: $crate::player::PlayerId, build: &$crate::wizard::WizardBuild) -> Result<(), $crate::wizard::BuildError>;
            cheat_defeat_player(loser: $crate::player::PlayerId, winner: $crate::player::PlayerId) -> Vec<$crate::world::Event>;
            cheat_eliminate(player: $crate::player::PlayerId, by: $crate::player::PlayerId) -> Vec<$crate::world::Event>;
            cheat_add_lair(pos: $crate::map::Position, feature: $crate::map::FeatureId, strong: bool) -> Result<(), String>;
            cheat_reroll_hero(kind: usize) -> Result<(), String>;
            cheat_event(kind: $crate::world_event::EventKind, victim: $crate::player::PlayerId) -> Result<Vec<$crate::world::Event>, String>;
        }
    };
    ($kind:ty; $($name:ident($($arg:ident: $of:ty),*) -> $ret:ty;)*) => {
        impl $kind {
            $(
                #[allow(clippy::unused_unit)]
                pub fn $name(&mut self, $($arg: $of),*) -> $ret {
                    let (world, rng) = self.world_and_rng();
                    world.$name($($arg,)* rng)
                }
            )*

            /// Uniform random integer in `1..=n`, from the test's generator.
            pub fn roll(&mut self, n: u32) -> u32 {
                self.world_and_rng().1.roll(n)
            }

            /// The next value of the test's generator.
            pub fn next_u64(&mut self) -> u64 {
                $crate::world::rng::RngCore::next_u64(self.world_and_rng().1)
            }
        }
    };
}
