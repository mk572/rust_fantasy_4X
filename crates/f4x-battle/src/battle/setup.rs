//! Setting a battle up from what the world tells it: where one side's units stand on the field
//! (front and back squares, a ship at the bow of its deck with the units it carries on the rest
//! of the deck), and the settings of the game that change the rules of the fight.
//!
//! The world builds the units, picks who fights and works out which ship carries whom and where
//! the decks lie (`f4x-core`'s `battle/world.rs`); the squares are decided here.
//!
//! Tests: in f4x-core's battle/tests.rs, on real sea and land battles.

use f4x_def::rules::stats::{ModOp, Modifier, Source, Stat};
use f4x_def::settings::RuleSettings;

use super::Battle;
use crate::map::{deployment_order, Pos};
use crate::unit::BattleUnit;

/// What decides where one side's units stand. Units are named by their index in the side's list.
pub struct Placement<'a> {
    /// The side's deployment squares, in the order they are filled.
    pub cells: Vec<Pos>,
    /// The ships of the side that got a deck: the ship's index and the deck's squares, the bow
    /// first.
    pub decks: &'a [(usize, Vec<Pos>)],
    /// For each unit, the index of the ship that carries it, if any.
    pub carried_by: &'a [Option<usize>],
    /// Every deck square of both sides: the units that stand on no deck keep off them.
    pub deck_cells: &'a [Pos],
}

/// Put a side's units on their squares and say which of them found one (a side may have more
/// units than squares). Melee units take the front squares, shooters the ones behind them
/// ([`deployment_order`]). A ship stays at the bow of its deck and does not sail; the units it
/// carries stand on the rest of the deck, the middle first.
pub fn place_side(units: &mut [BattleUnit], placement: Placement) -> Vec<bool> {
    let Placement {
        cells,
        decks,
        carried_by,
        deck_cells,
    } = placement;
    let order = deployment_order(units);
    let mut placed = vec![false; units.len()];
    let mut taken: Vec<Pos> = Vec::new();
    for (ship, deck) in decks {
        units[*ship].cell = deck[0];
        units[*ship].sheet_mut().modifiers.push(Modifier::new(
            Stat::Moves,
            ModOp::Set,
            0,
            Source::Other,
        ));
        placed[*ship] = true;
        taken.push(deck[0]);
        let centre = (
            deck.iter().map(|c| c.0).sum::<i32>() / deck.len() as i32,
            deck.iter().map(|c| c.1).sum::<i32>() / deck.len() as i32,
        );
        let mut rest: Vec<Pos> = deck[1..].to_vec();
        rest.sort_by_key(|c| (c.0 - centre.0).abs() + (c.1 - centre.1).abs());
        let mut rest = rest.into_iter();
        for (i, carrier) in carried_by.iter().enumerate() {
            if *carrier == Some(*ship) {
                if let Some(cell) = rest.next() {
                    units[i].cell = cell;
                    placed[i] = true;
                    taken.push(cell);
                }
            }
        }
    }
    let mut free = cells
        .into_iter()
        .filter(|c| !taken.contains(c) && !deck_cells.contains(c));
    for &i in &order {
        if !placed[i] {
            if let Some(cell) = free.next() {
                units[i].cell = cell;
                placed[i] = true;
            }
        }
    }
    placed
}

impl Battle {
    /// Take the game's settings that change the rules of a fight: melee against fliers, whether
    /// walls cover the city, the immunity exceptions of version 1.50 and the most units a side
    /// fights with.
    pub fn apply_settings(&mut self, rules: &RuleSettings) {
        self.melee_vs_fliers = rules.melee_vs_fliers;
        self.walls_cover_city = rules.walls_cover_city;
        self.death_immunity_exceptions = rules.death_immunity_exceptions;
        self.magic_immunity_exceptions = rules.magic_immunity_exceptions;
        self.max_side = rules.max_battle_side;
    }
}
