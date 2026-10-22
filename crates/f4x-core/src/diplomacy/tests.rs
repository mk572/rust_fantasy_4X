use super::*;
use crate::game::save::Snapshot;
use crate::game::testutil::{content_with_magic, FakeView};
use crate::map::{Grid, Map, Plane, Position};

use crate::game::{Command, Rejection};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// Three wizards, the first of them the human, nobody met yet.
fn world() -> Game {
    let c = content_with_magic();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(11);
    g.content = c;
    g.map = Some(Map::new(Grid::new(30, 20, grass), Grid::new(30, 20, grass)));
    for id in [P0, P1, P2] {
        g.add_player(id, "W");
    }
    g.set_human(Some(P0));
    for id in [P0, P1, P2] {
        g.init_wizard(id, &[("sorcery", 2)], &[], None).unwrap();
    }
    g
}

/// Diplomacy on its own: its state, a fake view of the world and a generator, with the same
/// calls as the game's interface. Most of the rules need nothing more than this.
struct Rig {
    diplomacy: Diplomacy,
    view: FakeView,
    rng: GameRng,
}

impl std::ops::Deref for Rig {
    type Target = FakeView;
    fn deref(&self) -> &FakeView {
        &self.view
    }
}

impl std::ops::DerefMut for Rig {
    fn deref_mut(&mut self) -> &mut FakeView {
        &mut self.view
    }
}

// It mirrors the game's whole interface; a test uses the part it needs.
#[allow(dead_code)]
impl Rig {
    /// Three wizards, the first of them the human, nobody met yet.
    fn new() -> Rig {
        let mut view = FakeView::new(&[P0, P1, P2]).with_content(content_with_magic());
        view.human = Some(P0);
        for id in [P0, P1, P2] {
            view.wizard_mut(id)
                .get_mut::<crate::wizard::sheet::Books>()
                .0
                .insert("sorcery".into(), 2);
        }
        Rig {
            diplomacy: Diplomacy::default(),
            view,
            rng: GameRng::new(11),
        }
    }

    fn lend<R>(&mut self, f: impl FnOnce(&mut Diplomacy, &mut DiplomacyCtx<'_>) -> R) -> R {
        let (result, out) = {
            let mut ctx = DiplomacyCtx {
                view: &self.view,
                rng: &mut self.rng,
                out: Vec::new(),
            };
            let result = f(&mut self.diplomacy, &mut ctx);
            (result, ctx.out)
        };
        for outcome in out {
            match outcome {
                Outcome::Gold { from, to, amount } => {
                    self.view.treasury_mut(from).unwrap().gold -= amount;
                    self.view.treasury_mut(to).unwrap().gold += amount;
                }
                Outcome::Spell { player, spell } => {
                    self.wizard_mut_for_tests(player)
                        .get_mut::<crate::wizard::sheet::KnownSpells>()
                        .known
                        .insert(spell);
                }
            }
        }
        result
    }

    fn wizard_mut_for_tests(&mut self, player: PlayerId) -> crate::wizard::sheet::WizardMut<'_> {
        self.view.wizard_mut(player)
    }

    fn wizards(&self) -> Vec<PlayerId> {
        wizards(&self.view)
    }

    /// The turn: what diplomacy does at its end, and the events it raised.
    fn end_turn(&mut self) -> Vec<Event> {
        self.lend(|d, ctx| d.process_diplomacy(ctx));
        self.view.turn += 1;
        self.drain_diplomacy_events()
    }

    pub fn have_met(&self, a: PlayerId, b: PlayerId) -> bool {
        self.diplomacy.have_met(a, b)
    }

    /// The treaty between two wizards.
    pub fn treaty(&self, a: PlayerId, b: PlayerId) -> Treaty {
        self.diplomacy.treaty(a, b)
    }

    /// The relation between two wizards as the player sees it.
    pub fn relation(&self, a: PlayerId, b: PlayerId) -> i32 {
        self.diplomacy.relation(&self.view, a, b)
    }

    /// The temper of a wizard.
    pub fn personality(&self, wizard: PlayerId) -> Personality {
        crate::game::view::WorldView::personality(&self.view, wizard)
    }

    /// What the hooks raised since the last command, as events.
    pub fn drain_diplomacy_events(&mut self) -> Vec<Event> {
        self.diplomacy.drain_events()
    }

    /// The two wizards meet. Returns whether it was the first time.
    pub fn make_contact(&mut self, a: PlayerId, b: PlayerId) -> bool {
        self.lend(|d, ctx| d.make_contact(ctx, a, b))
    }

    /// `player` has just looked at the squares within `radius` of `at`: it meets the wizards
    /// whose cities or visible units stand there.
    pub fn contact_near(&mut self, player: PlayerId, at: Position, radius: u32) {
        self.lend(|d, ctx| d.contact_near(ctx, player, at, radius));
    }

    /// What `attacker` did moves `defender`'s feeling about it (`Change_Relations`).
    pub fn change_relations(
        &mut self,
        value: i32,
        attacker: PlayerId,
        defender: PlayerId,
        action: Action,
        spell: Option<&str>,
    ) {
        self.lend(|d, ctx| d.change_relations(ctx, value, attacker, defender, action, spell));
    }

    /// A pact or an alliance between two wizards (`Start_Treaty`).
    pub fn start_treaty(&mut self, a: PlayerId, b: PlayerId, treaty: Treaty) {
        self.lend(|d, ctx| d.start_treaty(ctx, a, b, treaty));
    }

    /// `breaker` ends the pact or alliance with `victim` (`Break_Treaties`).
    pub fn break_treaties(&mut self, breaker: PlayerId, victim: PlayerId) {
        self.lend(|d, ctx| d.break_treaties(ctx, breaker, victim));
    }

    /// `attacker` declares war on `defender` (`Declare_War`).
    pub fn declare_war(&mut self, attacker: PlayerId, defender: PlayerId) {
        self.lend(|d, ctx| d.declare_war(ctx, attacker, defender));
    }

    /// The end of a war (`Declare_Peace`).
    pub fn declare_peace(&mut self, a: PlayerId, b: PlayerId) {
        self.lend(|d, ctx| d.declare_peace(ctx, a, b));
    }

    /// The total cost of a wizard's units, the original's army strength.
    pub fn army_strength(&self, player: PlayerId) -> i32 {
        army_strength(&self.view, player)
    }

    /// The original's `Diplomacy_Test` of a proposal that `asker` makes to `judge`: an AI that
    /// answers proposals can use it as its starting logic. It draws on the game's random
    /// numbers and wears down the asker's patience as the original does.
    pub fn rate_proposal(
        &mut self,
        asker: PlayerId,
        judge: PlayerId,
        kind: ProposalKind,
    ) -> Verdict {
        self.lend(|d, ctx| d.rate_proposal(ctx, asker, judge, kind))
    }

    /// Whether `from` may put `kind` to `to` now: the entries the original greys out.
    pub fn check_proposal(
        &self,
        from: PlayerId,
        to: PlayerId,
        kind: ProposalKind,
    ) -> Result<(), DiplomacyError> {
        self.diplomacy.check_proposal(&self.view, from, to, kind)
    }

    /// Carry out a diplomacy command.
    pub fn diplomacy_command(&mut self, command: DiplomacyCommand) -> Vec<Event> {
        self.lend(|d, ctx| d.command(ctx, command))
    }

    pub fn enchantments_in_force(&mut self) {
        self.lend(|d, ctx| d.enchantments_in_force(ctx));
    }

    pub fn enchantment_relations(&mut self, caster: PlayerId, spell: &str, divisor: i32) {
        self.lend(|d, ctx| d.enchantment_relations(ctx, caster, spell, divisor));
    }

    /// A spell was cast: the wizards it offends take note. `target_owner` is whose city it fell on.
    pub fn cast_relations(
        &mut self,
        caster: PlayerId,
        spell: SpellId,
        target_owner: Option<PlayerId>,
    ) {
        self.lend(|d, ctx| d.cast_relations(ctx, caster, spell, target_owner));
    }

    /// A wizard begins the Spell of Mastery: the others take note at once.
    pub fn begin_cast_relations(&mut self, player: PlayerId, spell: SpellId) {
        self.lend(|d, ctx| d.begin_cast_relations(ctx, player, spell));
    }

    /// Attacking a wizard one has a treaty with breaks it first.
    pub fn attack_relations(&mut self, attacker: PlayerId, defender: PlayerId) {
        self.lend(|d, ctx| d.attack_relations(ctx, attacker, defender));
    }

    /// `winner` took a city of `loser` that had people in it.
    pub fn city_taken_relations(&mut self, winner: PlayerId, loser: PlayerId) {
        self.lend(|d, ctx| d.city_taken_relations(ctx, winner, loser));
    }

    /// An attack on a city failed.
    pub fn city_held_relations(
        &mut self,
        attacker: PlayerId,
        defender: PlayerId,
        fortress_city: bool,
    ) {
        self.lend(|d, ctx| d.city_held_relations(ctx, attacker, defender, fortress_city));
    }

    /// A wizard's fortress fell or the wizard was destroyed.
    pub fn conquest_relations(&mut self, winner: PlayerId, loser: PlayerId, destroyed: bool) {
        self.lend(|d, ctx| d.conquest_relations(ctx, winner, loser, destroyed));
    }

    /// The part of the turn that belongs to diplomacy.
    pub fn process_diplomacy(&mut self) {
        self.lend(|d, ctx| d.process_diplomacy(ctx));
    }
}

/// What the tests do to either, the whole game or diplomacy alone.
trait Harness {
    fn met(&mut self, a: PlayerId, b: PlayerId);
    fn set_pair(&mut self, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair));
    fn command(&mut self, c: DiplomacyCommand) -> Vec<Event>;
}

impl Harness for Game {
    fn met(&mut self, a: PlayerId, b: PlayerId) {
        self.make_contact(a, b);
        self.drain_diplomacy_events();
    }
    fn set_pair(&mut self, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair)) {
        self.with_diplomacy(|d, ctx| f(d.pair_mut(ctx.view, a, b)));
    }
    fn command(&mut self, c: DiplomacyCommand) -> Vec<Event> {
        self.apply(Command::Diplomacy(c))
    }
}

impl Harness for Rig {
    fn met(&mut self, a: PlayerId, b: PlayerId) {
        self.make_contact(a, b);
        self.drain_diplomacy_events();
    }
    fn set_pair(&mut self, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair)) {
        self.lend(|d, ctx| f(d.pair_mut(ctx.view, a, b)));
    }
    fn command(&mut self, c: DiplomacyCommand) -> Vec<Event> {
        let mut events = self.diplomacy_command(c);
        events.extend(self.drain_diplomacy_events());
        events
    }
}

fn met(g: &mut impl Harness, a: PlayerId, b: PlayerId) {
    g.met(a, b);
}

fn set_pair(g: &mut impl Harness, a: PlayerId, b: PlayerId, f: impl FnOnce(&mut Pair)) {
    g.set_pair(a, b, f);
}

fn set_rel(g: &mut impl Harness, a: PlayerId, b: PlayerId, v: i32) {
    g.set_pair(a, b, |p| p.visible = v);
}

fn command(g: &mut impl Harness, c: DiplomacyCommand) -> Vec<Event> {
    g.command(c)
}

fn city(g: &mut Game, owner: PlayerId, pos: Position) -> hecs::Entity {
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner,
        pos,
        name: "Town".into(),
        race,
        population: 3,
    });
    g.economy().city_at(pos).unwrap()
}

fn rejected(events: &[Event]) -> Option<DiplomacyError> {
    events.iter().find_map(|e| match e {
        Event::Rejected(Rejection::Diplomacy(e)) => Some(*e),
        _ => None,
    })
}

#[test]
fn the_scale_runs_from_hate_to_harmony() {
    assert_eq!(relation_word(-100), "Hate");
    assert_eq!(relation_word(-81), "Hate");
    assert_eq!(relation_word(-80), "Troubled");
    assert_eq!(relation_word(-60), "Tense");
    assert_eq!(relation_word(0), "Neutral");
    assert_eq!(relation_word(99), "Friendly");
    assert_eq!(relation_word(100), "Harmony");
    assert_eq!(relation_level(0), 5);
}

#[test]
fn start_relations_come_from_the_books() {
    let mut g = world();
    // Life against death: -(life + death) * 5, no shared realm.
    g.init_wizard(P0, &[("life", 3)], &[], None).unwrap();
    g.init_wizard(P1, &[("death", 2)], &[], None).unwrap();
    assert_eq!(g.relation(P0, P1), -25);
    // Shared realms add two for each book of the smaller; chaos costs two for the larger.
    g.init_wizard(P0, &[("nature", 3), ("chaos", 1)], &[], None)
        .unwrap();
    g.init_wizard(P1, &[("nature", 2), ("chaos", 2)], &[], None)
        .unwrap();
    assert_eq!(g.relation(P0, P1), 2);
    // Never below -90.
    g.init_wizard(P0, &[("life", 5)], &[], None).unwrap();
    g.init_wizard(P1, &[("death", 5)], &[], None).unwrap();
    assert_eq!(g.relation(P0, P1), -50);
    g.init_wizard(P0, &[("life", 11)], &[], None).unwrap();
    g.init_wizard(P1, &[("death", 11)], &[], None).unwrap();
    assert_eq!(g.relation(P0, P1), -90);
}

#[test]
fn a_wizard_whose_city_is_seen_is_met() {
    let mut g = world();
    city(&mut g, P1, at(10, 10));
    assert!(!g.have_met(P0, P1));
    g.explore(P0, at(6, 10), 5);
    assert!(!g.have_met(P0, P1), "out of sight");
    g.explore(P0, at(8, 10), 5);
    assert!(g.have_met(P0, P1));
    assert!(g.have_met(P1, P0));
    assert_eq!(g.diplomacy.stance(P0, P1).unwrap().stage, Stage::Greeted);
    assert!(!g.have_met(P0, P2));
}

#[test]
fn the_event_of_a_meeting_comes_back_with_the_command() {
    let mut g = world();
    city(&mut g, P1, at(10, 10));
    let events = g.apply(Command::CreateUnit {
        owner: P0,
        pos: at(9, 10),
        unit: g.content.rules.units.id_of("walker").unwrap(),
    });
    // The unit looks round where it appears.
    assert!(
        events.contains(&Event::Diplomacy(DiplomacyEvent::ContactMade {
            a: P0,
            b: P1
        }))
    );
    assert!(
        g.drain_diplomacy_events().is_empty(),
        "returned with the command, not kept"
    );
}

#[test]
fn the_turn_meets_those_whose_units_one_can_see() {
    let mut g = world();
    city(&mut g, P0, at(10, 10));
    let walker = g.content.rules.units.id_of("walker").unwrap();
    g.spawn_unit(P1, at(12, 10), walker);
    // Forget that they have been shown to each other already.
    set_pair(&mut g, P0, P1, |p| p.contacted = false);
    assert!(!g.have_met(P0, P1));
    let events = g.apply(Command::EndTurn);
    assert!(g.have_met(P0, P1), "{events:?}");
    assert!(
        events.contains(&Event::Diplomacy(DiplomacyEvent::ContactMade {
            a: P0,
            b: P1
        }))
    );
}

#[test]
fn nature_awareness_meets_everyone() {
    let mut g = world();
    city(&mut g, P0, at(2, 2));
    city(&mut g, P1, at(25, 15));
    city(&mut g, P2, at(15, 8));
    let spell = g.content.rules.spells.id_of("nature_awareness").unwrap();
    // The spell's upkeep is paid from the reserve.
    g.treasury_mut(P0).unwrap().mana = 1000;
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::magic::Magic>()
        .global
        .push(spell);
    g.apply(Command::EndTurn);
    assert!(g.have_met(P0, P1) && g.have_met(P0, P2));
}

#[test]
fn good_deeds_count_less_the_better_the_relation() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    met(&mut g, P2, P1);
    g.set_human(Some(P0));
    // P1 is the defender (the human never is): attacker P2.
    set_rel(&mut g, P1, P2, 0);
    g.change_relations(8, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 8);
    // At +50 a gain is halved... divided by 50 / 25 + 1 = 3.
    set_rel(&mut g, P1, P2, 50);
    g.change_relations(9, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 53);
    // From a negative relation it is doubled, but not past +10.
    set_rel(&mut g, P1, P2, -30);
    g.change_relations(8, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), -14);
    set_rel(&mut g, P1, P2, -3);
    g.change_relations(20, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 10);
}

#[test]
fn bad_deeds_count_double_to_a_friend_and_less_to_an_enemy() {
    let mut g = Rig::new();
    met(&mut g, P2, P1);
    set_rel(&mut g, P1, P2, 20);
    g.change_relations(-10, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), 0);
    set_rel(&mut g, P1, P2, -50);
    g.change_relations(-10, P2, P1, Action::AttackedCity, None);
    // Divided by 50 / 25 + 1 = 3.
    assert_eq!(g.relation(P1, P2), -53);
    set_rel(&mut g, P1, P2, -100);
    g.change_relations(-50, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), -100);
}

#[test]
fn temper_and_charm_change_what_a_deed_is_worth() {
    let mut g = Rig::new();
    met(&mut g, P2, P1);
    set_rel(&mut g, P1, P2, 0);
    g.wizard_mut_for_tests(P1).put(Some(Personality::Lawful));
    g.change_relations(-5, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), -10);
    set_rel(&mut g, P1, P2, 0);
    g.wizard_mut_for_tests(P1).put(Some(Personality::Maniacal));
    g.change_relations(-4, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), -6);
    g.change_relations(4, P2, P1, Action::Friendship, None);
    // -6 is negative, so the gain doubles to 8, then Maniacal takes three quarters: 6.
    assert_eq!(g.relation(P1, P2), 0);
    // A charismatic attacker halves its penalties and doubles its gains.
    g.wizard_mut_for_tests(P1).put(Some(Personality::Chaotic));
    g.wizard_mut_for_tests(P2)
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::Charismatic);
    set_rel(&mut g, P1, P2, 0);
    g.change_relations(-10, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), -5);
    set_rel(&mut g, P1, P2, 0);
    g.change_relations(4, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 8);
}

#[test]
fn the_human_is_never_the_one_who_changes() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    let before = g.relation(P0, P1);
    g.change_relations(-40, P1, P0, Action::AttackedCity, None);
    assert_eq!(g.relation(P0, P1), before);
    g.change_relations(-40, P0, P1, Action::AttackedCity, None);
    assert!(g.relation(P0, P1) < before);
}

#[test]
fn strangers_do_not_react_and_neither_does_a_monster() {
    let mut g = Rig::new();
    let before = g.relation(P1, P2);
    g.change_relations(-40, P2, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), before);
    met(&mut g, P2, P1);
    g.change_relations(-40, crate::player::MONSTERS, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P1, P2), before);
}

#[test]
fn a_relation_stays_below_65_without_an_alliance_and_below_minus_25_at_war() {
    let mut g = Rig::new();
    met(&mut g, P2, P1);
    set_rel(&mut g, P1, P2, 60);
    g.change_relations(100, P2, P1, Action::Friendship, None);
    assert_eq!(g.relation(P1, P2), 65);
    g.start_treaty(P1, P2, Treaty::Alliance);
    g.change_relations(100, P2, P1, Action::Friendship, None);
    assert!(g.relation(P1, P2) > 65);
    g.declare_war(P1, P2);
    set_pair(&mut g, P1, P2, |p| p.visible = 40);
    g.change_relations(5, P2, P1, Action::Friendship, None);
    assert!(g.relation(P1, P2) <= -25);
}

#[test]
fn what_a_human_does_in_a_war_does_not_worsen_it_but_wears_down_peace() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    g.declare_war(P1, P0);
    let (rel, patience) = (
        g.relation(P0, P1),
        g.diplomacy.stance(P1, P0).unwrap().peace_patience,
    );
    g.change_relations(-40, P0, P1, Action::AttackedCity, None);
    assert_eq!(g.relation(P0, P1), rel);
    // The war is no worse, but peace is harder: a quarter of the deed (after the
    // diminishing returns of an already bad relation) wears on the patience.
    let wear = -40 / (-rel / 25 + 1) / 4;
    assert!(wear < 0);
    assert_eq!(
        g.diplomacy.stance(P1, P0).unwrap().peace_patience,
        patience + wear
    );
    // Other things (a talk-type action) still count.
    g.change_relations(-10, P0, P1, Action::Trespass, None);
    assert!(g.relation(P0, P1) < rel || rel == -100);
}

#[test]
fn the_last_strongest_act_of_the_human_is_remembered() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    set_rel(&mut g, P0, P1, 0);
    g.change_relations(-5, P0, P1, Action::BadCitySpell, Some("famine"));
    g.change_relations(-20, P0, P1, Action::AttackedCity, None);
    g.change_relations(-3, P0, P1, Action::BadCitySpell, Some("famine"));
    let last = g
        .diplomacy
        .stance(P1, P0)
        .unwrap()
        .last_action
        .clone()
        .unwrap();
    assert_eq!(last.action, Action::AttackedCity);
    assert!(last.strength <= -20);
}

#[test]
fn declaring_war_sets_the_state_the_original_sets() {
    let mut g = Rig::new();
    met(&mut g, P1, P2);
    let default = g.diplomacy.pair(P1, P2).unwrap().default;
    g.declare_war(P1, P2);
    assert_eq!(g.treaty(P1, P2), Treaty::War);
    assert_eq!(g.treaty(P2, P1), Treaty::War);
    let rel = g.relation(P2, P1);
    assert!((-100..=-76).contains(&rel), "{rel}");
    assert_eq!(g.diplomacy.pair(P1, P2).unwrap().default, default - 5);
    for (a, b) in [(P1, P2), (P2, P1)] {
        let s = g.diplomacy.stance(a, b).unwrap();
        assert_eq!(
            (s.treaty_patience, s.exchange_patience, s.peace_patience),
            (-200, -200, -130)
        );
        assert_eq!(s.hostility, 3);
    }
    // Once only.
    let events = g.drain_diplomacy_events();
    let wars = events
        .iter()
        .filter(|e| matches!(e, Event::Diplomacy(DiplomacyEvent::WarDeclared { .. })))
        .count();
    assert_eq!(wars, 1);
    g.declare_war(P1, P2);
    assert!(g.drain_diplomacy_events().is_empty());
}

#[test]
fn breaking_a_treaty_costs_trust_with_everyone() {
    let mut g = Rig::new();
    met(&mut g, P1, P2);
    met(&mut g, P0, P1);
    met(&mut g, P0, P2);
    g.start_treaty(P1, P2, Treaty::Alliance);
    let default = g.diplomacy.pair(P1, P2).unwrap().default;
    g.break_treaties(P1, P2);
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    // The oath breaker's own core reaction falls (the original raises it by mistake).
    assert_eq!(g.diplomacy.stance(P1, P2).unwrap().hidden, -20);
    // Everyone else thinks less of the victim.
    assert_eq!(g.diplomacy.stance(P0, P2).unwrap().hidden, -5);
    assert_eq!(g.diplomacy.pair(P1, P2).unwrap().default, default - 20);
    assert_eq!(
        g.diplomacy.stance(P2, P1).unwrap().broken,
        Some(Treaty::Alliance)
    );
    assert_eq!(g.diplomacy.stance(P1, P2).unwrap().treaty_patience, -200);
    assert_eq!(g.diplomacy.stance(P1, P2).unwrap().peace_patience, -200);
    // A pact costs half as much, a Lawful breaker twice.
    g.wizard_mut_for_tests(P1).put(Some(Personality::Lawful));
    g.start_treaty(P1, P2, Treaty::Pact);
    g.break_treaties(P1, P2);
    assert_eq!(g.diplomacy.stance(P1, P2).unwrap().hidden, -40);
}

#[test]
fn ending_no_treaty_costs_the_breaker_nothing_of_its_own() {
    let mut g = Rig::new();
    met(&mut g, P1, P2);
    g.break_treaties(P1, P2);
    assert_eq!(g.diplomacy.stance(P1, P2).map_or(0, |s| s.hidden), 0);
    assert_eq!(g.diplomacy.stance(P2, P1).and_then(|s| s.broken), None);
    // The others' record of the victim still falls, as in the original.
    assert_eq!(g.diplomacy.stance(P0, P2).map_or(0, |s| s.hidden), -5);
}

#[test]
fn a_pact_is_proposed_and_accepted() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    let events = command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Pact,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Diplomacy(DiplomacyEvent::ProposalMade(p)) if p.kind == ProposalKind::Pact
    )));
    assert_eq!(g.diplomacy.proposals().len(), 1);
    // One at a time.
    let again = command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Alliance,
        },
    );
    assert_eq!(rejected(&again), Some(DiplomacyError::AlreadyPending));
    let events = command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
    assert_eq!(g.treaty(P0, P1), Treaty::Pact);
    assert!(g.diplomacy.proposals().is_empty());
}

#[test]
fn a_refused_proposal_changes_nothing() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Alliance,
        },
    );
    let events = command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: false,
        },
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
            accepted: false,
            lapsed: false,
            ..
        })
    )));
    assert_eq!(g.treaty(P0, P1), Treaty::None);
    let none = command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(rejected(&none), Some(DiplomacyError::NothingPending));
}

#[test]
fn what_the_treaty_allows_decides_what_can_be_proposed() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    met(&mut g, P0, P2);
    met(&mut g, P1, P2);
    let ok = |g: &Rig, from, to, kind| g.check_proposal(from, to, kind).is_ok();
    assert!(ok(&g, P0, P1, ProposalKind::Pact));
    assert!(ok(&g, P0, P1, ProposalKind::Alliance));
    assert!(!ok(&g, P0, P1, ProposalKind::Peace));
    assert!(ok(&g, P0, P1, ProposalKind::WarOn(P2)));
    assert!(!ok(&g, P0, P1, ProposalKind::WarOn(P0)));
    assert!(
        !ok(&g, P0, P1, ProposalKind::BreakAllianceWith(P2)),
        "no alliance to break"
    );
    g.start_treaty(P1, P2, Treaty::Alliance);
    assert!(ok(&g, P0, P1, ProposalKind::BreakAllianceWith(P2)));
    assert!(
        ok(&g, P0, P1, ProposalKind::WarOn(P2)),
        "allies may still be sent to war"
    );
    g.declare_war(P1, P2);
    assert!(!ok(&g, P0, P1, ProposalKind::WarOn(P2)), "already at war");
}

#[test]
fn a_pact_cannot_be_offered_twice_nor_in_a_war() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    g.start_treaty(P0, P1, Treaty::Pact);
    assert!(g.check_proposal(P0, P1, ProposalKind::Pact).is_err());
    assert!(g.check_proposal(P0, P1, ProposalKind::Alliance).is_ok());
    g.declare_war(P1, P0);
    assert!(g.check_proposal(P0, P1, ProposalKind::Pact).is_err());
    assert!(g.check_proposal(P0, P1, ProposalKind::Alliance).is_err());
    assert!(g.check_proposal(P0, P1, ProposalKind::Peace).is_ok());
    assert_eq!(
        g.check_proposal(P0, P2, ProposalKind::Pact),
        Err(DiplomacyError::NotContacted)
    );
    assert_eq!(
        g.check_proposal(P0, P0, ProposalKind::Pact),
        Err(DiplomacyError::SameWizard)
    );
}

#[test]
fn peace_ends_a_war_and_the_human_is_left_alone_a_while() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    g.declare_war(P1, P0);
    g.drain_diplomacy_events();
    let before = g.relation(P0, P1);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Peace,
        },
    );
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(g.treaty(P0, P1), Treaty::None);
    assert_eq!(g.relation(P0, P1), (before + 40).min(100));
    let peace = g.diplomacy.stance(P1, P0).unwrap().peace_duration;
    assert!((8..=13).contains(&peace), "{peace}");
    assert_eq!(g.diplomacy.stance(P0, P1).unwrap().peace_duration, 0);
}

#[test]
fn a_war_on_a_third_wizard_is_declared_by_the_one_asked() {
    let mut g = Rig::new();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::WarOn(P2),
        },
    );
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(g.treaty(P1, P2), Treaty::War);
    assert_eq!(g.treaty(P0, P2), Treaty::None);
}

#[test]
fn asking_an_ally_to_break_with_another_makes_it_do_so() {
    let mut g = Rig::new();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    g.start_treaty(P1, P2, Treaty::Alliance);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::BreakAllianceWith(P2),
        },
    );
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    assert_eq!(g.diplomacy.stance(P1, P2).unwrap().hidden, -20);
}

#[test]
fn an_unanswered_proposal_lapses_at_the_end_of_the_turn() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Pact,
        },
    );
    let events = g.end_turn();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
            accepted: false,
            lapsed: true,
            ..
        })
    )));
    assert!(g.diplomacy.proposals().is_empty());
    assert_eq!(g.treaty(P0, P1), Treaty::None);
}

#[test]
fn a_pact_that_stopped_being_possible_is_not_signed() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind: ProposalKind::Pact,
        },
    );
    g.declare_war(P1, P0);
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert_eq!(g.treaty(P0, P1), Treaty::War);
}

#[test]
fn breaking_a_pact_by_command_needs_one() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    let none = command(
        &mut g,
        DiplomacyCommand::BreakTreaty {
            player: P0,
            with: P1,
        },
    );
    assert_eq!(rejected(&none), Some(DiplomacyError::NotAllowed));
    g.start_treaty(P0, P1, Treaty::Pact);
    command(
        &mut g,
        DiplomacyCommand::BreakTreaty {
            player: P0,
            with: P1,
        },
    );
    assert_eq!(g.treaty(P0, P1), Treaty::None);
    assert_eq!(g.diplomacy.stance(P0, P1).unwrap().hidden, -10);
}

#[test]
fn a_wizard_can_declare_war_only_on_one_it_has_met() {
    let mut g = Rig::new();
    let events = command(&mut g, DiplomacyCommand::DeclareWar { player: P1, on: P2 });
    assert_eq!(rejected(&events), Some(DiplomacyError::NotContacted));
    met(&mut g, P1, P2);
    command(&mut g, DiplomacyCommand::DeclareWar { player: P1, on: P2 });
    assert_eq!(g.treaty(P1, P2), Treaty::War);
}

#[test]
fn gold_is_a_gift_that_warms() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    set_rel(&mut g, P0, P1, 0);
    g.treasury_mut(P0).unwrap().gold = 300;
    g.treasury_mut(P1).unwrap().gold = 100;
    let events = command(
        &mut g,
        DiplomacyCommand::GiveGold {
            from: P0,
            to: P1,
            amount: 100,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
    assert_eq!(g.treasury(P0).unwrap().gold, 200);
    assert_eq!(g.treasury(P1).unwrap().gold, 200);
    assert!(g.relation(P0, P1) >= 0);
    let broke = command(
        &mut g,
        DiplomacyCommand::GiveGold {
            from: P0,
            to: P1,
            amount: 999,
        },
    );
    assert_eq!(rejected(&broke), Some(DiplomacyError::NotEnoughGold));
    let nobody = command(
        &mut g,
        DiplomacyCommand::GiveGold {
            from: P0,
            to: P2,
            amount: 10,
        },
    );
    assert_eq!(rejected(&nobody), Some(DiplomacyError::NotContacted));
}

#[test]
fn a_spell_given_is_known_and_wins_trust() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    let famine = g.content.rules.spells.id_of("famine").unwrap();
    g.wizard_mut_for_tests(P0)
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(famine);
    g.wizard_mut_for_tests(P1)
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .remove(&famine);
    set_rel(&mut g, P0, P1, 0);
    let events = command(
        &mut g,
        DiplomacyCommand::GiveSpell {
            from: P0,
            to: P1,
            spell: famine,
        },
    );
    assert!(rejected(&events).is_none(), "{events:?}");
    assert!(g.wizard(P1).unwrap().knows(famine));
    assert!(g.relation(P0, P1) > 0);
    let hidden = g.diplomacy.stance(P1, P0).unwrap().hidden;
    assert!((2..=9).contains(&hidden), "{hidden}");
    // It is not worth giving twice.
    let again = command(
        &mut g,
        DiplomacyCommand::GiveSpell {
            from: P0,
            to: P1,
            spell: famine,
        },
    );
    assert_eq!(rejected(&again), Some(DiplomacyError::SpellUnavailable));
}

#[test]
fn spells_are_exchanged_one_for_one() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    let famine = g.content.rules.spells.id_of("famine").unwrap();
    let gaia = g.content.rules.spells.id_of("gaias_blessing").unwrap();
    for s in [famine, gaia] {
        for p in [P0, P1] {
            g.wizard_mut_for_tests(p)
                .get_mut::<crate::wizard::sheet::KnownSpells>()
                .known
                .remove(&s);
        }
    }
    g.wizard_mut_for_tests(P0)
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(famine);
    g.wizard_mut_for_tests(P1)
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(gaia);
    let kind = ProposalKind::Exchange {
        give: famine,
        get: gaia,
    };
    assert!(g.check_proposal(P0, P1, kind).is_ok());
    command(
        &mut g,
        DiplomacyCommand::Propose {
            from: P0,
            to: P1,
            kind,
        },
    );
    command(
        &mut g,
        DiplomacyCommand::Answer {
            player: P1,
            from: P0,
            accept: true,
        },
    );
    assert!(g.wizard(P0).unwrap().knows(gaia));
    assert!(g.wizard(P1).unwrap().knows(famine));
    // Nothing left to swap.
    assert!(g.check_proposal(P0, P1, kind).is_err());
}

#[test]
fn the_rating_of_a_proposal_is_the_originals() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    // A pact below +10 is refused out of hand, and costs no patience.
    set_rel(&mut g, P0, P1, 10);
    assert_eq!(
        g.rate_proposal(P0, P1, ProposalKind::Pact),
        Verdict::Refused
    );
    assert_eq!(
        g.diplomacy.stance(P0, P1).map_or(0, |s| s.treaty_patience),
        0
    );
    // A great friend with great patience accepts a pact; patience then falls.
    set_rel(&mut g, P0, P1, 90);
    g.diplomacy.stance_mut(P0, P1).hidden = 30;
    g.diplomacy.stance_mut(P0, P1).treaty_patience = 100;
    g.wizard_mut_for_tests(P1).put(Some(Personality::Peaceful));
    assert_eq!(
        g.rate_proposal(P0, P1, ProposalKind::Pact),
        Verdict::Accepted
    );
    let s = g.diplomacy.stance(P0, P1).unwrap();
    assert!(
        (100 - 10 - 49..=100 - 10 - 21).contains(&s.treaty_patience),
        "{}",
        s.treaty_patience
    );
    assert_eq!((s.exchange_patience, s.peace_patience), (-10, -10));
    // An alliance needs above +50.
    set_rel(&mut g, P0, P1, 50);
    assert_eq!(
        g.rate_proposal(P0, P1, ProposalKind::Alliance),
        Verdict::Refused
    );
}

#[test]
fn a_bigger_army_makes_a_wizard_more_pliant() {
    let mut g = world();
    met(&mut g, P0, P1);
    let spear = g.content.rules.units.id_of("walker").unwrap();
    let cost = g.content.rules.units.get(spear).cost as i32;
    // Neither has an army: the asker is 'weaker', no threat to speak of.
    assert_eq!(threat_modifier(&g, P0, P1), -50);
    for _ in 0..4 {
        g.spawn_unit(P0, at(1, 1), spear);
    }
    assert_eq!(threat_modifier(&g, P0, P1), 50, "the other has no army");
    g.spawn_unit(P1, at(5, 5), spear);
    assert_eq!(threat_modifier(&g, P0, P1), -50 + 4 * cost * 100 / cost);
}

#[test]
fn peace_runs_down_and_patience_comes_back() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    g.diplomacy.stance_mut(P1, P0).peace_duration = 3;
    g.diplomacy.stance_mut(P1, P0).treaty_patience = -100;
    g.diplomacy.stance_mut(P1, P0).peace_patience = 100;
    g.end_turn();
    let s = g.diplomacy.stance(P1, P0).unwrap();
    assert_eq!(s.peace_duration, 2);
    // +10 at the end of the turn, then up to twice 1..5 while below zero (the first)
    // and below fifty (the second).
    assert!(
        (-100 + 10 + 2..=-100 + 10 + 10).contains(&s.treaty_patience),
        "{}",
        s.treaty_patience
    );
    assert_eq!(s.peace_patience, 100, "above 100 nothing is added");
}

#[test]
fn a_pact_makes_friends_over_time() {
    let mut g = Rig::new();
    met(&mut g, P1, P2);
    set_rel(&mut g, P1, P2, 0);
    set_pair(&mut g, P1, P2, |p| p.default = 0);
    g.start_treaty(P1, P2, Treaty::Alliance);
    for _ in 0..40 {
        g.end_turn();
    }
    assert!(g.relation(P1, P2) > 10, "{}", g.relation(P1, P2));
    assert!(g.relation(P1, P2) <= 100);
}

#[test]
fn without_cause_relations_drift_back_to_the_start() {
    let mut g = Rig::new();
    met(&mut g, P1, P2);
    let default = g.diplomacy.pair(P1, P2).unwrap().default;
    set_rel(&mut g, P1, P2, default - 30);
    for _ in 0..60 {
        g.end_turn();
    }
    assert_eq!(g.relation(P1, P2), default);
    // From above it comes down every tenth turn only.
    set_rel(&mut g, P1, P2, default + 20);
    for _ in 0..200 {
        g.end_turn();
    }
    assert_eq!(g.relation(P1, P2), default);
}

#[test]
fn a_recent_deed_holds_the_drift_for_a_turn() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    let default = g.diplomacy.pair(P0, P1).unwrap().default;
    set_rel(&mut g, P0, P1, default + 30);
    g.diplomacy.stance_mut(P1, P0).last_action = Some(LastAction {
        action: Action::Friendship,
        strength: 5,
        spell: None,
    });
    g.end_turn();
    assert_eq!(g.relation(P0, P1), default + 30);
    assert!(g.diplomacy.stance(P1, P0).unwrap().last_action.is_none());
}

#[test]
fn enchantments_offend_the_wizards_they_offend() {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    g.init_wizard(P1, &[("life", 2)], &[], None).unwrap();
    g.init_wizard(P2, &[("chaos", 2)], &[], None).unwrap();
    for (a, b) in [(P1, P0), (P2, P0)] {
        set_rel(&mut g, a, b, 0);
    }
    // Eternal Night offends both; Evil Omens only a Life or Nature wizard.
    g.enchantment_relations(P0, "eternal_night", 1);
    assert_eq!(g.relation(P1, P0), -12);
    assert_eq!(g.relation(P2, P0), -12);
    set_rel(&mut g, P1, P0, 0);
    set_rel(&mut g, P2, P0, 0);
    g.enchantment_relations(P0, "evil_omens", 1);
    assert_eq!(g.relation(P1, P0), -20);
    assert_eq!(g.relation(P2, P0), 0, "a chaos wizard does not mind");
    // The per-turn share is a tenth, rounded to zero.
    set_rel(&mut g, P1, P0, 0);
    g.enchantment_relations(P0, "eternal_night", 10);
    assert_eq!(g.relation(P1, P0), -1);
    // Nothing for a spell nobody minds.
    set_rel(&mut g, P1, P0, 0);
    g.enchantment_relations(P0, "water_walking", 1);
    assert_eq!(g.relation(P1, P0), 0);
}

#[test]
fn a_curse_on_a_city_is_noticed_by_its_owner() {
    let mut g = Rig::new();
    met(&mut g, P0, P1);
    set_rel(&mut g, P0, P1, 0);
    let famine = g.content.rules.spells.id_of("famine").unwrap();
    g.cast_relations(P0, famine, Some(P1));
    assert_eq!(g.relation(P0, P1), -5);
    // On one's own city it is nobody's business.
    set_rel(&mut g, P0, P1, 0);
    g.cast_relations(P0, famine, None);
    assert_eq!(g.relation(P0, P1), 0);
}

#[test]
fn casting_the_spell_of_mastery_turns_the_others_against_the_caster() {
    let mut g = Rig::new();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    for b in [P1, P2] {
        set_rel(&mut g, P0, b, 0);
    }
    let mastery = g.content.rules.spells.id_of("spell_of_mastery").unwrap();
    g.begin_cast_relations(P0, mastery);
    // One time in nine the original has the offended wizard declare war as well.
    if g.treaty(P0, P1) == Treaty::War {
        assert!(g.relation(P0, P1) <= -75);
    } else {
        assert_eq!(g.relation(P0, P1), -50);
    }
    // Begun, the Spell of Mastery keeps bothering them a little each turn (half of fifty).
    g.wizard_mut_for_tests(P1)
        .get_mut::<crate::wizard::magic::Magic>()
        .casting = Some(crate::wizard::magic::Casting {
        spell: mastery,
        remaining: 100,
    });
    set_rel(&mut g, P1, P2, 0);
    g.enchantments_in_force();
    // Half of fifty is twenty-five.
    assert_eq!(g.relation(P1, P2), -25);
}

#[test]
fn attacking_a_partner_breaks_the_treaty_and_costs_relations() {
    let mut g = Rig::new();
    met(&mut g, P1, P2);
    g.start_treaty(P1, P2, Treaty::Pact);
    set_rel(&mut g, P1, P2, 30);
    g.attack_relations(P1, P2);
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    assert!(g.relation(P1, P2) < 30 - 40 + 1, "{}", g.relation(P1, P2));
    assert_eq!(g.diplomacy.stance(P1, P2).unwrap().hidden, -10);
    // With no treaty there is nothing to break and nothing to lose.
    let before = g.relation(P1, P2);
    g.attack_relations(P1, P2);
    assert_eq!(g.relation(P1, P2), before);
}

#[test]
fn a_battle_against_a_partner_breaks_the_pact() {
    let mut g = world();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(P1, at(5, 5), walker);
    let d = g.spawn_unit(P2, at(6, 5), walker);
    met(&mut g, P1, P2);
    g.start_treaty(P1, P2, Treaty::Pact);
    g.drain_diplomacy_events();
    let events = g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::BattleStarted { .. })),
        "{events:?}"
    );
    assert_eq!(g.treaty(P1, P2), Treaty::None);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Diplomacy(DiplomacyEvent::TreatyBroken { breaker, victim, treaty: Treaty::Pact })
            if *breaker == P1 && *victim == P2
    )));
}

#[test]
fn a_battle_against_a_stranger_makes_them_known_to_each_other() {
    let mut g = world();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(P1, at(5, 5), walker);
    let d = g.spawn_unit(P2, at(6, 5), walker);
    set_pair(&mut g, P1, P2, |p| p.contacted = false);
    g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    assert!(g.have_met(P1, P2));
    assert_eq!(g.treaty(P1, P2), Treaty::None);
}

#[test]
fn attacking_a_wizard_one_has_not_met_meets_it() {
    let mut g = Rig::new();
    g.attack_relations(P1, P2);
    assert!(g.have_met(P1, P2));
}

#[test]
fn taking_a_city_is_a_war() {
    let mut g = world();
    let target = city(&mut g, P1, at(10, 10));
    city(&mut g, P1, at(20, 5));
    let events = g.conquer_city(target, P2, 0);
    assert!(!events.is_empty());
    assert_eq!(g.treaty(P1, P2), Treaty::War);
    assert!(g.relation(P1, P2) <= -76);
    let events = g.drain_diplomacy_events();
    assert!(
        events.contains(&Event::Diplomacy(DiplomacyEvent::WarDeclared {
            attacker: P2,
            defender: P1,
        }))
    );
}

#[test]
fn taking_an_independent_city_is_no_diplomacy() {
    let mut g = world();
    g.add_neutral_player(PlayerId(9));
    g.treasury_mut(PlayerId(9)).unwrap().gold = 0;
    let target = city(&mut g, PlayerId(9), at(10, 10));
    g.conquer_city(target, P1, 0);
    assert!(g.drain_diplomacy_events().is_empty());
    assert!(!g.have_met(P1, PlayerId(9)));
}

#[test]
fn a_city_that_holds_costs_the_attacker_twenty_or_sixty() {
    let mut g = Rig::new();
    met(&mut g, P1, P2);
    set_rel(&mut g, P1, P2, 0);
    g.city_held_relations(P2, P1, false);
    assert_eq!(g.relation(P1, P2), -20);
    set_rel(&mut g, P1, P2, 0);
    g.city_held_relations(P2, P1, true);
    assert_eq!(g.relation(P1, P2), -60);
}

#[test]
fn a_fallen_fortress_is_held_against_the_winner_by_everyone_else() {
    let mut g = Rig::new();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    set_rel(&mut g, P1, P2, 0);
    g.conquest_relations(P2, P0, false);
    assert!(g.relation(P1, P2) < 0);
    assert!(g.have_met(P0, P1), "a banished wizard is still known");
    // A wizard destroyed is forgotten, and costs more.
    let banished = g.relation(P1, P2);
    set_rel(&mut g, P1, P2, 0);
    g.conquest_relations(P2, P0, true);
    assert!(g.relation(P1, P2) < banished.min(0));
    assert!(!g.have_met(P0, P1));
    assert!(!g.have_met(P0, P2));
    assert_eq!(g.treaty(P0, P1), Treaty::None);
}

#[test]
fn diplomacy_survives_a_save() {
    let mut g = world();
    for (a, b) in [(P0, P1), (P0, P2), (P1, P2)] {
        met(&mut g, a, b);
    }
    g.start_treaty(P1, P2, Treaty::Alliance);
    g.declare_war(P0, P1);
    g.wizard_mut(P2).unwrap().put(Some(Personality::Lawful));
    g.diplomacy.stance_mut(P2, P0).last_action = Some(LastAction {
        action: Action::AttackedCity,
        strength: -20,
        spell: Some("famine".into()),
    });
    g.diplomacy.proposals.push(Proposal {
        from: P0,
        to: P2,
        kind: ProposalKind::WarOn(P1),
    });
    let famine = g.content.rules.spells.id_of("famine").unwrap();
    let gaia = g.content.rules.spells.id_of("gaias_blessing").unwrap();
    g.diplomacy.proposals.push(Proposal {
        from: P1,
        to: P2,
        kind: ProposalKind::Exchange {
            give: famine,
            get: gaia,
        },
    });
    g.drain_diplomacy_events();
    let snap: Snapshot = g.snapshot();
    let text = ron::to_string(&snap).unwrap();
    let back: Snapshot = ron::from_str(&text).unwrap();
    assert_eq!(back, snap);
    let h = back.restore(content_with_magic()).unwrap();
    assert_eq!(h.diplomacy, g.diplomacy);
    assert_eq!(h.treaty(P1, P2), Treaty::Alliance);
    assert_eq!(h.treaty(P0, P1), Treaty::War);
    assert_eq!(h.personality(P2), Personality::Lawful);
}

#[test]
fn an_old_save_has_no_diplomacy() {
    let g = world();
    let mut snap = g.snapshot();
    snap.diplomacy = DiplomacySave::default();
    let h = snap.restore(content_with_magic()).unwrap();
    assert!(!h.have_met(P0, P1));
    assert_eq!(h.relation(P0, P1), g.relation(P0, P1));
}

#[test]
fn the_same_game_replays_the_same_diplomacy() {
    let play = || {
        let mut g = world();
        city(&mut g, P0, at(10, 10));
        city(&mut g, P1, at(14, 10));
        met(&mut g, P1, P2);
        g.declare_war(P1, P2);
        for _ in 0..30 {
            g.apply(Command::EndTurn);
        }
        g.diplomacy.clone()
    };
    assert_eq!(play(), play());
}
