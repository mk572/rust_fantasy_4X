//! The wizard's side of the AI: how power is split, what is researched and what is cast
//! (`docs/ai-design.md` sections 5.3 and 7). Every choice is a function of what the AI sees, so a
//! game replays exactly. The order of wants each turn: the Spell of Mastery when it is known, a
//! summoning spell, a buff for the strongest units, a buff for a city, a lasting spell on the
//! whole wizard.

use f4x_def::rules::ai_strategy::{SplitLevers, StrategyAiRules};

use crate::assembly::mirror;
use crate::lever::Levered;
use f4x_core::diplomacy::Treaty;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::exec::Target;
use f4x_core::spell::unit_effect::UnitEffect;
use f4x_core::world::{Command, Event, World};
use f4x_def::rules::effects::Effect;
use f4x_def::rules::{CastContext, Duration, SpellDef, SpellId, SpellTarget};

use crate::view::{fighting_strength, is_fighter, unit_strength};
use crate::Ai;
use f4x_master::Game;

/// The kinds of overland spell the AI knows what to do with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Finishing it wins the game.
    Mastery,
    Summon,
    /// Summon Hero, Summon Champion and Incarnation: a hero joins at the summoning circle.
    SummonHero,
    /// A lasting spell that makes one of the wizard's units better.
    UnitBuff,
    /// Water Walking and Wind Walking: a lasting spell that lets a walker cross the sea without a
    /// ship, cast on a unit that waits for a ship none is coming to (`cross_target`). Floating
    /// Island is one too: it raises a ship (a transport of eight) on the sea square of the strait
    /// the waiting units cross (`island_target`). Flight does the same as the walks but stays a
    /// `UnitBuff`: it is worth casting for the fight as well.
    Cross,
    /// A lasting spell that makes one of the wizard's cities better.
    CityBuff,
    /// A lasting spell on the wizard that helps it.
    GlobalBuff,
    /// Earth Lore: explores a block of the map around the target square.
    Explore,
    /// Enchant Road: the roads between the fortress and the places that matter, so that the
    /// army and the settlers cross the land at no cost in moves (`enchant_road.rs`).
    EnchantRoad,
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
    /// Lycanthropy: turns an obsolete fighter into a werewolf. The unit is given up, so its
    /// production cost counts against the spell when it is rated against the summons
    /// (`lycanthropy_target`); the mana needed is the spell's own.
    Lycanthropy,
    /// Word of Recall: takes a hurt stack far from home back to the summoning circle.
    Recall,
    /// Plane Shift: carries the whole stack on a square to the other plane (the army's way
    /// across when no tower is open).
    PlaneShift,
    /// Ice Storm, Fire Storm and Black Wind: strike every unit of an enemy stack that stands by
    /// a city of the wizard.
    StackStrike,
    /// Corruption on the land of a city, Warp Node on the node of a wizard at war.
    LandCurse,
    /// Raise Volcano on a free square of the wizard's own land.
    LandBoost,
    /// Astral Gate: lets the army cross the planes in the city it gathers in.
    Gate,
    /// Earth Gate: a gate in the rally city and in the far cities that send troops to it, so that
    /// the troops arrive at once (`earth_gate_target`).
    EarthGate,
    /// Everything else: not cast by the AI yet.
    Other,
}

fn has_hook(def: &SpellDef, name: &str) -> bool {
    def.effects
        .iter()
        .any(|r| matches!(&r.effect, Effect::Custom { name: h, .. } if h == name))
}

/// Whether the spell summons a spirit that can meld with a node.
fn summons_melder(view: &PlayerView, def: &SpellDef) -> bool {
    def.effects.iter().any(|r| match &r.effect {
        Effect::Summon { unit, .. } => view
            .content()
            .rules
            .units
            .get(*unit)
            .abilities
            .contains(&f4x_def::rules::Ability::new(42 /* meld */)),
        _ => false,
    })
}

/// The own fighter Lycanthropy turns into a werewolf, the production cost of the unit given up
/// (counted one for one as mana in the rating only, not in the mana needed) and the rate: the
/// strength gained per mana in all. Only a unit the werewolf is clearly better than is turned
/// (`lycanthropy_edge`); of those, the one with the best rate.
pub(crate) fn lycanthropy_target(
    view: &PlayerView,
    spell: SpellId,
) -> Option<(hecs::Entity, i32, f32)> {
    let wolf = view
        .content()
        .rules
        .units
        .id_numbered(view.content().rules.config.spell_effects.werewolves)?;
    let wolf_power =
        f4x_battle::power::def_power(view.content().rules.units.get(wolf), &view.content().rules);
    let cast = view.casting_cost(spell).max(1) as f32;
    let mut units: Vec<hecs::Entity> = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u) && !crate::recover::needs_rest(view, u))
        .collect();
    view.sort_by_serial(&mut units);
    units
        .into_iter()
        .filter_map(|u| {
            let power = unit_strength(view, u);
            let extra = view.unit_def(u)?.cost as i32;
            (wolf_power >= power * view.levers().magic.lycanthropy_edge
                && view.can_aim(spell, Some(Target::Unit(u))))
            .then(|| ((wolf_power - power) / (cast + extra as f32), u, extra))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(rate, u, extra)| (u, extra, rate))
}

/// The abilities (`abilities.ron`) that take a unit over the sea: water walking and wind walking
/// (`SEA_WALKS`, the spells of `Kind::Cross`), and flying, which Flight gives.
const SEA_WALKS: [u16; 2] = [71 /* water_walking */, 74 /* wind_walking */];
const FLYING: u16 = 24;

/// Whether a spell grants one of the abilities.
fn grants_any(def: &SpellDef, abilities: &[u16]) -> bool {
    def.effects
        .iter()
        .any(|r| matches!(&r.effect, Effect::GrantAbility(a) if abilities.contains(&a.num())))
}

/// Whether the spell raises a floating island: a ship made of the sea square it is aimed at.
fn raises_island(def: &SpellDef) -> bool {
    def.effects.iter().any(|r| {
        matches!(&r.effect, Effect::Custom { name, args }
            if name == f4x_core::spell::terraform::TERRAIN_EFFECT
                && args.iter().any(|a| a == "floating_island"))
    })
}

/// What the AI makes of a spell.
pub fn kind(def: &SpellDef) -> Kind {
    if !def.contexts.contains(&CastContext::Overland) {
        return Kind::Other;
    }
    if def.has_hook(f4x_core::spell::special::SPELL_OF_MASTERY) {
        return Kind::Mastery;
    }
    if def
        .effects
        .iter()
        .any(|r| matches!(r.effect, Effect::Summon { .. }))
    {
        return Kind::Summon;
    }
    if matches!(
        f4x_core::spell::hero::HeroSummon::of(def),
        Some(
            f4x_core::spell::hero::HeroSummon::Hero
                | f4x_core::spell::hero::HeroSummon::Champion
                | f4x_core::spell::hero::HeroSummon::Torin
        )
    ) {
        return Kind::SummonHero;
    }
    if f4x_core::spell::terraform::TerrainEffect::EarthLore.is_in(def) {
        return Kind::Explore;
    }
    if f4x_core::spell::terraform::TerrainEffect::EnchantRoad.is_in(def) {
        return Kind::EnchantRoad;
    }
    match UnitEffect::of(def) {
        Some(UnitEffect::NaturesCures) => return Kind::Cure,
        Some(UnitEffect::ChaosChannels) => return Kind::Mutate,
        Some(UnitEffect::Lycanthropy) => return Kind::Lycanthropy,
        _ => {}
    }
    if def.has_hook_arg(
        f4x_core::spell::special::RECALL,
        f4x_core::spell::special::RECALL_ANYONE,
    ) {
        return Kind::Recall;
    }
    if def.has_hook(f4x_core::spell::special::PLANE_SHIFT) {
        return Kind::PlaneShift;
    }
    if UnitEffect::of(def) == Some(UnitEffect::Stasis) {
        return Kind::Stasis;
    }
    if def.target == SpellTarget::OwnUnit
        && def.duration == Duration::Permanent
        && grants_any(def, &SEA_WALKS)
    {
        return Kind::Cross;
    }
    if raises_island(def) {
        return Kind::Cross;
    }
    if def.target == SpellTarget::OwnUnit
        && def.duration == Duration::Permanent
        && has_hook(def, "heroism")
    {
        return Kind::UnitBuff;
    }
    if (def.effects.is_empty() && def.defence.is_empty()) || has_hook(def, "unimplemented") {
        return Kind::Other;
    }
    if def.target == SpellTarget::GlobalEnchantment {
        return Kind::Disjunction;
    }
    if crate::intent::is_astral_gate(def) {
        return Kind::Gate;
    }
    if crate::intent::is_earth_gate(def) {
        return Kind::EarthGate;
    }
    if crate::intent::is_stack_strike(def) {
        return Kind::StackStrike;
    }
    if crate::intent::is_land_curse(def) {
        return Kind::LandCurse;
    }
    if crate::intent::is_land_boost(def) {
        return Kind::LandBoost;
    }
    if def.target == SpellTarget::Tile
        && matches!(
            f4x_core::spell::dispel::CounterEffect::of(def),
            Some((f4x_core::spell::dispel::CounterEffect::Dispel, _))
        )
    {
        return Kind::Disenchant;
    }
    let helps = |r: &f4x_def::rules::effects::EffectRule| match &r.effect {
        Effect::Modify { stat, value, .. } => {
            crate::intent::gain_when_added(*stat, *value >= f4x_def::half_int::HalfInt::ZERO)
        }
        Effect::GrantAbility(_) => true,
        // Spell Lock is read by the battle AI, not worth a cast of its own.
        Effect::Custom { name, args } if name == "unit_effect" => {
            !args.iter().any(|a| a == "spell_lock")
        }
        Effect::Custom { name, .. } => name == "city_effect" || name == "wall",
        _ => false,
    };
    match def.target {
        SpellTarget::OwnUnit
            if def.duration != Duration::Instant
                && def.effects.iter().all(
                    |r| !matches!(&r.effect, Effect::Custom { name, .. } if name != "unit_effect"),
                )
                && def.effects.iter().all(helps) =>
        {
            Kind::UnitBuff
        }
        SpellTarget::OwnCity
            if (def.duration == Duration::Permanent || has_hook(def, "wall"))
                && def.effects.iter().all(helps) =>
        {
            Kind::CityBuff
        }
        SpellTarget::None => match crate::intent::global_use_of(def) {
            Some(crate::intent::GlobalUse::Boon | crate::intent::GlobalUse::Realm) => {
                Kind::GlobalBuff
            }
            Some(crate::intent::GlobalUse::War | crate::intent::GlobalUse::Against(..)) => {
                Kind::WorldSpell
            }
            Some(crate::intent::GlobalUse::Never) | None => Kind::Other,
        },
        SpellTarget::EnemyCity => Kind::CityCurse,
        SpellTarget::EnemyWizard
            if f4x_core::spell::wizard::WizardEffect::SpellBlast.is_in(def) =>
        {
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
        // Fast movement is a big advantage: the road is cast before the summons and the buffs.
        Kind::EnchantRoad => 2,
        Kind::Cure => 2,
        Kind::Summon => 3,
        Kind::SummonHero => 3,
        Kind::UnitBuff => 4,
        // Units wait on the shore for it: before the buffs.
        Kind::Cross => 3,
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
        // Rated against the summons: the same ladder step, the rate decides (`spellhelper::rate`).
        Kind::Lycanthropy => 3,
        Kind::Recall => 2,
        Kind::PlaneShift => 2,
        Kind::Gate => 2,
        // After the summons: a gate saves the army its walk, it does not make it stronger.
        Kind::EarthGate => 4,
        Kind::StackStrike => 2,
        Kind::LandCurse => 7,
        Kind::LandBoost => 6,
        Kind::Other => 9,
    }
}

/// Where a kind of spell stands in the order of casting for a wizard: the ladder of `priority`
/// (four to a step), moved up by what its objective weights (`Traits::cast_boost`). The Spell of
/// Mastery stays first, and nothing else is moved past it.
fn cast_rank(k: Kind, traits: &crate::objective::Traits) -> i32 {
    use crate::objective::Like;
    let like = match k {
        Kind::Summon | Kind::SummonHero | Kind::Lycanthropy => Some(Like::Summon),
        Kind::UnitBuff => Some(Like::UnitBuff),
        Kind::CityBuff => Some(Like::CityBuff),
        Kind::GlobalBuff => Some(Like::GlobalBuff),
        Kind::Explore => Some(Like::Explore),
        Kind::CityCurse
        | Kind::WizardCurse
        | Kind::Disjunction
        | Kind::WorldSpell
        | Kind::StackStrike
        | Kind::LandCurse => Some(Like::Curse),
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
    alchemy(ai, game, events);
    stop_aimless(ai, game, events);
    resolve_paid(ai, game, events);
    split(ai, game, events);
    research(ai, game, events);
    cast(ai, game, events);
    crate::craft::run(ai, game, events);
}

/// The mana wanted before a spell of this cost is begun (`mana_wanted_percent`).
fn mana_before(levers: &StrategyAiRules, cost: i32) -> i32 {
    cost * levers.magic.mana_wanted_percent / 100
}

/// The mana the wizard could put to use now: what its dearest summon it can aim costs with the
/// margin the casting rule asks for (zero when it has none to cast).
fn mana_wanted(view: &PlayerView, blocked: &[f4x_core::map::Plane], round: u32) -> i32 {
    let Some(magic) = view.wizard() else {
        return 0;
    };
    let summon = magic
        .known()
        .iter()
        .copied()
        .filter(|&s| matches!(kind(view.spell(s)), Kind::Summon | Kind::SummonHero))
        .filter(|&s| view.can_aim(s, None))
        .map(|s| mana_before(view.levers(), view.casting_cost(s)))
        .max()
        .unwrap_or(0);
    // The Spell of Mastery being cast: what is still owed is mana to buy, whatever the gold is
    // worth otherwise (the win itself, task 502).
    let mastery = magic
        .casting()
        .filter(|c| kind(view.spell(c.spell)) == Kind::Mastery)
        .map_or(0, |c| c.remaining.max(0));
    // The summon is cast first and paid for, and the pool then holds the mana for the battle spells
    // that take a tower the soldiers alone cannot (task 502).
    summon + crate::spellhelper::mana_goal(view, blocked, round) + mastery
}

/// A wizard rich in gold and short of the mana for a summon it could cast turns just enough of the
/// surplus gold into mana for it (Alchemy returns a percent of what is put in). Mana it has no use
/// for is not bought.
fn alchemy(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let gold = {
        let view = PlayerView::new(game, ai.player);
        let Some(w) = view.wizard() else {
            return;
        };
        let short =
            mana_wanted(&view, &ai.tower_blocked, game.round()) - view.treasury().mana.floor();
        let spare = view.treasury().gold.floor() - view.levers().magic.gold_kept;
        let percent = w
            .stat(
                &view.content().rules,
                f4x_def::rules::Stat::AlchemyPercent,
                50,
            )
            .max(1);
        (short * 100 / percent).min(spare)
    };
    if gold > 0 {
        ai.order(
            game,
            Command::Alchemy {
                player: ai.player,
                amount: gold,
                gold_to_mana: true,
            },
            events,
        );
    }
}

/// A banished wizard pays the Spell of Return from its mana reserve and has no income. When the
/// reserve does not cover this turn's payment it turns as much gold as it needs into mana, and no
/// more (a choice of the script, not a rule).
fn fund_return(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let gold = {
        let view = PlayerView::new(game, me);
        let Some(w) = view.wizard() else {
            return;
        };
        let Some(casting) = w
            .casting()
            .filter(|c| game.spell_has_hook(c.spell, f4x_core::spell::special::SPELL_OF_RETURN))
        else {
            return;
        };
        let due = w
            .casting_skill(&view.content().rules)
            .min(casting.remaining)
            .max(0);
        let short = due - view.treasury().mana.floor();
        if !w.is_banished() || short <= 0 {
            return;
        }
        // Alchemy returns a percent of what is put in, so the gold needed is mana over that.
        let percent = w
            .stat(
                &view.content().rules,
                f4x_def::rules::Stat::AlchemyPercent,
                50,
            )
            .max(1);
        (short * 100 / percent).min(view.treasury().gold.floor())
    };
    if gold > 0 {
        ai.order(
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
        ai.order(game, Command::CancelCast { player: ai.player }, events);
    }
}

// ---- the power split ------------------------------------------------------------

/// The least the shares of the split are.
fn least_shares(levers: &SplitLevers) -> [i32; 3] {
    [levers.least_mana, levers.least_research, levers.least_skill]
}

/// Move `amount` points of the split from one share to another, as far as the first can give.
fn shift(split: &mut [i32; 3], least: [i32; 3], from: usize, to: usize, amount: i32) {
    let moved = amount.min(split[from] - least[from]).max(0);
    split[from] -= moved;
    split[to] += moved;
}

/// The split a wizard wants before the upkeep is looked at: the balanced base, moved by the phase
/// of the game, the wizard's goal (`split_shift`, `lean_mana`) and the reserve it holds.
fn split_target(view: &PlayerView, traits: &crate::objective::Traits, round: u32) -> [i32; 3] {
    const MANA: usize = 0;
    const RESEARCH: usize = 1;
    const SKILL: usize = 2;
    let levers = view.levers().split;
    let (least, step) = (least_shares(&levers), levers.split_step);
    let mut t = [levers.base_mana, levers.base_research, levers.base_skill];
    match crate::phase::phase(view, round) {
        // Growing: spells and their research come first, little is cast.
        crate::phase::Phase::Build => shift(&mut t, least, MANA, RESEARCH, 10),
        crate::phase::Phase::Secure => {}
        // Winning: the spells are known, now they are cast.
        crate::phase::Phase::Win => {
            shift(&mut t, least, RESEARCH, MANA, 15);
            shift(&mut t, least, RESEARCH, SKILL, 5);
        }
    }
    // A goal that likes magic casts more; one that spends its mana to the bone researches more.
    for _ in 0..traits.split_shift {
        shift(&mut t, least, RESEARCH, MANA, step);
    }
    if traits.lean_mana {
        shift(&mut t, least, MANA, RESEARCH, step);
    }
    // A big reserve is not spent for the lack of power but for the lack of skill and spells.
    let reserve = view.treasury().mana.floor();
    if reserve > levers.rich_reserve {
        shift(&mut t, least, MANA, RESEARCH, 10);
    }
    t
}

fn split(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let (casting_mastery, racing) = {
        let view = PlayerView::new(game, me);
        let Some(m) = view.wizard() else {
            return;
        };
        let is_mastery = |s| kind(view.spell(s)) == Kind::Mastery;
        (
            m.casting().is_some_and(|c| is_mastery(c.spell)),
            m.researching().is_some_and(is_mastery),
        )
    };
    let levers = game.levers().split;
    if casting_mastery {
        let shares = (
            levers.mastery_mana,
            levers.mastery_research,
            levers.mastery_skill,
        );
        ai.order(game, set_split(me, shares), events);
        return;
    }
    // Nothing left to research: research points go to waste, so the power goes to mana and to the
    // casting skill that spends it (a wizard that knows every spell, seen in the soaks, put 70% of
    // 378 power into research and had 38 mana a turn).
    let nothing_to_research = {
        let view = PlayerView::new(game, me);
        view.wizard()
            .is_some_and(|m| m.researching().is_none() && !m.known().is_empty())
            && view.research_choices().is_empty()
    };
    if nothing_to_research {
        let shares = (levers.done_mana, levers.done_research, levers.done_skill);
        ai.order(game, set_split(me, shares), events);
        return;
    }
    if racing {
        let shares = (
            levers.racing_mana,
            levers.racing_research,
            levers.racing_skill,
        );
        ai.order(game, set_split(me, shares), events);
        return;
    }
    let mut target = {
        let view = PlayerView::new(game, me);
        split_target(&view, &crate::objective::traits(&view), game.round())
    };
    // The income must cover the upkeep of the lasting spells and creatures (a reserve pays for a
    // small deficit for a long while): mana takes from research first, then from skill.
    let mut chosen = target;
    for _ in 0..40 {
        chosen = target;
        game.apply(me, set_split(me, as_split(chosen)));
        let view = PlayerView::new(game, me);
        let income = view.income();
        let deficit_allowed = f4x_def::half_int::HalfInt::from_int(
            view.treasury().mana.floor().max(0) / levers.mana_horizon.max(1),
        );
        if income.split.mana - income.mana_upkeep + deficit_allowed
            >= f4x_def::half_int::HalfInt::ZERO
            || (target[1] == 0 && target[2] <= levers.least_skill)
        {
            break;
        }
        if target[1] > 0 {
            shift(&mut target, least_shares(&levers), 1, 0, levers.split_step);
        } else {
            shift(&mut target, least_shares(&levers), 2, 0, levers.split_step);
        }
    }
    ai.order(game, set_split(me, as_split(chosen)), events);
}

fn as_split(share: [i32; 3]) -> (u8, u8, u8) {
    (share[0] as u8, share[1] as u8, share[2] as u8)
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

/// Turns of research a candidate costs, with adjustments (cheaper when the AI has no spell of its
/// sort yet and the sort wins battles or fights, dearer when it has one).
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
    // Roads at no cost in moves are a big advantage, so the spell is researched early.
    if cat.0 == Kind::EnchantRoad {
        // Dearer when no road is ever built: only the roads of the world's start are left.
        score *= match (owned, crate::road::engineers_possible(view)) {
            (true, _) => 1.0,
            (false, true) => 0.5,
            (false, false) => 2.0,
        };
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
        Like::Holy => matches!(Some(def.realm.num()), Some(3 /* life */ | 5 /* arcane */)),
        Like::Curse => matches!(
            kind,
            Kind::CityCurse
                | Kind::WizardCurse
                | Kind::Disjunction
                | Kind::StackStrike
                | Kind::LandCurse
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
        ai.order(
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
    if paid.is_some_and(|s| {
        f4x_core::spell::hero::HeroSummon::Resurrection.is_in(game.content().rules.spells.get(s))
    }) {
        if let Some(kind) = game.units().ai_resurrection_pick() {
            let target = raise_target(&PlayerView::new(game, ai.player), kind);
            let command = Command::ResolveCast {
                player: ai.player,
                target,
            };
            ai.order(game, command, events);
        }
        return;
    }
    // The Spell of Return: the fortress goes to the biggest city (the game refuses a city that
    // is not the wizard's). Left unaimed it waited for ever, with no income all the while.
    if paid.is_some_and(|s| game.spell_has_hook(s, f4x_core::spell::special::SPELL_OF_RETURN)) {
        let city = {
            let view = PlayerView::new(game, ai.player);
            let mut cities = view.my_cities();
            cities.sort_by_key(|&(c, _)| {
                (
                    std::cmp::Reverse(view.city_economy(c).map_or(0, |e| e.figures)),
                    view.serial_of(c),
                )
            });
            cities.first().map(|&(c, _)| c)
        };
        if let Some(city) = city {
            let command = Command::ResolveCast {
                player: ai.player,
                target: Target::City(city),
            };
            ai.order(game, command, events);
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
        let k = kind(view.spell(spell));
        if aims_at_crossers(view.spell(spell), k) {
            cross_aim(ai, &view, spell, k)
        } else {
            pick_target(&view, spell)
        }
    };
    if let Some(target) = target {
        ai.order(
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
        Kind::UnitBuff => crate::spellhelper::best_buff(view, spell).map(|(u, _)| Target::Unit(u)),
        Kind::CityBuff => {
            // A city with an enemy stack by it first, then the fortress.
            let fortress = view.fortress();
            let mut cities = view.my_cities();
            cities.sort_by_key(|&(c, p)| {
                (
                    view.enemies_near(p, view.levers().shared.near_city_radius)
                        .is_empty(),
                    Some(p) != fortress,
                    view.serial_of(c),
                )
            });
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
        Kind::EnchantRoad => crate::enchant_road::target(view, spell),
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
        Kind::Lycanthropy => lycanthropy_target(view, spell).map(|(u, ..)| Target::Unit(u)),
        Kind::Mutate => {
            let mut units: Vec<hecs::Entity> = view
                .my_units()
                .into_iter()
                .filter(|&u| {
                    is_fighter(view, u)
                        && !view.unit_has_ability(
                            u,
                            f4x_core::rules::Ability::new(
                                view.content().rules.config.abilities.mutation.mutated,
                            ),
                        )
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
            .map(|u| recall_target(view, u))
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::Cure => hurt_stack_unit(view)
            .map(Target::Unit)
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::Stasis => threat_square(view)
            .map(Target::Tile)
            .filter(|&t| view.can_aim(spell, Some(t))),
        Kind::StackStrike => crate::intent::strike_square(view, spell),
        Kind::LandCurse => crate::intent::land_curse_target(view, spell),
        Kind::LandBoost => crate::intent::land_boost_target(view, spell),
        Kind::CityCurse => curse_city(view, spell).map(Target::City),
        Kind::Blast | Kind::WizardCurse => view
            .rivals_at_war()
            .into_iter()
            .map(Target::Player)
            .find(|&t| view.can_aim(spell, Some(t))),
        // With several summoning circles (`multiple_summoning_circles`) the city is chosen; with one,
        // no target is asked.
        Kind::Summon | Kind::SummonHero => summon_circle(view),
        _ => None,
    }
}

/// The summoning circle a creature, hero or company of mercenaries comes to: the one with the most
/// enemy stacks near it, the first made when none is threatened. `None` without a circle.
pub(crate) fn best_circle(view: &PlayerView) -> Option<Position> {
    view.wizard()?
        .summon_circles()
        .into_iter()
        .max_by_key(|&at| {
            (
                view.enemies_near(at, view.levers().shared.near_city_radius)
                    .len(),
                std::cmp::Reverse(view.city_at(at).map(|c| view.serial_of(c))),
            )
        })
}

/// The city of the circle a summoning is aimed at, when the wizard has several; `None` with one,
/// where no target is asked.
fn summon_circle(view: &PlayerView) -> Option<Target> {
    if view.wizard()?.summon_circles().len() < 2 {
        return None;
    }
    best_circle(view)
        .and_then(|at| view.city_at(at))
        .map(Target::City)
}

/// The aim of Resurrection at the dead hero `kind`: with several circles, the one it comes back to.
fn raise_target(view: &PlayerView, kind: usize) -> Target {
    match best_circle(view) {
        Some(at) if view.wizard().is_some_and(|w| w.summon_circles().len() > 1) => {
            Target::HeroAt(kind, at)
        }
        _ => Target::Hero(kind),
    }
}

/// The Plane Shift spell the wizard knows, if any.
pub(crate) fn plane_shift_spell(view: &PlayerView) -> Option<SpellId> {
    view.wizard()?
        .known()
        .iter()
        .copied()
        .find(|&s| kind(view.spell(s)) == Kind::PlaneShift)
}

/// The Astral Gate spell the wizard knows, if any.
pub(crate) fn astral_gate_spell(view: &PlayerView) -> Option<SpellId> {
    view.wizard()?
        .known()
        .iter()
        .copied()
        .find(|&s| kind(view.spell(s)) == Kind::Gate)
}

/// The square the army crosses by to attack `target` on the other plane, from the soldiers'
/// plane: an own city an Astral Gate stands in or can be raised in, or the square to cast Plane
/// Shift on, which is no city. The one with the shortest way for the last of the `soldiers` to it
/// and on from its other side to the target (one with a walk there first); `None` when no spell
/// or gate takes them across.
pub(crate) fn crossing_square(
    view: &PlayerView,
    soldiers: &[hecs::Entity],
    target: Position,
) -> Option<Position> {
    let shift = plane_shift_spell(view);
    let gate = astral_gate_spell(view);
    if shift.is_none() && gate.is_none() {
        return None;
    }
    let near: Vec<hecs::Entity> = soldiers
        .iter()
        .copied()
        .filter(|&u| {
            view.unit_position(u)
                .is_some_and(|p| p.plane != target.plane)
        })
        .collect();
    let cities = view.my_cities();
    let gated: Vec<Position> = cities
        .iter()
        .filter(|&&(c, p)| {
            p.plane != target.plane
                && view.city_at(mirror(p)).is_none()
                && gate.is_some_and(|spell| {
                    view.city_enchantments(c).contains(&spell)
                        || view.can_aim(spell, Some(Target::City(c)))
                })
        })
        .map(|&(_, p)| p)
        .collect();
    // The squares a Plane Shift could be cast on: round the soldiers, round the own cities and
    // round the square across from the target; the most promising by the crow's flight are
    // walked.
    let mut shifts: Vec<Position> = Vec::new();
    if shift.is_some() {
        let centres = near
            .iter()
            .filter_map(|&u| view.unit_position(u))
            .chain(cities.iter().map(|&(_, p)| p))
            .chain([mirror(target)])
            .filter(|p| p.plane != target.plane);
        for centre in centres {
            for p in crate::military::neighbours(view, centre)
                .into_iter()
                .chain([centre])
            {
                if !shifts.contains(&p) && crossing_stands(view, p) && view.city_at(p).is_none() {
                    shifts.push(p);
                }
            }
        }
        shifts.sort_by_key(|&p| {
            let to_it = near
                .iter()
                .filter_map(|&u| view.distance(view.unit_position(u)?, p))
                .max()
                .unwrap_or(u32::MAX);
            let on = view.distance(mirror(p), target).unwrap_or(u32::MAX);
            (to_it.saturating_add(on), p.y, p.x)
        });
        shifts.truncate(view.levers().magic.shift_squares);
    }
    gated
        .into_iter()
        .chain(shifts)
        .filter_map(|square| {
            let turns: Vec<u32> = near
                .iter()
                .filter_map(|&u| {
                    let from = view.unit_position(u)?;
                    Some(view.way(&[u], from, square)?.last().map_or(0, |w| w.1))
                })
                .collect();
            let last = turns.iter().copied().max()?;
            let walk = near.iter().find_map(|&u| {
                let way = view.way(&[u], mirror(square), target)?;
                Some(way.last().map_or(0, |w| w.1))
            });
            // With no walk from the other side (water in between), how far it is.
            let on = walk.or_else(|| view.distance(mirror(square), target))?;
            // The square that serves the most soldiers, then one the walk goes on from, then
            // the soonest.
            let left_behind = near.len() - turns.len();
            Some((
                (left_behind, walk.is_none(), last + on, square.y, square.x),
                square,
            ))
        })
        .min_by_key(|&(rank, _)| rank)
        .map(|(_, square)| square)
}

/// Whether the campaign can still cross by `square`: a tower or an own city, or a square with no
/// lair and no enemy on either plane and no city on the other.
pub(crate) fn crossing_stands(view: &PlayerView, square: Position) -> bool {
    if view.towers().iter().any(|&(p, _)| p == square) {
        return true;
    }
    if let Some(city) = view.city_at(square) {
        return view.city_owner(city) == Some(view.me()) && view.city_at(mirror(square)).is_none();
    }
    view.city_at(mirror(square)).is_none()
        && !view.has_lair(square)
        && !view.has_lair(mirror(square))
        && [square, mirror(square)]
            .into_iter()
            .all(|p| view.hostile_at(p).is_none_or(|u| u.is_empty()))
}

/// The city to raise an Astral Gate in: the army is gathered in the city it crosses from
/// (`Ai::crossing`) with the garrison and at least `min_shift` fighters over, its campaign is
/// on the other plane, and the gate is not there yet.
fn gate_target(ai: &Ai, view: &PlayerView, spell: SpellId) -> Option<Target> {
    let gather = ai.crossing?;
    let campaign = ai.campaign?;
    let city = view.city_at(gather)?;
    if campaign.plane == gather.plane || view.city_enchantments(city).contains(&spell) {
        return None;
    }
    let fighters = view
        .garrison(gather)
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .count();
    (fighters >= crate::view::garrison_wanted(view, gather) + view.levers().magic.min_shift)
        .then_some(Target::City(city))
        .filter(|&t| view.can_aim(spell, Some(t)))
}

/// The city to raise an Earth Gate in: with a campaign on, the own city nearest the rally point
/// of the army (`assembly.rs`; without one nearest the crossing or the target) first, then the own
/// city on its plane that is farthest from it, at least `gate_far` away, the largest of those (it
/// makes the most troops); a unit goes through a pair of gates when that is quicker
/// (`assembly::Travel`). At most `gates_max` cities have one.
fn earth_gate_target(ai: &Ai, view: &PlayerView, spell: SpellId) -> Option<Target> {
    let campaign = ai.campaign?;
    let cities = view.my_cities();
    let positions: Vec<Position> = cities.iter().map(|&(_, p)| p).collect();
    let capital = view.fortress().or_else(|| positions.first().copied())?;
    let near = [
        crate::assembly::rally_of(ai, campaign),
        ai.crossing,
        Some(campaign),
    ]
    .into_iter()
    .flatten()
    .find(|p| positions.iter().any(|c| c.plane == p.plane));
    let rally = near
        .and_then(|near| {
            positions
                .iter()
                .copied()
                .filter(|c| c.plane == near.plane)
                .min_by_key(|&c| (crate::view::distance(view, c, near), c.y, c.x))
        })
        .unwrap_or(capital);
    let gated = |c: hecs::Entity| view.city_enchantments(c).contains(&spell);
    if cities.iter().filter(|&&(c, _)| gated(c)).count() >= view.levers().magic.gates_max {
        return None;
    }
    let rally_city = cities.iter().find(|&&(_, p)| p == rally).map(|&(c, _)| c)?;
    let pick = if gated(rally_city) {
        let mut far: Vec<(u32, u32, hecs::Entity)> = cities
            .iter()
            .filter(|&&(c, p)| p.plane == rally.plane && !gated(c) && c != rally_city)
            .filter_map(|&(c, p)| {
                let d = crate::view::distance(view, p, rally)?;
                let size = view.city_report(c).map_or(0, |r| r.figures);
                (d >= view.levers().magic.gate_far).then_some((size, d, c))
            })
            .collect();
        far.sort_by_key(|&(size, d, c)| {
            (
                std::cmp::Reverse(size),
                std::cmp::Reverse(d),
                view.serial_of(c),
            )
        });
        far.first().map(|&(_, _, c)| c)?
    } else {
        // A far city must exist to make the pair worth it.
        let any_far = cities.iter().any(|&(c, p)| {
            c != rally_city
                && p.plane == rally.plane
                && crate::view::distance(view, p, rally)
                    .is_some_and(|d| d >= view.levers().magic.gate_far)
        });
        if !any_far {
            return None;
        }
        rally_city
    };
    Some(Target::City(pick)).filter(|&t| view.can_aim(spell, Some(t)))
}

/// The unit to cast Plane Shift on: at least `min_shift` fighters are gathered on the square the
/// army crosses by (`Ai::crossing`), which is no city, and the campaign is on the other plane. The
/// whole stack goes, so the strongest unit is named.
fn shift_target(ai: &Ai, view: &PlayerView, spell: SpellId) -> Option<Target> {
    let gather = ai.crossing?;
    let campaign = ai.campaign?;
    if campaign.plane == gather.plane || view.city_at(gather).is_some() {
        return None;
    }
    let fighters: Vec<hecs::Entity> = view
        .garrison(gather)
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .collect();
    if fighters.len() < view.levers().magic.min_shift {
        return None;
    }
    fighters
        .into_iter()
        .map(|u| (unit_strength(view, u), u))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, u)| Target::Unit(u))
        .filter(|&t| view.can_aim(spell, Some(t)))
}

/// Whether the spell goes first to a unit that waits for a ship: the walking spells, and Flight
/// (a `UnitBuff`).
fn aims_at_crossers(def: &SpellDef, k: Kind) -> bool {
    k == Kind::Cross || (k == Kind::UnitBuff && grants_any(def, &[FLYING]))
}

/// The aim of a spell for [`aims_at_crossers`]: a unit that waits for a ship comes first, for
/// Flight as for the walking spells; Flight with none waiting goes where a buff does.
fn cross_aim(ai: &Ai, view: &PlayerView, spell: SpellId, k: Kind) -> Option<Target> {
    cross_target(ai, view, spell).or_else(|| {
        (k == Kind::UnitBuff)
            .then(|| pick_target(view, spell))
            .flatten()
    })
}

/// The unit a spell that takes walkers over the sea is cast on: one that waits for a ship with
/// none assigned (`Registry::waiting_for_ship`: the dispatcher has had its turn with a ship
/// free, so none was) and that the spell would still help, that is, one that cannot cross by
/// itself or in its stack. Wind Walking carries the stack it is in, so it goes to the unit with
/// the most waiting mates on its square; Water Walking and Flight carry the one unit, the oldest
/// of those that wait.
fn cross_target(ai: &Ai, view: &PlayerView, spell: SpellId) -> Option<Target> {
    let waiting = ai.roles.waiting_for_ship();
    if waiting.is_empty() {
        return None;
    }
    if raises_island(view.spell(spell)) {
        return island_target(ai, view, spell, &waiting);
    }
    let mine = view.my_units();
    let wind = grants_any(view.spell(spell), &[74 /* wind_walking */]);
    let mut stands: Vec<(Position, hecs::Entity)> = waiting
        .iter()
        .filter_map(|&(s, _)| {
            let u = mine.iter().copied().find(|&u| view.serial_of(u) == s)?;
            Some((view.unit_position(u)?, u))
        })
        .filter(|&(_, u)| view.is_walker_in_stack(u) && !view.unit_enchantments(u).contains(&spell))
        .collect();
    let mates: Vec<usize> = stands
        .iter()
        .map(|&(p, _)| stands.iter().filter(|&&(q, _)| q == p).count())
        .collect();
    let mut ranked: Vec<(usize, _, hecs::Entity)> = stands
        .drain(..)
        .zip(mates)
        .map(|((_, u), n)| (if wind { n } else { 0 }, view.serial_of(u), u))
        .collect();
    ranked.sort_by_key(|&(n, serial, _)| (std::cmp::Reverse(n), serial));
    ranked
        .into_iter()
        .map(|(_, _, u)| Target::Unit(u))
        .find(|&t| view.can_aim(spell, Some(t)))
}

/// The sea square a floating island is raised on for the units that wait for a ship: the strait
/// between the land of the first of them (the oldest) and the land it goes to, where the ship
/// that fetches them is wanted (`bridge::strait_between`). Nothing while a ship of the wizard is
/// free or has no work yet (an island raised last turn, a galley just built): the dispatcher
/// gives it the waiting units first.
fn island_target(
    ai: &Ai,
    view: &PlayerView,
    spell: SpellId,
    waiting: &[(f4x_core::world::serial::Serial, Position)],
) -> Option<Target> {
    let mine = view.my_units();
    let spare = mine.iter().any(|&u| {
        crate::view::is_ship(view, u) && {
            let s = view.serial_of(u);
            ai.roles.is(s, crate::roles::Role::TransportFree) || !ai.roles.has_role(s)
        }
    });
    if spare {
        return None;
    }
    let mut waiting = waiting.to_vec();
    waiting.sort_by_key(|&(s, _)| s);
    waiting.into_iter().find_map(|(s, to)| {
        let u = mine.iter().copied().find(|&u| view.serial_of(u) == s)?;
        let sea = crate::bridge::strait_between(view, view.unit_position(u)?, to)?;
        Some(Target::Tile(sea)).filter(|&t| view.can_aim(spell, Some(t)))
    })
}

/// What the island costs each turn to keep, in mana (the spell itself has no upkeep: the island
/// is a summoned unit with its own).
fn island_upkeep(view: &PlayerView) -> i32 {
    let rules = &view.content().rules;
    rules
        .units
        .id_numbered(190 /* floating_island */)
        .map_or(0, |id| rules.units.get(id).upkeep.mana as i32)
}

/// The square of the strongest enemy stack within `near_city_radius` of an own city that is stronger
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
        for (square, units) in view.enemies_near(city, view.levers().shared.near_city_radius) {
            let strength = fighting_strength(view, &units);
            if strength > held && best.is_none_or(|(b, _)| strength > b) {
                best = Some((strength, square));
            }
        }
    }
    best.map(|(_, square)| square)
}

/// The army up to which the AI summons creatures.
fn army_cap(view: &PlayerView) -> usize {
    let magic = view.levers().magic;
    magic.army_cap_base + magic.army_cap_city * view.my_cities().len()
}

/// Is a spell of this cost worth beginning for a wizard that pays `power` for it each turn?
fn quick_enough(levers: &StrategyAiRules, cost: i32, power: i32) -> bool {
    let magic = levers.magic;
    cost <= magic.dear_spell || cost <= magic.max_casting_turns * power
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
        .filter_map(|(c, _, _)| view.city_report(c).map(|r| (r.figures, c)))
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
    best.filter(|&(n, _)| n >= view.levers().magic.explore_floor)
        .map(|(_, p)| p)
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
        if total == 0 || missing * 100 / total < view.levers().magic.cure_percent {
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
    let circles = view.wizard()?.summon_circles();
    if circles.is_empty() {
        return None;
    }
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
        // Near any circle it is not stranded.
        if circles.iter().any(|&circle| {
            view.distance(at, circle)
                .is_some_and(|d| d < view.levers().magic.recall_distance)
        }) {
            continue;
        }
        view.sort_by_serial(&mut units);
        let (mut missing, mut total) = (0u32, 0u32);
        for &u in &units {
            let (hits, max) = view.unit_hits(u);
            missing += max.saturating_sub(hits);
            total += max;
        }
        if total == 0 || missing * 100 / total < view.levers().magic.recall_percent {
            continue;
        }
        if best.is_none_or(|(m, _)| missing > m) {
            best = Some((missing, units[0]));
        }
    }
    best.map(|(_, u)| u)
}

/// The aim of Word of Recall at the unit: with several summoning circles, the one nearest to it.
fn recall_target(view: &PlayerView, unit: hecs::Entity) -> Target {
    let circles = view
        .wizard()
        .map(|w| w.summon_circles())
        .unwrap_or_default();
    let from = view.unit_position(unit);
    match (circles.len() > 1, from) {
        (true, Some(from)) => circles
            .into_iter()
            .min_by_key(|&c| view.distance(from, c).unwrap_or(u32::MAX))
            .map_or(Target::Unit(unit), |at| Target::UnitAt(unit, at)),
        _ => Target::Unit(unit),
    }
}

/// Whether the AI can carry the lasting spell's upkeep on its mana income.
fn upkeep_ok(view: &PlayerView, def: &SpellDef) -> bool {
    let income = view.income();
    income.split.mana - income.mana_upkeep - def.upkeep as i32 >= 0
}

/// Resurrection: the original weighs it highest when the wizard has a free place for a hero and
/// one lies dead. The last of the raisable heroes is raised (`ai_resurrection_pick`); a cast that
/// is paid over turns picks its hero when it is done (`resolve_paid`). Returns whether it began: a
/// cast the game refuses is not begun, and the wizard goes on to its other spells.
fn resurrect(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    let kind = {
        let Some(spell) =
            f4x_core::spell::hero::HeroSummon::Resurrection.spell_in(&game.content().rules)
        else {
            return false;
        };
        let knows = game
            .wizard(me)
            .is_some_and(|m| m.casting().is_none() && m.knows(spell));
        if !knows || game.units().hero_spell_blocked(me, spell).is_some() {
            return false;
        }
        let mana = game.player(me).map_or(0, |p| p.treasury().mana.floor());
        let cost = game.casting_cost(me, spell, false);
        // The same gate as the other spells: paid from the reserve, not too many turns.
        let per_turn = mana / 10 + game.income(me).split.mana.floor();
        let levers = game.levers();
        if mana_before(levers, cost) > mana || cost / levers.magic.most_paying_turns > per_turn {
            return false;
        }
        game.units().ai_resurrection_pick()
    };
    let Some(kind) = kind else {
        return false;
    };
    let Some(spell) =
        f4x_core::spell::hero::HeroSummon::Resurrection.spell_in(&game.content().rules)
    else {
        return false;
    };
    let target = raise_target(&PlayerView::new(game, me), kind);
    let command = Command::CastSpell {
        extra: 0,
        player: me,
        spell,
        target: Some(target),
    };
    ai.send(game, command, events).is_ok()
}

/// Where the spell would go in the situation of the wizard, if anywhere.
fn aim_for(ai: &Ai, view: &PlayerView, spell: SpellId, k: Kind) -> Option<Target> {
    let def = view.spell(spell);
    if k == Kind::PlaneShift {
        shift_target(ai, view, spell)
    } else if k == Kind::Gate {
        gate_target(ai, view, spell)
    } else if k == Kind::EarthGate {
        earth_gate_target(ai, view, spell)
    } else if aims_at_crossers(def, k) {
        cross_aim(ai, view, spell, k)
    } else {
        pick_target(view, spell)
    }
}

/// The kinds of spell that answer a situation of the map (a target exists or it does not), by the
/// name the soak prints. A situation for which the wizard knows no spell of the kind is a want
/// it cannot meet (`Ai::unmet_spell_wants`).
const SITUATION_KINDS: &[(Kind, &str)] = &[
    (Kind::Explore, "explore"),
    (Kind::EnchantRoad, "enchant road"),
    (Kind::Cure, "cure"),
    (Kind::Disenchant, "disenchant"),
    (Kind::Recall, "recall"),
    (Kind::PlaneShift, "plane shift"),
    (Kind::Stasis, "stasis"),
    (Kind::Cross, "cross water"),
    (Kind::Disjunction, "disjunction"),
    (Kind::StackStrike, "strike a stack"),
    (Kind::LandCurse, "curse land"),
    (Kind::LandBoost, "boost land"),
    (Kind::CityCurse, "curse a city"),
    (Kind::Blast, "spell blast"),
    (Kind::Gate, "astral gate"),
];

/// Count the situations the wizard could use a spell for and knows none of the kind: for each kind
/// of `SITUATION_KINDS` that no known spell has, a spell of that kind (the first in the rules)
/// is aimed as if it were known, and a target counts as one want, once a turn.
fn note_unmet(ai: &Ai, view: &PlayerView, world: &World, known: &[(Kind, i32, SpellId)]) {
    for &(k, name) in SITUATION_KINDS {
        if known.iter().any(|&(have, ..)| have == k) {
            continue;
        }
        let Some(spell) = world
            .content()
            .rules
            .spells
            .iter()
            .map(|(id, _)| id)
            .find(|&id| kind(view.spell(id)) == k)
        else {
            continue;
        };
        if aim_for(ai, view, spell, k).is_some() {
            ai.note_unmet_spell(name);
        }
    }
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
        // A banished wizard casts nothing but the Spell of Return, which the game begins and keeps
        // casting for it (`World::resume_return`).
        if magic.casting().is_some() || magic.is_banished() {
            return;
        }
        let mana = view.treasury().mana.floor();
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
        note_unmet(ai, &view, game, &options);
        let traits = crate::objective::traits(&view);
        // The intents in the order the wizard's objective ranks them; within an intent the helper's
        // rating puts the best spell first.
        let rated: std::collections::HashMap<SpellId, i64> = options
            .iter()
            .map(|&(k, _, s)| (s, (crate::spellhelper::rate(&view, s, k) * 1000.0) as i64))
            .collect();
        // A spirit for a node that is open or being cleared comes before everything else.
        let spirit = crate::meld::spirit_wanted(ai, &view, game.round());
        let priority_spirit = |s: SpellId| spirit && summons_melder(&view, view.spell(s));
        options.sort_by_key(|&(k, _cost, s)| {
            (
                !priority_spirit(s),
                cast_rank(k, &traits),
                std::cmp::Reverse(rated[&s]),
                s,
            )
        });
        let mut found = None;
        // Saving up for a summon the pool cannot pay yet: the cheap lasting spells wait, or they
        // take the mana (and then the income, as upkeep) every turn and no summon is ever begun.
        let net = (income.split.mana - income.mana_upkeep).floor();
        let mut saving = false;
        for (k, cost, spell) in options {
            let def = view.spell(spell);
            if saving
                && matches!(
                    k,
                    Kind::UnitBuff
                        | Kind::Cross
                        | Kind::CityBuff
                        | Kind::GlobalBuff
                        | Kind::Mutate
                        | Kind::Lycanthropy
                        | Kind::CityCurse
                        | Kind::WizardCurse
                        | Kind::Disjunction
                        | Kind::WorldSpell
                        | Kind::LandCurse
                        | Kind::LandBoost
                )
            {
                continue;
            }
            // Paid for from the reserve, over as many turns as the casting skill needs, with
            // a third of the mana left over for the upkeep. The Spell of Mastery goes on until it
            // is paid whatever the reserve.
            // Summoned creatures and lasting spells cost mana every turn: not when the upkeep
            // already takes more than half of what the power makes (the split then leaves research
            // starved).
            let room = magic.power_split().mana <= 55;
            let target = aim_for(ai, &view, spell, k);
            let now = mana_before(view.levers(), cost) <= mana;
            let wanted = match k {
                Kind::Mastery => true,
                // Summoned creatures cost upkeep mana: only while the army is thin.
                Kind::Summon if priority_spirit(spell) => now && view.can_aim(spell, target),
                Kind::Summon => {
                    // A conqueror in the late game summons whatever the mana pays for and the
                    // upkeep allows: the army cap is for a wizard that is still building up.
                    let conquering = crate::phase::phase(&view, game.round())
                        == crate::phase::Phase::Win
                        && crate::phase::goal(&view) == crate::phase::Goal::Conquest;
                    now && (room || conquering)
                        && (fighters < army_cap(&view)
                            || (conquering && upkeep_ok(&view, def)))
                        // The game makes room on a full square, or refuses the spell.
                        && view.can_aim(spell, target)
                }
                // There is no cap on heroes: only the supply of heroes (`can_aim` is refused
                // when none is left to send) and the mana limit it.
                Kind::SummonHero => {
                    now && room && view.hero_to_summon(spell) && view.can_aim(spell, target)
                }
                Kind::UnitBuff => {
                    now && room && fighters >= 3 && upkeep_ok(&view, def) && target.is_some()
                }
                // Units wait on the shore for it: paid from what is in the pool, without the third
                // that the other spells leave over (the wizards' pools stay below it for hundreds
                // of turns, and the units wait just as long).
                Kind::Cross => {
                    let island = if raises_island(def) {
                        island_upkeep(&view)
                    } else {
                        0
                    };
                    cost <= mana
                        && room
                        && upkeep_ok(&view, def)
                        && view.income().split.mana - view.income().mana_upkeep - island >= 0
                        && target.is_some()
                }
                Kind::Mutate => now && fighters >= 3 && target.is_some(),
                Kind::Lycanthropy => now && fighters >= 3 && target.is_some(),
                Kind::CityBuff => now && room && upkeep_ok(&view, def) && target.is_some(),
                Kind::GlobalBuff => {
                    now && room
                        && mana - cost >= view.levers().shared.mana_reserve
                        && upkeep_ok(&view, def)
                        && !magic.has_global(spell)
                        && crate::intent::global_wanted(&view, def)
                }
                Kind::Explore
                | Kind::EnchantRoad
                | Kind::Cure
                | Kind::Disenchant
                | Kind::Recall
                | Kind::PlaneShift
                | Kind::Blast
                | Kind::StackStrike
                | Kind::Stasis => now && target.is_some(),
                Kind::Gate | Kind::EarthGate => {
                    now && room && upkeep_ok(&view, def) && target.is_some()
                }
                Kind::LandCurse | Kind::LandBoost => {
                    now && room
                        && mana - cost >= view.levers().shared.mana_reserve
                        && target.is_some()
                }
                // Blows at an enemy at war, from mana the AI can spare.
                Kind::CityCurse | Kind::WizardCurse | Kind::Disjunction => {
                    now && room
                        && mana - cost >= view.levers().shared.mana_reserve
                        && target.is_some()
                }
                Kind::WorldSpell => {
                    now && room
                        && mana - cost >= view.levers().shared.mana_reserve
                        && upkeep_ok(&view, def)
                        && !magic.has_global(spell)
                        && crate::intent::global_wanted(&view, def)
                }
                Kind::Other => false,
            };
            if !wanted
                && matches!(k, Kind::Summon | Kind::SummonHero)
                && mana_before(view.levers(), cost) > mana
                && mana_before(view.levers(), cost)
                    <= mana + view.levers().magic.saving_turns * net.max(0)
                && room
                && view.can_aim(spell, target)
                && (k == Kind::SummonHero || fighters < army_cap(&view))
            {
                saving = true;
            }
            // Not too many turns of what the AI can put towards it.
            let per_turn = mana / 10 + income.split.mana.floor();
            if wanted
                && (k == Kind::Mastery
                    || (cost / view.levers().magic.most_paying_turns <= per_turn
                        && quick_enough(view.levers(), cost, view.casting_power())))
            {
                found = Some((spell, target));
                break;
            }
        }
        found
    };
    if let Some((spell, target)) = choice {
        ai.order(
            game,
            Command::CastSpell {
                extra: 0,
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
    use crate::testworld::levers;
    use crate::testworld::{at, fresh_game, game, shipped, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
    use f4x_core::diplomacy::DiplomacyCommand;
    use f4x_core::player::{PlayerId, Treasury};
    use f4x_core::wizard::objective::Objective;
    use f4x_def::half_int::HalfInt;
    use f4x_def::rules::RealmId;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A wizard of the fresh game that knows the dearest summon the rules have, with the given
    /// gold and mana.
    fn summoner(gold: i32, mana: i32) -> Result<(Game, i32), Box<dyn std::error::Error>> {
        let mut w = fresh_game()?;
        let summon = w
            .content()
            .rules
            .spells
            .iter()
            .filter(|(_, d)| kind(d) == Kind::Summon)
            .map(|(id, _)| id)
            .next()
            .ok_or("a summon")?;
        w.cheat_learn_spell(P1, summon);
        let t = w.treasury_mut(P1).ok_or("a treasury")?;
        t.gold = HalfInt::from_int(gold);
        t.mana = HalfInt::from_int(mana);
        let wanted = mana_wanted(&PlayerView::new(&w, P1), &[], w.round());
        Ok((w, wanted))
    }

    #[test]
    fn a_wizard_with_much_gold_buys_the_mana_for_its_summon_and_no_more() -> TestResult {
        let (mut w, wanted) = summoner(50_000, 0)?;
        assert!(wanted > 0, "the summon can be aimed");
        alchemy(&Ai::new(P1), &mut w, &mut Vec::new());
        let t = w.treasury_mut(P1).ok_or("a treasury")?;
        assert!(
            t.mana.floor() >= wanted,
            "mana {} for {wanted}",
            t.mana.floor()
        );
        assert!(t.mana.floor() < wanted + 3, "no more than the summon needs");
        assert!(t.gold.floor() > 40_000, "most of the gold is kept");
        Ok(())
    }

    #[test]
    fn a_wizard_with_enough_mana_buys_none() -> TestResult {
        let (mut w, wanted) = summoner(50_000, 5_000)?;
        assert!(wanted < 5_000);
        alchemy(&Ai::new(P1), &mut w, &mut Vec::new());
        let t = w.treasury_mut(P1).ok_or("a treasury")?;
        assert_eq!(t.gold.floor(), 50_000);
        assert_eq!(t.mana.floor(), 5_000);
        Ok(())
    }

    #[test]
    fn a_wizard_with_little_gold_keeps_it() -> TestResult {
        let mut w = fresh_game()?;
        let ai = Ai::new(P1);
        w.treasury_mut(P1).ok_or("a treasury")?.gold = HalfInt::from_int(900);
        alchemy(&ai, &mut w, &mut Vec::new());
        assert_eq!(w.treasury_mut(P1).ok_or("a treasury")?.gold.floor(), 900);
        Ok(())
    }

    fn spell(id: u16) -> Result<SpellId, String> {
        shipped()
            .rules
            .spells
            .id_numbered(id)
            .ok_or(format!("{id} is shipped"))
    }

    fn treasury(w: &mut Game, player: PlayerId) -> Result<&mut Treasury, &'static str> {
        w.treasury_mut(player).ok_or("the player has a treasury")
    }

    #[test]
    fn an_open_node_and_no_spirit_make_the_spirit_the_first_summon() -> TestResult {
        use f4x_core::feature::entity::Placement;
        let mut w = fresh_game()?;
        let spirit = spell(201 /* magic_spirit */)?;
        w.cheat_learn_spell(P1, spirit);
        let dearest = w
            .content()
            .rules
            .spells
            .iter()
            .filter(|(id, d)| *id != spirit && kind(d) == Kind::Summon)
            .map(|(id, _)| id)
            .next()
            .ok_or("another summon")?;
        w.cheat_learn_spell(P1, dearest);
        treasury(&mut w, P1)?.mana = HalfInt::from_int(5_000);
        let mut ai = Ai::new(P1);
        ai.frontier_closed = true;
        assert!(!crate::meld::spirit_wanted(
            &ai,
            &PlayerView::new(&w, P1),
            w.round()
        ));
        let feature = w
            .content()
            .features
            .id_numbered(20 /* sorcery_node */)
            .ok_or("a node feature")?;
        let circle = PlayerView::new(&w, P1)
            .wizard()
            .and_then(|m| m.summon_at())
            .ok_or("a summoning circle")?;
        let spot = at(circle.x + 2, circle.y);
        w.explore(P1, spot, HalfInt::from_int(5));
        w.spawn_feature(Placement {
            at: spot,
            feature,
            node: Some(f4x_core::feature::node::Node {
                realm: f4x_def::rules::RealmId::new(1),
                power: 5,
                aura: vec![(spot.x, spot.y)],
                guardian: false,
                warped: false,
            }),
            lair: None,
        });
        assert!(crate::meld::spirit_wanted(
            &ai,
            &PlayerView::new(&w, P1),
            w.round()
        ));
        cast(&ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P1);
        let casting = view
            .wizard()
            .and_then(|m| m.casting())
            .ok_or("a spell is being cast")?;
        assert_eq!(casting.spell, spirit);
        Ok(())
    }

    #[test]
    fn an_objective_moves_the_kinds_it_weights_up_the_order_of_casting() {
        use crate::objective::traits_of;
        let pragmatist = traits_of(crate::testworld::levers(), Some(Objective::Pragmatist));
        let militarist = traits_of(crate::testworld::levers(), Some(Objective::Militarist));
        let perfectionist = traits_of(crate::testworld::levers(), Some(Objective::Perfectionist));
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

    fn kind_of(id: u16) -> Result<Kind, String> {
        let rules = &shipped().rules;
        Ok(kind(rules.spells.get(spell(id)?)))
    }

    #[test]
    fn spells_are_sorted_into_the_kinds_the_ai_casts() -> TestResult {
        let cases = [
            (213 /* spell_of_mastery */, Kind::Mastery),
            (6 /* war_bears */, Kind::Summon),
            (4 /* giant_strength */, Kind::UnitBuff),
            (8 /* water_walking */, Kind::Cross),
            (63 /* wind_walking */, Kind::Cross),
            (43 /* floating_island */, Kind::Cross),
            (56 /* flight */, Kind::UnitBuff),
            (130 /* heroism */, Kind::UnitBuff),
            (21 /* elemental_armor */, Kind::UnitBuff),
            (12 /* natures_eye */, Kind::CityBuff),
            (38 /* herb_mastery */, Kind::GlobalBuff),
            (10 /* earth_lore */, Kind::Explore),
            (55 /* enchant_road */, Kind::EnchantRoad),
            (19 /* natures_cures */, Kind::Cure),
            (204 /* disenchant_area */, Kind::Disenchant),
            (52 /* disenchant_true */, Kind::Disenchant),
            (93 /* chaos_channels */, Kind::Mutate),
            (173 /* lycanthropy */, Kind::Lycanthropy),
            (47 /* word_of_recall */, Kind::Recall),
            (210 /* disjunction */, Kind::Disjunction),
            (76 /* spell_binding */, Kind::Disjunction),
            (189 /* famine */, Kind::CityCurse),
            (26 /* earthquake */, Kind::CityCurse),
            (191 /* cruel_unminding */, Kind::WizardCurse),
            (113 /* meteor_storm */, Kind::WorldSpell),
            (120 /* armageddon */, Kind::WorldSpell),
            (80 /* time_stop */, Kind::WorldSpell),
            (114 /* great_wasting */, Kind::WorldSpell),
            (40 /* natures_wrath */, Kind::WorldSpell),
            (58 /* spell_blast */, Kind::Blast),
            (68 /* stasis */, Kind::Stasis),
            (18 /* transmute */, Kind::Other),
            (132 /* plane_shift */, Kind::PlaneShift),
            (34 /* nature_awareness */, Kind::GlobalBuff),
            (116 /* chaos_surge */, Kind::GlobalBuff),
            (79 /* suppress_magic */, Kind::WorldSpell),
            (156 /* life_force */, Kind::WorldSpell),
            (199 /* death_wish */, Kind::WorldSpell),
            (135 /* planar_seal */, Kind::Other),
            (139 /* heavenly_light */, Kind::CityBuff),
            (155 /* consecration */, Kind::CityBuff),
            (3 /* wall_of_stone */, Kind::CityBuff),
            (25 /* ice_storm */, Kind::StackStrike),
            (107 /* fire_storm */, Kind::StackStrike),
            (187 /* black_wind */, Kind::StackStrike),
            (85 /* corruption */, Kind::LandCurse),
            (186 /* warp_node */, Kind::LandCurse),
            (98 /* raise_volcano */, Kind::LandBoost),
            (153 /* astral_gate */, Kind::Gate),
            (37 /* earth_gate */, Kind::EarthGate),
        ];
        for (id, expected) in cases {
            assert_eq!(kind_of(id)?, expected, "{id}");
        }
        Ok(())
    }

    #[test]
    fn what_the_ai_cannot_use_is_left_alone() -> TestResult {
        // A curse on the whole world, a bolt for battle, a spell that is not done yet.
        for id in [83 /* fire_bolt */, 1 /* earth_to_mud */] {
            assert_eq!(kind_of(id)?, Kind::Other, "{id}");
        }
        Ok(())
    }

    #[test]
    fn a_banished_wizard_does_not_begin_resurrection_and_so_goes_on_to_other_spells() -> TestResult
    {
        let mut w = fresh_game()?;
        let ai = Ai::new(P1);
        let resurrection = f4x_def::rules::SpellId::new(133) /* resurrection */;
        treasury(&mut w, P1)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        let hero = w.hire_hero(P1, 1).map_err(|e| format!("{e:?}"))?;
        w.cheat_slay_unit(hero);
        w.cheat_learn_spell(P1, resurrection);
        w.wizard_mut(P1)
            .ok_or("player 1 is a wizard")?
            .put(Some(f4x_core::wizard::fortress::Banishment { by: None }));
        let mut events = Vec::new();
        assert!(!resurrect(&ai, &mut w, &mut events), "{events:?}");
        assert!(w
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .casting()
            .is_none());
        Ok(())
    }

    #[test]
    fn a_wizard_that_knows_resurrection_raises_the_last_dead_hero() -> TestResult {
        let mut w = fresh_game()?;
        let ai = Ai::new(P1);
        let resurrection = f4x_def::rules::SpellId::new(133) /* resurrection */;
        treasury(&mut w, P1)?.gold = f4x_def::half_int::HalfInt::from_int(50_000);
        treasury(&mut w, P1)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        for k in [1, 4] {
            let hero = w
                .hire_hero(P1, k)
                .map_err(|e| format!("hiring hero {k}: {e:?}"))?;
            w.cheat_slay_unit(hero);
        }
        // Not known: nothing happens. Known: the spell is begun for the last dead hero.
        let mut events = Vec::new();
        assert!(!resurrect(&ai, &mut w, &mut events));
        w.cheat_learn_spell(P1, resurrection);
        assert!(resurrect(&ai, &mut w, &mut events), "{events:?}");
        let started = w
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .casting()
            .is_some()
            || events.iter().any(|e| matches!(e, Event::HeroJoined { .. }));
        assert!(started, "{events:?}");
        Ok(())
    }

    #[test]
    fn a_wizard_that_knows_summon_hero_casts_it_whatever_the_number_of_its_heroes() -> TestResult {
        let summon_hero = f4x_def::rules::SpellId::new(208) /* summon_hero */;
        for hired in [0, 6] {
            let mut w = fresh_game()?;
            let mut ai = Ai::new(P1);
            // Nothing left to scout: no spirit is wanted ahead of the hero.
            ai.frontier_closed = true;
            treasury(&mut w, P1)?.gold = f4x_def::half_int::HalfInt::from_int(50_000);
            treasury(&mut w, P1)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
            w.wizard_mut(P1)
                .ok_or("player 1 is a wizard")?
                .set_split(20, 40, 40)
                .map_err(|e| format!("{e:?}"))?;
            w.cheat_learn_spell(P1, summon_hero);
            for k in 0..hired {
                w.hire_hero(P1, k)
                    .map_err(|e| format!("hiring hero {k}: {e:?}"))?;
            }
            let heroes = |w: &World| PlayerView::new(w, P1).my_heroes().len();
            let before = heroes(&w);
            let mut events = Vec::new();
            cast(&ai, &mut w, &mut events);
            let begun = w
                .wizard(P1)
                .ok_or("player 1 is a wizard")?
                .casting()
                .is_some_and(|c| c.spell == summon_hero);
            assert!(
                begun || heroes(&w) > before,
                "with {hired} heroes a hero is summoned or the spell begun: {events:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_militarist_gives_more_power_to_mana_than_a_pragmatist() -> TestResult {
        let research_share = |objective: Objective| -> Result<_, Box<dyn std::error::Error>> {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            w.wizard_mut(P0)
                .ok_or("player 0 is a wizard")?
                .put(Some(objective));
            // A big reserve needs no spare mana income: the first candidate that pays is taken.
            treasury(&mut w, P0)?.mana = f4x_def::half_int::HalfInt::from_int(1_000);
            split(&Ai::new(P0), &mut w, &mut Vec::new());
            Ok(w.wizard(P0)
                .ok_or("player 0 is a wizard")?
                .power_split()
                .research)
        };
        assert!(research_share(Objective::Militarist)? < research_share(Objective::Pragmatist)?);
        Ok(())
    }

    /// Puts lasting spells with an upkeep on the wizard until its mana income at the split that
    /// gives mana the most is below the upkeep.
    fn overburdened(w: &mut Game, player: PlayerId) -> Result<(), Box<dyn std::error::Error>> {
        let loaded: Vec<_> = w
            .content()
            .rules
            .spells
            .iter()
            .filter(|(_, d)| d.upkeep > 0 && d.duration == f4x_def::rules::Duration::Permanent)
            .filter(|(_, d)| d.target == f4x_def::rules::SpellTarget::None)
            .map(|(id, _)| id)
            .collect();
        w.command(set_split(player, (80, 0, 20)));
        for spell in loaded {
            let income = w.income(player);
            if income.split.mana < income.mana_upkeep {
                return Ok(());
            }
            w.attach_enchantment(f4x_core::rules::exec::Target::Player(player), spell, player);
        }
        let income = w.income(player);
        if income.split.mana < income.mana_upkeep {
            Ok(())
        } else {
            Err("no upkeep was enough to outrun the income".into())
        }
    }

    #[test]
    fn a_wizard_whose_upkeep_eats_the_income_keeps_researching_while_its_reserve_lasts(
    ) -> TestResult {
        let research_share = |mana: i32| -> Result<_, Box<dyn std::error::Error>> {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            overburdened(&mut w, P0)?;
            treasury(&mut w, P0)?.mana = HalfInt::from_int(mana);
            split(&Ai::new(P0), &mut w, &mut Vec::new());
            Ok(w.wizard(P0)
                .ok_or("player 0 is a wizard")?
                .power_split()
                .research)
        };
        assert_eq!(research_share(0)?, 0, "no reserve: all of it to mana");
        assert!(
            research_share(100_000)? > 0,
            "a reserve for the deficit: research goes on"
        );
        Ok(())
    }

    #[test]
    fn a_wizard_with_nothing_left_to_research_puts_no_power_into_research() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let ids: Vec<_> = w.content().rules.spells.iter().map(|(id, _)| id).collect();
        for id in ids {
            w.cheat_learn_spell(P0, id);
        }
        assert!(PlayerView::new(&w, P0).research_choices().is_empty());
        split(&Ai::new(P0), &mut w, &mut Vec::new());
        let share = w.wizard(P0).ok_or("player 0 is a wizard")?.power_split();
        assert_eq!(share.research, 0);
        assert!(share.mana >= 60, "mana {}", share.mana);
        Ok(())
    }

    #[test]
    fn the_split_starts_balanced_and_moves_with_the_phase_of_the_game() -> TestResult {
        let share = |turn: u32| -> Result<_, Box<dyn std::error::Error>> {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            // A known spell and one to research, so that research still has a use.
            let ids: Vec<_> = w.content().rules.spells.iter().map(|(id, _)| id).collect();
            w.cheat_learn_spell(P0, ids[0]);
            w.wizard_mut(P0)
                .ok_or("player 0 is a wizard")?
                .get_mut::<f4x_core::wizard::sheet::Candidates>()
                .0
                .push(ids[1]);
            assert!(!PlayerView::new(&w, P0).research_choices().is_empty());
            w.set_round(turn);
            split(&Ai::new(P0), &mut w, &mut Vec::new());
            let s = w.wizard(P0).ok_or("player 0 is a wizard")?.power_split();
            Ok((s.mana, s.research, s.skill))
        };
        let (build, secure, win) = (share(10)?, share(250)?, share(400)?);
        assert_eq!(build.0 + build.1 + build.2, 100);
        assert!(
            secure.0 >= 25 && secure.1 >= 35 && secure.2 >= 25,
            "{secure:?}"
        );
        assert!(build.1 > secure.1, "{build:?} {secure:?}");
        assert!(win.0 > secure.0 && win.1 < secure.1, "{win:?} {secure:?}");
        Ok(())
    }

    #[test]
    fn an_objective_researches_the_sort_of_spell_it_likes_first() -> TestResult {
        use crate::objective::{traits_of, Like};
        let spells = &shipped().rules.spells;
        let bears = spells.get(spell(6 /* war_bears */)?);
        let militarist = traits_of(crate::testworld::levers(), Some(Objective::Militarist));
        assert!(liked(bears, kind(bears), &militarist));
        assert!(!liked(
            bears,
            kind(bears),
            &traits_of(crate::testworld::levers(), Some(Objective::Perfectionist))
        ));
        assert!(!liked(
            bears,
            kind(bears),
            &traits_of(crate::testworld::levers(), None)
        ));
        assert!(militarist.likes.contains(&Like::Summon));
        Ok(())
    }

    #[test]
    fn a_banished_wizard_turns_just_enough_gold_into_mana_for_the_return() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let spell = spell(214 /* spell_of_return */)?;
        let mut wizard = w.wizard_mut(P0).ok_or("player 0 is a wizard")?;
        wizard.put(Some(f4x_core::wizard::fortress::Banishment {
            by: Some(P1),
        }));
        wizard.get_mut::<f4x_core::wizard::sheet::Skill>().points = 2_000;
        w.scenario_casting(P0, spell, 500);
        let due = w
            .wizard(P0)
            .ok_or("player 0 is a wizard")?
            .casting_skill(&w.content().rules)
            .min(500);
        assert!(due > 4, "the test wizard has a casting skill ({due})");
        {
            let t = treasury(&mut w, P0)?;
            t.mana = f4x_def::half_int::HalfInt::from_int(0);
            t.gold = f4x_def::half_int::HalfInt::from_int(10_000);
        }
        let ai = Ai::new(P0);
        fund_return(&ai, &mut w, &mut Vec::new());
        let t = w.player(P0).ok_or("player 0 exists")?.treasury();
        assert_eq!(t.mana, due, "mana now covers the payment");
        assert!(t.gold < 10_000 && t.gold >= 10_000 - 2 * due - 2);
        // Covered already: nothing more is converted.
        let gold = t.gold;
        fund_return(&ai, &mut w, &mut Vec::new());
        assert_eq!(w.player(P0).ok_or("player 0 exists")?.treasury().gold, gold);
        Ok(())
    }

    #[test]
    fn a_banished_wizard_that_has_paid_for_the_return_aims_it_at_a_city() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.wizard_mut(P0).ok_or("player 0 is a wizard")?.put(Some(
            f4x_core::wizard::fortress::Banishment { by: Some(P1) },
        ));
        w.scenario_casting(P0, spell(214 /* spell_of_return */)?, 0);
        assert!(w.wizard(P0).ok_or("a wizard")?.is_banished());
        resolve_paid(&Ai::new(P0), &mut w, &mut Vec::new());
        let wizard = w.wizard(P0).ok_or("a wizard")?;
        assert!(!wizard.is_banished(), "the wizard is home again");
        assert!(wizard.casting().is_none());
        assert_eq!(wizard.fortress(), Some(at(COAST_CITY.0, COAST_CITY.1)));
        Ok(())
    }

    #[test]
    fn a_wizard_that_is_not_banished_keeps_its_gold() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.scenario_casting(P0, spell(214 /* spell_of_return */)?, 500);
        treasury(&mut w, P0)?.mana = f4x_def::half_int::HalfInt::from_int(0);
        let gold = w.player(P0).ok_or("player 0 exists")?.treasury().gold;
        fund_return(&Ai::new(P0), &mut w, &mut Vec::new());
        assert_eq!(w.player(P0).ok_or("player 0 exists")?.treasury().gold, gold);
        Ok(())
    }

    #[test]
    fn a_wizard_curses_the_city_of_an_enemy_at_war_and_leaves_a_neighbour_at_peace_alone(
    ) -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let famine = spell(189 /* famine */)?;
        w.cheat_learn_spell(P0, famine);
        treasury(&mut w, P0)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        w.wizard_mut(P0)
            .ok_or("player 0 is a wizard")?
            .set_split(20, 40, 40)
            .map_err(|e| format!("{e:?}"))?;
        let ai = Ai::new(P0);
        let enemy_city = at(ENEMY_CITY.0, ENEMY_CITY.1);
        w.explore(P0, enemy_city, HalfInt::from_raw(3));
        let enemy = w
            .economy()
            .city_at(enemy_city)
            .ok_or("the enemy has a city")?;
        let aimed = |w: &World| {
            PlayerView::new(w, P0)
                .known_cities()
                .iter()
                .any(|c| c.0 == enemy)
        };
        assert!(aimed(&w), "the test world shows the enemy city");
        cast(&ai, &mut w, &mut Vec::new());
        assert!(
            w.wizard(P0)
                .ok_or("player 0 is a wizard")?
                .casting()
                .is_none(),
            "at peace: no curse"
        );
        w.command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        cast(&ai, &mut w, &mut Vec::new());
        let began = w
            .wizard(P0)
            .ok_or("player 0 is a wizard")?
            .casting()
            .is_some()
            || w.spells()
                .city_enchantments(enemy)
                .iter()
                .any(|&(s, _)| s == famine);
        assert!(began, "at war: Famine begun or cast");
        Ok(())
    }

    #[test]
    fn a_wizard_begins_a_spell_on_the_world_only_while_at_war() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let storm = spell(113 /* meteor_storm */)?;
        w.cheat_learn_spell(P0, storm);
        treasury(&mut w, P0)?.mana = f4x_def::half_int::HalfInt::from_int(20_000);
        let mut wizard = w.wizard_mut(P0).ok_or("player 0 is a wizard")?;
        wizard.found(
            &f4x_def::testdir::shipped().rules,
            &[
                (RealmId::new(2) /* chaos */, 11),
                (RealmId::new(4) /* death */, 11),
                (RealmId::new(0) /* nature */, 11),
            ],
            &[],
        );
        wizard.put(Some(f4x_core::wizard::fortress::Fortress(at(
            COAST_CITY.0,
            COAST_CITY.1,
        ))));
        wizard.get_mut::<f4x_core::wizard::sheet::Skill>().points = 20_000;
        wizard.set_split(55, 0, 45).map_err(|e| format!("{e:?}"))?;
        let ai = Ai::new(P0);
        let began = |w: &World| -> Result<bool, &'static str> {
            let magic = w.wizard(P0).ok_or("player 0 is a wizard")?;
            Ok(magic.casting().is_some() || magic.has_global(storm))
        };
        cast(&ai, &mut w, &mut Vec::new());
        assert!(!began(&w)?, "at peace: nothing on the world");
        w.command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        cast(&ai, &mut w, &mut Vec::new());
        assert!(began(&w)?, "at war: Meteor Storm begun or cast");
        Ok(())
    }

    /// A wizard with the mana, the skill and the books to cast what it knows at once, and a
    /// world with a strait: its walkers at the west wait for a target in the east.
    fn strait_wizard(spells: &[u16]) -> Result<(Game, Ai), Box<dyn std::error::Error>> {
        use crate::roles::{Crossing, Role};
        let mut w = crate::testworld::game_with_strait(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        for &s in spells {
            w.cheat_learn_spell(P0, spell(s)?);
        }
        treasury(&mut w, P0)?.mana = HalfInt::from_int(20_000);
        let mut wizard = w.wizard_mut(P0).ok_or("player 0 is a wizard")?;
        wizard.found(
            &f4x_def::testdir::shipped().rules,
            &[
                (RealmId::new(2) /* chaos */, 11),
                (RealmId::new(4) /* death */, 11),
                (RealmId::new(0) /* nature */, 11),
            ],
            &[],
        );
        wizard.put(Some(f4x_core::wizard::fortress::Fortress(at(
            COAST_CITY.0,
            COAST_CITY.1,
        ))));
        {
            let mut skill = wizard.get_mut::<f4x_core::wizard::sheet::Skill>();
            skill.points = 20_000;
            // A turn's casting skill, enough to pay for any of the spells at once.
            skill.left = 5_000;
        }
        wizard.set_split(55, 0, 45).map_err(|e| format!("{e:?}"))?;
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.round());
        let _ = (Crossing::Sea, Role::Reinforce);
        Ok((w, ai))
    }

    /// `units` wait for a ship to the east shore, as the dispatcher leaves them.
    fn wait_for_a_ship(ai: &mut Ai, w: &Game, units: &[hecs::Entity]) {
        use crate::roles::{Crossing, Role};
        let view = PlayerView::new(w, P0);
        for &u in units {
            let s = view.serial_of(u);
            let _ = ai
                .roles
                .claim(s, Role::Reinforce, w.round(), Some(at(30, 6)));
            ai.roles.set_crossing(s, Some(Crossing::Sea));
        }
    }

    fn has(w: &World, u: hecs::Entity, id: u16) -> Result<bool, String> {
        Ok(PlayerView::new(w, P0)
            .unit_enchantments(u)
            .contains(&spell(id)?))
    }

    /// Task 701: a walker that waits for a ship, with none assigned, is given Water Walking and
    /// crosses the strait by itself; the others of the stack are not given it, and a unit that
    /// has a ship coming is left alone.
    #[test]
    fn a_walker_that_waits_for_a_ship_is_given_water_walking() -> TestResult {
        let (mut w, mut ai) = strait_wizard(&[8 /* water_walking */])?;
        let ship = spawn(&mut w, P0, at(25, 1), 36 /* galley */, 1)[0];
        let walkers = spawn(&mut w, P0, at(9, 6), 104 /* spearmen */, 3);
        crate::roles::register(&mut ai, &w, w.round());
        wait_for_a_ship(&mut ai, &w, &walkers);
        // One has a ship coming: it is not the one.
        {
            let view = PlayerView::new(&w, P0);
            ai.roles
                .board_at(view.serial_of(walkers[0]), view.serial_of(ship), at(9, 6));
        }
        cast(&ai, &mut w, &mut Vec::new());
        assert!(!has(&w, walkers[0], 8)?, "its ship is coming");
        assert!(has(&w, walkers[1], 8)?, "the first that waits with no ship");
        assert!(!has(&w, walkers[2], 8)?, "one spell, one unit");
        // The unit now walks on water: it needs no ship.
        assert!(!PlayerView::new(&w, P0).is_walker(walkers[1]));
        assert!(w.find_path(&[walkers[1]], at(30, 6)).is_some());
        Ok(())
    }

    #[test]
    fn nothing_is_cast_for_units_that_wait_for_nothing_or_need_no_ship() -> TestResult {
        let (mut w, mut ai) =
            strait_wizard(&[8 /* water_walking */, 63 /* wind_walking */])?;
        let walkers = spawn(&mut w, P0, at(9, 6), 104 /* spearmen */, 2);
        let flier = spawn(&mut w, P0, at(9, 6), 164 /* great_drake */, 1)[0];
        crate::roles::register(&mut ai, &w, w.round());
        // Nobody waits for a ship.
        cast(&ai, &mut w, &mut Vec::new());
        for &u in &walkers {
            assert!(!has(&w, u, 8)? && !has(&w, u, 63)?);
        }
        // A flier that carries the sea tag is no walker: it is not aimed at.
        wait_for_a_ship(&mut ai, &w, &[flier]);
        cast(&ai, &mut w, &mut Vec::new());
        assert!(!has(&w, flier, 8)? && !has(&w, flier, 63)?);
        Ok(())
    }

    /// Wind Walking carries the stack it is cast in: it goes to the unit with the most mates
    /// waiting on its square, and once cast the mates are no walkers in the stack, so it is cast
    /// once. With Water Walking known as well, the cheaper one is chosen.
    #[test]
    fn wind_walking_is_cast_once_on_the_largest_waiting_stack() -> TestResult {
        let (mut w, mut ai) = strait_wizard(&[63 /* wind_walking */])?;
        let lone = spawn(&mut w, P0, at(8, 6), 104 /* spearmen */, 1);
        let stack = spawn(&mut w, P0, at(9, 6), 104 /* spearmen */, 3);
        crate::roles::register(&mut ai, &w, w.round());
        wait_for_a_ship(&mut ai, &w, &[lone[0], stack[0], stack[1], stack[2]]);
        cast(&ai, &mut w, &mut Vec::new());
        let carriers: Vec<_> = stack
            .iter()
            .filter(|&&u| has(&w, u, 63) == Ok(true))
            .collect();
        assert_eq!(carriers.len(), 1, "one Wind Walker in the big stack");
        assert!(!has(&w, lone[0], 63)?);
        cast(&ai, &mut w, &mut Vec::new());
        let again = stack
            .iter()
            .filter(|&&u| has(&w, u, 63) == Ok(true))
            .count();
        assert_eq!(again, 1, "the stack is carried: no second cast");
        // The stack crosses the strait as one.
        assert!(w.find_path(&stack, at(30, 6)).is_some());
        Ok(())
    }

    #[test]
    fn the_cheaper_crossing_spell_goes_first() -> TestResult {
        let (mut w, mut ai) =
            strait_wizard(&[63 /* wind_walking */, 8 /* water_walking */])?;
        let units = spawn(&mut w, P0, at(9, 6), 104 /* spearmen */, 2);
        crate::roles::register(&mut ai, &w, w.round());
        wait_for_a_ship(&mut ai, &w, &units);
        cast(&ai, &mut w, &mut Vec::new());
        assert!(units.iter().any(|&u| has(&w, u, 8) == Ok(true)));
        assert!(units.iter().all(|&u| has(&w, u, 63) == Ok(false)));
        Ok(())
    }

    /// Flight is a buff for the fight too: with fighters to buff it goes where it gains most;
    /// but a walker that waits for a ship comes first.
    #[test]
    fn flight_goes_to_the_walker_that_waits_for_a_ship() -> TestResult {
        let (mut w, mut ai) = strait_wizard(&[56 /* flight */])?;
        let units = spawn(&mut w, P0, at(9, 6), 104 /* spearmen */, 3);
        crate::roles::register(&mut ai, &w, w.round());
        // The last one waits: by serial order the buff would go to the first.
        wait_for_a_ship(&mut ai, &w, &units[2..]);
        cast(&ai, &mut w, &mut Vec::new());
        assert!(has(&w, units[2], 56)?);
        assert!(!has(&w, units[0], 56)? && !has(&w, units[1], 56)?);
        Ok(())
    }

    /// Floating Island is a ship raised on the strait: cast when units wait for a ship and none
    /// is free, once, and the units that wait are then fetched like any ship's passengers.
    #[test]
    fn a_floating_island_is_raised_in_the_strait_for_units_that_wait_for_a_ship() -> TestResult {
        let (mut w, mut ai) = strait_wizard(&[43 /* floating_island */])?;
        let units = spawn(&mut w, P0, at(9, 6), 104 /* spearmen */, 2);
        crate::roles::register(&mut ai, &w, w.round());
        let islands = |w: &World| {
            let view = PlayerView::new(w, P0);
            view.my_units()
                .into_iter()
                .filter(|&u| crate::view::is_ship(&view, u))
                .count()
        };
        // Nobody waits: no island.
        cast(&ai, &mut w, &mut Vec::new());
        assert_eq!(islands(&w), 0, "nobody waits for a ship");
        wait_for_a_ship(&mut ai, &w, &units);
        cast(&ai, &mut w, &mut Vec::new());
        assert_eq!(islands(&w), 1, "an island for the waiting units");
        // It has no work yet: no second one is raised.
        crate::roles::register(&mut ai, &w, w.round());
        wait_for_a_ship(&mut ai, &w, &units);
        cast(&ai, &mut w, &mut Vec::new());
        assert_eq!(islands(&w), 1, "one island at a time");
        Ok(())
    }

    /// Earth Gate: with a campaign on, the rally city first, then the far city that makes the
    /// troops, and no more than the pair.
    #[test]
    fn an_earth_gate_goes_to_the_rally_city_and_then_to_a_far_city() -> TestResult {
        let mut w = fresh_game()?;
        let mut ai = Ai::new(P0);
        let gate = spell(37 /* earth_gate */)?;
        w.cheat_learn_spell(P0, gate);
        w.reveal_map(P0);
        let enemy = PlayerView::new(&w, P0)
            .known_cities()
            .into_iter()
            .find(|&(_, owner, _)| owner == P1)
            .map(|(_, _, p)| p)
            .ok_or("an enemy city")?;
        let home = PlayerView::new(&w, P0).fortress().ok_or("a fortress")?;
        // A second city of the wizard far from the first, on the same plane.
        let (width, h) = PlayerView::new(&w, P0).map_size().ok_or("a map")?;
        let candidates: Vec<Position> = {
            let view = PlayerView::new(&w, P0);
            (0..h)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| Position { x, y, ..home })
                .filter(|&p| {
                    crate::view::distance(&view, p, home)
                        .is_some_and(|d| d >= w.levers().magic.gate_far)
                })
                .collect()
        };
        let far = candidates
            .into_iter()
            .find(|&pos| {
                let events = w.command(Command::FoundCity {
                    owner: P0,
                    pos,
                    name: "Far".into(),
                    race: w.content().rules.races.id_numbered(8),
                    population: 6,
                });
                !events.iter().any(|e| matches!(e, Event::Rejected(_)))
            })
            .ok_or("no room for a far city")?;
        let city_at = |w: &World, p| w.economy().city_at(p).ok_or("a city");
        let (home_city, far_city) = (city_at(&w, home)?, city_at(&w, far)?);
        // The rally city is the one nearer the enemy.
        let view = PlayerView::new(&w, P0);
        let near = |p| crate::view::distance(&view, p, enemy).unwrap_or(u32::MAX);
        let (rally_city, other_city) = if near(home) <= near(far) {
            (home_city, far_city)
        } else {
            (far_city, home_city)
        };
        let aim = |w: &World, ai: &Ai| earth_gate_target(ai, &PlayerView::new(w, P0), gate);
        assert_eq!(aim(&w, &ai), None, "no campaign: no gate");
        ai.campaign = Some(enemy);
        assert_eq!(aim(&w, &ai), Some(Target::City(rally_city)));
        w.scenario_enchant_city(rally_city, gate, P0);
        assert_eq!(aim(&w, &ai), Some(Target::City(other_city)));
        w.scenario_enchant_city(other_city, gate, P0);
        assert_eq!(aim(&w, &ai), None, "the pair is done");
        Ok(())
    }

    /// The overland spells the AI leaves alone, and why (task 509). Each is cast by something else
    /// or has no worth the AI can weigh yet.
    const LEFT_ALONE: &[(u16, &str)] = &[
        (
            15, /* change_terrain */
            "no model of what a changed square is worth",
        ),
        (
            18, /* transmute */
            "no model of what a changed mineral is worth",
        ),
        (
            28, /* move_fortress */
            "the AI never moves its fortress",
        ),
        (
            203, /* summoning_circle */
            "the AI never moves its summoning circle",
        ),
        (
            54, /* spell_lock */
            "protects a unit from dispels in battle; no overland worth",
        ),
        (
            163, /* dark_rituals */
            "costs the city its growth and adds unrest",
        ),
        (
            135, /* planar_seal */
            "shuts the planes to the caster's own army as well",
        ),
        (
            133, /* resurrection */
            "cast by `resurrect`, which picks the hero",
        ),
        (207 /* enchant_item */, "cast by `craft.rs`"),
        (211 /* create_artifact */, "cast by `craft.rs`"),
        (
            214, /* spell_of_return */
            "begun by the game when a wizard is banished; `fund_return` pays for it",
        ),
    ];

    #[test]
    fn every_overland_spell_is_cast_for_an_intent_or_left_alone_on_purpose() {
        let mut left: Vec<u16> = shipped()
            .rules
            .spells
            .defs()
            .iter()
            .filter(|d| d.contexts.contains(&CastContext::Overland) && kind(d) == Kind::Other)
            .map(|d| d.num)
            .collect();
        left.sort_unstable();
        let mut expected: Vec<u16> = LEFT_ALONE.iter().map(|&(n, _)| n).collect();
        expected.sort_unstable();
        assert_eq!(left, expected, "a spell gained or lost its use");
    }

    #[test]
    fn a_city_with_an_enemy_by_it_is_bolstered_before_the_fortress() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let events = w.command(f4x_core::world::Command::FoundCity {
            owner: P0,
            pos: at(3, 3),
            name: "Second".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 6,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        let wall = spell(3 /* wall_of_stone */)?;
        w.cheat_learn_spell(P0, wall);
        w.make_contact(P0, P1);
        w.command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        w.reveal_map(P0);
        let city_at = |w: &World, p| w.economy().city_at(p);
        // No enemy near: the first city, as before (the test world has no fortress).
        assert_eq!(
            pick_target(&PlayerView::new(&w, P0), wall),
            city_at(&w, at(COAST_CITY.0, COAST_CITY.1)).map(Target::City),
            "no enemy: the first city"
        );
        spawn(&mut w, P1, at(4, 3), 104 /* high_men_spearmen */, 3);
        let threatened = pick_target(&PlayerView::new(&w, P0), wall);
        assert_eq!(
            threatened,
            city_at(&w, at(3, 3)).map(Target::City),
            "an enemy by the second city: it comes first"
        );
        Ok(())
    }

    #[test]
    fn natures_wrath_has_a_victim_only_in_an_enemy_with_death_or_chaos_books() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let wrath = spell(40 /* natures_wrath */)?;
        let storm = spell(113 /* meteor_storm */)?;
        w.wizard_mut(P1).ok_or("player 1 is a wizard")?.found(
            &f4x_def::testdir::shipped().rules,
            &[(RealmId::new(3) /* life */, 5)],
            &[],
        );
        let victim = |w: &World, id| {
            let def = w.content().rules.spells.get(id);
            crate::intent::global_wanted(&PlayerView::new(w, P0), def)
        };
        assert!(!victim(&w, wrath) && !victim(&w, storm), "at peace");
        w.command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(victim(&w, storm), "at war: the storm has a victim");
        assert!(!victim(&w, wrath), "only Life: no Wrath");
        w.wizard_mut(P1).ok_or("player 1 is a wizard")?.found(
            &f4x_def::testdir::shipped().rules,
            &[(RealmId::new(2) /* chaos */, 5)],
            &[],
        );
        assert!(victim(&w, wrath), "Chaos books: Wrath");
        Ok(())
    }

    #[test]
    fn a_wizard_disenchants_the_city_an_enemy_has_cursed_and_otherwise_leaves_it() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let (area, famine, own) = (
            spell(204 /* disenchant_area */)?,
            spell(189 /* famine */)?,
            spell(12 /* natures_eye */)?,
        );
        w.cheat_learn_spell(P0, area);
        treasury(&mut w, P0)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        let city = w
            .economy()
            .city_at(at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("player 0 has a city")?;
        w.scenario_enchant_city(city, own, P0);
        let aim = |w: &World| pick_target(&PlayerView::new(w, P0), area);
        assert!(aim(&w).is_none(), "a spell of its own is no curse");
        w.scenario_enchant_city(city, famine, P1);
        assert_eq!(
            aim(&w),
            Some(Target::Tile(at(COAST_CITY.0, COAST_CITY.1))),
            "the cursed city is the aim"
        );
        Ok(())
    }

    #[test]
    fn a_wizard_breaks_the_global_spell_of_an_enemy_at_war_and_not_of_a_neighbour_at_peace(
    ) -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let (disjunction, awareness) = (spell(210 /* disjunction */)?, spell(209 /* awareness */)?);
        w.cheat_learn_spell(P0, disjunction);
        w.scenario_add_global(P1, awareness);
        let aim = |w: &World| pick_target(&PlayerView::new(w, P0), disjunction);
        assert!(aim(&w).is_none(), "at peace: left alone");
        w.command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        let instance = w
            .global_instance(P1, awareness)
            .ok_or("player 1 has the global spell")?;
        assert_eq!(aim(&w), Some(Target::Spell(instance)));
        Ok(())
    }

    #[test]
    fn a_wizard_blasts_a_rival_at_war_who_is_casting_and_leaves_one_who_is_not() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let blast = spell(58 /* spell_blast */)?;
        w.cheat_learn_spell(P0, blast);
        treasury(&mut w, P0)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        let aim = |w: &World| pick_target(&PlayerView::new(w, P0), blast);
        w.command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(aim(&w).is_none(), "the rival casts nothing");
        w.scenario_casting(P1, spell(113 /* meteor_storm */)?, 100);
        assert_eq!(aim(&w), Some(Target::Player(P1)));
        Ok(())
    }

    /// Patch 1.50, note 271: the AI aims no curse at a wizard it has not met.
    #[test]
    fn a_wizard_blasts_nobody_it_has_not_met() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let blast = spell(58 /* spell_blast */)?;
        w.cheat_learn_spell(P0, blast);
        treasury(&mut w, P0)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        w.scenario_casting(P1, spell(113 /* meteor_storm */)?, 100);
        w.command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(pick_target(&PlayerView::new(&w, P0), blast).is_none());
        Ok(())
    }

    #[test]
    fn a_wizard_holds_an_enemy_stack_near_its_city_only_while_at_war_and_stronger_than_the_garrison(
    ) -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let stasis = spell(68 /* stasis */)?;
        w.cheat_learn_spell(P0, stasis);
        let near = at(COAST_CITY.0 - 1, COAST_CITY.1);
        spawn(&mut w, P1, near, 104 /* high_men_spearmen */, 8);
        let aim = |w: &World| pick_target(&PlayerView::new(w, P0), stasis);
        assert!(aim(&w).is_none(), "at peace: left alone");
        w.command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert_eq!(aim(&w), Some(Target::Tile(near)));
        spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            104, /* high_men_spearmen */
            9,
        );
        assert!(aim(&w).is_none(), "the garrison is the stronger");
        Ok(())
    }

    #[test]
    fn a_dear_spell_is_only_begun_when_it_is_paid_for_in_five_turns() {
        assert!(
            quick_enough(levers(), levers().magic.dear_spell, 1),
            "cheap spells are never held back"
        );
        assert!(!quick_enough(levers(), 1000, 150), "seven turns");
        assert!(quick_enough(levers(), 1000, 200), "five turns");
        assert!(!quick_enough(levers(), 1500, 0), "no casting power at all");
    }

    #[test]
    fn a_hurt_stack_far_from_home_is_recalled_and_one_in_a_city_is_not() -> TestResult {
        let mut w = fresh_game()?;
        let recall = spell(47 /* word_of_recall */)?;
        w.cheat_learn_spell(P1, recall);
        let circle = w
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        let aim = |w: &World| pick_target(&PlayerView::new(w, P1), recall);
        for u in PlayerView::new(&w, P1).my_units() {
            w.scenario_set_hurt(u, 1);
        }
        assert!(aim(&w).is_none(), "units at home are not stranded");
        let width = w.map().ok_or("a map")?.width();
        let far = f4x_core::map::Position {
            x: (circle.x + 20) % width,
            ..circle
        };
        let fighters = spawn(&mut w, P1, far, 104 /* high_men_spearmen */, 2);
        assert!(aim(&w).is_none(), "whole stack is not hurt");
        for &u in &fighters {
            let (_, max) = PlayerView::new(&w, P1).unit_hits(u);
            w.scenario_set_hurt(u, max);
        }
        assert!(matches!(aim(&w), Some(Target::Unit(u)) if fighters.contains(&u)));
        Ok(())
    }

    #[test]
    fn heroism_is_aimed_at_a_fighter_and_not_at_one_that_has_it() -> TestResult {
        let mut w = fresh_game()?;
        let heroism = spell(130 /* heroism */)?;
        w.cheat_learn_spell(P1, heroism);
        let aim = |w: &World| pick_target(&PlayerView::new(w, P1), heroism);
        let Some(Target::Unit(first)) = aim(&w) else {
            return Err("Heroism has a unit to go on".into());
        };
        w.scenario_enchant_unit(first, heroism, P1);
        match aim(&w) {
            Some(Target::Unit(second)) => assert_ne!(second, first),
            other => assert!(other.is_none(), "{other:?}"),
        }
        Ok(())
    }

    #[test]
    fn the_mana_owed_for_the_spell_of_mastery_is_mana_to_buy_with_gold() -> TestResult {
        let (mut w, _) = summoner(50_000, 0)?;
        let before = mana_wanted(&PlayerView::new(&w, P1), &[], w.round());
        let mastery = w
            .content()
            .rules
            .spells
            .iter()
            .find(|(_, d)| kind(d) == Kind::Mastery)
            .map(|(id, _)| id)
            .ok_or("a spell of mastery")?;
        w.cheat_learn_spell(P1, mastery);
        w.scenario_casting(P1, mastery, 3000);
        assert_eq!(
            mana_wanted(&PlayerView::new(&w, P1), &[], w.round()),
            before + 3000,
            "what is still owed is wanted on top"
        );
        Ok(())
    }

    /// Lycanthropy turns the obsolete (spearmen), not a unit the werewolf is no better than, and
    /// the unit's production cost weighs in the rating, not in the mana needed.
    #[test]
    fn lycanthropy_is_aimed_at_an_obsolete_unit_and_its_cost_lowers_the_rate() -> TestResult {
        let mut w = fresh_game()?;
        let lycanthropy = spell(173)?;
        w.cheat_learn_spell(P1, lycanthropy);
        let circle = w
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        let view = PlayerView::new(&w, P1);
        let spearmen_cost = {
            let rules = &view.content().rules;
            rules
                .units
                .get(rules.units.id_numbered(104).ok_or("spearmen")?)
                .cost as i32
        };
        drop(view);
        for u in PlayerView::new(&w, P1).my_units() {
            w.scenario_set_hurt(u, 0);
        }
        let paladins = spawn(&mut w, P1, circle, 113 /* high_men_paladins */, 1);
        let before = lycanthropy_target(&PlayerView::new(&w, P1), lycanthropy);
        assert!(
            before.is_none_or(|(u, ..)| u != paladins[0]),
            "a paladin is no better as a werewolf"
        );
        let spearmen = spawn(&mut w, P1, circle, 104 /* high_men_spearmen */, 1);
        let (aimed, extra, rate) = lycanthropy_target(&PlayerView::new(&w, P1), lycanthropy)
            .ok_or("an obsolete unit to turn")?;
        assert_eq!(aimed, spearmen[0]);
        assert_eq!(extra, spearmen_cost);
        // The unit given up is part of the price: the same gain for the spell's mana alone would
        // rate higher, as a summon of that strength does (it costs no unit).
        let view = PlayerView::new(&w, P1);
        let cast = view.casting_cost(lycanthropy) as f32;
        let gain = rate * (cast + extra as f32);
        assert!(rate < gain / cast);
        assert!(crate::spellhelper::rate(&view, lycanthropy, Kind::Lycanthropy) > 0.0);
        let traits = crate::objective::traits(&view);
        assert_eq!(
            cast_rank(Kind::Lycanthropy, &traits),
            cast_rank(Kind::Summon, &traits),
            "rated against the summons"
        );
        Ok(())
    }

    /// A lasting spell on one unit is rated by what it adds to the unit it is cast on: Iron Skin
    /// does more for a great drake (30 hit points) than for a spearman, so the drake is the aim, and
    /// the spell's rating is the drake's.
    #[test]
    fn a_unit_buff_is_aimed_at_the_unit_it_adds_most_to() -> TestResult {
        let mut w = fresh_game()?;
        let iron_skin = spell(24)?;
        w.cheat_learn_spell(P1, iron_skin);
        let circle = w
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        let spearmen = spawn(&mut w, P1, circle, 104 /* high_men_spearmen */, 1);
        let drake = spawn(&mut w, P1, circle, 164 /* great_drake */, 1);
        let view = PlayerView::new(&w, P1);
        let gain = |u| view.unit_power_with_spell(u, iron_skin) - view.unit_power(u);
        assert!(
            gain(spearmen[0]) > 0.0,
            "a defence of five adds to a spearman"
        );
        assert!(gain(drake[0]) > gain(spearmen[0]));
        let (aimed, value) = crate::spellhelper::best_buff(&view, iron_skin).ok_or("an aim")?;
        assert_eq!(aimed, drake[0]);
        assert!(value > 0.0);
        assert!(crate::spellhelper::rate(&view, iron_skin, Kind::UnitBuff) > 0.0);
        Ok(())
    }

    /// Heroism adds the rating of Elite over the unit's own level: a recruit gains a lot, and a
    /// unit that is Elite already gains nothing.
    #[test]
    fn heroism_is_worth_the_levels_it_lifts() -> TestResult {
        let mut w = fresh_game()?;
        let heroism = spell(130 /* heroism */)?;
        w.cheat_learn_spell(P1, heroism);
        let circle = w
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        let recruit = spawn(&mut w, P1, circle, 104 /* high_men_spearmen */, 1)[0];
        let gain = |w: &World, u| {
            let view = PlayerView::new(w, P1);
            view.unit_power_with_spell(u, heroism) - view.unit_power(u)
        };
        assert!(gain(&w, recruit) > 0.0);
        w.scenario_enchant_unit(recruit, heroism, P1);
        assert_eq!(gain(&w, recruit), 0.0, "an Elite unit gains nothing");
        Ok(())
    }

    /// A buff that adds nothing to any unit is not rated and has no aim: Regeneration on a troll,
    /// which regenerates already.
    #[test]
    fn a_unit_buff_with_nothing_to_add_has_no_aim() -> TestResult {
        let mut w = fresh_game()?;
        let regeneration = spell(31)?;
        w.cheat_learn_spell(P1, regeneration);
        let circle = w
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        let view = PlayerView::new(&w, P1);
        let others: Vec<_> = view.my_units();
        drop(view);
        for u in others {
            w.scenario_set_hurt(u, 0);
        }
        let trolls = spawn(&mut w, P1, circle, 147 /* troll_spearmen */, 1);
        let view = PlayerView::new(&w, P1);
        let gain = view.unit_power_with_spell(trolls[0], regeneration) - view.unit_power(trolls[0]);
        assert_eq!(gain, 0.0, "a troll regenerates already");
        assert_ne!(
            crate::spellhelper::best_buff(&view, regeneration).map(|b| b.0),
            Some(trolls[0])
        );
        Ok(())
    }

    /// The unit given up does not raise the mana needed: the spell's own cost with the usual
    /// margin is enough, and a spearman becomes a werewolf.
    #[test]
    fn lycanthropy_needs_only_the_spells_own_mana() -> TestResult {
        let mut w = fresh_game()?;
        let lycanthropy = spell(173)?;
        w.cheat_learn_spell(P1, lycanthropy);
        let circle = w
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        // An army big enough that no summon is wanted, so the spell under test is the one left.
        spawn(&mut w, P1, circle, 104 /* high_men_spearmen */, 8);
        let land: Vec<_> = {
            let view = PlayerView::new(&w, P1);
            [(1, 0), (0, 1), (-1, 0), (0, -1)]
                .into_iter()
                .filter_map(|(dx, dy)| view.shift(circle, dx, dy))
                .filter(|&p| {
                    view.terrain_class(p)
                        .is_some_and(|t| t != f4x_core::map::terrain::TerrainClass::Ocean)
                })
                .collect()
        };
        for &p in &land {
            spawn(&mut w, P1, p, 104, 8);
        }
        let cost = PlayerView::new(&w, P1).casting_cost(lycanthropy);
        let ai = Ai::new(P1);
        let werewolves = |w: &World| {
            PlayerView::new(w, P1)
                .my_units()
                .into_iter()
                .filter(|&u| {
                    PlayerView::new(w, P1)
                        .unit_def(u)
                        .is_some_and(|d| d.num == w.content().rules.config.spell_effects.werewolves)
                })
                .count()
        };
        // The spell's cost with the margin, and nothing for the spearman.
        for _ in 0..40 {
            treasury(&mut w, P1)?.mana = HalfInt::from_int(mana_before(levers(), cost) + 1);
            // Turns kill units of a world with no food or gold: keep the army up.
            for p in &land {
                spawn(&mut w, P1, *p, 104, 3);
            }
            resolve_paid(&ai, &mut w, &mut Vec::new());
            cast(&ai, &mut w, &mut Vec::new());
            if werewolves(&w) > 0 {
                break;
            }
            w.play_round();
        }
        assert!(werewolves(&w) >= 1, "a spearman is a werewolf");
        Ok(())
    }
}
