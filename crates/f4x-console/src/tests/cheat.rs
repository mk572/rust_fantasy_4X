//! The override (cheat) commands: they need the override mode, each use is logged, the log is
//! saved, and a game that used the mode is `cheated` (no Hall of Fame).

use super::*;

fn first_unit_of(g: &Game, owner: PlayerId) -> hecs::Entity {
    g.units()
        .all_units()
        .into_iter()
        .find(|&u| g.units().unit_owner(u) == Some(owner))
        .expect("the player has a unit")
}

#[test]
fn override_commands_need_the_override_mode() {
    let (mut g, mut c) = (game(), console());
    let refused = c.execute(&mut g, "give gold 500");
    assert!(!refused.ok);
    assert!(
        matches!(err(&refused), ConsoleError::NotAllowed(_)),
        "{refused:?}"
    );
    let gold = g.player(HUMAN).expect("a player").treasury().gold;

    assert!(c.execute(&mut g, "mode override").ok);
    assert_eq!(c.session.mode, Mode::Override);
    assert!(c.execute(&mut g, "give gold 500").ok);
    assert_eq!(
        g.player(HUMAN).expect("a player").treasury().gold,
        gold + 500
    );

    assert!(c.execute(&mut g, "mode legal").ok);
    assert!(!c.execute(&mut g, "give gold 500").ok);
    assert_eq!(
        g.player(HUMAN).expect("a player").treasury().gold,
        gold + 500
    );

    // Switching the mode on and each cheat are in the game's saved log.
    let log = g.override_log().to_vec();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[0].contains("mode override") && log[1].contains("give gold 500"));
    let restored = g
        .snapshot()
        .restore(g.content().clone())
        .expect("the snapshot restores");
    assert_eq!(restored.override_log(), log.as_slice());
    assert!(c.execute(&mut g, "history").text().contains("give gold"));
}

#[test]
fn a_game_that_entered_the_cheat_mode_is_marked_for_good_and_stays_so_in_a_save() {
    let (mut g, mut c) = (game(), console());
    assert!(!g.cheated());
    // Looking is free: a refused cheat and the queries do not mark the game.
    assert!(!c.execute(&mut g, "give gold 1").ok);
    assert!(c.execute(&mut g, "status").ok);
    assert!(!g.cheated());
    // Entering the mode is enough, before any cheat is used.
    let on = c.execute(&mut g, "mode override");
    assert!(on.text().contains("Hall of Fame"), "{}", on.text());
    assert!(g.cheated());
    assert!(c.execute(&mut g, "mode legal").ok);
    assert!(g.cheated(), "going back does not clear it");
    let restored = g
        .snapshot()
        .restore(g.content().clone())
        .expect("the snapshot restores");
    assert!(restored.cheated());
    let text = g.snapshot().to_text().expect("the snapshot is written");
    let reread = f4x_core::game::save::Snapshot::from_text(&text).expect("it reads back");
    assert!(reread
        .restore(g.content().clone())
        .expect("it restores")
        .cheated());
}

#[test]
fn a_player_console_cannot_enter_the_override_mode() {
    let (mut g, _) = (game(), ());
    let mut player = console_for(Access::Play);
    assert!(!player.execute(&mut g, "mode override").ok);
    assert!(!player.execute(&mut g, "give gold 1").ok);
    assert!(!g.cheated());
    assert!(!player.complete("sp").contains(&"spawn"));
    let mut full = console();
    full.execute(&mut g, "mode override");
    assert!(full.complete("sp").contains(&"spawn"));
}

#[test]
fn eliminating_the_rival_wins_by_conquest() {
    let (mut g, mut c) = (game(), console());
    c.execute(&mut g, "mode override");
    let r = c.execute(&mut g, "eliminate 1");
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("GAME OVER"), "{}", r.text());
    assert_eq!(g.game_over.expect("the game is over").winner, HUMAN);
}

#[test]
fn undead_enchant_restore_and_kill_change_units() {
    let (mut g, mut c) = (game(), console());
    let unit = first_unit_of(&g, HUMAN);
    let serial = g.serial_of(unit).0;
    assert!(!c.execute(&mut g, &format!("undead {serial}")).ok);
    assert!(!g.units().unit_is_undead(unit));
    assert!(c.execute(&mut g, "mode override").ok);
    let r = c.execute(&mut g, &format!("undead {serial}"));
    assert!(r.ok, "{}", r.text());
    assert!(g.units().unit_is_undead(unit));
    let r = c.execute(&mut g, &format!("enchant {serial} haste"));
    assert!(r.ok, "{}", r.text());
    assert!(
        g.units()
            .unit_enchantments(unit)
            .iter()
            .any(|(s, _)| { g.content().rules.spells.get(*s).id == "haste" }),
        "the unit has Haste"
    );
    assert!(c.execute(&mut g, &format!("restore {serial}")).ok);
    assert!(c.execute(&mut g, &format!("kill {serial}")).ok);
    assert!(!g.units().all_units().contains(&unit));
}

#[test]
fn citizens_spawn_and_reveal_work_in_the_override_mode() {
    let (mut g, mut c) = (game(), console());
    c.execute(&mut g, "mode override");
    let n = first_city(&g);
    let r = c.execute(&mut g, &format!("citizens {n} 9"));
    assert!(r.ok, "{}", r.text());
    assert!(c.execute(&mut g, &format!("city {n}")).ok);
    let before = g.units().all_units().len();
    let r = c.execute(&mut g, "spawn barbarian_spearmen 3,3 1");
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.units().all_units().len(), before + 1);
    let before = g.explored_squares(HUMAN);
    assert!(c.execute(&mut g, "reveal").ok);
    let after = g.explored_squares(HUMAN);
    assert!(before < after);
    // Both planes, every square: the map is the same size on each.
    assert_eq!(after % 2, 0, "{after}");
    let rival = g.explored_squares(PlayerId(1));
    assert!(rival < after, "only the asked wizard sees it all");
    assert!(c.execute(&mut g, &format!("build {n} barracks")).ok);
}

#[test]
fn contact_meets_the_other_wizards_in_the_override_mode_only() {
    let (mut g, mut c) = (game(), console());
    assert!(
        !c.execute(&mut g, "contact").ok,
        "a cheat needs the override mode"
    );
    c.execute(&mut g, "mode override");
    let rivals: Vec<PlayerId> = g.wizards().into_iter().filter(|&w| w != HUMAN).collect();
    assert!(!rivals.is_empty());
    let r = c.execute(&mut g, &format!("contact {}", rivals[0].0));
    assert!(r.ok, "{}", r.text());
    assert!(g.have_met(HUMAN, rivals[0]));
    let r = c.execute(&mut g, "contact");
    assert!(r.ok, "{}", r.text());
    for w in rivals {
        assert!(g.have_met(HUMAN, w));
    }
    assert!(!c.execute(&mut g, "contact 99").ok);
}
