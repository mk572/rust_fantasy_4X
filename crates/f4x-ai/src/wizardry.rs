//! The wizard's side of the AI: how power is split, what is researched and what is cast
//! (`docs/ai-design.md` sections 5.3 and 7). Every choice is a function of what the AI sees, so a
//! game replays exactly. The order of wants each turn: the Spell of Mastery when it is known, a
//! summoning spell, a buff for the strongest units, a buff for a city, a lasting spell on the
//! whole wizard.

use f4x_core::diplomacy::Treaty;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::exec::Target;
use f4x_core::spell::unit_effect::UnitEffect;
use f4x_def::rules::effects::Effect;
use f4x_def::rules::{CastContext, Duration, SpellDef, SpellId, SpellTarget};

use crate::view::{fighting_strength, is_fighter, unit_strength};
use crate::Ai;

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
    /// Everything else: not cast by the AI yet.
    Other,
}

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
/// Turns of net mana income within which a summon that cannot be paid yet is saved for.
const SAVING_TURNS: i32 = 15;
/// Mana kept in the reserve before the AI starts a global enchantment (the original's gate).
const GLOBAL_RESERVE: i32 = 200;
/// How many times the strength of a unit a werewolf must be for Lycanthropy to be worth it: only
/// the obsolete (spearmen next to werewolves) are turned.
const LYCANTHROPY_EDGE: f32 = 1.5;
/// Werewolves, the unit Lycanthropy makes (content number).
const WEREWOLVES: u16 = 168;

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
            .contains(&f4x_def::rules::Ability::new(42)),
        _ => false,
    })
}

/// The own fighter Lycanthropy turns into a werewolf, the production cost of the unit given up
/// (counted one for one as mana in the rating only, not in the mana needed) and the rate: the
/// strength gained per mana in all. Only a unit the werewolf is clearly better than is turned
/// (`LYCANTHROPY_EDGE`); of those, the one with the best rate.
pub(crate) fn lycanthropy_target(
    view: &PlayerView,
    spell: SpellId,
) -> Option<(hecs::Entity, i32, f32)> {
    let wolf = view.content().rules.units.id_numbered(WEREWOLVES)?;
    let wolf_power = f4x_core::battle::power::def_power(view.content().rules.units.get(wolf));
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
            (wolf_power >= power * LYCANTHROPY_EDGE && view.can_aim(spell, Some(Target::Unit(u))))
                .then(|| ((wolf_power - power) / (cast + extra as f32), u, extra))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(rate, u, extra)| (u, extra, rate))
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
        && has_hook(def, "heroism")
    {
        return Kind::UnitBuff;
    }
    if def.effects.is_empty() || has_hook(def, "unimplemented") {
        return Kind::Other;
    }
    if def.target == SpellTarget::GlobalEnchantment {
        return Kind::Disjunction;
    }
    if crate::intent::is_astral_gate(def) {
        return Kind::Gate;
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

/// Gold a wizard keeps; what is above it, once the cities have bought what they can, is turned
/// into mana (Alchemy), which pays for more spells, crafting and moving items.
const GOLD_KEPT: i32 = 1000;

/// The mana the wizard could put to use now: what its dearest summon it can aim costs with the
/// margin the casting rule asks for (zero when it has none to cast).
fn mana_wanted(view: &PlayerView, blocked: &[f4x_core::map::Plane]) -> i32 {
    let Some(magic) = view.wizard() else {
        return 0;
    };
    let summon = magic
        .known()
        .iter()
        .copied()
        .filter(|&s| matches!(kind(view.spell(s)), Kind::Summon | Kind::SummonHero))
        .filter(|&s| view.can_aim(s, None))
        .map(|s| view.casting_cost(s) * 3 / 2)
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
    summon + crate::spellhelper::mana_goal(view, blocked) + mastery
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
        let short = mana_wanted(&view, &ai.tower_blocked) - view.treasury().mana.floor();
        let spare = view.treasury().gold.floor() - GOLD_KEPT;
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

/// The balanced split every wizard starts from: (mana, research, skill) in percent. Goals, the phase
/// and the state of the game move it (`split_target`).
const BASE_SPLIT: [i32; 3] = [30, 40, 30];
/// The steps the shares move in.
const STEP: i32 = 5;
/// The least a share is (skill keeps a fifth so that spells can still be cast at once).
const MIN_SHARE: [i32; 3] = [0, 0, 20];
/// Turns a mana reserve is meant to last when the income falls short of the upkeep.
const MANA_HORIZON: i32 = 40;
/// A reserve this big (and big against the income) is more than the wizard can spend soon.
const RICH_RESERVE: i32 = 1500;
/// The split while the Spell of Mastery is being cast: mana pays for it and skill sets how much
/// can be paid a turn.
const MASTERY_SPLIT: (u8, u8, u8) = (50, 0, 50);
/// The split of a wizard that has nothing left to research: no research, a bigger share for casting.
const DONE_RESEARCHING: (u8, u8, u8) = (60, 0, 40);
/// The split of a wizard that races to research the Spell of Mastery.
const RACING_SPLIT: (u8, u8, u8) = (15, 65, 20);

/// Move `amount` points of the split from one share to another, as far as the first can give.
fn shift(split: &mut [i32; 3], from: usize, to: usize, amount: i32) {
    let moved = amount.min(split[from] - MIN_SHARE[from]).max(0);
    split[from] -= moved;
    split[to] += moved;
}

/// The split a wizard wants before the upkeep is looked at: the balanced base, moved by the phase
/// of the game, the wizard's goal (`split_shift`, `lean_mana`) and the reserve it holds.
fn split_target(view: &PlayerView, traits: &crate::objective::Traits) -> [i32; 3] {
    const MANA: usize = 0;
    const RESEARCH: usize = 1;
    const SKILL: usize = 2;
    let mut t = BASE_SPLIT;
    match crate::phase::phase(view) {
        // Growing: spells and their research come first, little is cast.
        crate::phase::Phase::Build => shift(&mut t, MANA, RESEARCH, 10),
        crate::phase::Phase::Secure => {}
        // Winning: the spells are known, now they are cast.
        crate::phase::Phase::Win => {
            shift(&mut t, RESEARCH, MANA, 15);
            shift(&mut t, RESEARCH, SKILL, 5);
        }
    }
    // A goal that likes magic casts more; one that spends its mana to the bone researches more.
    for _ in 0..traits.split_shift {
        shift(&mut t, RESEARCH, MANA, STEP);
    }
    if traits.lean_mana {
        shift(&mut t, MANA, RESEARCH, STEP);
    }
    // A big reserve is not spent for the lack of power but for the lack of skill and spells.
    let reserve = view.treasury().mana.floor();
    if reserve > RICH_RESERVE {
        shift(&mut t, MANA, RESEARCH, 10);
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
    if casting_mastery {
        ai.order(game, set_split(me, MASTERY_SPLIT), events);
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
        ai.order(game, set_split(me, DONE_RESEARCHING), events);
        return;
    }
    if racing {
        ai.order(game, set_split(me, RACING_SPLIT), events);
        return;
    }
    let mut target = {
        let view = PlayerView::new(game, me);
        split_target(&view, &crate::objective::traits(&view))
    };
    // The income must cover the upkeep of the lasting spells and creatures (a reserve pays for a
    // small deficit for a long while): mana takes from research first, then from skill.
    let mut chosen = target;
    for _ in 0..40 {
        chosen = target;
        game.apply(set_split(me, as_split(chosen)));
        let view = PlayerView::new(game, me);
        let income = view.income();
        let deficit_allowed = f4x_def::half_int::HalfInt::from_int(
            view.treasury().mana.floor().max(0) / MANA_HORIZON,
        );
        if income.split.mana - income.mana_upkeep + deficit_allowed
            >= f4x_def::half_int::HalfInt::ZERO
            || (target[1] == 0 && target[2] <= MIN_SHARE[2])
        {
            break;
        }
        if target[1] > 0 {
            shift(&mut target, 1, 0, STEP);
        } else {
            shift(&mut target, 2, 0, STEP);
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
            let command = Command::ResolveCast {
                player: ai.player,
                target: Target::Hero(kind),
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
        pick_target(&view, spell)
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
            // A city with an enemy stack by it first, then the fortress.
            let fortress = view.fortress();
            let mut cities = view.my_cities();
            cities.sort_by_key(|&(c, p)| {
                (
                    view.enemies_near(p, STASIS_RADIUS).is_empty(),
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
        Kind::StackStrike => crate::intent::strike_square(view, spell),
        Kind::LandCurse => crate::intent::land_curse_target(view, spell),
        Kind::LandBoost => crate::intent::land_boost_target(view, spell),
        Kind::CityCurse => curse_city(view, spell).map(Target::City),
        Kind::Blast | Kind::WizardCurse => view
            .rivals_at_war()
            .into_iter()
            .map(Target::Player)
            .find(|&t| view.can_aim(spell, Some(t))),
        _ => None,
    }
}

/// The fewest fighters beyond the garrison that Plane Shift is cast for.
const MIN_SHIFT: usize = 6;

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

/// The own city the army can cross from: Plane Shift would put its stack on a free square the
/// units can stand on (the square on the other plane may be sea), or an Astral Gate stands in it
/// or can be raised in it. The fortress first.
pub(crate) fn shift_gather(view: &PlayerView) -> Option<Position> {
    let shift = plane_shift_spell(view);
    let gate = astral_gate_spell(view);
    if shift.is_none() && gate.is_none() {
        return None;
    }
    let fortress = view.fortress();
    let mut cities = view.my_cities();
    cities.sort_by_key(|&(c, p)| (Some(p) != fortress, view.serial_of(c)));
    cities.into_iter().find_map(|(c, p)| {
        let shifts = shift.is_some_and(|spell| {
            view.garrison(p)
                .into_iter()
                .find(|&u| is_fighter(view, u))
                .is_some_and(|u| view.can_aim(spell, Some(Target::Unit(u))))
        });
        let gated = gate.is_some_and(|spell| {
            view.city_enchantments(c).contains(&spell) || view.can_aim(spell, Some(Target::City(c)))
        });
        (shifts || gated).then_some(p)
    })
}

/// The city to raise an Astral Gate in: the army is gathered in the city it crosses from
/// (`Ai::crossing`) with the garrison and at least [`MIN_SHIFT`] fighters over, its campaign is
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
    (fighters >= crate::view::garrison_wanted(view, gather) + MIN_SHIFT)
        .then_some(Target::City(city))
        .filter(|&t| view.can_aim(spell, Some(t)))
}

/// The unit to cast Plane Shift on: the army is gathered in the city it crosses from
/// (`Ai::crossing`), with the garrison and at least [`MIN_SHIFT`] fighters over, and its campaign
/// is on the other plane. The whole stack goes, so the strongest unit is named.
fn shift_target(ai: &Ai, view: &PlayerView, spell: SpellId) -> Option<Target> {
    let gather = ai.crossing?;
    let campaign = ai.campaign?;
    if campaign.plane == gather.plane || view.city_at(gather).is_none() {
        return None;
    }
    let fighters: Vec<hecs::Entity> = view
        .garrison(gather)
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .collect();
    if fighters.len() < crate::view::garrison_wanted(view, gather) + MIN_SHIFT {
        return None;
    }
    fighters
        .into_iter()
        .map(|u| (unit_strength(view, u), u))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, u)| Target::Unit(u))
        .filter(|&t| view.can_aim(spell, Some(t)))
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
        // The same gate as the other spells: paid from the reserve, not more than fifty turns.
        let per_turn = mana / 10 + game.income(me).split.mana.floor();
        if cost * 3 / 2 > mana || cost / 50 > per_turn {
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
    let command = Command::CastSpell {
        extra: 0,
        player: me,
        spell,
        target: Some(Target::Hero(kind)),
    };
    ai.send(game, command, events).is_ok()
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
        let traits = crate::objective::traits(&view);
        // The intents in the order the wizard's objective ranks them; within an intent the helper's
        // rating puts the best spell first.
        let rated: std::collections::HashMap<SpellId, i64> = options
            .iter()
            .map(|&(k, _, s)| (s, (crate::spellhelper::rate(&view, s, k) * 1000.0) as i64))
            .collect();
        // A spirit for a node that is open or being cleared comes before everything else.
        let spirit = crate::meld::spirit_wanted(ai, &view);
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
            let target = if k == Kind::PlaneShift {
                shift_target(ai, &view, spell)
            } else if k == Kind::Gate {
                gate_target(ai, &view, spell)
            } else {
                pick_target(&view, spell)
            };
            let now = cost * 3 / 2 <= mana;
            let wanted = match k {
                Kind::Mastery => true,
                // Summoned creatures cost upkeep mana: only while the army is thin.
                Kind::Summon if priority_spirit(spell) => now && view.can_aim(spell, None),
                Kind::Summon => {
                    // A conqueror in the late game summons whatever the mana pays for and the
                    // upkeep allows: the army cap is for a wizard that is still building up.
                    let conquering = crate::phase::phase(&view) == crate::phase::Phase::Win
                        && crate::phase::goal(&view) == crate::phase::Goal::Conquest;
                    now && (room || conquering)
                        && (fighters < 12 + 2 * view.my_cities().len()
                            || (conquering && upkeep_ok(&view, def)))
                        // The game makes room on a full square, or refuses the spell.
                        && view.can_aim(spell, None)
                }
                // There is no cap on heroes: only the supply of heroes (`can_aim` is refused
                // when none is left to send) and the mana limit it.
                Kind::SummonHero => {
                    now && room && view.hero_to_summon(spell) && view.can_aim(spell, None)
                }
                Kind::UnitBuff => {
                    now && room && fighters >= 3 && upkeep_ok(&view, def) && target.is_some()
                }
                Kind::Mutate => now && fighters >= 3 && target.is_some(),
                Kind::Lycanthropy => now && fighters >= 3 && target.is_some(),
                Kind::CityBuff => now && room && upkeep_ok(&view, def) && target.is_some(),
                Kind::GlobalBuff => {
                    now && room
                        && mana - cost >= GLOBAL_RESERVE
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
                Kind::Gate => now && room && upkeep_ok(&view, def) && target.is_some(),
                Kind::LandCurse | Kind::LandBoost => {
                    now && room && mana - cost >= GLOBAL_RESERVE && target.is_some()
                }
                // Blows at an enemy at war, from mana the AI can spare.
                Kind::CityCurse | Kind::WizardCurse | Kind::Disjunction => {
                    now && room && mana - cost >= GLOBAL_RESERVE && target.is_some()
                }
                Kind::WorldSpell => {
                    now && room
                        && mana - cost >= GLOBAL_RESERVE
                        && upkeep_ok(&view, def)
                        && !magic.has_global(spell)
                        && crate::intent::global_wanted(&view, def)
                }
                Kind::Other => false,
            };
            if !wanted
                && matches!(k, Kind::Summon | Kind::SummonHero)
                && cost * 3 / 2 > mana
                && cost * 3 / 2 <= mana + SAVING_TURNS * net.max(0)
                && room
                && view.can_aim(spell, None)
                && (k == Kind::SummonHero || fighters < 12 + 2 * view.my_cities().len())
            {
                saving = true;
            }
            // The original's check: not more than fifty turns of what the AI can put towards it.
            let per_turn = mana / 10 + income.split.mana.floor();
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
    use crate::testworld::{at, fresh_game, game, shipped, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
    use f4x_core::diplomacy::DiplomacyCommand;
    use f4x_core::player::{PlayerId, Treasury};
    use f4x_core::wizard::objective::Objective;
    use f4x_def::half_int::HalfInt;
    use f4x_def::rules::RealmId;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A wizard of the fresh game that knows the dearest summon the rules have, with the given
    /// gold and mana.
    fn summoner(gold: i32, mana: i32) -> Result<(Game, i32), Box<dyn std::error::Error>> {
        let mut g = fresh_game()?;
        let summon = g
            .content()
            .rules
            .spells
            .iter()
            .filter(|(_, d)| kind(d) == Kind::Summon)
            .map(|(id, _)| id)
            .next()
            .ok_or("a summon")?;
        g.cheat_learn_spell(P1, summon);
        let t = g.treasury_mut(P1).ok_or("a treasury")?;
        t.gold = HalfInt::from_int(gold);
        t.mana = HalfInt::from_int(mana);
        let wanted = mana_wanted(&PlayerView::new(&g, P1), &[]);
        Ok((g, wanted))
    }

    #[test]
    fn a_wizard_with_much_gold_buys_the_mana_for_its_summon_and_no_more() -> TestResult {
        let (mut g, wanted) = summoner(50_000, 0)?;
        assert!(wanted > 0, "the summon can be aimed");
        alchemy(&Ai::new(P1), &mut g, &mut Vec::new());
        let t = g.treasury_mut(P1).ok_or("a treasury")?;
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
        let (mut g, wanted) = summoner(50_000, 5_000)?;
        assert!(wanted < 5_000);
        alchemy(&Ai::new(P1), &mut g, &mut Vec::new());
        let t = g.treasury_mut(P1).ok_or("a treasury")?;
        assert_eq!(t.gold.floor(), 50_000);
        assert_eq!(t.mana.floor(), 5_000);
        Ok(())
    }

    #[test]
    fn a_wizard_with_little_gold_keeps_it() -> TestResult {
        let mut g = fresh_game()?;
        let ai = Ai::new(P1);
        g.treasury_mut(P1).ok_or("a treasury")?.gold = HalfInt::from_int(900);
        alchemy(&ai, &mut g, &mut Vec::new());
        assert_eq!(g.treasury_mut(P1).ok_or("a treasury")?.gold.floor(), 900);
        Ok(())
    }

    fn spell(id: u16) -> Result<SpellId, String> {
        shipped()
            .rules
            .spells
            .id_numbered(id)
            .ok_or(format!("{id} is shipped"))
    }

    fn treasury(g: &mut Game, player: PlayerId) -> Result<&mut Treasury, &'static str> {
        g.treasury_mut(player).ok_or("the player has a treasury")
    }

    #[test]
    fn an_open_node_and_no_spirit_make_the_spirit_the_first_summon() -> TestResult {
        use f4x_core::feature::entity::Placement;
        let mut g = fresh_game()?;
        let spirit = spell(201 /* magic_spirit */)?;
        g.cheat_learn_spell(P1, spirit);
        let dearest = g
            .content()
            .rules
            .spells
            .iter()
            .filter(|(id, d)| *id != spirit && kind(d) == Kind::Summon)
            .map(|(id, _)| id)
            .next()
            .ok_or("another summon")?;
        g.cheat_learn_spell(P1, dearest);
        treasury(&mut g, P1)?.mana = HalfInt::from_int(5_000);
        let mut ai = Ai::new(P1);
        ai.frontier_closed = true;
        assert!(!crate::meld::spirit_wanted(&ai, &PlayerView::new(&g, P1)));
        let feature = g
            .content()
            .features
            .id_numbered(20 /* sorcery_node */)
            .ok_or("a node feature")?;
        let circle = PlayerView::new(&g, P1)
            .wizard()
            .and_then(|m| m.summon_at())
            .ok_or("a summoning circle")?;
        let spot = at(circle.x + 2, circle.y);
        g.explore(P1, spot, HalfInt::from_int(5));
        g.spawn_feature(Placement {
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
        assert!(crate::meld::spirit_wanted(&ai, &PlayerView::new(&g, P1)));
        cast(&ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P1);
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
        let mut g = fresh_game()?;
        let ai = Ai::new(P1);
        let resurrection = f4x_def::rules::SpellId::new(133) /* resurrection */;
        treasury(&mut g, P1)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        let hero = g.hire_hero(P1, 1).map_err(|e| format!("{e:?}"))?;
        g.cheat_slay_unit(hero);
        g.cheat_learn_spell(P1, resurrection);
        g.wizard_mut(P1)
            .ok_or("player 1 is a wizard")?
            .put(Some(f4x_core::wizard::fortress::Banishment { by: None }));
        let mut events = Vec::new();
        assert!(!resurrect(&ai, &mut g, &mut events), "{events:?}");
        assert!(g
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .casting()
            .is_none());
        Ok(())
    }

    #[test]
    fn a_wizard_that_knows_resurrection_raises_the_last_dead_hero() -> TestResult {
        let mut g = fresh_game()?;
        let ai = Ai::new(P1);
        let resurrection = f4x_def::rules::SpellId::new(133) /* resurrection */;
        treasury(&mut g, P1)?.gold = f4x_def::half_int::HalfInt::from_int(50_000);
        treasury(&mut g, P1)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
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
    fn a_wizard_that_knows_summon_hero_casts_it_whatever_the_number_of_its_heroes() -> TestResult {
        let summon_hero = f4x_def::rules::SpellId::new(208) /* summon_hero */;
        for hired in [0, 6] {
            let mut g = fresh_game()?;
            let mut ai = Ai::new(P1);
            // Nothing left to scout: no spirit is wanted ahead of the hero.
            ai.frontier_closed = true;
            treasury(&mut g, P1)?.gold = f4x_def::half_int::HalfInt::from_int(50_000);
            treasury(&mut g, P1)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
            g.wizard_mut(P1)
                .ok_or("player 1 is a wizard")?
                .set_split(20, 40, 40)
                .map_err(|e| format!("{e:?}"))?;
            g.cheat_learn_spell(P1, summon_hero);
            for k in 0..hired {
                g.hire_hero(P1, k)
                    .map_err(|e| format!("hiring hero {k}: {e:?}"))?;
            }
            let heroes = |g: &Game| PlayerView::new(g, P1).my_heroes().len();
            let before = heroes(&g);
            let mut events = Vec::new();
            cast(&ai, &mut g, &mut events);
            let begun = g
                .wizard(P1)
                .ok_or("player 1 is a wizard")?
                .casting()
                .is_some_and(|c| c.spell == summon_hero);
            assert!(
                begun || heroes(&g) > before,
                "with {hired} heroes a hero is summoned or the spell begun: {events:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_militarist_gives_more_power_to_mana_than_a_pragmatist() -> TestResult {
        let research_share = |objective: Objective| -> Result<_, Box<dyn std::error::Error>> {
            let mut g = game(COAST_CITY, 8 /* high_men */);
            g.wizard_mut(P0)
                .ok_or("player 0 is a wizard")?
                .put(Some(objective));
            // A big reserve needs no spare mana income: the first candidate that pays is taken.
            treasury(&mut g, P0)?.mana = f4x_def::half_int::HalfInt::from_int(1_000);
            split(&Ai::new(P0), &mut g, &mut Vec::new());
            Ok(g.wizard(P0)
                .ok_or("player 0 is a wizard")?
                .power_split()
                .research)
        };
        assert!(research_share(Objective::Militarist)? < research_share(Objective::Pragmatist)?);
        Ok(())
    }

    /// Puts lasting spells with an upkeep on the wizard until its mana income at the split that
    /// gives mana the most is below the upkeep.
    fn overburdened(g: &mut Game, player: PlayerId) -> Result<(), Box<dyn std::error::Error>> {
        let loaded: Vec<_> = g
            .content()
            .rules
            .spells
            .iter()
            .filter(|(_, d)| d.upkeep > 0 && d.duration == f4x_def::rules::Duration::Permanent)
            .filter(|(_, d)| d.target == f4x_def::rules::SpellTarget::None)
            .map(|(id, _)| id)
            .collect();
        g.apply(set_split(player, (80, 0, 20)));
        for spell in loaded {
            let income = g.income(player);
            if income.split.mana < income.mana_upkeep {
                return Ok(());
            }
            g.attach_enchantment(f4x_core::rules::exec::Target::Player(player), spell, player);
        }
        let income = g.income(player);
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
            let mut g = game(COAST_CITY, 8 /* high_men */);
            overburdened(&mut g, P0)?;
            treasury(&mut g, P0)?.mana = HalfInt::from_int(mana);
            split(&Ai::new(P0), &mut g, &mut Vec::new());
            Ok(g.wizard(P0)
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let ids: Vec<_> = g.content().rules.spells.iter().map(|(id, _)| id).collect();
        for id in ids {
            g.cheat_learn_spell(P0, id);
        }
        assert!(PlayerView::new(&g, P0).research_choices().is_empty());
        split(&Ai::new(P0), &mut g, &mut Vec::new());
        let share = g.wizard(P0).ok_or("player 0 is a wizard")?.power_split();
        assert_eq!(share.research, 0);
        assert!(share.mana >= 60, "mana {}", share.mana);
        Ok(())
    }

    #[test]
    fn the_split_starts_balanced_and_moves_with_the_phase_of_the_game() -> TestResult {
        let share = |turn: u32| -> Result<_, Box<dyn std::error::Error>> {
            let mut g = game(COAST_CITY, 8 /* high_men */);
            // A known spell and one to research, so that research still has a use.
            let ids: Vec<_> = g.content().rules.spells.iter().map(|(id, _)| id).collect();
            g.cheat_learn_spell(P0, ids[0]);
            g.wizard_mut(P0)
                .ok_or("player 0 is a wizard")?
                .get_mut::<f4x_core::wizard::sheet::Candidates>()
                .0
                .push(ids[1]);
            assert!(!PlayerView::new(&g, P0).research_choices().is_empty());
            g.turn = turn;
            split(&Ai::new(P0), &mut g, &mut Vec::new());
            let s = g.wizard(P0).ok_or("player 0 is a wizard")?.power_split();
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let spell = spell(214 /* spell_of_return */)?;
        let mut wizard = g.wizard_mut(P0).ok_or("player 0 is a wizard")?;
        wizard.put(Some(f4x_core::wizard::fortress::Banishment {
            by: Some(P1),
        }));
        wizard.get_mut::<f4x_core::wizard::sheet::Skill>().points = 2_000;
        g.scenario_casting(P0, spell, 500);
        let due = g
            .wizard(P0)
            .ok_or("player 0 is a wizard")?
            .casting_skill(&g.content().rules)
            .min(500);
        assert!(due > 4, "the test wizard has a casting skill ({due})");
        {
            let t = treasury(&mut g, P0)?;
            t.mana = f4x_def::half_int::HalfInt::from_int(0);
            t.gold = f4x_def::half_int::HalfInt::from_int(10_000);
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
    fn a_banished_wizard_that_has_paid_for_the_return_aims_it_at_a_city() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.wizard_mut(P0).ok_or("player 0 is a wizard")?.put(Some(
            f4x_core::wizard::fortress::Banishment { by: Some(P1) },
        ));
        g.scenario_casting(P0, spell(214 /* spell_of_return */)?, 0);
        assert!(g.wizard(P0).ok_or("a wizard")?.is_banished());
        resolve_paid(&Ai::new(P0), &mut g, &mut Vec::new());
        let wizard = g.wizard(P0).ok_or("a wizard")?;
        assert!(!wizard.is_banished(), "the wizard is home again");
        assert!(wizard.casting().is_none());
        assert_eq!(wizard.fortress(), Some(at(COAST_CITY.0, COAST_CITY.1)));
        Ok(())
    }

    #[test]
    fn a_wizard_that_is_not_banished_keeps_its_gold() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.scenario_casting(P0, spell(214 /* spell_of_return */)?, 500);
        treasury(&mut g, P0)?.mana = f4x_def::half_int::HalfInt::from_int(0);
        let gold = g.player(P0).ok_or("player 0 exists")?.treasury().gold;
        fund_return(&Ai::new(P0), &mut g, &mut Vec::new());
        assert_eq!(g.player(P0).ok_or("player 0 exists")?.treasury().gold, gold);
        Ok(())
    }

    #[test]
    fn a_wizard_curses_the_city_of_an_enemy_at_war_and_leaves_a_neighbour_at_peace_alone(
    ) -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let famine = spell(189 /* famine */)?;
        g.cheat_learn_spell(P0, famine);
        treasury(&mut g, P0)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        g.wizard_mut(P0)
            .ok_or("player 0 is a wizard")?
            .set_split(20, 40, 40)
            .map_err(|e| format!("{e:?}"))?;
        let ai = Ai::new(P0);
        let enemy_city = at(ENEMY_CITY.0, ENEMY_CITY.1);
        g.explore(P0, enemy_city, HalfInt::from_raw(3));
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
            || g.spells()
                .city_enchantments(enemy)
                .iter()
                .any(|&(s, _)| s == famine);
        assert!(began, "at war: Famine begun or cast");
        Ok(())
    }

    #[test]
    fn a_wizard_begins_a_spell_on_the_world_only_while_at_war() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let storm = spell(113 /* meteor_storm */)?;
        g.cheat_learn_spell(P0, storm);
        treasury(&mut g, P0)?.mana = f4x_def::half_int::HalfInt::from_int(20_000);
        let mut wizard = g.wizard_mut(P0).ok_or("player 0 is a wizard")?;
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
            43, /* floating_island */
            "one square of land does not carry an army over the sea (the naval planner)",
        ),
        (
            37, /* earth_gate */
            "pays for a gate in two cities for a teleport; the army uses one it has",
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let events = g.apply(f4x_core::game::Command::FoundCity {
            owner: P0,
            pos: at(3, 3),
            name: "Second".into(),
            race: g.content().rules.races.id_numbered(8),
            population: 6,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        let wall = spell(3 /* wall_of_stone */)?;
        g.cheat_learn_spell(P0, wall);
        g.make_contact(P0, P1);
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        g.reveal_map(P0);
        let city_at = |g: &Game, p| g.economy().city_at(p);
        // No enemy near: the first city, as before (the test world has no fortress).
        assert_eq!(
            pick_target(&PlayerView::new(&g, P0), wall),
            city_at(&g, at(COAST_CITY.0, COAST_CITY.1)).map(Target::City),
            "no enemy: the first city"
        );
        spawn(&mut g, P1, at(4, 3), 104 /* high_men_spearmen */, 3);
        let threatened = pick_target(&PlayerView::new(&g, P0), wall);
        assert_eq!(
            threatened,
            city_at(&g, at(3, 3)).map(Target::City),
            "an enemy by the second city: it comes first"
        );
        Ok(())
    }

    #[test]
    fn natures_wrath_has_a_victim_only_in_an_enemy_with_death_or_chaos_books() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let wrath = spell(40 /* natures_wrath */)?;
        let storm = spell(113 /* meteor_storm */)?;
        g.wizard_mut(P1).ok_or("player 1 is a wizard")?.found(
            &f4x_def::testdir::shipped().rules,
            &[(RealmId::new(3) /* life */, 5)],
            &[],
        );
        let victim = |g: &Game, id| {
            let def = g.content().rules.spells.get(id);
            crate::intent::global_wanted(&PlayerView::new(g, P0), def)
        };
        assert!(!victim(&g, wrath) && !victim(&g, storm), "at peace");
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(victim(&g, storm), "at war: the storm has a victim");
        assert!(!victim(&g, wrath), "only Life: no Wrath");
        g.wizard_mut(P1).ok_or("player 1 is a wizard")?.found(
            &f4x_def::testdir::shipped().rules,
            &[(RealmId::new(2) /* chaos */, 5)],
            &[],
        );
        assert!(victim(&g, wrath), "Chaos books: Wrath");
        Ok(())
    }

    #[test]
    fn a_wizard_disenchants_the_city_an_enemy_has_cursed_and_otherwise_leaves_it() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let (area, famine, own) = (
            spell(204 /* disenchant_area */)?,
            spell(189 /* famine */)?,
            spell(12 /* natures_eye */)?,
        );
        g.cheat_learn_spell(P0, area);
        treasury(&mut g, P0)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let (disjunction, awareness) = (spell(210 /* disjunction */)?, spell(209 /* awareness */)?);
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let blast = spell(58 /* spell_blast */)?;
        g.cheat_learn_spell(P0, blast);
        treasury(&mut g, P0)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        let aim = |g: &Game| pick_target(&PlayerView::new(g, P0), blast);
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(aim(&g).is_none(), "the rival casts nothing");
        g.scenario_casting(P1, spell(113 /* meteor_storm */)?, 100);
        assert_eq!(aim(&g), Some(Target::Player(P1)));
        Ok(())
    }

    /// Patch 1.50, note 271: the AI aims no curse at a wizard it has not met.
    #[test]
    fn a_wizard_blasts_nobody_it_has_not_met() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let blast = spell(58 /* spell_blast */)?;
        g.cheat_learn_spell(P0, blast);
        treasury(&mut g, P0)?.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        g.scenario_casting(P1, spell(113 /* meteor_storm */)?, 100);
        g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: P0,
            on: P1,
        }));
        assert!(pick_target(&PlayerView::new(&g, P0), blast).is_none());
        Ok(())
    }

    #[test]
    fn a_wizard_holds_an_enemy_stack_near_its_city_only_while_at_war_and_stronger_than_the_garrison(
    ) -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let stasis = spell(68 /* stasis */)?;
        g.cheat_learn_spell(P0, stasis);
        let near = at(COAST_CITY.0 - 1, COAST_CITY.1);
        spawn(&mut g, P1, near, 104 /* high_men_spearmen */, 8);
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
            104, /* high_men_spearmen */
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
        let recall = spell(47 /* word_of_recall */)?;
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
        let fighters = spawn(&mut g, P1, far, 104 /* high_men_spearmen */, 2);
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
        let heroism = spell(130 /* heroism */)?;
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

    #[test]
    fn the_mana_owed_for_the_spell_of_mastery_is_mana_to_buy_with_gold() -> TestResult {
        let (mut g, _) = summoner(50_000, 0)?;
        let before = mana_wanted(&PlayerView::new(&g, P1), &[]);
        let mastery = g
            .content()
            .rules
            .spells
            .iter()
            .find(|(_, d)| kind(d) == Kind::Mastery)
            .map(|(id, _)| id)
            .ok_or("a spell of mastery")?;
        g.cheat_learn_spell(P1, mastery);
        g.scenario_casting(P1, mastery, 3000);
        assert_eq!(
            mana_wanted(&PlayerView::new(&g, P1), &[]),
            before + 3000,
            "what is still owed is wanted on top"
        );
        Ok(())
    }

    /// Lycanthropy turns the obsolete (spearmen), not a unit the werewolf is no better than, and
    /// the unit's production cost weighs in the rating, not in the mana needed.
    #[test]
    fn lycanthropy_is_aimed_at_an_obsolete_unit_and_its_cost_lowers_the_rate() -> TestResult {
        let mut g = fresh_game()?;
        let lycanthropy = spell(173)?;
        g.cheat_learn_spell(P1, lycanthropy);
        let circle = g
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        let view = PlayerView::new(&g, P1);
        let spearmen_cost = {
            let rules = &view.content().rules;
            rules
                .units
                .get(rules.units.id_numbered(104).ok_or("spearmen")?)
                .cost as i32
        };
        drop(view);
        for u in PlayerView::new(&g, P1).my_units() {
            g.scenario_set_hurt(u, 0);
        }
        let paladins = spawn(&mut g, P1, circle, 113 /* high_men_paladins */, 1);
        let before = lycanthropy_target(&PlayerView::new(&g, P1), lycanthropy);
        assert!(
            before.is_none_or(|(u, ..)| u != paladins[0]),
            "a paladin is no better as a werewolf"
        );
        let spearmen = spawn(&mut g, P1, circle, 104 /* high_men_spearmen */, 1);
        let (aimed, extra, rate) = lycanthropy_target(&PlayerView::new(&g, P1), lycanthropy)
            .ok_or("an obsolete unit to turn")?;
        assert_eq!(aimed, spearmen[0]);
        assert_eq!(extra, spearmen_cost);
        // The unit given up is part of the price: the same gain for the spell's mana alone would
        // rate higher, as a summon of that strength does (it costs no unit).
        let view = PlayerView::new(&g, P1);
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

    /// The unit given up does not raise the mana needed: the spell's own cost with the usual
    /// margin is enough, and a spearman becomes a werewolf.
    #[test]
    fn lycanthropy_needs_only_the_spells_own_mana() -> TestResult {
        let mut g = fresh_game()?;
        let lycanthropy = spell(173)?;
        g.cheat_learn_spell(P1, lycanthropy);
        let circle = g
            .wizard(P1)
            .ok_or("player 1 is a wizard")?
            .summon_at()
            .ok_or("a summoning circle")?;
        // An army big enough that no summon is wanted, so the spell under test is the one left.
        spawn(&mut g, P1, circle, 104 /* high_men_spearmen */, 8);
        let land: Vec<_> = {
            let view = PlayerView::new(&g, P1);
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
            spawn(&mut g, P1, p, 104, 8);
        }
        let cost = PlayerView::new(&g, P1).casting_cost(lycanthropy);
        let ai = Ai::new(P1);
        let werewolves = |g: &Game| {
            PlayerView::new(g, P1)
                .my_units()
                .into_iter()
                .filter(|&u| {
                    PlayerView::new(g, P1)
                        .unit_def(u)
                        .is_some_and(|d| d.num == WEREWOLVES)
                })
                .count()
        };
        // The spell's cost with the margin, and nothing for the spearman.
        for _ in 0..40 {
            treasury(&mut g, P1)?.mana = HalfInt::from_int(cost * 3 / 2 + 1);
            // Turns kill units of a world with no food or gold: keep the army up.
            for p in &land {
                spawn(&mut g, P1, *p, 104, 3);
            }
            resolve_paid(&ai, &mut g, &mut Vec::new());
            cast(&ai, &mut g, &mut Vec::new());
            if werewolves(&g) > 0 {
                break;
            }
            g.apply(f4x_core::game::Command::EndTurn);
        }
        assert!(werewolves(&g) >= 1, "a spearman is a werewolf");
        Ok(())
    }
}
