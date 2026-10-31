//! The wizard's side of the AI: how power is split, what is researched and what is cast
//! (`docs/ai-design.md` sections 5.3 and 7). Every choice is a function of what the AI sees, so a
//! game replays exactly. The order of wants each turn: the Spell of Mastery when it is known, a
//! summoning spell, a buff for the strongest units, a buff for a city, a lasting spell on the
//! whole wizard.

use f4x_core::diplomacy::Treaty;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::effects::Effect;
use f4x_core::rules::exec::Target;
use f4x_core::rules::{CastContext, Duration, SpellDef, SpellId, SpellTarget};

use crate::view::{fighting_strength, is_fighter, unit_strength};
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
    /// A curse or blow on a city of a wizard the AI is at war with.
    CityCurse,
    /// A curse on a wizard the AI is at war with (Drain Power, Subversion, Cruel Unminding).
    WizardCurse,
    /// Disenchant Area and True: takes the curses of other wizards off a square of the map.
    Disenchant,
    /// Disjunction, Disjunction True and Spell Binding: end (or take) the lasting spell on the
    /// world of a wizard at war with the AI.
    Disjunction,
    /// Time Stop, Meteor Storm, Great Wasting and Armageddon: a lasting spell on the world that
    /// is cast while the AI is at war.
    WorldSpell,
    /// Spell Blast: ends the spell a wizard at war is casting, for as much mana as it has spent.
    Blast,
    /// Stasis: holds an enemy stack that stands near a city of the wizard.
    Stasis,
    /// Chaos Channels: gives a fighter a lasting mutation.
    Mutate,
    /// Word of Recall: takes a hurt stack far from home back to the summoning circle.
    Recall,
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

/// Global enchantments that strike the whole world, begun only while the AI is at war with a
/// wizard.
const WORLD_SPELLS: &[&str] = &[
    "time_stop",
    "meteor_storm",
    "great_wasting",
    "armageddon",
    "natures_wrath",
];

/// Nature's Wrath strikes only a wizard who casts a spell of these realms: it is begun only
/// against an enemy with a book in one.
const WRATH_REALMS: &[&str] = &["death", "chaos"];

/// Lasting spells on a unit whose effect the rules read by the spell's id, so their `effects` are
/// empty: Heroism, and the wards against a realm. All of them help the unit they sit on.
const READ_BY_ID_BUFFS: &[&str] = &[
    "heroism",
    "bless",
    "righteousness",
    "resist_magic",
    "resist_elements",
    "elemental_armor",
];

/// Unexplored squares a block must hold to be worth Earth Lore.
const EXPLORE_FLOOR: u32 = 70;
/// Share of a stack's hit points (percent) that must be gone to be worth Nature's Cures.
const CURE_PERCENT: u32 = 35;
/// Spells costing more than this are only begun when the wizard pays for them within
/// `MAX_CASTING_TURNS`: for the cost of a dear spell the wizard gets more from several quick ones.
const DEAR_SPELL: i32 = 300;
/// The most turns the AI spends paying for one dear spell.
const MAX_CASTING_TURNS: i32 = 5;
/// Squares from the summoning circle (or another plane) a stack must be to be worth a recall.
const RECALL_DISTANCE: u32 = 8;
/// Share of a stranded stack's hit points (percent) that must be gone to be worth a recall.
const RECALL_PERCENT: u32 = 50;
/// Squares from a city within which an enemy stack is worth a Stasis.
const STASIS_RADIUS: i32 = 3;
/// Mana kept in the reserve before the AI starts a global enchantment (the original's gate).
const GLOBAL_RESERVE: i32 = 200;

/// Whether a spell on the world has someone to strike: any enemy wizard, and for Nature's
/// Wrath one with a book of a realm it punishes.
fn world_spell_has_victim(view: &PlayerView, def: &SpellDef) -> bool {
    if def.id == "natures_wrath" {
        view.rival_at_war_has_books(WRATH_REALMS)
    } else {
        !view.rivals_at_war().is_empty()
    }
}

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
    if has_hook(def, f4x_core::unit::mutation::CHAOS_CHANNELS) {
        return Kind::Mutate;
    }
    if has_hook(def, f4x_core::spell::special::WORD_OF_RECALL) {
        return Kind::Recall;
    }
    if has_hook(def, f4x_core::unit::stasis::STASIS) {
        return Kind::Stasis;
    }
    if def.effects.is_empty()
        && def.target == SpellTarget::OwnUnit
        && def.duration == Duration::Permanent
        && READ_BY_ID_BUFFS.contains(&def.id.as_str())
    {
        return Kind::UnitBuff;
    }
    if def.effects.is_empty() || has_hook(def, "unimplemented") {
        return Kind::Other;
    }
    if def.target == SpellTarget::GlobalEnchantment {
        return Kind::Disjunction;
    }
    if def.target == SpellTarget::Tile
        && def
            .effects
            .iter()
            .any(|r| matches!(r.effect, Effect::Dispel { .. }))
    {
        return Kind::Disenchant;
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
        SpellTarget::None if WORLD_SPELLS.contains(&def.id.as_str()) => Kind::WorldSpell,
        SpellTarget::EnemyCity => Kind::CityCurse,
        SpellTarget::EnemyWizard if has_hook(def, f4x_core::spell::dispel::SPELL_BLAST) => {
            Kind::Blast
        }
        SpellTarget::EnemyWizard => Kind::WizardCurse,
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
        Kind::CityCurse => 7,
        Kind::WizardCurse => 8,
        // A wizard under a curse wants it off before it blows at anyone.
        Kind::Disenchant => 2,
        Kind::Disjunction => 8,
        Kind::WorldSpell => 8,
        Kind::Blast => 1,
        Kind::Stasis => 2,
        Kind::Mutate => 4,
        Kind::Recall => 2,
        Kind::Other => 9,
    }
}

/// Where a kind of spell stands in the order of casting for a wizard: the ladder of `priority`
/// (four to a step), moved up by what its objective weights (`Traits::cast_boost`). The Spell of
/// Mastery stays first, and nothing else is moved past it.
fn cast_rank(k: Kind, traits: &crate::objective::Traits) -> i32 {
    use crate::objective::Like;
    let like = match k {
        Kind::Summon => Some(Like::Summon),
        Kind::UnitBuff => Some(Like::UnitBuff),
        Kind::CityBuff => Some(Like::CityBuff),
        Kind::GlobalBuff => Some(Like::GlobalBuff),
        Kind::Explore => Some(Like::Explore),
        Kind::CityCurse | Kind::WizardCurse | Kind::Disjunction | Kind::WorldSpell => {
            Some(Like::Curse)
        }
        _ => None,
    };
    let boost = like
        .and_then(|l| traits.cast_boost.iter().find(|&&(b, _)| b == l))
        .map_or(0, |&(_, by)| by);
    let rank = i32::from(priority(k)) * 4 - boost;
    if k == Kind::Mastery {
        rank
    } else {
        rank.max(1)
    }
}

/// Everything the wizard does with its magic in a turn.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    fund_return(ai, game, events);
    stop_aimless(ai, game, events);
    resolve_paid(ai, game, events);
    split(ai, game, events);
    research(ai, game, events);
    cast(ai, game, events);
}

/// A banished wizard pays the Spell of Return from its mana reserve and has no income. When the
/// reserve does not cover this turn's payment it turns as much gold as it needs into mana, and no
/// more (a choice of the script, not a rule: the original does it inside the rules).
fn fund_return(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let gold = {
        let view = PlayerView::new(game, me);
        let Some(w) = view.wizard() else {
            return;
        };
        let Some(casting) = w
            .casting()
            .filter(|c| view.spell(c.spell).id == "spell_of_return")
        else {
            return;
        };
        let due = w.casting_skill().min(casting.remaining).max(0);
        let short = due - view.treasury().mana;
        if !w.is_banished() || short <= 0 {
            return;
        }
        let rate = if w.has(f4x_core::wizard::magic::Retort::Alchemy) {
            1
        } else {
            2
        };
        (short * rate).min(view.treasury().gold)
    };
    if gold > 0 {
        ai.send(
            game,
            Command::Alchemy {
                player: me,
                amount: gold,
                gold_to_mana: true,
            },
            events,
        );
    }
}

/// The game asks a wizard whose spell has nothing to aim at whether to stop casting it; the AI
/// answers yes, as a human would.
fn stop_aimless(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let Some(spell) = game
        .wizard(ai.player)
        .and_then(|m| m.casting())
        .map(|c| c.spell)
    else {
        return;
    };
    if game.spell_needs_target(ai.player, spell) && !game.spell_has_target(ai.player, spell) {
        ai.send(game, Command::CancelCast { player: ai.player }, events);
    }
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
        let Some(m) = view.wizard() else {
            return;
        };
        m.casting()
            .is_some_and(|c| kind(view.spell(c.spell)) == Kind::Mastery)
    };
    if casting_mastery {
        ai.send(game, set_split(me, MASTERY_SPLIT), events);
        return;
    }
    let traits = crate::objective::traits(&PlayerView::new(game, me));
    let mut chosen = SPLITS[SPLITS.len() - 1];
    for candidate in SPLITS.into_iter().skip(traits.split_shift) {
        game.apply(set_split(me, candidate));
        let view = PlayerView::new(game, me);
        let income = view.income();
        // Mana for the upkeep and a little to spare; with a big reserve less is needed (and a
        // wizard that favours research never keeps a spare).
        let reserve = view.treasury().mana;
        let margin = if reserve > 400 || traits.lean_mana {
            0
        } else {
            2
        };
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
    traits: &crate::objective::Traits,
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
    if liked(def, cat.0, traits) {
        score *= traits.like_share;
    }
    score
}

/// Whether the wizard's objective has a liking for this sort of spell.
fn liked(def: &SpellDef, kind: Kind, traits: &crate::objective::Traits) -> bool {
    use crate::objective::Like;
    traits.likes.iter().any(|like| match like {
        Like::Summon => kind == Kind::Summon,
        Like::UnitBuff => kind == Kind::UnitBuff,
        Like::CityBuff => kind == Kind::CityBuff,
        Like::GlobalBuff => kind == Kind::GlobalBuff,
        Like::Explore => kind == Kind::Explore,
        Like::Holy => def.realm == "life" || def.realm == "arcane",
        Like::Curse => matches!(
            kind,
            Kind::CityCurse | Kind::WizardCurse | Kind::Disjunction
        ),
    })
}

fn research(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let spell = {
        let view = PlayerView::new(game, ai.player);
        let Some(m) = view.wizard() else {
            return;
        };
        if m.researching().is_some() {
            return;
        }
        let have: Vec<(Kind, bool)> = m.known().iter().map(|&s| category(view.spell(s))).collect();
        let income = view.income().research;
        let traits = crate::objective::traits(&view);
        view.research_choices().into_iter().min_by(|&a, &b| {
            research_score(&view, a, &have, income, ai.racing, &traits)
                .total_cmp(&research_score(&view, b, &have, income, ai.racing, &traits))
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
    // Resurrection: the last of the dead heroes, chosen now that the cost is paid.
    let paid = PlayerView::new(game, ai.player).paid_spell();
    if paid.is_some_and(|s| game.spell_has_hook(s, f4x_core::spell::hero::RESURRECTION)) {
        if let Some(kind) = game.units().ai_resurrection_pick() {
            let command = Command::ResolveCast {
                player: ai.player,
                target: Target::Hero(kind),
            };
            ai.send(game, command, events);
        }
        return;
    }
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
            let fortress = view.fortress();
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
        Kind::Disenchant => cursed_city(view)
            .map(Target::Tile)
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::Disjunction => view
            .globals_of_enemies()
            .into_iter()
            // The dearest first: a spell that cost the enemy more is worth more to break. Only
            // what the dispel has a fifth of a chance against (strength / (strength + cost)).
            .filter(|&(s, _)| view.spell(s).cost <= dispel_strength(def) * 4)
            .max_by_key(|&(s, i)| (view.spell(s).cost, std::cmp::Reverse(view.serial_of(i))))
            .map(|(_, i)| Target::Spell(i))
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::Mutate => {
            let mut units: Vec<hecs::Entity> = view
                .my_units()
                .into_iter()
                .filter(|&u| {
                    is_fighter(view, u)
                        && !view.unit_has_ability(u, f4x_core::unit::mutation::MUTATED)
                })
                .collect();
            view.sort_by_serial(&mut units);
            units
                .into_iter()
                .map(|u| (unit_strength(view, u), u))
                .filter(|&(_, u)| view.can_aim(spell, Some(Target::Unit(u))))
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, u)| Target::Unit(u))
        }
        Kind::Recall => stranded_unit(view)
            .map(Target::Unit)
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::Cure => hurt_stack_unit(view)
            .map(Target::Unit)
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::Stasis => threat_square(view)
            .map(Target::Tile)
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::CityCurse => curse_city(view, spell).map(Target::City),
        Kind::Blast | Kind::WizardCurse => view
            .rivals_at_war()
            .into_iter()
            .map(Target::Player)
            .find(|&t| view.can_aim(spell, Some(t))),
        _ => None,
    }
}

/// The square of the strongest enemy stack within `STASIS_RADIUS` of an own city that is stronger
/// than what holds the city.
fn threat_square(view: &PlayerView) -> Option<Position> {
    let mut best: Option<(f32, Position)> = None;
    for (_, city) in view.my_cities() {
        let held: Vec<_> = view
            .garrison(city)
            .into_iter()
            .filter(|&u| is_fighter(view, u))
            .collect();
        let held = fighting_strength(view, &held);
        for (square, units) in view.enemies_near(city, STASIS_RADIUS) {
            let strength = fighting_strength(view, &units);
            if strength > held && best.is_none_or(|(b, _)| strength > b) {
                best = Some((strength, square));
            }
        }
    }
    best.map(|(_, square)| square)
}

/// Is a spell of this cost worth beginning for a wizard that pays `power` for it each turn?
fn quick_enough(cost: i32, power: i32) -> bool {
    cost <= DEAR_SPELL || cost <= MAX_CASTING_TURNS * power
}

/// The strength of a dispel cast overland: the base cost, three times that for the "True" spells.
fn dispel_strength(def: &SpellDef) -> u32 {
    if def.id.ends_with("_true") {
        def.cost * 3
    } else {
        def.cost
    }
}

/// The most populous city of a wizard at war with the AI that the spell can be aimed at and
/// does not already lie on.
fn curse_city(view: &PlayerView, spell: SpellId) -> Option<hecs::Entity> {
    let mut cities: Vec<(u32, hecs::Entity)> = view
        .known_cities()
        .into_iter()
        .filter(|&(c, owner, _)| {
            view.is_rival_wizard(owner)
                && view.treaty_with(owner) == Treaty::War
                && !view.city_enchantments(c).contains(&spell)
        })
        .filter_map(|(c, _, _)| view.city_economy(c).map(|e| (e.figures, c)))
        .collect();
    cities.sort_by_key(|&(n, c)| (std::cmp::Reverse(n), view.serial_of(c)));
    cities
        .into_iter()
        .map(|(_, c)| c)
        .find(|&c| view.can_aim(spell, Some(Target::City(c))))
}

/// The square of the wizard's most populous city that another wizard has put a spell on.
fn cursed_city(view: &PlayerView) -> Option<Position> {
    let mut cities: Vec<(u32, hecs::Entity, Position)> = view
        .my_cities()
        .into_iter()
        .filter(|&(c, _)| !view.city_curses(c).is_empty())
        .filter_map(|(c, p)| view.city_economy(c).map(|e| (e.figures, c, p)))
        .collect();
    cities.sort_by_key(|&(n, c, _)| (std::cmp::Reverse(n), view.serial_of(c)));
    cities.first().map(|&(_, _, p)| p)
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

/// One unit of the most hurt stack that stands in the field, well away from the summoning
/// circle, when a good part of it is hurt: the stack Word of Recall brings home.
fn stranded_unit(view: &PlayerView) -> Option<hecs::Entity> {
    let circle = view.wizard()?.summon_at()?;
    let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    let mut stacks: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
    for u in view.my_units() {
        let Some(at) = view.unit_position(u) else {
            continue;
        };
        if cities.contains(&at) || !is_fighter(view, u) {
            continue;
        }
        match stacks.iter_mut().find(|(p, _)| *p == at) {
            Some((_, units)) => units.push(u),
            None => stacks.push((at, vec![u])),
        }
    }
    let mut best: Option<(u32, hecs::Entity)> = None;
    for (at, mut units) in stacks {
        if view
            .distance(at, circle)
            .is_some_and(|d| d < RECALL_DISTANCE)
        {
            continue;
        }
        view.sort_by_serial(&mut units);
        let (mut missing, mut total) = (0u32, 0u32);
        for &u in &units {
            let (hits, max) = view.unit_hits(u);
            missing += max.saturating_sub(hits);
            total += max;
        }
        if total == 0 || missing * 100 / total < RECALL_PERCENT {
            continue;
        }
        if best.is_none_or(|(m, _)| missing > m) {
            best = Some((missing, units[0]));
        }
    }
    best.map(|(_, u)| u)
}

/// Whether the AI can carry the lasting spell's upkeep on its mana income.
fn upkeep_ok(view: &PlayerView, def: &SpellDef) -> bool {
    let income = view.income();
    income.split.mana - income.mana_upkeep - def.upkeep as i32 >= 0
}

/// Resurrection: the original weighs it highest when the wizard has a free place for a hero and
/// one lies dead. The last of the raisable heroes is raised (`ai_resurrection_pick`); a cast that
/// is paid over turns picks its hero when it is done (`resolve_paid`). Returns whether it began.
fn resurrect(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    let kind = {
        let spell = game
            .content()
            .rules
            .spells
            .id_of(f4x_core::spell::hero::RESURRECTION);
        let Some(spell) = spell else {
            return false;
        };
        let knows = game
            .wizard(me)
            .is_some_and(|m| m.casting().is_none() && m.knows(spell));
        if !knows
            || game
                .units()
                .hero_spell_blocked(me, f4x_core::spell::hero::RESURRECTION)
                .is_some()
        {
            return false;
        }
        let mana = game.player(me).map_or(0, |p| p.treasury().mana);
        let cost = game.casting_cost(me, spell, false);
        // The same gate as the other spells: paid from the reserve, not more than fifty turns.
        let per_turn = mana / 10 + game.income(me).split.mana;
        if cost * 3 / 2 > mana || cost / 50 > per_turn {
            return false;
        }
        game.units().ai_resurrection_pick()
    };
    let Some(kind) = kind else {
        return false;
    };
    let spell = game
        .content()
        .rules
        .spells
        .id_of(f4x_core::spell::hero::RESURRECTION);
    let Some(spell) = spell else {
        return false;
    };
    let command = Command::CastSpell {
        player: me,
        spell,
        target: Some(Target::Hero(kind)),
    };
    ai.send(game, command, events);
    true
}

fn cast(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    if resurrect(ai, game, events) {
        return;
    }
    let choice: Option<(SpellId, Option<Target>)> = {
        let view = PlayerView::new(game, me);
        let Some(magic) = view.wizard() else {
            return;
        };
        if magic.casting().is_some() {
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
            .known()
            .iter()
            .copied()
            .map(|s| (kind(view.spell(s)), view.casting_cost(s), s))
            .filter(|&(k, cost, _)| k != Kind::Other && cost > 0)
            .collect();
        let traits = crate::objective::traits(&view);
        options.sort_by_key(|&(k, cost, s)| (cast_rank(k, &traits), std::cmp::Reverse(cost), s));
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
            let room = magic.power_split().mana <= 55;
            let target = pick_target(&view, spell);
            let wanted = match k {
                Kind::Mastery => true,
                // Summoned creatures cost upkeep mana: only while the army is thin.
                Kind::Summon => {
                    now && room
                        && fighters < 12 + 2 * view.my_cities().len()
                        // The square the creatures appear on may be full.
                        && view.can_aim(spell, None)
                }
                Kind::UnitBuff => {
                    now && room && fighters >= 3 && upkeep_ok(&view, def) && target.is_some()
                }
                Kind::Mutate => now && fighters >= 3 && target.is_some(),
                Kind::CityBuff => now && room && upkeep_ok(&view, def) && target.is_some(),
                Kind::GlobalBuff => {
                    now && room
                        && mana - cost >= GLOBAL_RESERVE
                        && upkeep_ok(&view, def)
                        && !magic.has_global(spell)
                }
                Kind::Explore
                | Kind::Cure
                | Kind::Disenchant
                | Kind::Recall
                | Kind::Blast
                | Kind::Stasis => now && target.is_some(),
                // Blows at an enemy at war, from mana the AI can spare.
                Kind::CityCurse | Kind::WizardCurse | Kind::Disjunction => {
                    now && room && mana - cost >= GLOBAL_RESERVE && target.is_some()
                }
                Kind::WorldSpell => {
                    now && room
                        && mana - cost >= GLOBAL_RESERVE
                        && upkeep_ok(&view, def)
                        && !magic.has_global(spell)
                        && world_spell_has_victim(&view, def)
                }
                Kind::Other => false,
            };
            // The original's check: not more than fifty turns of what the AI can put towards it.
            let per_turn = mana / 10 + income.split.mana;
            if wanted
                && (k == Kind::Mastery
                    || (cost / 50 <= per_turn && quick_enough(cost, view.casting_power())))
            {
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
    use crate::testworld::{at, fresh_game, game, shipped, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
    use f4x_core::diplomacy::DiplomacyCommand;
    use f4x_core::player::{PlayerId, Treasury};
    use f4x_core::wizard::objective::Objective;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn spell(id: &str) -> Result<SpellId, String> {
        shipped()
            .rules
            .spells
            .id_of(id)
            .ok_or(format!("{id} is shipped"))
    }

    fn treasury(g: &mut Game, player: PlayerId) -> Result<&mut Treasury, &'static str> {
        g.treasury_mut(player).ok_or("the player has a treasury")
    }

    #[test]
    fn an_objective_moves_the_kinds_it_weights_up_the_order_of_casting() {
        use crate::objective::traits_of;
        let pragmatist = traits_of(Some(Objective::Pragmatist));
        let militarist = traits_of(Some(Objective::Militarist));
        let perfectionist = traits_of(Some(Objective::Perfectionist));
        // The ladder: summons before buffs before curses.
        assert!(cast_rank(Kind::Summon, &pragmatist) < cast_rank(Kind::GlobalBuff, &pragmatist));
        assert!(cast_rank(Kind::GlobalBuff, &pragmatist) < cast_rank(Kind::CityCurse, &pragmatist));
        // A militarist curses its enemies before it buffs its cities.
        assert!(cast_rank(Kind::CityCurse, &militarist) < cast_rank(Kind::CityBuff, &militarist));
        assert!(cast_rank(Kind::CityBuff, &pragmatist) < cast_rank(Kind::CityCurse, &pragmatist));
        // A perfectionist buffs its cities before it summons.
        assert!(
            cast_rank(Kind::CityBuff, &perfectionist) < cast_rank(Kind::Summon, &perfectionist)
        );
        // Nothing passes the Spell of Mastery.
        for t in [&pragmatist, &militarist, &perfectionist] {
            for k in [
                Kind::Summon,
                Kind::CityCurse,
                Kind::GlobalBuff,
                Kind::Explore,
            ] {
                assert!(cast_rank(Kind::Mastery, t) < cast_rank(k, t));
            }
        }
    }

    fn kind_of(id: &str) -> Result<Kind, String> {
        let rules = &shipped().rules;
        Ok(kind(rules.spells.get(spell(id)?)))
    }

    #[test]
    fn spells_are_sorted_into_the_kinds_the_ai_casts() -> TestResult {
        let cases = [
            ("spell_of_mastery", Kind::Mastery),
            ("war_bears", Kind::Summon),
            ("giant_strength", Kind::UnitBuff),
            ("heroism", Kind::UnitBuff),
            ("elemental_armor", Kind::UnitBuff),
            ("natures_eye", Kind::CityBuff),
            ("herb_mastery", Kind::GlobalBuff),
            ("earth_lore", Kind::Explore),
            ("natures_cures", Kind::Cure),
            ("disenchant_area", Kind::Disenchant),
            ("disenchant_true", Kind::Disenchant),
            ("chaos_channels", Kind::Mutate),
            ("word_of_recall", Kind::Recall),
            ("disjunction", Kind::Disjunction),
            ("spell_binding", Kind::Disjunction),
            ("famine", Kind::CityCurse),
            ("earthquake", Kind::CityCurse),
            ("cruel_unminding", Kind::WizardCurse),
            ("meteor_storm", Kind::WorldSpell),
            ("armageddon", Kind::WorldSpell),
            ("time_stop", Kind::WorldSpell),
            ("great_wasting", Kind::WorldSpell),
            ("natures_wrath", Kind::WorldSpell),
            ("spell_blast", Kind::Blast),
            ("stasis", Kind::Stasis),
            ("transmute", Kind::Other),
            ("plane_shift", Kind::Other),
        ];
        for (id, expected) in cases {
            assert_eq!(kind_of(id)?, expected, "{id}");
        }
        Ok(())
    }

    #[test]
    fn what_the_ai_cannot_use_is_left_alone() -> TestResult {
        // A curse on the whole world, a bolt for battle, a spell that is not done yet.
        for id in ["fire_bolt", "suppress_magic", "earth_to_mud"] {
            assert_eq!(kind_of(id)?, Kind::Other, "{id}");
        }
        Ok(())
    }

    #[test]
    fn a_wizard_that_knows_resurrection_raises_the_last_dead_hero() -> TestResult {
        let mut g = fresh_game()?;
        let ai = Ai::new(P1);
        let resurrection = spell(f4x_core::spell::hero::RESURRECTION)?;
        treasury(&mut g, P1)?.gold = 50_000;
        treasury(&mut g, P1)?.mana = 5_000;
        for k in [1, 4] {
            let hero = g
                .hire_hero(P1, k)
                .map_err(|e| format!("hiring hero {k}: {e:?}"))?;
            g.cheat_slay_unit(hero);
        }
        // Not known: nothing happens. Known: the spell is begun for the last dead hero.
        let mut events = Vec::new();
        assert!(!resurrect(&ai, &mut g, &mut events));
        g.cheat_learn_spell(P1, resurrection);
        assert!(resurrect(&ai, &mut g, &mut events), "{events:?}");
        let started = g
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .casting()
            .is_some()
            || events.iter().any(|e| matches!(e, Event::HeroJoined { .. }));
        assert!(started, "{events:?}");
        Ok(())
    }

    #[test]
    fn a_militarist_gives_more_power_to_mana_than_a_pragmatist() -> TestResult {
        let research_share = |objective: Objective| -> Result<_, Box<dyn std::error::Error>> {
            let mut g = game(COAST_CITY, "high_men");
            g.wizard_mut(P0)
                .ok_or("player 0 is a wizard")?
                .put(Some(objective));
            // A big reserve needs no spare mana income: the first candidate that pays is taken.
            treasury(&mut g, P0)?.mana = 1_000;
            split(&Ai::new(P0), &mut g, &mut Vec::new());
            Ok(g.wizard(P0)
                .ok_or("player 0 is a wizard")?
                .power_split()
                .research)
        };
        assert!(research_share(Objective::Militarist)? < research_share(Objective::Pragmatist)?);
        Ok(())
    }

    #[test]
    fn an_objective_researches_the_sort_of_spell_it_likes_first() -> TestResult {
        use crate::objective::{traits_of, Like};
        let spells = &shipped().rules.spells;
        let bears = spells.get(spell("war_bears")?);
        let militarist = traits_of(Some(Objective::Militarist));
        assert!(liked(bears, kind(bears), &militarist));
        assert!(!liked(
            bears,
            kind(bears),
            &traits_of(Some(Objective::Perfectionist))
        ));
        assert!(!liked(bears, kind(bears), &traits_of(None)));
        assert!(militarist.likes.contains(&Like::Summon));
        Ok(())
    }

    #[test]
    fn a_banished_wizard_turns_just_enough_gold_into_mana_for_the_return() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let spell = spell("spell_of_return")?;
        let mut wizard = g.wizard_mut(P0).ok_or("player 0 is a wizard")?;
        wizard.put(Some(f4x_core::wizard::fortress::Banishment {
            by: Some(P1),
        }));
        wizard.get_mut::<f4x_core::wizard::sheet::Skill>().points = 2_000;
        g.scenario_casting(P0, spell, 500);
        let due = g
            .wizard(P0)
            .ok_or("player 0 is a wizard")?
            .casting_skill()
            .min(500);
        assert!(due > 4, "the test wizard has a casting skill ({due})");
        {
            let t = treasury(&mut g, P0)?;
            t.mana = 0;
            t.gold = 10_000;
        }
        let ai = Ai::new(P0);
        fund_return(&ai, &mut g, &mut Vec::new());
        let t = g.player(P0).ok_or("player 0 exists")?.treasury();
        assert_eq!(t.mana, due, "mana now covers the payment");
        assert!(t.gold < 10_000 && t.gold >= 10_000 - 2 * due - 2);
        // Covered already: nothing more is converted.
        let gold = t.gold;
        fund_return(&ai, &mut g, &mut Vec::new());
        assert_eq!(g.player(P0).ok_or("player 0 exists")?.treasury().gold, gold);
        Ok(())
    }

    #[test]
    fn a_wizard_that_is_not_banished_keeps_its_gold() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.scenario_casting(P0, spell("spell_of_return")?, 500);
        treasury(&mut g, P0)?.mana = 0;
        let gold = g.player(P0).ok_or("player 0 exists")?.treasury().gold;
        fund_return(&Ai::new(P0), &mut g, &mut Vec::new());
        assert_eq!(g.player(P0).ok_or("player 0 exists")?.treasury().gold, gold);
        Ok(())
    }

    #[test]
    fn a_wizard_curses_the_city_of_an_enemy_at_war_and_leaves_a_neighbour_at_peace_alone(
    ) -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let famine = spell("famine")?;
        g.cheat_learn_spell(P0, famine);
        treasury(&mut g, P0)?.mana = 5_000;
        g.wizard_mut(P0)
            .ok_or("player 0 is a wizard")?
            .set_split(20, 40, 40)
            .map_err(|e| format!("{e:?}"))?;
        let ai = Ai::new(P0);
        let enemy_city = at(ENEMY_CITY.0, ENEMY_CITY.1);
        g.explore(P0, enemy_city, 3);
        let enemy = g
            .economy()
            .city_at(enemy_city)
            .ok_or("the enemy has a city")?;
        let aimed = |g: &Game| {
            PlayerView::new(g, P0)
                .known_cities()
                .iter()
                .any(|c| c.0 == enemy)
        };
        assert!(aimed(&g), "the test world shows the enemy city");
        cast(&ai, &mut g, &mut Vec::new());
        assert!(
            g.wizard(P0)
                .ok_or("player 0 is a wizard")?
                .casting()
                .is_none(),
            "at peace: no curse"
        );
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        cast(&ai, &mut g, &mut Vec::new());
        let began = g
            .wizard(P0)
            .ok_or("player 0 is a wizard")?
            .casting()
            .is_some()
            || g.city_enchantments(enemy).iter().any(|&(s, _)| s == famine);
        assert!(began, "at war: Famine begun or cast");
        Ok(())
    }

    #[test]
    fn a_wizard_begins_a_spell_on_the_world_only_while_at_war() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let storm = spell("meteor_storm")?;
        g.cheat_learn_spell(P0, storm);
        treasury(&mut g, P0)?.mana = 20_000;
        let mut wizard = g.wizard_mut(P0).ok_or("player 0 is a wizard")?;
        wizard.found(&[("chaos", 11), ("death", 11), ("nature", 11)], &[]);
        wizard.put(Some(f4x_core::wizard::fortress::Fortress(at(
            COAST_CITY.0,
            COAST_CITY.1,
        ))));
        wizard.get_mut::<f4x_core::wizard::sheet::Skill>().points = 20_000;
        wizard.set_split(55, 0, 45).map_err(|e| format!("{e:?}"))?;
        let ai = Ai::new(P0);
        let began = |g: &Game| -> Result<bool, &'static str> {
            let magic = g.wizard(P0).ok_or("player 0 is a wizard")?;
            Ok(magic.casting().is_some() || magic.has_global(storm))
        };
        cast(&ai, &mut g, &mut Vec::new());
        assert!(!began(&g)?, "at peace: nothing on the world");
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        cast(&ai, &mut g, &mut Vec::new());
        assert!(began(&g)?, "at war: Meteor Storm begun or cast");
        Ok(())
    }

    #[test]
    fn natures_wrath_has_a_victim_only_in_an_enemy_with_death_or_chaos_books() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let wrath = spell("natures_wrath")?;
        let storm = spell("meteor_storm")?;
        g.wizard_mut(P1)
            .ok_or("player 1 is a wizard")?
            .found(&[("life", 5)], &[]);
        let victim = |g: &Game, id| {
            let def = g.content().rules.spells.get(id);
            world_spell_has_victim(&PlayerView::new(g, P0), def)
        };
        assert!(!victim(&g, wrath) && !victim(&g, storm), "at peace");
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(victim(&g, storm), "at war: the storm has a victim");
        assert!(!victim(&g, wrath), "only Life: no Wrath");
        g.wizard_mut(P1)
            .ok_or("player 1 is a wizard")?
            .found(&[("chaos", 5)], &[]);
        assert!(victim(&g, wrath), "Chaos books: Wrath");
        Ok(())
    }

    #[test]
    fn a_wizard_disenchants_the_city_an_enemy_has_cursed_and_otherwise_leaves_it() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let (area, famine, own) = (
            spell("disenchant_area")?,
            spell("famine")?,
            spell("natures_eye")?,
        );
        g.cheat_learn_spell(P0, area);
        treasury(&mut g, P0)?.mana = 5_000;
        let city = g
            .economy()
            .city_at(at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("player 0 has a city")?;
        g.scenario_enchant_city(city, own, P0);
        let aim = |g: &Game| pick_target(&PlayerView::new(g, P0), area);
        assert!(aim(&g).is_none(), "a spell of its own is no curse");
        g.scenario_enchant_city(city, famine, P1);
        assert_eq!(
            aim(&g),
            Some(Target::Tile(at(COAST_CITY.0, COAST_CITY.1))),
            "the cursed city is the aim"
        );
        Ok(())
    }

    #[test]
    fn a_wizard_breaks_the_global_spell_of_an_enemy_at_war_and_not_of_a_neighbour_at_peace(
    ) -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let (disjunction, awareness) = (spell("disjunction")?, spell("awareness")?);
        g.cheat_learn_spell(P0, disjunction);
        g.scenario_add_global(P1, awareness);
        let aim = |g: &Game| pick_target(&PlayerView::new(g, P0), disjunction);
        assert!(aim(&g).is_none(), "at peace: left alone");
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        let instance = g
            .global_instance(P1, awareness)
            .ok_or("player 1 has the global spell")?;
        assert_eq!(aim(&g), Some(Target::Spell(instance)));
        Ok(())
    }

    #[test]
    fn a_wizard_blasts_a_rival_at_war_who_is_casting_and_leaves_one_who_is_not() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let blast = spell("spell_blast")?;
        g.cheat_learn_spell(P0, blast);
        treasury(&mut g, P0)?.mana = 5_000;
        let aim = |g: &Game| pick_target(&PlayerView::new(g, P0), blast);
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(aim(&g).is_none(), "the rival casts nothing");
        g.scenario_casting(P1, spell("meteor_storm")?, 100);
        assert_eq!(aim(&g), Some(Target::Player(P1)));
        Ok(())
    }

    #[test]
    fn a_wizard_holds_an_enemy_stack_near_its_city_only_while_at_war_and_stronger_than_the_garrison(
    ) -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let stasis = spell("stasis")?;
        g.cheat_learn_spell(P0, stasis);
        let near = at(COAST_CITY.0 - 1, COAST_CITY.1);
        spawn(&mut g, P1, near, "high_men_spearmen", 8);
        let aim = |g: &Game| pick_target(&PlayerView::new(g, P0), stasis);
        assert!(aim(&g).is_none(), "at peace: left alone");
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert_eq!(aim(&g), Some(Target::Tile(near)));
        spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            "high_men_spearmen",
            9,
        );
        assert!(aim(&g).is_none(), "the garrison is the stronger");
        Ok(())
    }

    #[test]
    fn a_dear_spell_is_only_begun_when_it_is_paid_for_in_five_turns() {
        assert!(
            quick_enough(DEAR_SPELL, 1),
            "cheap spells are never held back"
        );
        assert!(!quick_enough(1000, 150), "seven turns");
        assert!(quick_enough(1000, 200), "five turns");
        assert!(!quick_enough(1500, 0), "no casting power at all");
    }

    #[test]
    fn a_hurt_stack_far_from_home_is_recalled_and_one_in_a_city_is_not() -> TestResult {
        let mut g = fresh_game()?;
        let recall = spell("word_of_recall")?;
        g.cheat_learn_spell(P1, recall);
        let circle = g
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        let aim = |g: &Game| pick_target(&PlayerView::new(g, P1), recall);
        for u in PlayerView::new(&g, P1).my_units() {
            g.scenario_set_hurt(u, 1);
        }
        assert!(aim(&g).is_none(), "units at home are not stranded");
        let width = g.map().ok_or("a map")?.width();
        let far = f4x_core::map::Position {
            x: (circle.x + 20) % width,
            ..circle
        };
        let fighters = spawn(&mut g, P1, far, "high_men_spearmen", 2);
        assert!(aim(&g).is_none(), "whole stack is not hurt");
        for &u in &fighters {
            let (_, max) = PlayerView::new(&g, P1).unit_hits(u);
            g.scenario_set_hurt(u, max);
        }
        assert!(matches!(aim(&g), Some(Target::Unit(u)) if fighters.contains(&u)));
        Ok(())
    }

    #[test]
    fn heroism_is_aimed_at_a_fighter_and_not_at_one_that_has_it() -> TestResult {
        let mut g = fresh_game()?;
        let heroism = spell("heroism")?;
        g.cheat_learn_spell(P1, heroism);
        let aim = |g: &Game| pick_target(&PlayerView::new(g, P1), heroism);
        let Some(Target::Unit(first)) = aim(&g) else {
            return Err("Heroism has a unit to go on".into());
        };
        g.scenario_enchant_unit(first, heroism, P1);
        match aim(&g) {
            Some(Target::Unit(second)) => assert_ne!(second, first),
            other => assert!(other.is_none(), "{other:?}"),
        }
        Ok(())
    }
}
