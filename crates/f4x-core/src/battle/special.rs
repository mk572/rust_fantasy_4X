//! Special attacks: what a unit's blows do besides damage.
//!
//! The rules follow the original's `Battle_Unit_Process_Attack` (see `attack::process_attack`
//! for where each one runs):
//!
//! - **Poison:** each attacking figure makes the target resist once per point of poison
//!   strength; each failure is one point of damage that no defence blocks.
//! - **Stoning touch, death touch, destruction, dispel evil:** each attacking figure makes the
//!   target resist (a stoning or death touch at a penalty, destruction at none, dispel evil at
//!   -4 against creatures of chaos and death, -9 against the undead); a failure kills a whole
//!   figure.
//! - **Life steal:** each attacking figure makes the target resist at a penalty; every point by
//!   which it fails is a point of damage no defence blocks, and the attacker heals as much.
//! - **Gazes** (stoning, death, or both for a multiple gaze): used in the thrown and breath
//!   step, once against each of the target's figures, each resisting at a penalty or dying.
//! - **Chaos** (`doom_damage`): no to-hit roll, no defence: the attack strength is the damage.
//! - **Armour piercing** halves the defence (`immunity::effective_defense`).
//!
//! Magic immunity protects against all of these but poison, and each of the others is stopped
//! by its own immunity (stoning, death, poison). The penalty is the size of the unit's special
//! level (`Stat::SpecialLevel`). Not here: power drain (it needs the mana of battle spells,
//! task 43), and the extra penalty of touches that come from an item or spell rather than the
//! unit itself.

/// A gaze: a ranged attack that makes each of the target's figures resist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gaze {
    #[default]
    None,
    Stoning,
    Death,
    /// Both of the above, and the damage of the unit's ranged attack ignores defence.
    Multi,
}

/// The special attacks a unit has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpecialAttacks {
    /// Poison strength: resistance checks per attacking figure.
    pub poison: i32,
    pub life_steal: bool,
    pub stoning_touch: bool,
    pub death_touch: bool,
    pub destruction: bool,
    pub dispel_evil: bool,
    /// Its victims can be raised as undead (Ghouls); with life steal, which does so too, this
    /// is not an attack of its own.
    pub create_undead: bool,
    /// An item's Power Drain: the target that fails a resistance check loses 2 to 20 mana.
    pub power_drain: bool,
    /// The penalty to the target's resistance for gazes, stoning and death touches and life
    /// steal.
    pub penalty: i32,
    /// What more the touches cost the target's resistance when an item or a spell gives them:
    /// stoning touch 1, death touch and life steal 3 (the original's `noninnate` attributes).
    pub stoning_extra: i32,
    pub death_extra: i32,
    pub life_steal_extra: i32,
    pub gaze: Gaze,
}

impl SpecialAttacks {
    /// The special attacks among a unit's abilities, with its special level.
    pub fn of_unit(abilities: &[String], granted: &[String], special_level: i32) -> SpecialAttacks {
        let has = |name: &str| abilities.iter().any(|a| a == name);
        let given = |name: &str, extra: i32| {
            if granted.iter().any(|a| a == name) {
                extra
            } else {
                0
            }
        };
        let level = special_level.abs();
        SpecialAttacks {
            poison: if has("poison_touch") { level } else { 0 },
            life_steal: has("life_steal"),
            stoning_touch: has("stoning_touch"),
            death_touch: has("death_touch"),
            destruction: has("destruction"),
            dispel_evil: has("dispel_evil"),
            create_undead: has("create_undead"),
            power_drain: has("power_drain"),
            penalty: level,
            stoning_extra: given("stoning_touch", 1),
            death_extra: given("death_touch", 3),
            life_steal_extra: given("life_steal", 3),
            gaze: if has("multi_gaze") {
                Gaze::Multi
            } else if has("stoning_gaze") {
                Gaze::Stoning
            } else if has("death_gaze") {
                Gaze::Death
            } else {
                Gaze::None
            },
        }
    }

    /// Whether the unit has none of them.
    pub fn is_none(&self) -> bool {
        self.poison == 0
            && !self.life_steal
            && !self.stoning_touch
            && !self.death_touch
            && !self.destruction
            && !self.dispel_evil
            && !self.create_undead
            && !self.power_drain
            && self.gaze == Gaze::None
    }
}
