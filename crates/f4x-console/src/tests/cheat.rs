//! The override (cheat) commands: they need the override mode, each use is logged, the log is
//! saved, and a game that used the mode is `cheated` (no Hall of Fame).

use super::*;
use f4x_core::map::{Plane, Position};

fn first_unit_of(w: &World, owner: PlayerId) -> hecs::Entity {
    w.units()
        .all_units()
        .into_iter()
        .find(|&u| w.units().unit_owner(u) == Some(owner))
        .expect("the player has a unit")
}

#[test]
fn override_commands_need_the_override_mode() {
    let (mut w, mut c) = (game(), console());
    let refused = c.execute(&mut w, "give gold 500");
    assert!(!refused.ok);
    assert!(
        matches!(err(&refused), ConsoleError::NotAllowed(_)),
        "{refused:?}"
    );
    let gold = w.player(HUMAN).expect("a player").treasury().gold;

    assert!(c.execute(&mut w, "mode override").ok);
    assert_eq!(c.session.mode, Mode::Override);
    assert!(c.execute(&mut w, "give gold 500").ok);
    assert_eq!(
        w.player(HUMAN).expect("a player").treasury().gold,
        gold + 500
    );

    assert!(c.execute(&mut w, "mode legal").ok);
    assert!(!c.execute(&mut w, "give gold 500").ok);
    assert_eq!(
        w.player(HUMAN).expect("a player").treasury().gold,
        gold + 500
    );

    // Switching the mode on and each cheat are in the game's saved log.
    let log = w.override_log().to_vec();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[0].contains("mode override") && log[1].contains("give gold 500"));
    let restored = w
        .snapshot()
        .restore(w.content().clone())
        .expect("the snapshot restores");
    assert_eq!(restored.override_log(), log.as_slice());
    assert!(c.execute(&mut w, "history").text().contains("give gold"));
}

#[test]
fn a_game_that_entered_the_cheat_mode_is_marked_for_good_and_stays_so_in_a_save() {
    let (mut w, mut c) = (game(), console());
    assert!(!w.cheated());
    // Looking is free: a refused cheat and the queries do not mark the game.
    assert!(!c.execute(&mut w, "give gold 1").ok);
    assert!(c.execute(&mut w, "status").ok);
    assert!(!w.cheated());
    // Entering the mode is enough, before any cheat is used.
    let on = c.execute(&mut w, "mode override");
    assert!(on.text().contains("Hall of Fame"), "{}", on.text());
    assert!(w.cheated());
    assert!(c.execute(&mut w, "mode legal").ok);
    assert!(w.cheated(), "going back does not clear it");
    let restored = w
        .snapshot()
        .restore(w.content().clone())
        .expect("the snapshot restores");
    assert!(restored.cheated());
    let text = w.snapshot().to_text().expect("the snapshot is written");
    let reread = f4x_core::world::save::Snapshot::from_text(&text).expect("it reads back");
    assert!(reread
        .restore(w.content().clone())
        .expect("it restores")
        .cheated());
}

#[test]
fn a_player_console_cannot_enter_the_override_mode() {
    let (mut w, _) = (game(), ());
    let mut player = console_for(Access::Play);
    assert!(!player.execute(&mut w, "mode override").ok);
    assert!(!player.execute(&mut w, "give gold 1").ok);
    assert!(!w.cheated());
    assert!(!player.complete("sp").contains(&"spawn"));
    let mut full = console();
    full.execute(&mut w, "mode override");
    assert!(full.complete("sp").contains(&"spawn"));
}

#[test]
fn eliminating_the_rival_wins_by_conquest() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let r = c.execute(&mut w, "eliminate 1");
    assert!(r.ok, "{}", r.text());
    assert!(r.text().contains("GAME OVER"), "{}", r.text());
    assert_eq!(w.game_over().expect("the game is over").winner, HUMAN);
}

#[test]
fn undead_enchant_restore_and_kill_change_units() {
    let (mut w, mut c) = (game(), console());
    let unit = first_unit_of(&w, HUMAN);
    let serial = w.units().serial_of(unit).0;
    assert!(!c.execute(&mut w, &format!("undead {serial}")).ok);
    assert!(!w.units().unit_is_undead(unit));
    assert!(c.execute(&mut w, "mode override").ok);
    let r = c.execute(&mut w, &format!("undead {serial}"));
    assert!(r.ok, "{}", r.text());
    assert!(w.units().unit_is_undead(unit));
    let r = c.execute(&mut w, &format!("enchant {serial} haste"));
    assert!(r.ok, "{}", r.text());
    assert!(
        w.units()
            .unit_enchantments(unit)
            .iter()
            .any(|(s, _)| { w.content().rules.spells.get(*s).id == "haste" }),
        "the unit has Haste"
    );
    assert!(c.execute(&mut w, &format!("restore {serial}")).ok);
    assert!(c.execute(&mut w, &format!("kill {serial}")).ok);
    assert!(!w.units().all_units().contains(&unit));
}

#[test]
fn citizens_spawn_and_reveal_work_in_the_override_mode() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let n = first_city(&w);
    let r = c.execute(&mut w, &format!("citizens {n} 9"));
    assert!(r.ok, "{}", r.text());
    assert!(c.execute(&mut w, &format!("city {n}")).ok);
    let before = w.units().all_units().len();
    let r = c.execute(&mut w, "spawn barbarian_spearmen 3,3 1");
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.units().all_units().len(), before + 1);
    let before = w.explored_squares(HUMAN);
    assert!(c.execute(&mut w, "reveal").ok);
    let after = w.explored_squares(HUMAN);
    assert!(before < after);
    // Both planes, every square: the map is the same size on each.
    assert_eq!(after % 2, 0, "{after}");
    let rival = w.explored_squares(PlayerId(1));
    assert!(rival < after, "only the asked wizard sees it all");
    assert!(c.execute(&mut w, &format!("build {n} barracks")).ok);
}

#[test]
fn contact_meets_the_other_wizards_in_the_override_mode_only() {
    let (mut w, mut c) = (game(), console());
    assert!(
        !c.execute(&mut w, "contact").ok,
        "a cheat needs the override mode"
    );
    c.execute(&mut w, "mode override");
    let rivals: Vec<PlayerId> = w.wizards().into_iter().filter(|&id| id != HUMAN).collect();
    assert!(!rivals.is_empty());
    let r = c.execute(&mut w, &format!("contact {}", rivals[0].0));
    assert!(r.ok, "{}", r.text());
    assert!(w.have_met(HUMAN, rivals[0]));
    let r = c.execute(&mut w, "contact");
    assert!(r.ok, "{}", r.text());
    for id in rivals {
        assert!(w.have_met(HUMAN, id));
    }
    assert!(!c.execute(&mut w, "contact 99").ok);
}

/// The id of the first feature of a kind in the shipped content.
fn feature_of(w: &World, kind: f4x_core::feature::def::FeatureKind) -> String {
    w.content()
        .features
        .defs()
        .iter()
        .find(|d| d.kind == kind)
        .map(|d| d.id.clone())
        .expect("the content has a feature of the kind")
}

#[test]
fn a_lair_is_put_on_a_square_with_guards_and_taken_off_again() {
    use f4x_core::feature::def::FeatureKind;
    let (mut w, mut c) = (game(), console());
    let at = Position {
        plane: Plane::Arcanus,
        x: 2,
        y: 2,
    };
    assert!(!c.execute(&mut w, "unlair 2,2").ok, "not in the legal mode");
    c.execute(&mut w, "mode override");
    assert!(!c.execute(&mut w, "unlair 2,2").ok, "there is no lair yet");
    let site = feature_of(&w, FeatureKind::Encounter);
    let r = c.execute(&mut w, &format!("lair {site} 2,2 strong"));
    assert!(r.ok, "{}", r.text());
    let lair = w.economy().lair_at(at).expect("a lair stands there");
    assert!(!lair.guards.is_empty(), "the lair has guards");
    let r = c.execute(&mut w, "unlair 2,2");
    assert!(r.ok, "{}", r.text());
    assert!(w.economy().lair_at(at).is_none());
    assert!(!c.execute(&mut w, "lair nonsense 2,2").ok);
    let mineral = format!("lair {} 2,2", feature_of(&w, FeatureKind::Mineral));
    assert!(!c.execute(&mut w, &mineral).ok, "a mineral has no lair");
}

#[test]
fn a_feature_is_put_on_a_square_and_taken_off() {
    use f4x_core::feature::def::FeatureKind;
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let at = Position {
        plane: Plane::Arcanus,
        x: 2,
        y: 2,
    };
    let site = feature_of(&w, FeatureKind::Encounter);
    let r = c.execute(&mut w, &format!("feature 2,2 {site}"));
    assert!(r.ok, "{}", r.text());
    assert!(w.economy().feature_entity_at(at).is_some());
    let r = c.execute(&mut w, "feature 2,2 none");
    assert!(r.ok, "{}", r.text());
    assert!(w.economy().feature_entity_at(at).is_none());
}

#[test]
fn a_hero_is_hired_rerolled_and_dismissed_and_the_treasury_is_read() {
    use f4x_core::hero::HeroState;
    let (mut w, mut c) = (game(), console());
    assert!(
        !c.execute(&mut w, "hire healer").ok,
        "not in the legal mode"
    );
    c.execute(&mut w, "mode override");
    let kind = 7; // healer
    let hero = |w: &World| w.units().hero_entity(kind).expect("the hero exists");
    assert_eq!(w.units().hero_state(hero(&w)), Some(HeroState::Free));
    let r = c.execute(&mut w, "hire healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.units().hero_state(hero(&w)), Some(HeroState::Serving));
    assert!(!c.execute(&mut w, "hire healer").ok, "already in service");
    let r = c.execute(&mut w, "reroll healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.units().hero_state(hero(&w)), Some(HeroState::Serving));
    assert!(
        !c.execute(&mut w, "reroll barbarian_spearmen").ok,
        "not a hero"
    );
    let r = c.execute(&mut w, "dismiss healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(w.units().hero_state(hero(&w)), Some(HeroState::Free));
    assert!(!c.execute(&mut w, "dismiss healer").ok, "nobody's now");
    let gold = w.player(HUMAN).expect("a player").treasury().gold;
    let r = c.execute(&mut w, "treasury");
    assert!(r.ok && r.text().contains(&gold.to_string()), "{}", r.text());
    assert!(c.execute(&mut w, "treasury 1").ok);
}

#[test]
fn a_hero_is_hired_on_any_square_given_and_else_on_the_fortress() {
    let (mut w, mut c) = (game(), console());
    c.execute(&mut w, "mode override");
    let fortress = w
        .wizard(HUMAN)
        .and_then(|wizard| wizard.fortress())
        .expect("a fortress");
    let kind = 7; // healer
    let at = |w: &World| {
        let hero = w.units().hero_entity(kind).expect("the hero exists");
        w.units().unit_position(hero)
    };
    // Off the map: refused, and the hero stays free.
    assert!(!c.execute(&mut w, "hire healer 9999,9999").ok);
    assert_eq!(at(&w), None);
    // Any square of the map, with or without the player.
    let square = f4x_core::map::Position {
        x: 1,
        y: 1,
        plane: fortress.plane,
    };
    let r = c.execute(
        &mut w,
        &format!(
            "hire healer {} 1,1,{}",
            HUMAN.0,
            if fortress.plane == f4x_core::map::Plane::Arcanus {
                "a"
            } else {
                "m"
            }
        ),
    );
    assert!(r.ok, "{}", r.text());
    assert_eq!(at(&w), Some(square));
    c.execute(&mut w, "dismiss healer");
    let r = c.execute(&mut w, "hire healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(at(&w), Some(fortress), "no square: the fortress");
}

fn spell_text(w: &World, num: u16) -> (f4x_core::rules::SpellId, String) {
    let rules = &w.content().rules;
    let id = rules.spells.id_numbered(num).expect("the content has it");
    (id, rules.spells.get(id).id.clone())
}

#[test]
fn set_books_changes_the_books_and_the_casting_skill() {
    use f4x_core::rules::RealmId;
    let nature = RealmId::new(0 /* nature */);
    let (mut w, mut c) = (game(), console());
    assert!(
        !c.execute(&mut w, "set books nature 4").ok,
        "needs override"
    );
    assert!(c.execute(&mut w, "mode override").ok);
    let skill = |w: &World| w.wizard(HUMAN).expect("a wizard").skill_points();
    let before = skill(&w);
    assert!(c.execute(&mut w, "set books nature 4").ok);
    assert_eq!(w.wizard(HUMAN).expect("a wizard").books_in(nature), 4);
    assert!(skill(&w) > before, "{} after {before}", skill(&w));
    // Another wizard, by number; none takes the books away.
    assert!(c.execute(&mut w, "set books nature 3 1").ok);
    assert_eq!(w.wizard(PlayerId(1)).expect("a wizard").books_in(nature), 3);
    assert_eq!(w.wizard(HUMAN).expect("a wizard").books_in(nature), 4);
    assert!(c.execute(&mut w, "set books nature 0").ok);
    assert_eq!(w.wizard(HUMAN).expect("a wizard").books_in(nature), 0);
    assert_eq!(skill(&w), before);
    // More than the game allows is cut to the most.
    assert!(c.execute(&mut w, "set books nature 200").ok);
    assert_eq!(
        w.wizard(HUMAN).expect("a wizard").books_in(nature),
        i32::from(w.settings().wizards.max_books_per_realm)
    );
    for bad in [
        "set books",
        "set books nowhere 3",
        "set books nature x",
        "set tomes nature 3",
    ] {
        assert!(!c.execute(&mut w, bad).ok, "{bad}");
    }
}

#[test]
fn a_retort_is_given_and_taken_away_and_the_skill_follows() {
    use f4x_core::wizard::magic::Retort;
    let channeler = Retort::new(8 /* channeler */);
    let (mut w, mut c) = (game(), console());
    assert!(
        !c.execute(&mut w, "give retort channeler").ok,
        "needs override"
    );
    assert!(c.execute(&mut w, "mode override").ok);
    assert!(!w.wizard(HUMAN).expect("a wizard").has(channeler));
    assert!(c.execute(&mut w, "give retort channeler").ok);
    assert!(w.wizard(HUMAN).expect("a wizard").has(channeler));
    // Giving it twice, or taking away one that is not there, says so.
    assert!(!c.execute(&mut w, "give retort channeler").ok);
    assert!(c.execute(&mut w, "remove retort channeler").ok);
    assert!(!w.wizard(HUMAN).expect("a wizard").has(channeler));
    assert!(!c.execute(&mut w, "remove retort channeler").ok);
    // A name with two words, and another wizard.
    assert!(c.execute(&mut w, "give retort chaos mastery 1").ok);
    assert!(w
        .wizard(PlayerId(1))
        .expect("a wizard")
        .has(Retort::new(2 /* chaos_mastery */)));
    for bad in [
        "give retort",
        "give retort nothing_like_it",
        "remove retort",
        "remove tome x",
    ] {
        assert!(!c.execute(&mut w, bad).ok, "{bad}");
    }
}

#[test]
fn a_spell_is_given_and_taken_away() {
    let (mut w, mut c) = (game(), console());
    let (spell, name) = spell_text(&w, 12 /* nature_s_eye */);
    assert!(
        !c.execute(&mut w, &format!("give spell {name}")).ok,
        "needs override"
    );
    assert!(c.execute(&mut w, "mode override").ok);
    let knows = |w: &World, p| w.wizard(p).expect("a wizard").known().contains(&spell);
    assert!(!knows(&w, HUMAN));
    assert!(c.execute(&mut w, &format!("give spell {name}")).ok);
    assert!(knows(&w, HUMAN));
    assert!(c.execute(&mut w, &format!("remove spell {name}")).ok);
    assert!(!knows(&w, HUMAN));
    // Taking away one that is not known says so; another wizard, by number.
    assert!(!c.execute(&mut w, &format!("remove spell {name}")).ok);
    assert!(c.execute(&mut w, &format!("give spell {name} 1")).ok);
    assert!(knows(&w, PlayerId(1)) && !knows(&w, HUMAN));
    assert!(!c.execute(&mut w, "give spell no_such_spell").ok);
    // Each use is in the log.
    let log = w.override_log().join("\n");
    assert!(
        log.contains("give spell") && log.contains("remove spell"),
        "{log}"
    );
}
