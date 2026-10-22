//! The wizard's side of the AI: how power is split, what is researched and what is cast
//! (`docs/ai-design.md` sections 5.3 and 7).
//!
//! The original weighs eleven kinds of overland spell and picks one at random; the picks inside
//! each kind are left unfinished in the reference source. Here every choice is a function of
//! what the AI sees, so a game replays exactly. The order of wants each turn: the Spell of
//! Mastery when it is known, a summoning spell, a buff for the strongest units, a buff for a
//! city, a lasting spell on the whole wizard.

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::effects::Effect;
use f4x_core::rules::exec::Target;
use f4x_core::rules::{CastContext, Duration, SpellDef, SpellId, SpellTarget};

use crate::view::{is_fighter, unit_strength};
use crate::Ai;

/// The kinds of overland spell the AI knows what to do with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Finishing it wins the game.
    Mastery,
    Summon,
    /// A lasting spell that makes one of the wizard's units better.
    UnitBuff,
    /// A lasting spell that makes one of the wizard's cities better.
    CityBuff,
    /// A lasting spell on the wizard that helps it.
    GlobalBuff,
    /// Earth Lore: explores a block of the map around the target square.
    Explore,
    /// Nature's Cures: heals the whole stack on the target unit's square.
    Cure,
    /// Everything else: not cast by the AI yet.
    Other,
}

/// Global enchantments that only help their caster.
const GOOD_GLOBALS: &[&str] = &[
    "herb_mastery",
    "holy_arms",
    "crusade",
    "charm_of_life",
    "awareness",
];

/// Unexplored squares a block must hold to be worth Earth Lore.
const EXPLORE_FLOOR: u32 = 70;
/// Share of a stack's hit points (percent) that must be gone to be worth Nature's Cures.
const CURE_PERCENT: u32 = 35;
/// Mana kept in the reserve before the AI starts a global enchantment (the original's gate).
const GLOBAL_RESERVE: i32 = 200;

fn has_hook(def: &SpellDef, name: &str) -> bool {
    def.effects
        .iter()
        .any(|r| matches!(&r.effect, Effect::Custom(h) if h == name))
}

/// What the AI makes of a spell.
pub fn kind(def: &SpellDef) -> Kind {
    if !def.contexts.contains(&CastContext::Overland) {
        return Kind::Other;
    }
    if has_hook(def, f4x_core::game::endgame::SPELL_OF_MASTERY) {
        return Kind::Mastery;
    }
    if def
        .effects
        .iter()
        .any(|r| matches!(r.effect, Effect::Summon { .. }))
    {
        return Kind::Summon;
    }
    if has_hook(def, f4x_core::spell::special::EARTH_LORE) {
        return Kind::Explore;
    }
    if has_hook(def, f4x_core::spell::special::NATURES_CURES) {
        return Kind::Cure;
    }
    if def.effects.is_empty() || has_hook(def, "unimplemented") {
        return Kind::Other;
    }
    let helps = |r: &f4x_core::rules::effects::EffectRule| match &r.effect {
        Effect::Modify { value, .. } => *value >= 0,
        Effect::GrantAbility(_) => true,
        Effect::Custom(h) => h == "city_flag" || h == "wall_of_stone",
        _ => false,
    };
    match def.target {
        SpellTarget::OwnUnit
            if def.duration != Duration::Instant
                && def
                    .effects
                    .iter()
                    .all(|r| !matches!(r.effect, Effect::Custom(_)))
                && def.effects.iter().all(helps) =>
        {
            Kind::UnitBuff
        }
        SpellTarget::OwnCity
            if def.duration == Duration::Permanent && def.effects.iter().all(helps) =>
        {
            Kind::CityBuff
        }
        SpellTarget::None if GOOD_GLOBALS.contains(&def.id.as_str()) => Kind::GlobalBuff,
        _ => Kind::Other,
    }
}

/// The order in which kinds are tried: the win first, then what pays at once (seeing the
/// world, healing a stack), then the army, then the buffs.
fn priority(k: Kind) -> u8 {
    match k {
        Kind::Mastery => 0,
        Kind::Explore => 1,
        Kind::Cure => 2,
        Kind::Summon => 3,
        Kind::UnitBuff => 4,
        Kind::CityBuff => 5,
        Kind::GlobalBuff => 6,
        Kind::Other => 7,
    }
}

/// Everything the wizard does with its magic in a turn.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    resolve_paid(ai, game, events);
    split(ai, game, events);
    research(ai, game, events);
    cast(ai, game, events);
}

// ---- the power split ------------------------------------------------------------

/// Shares for (mana, research, skill) tried in order; the first that keeps the mana income
/// ahead of the upkeep is taken. Skill stays at a fifth so that spells can still be cast at
/// once.
const SPLITS: [(u8, u8, u8); 6] = [
    (10, 70, 20),
    (25, 55, 20),
    (40, 40, 20),
    (55, 25, 20),
    (70, 10, 20),
    (80, 0, 20),
];
/// The split while the Spell of Mastery is being cast: mana pays for it and skill sets how much
/// can be paid a turn.
const MASTERY_SPLIT: (u8, u8, u8) = (50, 0, 50);

fn split(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let casting_mastery = {
        let view = PlayerView::new(game, me);
        let Some(m) = view.magic() else {
            return;
        };
        m.casting
            .is_some_and(|c| kind(view.spell(c.spell)) == Kind::Mastery)
    };
    if casting_mastery {
        ai.send(game, set_split(me, MASTERY_SPLIT), events);
        return;
    }
    let mut chosen = SPLITS[SPLITS.len() - 1];
    for candidate in SPLITS {
        game.apply(set_split(me, candidate));
        let view = PlayerView::new(game, me);
        let income = view.income();
        // Mana for the upkeep and a little to spare; with a big reserve less is needed.
        let reserve = view.treasury().mana;
        let margin = if reserve > 400 { 0 } else { 2 };
        if income.split.mana - income.mana_upkeep >= margin {
            chosen = candidate;
            break;
        }
    }
    ai.send(game, set_split(me, chosen), events);
}

fn set_split(player: f4x_core::player::PlayerId, (mana, research, skill): (u8, u8, u8)) -> Command {
    Command::SetPowerSplit {
        player,
        mana,
        research,
        skill,
    }
}

// ---- research -------------------------------------------------------------------

/// A spell's kind of use, for telling whether the AI already has one of its sort.
fn category(def: &SpellDef) -> (Kind, bool) {
    let battle_harm = def.contexts.contains(&CastContext::Battle)
        && def
            .effects
            .iter()
            .any(|r| matches!(r.effect, Effect::Damage { .. } | Effect::Slay { .. }));
    (kind(def), battle_harm)
}

/// Turns of research a candidate costs, with the original's adjustments (cheaper when the AI has
/// no spell of its sort yet and the sort wins battles or fights, dearer when it has one).
fn research_score(
    view: &PlayerView,
    spell: SpellId,
    have: &[(Kind, bool)],
    income: i32,
    racing: bool,
) -> f32 {
    let def = view.spell(spell);
    let turns = view.research_cost(spell) as f32 / income.max(1) as f32;
    let cat = category(def);
    let owned = have.contains(&cat);
    let mut score = turns;
    if cat.1 || cat.0 == Kind::Summon {
        score *= if owned { 1.0 } else { 0.25 };
    } else if cat.0 == Kind::UnitBuff || cat.0 == Kind::CityBuff {
        score *= if owned { 1.5 } else { 0.7 };
    } else if owned {
        score *= 1.5;
    }
    // The Spell of Mastery takes thousands of points: the AI goes for it only when it is within
    // reach, and otherwise takes it last.
    // (Unless another wizard is casting it and the AI can still beat them to it.)
    if cat.0 == Kind::Mastery && turns > 250.0 && !racing {
        score = score * 100.0 + 1.0e6;
    }
    score
}

fn research(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let spell = {
        let view = PlayerView::new(game, ai.player);
        let Some(m) = view.magic() else {
            return;
        };
        if m.researching.is_some() {
            return;
        }
        let have: Vec<(Kind, bool)> = m.known.iter().map(|&s| category(view.spell(s))).collect();
        let income = view.income().research;
        view.research_choices().into_iter().min_by(|&a, &b| {
            research_score(&view, a, &have, income, ai.racing)
                .total_cmp(&research_score(&view, b, &have, income, ai.racing))
                .then(a.cmp(&b))
        })
    };
    if let Some(spell) = spell {
        ai.send(
            game,
            Command::SetResearch {
                player: ai.player,
                spell,
            },
            events,
        );
    }
}

// ---- casting --------------------------------------------------------------------

/// A spell that has been paid for over turns and waits for its target.
fn resolve_paid(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let target = {
        let view = PlayerView::new(game, ai.player);
        let Some(spell) = view.paid_spell() else {
            return;
        };
        if !view.spell_needs_target(spell) {
            return;
        }
        pick_target(&view, spell)
    };
    if let Some(target) = target {
        ai.send(
            game,
            Command::ResolveCast {
                player: ai.player,
                target,
            },
            events,
        );
    }
}

/// The best aim for one of the spells the AI casts, if there is one the game accepts.
fn pick_target(view: &PlayerView, spell: SpellId) -> Option<Target> {
    let def = view.spell(spell);
    match kind(def) {
        Kind::UnitBuff => {
            let mut units: Vec<hecs::Entity> = view
                .my_units()
                .into_iter()
                .filter(|&u| is_fighter(view, u) && !view.unit_enchantments(u).contains(&spell))
                .collect();
            view.sort_by_serial(&mut units);
            units
                .into_iter()
                .map(|u| (unit_strength(view, u), u))
                .filter(|&(_, u)| view.can_aim(spell, Some(Target::Unit(u))))
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, u)| Target::Unit(u))
        }
        Kind::CityBuff => {
            let fortress = view.player().and_then(|p| p.fortress);
            let mut cities = view.my_cities();
            cities.sort_by_key(|&(c, p)| (Some(p) != fortress, view.serial_of(c)));
            cities
                .into_iter()
                .filter(|&(c, _)| {
                    !view.city_enchantments(c).contains(&spell)
                        && view.city_economy(c).is_some_and(|e| e.figures > 0)
                })
                .map(|(c, _)| Target::City(c))
                .find(|&t| view.can_aim(spell, Some(t)))
        }
        Kind::Explore => explore_target(view)
            .map(Target::Tile)
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::Cure => hurt_stack_unit(view)
            .map(Target::Unit)
            .filter(|&t| view.can_aim(spell, Some(t))),
        _ => None,
    }
}

/// The square to aim Earth Lore at: the centre of the block of the map with the most ground the
/// wizard has not explored, if that is a good deal.
fn explore_target(view: &PlayerView) -> Option<Position> {
    let anchor = view.my_cities().first().map(|&(_, p)| p)?;
    let mut best: Option<(u32, Position)> = None;
    for dy in (-18i32..=18).step_by(4) {
        for dx in (-30i32..=30).step_by(4) {
            let Some(centre) = view.shift(anchor, dx, dy) else {
                continue;
            };
            let mut fresh = 0;
            for by in -5i32..5 {
                for bx in -6i32..6 {
                    if let Some(p) = view.shift(centre, bx, by) {
                        if !view.explored(p) {
                            fresh += 1;
                        }
                    }
                }
            }
            if best
                .is_none_or(|(n, b)| fresh > n || (fresh == n && (centre.y, centre.x) < (b.y, b.x)))
            {
                best = Some((fresh, centre));
            }
        }
    }
    best.filter(|&(n, _)| n >= EXPLORE_FLOOR).map(|(_, p)| p)
}

/// One unit of the most hurt stack in a city, if a good part of the stack is hurt.
fn hurt_stack_unit(view: &PlayerView) -> Option<hecs::Entity> {
    let mut best: Option<(u32, hecs::Entity)> = None;
    for (_, pos) in view.my_cities() {
        let stack = view.garrison(pos);
        let (mut missing, mut total) = (0u32, 0u32);
        for &u in &stack {
            let (hits, max) = view.unit_hits(u);
            missing += max.saturating_sub(hits);
            total += max;
        }
        if total == 0 || missing * 100 / total < CURE_PERCENT {
            continue;
        }
        if best.is_none_or(|(m, _)| missing > m) {
            best = Some((missing, stack[0]));
        }
    }
    best.map(|(_, u)| u)
}

/// Whether the AI can carry the lasting spell's upkeep on its mana income.
fn upkeep_ok(view: &PlayerView, def: &SpellDef) -> bool {
    let income = view.income();
    income.split.mana - income.mana_upkeep - def.upkeep as i32 >= 0
}

fn cast(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let choice: Option<(SpellId, Option<Target>)> = {
        let view = PlayerView::new(game, me);
        let Some(magic) = view.magic() else {
            return;
        };
        if magic.casting.is_some() {
            return;
        }
        let mana = view.treasury().mana;
        let income = view.income();
        let fighters = view
            .my_units()
            .into_iter()
            .filter(|&u| is_fighter(&view, u))
            .count();
        // What each known spell is, dearest first within a kind so the strongest is tried first.
        let mut options: Vec<(Kind, i32, SpellId)> = magic
            .known
            .iter()
            .copied()
            .map(|s| (kind(view.spell(s)), view.casting_cost(s), s))
            .filter(|&(k, cost, _)| k != Kind::Other && cost > 0)
            .collect();
        options.sort_by_key(|&(k, cost, s)| (priority(k), std::cmp::Reverse(cost), s));
        let mut found = None;
        for (k, cost, spell) in options {
            let def = view.spell(spell);
            // Paid for from the reserve, over as many turns as the casting skill needs, with
            // a third of the mana left over for the upkeep. The Spell of Mastery goes on until it
            // is paid whatever the reserve.
            let now = cost * 3 / 2 <= mana;
            // Summoned creatures and lasting spells cost mana every turn: not when the upkeep
            // already takes more than half of what the power makes (the split then leaves research
            // starved).
            let room = magic.mana_pct <= 55;
            let target = pick_target(&view, spell);
            let wanted = match k {
                Kind::Mastery => true,
                // Summoned creatures cost upkeep mana: only while the army is thin.
                Kind::Summon => now && room && fighters < 12 + 2 * view.my_cities().len(),
                Kind::UnitBuff => {
                    now && room && fighters >= 3 && upkeep_ok(&view, def) && target.is_some()
                }
                Kind::CityBuff => now && room && upkeep_ok(&view, def) && target.is_some(),
                Kind::GlobalBuff => {
                    now && room
                        && mana - cost >= GLOBAL_RESERVE
                        && upkeep_ok(&view, def)
                        && !magic.global.contains(&spell)
                }
                Kind::Explore | Kind::Cure => now && target.is_some(),
                Kind::Other => false,
            };
            // The original's check: not more than fifty turns of what the AI can put towards it.
            let per_turn = mana / 10 + income.split.mana;
            if wanted && (k == Kind::Mastery || cost / 50 <= per_turn) {
                found = Some((spell, target));
                break;
            }
        }
        found
    };
    if let Some((spell, target)) = choice {
        ai.send(
            game,
            Command::CastSpell {
                player: me,
                spell,
                target,
            },
            events,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_of(id: &str) -> Kind {
        let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
        let spell = content
            .rules
            .spells
            .id_of(id)
            .unwrap_or_else(|| panic!("no spell {id}"));
        kind(content.rules.spells.get(spell))
    }

    #[test]
    fn spells_are_sorted_into_the_kinds_the_ai_casts() {
        assert_eq!(kind_of("spell_of_mastery"), Kind::Mastery);
        assert_eq!(kind_of("war_bears"), Kind::Summon);
        assert_eq!(kind_of("giant_strength"), Kind::UnitBuff);
        assert_eq!(kind_of("natures_eye"), Kind::CityBuff);
        assert_eq!(kind_of("herb_mastery"), Kind::GlobalBuff);
        assert_eq!(kind_of("earth_lore"), Kind::Explore);
        assert_eq!(kind_of("natures_cures"), Kind::Cure);
    }

    #[test]
    fn what_the_ai_cannot_use_is_left_alone() {
        // A curse on the whole world, a bolt for battle, a spell that is not done yet.
        assert_eq!(kind_of("fire_bolt"), Kind::Other);
        assert_eq!(kind_of("suppress_magic"), Kind::Other);
        assert_eq!(kind_of("earth_to_mud"), Kind::Other);
    }
}
