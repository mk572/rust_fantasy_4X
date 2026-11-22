//! The override (cheat) commands: they need the override mode, each use is logged, the log is
//! saved, and a game that used the mode is `cheated` (no Hall of Fame).

use super::*;
use f4x_core::map::{Plane, Position};

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
    let serial = g.units().serial_of(unit).0;
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

/// The id of the first feature of a kind in the shipped content.
fn feature_of(g: &Game, kind: f4x_core::feature::def::FeatureKind) -> String {
    g.content()
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
    let (mut g, mut c) = (game(), console());
    let at = Position {
        plane: Plane::Arcanus,
        x: 2,
        y: 2,
    };
    assert!(!c.execute(&mut g, "unlair 2,2").ok, "not in the legal mode");
    c.execute(&mut g, "mode override");
    assert!(!c.execute(&mut g, "unlair 2,2").ok, "there is no lair yet");
    let site = feature_of(&g, FeatureKind::Encounter);
    let r = c.execute(&mut g, &format!("lair {site} 2,2 strong"));
    assert!(r.ok, "{}", r.text());
    let lair = g.economy().lair_at(at).expect("a lair stands there");
    assert!(!lair.guards.is_empty(), "the lair has guards");
    let r = c.execute(&mut g, "unlair 2,2");
    assert!(r.ok, "{}", r.text());
    assert!(g.economy().lair_at(at).is_none());
    assert!(!c.execute(&mut g, "lair nonsense 2,2").ok);
    let mineral = format!("lair {} 2,2", feature_of(&g, FeatureKind::Mineral));
    assert!(!c.execute(&mut g, &mineral).ok, "a mineral has no lair");
}

#[test]
fn a_feature_is_put_on_a_square_and_taken_off() {
    use f4x_core::feature::def::FeatureKind;
    let (mut g, mut c) = (game(), console());
    c.execute(&mut g, "mode override");
    let at = Position {
        plane: Plane::Arcanus,
        x: 2,
        y: 2,
    };
    let site = feature_of(&g, FeatureKind::Encounter);
    let r = c.execute(&mut g, &format!("feature 2,2 {site}"));
    assert!(r.ok, "{}", r.text());
    assert!(g.economy().feature_entity_at(at).is_some());
    let r = c.execute(&mut g, "feature 2,2 none");
    assert!(r.ok, "{}", r.text());
    assert!(g.economy().feature_entity_at(at).is_none());
}

#[test]
fn a_hero_is_hired_rerolled_and_dismissed_and_the_treasury_is_read() {
    use f4x_core::hero::HeroState;
    let (mut g, mut c) = (game(), console());
    assert!(
        !c.execute(&mut g, "hire healer").ok,
        "not in the legal mode"
    );
    c.execute(&mut g, "mode override");
    let kind = 7; // healer
    let hero = |g: &Game| g.units().hero_entity(kind).expect("the hero exists");
    assert_eq!(g.units().hero_state(hero(&g)), Some(HeroState::Free));
    let r = c.execute(&mut g, "hire healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.units().hero_state(hero(&g)), Some(HeroState::Serving));
    assert!(!c.execute(&mut g, "hire healer").ok, "already in service");
    let r = c.execute(&mut g, "reroll healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.units().hero_state(hero(&g)), Some(HeroState::Serving));
    assert!(
        !c.execute(&mut g, "reroll barbarian_spearmen").ok,
        "not a hero"
    );
    let r = c.execute(&mut g, "dismiss healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(g.units().hero_state(hero(&g)), Some(HeroState::Free));
    assert!(!c.execute(&mut g, "dismiss healer").ok, "nobody's now");
    let gold = g.player(HUMAN).expect("a player").treasury().gold;
    let r = c.execute(&mut g, "treasury");
    assert!(r.ok && r.text().contains(&gold.to_string()), "{}", r.text());
    assert!(c.execute(&mut g, "treasury 1").ok);
}

#[test]
fn a_hero_is_hired_on_any_square_given_and_else_on_the_fortress() {
    let (mut g, mut c) = (game(), console());
    c.execute(&mut g, "mode override");
    let fortress = g
        .wizard(HUMAN)
        .and_then(|w| w.fortress())
        .expect("a fortress");
    let kind = 7; // healer
    let at = |g: &Game| {
        let hero = g.units().hero_entity(kind).expect("the hero exists");
        g.units().unit_position(hero)
    };
    // Off the map: refused, and the hero stays free.
    assert!(!c.execute(&mut g, "hire healer 9999,9999").ok);
    assert_eq!(at(&g), None);
    // Any square of the map, with or without the player.
    let square = f4x_core::map::Position {
        x: 1,
        y: 1,
        plane: fortress.plane,
    };
    let r = c.execute(
        &mut g,
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
    assert_eq!(at(&g), Some(square));
    c.execute(&mut g, "dismiss healer");
    let r = c.execute(&mut g, "hire healer");
    assert!(r.ok, "{}", r.text());
    assert_eq!(at(&g), Some(fortress), "no square: the fortress");
}

fn spell_text(g: &Game, num: u16) -> (f4x_core::rules::SpellId, String) {
    let rules = &g.content().rules;
    let id = rules.spells.id_numbered(num).expect("the content has it");
    (id, rules.spells.get(id).id.clone())
}

#[test]
fn set_books_changes_the_books_and_the_casting_skill() {
    use f4x_core::rules::RealmId;
    let nature = RealmId::new(0 /* nature */);
    let (mut g, mut c) = (game(), console());
    assert!(
        !c.execute(&mut g, "set books nature 4").ok,
        "needs override"
    );
    assert!(c.execute(&mut g, "mode override").ok);
    let skill = |g: &Game| g.wizard(HUMAN).expect("a wizard").skill_points();
    let before = skill(&g);
    assert!(c.execute(&mut g, "set books nature 4").ok);
    assert_eq!(g.wizard(HUMAN).expect("a wizard").books_in(nature), 4);
    assert!(skill(&g) > before, "{} after {before}", skill(&g));
    // Another wizard, by number; none takes the books away.
    assert!(c.execute(&mut g, "set books nature 3 1").ok);
    assert_eq!(g.wizard(PlayerId(1)).expect("a wizard").books_in(nature), 3);
    assert_eq!(g.wizard(HUMAN).expect("a wizard").books_in(nature), 4);
    assert!(c.execute(&mut g, "set books nature 0").ok);
    assert_eq!(g.wizard(HUMAN).expect("a wizard").books_in(nature), 0);
    assert_eq!(skill(&g), before);
    // More than the game allows is cut to the most.
    assert!(c.execute(&mut g, "set books nature 200").ok);
    assert_eq!(
        g.wizard(HUMAN).expect("a wizard").books_in(nature),
        i32::from(g.settings().wizards.max_books_per_realm)
    );
    for bad in [
        "set books",
        "set books nowhere 3",
        "set books nature x",
        "set tomes nature 3",
    ] {
        assert!(!c.execute(&mut g, bad).ok, "{bad}");
    }
}

#[test]
fn a_retort_is_given_and_taken_away_and_the_skill_follows() {
    use f4x_core::wizard::magic::Retort;
    let channeler = Retort::new(8 /* channeler */);
    let (mut g, mut c) = (game(), console());
    assert!(
        !c.execute(&mut g, "give retort channeler").ok,
        "needs override"
    );
    assert!(c.execute(&mut g, "mode override").ok);
    assert!(!g.wizard(HUMAN).expect("a wizard").has(channeler));
    assert!(c.execute(&mut g, "give retort channeler").ok);
    assert!(g.wizard(HUMAN).expect("a wizard").has(channeler));
    // Giving it twice, or taking away one that is not there, says so.
    assert!(!c.execute(&mut g, "give retort channeler").ok);
    assert!(c.execute(&mut g, "remove retort channeler").ok);
    assert!(!g.wizard(HUMAN).expect("a wizard").has(channeler));
    assert!(!c.execute(&mut g, "remove retort channeler").ok);
    // A name with two words, and another wizard.
    assert!(c.execute(&mut g, "give retort chaos mastery 1").ok);
    assert!(g
        .wizard(PlayerId(1))
        .expect("a wizard")
        .has(Retort::new(2 /* chaos_mastery */)));
    for bad in [
        "give retort",
        "give retort nothing_like_it",
        "remove retort",
        "remove tome x",
    ] {
        assert!(!c.execute(&mut g, bad).ok, "{bad}");
    }
}

#[test]
fn a_spell_is_given_and_taken_away() {
    let (mut g, mut c) = (game(), console());
    let (spell, name) = spell_text(&g, 12 /* nature_s_eye */);
    assert!(
        !c.execute(&mut g, &format!("give spell {name}")).ok,
        "needs override"
    );
    assert!(c.execute(&mut g, "mode override").ok);
    let knows = |g: &Game, p| g.wizard(p).expect("a wizard").known().contains(&spell);
    assert!(!knows(&g, HUMAN));
    assert!(c.execute(&mut g, &format!("give spell {name}")).ok);
    assert!(knows(&g, HUMAN));
    assert!(c.execute(&mut g, &format!("remove spell {name}")).ok);
    assert!(!knows(&g, HUMAN));
    // Taking away one that is not known says so; another wizard, by number.
    assert!(!c.execute(&mut g, &format!("remove spell {name}")).ok);
    assert!(c.execute(&mut g, &format!("give spell {name} 1")).ok);
    assert!(knows(&g, PlayerId(1)) && !knows(&g, HUMAN));
    assert!(!c.execute(&mut g, "give spell no_such_spell").ok);
    // Each use is in the log.
    let log = g.override_log().join("\n");
    assert!(
        log.contains("give spell") && log.contains("remove spell"),
        "{log}"
    );
}
