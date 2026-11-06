//! The magic commands that need no world, as functions: alchemy and giving up the
//! spell being cast. `Game` finds the wizard and passes what it alone knows (whether the Spell of
//! Return is cast).
//!
//! Tests: in wizard/magic_command/tests.rs.

use crate::player::Treasury;
use crate::wizard::magic::MagicError;
use crate::wizard::sheet::WizardMut;

/// Convert gold to mana or mana to gold (alchemy): `percent` of the amount comes back (half
/// normally, all of it with the retort; `Stat::AlchemyPercent`). `gold_to_mana` converts that much
/// gold; otherwise that much mana becomes gold.
pub(crate) fn alchemy(
    percent: i32,
    treasury: &mut Treasury,
    amount: i32,
    gold_to_mana: bool,
) -> Result<(), MagicError> {
    let amount = amount.max(0);
    if gold_to_mana {
        if treasury.gold < amount {
            return Err(MagicError::NotEnoughGold);
        }
        treasury.gold -= amount;
        treasury.mana += amount * percent / 100;
    } else {
        if treasury.mana < amount {
            return Err(MagicError::NotEnoughMana);
        }
        treasury.mana -= amount;
        treasury.gold += amount * percent / 100;
    }
    Ok(())
}

/// Give up the spell being cast; the mana already paid is lost. Not while the Spell of Return
/// is being cast.
pub(crate) fn cancel_cast(wizard: &mut WizardMut, casting_return: bool) -> Result<(), MagicError> {
    if casting_return {
        return Err(MagicError::NotAvailable);
    }
    wizard.cancel_casting();
    Ok(())
}

#[cfg(test)]
mod tests;
