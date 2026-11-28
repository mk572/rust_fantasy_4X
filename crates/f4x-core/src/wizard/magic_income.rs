//! What a wizard earns and owes each turn, worked out over a small read-only view of the world
//! ([`IncomeView`]): the power base (cities, fortress, volcanoes, blighted squares, nodes), the research the cities
//! make, the split of the power, the research income and the mana upkeep. `World`
//! gives the view through [`WorldIncome`]; tests give a fake one, so none of this needs a world.
//!
//! Tests: in wizard/magic_income/tests.rs.

use crate::map::Plane;
use crate::player::PlayerId;
use crate::wizard::entity::Wizard;
use crate::wizard::magic::PowerShares;
use crate::world::World;
use f4x_def::content::Content;
use f4x_def::half_int::HalfInt;

/// What the income rules ask of the world, for one wizard at a time.
pub(crate) trait IncomeView {
    fn wizard(&self, player: PlayerId) -> Option<Wizard<'_>>;
    fn content(&self) -> &Content;
    /// Whether the wizard is casting the Spell of Return (then there is no income of any kind).
    fn casting_return(&self, player: PlayerId) -> bool;
    /// The mana and the research points the wizard's cities make (and its Sage heroes).
    fn city_yield(&self, player: PlayerId) -> (HalfInt, i32);
    /// The plane of the wizard's fortress city, if it has one.
    fn fortress_plane(&self, player: PlayerId) -> Option<Plane>;
    /// Power from the volcanoes the wizard owns.
    fn volcanoes(&self, player: PlayerId) -> i32;
    /// Power from the nodes the wizard owns.
    fn node_income(&self, player: PlayerId) -> i32;
    /// Power from the corrupted squares round the cities of a wizard who keeps Great Wasting.
    fn blight_power(&self, _player: PlayerId) -> i32 {
        0
    }
    /// Whether a Mana Short has dried up every source of power.
    fn mana_is_short(&self) -> bool {
        false
    }
    /// Mana the lasting spells and summoned creatures cost per turn, before a Channeler's half.
    fn upkeep_total(&self, player: PlayerId) -> i32;
    /// The wizards this one has a research pact with.
    fn research_partners(&self, _player: PlayerId) -> Vec<PlayerId> {
        Vec::new()
    }
    /// A computer wizard's difficulty bonus on the power of its nodes, in percent.
    fn node_percent(&self, _player: PlayerId) -> i32 {
        100
    }
}

/// A wizard's income and upkeep, as the screens and the AI read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Income {
    /// Power before the split.
    pub power_base: HalfInt,
    /// Research points the cities produce.
    pub city_research: i32,
    /// The power base split by the wizard's percentages.
    pub split: PowerShares,
    /// Research points paid at the next turn change, with bonuses and what research pacts add.
    pub research: i32,
    /// Mana per turn to keep lasting spells and summoned creatures going.
    pub mana_upkeep: HalfInt,
}

/// Mana from the wizard's cities, plus the fortress (a point per book, five more on Myrror: `myrror_fortress_power`),
/// the volcanoes and the nodes.
fn base_from(view: &impl IncomeView, player: PlayerId, city_mana: HalfInt) -> HalfInt {
    let fortress = match view.fortress_plane(player) {
        Some(plane) => {
            let books = view.wizard(player).map_or(0, |m| m.total_books());
            let myrror = view.content().settings.myrror_fortress_power;
            books + if plane == Plane::Myrror { myrror } else { 0 }
        }
        None => 0,
    };
    let nodes = crate::player::Bonus::of(view.node_percent(player), view.node_income(player));
    (city_mana
        + HalfInt::from_int(fortress + view.volcanoes(player) + view.blight_power(player) + nodes))
    .max(HalfInt::ZERO)
}

/// What the wizard pays in mana each turn, without the rest of the income.
fn mana_upkeep(view: &impl IncomeView, player: PlayerId) -> HalfInt {
    let percent = view
        .wizard(player)
        .map_or(100, |m| m.upkeep_percent(&view.content().rules));
    HalfInt::from_raw(view.upkeep_total(player).saturating_mul(2) * percent / 100)
}

/// What the wizard's power comes to, split, and what it researches by itself.
fn earnings(view: &impl IncomeView, player: PlayerId) -> Income {
    let (city_mana, city_research) = view.city_yield(player);
    let casting_return = view.casting_return(player);
    let power_base = if casting_return || view.mana_is_short() {
        HalfInt::ZERO
    } else {
        base_from(view, player, city_mana)
    };
    let split = match view.wizard(player) {
        Some(m) if !casting_return => {
            m.split_power(&view.content().rules, power_base, city_research)
        }
        // A player without a wizard (a neutral race, task 522) has no research and no skill:
        // all its power is mana.
        None if !casting_return => PowerShares {
            mana: power_base,
            ..PowerShares::default()
        },
        _ => PowerShares::default(),
    };
    let research = view.wizard(player).map_or(split.research.max(0), |m| {
        m.research_income(view.content(), &split)
    });
    Income {
        power_base,
        city_research,
        split,
        research,
        mana_upkeep: HalfInt::ZERO,
    }
}

/// What the research pact with `partner` adds to the research of `player`, who researches `own`
/// by itself: a percent of what the two research together, more for each book of a realm both hold.
fn pact_research(view: &impl IncomeView, player: PlayerId, partner: PlayerId, own: i32) -> i32 {
    let (Some(a), Some(b)) = (view.wizard(player), view.wizard(partner)) else {
        return 0;
    };
    let shared: i32 = crate::rules::BOOK_REALMS
        .iter()
        .map(|&realm| a.books_in(realm).min(b.books_in(realm)))
        .sum();
    let rules = &view.content().rules.config.diplomacy;
    let percent = rules.research_pact_percent + rules.research_pact_book_percent * shared;
    (own + earnings(view, partner).research).max(0) * percent / 100
}

/// Everything the wizard earns (zero power while the Spell of Return is cast or mana is short) and owes, with the cities asked once.
pub(crate) fn income(view: &impl IncomeView, player: PlayerId) -> Income {
    let mut income = earnings(view, player);
    if !view.casting_return(player) {
        let own = income.research;
        income.research += view
            .research_partners(player)
            .into_iter()
            .map(|partner| pact_research(view, player, partner, own))
            .sum::<i32>();
    }
    income.mana_upkeep = mana_upkeep(view, player);
    income
}

/// [`IncomeView`] over a whole game.
pub(crate) struct WorldIncome<'a>(pub &'a World);

impl IncomeView for WorldIncome<'_> {
    fn node_percent(&self, player: PlayerId) -> i32 {
        crate::player::component::<crate::player::Bonus>(&self.0.ecs, player)
            .map_or(100, |b| b.mana)
    }

    fn research_partners(&self, player: PlayerId) -> Vec<PlayerId> {
        if !self.0.settings.rules.expanded_diplomacy {
            return Vec::new();
        }
        self.0.diplomacy.research_partners(player).collect()
    }

    fn wizard(&self, player: PlayerId) -> Option<Wizard<'_>> {
        self.0.wizard(player)
    }

    fn content(&self) -> &Content {
        &self.0.content
    }

    fn casting_return(&self, player: PlayerId) -> bool {
        self.0.casting_return(player)
    }

    fn city_yield(&self, player: PlayerId) -> (HalfInt, i32) {
        // Sage heroes add to the research share, as the cities' points do (`CITYCALC.c`).
        let (mana, research) = self
            .0
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .filter_map(|(e, ..)| self.0.economy().city_economy(e))
            .fold((HalfInt::ZERO, 0), |(mana, research), eco| {
                (mana + eco.mana, research + eco.research)
            });
        (mana, research + self.0.units().sage_research(player))
    }

    fn fortress_plane(&self, player: PlayerId) -> Option<Plane> {
        let city = self.0.economy().fortress_city(player)?;
        self.0
            .ecs
            .get::<&crate::map::Position>(city)
            .ok()
            .map(|p| p.plane)
    }

    fn volcanoes(&self, player: PlayerId) -> i32 {
        self.0.volcano_count(player)
    }

    fn node_income(&self, player: PlayerId) -> i32 {
        self.0.node_income(player)
    }

    fn blight_power(&self, player: PlayerId) -> i32 {
        self.0.economy().blight_power(player)
    }

    fn mana_is_short(&self) -> bool {
        self.0.events.mana_is_short()
    }

    fn upkeep_total(&self, player: PlayerId) -> i32 {
        self.0.upkeep_items(player).iter().map(|i| i.mana).sum()
    }
}

impl World {
    /// What a wizard owes in mana each turn: `income(player).mana_upkeep`, without working out
    /// the income (the battle AI asks at every command).
    pub fn mana_upkeep(&self, player: PlayerId) -> HalfInt {
        mana_upkeep(&WorldIncome(self), player)
    }

    /// What a wizard earns and owes (see [`Income`]).
    pub fn income(&self, player: PlayerId) -> Income {
        income(&WorldIncome(self), player)
    }
}
