//! `f4x-tools duel`: real battles between shipped units on the shipped rules, played by the battle
//! AI with the real engine, many of them, with the rules' invariants checked on every event.
//!
//! - `duel pairs`: every unit of a set against every unit of the set, one against one (the set is
//!   the units whose abilities carry an attack or a defence rule, or `--units`, or `--all`).
//! - `duel teams`: random groups of one to four units against each other.
//!
//! It reports the fights, the ones that never ended, the orders the engine refused, who won, and
//! a table of the win rate of each unit (`--csv FILE` writes all of it; it is the data for task
//! 655, the unit strength measure). The invariants (an `Unharmed` event names an immunity the
//! defender has; a shot that did damage was not stopped by an immunity on every type it carries)
//! are listed as violations and make the command fail.
//!
//! Tests: in the module below (a small set plays through and breaks no invariant).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use f4x_ai::battle::order::next_command;
use f4x_battle::battle::{Battle, BattleEvent};
use f4x_battle::map::{BattleMap, Pos};
use f4x_battle::Side;
use f4x_core::game::{Command, Game};
use f4x_core::map::{Grid, Map, Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::rules::damage_type::{DamageType, DamageTypes};
use f4x_def::content::Content;
use f4x_def::rules::immunity::Immunities;

pub const USAGE: &str = "usage: f4x-tools duel pairs|teams|ladder|heroes [--units N,N,...|--all] [--seeds N] [--battles N] [--max-team N] [--max-commands N] [--seed N] [--csv FILE] [--content DIR]
  pairs   every unit of the set against every unit of the set, --seeds fights each (default 1)
  teams   --battles random groups of 1 to --max-team units (default 6000 and 4)
  the set is --units, or --all, or by default the units that have an attack or a defence rule
  --max-commands is the orders a fight may take before it counts as never ended (default 3000)";

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
/// The attackers stand in the west, the defenders in the east, on open ground.
const WEST_X: i32 = 3;
const EAST_X: i32 = 14;

/// What one run found.
#[derive(Debug, Default)]
pub struct Report {
    pub fights: u32,
    pub never_ended: u32,
    pub refused: u32,
    pub skipped: Vec<u16>,
    pub winners: [u32; 2],
    pub immunity_events: BTreeMap<String, u32>,
    pub violations: Vec<String>,
    /// Per unit number: fights and wins.
    pub by_unit: BTreeMap<u16, (u32, u32)>,
    /// What each group battle of `teams` came to, for the table of how well the sum of the unit
    /// ratings (and, beside it, the old formula) tells the winner (task 721).
    pub groups: Vec<GroupFight>,
}

/// One battle of two groups: what each side is worth by the unit combat ratings and by the old
/// formula (`battle::power`), and who won.
#[derive(Debug, Clone, Copy)]
pub struct GroupFight {
    /// Attacker's and defender's worth by the ratings (`None` when a unit has no row).
    pub rating: Option<(f32, f32)>,
    pub formula: (f32, f32),
    pub winner: Option<Side>,
}

/// A small deterministic generator for the teams (no dependency for it).
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mode = args.first().map(String::as_str).ok_or(USAGE)?;
    if mode == "ladder" {
        return crate::ladder::run(&args[1..]);
    }
    if mode == "spells" {
        return crate::spell_duel::run(&args[1..]);
    }
    if mode == "heroes" {
        return crate::hero_duel::run(&args[1..]);
    }
    let (mut units, mut all, mut seeds, mut battles, mut max_team) =
        (Vec::<u16>::new(), false, 1u64, 6000u64, 4usize);
    let (mut max_commands, mut seed, mut csv, mut dir) = (
        3000usize,
        7u64,
        None::<PathBuf>,
        PathBuf::from(f4x_def::SHIPPED_CONTENT),
    );
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        let number = |s: String, name: &str| s.parse::<u64>().map_err(|_| format!("bad {name}"));
        match a.as_str() {
            "--units" => {
                for n in value("--units")?.split(',') {
                    units.push(n.trim().parse().map_err(|_| "bad unit number")?);
                }
            }
            "--all" => all = true,
            "--seeds" => seeds = number(value("--seeds")?, "--seeds")?,
            "--battles" => battles = number(value("--battles")?, "--battles")?,
            "--max-team" => max_team = number(value("--max-team")?, "--max-team")? as usize,
            "--max-commands" => {
                max_commands = number(value("--max-commands")?, "--max-commands")? as usize
            }
            "--seed" => seed = number(value("--seed")?, "--seed")?,
            "--csv" => csv = Some(value("--csv")?.into()),
            "--content" => dir = value("--content")?.into(),
            other => return Err(format!("unknown option {other:?}\n{USAGE}")),
        }
    }
    let content = Arc::new(
        f4x_def::load::load_content_layers(&[&dir, std::path::Path::new(f4x_def::SHIPPED_CONTENT)])
            .map_err(|e| e.to_string())?,
    );
    let set = if !units.is_empty() {
        units
    } else {
        unit_set(&content, all)
    };
    let report = match mode {
        "pairs" => pairs(&content, &set, seeds, max_commands),
        "teams" => teams(&content, &set, battles, max_team.max(1), seed, max_commands),
        _ => return Err(USAGE.to_string()),
    };
    print_report(&content, &report);
    if let Some(path) = csv {
        std::fs::write(&path, csv_text(&content, &report)).map_err(|e| e.to_string())?;
        println!("win rates of every unit: {}", path.display());
    }
    if report.violations.is_empty() && report.refused == 0 && report.never_ended == 0 {
        Ok(())
    } else {
        Err(format!(
            "{} violations, {} refused orders, {} fights never ended",
            report.violations.len(),
            report.refused,
            report.never_ended
        ))
    }
}

/// The unit numbers worth fighting: every unit, or those with an ability that carries an attack or
/// a defence rule, and the plain fighters of the content for contrast (those with no ability at all).
pub fn unit_set(content: &Content, all: bool) -> Vec<u16> {
    let ruled = |num: u16| {
        content
            .rules
            .abilities
            .iter()
            .any(|(_, a)| a.num == num && (a.attack.is_some() || !a.defence.is_empty()))
    };
    let mut set: Vec<u16> = content
        .rules
        .units
        .iter()
        .filter(|(_, u)| all || u.abilities.iter().any(|a| ruled(a.num())))
        .map(|(_, u)| u.num)
        .collect();
    set.sort_unstable();
    set
}

/// A battle between the two lists of (unit number, square) on open ground, set up by the game
/// itself (`BeginBattle`), with the dice from `seed`; `Err` when a unit does not enter a battle
/// (a ship, a settler).
pub fn make_battle(
    content: &Arc<Content>,
    ours: &[(u16, Pos)],
    theirs: &[(u16, Pos)],
    seed: u64,
) -> Result<Battle, String> {
    let ocean = content.terrains.id_numbered(8).ok_or("no ocean")?;
    let grass = content.terrains.id_numbered(4).ok_or("no grassland")?;
    let race = content.rules.races.id_numbered(7).ok_or("no race 7")?;
    let grid = || {
        let mut g = Grid::new(40, 14, ocean);
        for y in 3..=10 {
            for x in (3..=10).chain(28..=35) {
                g.set(x, y, grass);
            }
        }
        g
    };
    let mut game = Game::new(seed);
    game.set_content(Arc::clone(content));
    game.set_map(Map::new(grid(), grid()));
    game.add_player(P0, "West");
    game.add_player(P1, "East");
    for (owner, x, name) in [(P0, 10, "Westport"), (P1, 28, "Eastport")] {
        game.apply(Command::FoundCity {
            owner,
            pos: Position {
                plane: Plane::Arcanus,
                x,
                y: 6,
            },
            name: name.into(),
            race: Some(race),
            population: 8,
        });
    }
    let mut spawn = |owner: PlayerId, x: u32, list: &[(u16, Pos)]| -> Result<Vec<_>, String> {
        list.iter()
            .map(|&(num, _)| {
                let id = content
                    .rules
                    .units
                    .id_numbered(num)
                    .ok_or(format!("no unit {num}"))?;
                Ok(game.spawn_unit(
                    owner,
                    Position {
                        plane: Plane::Arcanus,
                        x,
                        y: 4,
                    },
                    id,
                ))
            })
            .collect()
    };
    let attackers = spawn(P0, 4, ours)?;
    let defenders = spawn(P1, 5, theirs)?;
    game.apply(Command::BeginBattle {
        attackers,
        defenders,
    });
    let mut b = game.battle().ok_or("no battle began")?.clone();
    b.map = BattleMap::open();
    let mut placed = vec![false; b.units.len()];
    let mut order = Vec::new();
    for (side, list) in [(Side::Attacker, ours), (Side::Defender, theirs)] {
        for &(num, cell) in list {
            let kind = content.rules.units.id_numbered(num);
            let at = b
                .units
                .iter()
                .position(|u| u.side == side && u.unit_id == kind && !placed[u.id])
                .ok_or(format!("unit {num} is not in the battle"))?;
            placed[at] = true;
            b.units[at].cell = cell;
            order.push(at);
        }
    }
    let mut units: Vec<_> = order.iter().map(|&i| b.units[i].clone()).collect();
    for (id, u) in units.iter_mut().enumerate() {
        u.id = id;
    }
    b.units = units;
    Ok(b)
}

/// Play the battle with the battle AI and fold what happened into the report. `label` names the
/// fight in a violation.
fn play(content: &Content, b: &mut Battle, label: &str, max_commands: usize, r: &mut Report) {
    r.fights += 1;
    let mut ended = false;
    for _ in 0..max_commands {
        if b.winner.is_some() {
            ended = true;
            break;
        }
        let command = next_command(b);
        let events = match b.apply(command.clone()) {
            Ok(events) => events,
            Err(e) => {
                r.refused += 1;
                note(r, format!("{label}: {command:?} refused: {e:?}"));
                break;
            }
        };
        for event in &events {
            check(content, b, event, label, r);
        }
    }
    if !ended && r.refused == 0 {
        r.never_ended += 1;
        note(r, format!("{label}: never ended"));
    }
    match b.winner {
        Some(Side::Attacker) => r.winners[0] += 1,
        Some(_) => r.winners[1] += 1,
        None => {}
    }
}

fn note(r: &mut Report, line: String) {
    if r.violations.len() < 100 {
        r.violations.push(line);
    }
}

/// The invariants of one event.
fn check(content: &Content, b: &Battle, event: &BattleEvent, label: &str, r: &mut Report) {
    let types_of = |abilities: &[f4x_core::rules::Ability]| -> Vec<DamageTypes> {
        abilities
            .iter()
            .filter_map(|ab| {
                let def = content
                    .rules
                    .abilities
                    .iter()
                    .find(|(_, a)| a.num == ab.num())?
                    .1;
                let attack = def.attack.as_ref()?;
                (!attack.types.is_empty())
                    .then(|| DamageTypes::of(&attack.types).with(DamageType::Ranged))
            })
            .collect()
    };
    match event {
        BattleEvent::Unharmed { defender, by, .. } => {
            *r.immunity_events.entry(format!("{by:?}")).or_default() += 1;
            if b.units[*defender].immune_type(&[*by], &content.rules) != Some(*by) {
                note(
                    r,
                    format!("{label}: Unharmed by {by:?} but the unit has no such rule"),
                );
            }
        }
        BattleEvent::SpellImmune { by: Some(by), .. } => {
            *r.immunity_events.entry(format!("{by:?}")).or_default() += 1;
        }
        BattleEvent::Shot {
            attacker,
            defender,
            dealt,
            ..
        } if *dealt > 0 => {
            let def = &b.units[*defender];
            let immunities = Immunities::of(&def.abilities, &def.enchantments, &content.rules);
            let sets = types_of(&b.units[*attacker].abilities);
            if !sets.is_empty() && sets.iter().all(|s| immunities.stopped_by(*s).is_some()) {
                note(
                    r,
                    format!(
                        "{label}: a shot dealt {dealt} though every type it carries is stopped"
                    ),
                );
            }
        }
        _ => {}
    }
}

fn credit(r: &mut Report, ours: &[u16], theirs: &[u16], winner: Option<Side>) {
    for (list, side) in [(ours, Side::Attacker), (theirs, Side::Defender)] {
        for &n in list {
            let e = r.by_unit.entry(n).or_default();
            e.0 += 1;
            e.1 += u32::from(winner == Some(side));
        }
    }
}

pub fn pairs(content: &Arc<Content>, set: &[u16], seeds: u64, max_commands: usize) -> Report {
    let mut r = Report::default();
    let mut left_out = Vec::new();
    // Units that cannot fight in a plain battle (ships, settlers) are left out.
    let set: Vec<u16> = set
        .iter()
        .copied()
        .filter(|&n| {
            let fine = make_battle(content, &[(n, (WEST_X, 10))], &[(n, (6, 10))], 1).is_ok();
            if !fine {
                left_out.push(n);
            }
            fine
        })
        .collect();
    r.skipped = left_out;
    for &a in &set {
        for &d in &set {
            for s in 0..seeds {
                let seed = u64::from(a) * 100_000 + u64::from(d) * 100 + s;
                let Ok(mut b) = make_battle(content, &[(a, (WEST_X, 10))], &[(d, (6, 10))], seed)
                else {
                    continue;
                };
                play(
                    content,
                    &mut b,
                    &format!("{a} v {d} seed {seed}"),
                    max_commands,
                    &mut r,
                );
                credit(&mut r, &[a], &[d], b.winner);
            }
        }
    }
    r
}

pub fn teams(
    content: &Arc<Content>,
    set: &[u16],
    battles: u64,
    max_team: usize,
    seed: u64,
    max_commands: usize,
) -> Report {
    let mut r = Report::default();
    // A hero's rating is of a hero with rolled abilities, which a plain battle does not give it:
    // the groups are of the other units.
    let is_hero = |n: u16| {
        content
            .rules
            .units
            .iter()
            .any(|(_, u)| u.num == n && u.kind == f4x_def::rules::UnitKind::Hero)
    };
    let set: Vec<u16> = set
        .iter()
        .copied()
        .filter(|&n| !is_hero(n))
        .filter(|&n| make_battle(content, &[(n, (WEST_X, 10))], &[(n, (6, 10))], 1).is_ok())
        .collect();
    if set.is_empty() {
        return r;
    }
    let mut rng = Lcg(seed);
    for fight in 0..battles {
        let mut pick = |x: i32| -> Vec<(u16, Pos)> {
            (0..1 + rng.below(max_team))
                .map(|i| (set[rng.below(set.len())], (x, 6 + 2 * i as i32)))
                .collect()
        };
        let (ours, theirs) = (pick(WEST_X), pick(EAST_X));
        let Ok(mut b) = make_battle(content, &ours, &theirs, seed.wrapping_add(fight)) else {
            continue;
        };
        let label = format!("{ours:?} v {theirs:?} seed {}", seed.wrapping_add(fight));
        play(content, &mut b, &label, max_commands, &mut r);
        let (o, t): (Vec<u16>, Vec<u16>) = (
            ours.iter().map(|x| x.0).collect(),
            theirs.iter().map(|x| x.0).collect(),
        );
        credit(&mut r, &o, &t, b.winner);
        r.groups.push(group_fight(content, &o, &t, b.winner));
    }
    r
}

/// The worth of a group of recruits by the ratings (the strongest `max_battle_side` of them
/// added, as the game's forecast does) and by the old formula.
fn group_fight(
    content: &Content,
    ours: &[u16],
    theirs: &[u16],
    winner: Option<Side>,
) -> GroupFight {
    let rules = &content.rules;
    let max = f4x_def::settings::MAX_STACK;
    let worth = |list: &[u16], rated: bool| -> Option<f32> {
        let mut v: Vec<f32> = list
            .iter()
            .map(|&n| {
                let d = rules.units.iter().find(|(_, u)| u.num == n)?.1;
                if rated {
                    rules.combat_rating(d, 0)
                } else {
                    Some(f4x_battle::power::def_formula(d))
                }
            })
            .collect::<Option<Vec<f32>>>()?;
        v.sort_by(|a, b| b.total_cmp(a));
        Some(v.iter().take(max).sum())
    };
    GroupFight {
        rating: worth(ours, true).zip(worth(theirs, true)),
        formula: (
            worth(ours, false).unwrap_or(0.0),
            worth(theirs, false).unwrap_or(0.0),
        ),
        winner,
    }
}

/// The share of group battles the side with the larger sum wins, by the ratings and by the old
/// formula, and by how much larger the sum is (task 721).
fn print_groups(r: &Report) {
    if r.groups.is_empty() {
        return;
    }
    // (larger side won, smaller side won, draw) over the battles with a larger side.
    let tally = |worth: &dyn Fn(&GroupFight) -> Option<(f32, f32)>, from: f32, to: f32| {
        let mut t = [0u32; 3];
        for g in &r.groups {
            let Some((a, d)) = worth(g) else { continue };
            let (hi, lo) = (a.max(d), a.min(d));
            if lo <= 0.0 || a == d || hi / lo < from || hi / lo >= to {
                continue;
            }
            let larger = if a > d {
                Side::Attacker
            } else {
                Side::Defender
            };
            match g.winner {
                Some(w) if w == larger => t[0] += 1,
                Some(_) => t[1] += 1,
                None => t[2] += 1,
            }
        }
        t
    };
    let by_rating = |g: &GroupFight| g.rating;
    let by_formula = |g: &GroupFight| Some(g.formula);
    let line = |name: &str, t: [u32; 3]| {
        let n = t.iter().sum::<u32>().max(1);
        println!(
            "  {name:<8} {:>6} battles: larger sum wins {:>5.1}%, smaller {:>5.1}%, draw {:>4.1}%",
            t.iter().sum::<u32>(),
            100.0 * f64::from(t[0]) / f64::from(n),
            100.0 * f64::from(t[1]) / f64::from(n),
            100.0 * f64::from(t[2]) / f64::from(n),
        );
    };
    println!("group battles: the side with the larger sum of ratings, and of the old formula:");
    line("ratings", tally(&by_rating, 0.0, f32::INFINITY));
    line("formula", tally(&by_formula, 0.0, f32::INFINITY));
    println!("by how much larger the sum of the ratings is (larger over smaller):");
    let edges = [1.0, 1.2, 1.5, 2.0, 3.0, 5.0, f32::INFINITY];
    for w in edges.windows(2) {
        line(&format!("{}-{}", w[0], w[1]), tally(&by_rating, w[0], w[1]));
    }
}

fn unit_name(content: &Content, num: u16) -> String {
    content
        .rules
        .units
        .iter()
        .find(|(_, u)| u.num == num)
        .map_or_else(|| "?".to_string(), |(_, u)| u.id.clone())
}

fn rates(report: &Report) -> Vec<(u16, u32, u32)> {
    let mut v: Vec<_> = report
        .by_unit
        .iter()
        .map(|(&n, &(fights, wins))| (n, fights, wins))
        .collect();
    v.sort_by(|a, b| {
        let (ra, rb) = (
            a.2 as f64 / a.1.max(1) as f64,
            b.2 as f64 / b.1.max(1) as f64,
        );
        rb.total_cmp(&ra).then(a.0.cmp(&b.0))
    });
    v
}

fn csv_text(content: &Content, report: &Report) -> String {
    let mut out = String::from("unit,id,fights,wins,win_rate\n");
    for (n, fights, wins) in rates(report) {
        out += &format!(
            "{n},{},{fights},{wins},{:.3}\n",
            unit_name(content, n),
            f64::from(wins) / f64::from(fights.max(1))
        );
    }
    out
}

fn print_report(content: &Content, r: &Report) {
    println!(
        "fights {}, never ended {}, refused orders {}, left out (do not fight) {:?}",
        r.fights, r.never_ended, r.refused, r.skipped
    );
    println!("winners attacker/defender: {:?}", r.winners);
    println!(
        "immunity lines in the log, by type: {:?}",
        r.immunity_events
    );
    let table = rates(r);
    let show = |title: &str, rows: &[(u16, u32, u32)]| {
        println!("{title}:");
        for &(n, fights, wins) in rows {
            println!(
                "  {n:>4} {:<26} {wins}/{fights} ({:.0}%)",
                unit_name(content, n),
                100.0 * f64::from(wins) / f64::from(fights.max(1))
            );
        }
    };
    if table.len() > 20 {
        show("strongest by win rate", &table[..10]);
        show("weakest by win rate", &table[table.len() - 10..]);
    } else {
        show("win rate", &table);
    }
    print_groups(r);
    for v in &r.violations {
        println!("VIOLATION {v}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::shipped;

    fn content() -> Arc<Content> {
        Arc::new(shipped().clone())
    }

    #[test]
    fn a_team_run_keeps_the_worth_of_both_sides_of_every_group_battle() {
        let c = content();
        let set = unit_set(&c, false);
        let r = teams(&c, &set, 30, 3, 1, 3000);
        assert_eq!(r.groups.len() as u32, r.fights);
        assert!(r
            .groups
            .iter()
            .any(|g| g.rating.is_some_and(|(a, d)| a > 0.0 && d > 0.0)));
        assert!(r
            .groups
            .iter()
            .all(|g| g.formula.0 > 0.0 && g.formula.1 > 0.0));
    }

    #[test]
    fn the_default_set_holds_units_with_attack_and_defence_rules() {
        let c = content();
        let set = unit_set(&c, false);
        assert!(set.len() > 20, "{}", set.len());
        assert!(unit_set(&c, true).len() > set.len());
    }

    #[test]
    fn a_missile_shot_at_a_unit_immune_to_missiles_is_logged_and_does_no_harm() {
        let c = content();
        // The units with missile immunity and a bow come from the data, not from numbers.
        let immune = c
            .rules
            .abilities
            .iter()
            .find(|(_, a)| {
                a.defence.iter().any(|d| {
                    d.effect == f4x_core::rules::damage_type::DefenceEffect::Immune
                        && d.against.contains(DamageType::Missile)
                })
            })
            .map(|(_, a)| a.num)
            .expect("an ability is immune to missiles");
        let target = c
            .rules
            .units
            .iter()
            .find(|(_, u)| u.abilities.iter().any(|a| a.num() == immune))
            .map(|(_, u)| u.num)
            .expect("a unit has it");
        // The first unit with a missile attack that can shoot a target at hand.
        let shoots_missiles = |num: u16| {
            c.rules.units.iter().any(|(_, u)| {
                u.num == num
                    && u.abilities.iter().any(|a| {
                        c.rules.abilities.iter().any(|(_, d)| {
                            d.num == a.num()
                                && d.attack
                                    .as_ref()
                                    .is_some_and(|x| x.types.contains(&DamageType::Missile))
                        })
                    })
            })
        };
        let mut logged = false;
        for (_, u) in c.rules.units.iter().filter(|(_, u)| shoots_missiles(u.num)) {
            let mut b = make_battle(&c, &[(u.num, (3, 10))], &[(target, (6, 10))], 1)
                .expect("the battle starts");
            let Ok(events) =
                b.apply(f4x_battle::battle::BattleCommand::Shoot { unit: 0, target: 1 })
            else {
                continue;
            };
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    BattleEvent::Unharmed {
                        by: DamageType::Missile,
                        ..
                    }
                )),
                "{} shot at a unit immune to missiles: {events:?}",
                u.id
            );
            logged = true;
            break;
        }
        assert!(logged, "no unit could shoot");
    }

    #[test]
    fn a_small_set_plays_through_without_a_violation() {
        let c = content();
        let set: Vec<u16> = unit_set(&c, false).into_iter().take(12).collect();
        let r = pairs(&c, &set, 1, 3000);
        assert!(r.fights > 100, "{}", r.fights);
        assert_eq!((r.refused, r.never_ended), (0, 0));
        assert!(r.violations.is_empty(), "{:?}", r.violations);
        let t = teams(&c, &set, 50, 3, 7, 3000);
        assert_eq!((t.refused, t.never_ended), (0, 0), "{:?}", t.violations);
    }
}
