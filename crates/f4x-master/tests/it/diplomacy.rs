//! The game never starts or ends a war by itself, however many turns pass (`docs/diplomacy.md` 9.2).

use f4x_core::diplomacy::Treaty;
use f4x_core::player::PlayerId;
use f4x_master::Game;

use crate::support::games;

/// Every world with its wizards, that have all met.
fn met() -> Vec<(String, Game, Vec<PlayerId>)> {
    games()
        .into_iter()
        .map(|(name, mut w)| {
            let ids = w.wizards();
            for &a in &ids {
                for &b in &ids {
                    w.make_contact(a, b);
                }
            }
            (name, w, ids)
        })
        .collect()
}

#[test]
fn the_game_never_changes_a_treaty_by_itself() {
    // Taking a city costs the winner standing but starts no war (`docs/diplomacy.md` 9.2).
    for (name, mut w, ids) in met() {
        let (a, b) = (ids[0], ids[1]);
        let city = w
            .economy()
            .cities()
            .into_iter()
            .find(|c| c.1.owner == b && w.economy().citizens(c.0) > 0)
            .expect("a city with people")
            .0;
        w.conquer_city(city, a, 0);
        assert_eq!(
            w.treaty(a, b),
            Treaty::None,
            "{name}: a city taken is no war"
        );
        // Nor do the turns that follow start one: a pact is ended only by a player.
        w.start_treaty(a, b, Treaty::Pact);
        for _ in 0..6 {
            w.play_round();
        }
        assert_ne!(w.treaty(a, b), Treaty::War, "{name}: war came by itself");
    }
}

#[test]
fn an_alliance_stands_though_one_ally_is_at_war_with_the_others_ally() {
    for (name, mut w, ids) in met().into_iter().filter(|m| m.2.len() >= 3) {
        let (a, b, c) = (ids[0], ids[1], ids[2]);
        w.start_treaty(a, b, Treaty::Alliance);
        w.start_treaty(b, c, Treaty::Alliance);
        w.declare_war(a, c);
        for _ in 0..3 {
            w.play_round();
        }
        assert_eq!(
            w.treaty(a, b),
            Treaty::Alliance,
            "{name}: the game split it"
        );
        assert_eq!(
            w.treaty(b, c),
            Treaty::Alliance,
            "{name}: the game split it"
        );
    }
}
