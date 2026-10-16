//! A battle scenario: everything that makes one training battle (`docs/learning-ai.md` 7.5).
//!
//! A scenario is a plain value, written as RON and rebuilt into a game with [`crate::build`], so
//! any battle can be replayed, loaded into a test or kept in the evaluation suite. [`generate`]
//! makes random ones from the rules.

use f4x_core::content::Content;

use crate::obs::MAX_SPELLS;
use f4x_core::rules::Stat;
use f4x_core::rules::UnitKind;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;
use serde::{Deserialize, Serialize};

/// How the two armies compare. The attacker's side is the one the odds are named for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Odds {
    /// About equal: either side can win.
    Even,
    /// The attacker is much stronger: it should win with little loss.
    Overwhelming,
    /// The attacker is much weaker: it should sell its units dearly or flee.
    Overwhelmed,
}

/// Where the fight is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ground {
    /// Open ground.
    Field,
    /// The defender's city; with walls and a gate if `walls`.
    City { walls: bool },
}

/// One unit of an army.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitPick {
    /// The unit's id in the rules.
    pub unit: String,
    /// Experience points.
    pub xp: u32,
    /// Hit points already lost.
    pub hurt: u32,
}

/// A wizard behind one side. A side with none casts nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardSpec {
    /// Spell ids the wizard knows (those the game cannot cast in a battle are never offered).
    pub spells: Vec<String>,
    /// Mana in the reserve.
    pub mana: i32,
    /// Skill points, which set the casting skill for the battle.
    pub skill_points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleScenario {
    /// Seeds the game (the map, the wizards, the battle's dice).
    pub seed: u64,
    pub odds: Odds,
    pub ground: Ground,
    pub attackers: Vec<UnitPick>,
    pub defenders: Vec<UnitPick>,
    pub attacker_wizard: Option<WizardSpec>,
    pub defender_wizard: Option<WizardSpec>,
}

impl BattleScenario {
    pub fn to_text(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap_or_default()
    }

    pub fn from_text(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }
}

/// A rough strength of a unit, used only to sample armies of a wanted odds. It is not a unit
/// value and not part of any reward.
fn rough_strength(content: &Content, unit: &str) -> f32 {
    let rules = &content.rules;
    let Some(id) = rules.units.id_of(unit) else {
        return 0.0;
    };
    let d = rules.units.get(id);
    let s = |stat| d.stats.get(&stat).copied().unwrap_or(0).max(0) as f32;
    let figures = s(Stat::Figures).max(1.0);
    let attack = s(Stat::Melee) + 0.7 * s(Stat::Ranged);
    figures * (attack + 1.0) * (s(Stat::MaxHp).max(1.0) + s(Stat::Defense) * 0.6) / 4.0
}

/// The units that can fight in a generated army: a creature with an attack that walks or flies.
fn fighters(content: &Content) -> Vec<String> {
    use f4x_core::rules::MovementType;
    content
        .rules
        .units
        .iter()
        .filter(|(_, d)| d.kind != UnitKind::Hero)
        .filter(|(_, d)| !matches!(d.movement, MovementType::Sailing))
        .filter(|(_, d)| {
            d.stats.get(&Stat::Melee).copied().unwrap_or(0) > 0
                || d.stats.get(&Stat::Ranged).copied().unwrap_or(0) > 0
        })
        .map(|(_, d)| d.id.clone())
        .collect()
}

fn pick_army(
    content: &Content,
    pool: &[String],
    count: usize,
    rng: &mut Pcg64Mcg,
) -> Vec<UnitPick> {
    (0..count)
        .map(|_| {
            let unit = pool[rng.gen_range(0..pool.len())].clone();
            let xp = if rng.gen_bool(0.3) {
                rng.gen_range(0..120)
            } else {
                0
            };
            let hurt = if rng.gen_bool(0.15) {
                rng.gen_range(1..4)
            } else {
                0
            };
            let _ = content;
            UnitPick { unit, xp, hurt }
        })
        .collect()
}

fn power(content: &Content, army: &[UnitPick]) -> f32 {
    army.iter().map(|u| rough_strength(content, &u.unit)).sum()
}

/// Spells the game can cast in a battle that the agent has not been trained with: they aim at
/// a square of the field or at a wall, and the observation has no place for the walls and mud
/// they act on (Magic Vortex puts an object on the field the observation does not show). They
/// stay out of the generated wizards, so the learned brains keep their shape.
const NOT_TRAINED: [&str; 6] = [
    "earth_to_mud",
    "cracks_call",
    "disrupt",
    "life_drain",
    "word_of_recall",
    "magic_vortex",
];

/// A spell the game can cast in a battle at all: not a placeholder (`unimplemented`), and one
/// the agent trains with.
pub fn battle_spell(d: &f4x_core::rules::defs::SpellDef) -> bool {
    use f4x_core::rules::defs::CastContext;
    use f4x_core::rules::effects::Effect;
    d.contexts.contains(&CastContext::Battle)
        && !d.effects.iter().any(|r| {
            matches!(&r.effect, Effect::Custom(h) if h == "unimplemented" || NOT_TRAINED.contains(&h.as_str()))
        })
}

/// The realms a wizard's books can be in.
const REALMS: [&str; 5] = ["nature", "sorcery", "chaos", "life", "death"];

/// How many realms a generated wizard has books in: mostly one or two, as in real games, and
/// now and then three to five so the agent is not helpless against the rest.
fn realm_count(rng: &mut Pcg64Mcg) -> usize {
    match rng.gen_range(0..100) {
        0..=39 => 1,
        40..=89 => 2,
        _ => rng.gen_range(3..=REALMS.len()),
    }
}

fn wizard(content: &Content, rng: &mut Pcg64Mcg) -> Option<WizardSpec> {
    use rand::seq::SliceRandom;
    if rng.gen_bool(0.3) {
        return None;
    }
    let mut realms = REALMS.to_vec();
    realms.shuffle(rng);
    realms.truncate(realm_count(rng));
    let share = rng.gen_range(0.1..0.9);
    // The spells of the wizard's realms and the arcane ones, a share of them known.
    let mut spells: Vec<String> = content
        .rules
        .spells
        .iter()
        .filter(|(_, d)| battle_spell(d))
        .filter(|(_, d)| d.realm == "arcane" || realms.contains(&d.realm.as_str()))
        .filter(|_| rng.gen_bool(share))
        .map(|(_, d)| d.id.clone())
        .collect();
    // A wizard of many realms can know more than the agent has slots for; a random few are
    // left out (one or two realms never reach the limit, which a test checks).
    if spells.len() > MAX_SPELLS {
        spells.shuffle(rng);
        spells.truncate(MAX_SPELLS);
    }
    // Log-uniform: from a reserve that buys one spell to one that buys the whole battle.
    let mana = [0, 15, 40, 120, 400, 1500, 5000][rng.gen_range(0..7)];
    let skill_points = [0, 300, 1200, 4000, 12000, 30000][rng.gen_range(0..6)];
    Some(WizardSpec {
        spells,
        mana,
        skill_points,
    })
}

/// A random scenario from the rules, fixed by the seed.
pub fn generate(content: &Content, seed: u64) -> BattleScenario {
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x5eed_ba77_1e00);
    let pool = fighters(content);
    let odds = match rng.gen_range(0..4) {
        0 => Odds::Overwhelming,
        1 => Odds::Overwhelmed,
        _ => Odds::Even,
    };
    let ground = if rng.gen_bool(0.5) {
        Ground::Field
    } else {
        Ground::City {
            walls: rng.gen_bool(0.6),
        }
    };
    // The army sizes grow with the seed's draw: 1 v 1 up to full stacks.
    let max = [2usize, 4, 6, 9][rng.gen_range(0..4)];
    let defenders = pick_army(content, &pool, rng.gen_range(1..=max), &mut rng);
    let want = |ratio: f32, p: f32| ratio * p;
    let target_ratio = match odds {
        Odds::Even => rng.gen_range(0.8..1.25),
        Odds::Overwhelming => rng.gen_range(2.0..4.0),
        Odds::Overwhelmed => rng.gen_range(0.15..0.5),
    };
    let goal = want(target_ratio, power(content, &defenders));
    // Add random units until the attackers' strength reaches the goal (at most nine).
    let mut attackers: Vec<UnitPick> = Vec::new();
    while attackers.len() < 9 && power(content, &attackers) < goal {
        attackers.extend(pick_army(content, &pool, 1, &mut rng));
    }
    if attackers.is_empty() {
        attackers = pick_army(content, &pool, 1, &mut rng);
    }
    BattleScenario {
        seed,
        odds,
        ground,
        attackers,
        defenders,
        attacker_wizard: wizard(content, &mut rng),
        defender_wizard: wizard(content, &mut rng),
    }
}

/// A random scenario with no aim at any odds: both armies are drawn freely. The odds field is
/// a placeholder until [`crate::bank::calibrate`] measures it.
pub fn generate_raw(content: &Content, seed: u64) -> BattleScenario {
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x7a77_b4e1_5ce2);
    let pool = fighters(content);
    let ground = if rng.gen_bool(0.5) {
        Ground::Field
    } else {
        Ground::City {
            walls: rng.gen_bool(0.6),
        }
    };
    let max = [2usize, 4, 6, 9][rng.gen_range(0..4)];
    let attackers = pick_army(content, &pool, rng.gen_range(1..=max), &mut rng);
    let defenders = pick_army(content, &pool, rng.gen_range(1..=max), &mut rng);
    BattleScenario {
        seed,
        odds: Odds::Even,
        ground,
        attackers,
        defenders,
        attacker_wizard: wizard(content, &mut rng),
        defender_wizard: wizard(content, &mut rng),
    }
}
