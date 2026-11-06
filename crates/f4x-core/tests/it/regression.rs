//! Regression tests on the whole game, without the client (`docs/rust_coding_rules.md`).
//!
//! Each scenario loads a prepared save from `tests/regression/<name>.save`, applies a fixed list
//! of commands and compares the resulting game with `tests/regression/<name>.target`: a short
//! summary of the state, then the checksum line of the full save text. A rule that changes by
//! accident changes the summary or the checksum and the test fails, with both texts in the message.
//!
//! **Changing the target on purpose** (a rule changed and the new result is right):
//! `F4X_REFRESH_REGRESSION=targets cargo test -p f4x-core --test it regression`
//! rewrites the `.target` files; read the diff in git before committing it.
//! **Rebuilding the saves** (the save format version changed, or the scenario's set-up changed):
//! `F4X_REFRESH_REGRESSION=saves cargo test -p f4x-core --test it regression` rewrites the
//! `.save` files from the set-ups below, then the targets.

use std::path::{Path, PathBuf};

use f4x_core::game::save::Snapshot;
use f4x_core::game::setup::{new_game, NewGame};
use f4x_core::game::{Command, Game};
use f4x_core::player::PlayerId;
use f4x_def::content::Content;
use f4x_def::testdir::TestResult;

fn shipped() -> &'static Content {
    f4x_def::testdir::shipped()
}

fn folder() -> PathBuf {
    Path::new("../../tests/regression").to_path_buf()
}

fn end_turns(g: &mut Game, n: usize) {
    for _ in 0..n {
        g.apply(Command::EndTurn);
    }
}

fn fresh(seed: u64) -> Result<Game, String> {
    new_game(
        shipped(),
        &NewGame {
            seed,
            all_ai: true,
            ..Default::default()
        },
    )
}

/// A game a few turns old: the first ten turns of the AI's opening.
fn early() -> Result<Game, String> {
    let mut g = fresh(11)?;
    end_turns(&mut g, 2);
    Ok(g)
}

/// A game in the middle, with the first two wizards at war.
fn war() -> Result<Game, String> {
    let mut g = fresh(21)?;
    end_turns(&mut g, 40);
    let ids = g.player_ids();
    let (a, b) = (ids[0], ids[1]);
    g.declare_war(a, b);
    Ok(g)
}

/// A game in which the first wizard keeps three global enchantments going.
fn enchanted() -> Result<Game, String> {
    let mut g = fresh(31)?;
    let me = g.player_ids()[0];
    g.treasury_mut(me).ok_or("no treasury")?.mana = 30_000;
    for id in [
        57,  /* wind_mastery */
        158, /* crusade */
        157, /* tranquility */
    ] {
        let spell = g
            .content()
            .rules
            .spells
            .id_numbered(id)
            .ok_or_else(|| format!("no spell {id}"))?;
        g.cheat_learn_spell(me, spell);
        let events = g.apply(Command::CastSpell {
            player: me,
            spell,
            target: None,
            extra: 0,
        });
        assert!(
            events.is_empty()
                || !events
                    .iter()
                    .any(|e| matches!(e, f4x_core::game::Event::Rejected(_))),
            "{events:?}"
        );
        // The cast is paid over the turns the wizard's skill needs.
        for _ in 0..80 {
            if g.global_instance(me, spell).is_some() {
                break;
            }
            end_turns(&mut g, 1);
        }
    }
    for id in [
        57,  /* wind_mastery */
        158, /* crusade */
        157, /* tranquility */
    ] {
        let spell = g.content().rules.spells.id_numbered(id).ok_or("no spell")?;
        if g.global_instance(me, spell).is_none() {
            return Err(format!("{id} is not in force after the casts"));
        }
    }
    Ok(g)
}

/// The commands each scenario plays on its prepared save.
fn play(name: &str, g: &mut Game) {
    match name {
        "early" => end_turns(g, 15),
        "war" => end_turns(g, 15),
        _ => end_turns(g, 10),
    }
}

fn money(g: &Game, id: PlayerId) -> String {
    g.player(id).map_or_else(String::new, |p| {
        let t = p.treasury();
        format!("gold {} mana {}", t.gold, t.mana)
    })
}

/// The summary and the checksum line of the whole save (without the game's version).
fn digest(g: &Game) -> Result<String, Box<dyn std::error::Error>> {
    let mut snap = g.snapshot();
    // The version of the game that wrote the save is not a result: a release must not fail every
    // target.
    snap.game_version.clear();
    let text = snap.to_text()?;
    let mut out = format!(
        "turn {}\ndraws {}\ncities {}\nunits {}\nspells {}\nitems {}\n",
        snap.turn,
        snap.draws,
        snap.cities.len(),
        snap.units.len(),
        snap.spells.len(),
        snap.items.len()
    );
    for id in g.player_ids() {
        out.push_str(&format!(
            "player {} cities {} {}\n",
            id.0,
            g.city_count(id),
            money(g, id)
        ));
    }
    let first = text.lines().next().unwrap_or("");
    out.push_str(&format!(
        "checksum {}\n",
        first.rsplit_once("checksum ").map_or("", |(_, sum)| sum)
    ));
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
        std::fs::write(&save, build()?.snapshot().to_text()?)?;
    }
    let text = std::fs::read_to_string(&save).map_err(|e| {
        format!(
            "{}: {e} (rebuild with F4X_REFRESH_REGRESSION=saves)",
            save.display()
        )
    })?;
    let mut g = Snapshot::from_text(&text)?.restore(shipped().clone())?;
    play(name, &mut g);
    let got = digest(&g)?;
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
         F4X_REFRESH_REGRESSION=targets cargo test -p f4x-core --test it regression"
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
