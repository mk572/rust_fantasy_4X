//! `f4x-tools duel spells`: what a lasting spell on a unit does to a fight, against every unit.
//!
//! One reference unit (default 47, the beastmen swordsmen: a mid-strength plain soldier) fights
//! every unit one against one on open ground with `--seeds` fights each (half of them with the
//! reference as the attacker), four times over:
//!
//! - `base`: nothing cast;
//! - `ours`: the spell is on the reference unit (a spell for one's own unit), or the reference side
//!   casts it on the opponent (a spell for an enemy unit);
//! - `theirs`: the same for the opponent: the spell on the opponent, or the opponent side casts it
//!   on the reference unit.
//!
//! The question is not the average (that is the unit ranking) but the *outliers*: the opponents
//! against which the spell changes the win rate a lot. The CSV has one row per opponent with
//! its features (kind, realm, abilities, stats), so a rule such as "against undead, spells A, B and
//! C gain the most" can be read off it. A spell for one's own unit is put on before the battle, as
//! an overland cast would be; a spell for an enemy unit is cast in the battle (the wizard of the
//! casting side knows it and has the mana), on its first turn. Documented in `docs/duel.md`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use f4x_ai::battle::cast::next_command;
use f4x_battle::battle::BattleCommand;
use f4x_battle::battle::BattleEvent;
use f4x_battle::battlemagic::caster::SpellAim;
use f4x_battle::map::BattleMap;
use f4x_battle::Side;
use f4x_core::player::PlayerId;
use f4x_core::world::FIRST_ROUND;
use f4x_core::world::{Command, Event, World};
use f4x_def::content::Content;
use f4x_def::rules::{CastContext, SpellId, SpellTarget};

use crate::ladder::base_game;

pub const USAGE: &str = "usage: f4x-tools duel spells [--unit N (default 47)] [--level 1-6 (default 1) | --balance] [--against N,N,..] [--hurt PERCENT] [--count N] [--spells N,N,.. | --all] [--seeds N (default 20)] [--jobs N] [--out DIR (default duel-out/spells)] [--content DIR]";

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const MAX_COMMANDS: usize = 3000;
/// `--trace OPPONENT`: print the orders and events of the first fight against that opponent.
static TRACE: AtomicUsize = AtomicUsize::new(usize::MAX);

/// `--hurt PERCENT`: both units start the fight with this share of their hit points lost (for the
/// healing spells, which have nothing to do in a fight between two whole units).
static HURT: AtomicU32 = AtomicU32::new(0);

/// `--count N`: each side fields N copies of its unit (default 1), the spell goes to or from the
/// first of the side (for the spells that need allies: Confusion lends a unit to the enemy).
static COUNT: AtomicUsize = AtomicUsize::new(1);

/// What fights what: the reference unit at a level against an opponent, and the spell in play.
#[derive(Debug, Clone, Copy)]
struct Pairing {
    reference: u16,
    level: usize,
    other: u16,
    spell: SpellId,
}

/// Who gets the spell in a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variant {
    Base,
    /// The reference unit's side gains from the spell.
    Ours,
    /// The opponent's side gains from it.
    Theirs,
}

/// The wins of the reference unit over the fights of one variant.
#[derive(Debug, Default, Clone, Copy)]
struct Wins {
    won: u32,
    lost: u32,
    drawn: u32,
    /// Casts that were refused or failed (the spell did not land).
    missed: u32,
}

/// Does the spell go on one's own unit (as against an enemy's)?
fn for_own(def: &f4x_def::rules::SpellDef) -> bool {
    matches!(def.target, SpellTarget::OwnUnit)
}

/// The spells this harness can run: a lasting or harmful spell on one unit.
fn unit_spell(def: &f4x_def::rules::SpellDef) -> bool {
    matches!(
        def.target,
        SpellTarget::OwnUnit | SpellTarget::EnemyUnit | SpellTarget::AnyUnit
    )
}

/// One fight of the reference unit against `other`. Returns the outcome for the reference unit
/// (`Some(true)` win, `Some(false)` loss, `None` draw) and whether a cast missed.
fn fight(
    content: &Arc<Content>,
    pairing: Pairing,
    variant: Variant,
    reference_attacks: bool,
    seed: u64,
) -> Result<(Option<bool>, bool), String> {
    let Pairing {
        reference,
        level,
        other,
        spell,
    } = pairing;
    let def = content.rules.spells.get(spell);
    let own = for_own(def);
    let (mut world, mut rng) = base_game(content, seed)?;
    let (ref_owner, other_owner) = if reference_attacks {
        (P0, P1)
    } else {
        (P1, P0)
    };
    let spawn = |world: &mut World, owner: PlayerId, x: u32, num: u16| -> Result<_, String> {
        let id = content
            .rules
            .units
            .id_numbered(num)
            .ok_or(format!("no unit {num}"))?;
        Ok(world.spawn_unit(
            owner,
            f4x_core::map::Position {
                plane: f4x_core::map::Plane::Arcanus,
                x,
                y: 4,
            },
            id,
        ))
    };
    let (rx, ox) = if reference_attacks { (4, 5) } else { (5, 4) };
    let count = COUNT.load(Ordering::Relaxed).max(1);
    let mut r_units = Vec::new();
    let mut o_units = Vec::new();
    for _ in 0..count {
        r_units.push(spawn(&mut world, ref_owner, rx, reference)?);
        o_units.push(spawn(&mut world, other_owner, ox, other)?);
    }
    let (r_unit, o_unit) = (r_units[0], o_units[0]);
    for &unit in &r_units {
        crate::ladder::give_level(&mut world, content, unit, ref_owner, reference, level);
    }
    let hurt = HURT.load(Ordering::Relaxed);
    if hurt > 0 {
        for (unit, num) in r_units
            .iter()
            .map(|&u| (u, reference))
            .chain(o_units.iter().map(|&u| (u, other)))
        {
            let points = content
                .rules
                .units
                .iter()
                .find(|(_, d)| d.num == num)
                .map_or(0, |(_, d)| {
                    let stat = |s: f4x_def::rules::Stat| d.stats.get(&s).map_or(0, |v| v.floor());
                    stat(f4x_def::rules::Stat::MaxHp) * stat(f4x_def::rules::Stat::Figures).max(1)
                });
            world.scenario_set_hurt(unit, u32::try_from(points).unwrap_or(0) * hurt / 100);
        }
    }
    // The unit that gains from the spell, and who casts it, when it is cast in the battle.
    let (gainer, gainer_owner, caster_owner) = match variant {
        Variant::Base => (None, ref_owner, ref_owner),
        Variant::Ours => (Some(r_unit), ref_owner, ref_owner),
        Variant::Theirs => (Some(o_unit), other_owner, other_owner),
    };
    let mut pending = None;
    // A healing spell is no enchantment: the wizard of the side that gains from it knows it and
    // has the mana, and the battle AI casts it when it sees fit.
    let mut healer = None;
    if let Some(gain) = gainer {
        if own && def.duration == f4x_def::rules::Duration::Instant {
            world.cheat_learn_spell(gainer_owner, spell);
            healer = Some(gainer_owner);
        } else if own {
            // Put on before the battle, as an overland cast would be.
            world.scenario_enchant_unit(gain, spell, gainer_owner);
        } else {
            // A spell for an enemy unit: cast in the battle by the side that gains from it, on the
            // other unit.
            let target = if gain == r_unit { o_unit } else { r_unit };
            pending = Some((caster_owner, target));
            world.cheat_learn_spell(caster_owner, spell);
        }
    }
    let (attackers, defenders) = if reference_attacks {
        (r_units, o_units)
    } else {
        (o_units, r_units)
    };
    world.apply(
        Command::BeginBattle {
            attackers,
            defenders,
        },
        FIRST_ROUND,
        &mut rng,
    );
    if world.battle().is_none() {
        return Err("no battle began".into());
    }
    world.scenario_edit_battle(|b| b.map = BattleMap::open(*b.map.movement()));
    if let Some(owner) = healer {
        let side = if owner == P0 {
            Side::Attacker
        } else {
            Side::Defender
        };
        world.scenario_battle_caster(side, 1000);
    }
    let mut missed = false;
    let mut exhausted = false;
    for _ in 0..MAX_COMMANDS {
        let Some(battle) = world.battle() else { break };
        if let Some((owner, target)) = pending {
            let side = if owner == P0 {
                Side::Attacker
            } else {
                Side::Defender
            };
            if battle.active == side {
                pending = None;
                let target_side = side.other();
                let index = battle
                    .units
                    .iter()
                    .position(|u| u.side == target_side)
                    .ok_or("no target in the battle")?;
                let _ = target;
                world.scenario_battle_caster(side, 1000);
                let events = world.apply(
                    Command::Battle(BattleCommand::Cast {
                        spell,
                        aim: SpellAim::Unit(index),
                        mana: 0,
                    }),
                    FIRST_ROUND,
                    &mut rng,
                );
                if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
                    missed = true;
                }
                continue;
            }
        }
        let Some(command) = next_command(&world) else {
            break;
        };
        let events = world.apply(Command::Battle(command.clone()), FIRST_ROUND, &mut rng);
        if TRACE.load(Ordering::Relaxed) == usize::from(other) {
            println!("{command:?} -> {events:?}");
        }
        for event in &events {
            match event {
                Event::Rejected(why) => return Err(format!("{command:?} refused: {why:?}")),
                Event::Battle(BattleEvent::Exhausted) => exhausted = true,
                Event::BattleEnded { winner, .. } => {
                    let outcome = if exhausted {
                        None
                    } else {
                        Some(*winner == ref_owner)
                    };
                    return Ok((outcome, missed));
                }
                _ => {}
            }
        }
    }
    Err("never ended".into())
}

fn seed_of(parts: &[u64]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in parts {
        h = (h ^ p).wrapping_mul(0x0100_0000_01b3).rotate_left(17);
    }
    h
}

/// The fights of one variant against one opponent: the same dice for every variant, so the
/// difference between two variants is the spell.
fn series(
    content: &Arc<Content>,
    pairing: Pairing,
    variant: Variant,
    seeds: u64,
    errors: &mut Vec<String>,
) -> Wins {
    let Pairing {
        reference,
        level,
        other,
        ..
    } = pairing;
    let mut w = Wins::default();
    for i in 0..seeds {
        let seed = seed_of(&[u64::from(reference), level as u64, u64::from(other), i]);
        match fight(content, pairing, variant, i % 2 == 0, seed) {
            Ok((Some(true), m)) => {
                w.won += 1;
                w.missed += u32::from(m);
            }
            Ok((Some(false), m)) => {
                w.lost += 1;
                w.missed += u32::from(m);
            }
            Ok((None, m)) => {
                w.drawn += 1;
                w.missed += u32::from(m);
            }
            Err(e) => errors.push(format!(
                "{reference} v {other} {variant:?} seed {seed}: {e}"
            )),
        }
    }
    w
}

/// What an opponent is, for the CSV: kind, realm, flies, ranged, and its stats and abilities.
fn features(content: &Content, num: u16) -> String {
    let Some((_, d)) = content.rules.units.iter().find(|(_, d)| d.num == num) else {
        return String::new();
    };
    let stat = |s: f4x_def::rules::Stat| d.stats.get(&s).map_or(0, |v| v.floor());
    let abilities: Vec<String> = d.abilities.iter().map(|a| a.num().to_string()).collect();
    format!(
        "{:?},{},{},{},{},{},{},{},{},{}",
        d.kind,
        d.realm.map_or(-1, |r| i32::from(r.num())),
        i32::from(d.movement == f4x_def::rules::MovementType::Flying),
        stat(f4x_def::rules::Stat::Melee),
        stat(f4x_def::rules::Stat::Ranged),
        stat(f4x_def::rules::Stat::Defense),
        stat(f4x_def::rules::Stat::Resistance),
        stat(f4x_def::rules::Stat::MaxHp),
        stat(f4x_def::rules::Stat::Figures),
        abilities.join("+"),
    )
}

pub fn run(args: &[String]) -> Result<(), String> {
    let (mut reference, mut spells, mut all, mut seeds, mut jobs) =
        (47u16, Vec::<u16>::new(), false, 20u64, 4usize);
    let (mut level, mut balance) = (1usize, false);
    let mut against = Vec::<u16>::new();
    let mut out = PathBuf::from("duel-out/spells");
    let mut dir = PathBuf::from(f4x_def::SHIPPED_CONTENT);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |n: &str| it.next().cloned().ok_or(format!("{n} needs a value"));
        match a.as_str() {
            "--unit" => reference = value("--unit")?.parse().map_err(|_| "bad unit")?,
            "--spells" => {
                for n in value("--spells")?.split(',') {
                    spells.push(n.trim().parse().map_err(|_| "bad spell number")?);
                }
            }
            "--all" => all = true,
            "--level" => level = value("--level")?.parse().map_err(|_| "bad level")?,
            "--balance" => balance = true,
            "--count" => {
                COUNT.store(
                    value("--count")?.parse().map_err(|_| "bad count")?,
                    Ordering::Relaxed,
                );
            }
            "--hurt" => {
                HURT.store(
                    value("--hurt")?.parse().map_err(|_| "bad percent")?,
                    Ordering::Relaxed,
                );
            }
            "--against" => {
                for n in value("--against")?.split(',') {
                    against.push(n.trim().parse().map_err(|_| "bad unit number")?);
                }
            }
            "--trace" => {
                // One fight of the reference as the attacker against this opponent, narrated.
                TRACE.store(
                    value("--trace")?.parse().map_err(|_| "bad unit")?,
                    Ordering::Relaxed,
                );
                seeds = 1;
                jobs = 1;
            }
            "--seeds" => seeds = value("--seeds")?.parse().map_err(|_| "bad seeds")?,
            "--jobs" => jobs = value("--jobs")?.parse().map_err(|_| "bad jobs")?,
            "--out" => out = value("--out")?.into(),
            "--content" => dir = value("--content")?.into(),
            other => return Err(format!("unknown option {other:?}\n{USAGE}")),
        }
    }
    let content = Arc::new(
        f4x_def::load::load_content_layers(&[&dir, std::path::Path::new(f4x_def::SHIPPED_CONTENT)])
            .map_err(|e| e.to_string())?,
    );
    if all {
        spells = content
            .rules
            .spells
            .iter()
            .filter(|(_, d)| {
                unit_spell(d)
                    && (d.contexts.contains(&CastContext::Battle)
                        || d.contexts.contains(&CastContext::Overland))
            })
            .map(|(_, d)| d.num)
            .collect();
    }
    if spells.is_empty() {
        return Err(format!("no spell given\n{USAGE}"));
    }
    let opponents: Vec<u16> = content
        .rules
        .units
        .iter()
        .filter(|(_, d)| d.is_fighter())
        .map(|(_, d)| d.num)
        .filter(|n| against.is_empty() || against.contains(n))
        .collect();
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for spell_num in spells {
        let Some(spell) = content.rules.spells.id_numbered(spell_num) else {
            return Err(format!("no spell {spell_num}"));
        };
        let spell_def = content.rules.spells.get(spell);
        let rows = Mutex::new(Vec::<(u16, usize, Wins, Wins, Wins)>::new());
        let errors = Mutex::new(Vec::<String>::new());
        let next = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..jobs.max(1) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&other) = opponents.get(i) else {
                        break;
                    };
                    let mut errs = Vec::new();
                    // The level of the reference unit: given, or the one whose fights against this
                    // opponent are closest to even (so that a spell has a fight to change).
                    let level = if balance {
                        (1..=6usize)
                            .map(|l| {
                                let pairing = Pairing {
                                    reference,
                                    level: l,
                                    other,
                                    spell,
                                };
                                let w = series(
                                    &content,
                                    pairing,
                                    Variant::Base,
                                    seeds.min(20),
                                    &mut Vec::new(),
                                );
                                let n = f64::from(w.won + w.lost + w.drawn).max(1.0);
                                let share = (f64::from(w.won) + 0.5 * f64::from(w.drawn)) / n;
                                ((share - 0.5).abs(), l)
                            })
                            .min_by(|a, b| a.0.total_cmp(&b.0))
                            .map_or(1, |(_, l)| l)
                    } else {
                        level
                    };
                    let pairing = Pairing {
                        reference,
                        level,
                        other,
                        spell,
                    };
                    let base = series(&content, pairing, Variant::Base, seeds, &mut errs);
                    let ours = series(&content, pairing, Variant::Ours, seeds, &mut errs);
                    let theirs = series(&content, pairing, Variant::Theirs, seeds, &mut errs);
                    if let Ok(mut e) = errors.lock() {
                        e.extend(errs);
                    }
                    if let Ok(mut r) = rows.lock() {
                        r.push((other, level, base, ours, theirs));
                    }
                });
            }
        });
        let mut rows = rows.into_inner().map_err(|e| e.to_string())?;
        rows.sort_by_key(|r| r.0);
        let mut csv = String::from(
            "opponent,id,kind,realm,flies,melee,ranged,defense,resistance,hp,figures,abilities,level,base_won,base_lost,base_drawn,ours_won,ours_lost,ours_drawn,ours_missed,theirs_won,theirs_lost,theirs_drawn,theirs_missed\n",
        );
        for (other, lvl, b, o, t) in &rows {
            let id = content
                .rules
                .units
                .iter()
                .find(|(_, d)| d.num == *other)
                .map_or(String::new(), |(_, d)| d.id.clone());
            csv += &format!(
                "{other},{id},{},{lvl},{},{},{},{},{},{},{},{},{},{},{}\n",
                features(&content, *other),
                b.won,
                b.lost,
                b.drawn,
                o.won,
                o.lost,
                o.drawn,
                o.missed,
                t.won,
                t.lost,
                t.drawn,
                t.missed,
            );
        }
        let path = out.join(format!("spell_{spell_num}_{}.csv", spell_def.id));
        std::fs::write(&path, csv).map_err(|e| e.to_string())?;
        let errs = errors.into_inner().map_err(|e| e.to_string())?;
        let rate = |f: fn(&(u16, usize, Wins, Wins, Wins)) -> Wins| {
            let (mut won, mut all) = (0u32, 0u32);
            for r in &rows {
                let w = f(r);
                won += w.won;
                all += w.won + w.lost + w.drawn;
            }
            f64::from(won) / f64::from(all.max(1))
        };
        println!(
            "spell {spell_num} {}: {} opponents, win rate base {:.3} ours {:.3} theirs {:.3}, {} errors -> {}",
            spell_def.id,
            rows.len(),
            rate(|r| r.2),
            rate(|r| r.3),
            rate(|r| r.4),
            errs.len(),
            path.display()
        );
        for e in errs.iter().take(3) {
            println!("  {e}");
        }
    }
    Ok(())
}
