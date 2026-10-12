//! The magic commands that need no world, as functions over the player (task 197): setting a
//! wizard up, alchemy and giving up the spell being cast. `Game` finds the player and passes
//! what it alone knows (the content, the generator, whether the Spell of Return is cast).

use crate::content::Content;
use crate::magic::{Magic, MagicError, Retort};
use crate::player::Player;
use crate::rng::GameRng;
use crate::rules::SpellId;

/// Set a new wizard up: books and retorts, starting casting skill, a fame of 10 for a Famous
/// wizard, and the spell library (what is known and what can be researched) from the number of
/// books, with the starting spells given or drawn at random. The first offer for research is
/// made by the game once it can work out the income.
pub(crate) fn found_wizard(
    player: &mut Player,
    content: &Content,
    rng: &mut GameRng,
    books: &[(&str, u8)],
    retorts: &[Retort],
    start_spells: Option<&[SpellId]>,
) {
    let magic = Magic::new(books, retorts);
    player.fame = if magic.has(Retort::Famous) { 10 } else { 0 };
    player.magic = magic;
    player.magic.init_library(content, rng, start_spells);
}

/// Convert gold to mana or mana to gold (alchemy): one for one with the retort, two for one
/// without. `gold_to_mana` converts that much gold; otherwise that much mana becomes gold.
pub(crate) fn alchemy(
    player: &mut Player,
    amount: i32,
    gold_to_mana: bool,
) -> Result<(), MagicError> {
    let skilled = player.magic.has(Retort::Alchemy);
    let amount = amount.max(0);
    if gold_to_mana {
        if player.gold < amount {
            return Err(MagicError::NotEnoughGold);
        }
        player.gold -= amount;
        player.mana += if skilled { amount } else { amount / 2 };
    } else {
        if player.mana < amount {
            return Err(MagicError::NotEnoughMana);
        }
        player.mana -= amount;
        player.gold += if skilled { amount } else { amount / 2 };
    }
    Ok(())
}

/// Give up the spell being cast; the mana already paid is lost. Not while the Spell of Return
/// is being cast.
pub(crate) fn cancel_cast(player: &mut Player, casting_return: bool) -> Result<(), MagicError> {
    if casting_return {
        return Err(MagicError::NotAvailable);
    }
    player.magic.cancel_casting();
    Ok(())
}

#[cfg(test)]
mod tests;
