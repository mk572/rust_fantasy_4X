//! Whether to keep or raze a city just taken (`docs/ai-design.md`). The decision is the AI's: the
//! core only offers the window (`Command::RazeCity`) to the taker until its turn ends. The AI weighs
//! the wizard's temper, how the two races get on, what the land is worth, the city's people and
//! buildings, how well it can be held and the soldiers there are to hold it; nothing is left to
//! chance. A city of the independents is never razed.
//!
//! Tests: below.

use f4x_def::rules::ai_strategy::RazeLevers;

use crate::lever::Levered;
use f4x_core::diplomacy::Treaty;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::wizard::objective::Personality;
use f4x_core::world::{Command, Event};
use f4x_def::rules::BuildingId;

use crate::view::{is_fighter, stack_strength};
use crate::Ai;
use f4x_master::Game;

/// What speaks for razing a city, by reason; a negative part speaks for keeping it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Case {
    temper: i32,
    race: i32,
    land: i32,
    size: i32,
    defence: i32,
    garrison: i32,
}

impl Case {
    fn total(self) -> i32 {
        self.temper + self.race + self.land + self.size + self.defence + self.garrison
    }
}

/// What the wizard's temper says, or `None` for one that keeps the city whatever else speaks:
/// a peaceful wizard always, a lawful one unless it fights the former owner.
fn temper_part(view: &PlayerView, from: PlayerId, k: &RazeLevers) -> Option<i32> {
    let temper = view
        .wizard()
        .map(|w| crate::objective::temper(w.personality(), view.difficulty()));
    match temper {
        Some(Personality::Peaceful) => None,
        Some(Personality::Lawful) => (view.treaty_with(from) == Treaty::War).then_some(0),
        Some(Personality::Maniacal) => Some(k.maniacal),
        Some(Personality::Ruthless) => Some(k.ruthless),
        Some(Personality::Chaotic) => Some(k.chaotic),
        Some(Personality::Aggressive) => Some(k.aggressive),
        // An independent race has no temper.
        None => Some(0),
    }
}

/// How the city's people get on with the wizard's (the game's racial unrest between them).
fn race_part(view: &PlayerView, city: hecs::Entity, k: &RazeLevers) -> i32 {
    let (Some(mine), Some(theirs)) = (view.my_race(), view.city_race(city)) else {
        return 0;
    };
    let steps = view
        .content()
        .rules
        .config
        .racial_unrest
        .get(usize::from(mine.num()))
        .and_then(|row| row.get(usize::from(theirs.num())))
        .map_or(0, |&s| i32::from(s));
    steps * k.race_step
}

/// Poor land speaks for razing, good land for keeping.
fn land_part(view: &PlayerView, at: Position, k: &RazeLevers) -> i32 {
    let worth = crate::city_site::land_worth(view, at).round() as i32;
    ((k.land_fair - worth) * k.land_pct / 100).clamp(-k.land_cap, k.land_cap)
}

/// The more people live in the city and the more is built there, the more speaks for keeping it.
fn size_part(view: &PlayerView, city: hecs::Entity, k: &RazeLevers) -> i32 {
    let citizens = view.city_economy(city).map_or(0, |e| e.figures);
    let built: u32 = view
        .city_buildings(city)
        .into_iter()
        .map(|b| view.content().rules.buildings.get(b).cost)
        .sum();
    let worth = i64::from(citizens) * i64::from(k.citizen)
        + i64::from(built) / i64::from(k.built_per_point);
    -(worth.min(i64::from(k.size_cap)) as i32)
}

/// How hard the city is to hold: its distance from the own cities, the enemy cities round it
/// against the own ones, and its walls.
fn defence_part(
    view: &PlayerView,
    city: hecs::Entity,
    at: Position,
    from: PlayerId,
    k: &RazeLevers,
) -> i32 {
    let near = |p: Position| {
        view.distance(at, p)
            .is_some_and(|d| d <= view.levers().shared.rival_range)
    };
    let mut enemies = view.rivals_at_war();
    enemies.push(from);
    let theirs = view
        .known_cities()
        .into_iter()
        .filter(|&(_, o, p)| enemies.contains(&o) && near(p))
        .count() as i32;
    let mine = view
        .my_cities()
        .into_iter()
        .filter(|&(c, p)| c != city && near(p))
        .count() as i32;
    let mut part = ((theirs - mine) * k.exposed).clamp(-k.exposed_cap, k.exposed_cap);
    if mine == 0 {
        part += k.alone;
    }
    let walls = BuildingId::new(view.content().rules.config.spell_effects.city_walls);
    if view.city_buildings(city).contains(&walls) {
        part -= k.walls;
    }
    part
}

/// How much of the defence the city wants is missing: the soldiers in it and near it count, those
/// that hold another own city do not.
fn garrison_part(view: &PlayerView, at: Position, k: &RazeLevers, round: u32) -> i32 {
    let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    let there: Vec<hecs::Entity> = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .filter(|&u| {
            view.unit_position(u).is_some_and(|p| {
                p == at
                    || (!cities.contains(&p)
                        && view
                            .distance(at, p)
                            .is_some_and(|d| d <= view.levers().raze.reach))
            })
        })
        .collect();
    let have = stack_strength(view, &there);
    let wanted = crate::garrison::wanted_strength(view, at, round);
    let missing = if have <= 0.0 {
        1.0
    } else if wanted <= have {
        0.0
    } else {
        1.0 - have / wanted
    };
    (missing * k.unheld as f32).round() as i32
}

/// The case for razing `city` at `at`, which `from` held until now; the city is the taker's.
/// `None` when the wizard's temper keeps it whatever else speaks.
fn case(
    view: &PlayerView,
    city: hecs::Entity,
    at: Position,
    from: PlayerId,
    k: &RazeLevers,
    round: u32,
) -> Option<Case> {
    Some(Case {
        temper: temper_part(view, from, k)?,
        race: race_part(view, city, k),
        land: land_part(view, at, k),
        size: size_part(view, city, k),
        defence: defence_part(view, city, at, from, k),
        garrison: garrison_part(view, at, k, round),
    })
}

/// Decide for every city of a wizard taken in the events so far this turn.
pub(crate) fn run(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let k = game.levers().raze;
    let taken: Vec<(hecs::Entity, PlayerId)> = events
        .iter()
        .filter_map(|e| match e {
            Event::CityCaptured { city, from, to } if *to == me => Some((*city, *from)),
            _ => None,
        })
        .collect();
    for (city, from) in taken {
        let unwanted = {
            let view = PlayerView::new(game, me);
            if !view.is_rival_wizard(from) {
                continue;
            }
            let Some(at) = view
                .my_cities()
                .into_iter()
                .find(|&(e, _)| e == city)
                .map(|(_, p)| p)
            else {
                continue;
            };
            case(&view, city, at, from, &k, game.round()).is_some_and(|c| c.total() > k.threshold)
        };
        if unwanted {
            ai.order(game, Command::RazeCity { player: me, city }, events);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_core::diplomacy::DiplomacyCommand;
    use f4x_core::map::terrain::TerrainClass;
    use f4x_core::map::Plane;
    use f4x_core::wizard::entity::CapitalRace;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A square on the east island: near the former owner's other city, far from the taker's.
    const FAR: (u32, u32) = (32, 6);
    /// A square on the west island: near the taker's own city.
    const HOME: (u32, u32) = (4, 6);

    /// A wizard of the high men, of this temper, has just taken a city of `race` at `square`.
    struct Taken {
        w: Game,
        city: hecs::Entity,
        at: Position,
        events: Vec<Event>,
    }

    impl Taken {
        fn new(
            temper: Personality,
            race: u16,
            square: (u32, u32),
        ) -> Result<Self, Box<dyn std::error::Error>> {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            let mine = w.content().rules.races.id_numbered(8 /* high_men */);
            let race = w.content().rules.races.id_numbered(race);
            let at = at(square.0, square.1);
            w.command(Command::FoundCity {
                owner: P1,
                pos: at,
                name: "Taken".into(),
                race,
                population: 8,
            });
            let city = w.economy().city_at(at).ok_or("the city is founded")?;
            // The taker has seen the land round the city, as an attacker that came to it has.
            for scout in spawn(&mut w, P0, at, 104 /* high_men_spearmen */, 1) {
                w.scenario_remove_unit(scout);
            }
            let mut wizard = w.wizard_mut(P0).ok_or("player 0 is a wizard")?;
            wizard.put(Some(temper));
            wizard.put(Some(CapitalRace(mine)));
            let events = w.conquer_city(city, P0, 0);
            Ok(Self {
                w,
                city,
                at,
                events,
            })
        }

        fn case(&self) -> Option<Case> {
            let view = PlayerView::new(&self.w, P0);
            case(
                &view,
                self.city,
                self.at,
                P1,
                &self.w.levers().raze,
                self.w.round(),
            )
        }

        fn parts(&self) -> Result<Case, &'static str> {
            self.case().ok_or("this temper weighs the city")
        }

        /// Let the AI decide; whether the city is gone.
        fn decide(&mut self) -> bool {
            let ai = Ai::new(P0);
            run(&ai, &mut self.w, &mut self.events);
            self.w.economy().city_at(self.at).is_none()
        }
    }

    /// The race whose cities the high men's people get on with worst.
    fn hated_race() -> Result<u16, &'static str> {
        let content = crate::testworld::shipped();
        let row = &content.rules.config.racial_unrest[8 /* high_men */];
        let (race, &steps) = row
            .iter()
            .enumerate()
            .max_by_key(|&(_, &s)| s)
            .ok_or("the unrest table has a row")?;
        assert!(steps > row[8 /* high_men */], "{row:?}");
        u16::try_from(race).map_err(|_| "a race number")
    }

    #[test]
    fn a_peaceful_wizard_keeps_every_city_and_a_lawful_one_unless_it_is_at_war() -> TestResult {
        let hated = hated_race()?;
        let peaceful = Taken::new(Personality::Peaceful, hated, FAR)?;
        assert_eq!(peaceful.case(), None);
        let mut lawful = Taken::new(Personality::Lawful, hated, FAR)?;
        assert_eq!(lawful.case(), None);
        lawful
            .w
            .command(Command::Diplomacy(DiplomacyCommand::DeclareWar {
                player: P0,
                on: P1,
            }));
        assert!(lawful.case().is_some());
        Ok(())
    }

    #[test]
    fn an_independent_race_judges_by_its_own_people() -> TestResult {
        let w = crate::testworld::fresh_game()?;
        let (player, race) = w
            .player_ids()
            .into_iter()
            .find_map(|p| Some((p, w.race_of_player(p)?)))
            .ok_or("a new game has independent cities")?;
        assert_eq!(PlayerView::new(&w, player).my_race(), Some(race));
        Ok(())
    }

    #[test]
    fn a_crueller_temper_speaks_more_for_razing() -> TestResult {
        let mut temper = Vec::new();
        for p in [
            Personality::Maniacal,
            Personality::Ruthless,
            Personality::Chaotic,
            Personality::Aggressive,
        ] {
            temper.push(Taken::new(p, 8 /* high_men */, FAR)?.parts()?.temper);
        }
        assert!(temper.windows(2).all(|w| w[0] > w[1]), "{temper:?}");
        Ok(())
    }

    #[test]
    fn a_city_of_a_hated_race_speaks_more_for_razing_than_one_of_the_own_race() -> TestResult {
        let own = Taken::new(Personality::Ruthless, 8 /* high_men */, FAR)?;
        let hated = Taken::new(Personality::Ruthless, hated_race()?, FAR)?;
        assert!(hated.parts()?.race > own.parts()?.race);
        Ok(())
    }

    #[test]
    fn poor_land_speaks_more_for_razing_than_good_land() -> TestResult {
        let mut t = Taken::new(Personality::Ruthless, 8 /* high_men */, FAR)?;
        let view = PlayerView::new(&t.w, P0);
        assert!(view.terrain_class(t.at).is_some(), "the land is explored");
        let good = t.parts()?.land;
        let desert =
            t.w.content()
                .terrains
                .first_of_class(TerrainClass::Desert, Plane::Arcanus)
                .ok_or("a desert is shipped")?;
        for dy in 0..5 {
            for dx in 0..5 {
                let square = at(t.at.x + dx - 2, t.at.y + dy - 2);
                if square != t.at {
                    t.w.scenario_set_terrain(square, desert);
                }
            }
        }
        assert!(t.parts()?.land > good, "{good} then {:?}", t.parts()?);
        Ok(())
    }

    #[test]
    fn people_and_buildings_speak_for_keeping_a_city() -> TestResult {
        let mut t = Taken::new(Personality::Ruthless, 8 /* high_men */, FAR)?;
        t.w.scenario_set_citizens(t.city, 2);
        let small = t.parts()?.size;
        let most = t.w.economy().max_city_size();
        t.w.scenario_set_citizens(t.city, most);
        let large = t.parts()?.size;
        assert!(large < small, "{small} then {large}");
        // On a small city, so that the size has not reached the most it can say.
        t.w.scenario_set_citizens(t.city, 2);
        let parthenon = BuildingId::new(24 /* parthenon */);
        t.w.scenario_set_building(t.city, parthenon, true);
        assert!(t.parts()?.size < small);
        Ok(())
    }

    #[test]
    fn a_city_far_from_home_among_enemies_is_harder_to_hold_and_walls_help() -> TestResult {
        let mut far = Taken::new(Personality::Ruthless, 8 /* high_men */, FAR)?;
        let home = Taken::new(Personality::Ruthless, 8 /* high_men */, HOME)?;
        let open = far.parts()?.defence;
        assert!(open > home.parts()?.defence);
        let walls = BuildingId::new(far.w.content().rules.config.spell_effects.city_walls);
        far.w.scenario_set_building(far.city, walls, true);
        assert!(far.parts()?.defence < open);
        Ok(())
    }

    #[test]
    fn soldiers_in_or_near_the_city_speak_for_keeping_it() -> TestResult {
        let empty = Taken::new(Personality::Ruthless, 8 /* high_men */, FAR)?;
        let unheld = empty.parts()?.garrison;
        assert!(unheld > 0);
        let reach = empty.w.levers().raze.reach;
        for (offset, counts) in [(0, true), (reach, true), (reach + 1, false)] {
            let mut t = Taken::new(Personality::Ruthless, 8 /* high_men */, FAR)?;
            let square = at(t.at.x - offset, t.at.y);
            let many = t.w.max_battle_side();
            spawn(&mut t.w, P0, square, 104 /* high_men_spearmen */, many);
            let part = t.parts()?.garrison;
            assert_eq!(part < unheld, counts, "{offset} squares away: {part}");
        }
        Ok(())
    }

    #[test]
    fn a_city_there_is_every_reason_against_is_razed_without_a_roll() -> TestResult {
        let mut t = Taken::new(Personality::Maniacal, hated_race()?, FAR)?;
        let draws = t.w.draws();
        assert!(t.decide(), "kept: {:?}", t.case());
        assert_eq!(t.w.draws(), draws);
        Ok(())
    }

    #[test]
    fn a_held_city_of_the_own_race_near_home_is_kept() -> TestResult {
        let mut t = Taken::new(Personality::Maniacal, 8 /* high_men */, HOME)?;
        let many = t.w.max_battle_side();
        spawn(&mut t.w, P0, t.at, 104 /* high_men_spearmen */, many);
        assert!(!t.decide(), "razed: {:?}", t.case());
        Ok(())
    }
}
