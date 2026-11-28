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
use crate::map::{deployment_order, layout, Pos};
use crate::unit::BattleUnit;
use f4x_def::rules::DeploymentRules;

/// What decides where one side's units stand. Units are named by their index in the side's list.
pub struct Placement<'a> {
    /// The side's deployment squares, in the order they are filled.
    pub cells: Vec<Pos>,
    /// The ships of the side that got a deck: the ship's index and the deck's squares, the bow
    /// first.
    pub decks: &'a [(usize, Vec<Pos>)],
    /// For each unit, the index of the ship that carries it, if any.
    pub carried_by: &'a [Option<usize>],
    /// Every square the ships of both sides cover, the decks and the water at their narrow
    /// ends (`hull_cells`): the units that stand on no deck keep off them, and off the squares
    /// next to them while there are others.
    pub hull_cells: &'a [Pos],
    /// The squares the units placed before these stand on.
    pub occupied: &'a [Pos],
    /// The deployment of `battle.ron`.
    pub rules: &'a DeploymentRules,
}

/// Put a side's units on their squares and say which of them found one (a side may have more
/// units than squares). Melee units take the front squares, shooters the ones behind them
/// ([`deployment_order`]). A ship stays at the bow of its deck and does not sail; the units it
/// carries stand on the rest of the deck, the middle first, and those of a ship that has no
/// deck on another deck of the side. In a fight with decks, a unit that stands on none keeps
/// clear of the ships, on the side's squares or else on the free square nearest to them, and
/// takes a square beside a ship only when no clear one is left.
pub fn place_side(units: &mut [BattleUnit], placement: Placement) -> Vec<bool> {
    let Placement {
        cells,
        decks,
        carried_by,
        hull_cells,
        occupied,
        rules,
    } = placement;
    let order = deployment_order(units, rules);
    let mut placed = vec![false; units.len()];
    let mut taken: Vec<Pos> = occupied.to_vec();
    // The free squares of the side's decks, ship by ship.
    let mut room: Vec<Pos> = Vec::new();
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
        room.extend(rest);
    }
    let mut room = room.into_iter();
    for (i, carrier) in carried_by.iter().enumerate() {
        if carrier.is_some() && !placed[i] {
            if let Some(cell) = room.next() {
                units[i].cell = cell;
                placed[i] = true;
                taken.push(cell);
            }
        }
    }
    let spare = if hull_cells.is_empty() {
        Vec::new()
    } else {
        nearest_first(&cells)
    };
    // A hull's picture reaches over the water beside it too.
    let beside = |c: &Pos| {
        hull_cells
            .iter()
            .any(|h| (h.0 - c.0).abs().max((h.1 - c.1).abs()) <= 1)
    };
    for &i in &order {
        if !placed[i] {
            let open = || {
                cells
                    .iter()
                    .chain(&spare)
                    .copied()
                    .filter(|c| !taken.contains(c) && !hull_cells.contains(c))
            };
            let free = open().find(|c| !beside(c)).or_else(|| open().next());
            if let Some(cell) = free {
                units[i].cell = cell;
                placed[i] = true;
                taken.push(cell);
            }
        }
    }
    placed
}

/// The squares a ship with this deck covers on the screen: the rectangle of the deck's width
/// and length, as its hull is drawn over the water at the narrow ends too.
pub fn hull_cells(deck: &[Pos]) -> Vec<Pos> {
    let span = |of: fn(&Pos) -> i32| {
        let low = deck.iter().map(of).min().unwrap_or(0);
        low..deck.iter().map(of).max().map_or(low, |high| high + 1)
    };
    let columns = span(|c| c.0);
    span(|c| c.1)
        .flat_map(|y| columns.clone().map(move |x| (x, y)))
        .collect()
}

/// The squares of the field, those nearest to any of `cells` first.
fn nearest_first(cells: &[Pos]) -> Vec<Pos> {
    let mut all: Vec<Pos> = (0..layout::H)
        .flat_map(|y| (0..layout::W).map(move |x| (x, y)))
        .filter(|&(x, y)| layout::in_band(layout::W - 1 - x, y))
        .collect();
    all.sort_by_key(|&(x, y)| {
        let away = cells
            .iter()
            .map(|c| (c.0 - x).abs().max((c.1 - y).abs()))
            .min()
            .unwrap_or(0);
        (away, y, x)
    });
    all
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
