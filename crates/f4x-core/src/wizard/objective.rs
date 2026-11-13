//! A wizard's personality and objective, as the original's `Init_Magic_Personalities_Objectives`
//! draws them at game start (`INITGAME.c`). The personality steers diplomacy; the
//! objective is recorded and shown on a rival's wizard sheet.

use serde::{Deserialize, Serialize};

use crate::game::Game;
use crate::player::PlayerId;
use crate::rules::UnitId;
use crate::wizard::entity::Wizard;

/// On a wizard: its temper. The human has none and counts as `Chaotic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Personality {
    Maniacal,
    Ruthless,
    Aggressive,
    #[default]
    Chaotic,
    Lawful,
    Peaceful,
}

impl Personality {
    /// The key of the personality's name in `content/text/`.
    pub fn key(self) -> &'static str {
        match self {
            Personality::Maniacal => "personality.maniacal",
            Personality::Ruthless => "personality.ruthless",
            Personality::Aggressive => "personality.aggressive",
            Personality::Chaotic => "personality.chaotic",
            Personality::Lawful => "personality.lawful",
            Personality::Peaceful => "personality.peaceful",
        }
    }

    /// The original's `TBL_AI_PRS_IDK_Mod`: how well the wizard takes proposals.
    pub fn modifier(self) -> i32 {
        self as i32 * 10
    }
}

/// What an AI wizard is after (the original's `AI_OBJ_*`, in its order). On a wizard; the human
/// has none.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumIter,
)]
pub enum Objective {
    Pragmatist,
    Militarist,
    Theurgist,
    Perfectionist,
    Expansionist,
}

impl Objective {
    /// Every objective, in the original's order.
    pub fn all() -> impl Iterator<Item = Objective> {
        <Objective as strum::IntoEnumIterator>::iter()
    }

    /// The key of the objective's name in `content/text/`.
    pub fn key(self) -> &'static str {
        match self {
            Objective::Pragmatist => "personality.pragmatist",
            Objective::Militarist => "personality.militarist",
            Objective::Theurgist => "personality.theurgist",
            Objective::Perfectionist => "personality.perfectionist",
            Objective::Expansionist => "personality.expansionist",
        }
    }
}

impl Wizard<'_> {
    /// The wizard's temper; the human's is `Chaotic`.
    pub fn personality(&self) -> Personality {
        self.component::<Personality>()
            .map_or_else(Default::default, |p| *p)
    }

    /// What the wizard is after (none for the human).
    pub fn objective(&self) -> Option<Objective> {
        self.component::<Objective>().map(|o| *o)
    }
}

impl Game {
    /// The unit definitions of a wizard's heroes (a hero's own name is not kept yet: its class
    /// stands for it).
    pub fn wizard_heroes(&self, wizard: PlayerId) -> Vec<UnitId> {
        let units = self.units();
        units
            .player_units(wizard)
            .into_iter()
            .filter_map(|u| units.unit_def(u))
            .filter(|d| d.kind == crate::rules::UnitKind::Hero)
            .map(|d| UnitId::new(d.num))
            .collect()
    }
}
