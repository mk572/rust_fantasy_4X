//! Why a spell cannot be cast in a battle.
//!
//! Tests: none, because it only names the refusal reasons.

use f4x_def::rules::target_error::TargetError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellError {
    /// `Battle::apply` cannot cast: go through the game.
    NeedsGame,
    /// This side has no wizard.
    NoWizard,
    /// The wizard has cast this turn already.
    AlreadyCast,
    NotKnown,
    /// A spell for the map, not the battlefield.
    NotForBattle,
    /// Its effects are not in the game yet.
    Unsupported,
    /// More than the casting skill left, or the reserve, will pay.
    TooExpensive,
    /// Less than the spell costs was put in.
    TooLittle,
    /// Not a unit or square the spell can be cast on, and why.
    Target(TargetError),
    /// No square for a creature, or the side has as many as it can control.
    NoRoom,
    /// The side has this battlefield spell up already.
    AlreadyUp,
    /// The unit has this lasting spell on it already.
    AlreadyEnchanted,
    /// The spell is not cumulative with one the unit has (or the wizard keeps).
    NotCumulative,
    /// The unit has the ability the spell would give it.
    AlreadyAble,
    /// A Spell Ward on the city guards it from the spell's realm, and the caster is attacking it.
    Warded,
}

impl SpellError {
    /// Refused only because the spell would do nothing: the caster may confirm and cast anyway.
    pub fn is_pointless(self) -> bool {
        matches!(self, Self::AlreadyEnchanted | Self::AlreadyAble)
    }
}

impl SpellError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NeedsGame => "refuse_battle.spell_error_needs_game",
            Self::NoWizard => "refuse_battle.spell_error_no_wizard",
            Self::AlreadyCast => "refuse_battle.spell_error_already_cast",
            Self::NotKnown => "refuse_battle.spell_error_not_known",
            Self::NotForBattle => "refuse_battle.spell_error_not_for_battle",
            Self::Unsupported => "refuse_battle.spell_error_unsupported",
            Self::TooExpensive => "refuse_battle.spell_error_too_expensive",
            Self::TooLittle => "refuse_battle.spell_error_too_little",
            Self::Target(e) => e.key(),
            Self::NoRoom => "refuse_battle.spell_error_no_room",
            Self::AlreadyUp => "refuse_battle.spell_error_already_up",
            Self::AlreadyEnchanted => "refuse_battle.spell_error_already_enchanted",
            Self::NotCumulative => "refuse_battle.spell_error_not_cumulative",
            Self::AlreadyAble => "refuse_battle.spell_error_already_able",
            Self::Warded => "refuse_battle.spell_error_warded",
        }
    }
}
