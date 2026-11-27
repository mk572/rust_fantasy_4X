//! Regression tests on the whole game, without the client (`docs/rust_coding_rules.md`).
//!
//! Each scenario loads a prepared save from `tests/regression/<name>.save`, applies a fixed list
//! of commands and compares the resulting game with `tests/regression/<name>.target`. To change
//! a target on purpose or rebuild the saves, see `docs/rust_coding_rules.md` (`F4X_REFRESH_REGRESSION`).

use std::path::{Path, PathBuf};

use f4x_core::player::PlayerId;
use f4x_core::world::save::ContentCheck;
use f4x_core::world::{Command, World};
use f4x_def::content::Content;
use f4x_def::testdir::TestResult;
use f4x_master::{Game, SaveFile};
use f4x_newgame::setup::{new_game, NewGame};

fn shipped() -> &'static Content {
    f4x_def::testdir::shipped()
}

fn folder() -> PathBuf {
    Path::new("../../tests/regression").to_path_buf()
}

fn end_turns(w: &mut Game, n: usize) {
    for _ in 0..n {
        w.play_round();
    }
}

fn fresh(seed: u64) -> Result<Game, f4x_newgame::error::NewGameError> {
    new_game(
        shipped(),
        &NewGame {
            seed,
            all_ai: true,
            ..Default::default()
        },
    )
    .map(Game::from)
}

/// A game a few turns old: the first ten turns of the AI's opening.
fn early() -> Result<Game, String> {
    let mut w = fresh(11)?;
    end_turns(&mut w, 2);
    Ok(w)
}

/// A game in the middle, with the first two wizards at war.
fn war() -> Result<Game, String> {
    let mut w = fresh(21)?;
    end_turns(&mut w, 40);
    let ids = w.player_ids();
    let (a, b) = (ids[0], ids[1]);
    w.declare_war(a, b);
    Ok(w)
}

/// A game in which the first wizard keeps three global enchantments going.
fn enchanted() -> Result<Game, String> {
    let mut w = fresh(31)?;
    let me = w.player_ids()[0];
    w.treasury_mut(me).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(30_000);
    for id in [
        57,  /* wind_mastery */
        158, /* crusade */
        157, /* tranquility */
    ] {
        let spell = w
            .content()
            .rules
            .spells
            .id_numbered(id)
            .ok_or_else(|| format!("no spell {id}"))?;
        w.cheat_learn_spell(me, spell);
        let events = w.command(Command::CastSpell {
            player: me,
            spell,
            target: None,
            extra: 0,
        });
        assert!(
            events.is_empty()
                || !events
                    .iter()
                    .any(|e| matches!(e, f4x_core::world::Event::Rejected(_))),
            "{events:?}"
        );
        // The cast is paid over the turns the wizard's skill needs.
        for _ in 0..80 {
            if w.global_instance(me, spell).is_some() {
                break;
            }
            end_turns(&mut w, 1);
        }
    }
    for id in [
        57,  /* wind_mastery */
        158, /* crusade */
        157, /* tranquility */
    ] {
        let spell = w.content().rules.spells.id_numbered(id).ok_or("no spell")?;
        if w.global_instance(me, spell).is_none() {
            return Err(format!("{id} is not in force after the casts"));
        }
    }
    Ok(w)
}

/// The commands each scenario plays on its prepared save.
fn play(name: &str, w: &mut Game) {
    match name {
        "early" => end_turns(w, 15),
        "war" => end_turns(w, 15),
        _ => end_turns(w, 10),
    }
}

fn money(w: &World, id: PlayerId) -> String {
    w.player(id).map_or_else(String::new, |p| {
        let t = p.treasury();
        format!("gold {} mana {}", t.gold, t.mana)
    })
}

/// The readable summary of a game: the turn, the draws of the random generator, the totals of the
/// save and, for each player, what it holds (cities, citizens, buildings, units, their hit points
/// and squares, spells, gold and mana), then how many units there are of each kind. A change shows
/// as the line that moved, so the diff of a target says what a change did (the save text itself
/// is not compared: a new field in the format is not a change of a rule).
fn digest(w: &Game) -> Result<String, Box<dyn std::error::Error>> {
    let snap = w.snapshot();
    let mut out = format!(
        "turn {}\ndraws {}\ncities {}\nunits {}\nspells {}\nitems {}\n",
        w.round(),
        w.draws(),
        snap.cities.len(),
        snap.units.len(),
        snap.spells.len(),
        snap.items.len()
    );
    let buildings: Vec<_> = w
        .content()
        .rules
        .buildings
        .iter()
        .map(|(id, _)| id)
        .collect();
    for id in w.player_ids() {
        let cities = w.economy().cities_of(id);
        let citizens: u32 = cities
            .iter()
            .map(|&(c, _)| w.economy().city_economy(c).map_or(0, |e| e.figures))
            .sum();
        let built: usize = cities
            .iter()
            .map(|&(c, _)| {
                buildings
                    .iter()
                    .filter(|&&b| w.economy().city_has_building(c, b))
                    .count()
            })
            .sum();
        let units = w.units().player_units(id);
        let (mut hits, mut max, mut x, mut y) = (0, 0, 0, 0);
        for &u in &units {
            let (h, m) = w.units().unit_hits(u);
            hits += h;
            max += m;
            if let Some(p) = w.units().unit_position(u) {
                x += p.x;
                y += p.y;
            }
        }
        let known = w.wizard(id).map_or(0, |wizard| wizard.known().len());
        out.push_str(&format!(
            "player {} cities {} citizens {} buildings {} units {} hits {}/{} squares x {} y {} spells {} {}\n",
            id.0,
            cities.len(),
            citizens,
            built,
            units.len(),
            hits,
            max,
            x,
            y,
            known,
            money(w, id)
        ));
    }
    let mut kinds: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for id in w.player_ids() {
        for u in w.units().player_units(id) {
            if let Some(d) = w.units().unit_def(u) {
                *kinds.entry(d.id.clone()).or_default() += 1;
            }
        }
    }
    let kinds: Vec<String> = kinds.into_iter().map(|(k, n)| format!("{k} {n}")).collect();
    out.push_str(&format!("kinds {}\n", kinds.join(", ")));
    Ok(out)
}

fn refresh() -> Option<String> {
    std::env::var("F4X_REFRESH_REGRESSION").ok()
}

fn check(name: &str, build: fn() -> Result<Game, String>) -> TestResult {
    let saves = folder();
    let save = saves.join(format!("{name}.save"));
    let target = saves.join(format!("{name}.target"));
    let refresh = refresh();
    if refresh.as_deref() == Some("saves") {
        std::fs::create_dir_all(&saves)?;
        std::fs::write(&save, build()?.save_file(Vec::new()).to_text()?)?;
    }
    let text = std::fs::read_to_string(&save).map_err(|e| {
        format!(
            "{}: {e} (rebuild with F4X_REFRESH_REGRESSION=saves)",
            save.display()
        )
    })?;
    let mut w = SaveFile::from_text(&text)?
        .restore(shipped().clone(), ContentCheck::Strict)?
        .game;
    play(name, &mut w);
    let got = digest(&w)?;
    if refresh.is_some() {
        std::fs::write(&target, &got)?;
        return Ok(());
    }
    let want = std::fs::read_to_string(&target).map_err(|e| {
        format!(
            "{}: {e} (write it with F4X_REFRESH_REGRESSION=targets)",
            target.display()
        )
    })?;
    assert_eq!(
        got, want,
        "the game `{name}` ended differently from its target. If the change is on purpose: \
         F4X_REFRESH_REGRESSION=targets cargo test -p f4x-newgame --test it regression"
    );
    Ok(())
}

#[test]
fn an_early_game_plays_on_as_before() -> TestResult {
    check("early", early)
}

#[test]
fn a_game_at_war_plays_on_as_before() -> TestResult {
    check("war", war)
}

#[test]
fn a_game_with_global_enchantments_plays_on_as_before() -> TestResult {
    check("enchanted", enchanted)
}
