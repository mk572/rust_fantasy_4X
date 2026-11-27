//! What a wizard feels about each of the others (task 780, `docs/diplomacy.md` section 5).
//!
//! The AI owns its opinions: the core only says what happened (events) and the AI changes its own
//! rating by the table of `Change_Relations`, with no dice (every `Random(n)` of the original is its
//! average). The opinion of the human is the AI's feeling for the human; what the human is shown.
//!
//! A first opinion starts from the books of the two wizards, as the original does (the core holds no table of
//! feelings: task 775).

use f4x_core::diplomacy::Treaty;
use f4x_core::diplomacy::{DiplomacyEvent, ProposalKind};
use f4x_core::player::PlayerId;
use f4x_core::rules::{RealmId, Stat};
use f4x_core::wizard::objective::Personality;
use f4x_core::world::{Event, World};
use serde::{Deserialize, Serialize};

use crate::memory::Memory;

/// What moved a feeling (`Change_Relations`): only a war's feeling is not made worse by what is done in it, and
/// only trespass is made worse by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// A treaty growing, a gift, the Aura of Majesty.
    Friendship,
    /// A wizard destroyed or a fortress taken, an army too strong, a threat.
    Threat,
    /// Losses in a battle.
    BattleLosses,
    /// Attacking or taking a city.
    AttackedCity,
    SpellOfMastery,
    /// An overland enchantment others dislike.
    Enchantment,
    /// A curse on a city.
    BadCitySpell,
    /// Too many cities.
    Overextension,
    /// The Aura of Majesty pleases.
    AuraOfMajesty,
    /// Units too close to a wizard's city.
    Trespass,
}

/// The keys of the words of the relation scale, `Hate` to `Harmony`.
pub const RELATION_KEYS: &[&str] = &[
    "relation.hate",
    "relation.troubled",
    "relation.tense",
    "relation.restless",
    "relation.unease",
    "relation.neutral",
    "relation.relaxed",
    "relation.calm",
    "relation.peaceful",
    "relation.friendly",
    "relation.harmony",
];

/// Where a feeling sits on the scale: 0 (hate) to 10 (harmony).
pub fn relation_level(relation: i32) -> usize {
    ((100 + relation.clamp(-100, 100)) / 20) as usize
}

/// The key of the word for a feeling.
pub fn relation_key(relation: i32) -> &'static str {
    RELATION_KEYS[relation_level(relation)]
}

/// What one wizard feels about another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Opinion {
    pub with: PlayerId,
    /// How friendly it feels, -100..=100: what the human is shown.
    pub friendship: i32,
    /// What `friendship` drifts back to.
    pub default: i32,
    /// The lasting part: oaths broken, spells given. Not shown.
    pub hidden: i32,
    /// This one broke a treaty with it.
    pub broken: bool,
    /// The two are at war (the opinion was last moved by a war).
    pub war: bool,
    /// What the per-turn look has added up in hundredths of a point (a rule that the original rolls a
    /// die for, at its average, is a small part of a point a turn: task 783).
    #[serde(default)]
    pub carry: i32,
    /// Turns in a row the other has had units next to one of its cities, while they have a pact or an alliance.
    #[serde(default)]
    pub trespass: u32,
    /// The last turn the other changed this feeling by what it did (the drift waits for a turn).
    #[serde(default)]
    pub moved: u32,
}

/// How a book of realm `x` stands to a book of realm `y` on the colour wheel (kin, next to it, opposed).
fn wheel_affinity(x: RealmId, y: RealmId) -> i32 {
    // The realms' numbers in wheel order: chaos, nature, life, sorcery, death.
    const WHEEL: [u16; 5] = [2, 0, 3, 1, 4];
    let place = |r: RealmId| WHEEL.iter().position(|&n| n == r.num());
    let (Some(i), Some(j)) = (place(x), place(y)) else {
        return 0;
    };
    match (i + 5 - j) % 5 {
        0 => 2,
        1 | 4 => 1,
        _ => -3,
    }
}

/// The feeling two wizards start with, from their books (`Init_Diplomatic_Relations`), or from the
/// colour wheel when the setting `color_wheel_relations` is on.
pub(crate) fn start_feeling(world: &World, a: PlayerId, b: PlayerId) -> i32 {
    let books = |p: PlayerId, r: RealmId| -> i32 { world.wizard(p).map_or(0, |m| m.books_in(r)) };
    if world.settings().rules.color_wheel_relations {
        let realms = [2u16, 0, 3, 1, 4].map(RealmId::new);
        let score: i32 = realms
            .iter()
            .flat_map(|&x| realms.iter().map(move |&y| (x, y)))
            .map(|(x, y)| books(a, x).min(books(b, y)) * wheel_affinity(x, y))
            .sum();
        return score.max(-90);
    }
    let (death, life, chaos) = (RealmId::new(4), RealmId::new(3), RealmId::new(2));
    let max_death = books(a, death).max(books(b, death));
    let sum_life = books(a, life) + books(b, life);
    let max_chaos = books(a, chaos).max(books(b, chaos));
    let mut score = if sum_life > 0 && max_death > 0 {
        -(sum_life + max_death) * 5
    } else {
        sum_life * 2 - max_death * 3
    };
    score -= max_chaos * 2;
    for realm in [RealmId::new(1), chaos, RealmId::new(0), life] {
        score += books(a, realm).min(books(b, realm)) * 2;
    }
    score.max(-90)
}

impl Memory {
    /// What `me` feels about `other`: the opinion it has kept, or the one it would start with.
    pub(crate) fn opinion(&self, world: &World, me: PlayerId, other: PlayerId) -> Opinion {
        self.opinions
            .iter()
            .find(|o| o.with == other)
            .copied()
            .unwrap_or_else(|| first_opinion(world, me, other))
    }

    /// How friendly `me` feels about `other`, -100..=100.
    pub(crate) fn friendship(&self, world: &World, me: PlayerId, other: PlayerId) -> i32 {
        self.opinion(world, me, other).friendship
    }

    fn slot(&mut self, world: &World, me: PlayerId, other: PlayerId) -> &mut Opinion {
        let i = match self.opinions.iter().position(|o| o.with == other) {
            Some(i) => i,
            None => {
                self.opinions.push(first_opinion(world, me, other));
                self.opinions.len() - 1
            }
        };
        &mut self.opinions[i]
    }

    /// Set the friendship outright (the tests; the client's demo helpers, task 784).
    pub(crate) fn set_friendship(&mut self, world: &World, me: PlayerId, other: PlayerId, to: i32) {
        self.slot(world, me, other).friendship = to.clamp(-100, 100);
    }

    /// What `from` did moves `me`'s feeling about it (`Change_Relations`): worse after a better one
    /// (diminishing returns), doubled for a Lawful wizard, and so on. `from` and `me` must have met.
    pub(crate) fn change(
        &mut self,
        world: &World,
        me: PlayerId,
        from: PlayerId,
        value: i32,
        action: Action,
        round: u32,
    ) {
        if value == 0 || from == me || !world.have_met(me, from) {
            return;
        }
        let rel = self.friendship(world, me, from);
        let mut v = value;
        if v > 0 {
            if rel < 0 {
                v *= 2;
                if rel + v > 10 {
                    v = 10 - rel;
                }
            } else {
                v /= rel / 25 + 1;
            }
        } else if rel > 0 {
            v *= 2;
        } else {
            v /= -rel / 25 + 1;
        }
        match world.personality(me) {
            Personality::Lawful if v < 0 => v *= 2,
            Personality::Maniacal if v < 0 => v = v * 3 / 2,
            Personality::Maniacal if v > 0 => v = v * 3 / 4,
            _ => {}
        }
        if let Some(m) = world.wizard(from) {
            let rules = &world.content().rules;
            if v > 0 {
                v = v * m.stat(rules, Stat::DiplomacyGainPercent, 100) / 100;
            } else if v < 0 {
                v = v * m.stat(rules, Stat::DiplomacyLossPercent, 100) / 100;
            }
        }
        let treaty = world.treaty(me, from);
        let o = self.slot(world, me, from);
        if world.human() == Some(from) && action != Action::AuraOfMajesty {
            o.moved = round;
        }
        // Wars are not made worse by what one does in them.
        if !(treaty == Treaty::War && action != Action::Trespass && v < 0) {
            o.friendship = (o.friendship + v).clamp(-100, 100);
        }
        if treaty == Treaty::War {
            o.friendship = o.friendship.min(-25);
        }
        if treaty != Treaty::Alliance {
            o.friendship = o.friendship.min(65);
        }
    }

    /// What an event does to `me`'s feelings (task 780): the facts the core reports, which the AI weighs by
    /// the table of `docs/diplomacy.md` section 5. (The rest of the table comes with 782 and 783.)
    pub(crate) fn feel(&mut self, world: &World, me: PlayerId, event: &Event, round: u32) {
        match *event {
            Event::CityCaptured { from, to, .. } if from == me => {
                self.change(world, me, to, -40, Action::AttackedCity, round);
            }
            Event::Diplomacy(DiplomacyEvent::WarDeclared { attacker, defender }) => {
                if me == attacker {
                    self.war_began(world, me, defender);
                } else if me == defender {
                    self.war_began(world, me, attacker);
                }
            }
            Event::Diplomacy(DiplomacyEvent::TreatyChanged { a, b, treaty })
                if treaty != Treaty::War =>
            {
                if treaty == Treaty::None && (me == a || me == b) {
                    self.war_ended(world, me, if me == a { b } else { a });
                }
            }
            Event::Diplomacy(DiplomacyEvent::TreatyBroken {
                breaker,
                victim,
                treaty,
            }) if me != breaker => {
                let mut penalty = match treaty {
                    Treaty::Pact => 10,
                    Treaty::Alliance => 20,
                    _ => 0,
                };
                if penalty == 0 {
                    return;
                }
                if world.personality(breaker) == Personality::Lawful {
                    penalty *= 2;
                }
                if me == victim {
                    let o = self.slot(world, me, breaker);
                    o.hidden = (o.hidden - penalty).max(-100);
                    // The original takes `Random(20)` off the relation: its average.
                    o.friendship = (o.friendship - 10).max(-100);
                    if treaty == Treaty::Alliance {
                        o.default = (o.default - penalty).max(-100);
                    }
                    o.broken = true;
                } else {
                    // Every other wizard thinks less of the oath-breaker.
                    let o = self.slot(world, me, breaker);
                    o.hidden = (o.hidden - 5).max(-100);
                }
            }
            Event::Diplomacy(DiplomacyEvent::GoldGiven { from, to, gold }) if to == me => {
                // A larger share of what it holds, the warmer the thanks; the original rolls
                // `Random(3)` (0..2) over ten: one tenth, rounded.
                let reserve = world
                    .player(me)
                    .map_or(0, |p| p.treasury().gold.floor() - gold)
                    .max(1);
                let choice = ((gold * 4 + reserve - 1) / reserve).clamp(1, 4);
                let base = if world.army_strength(me) > 0 {
                    choice * 4
                } else {
                    8
                };
                let o = self.slot(world, me, from);
                o.friendship = (o.friendship + (base + 5) / 10).min(100);
            }
            Event::Diplomacy(DiplomacyEvent::SpellGiven { from, to, .. }) if to == me => {
                let rel = self.friendship(world, me, from);
                let base = if rel < 0 { 20 } else { (100 - rel) / 10 };
                // `base * (Random(8) + Random(8)) / 10`, the two rolls at their average of 3.5; and
                // the hidden regard `2 + Random(8)`.
                let o = self.slot(world, me, from);
                o.friendship = (o.friendship + base * 7 / 10).min(100);
                o.hidden = (o.hidden + 5).min(30);
            }
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                proposal,
                accepted,
                lapsed: false,
            }) => match proposal.kind {
                // Turning a demand down sours the one who made it (`Random(10) + 1`).
                ProposalKind::Demand { .. } if !accepted && proposal.from == me => {
                    self.change(world, me, proposal.to, -6, Action::Threat, round);
                }
                // A threat costs the threatener the goodwill of the one threatened (`Random(15)`).
                ProposalKind::Threat if proposal.to == me => {
                    self.change(world, me, proposal.from, -7, Action::Threat, round);
                }
                // A trade that was agreed pleases both (the original: when it is the better deal).
                ProposalKind::Trade { .. }
                    if accepted && (proposal.to == me || proposal.from == me) =>
                {
                    let other = if proposal.to == me {
                        proposal.from
                    } else {
                        proposal.to
                    };
                    self.change(world, me, other, 5, Action::Friendship, round);
                }
                _ => {}
            },
            Event::Diplomacy(DiplomacyEvent::BattleLosses {
                attacker,
                defender,
                heroes,
                others,
                settlers,
            }) if defender == me => {
                // A hero counts 20, another unit `Random(20)` and a settler `Random(10)` more: the
                // rolls at their averages (19/2 and 9/2).
                let lost = 20 * heroes + (19 * others + 9 * settlers) / 2;
                self.change(
                    world,
                    me,
                    attacker,
                    -(lost as i32),
                    Action::BattleLosses,
                    round,
                );
            }
            Event::Diplomacy(DiplomacyEvent::AssaultFailed {
                attacker,
                defender,
                fortress,
            }) if defender == me => {
                let value = if fortress { -60 } else { -20 };
                self.change(world, me, attacker, value, Action::AttackedCity, round);
            }
            Event::Diplomacy(DiplomacyEvent::FortressTaken { winner, loser })
                if me != winner && me != loser =>
            {
                self.conquest(world, me, winner, loser, 20, round);
            }
            Event::Diplomacy(DiplomacyEvent::WizardDestroyed { winner, loser })
                if me != winner && me != loser =>
            {
                self.conquest(world, me, winner, loser, 40, round);
            }
            Event::Diplomacy(DiplomacyEvent::EnchantmentCast { caster, spell }) if caster != me => {
                if let Some((value, action)) = enchantment_reaction(world, spell.num(), me) {
                    self.change(world, me, caster, value, action, round);
                }
            }
            Event::Diplomacy(DiplomacyEvent::CitySpellCast {
                caster,
                owner,
                spell,
            }) if owner == me => {
                if let Some(value) = bad_city_spell(spell.num()) {
                    self.change(world, me, caster, value, Action::BadCitySpell, round);
                }
            }
            Event::Diplomacy(DiplomacyEvent::Subverted { target, .. })
                if me != target && world.have_met(me, target) =>
            {
                // Not through `change`: the original takes the penalty off the relation outright.
                let penalty = world.content().rules.config.diplomacy.subversion_penalty;
                let o = self.slot(world, me, target);
                o.friendship = (o.friendship - penalty).max(-100);
            }
            _ => {}
        }
    }

    /// `winner` took the fortress of `loser` or destroyed it (`base`: 20 or 40 in the original, which
    /// took that much off every bystander's feeling). Here it goes by whom `me` likes: a friend of the
    /// winner who dislikes the loser is pleased, a friend of the loser is angry, a wizard that cares for
    /// neither feels nothing. (The fear of the winner grows either way: `Memory::notice`.)
    fn conquest(
        &mut self,
        world: &World,
        me: PlayerId,
        winner: PlayerId,
        loser: PlayerId,
        base: i32,
        round: u32,
    ) {
        let gap = self.friendship(world, me, loser) - self.friendship(world, me, winner);
        let value = (-base * gap / 100).clamp(-base, base);
        self.change(world, me, winner, value, Action::Threat, round);
    }

    /// A part of a point, in hundredths, that adds up until it is a whole one and then moves the feeling.
    fn drip(
        &mut self,
        world: &World,
        me: PlayerId,
        other: PlayerId,
        hundredths: i32,
        action: Action,
        round: u32,
    ) {
        if hundredths == 0 || !world.have_met(me, other) {
            return;
        }
        let o = self.slot(world, me, other);
        o.carry += hundredths;
        let whole = o.carry / 100;
        if whole != 0 {
            o.carry -= whole * 100;
            self.change(world, me, other, whole, action, round);
        }
    }

    /// What `me` sees each turn in the game that moves its feeling for the others (`Next_Turn_Calc`'s
    /// diplomacy part, task 783): the treaty it keeps, the enchantments in force, an army or an empire
    /// grown too large, units next to its cities, and the drift back to where the feeling started. The
    /// original rolls a die for most of it; here each rule gives its average, with parts of a point added
    /// up from turn to turn (`Opinion::carry`). Called once a turn.
    pub(crate) fn look(&mut self, world: &World, me: PlayerId, round: u32) {
        if self.looked == Some(round) || !world.is_wizard(me) {
            return;
        }
        self.looked = Some(round);
        let turn = round;
        let mine = world.army_strength(me);
        let view = f4x_core::player::view::PlayerView::new(world, me);
        let my_cities = view.my_cities();
        for other in world.wizards() {
            if other == me || !world.have_met(me, other) {
                continue;
            }
            let treaty = world.treaty(me, other);
            // A treaty grows the feeling: `Random(3)` on half the turns for a pact, `Random(6)` for an alliance.
            match treaty {
                Treaty::Pact if turn.is_multiple_of(2) => {
                    self.change(world, me, other, 1, Action::Friendship, round);
                }
                Treaty::Alliance => {
                    let v = 1 + i32::from(turn.is_multiple_of(4));
                    self.change(world, me, other, v, Action::Friendship, round);
                }
                _ => {}
            }
            // The enchantments it keeps going, a tenth of the value of the cast each turn; the Spell of
            // Mastery while it is cast, a half.
            if let Some(w) = world.wizard(other) {
                let mut active: Vec<(u16, i32)> =
                    w.global().iter().map(|s| (s.num(), 10)).collect();
                if let Some(c) = w.casting().filter(|c| c.spell.num() == 213) {
                    active.push((c.spell.num(), 2));
                }
                for (spell, divisor) in active {
                    if let Some((value, action)) = enchantment_reaction(world, spell, me) {
                        self.change(world, me, other, value / divisor, action, round);
                    }
                }
            }
            // An army much stronger than its own: `roll(100) >= strength * 50 / its own`, then one in twenty, -10.
            let theirs = world.army_strength(other);
            if mine > 0 && theirs > mine {
                let threshold = theirs * 50 / mine;
                if threshold <= 100 {
                    self.drip(
                        world,
                        me,
                        other,
                        -(101 - threshold) / 2,
                        Action::Threat,
                        round,
                    );
                }
            }
            // Too many cities, looked at every second turn: one in four, the surplus over the allowance.
            let cities = world.city_count(other) as i32;
            let allowance = world.city_allowance();
            if turn.is_multiple_of(2) && allowance < cities && treaty != Treaty::Alliance {
                let divisor = (world.land_size() + 6 - i32::from(world.difficulty())).max(1);
                let value = (-(cities - allowance) / divisor * 2).min(-15);
                self.drip(world, me, other, value * 25, Action::Overextension, round);
            }
            // Units next to its cities (`DIPL_GetOffMyLawn`).
            self.watch_lawn(world, &view, &my_cities, me, other, round);
            // Drift back to where the feeling started, unless the other has just changed it.
            let o = self.opinion(world, me, other);
            if o.moved.saturating_add(1) < turn {
                // `Random(105) > |feeling|` then `Random(2)`: half a point at most each turn.
                let want = (105 - o.friendship.abs()).max(0) * 50 / 105;
                if o.friendship < o.default {
                    self.drift(world, me, other, want, o.default);
                } else if turn.is_multiple_of(10) && o.friendship > o.default {
                    self.drift(world, me, other, want * 10, o.default);
                }
            }
        }
    }

    /// Move the feeling towards `to` by `hundredths` (a whole point at a time, without passing it).
    fn drift(&mut self, world: &World, me: PlayerId, other: PlayerId, hundredths: i32, to: i32) {
        let o = self.slot(world, me, other);
        o.carry += if o.friendship < to {
            hundredths
        } else {
            -hundredths
        };
        let whole = o.carry / 100;
        if whole != 0 {
            o.carry -= whole * 100;
            o.friendship = if o.friendship < to {
                (o.friendship + whole).min(to)
            } else {
                (o.friendship + whole).max(to)
            };
        }
    }

    /// Non-settler units of `other` within two squares of one of `me`'s cities (that `me` sees).
    fn watch_lawn(
        &mut self,
        world: &World,
        view: &f4x_core::player::view::PlayerView,
        cities: &[(hecs::Entity, f4x_core::map::Position)],
        me: PlayerId,
        other: PlayerId,
        round: u32,
    ) {
        let treaty = world.treaty(me, other);
        let trespassing = treaty != Treaty::War
            && cities.iter().any(|&(_, city)| {
                (-2..=2).any(|dy| {
                    (-2..=2).any(|dx| {
                        view.shift(city, dx, dy)
                            .and_then(|square| view.hostile_at(square))
                            .is_some_and(|units| {
                                units.into_iter().any(|u| {
                                    world.units().unit_owner(u) == Some(other)
                                        && !world.units().unit_is_settler(u)
                                })
                            })
                    })
                })
            });
        if !trespassing {
            if let Some(o) = self.opinions.iter_mut().find(|o| o.with == other) {
                o.trespass = 0;
            }
            return;
        }
        if matches!(treaty, Treaty::Pact | Treaty::Alliance) {
            // A warning (`Random(5)`: 2), a turn of grace, then 5 (the original also breaks the treaty).
            let step = self.slot(world, me, other).trespass + 1;
            self.slot(world, me, other).trespass = step;
            match step {
                1 => self.change(world, me, other, -2, Action::Trespass, round),
                2 => {}
                _ => {
                    self.change(world, me, other, -5, Action::Trespass, round);
                    self.slot(world, me, other).trespass = 0;
                }
            }
        } else {
            // A turn in sixteen costs `Random(5)`: 2 / 16.
            self.drip(world, me, other, -12, Action::Trespass, round);
        }
    }

    /// A war with `other` began: the feeling falls to the middle of the original's `-75 - Random(25)`.
    pub(crate) fn war_began(&mut self, world: &World, me: PlayerId, other: PlayerId) {
        let o = self.slot(world, me, other);
        o.friendship = -87;
        o.default = (o.default - 5).max(-100);
        o.war = true;
    }

    /// The war with `other` ended in peace: the feeling is 40 better.
    pub(crate) fn war_ended(&mut self, world: &World, me: PlayerId, other: PlayerId) {
        let o = self.slot(world, me, other);
        if o.war {
            o.friendship = (o.friendship + 40).min(100);
            o.war = false;
        }
    }
}

/// What the overland enchantment `spell` (by number) does to `me`'s feeling for its caster
/// (`Change_Relations_For_Enchantments`): some are disliked only by wizards of certain books.
fn enchantment_reaction(world: &World, spell: u16, me: PlayerId) -> Option<(i32, Action)> {
    let books = |realm: u16| {
        world
            .wizard(me)
            .map_or(0, |m| m.books_in(RealmId::new(realm)))
    };
    let (nature, chaos, life, death) = (0, 2, 3, 4);
    Some(match spell {
        213 /* spell_of_mastery */ => (-50, Action::SpellOfMastery),
        197 /* eternal_night */ => (-12, Action::Enchantment),
        198 /* evil_omens */ if books(life) > 0 || books(nature) > 0 => (-20, Action::Enchantment),
        188 /* zombie_mastery */ => (-14, Action::Enchantment),
        59 /* aura_of_majesty */ => (10, Action::AuraOfMajesty),
        57 /* wind_mastery */ | 79 /* suppress_magic */ => (-4, Action::Enchantment),
        40 /* natures_wrath */ if books(chaos) > 0 || books(death) > 0 => (-20, Action::Enchantment),
        116 /* chaos_surge */ if books(chaos) == 0 => (-10, Action::Enchantment),
        117 /* doom_mastery */ => (-8, Action::Enchantment),
        114 /* great_wasting */ => (-20, Action::Enchantment),
        113 /* meteor_storm */ => (-15, Action::Enchantment),
        120 /* armageddon */ => (-25, Action::Enchantment),
        157 /* tranquility */ if books(chaos) > 0 => (-20, Action::Enchantment),
        156 /* life_force */ if books(death) > 0 => (-20, Action::Enchantment),
        158 /* crusade */ => (-10, Action::Enchantment),
        154 /* holy_arms */ => (-5, Action::Enchantment),
        _ => return None,
    })
}

/// What a curse on a city costs its owner's feeling for the caster (`Change_Relations_For_Bad_City_Spell`).
fn bad_city_spell(spell: u16) -> Option<i32> {
    Some(match spell {
        26 /* earthquake */ => -9,
        85 /* corruption */ => -25,
        98 /* raise_volcano */ => -50,
        110 /* chaos_rift */ => -5,
        119 /* call_the_void */ => -20,
        183 /* evil_presence */ => -5,
        189 /* famine */ => -5,
        190 /* cursed_lands */ => -5,
        196 /* pestilence */ => -8,
        _ => return None,
    })
}

/// The first opinion of `other`: what the two start with, from their books.
fn first_opinion(world: &World, me: PlayerId, other: PlayerId) -> Opinion {
    let start = start_feeling(world, me, other);
    Opinion {
        with: other,
        friendship: start,
        default: start,
        hidden: 0,
        broken: false,
        war: world.treaty(me, other) == Treaty::War,
        carry: 0,
        trespass: 0,
        moved: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplo_b_stance::tests::pair;
    use crate::testworld::{P0, P1};
    use f4x_core::world::Command;
    use f4x_master::Game;

    fn mind(w: &World, friendship: i32) -> Memory {
        let mut m = Memory::default();
        m.set_friendship(w, P0, P1, friendship);
        m
    }

    #[test]
    fn a_first_opinion_is_the_start_from_the_books() {
        let w = pair(8, 8, 100);
        let m = Memory::default();
        let o = m.opinion(&w, P0, P1);
        assert_eq!(
            (o.friendship, o.default),
            (start_feeling(&w, P0, P1), start_feeling(&w, P0, P1))
        );
        assert_eq!((o.hidden, o.broken, o.war), (0, false, false));
    }

    #[test]
    fn a_good_deed_counts_for_more_when_it_is_needed_and_a_bad_one_when_it_is_not() {
        let w = pair(8, 8, 100);
        let mut low = mind(&w, -40);
        low.change(&w, P0, P1, 5, Action::Friendship, w.round());
        assert!(
            low.friendship(&w, P0, P1) >= -40 + 10,
            "doubled when it is negative"
        );
        let mut high = mind(&w, 60);
        high.change(&w, P0, P1, -5, Action::AttackedCity, w.round());
        assert!(
            high.friendship(&w, P0, P1) <= 60 - 10,
            "doubled when it is positive"
        );
        let mut warm = mind(&w, 60);
        warm.change(&w, P0, P1, 10, Action::Friendship, w.round());
        assert!(
            warm.friendship(&w, P0, P1) < 70,
            "less when it is already good"
        );
    }

    #[test]
    fn the_feeling_is_capped_and_a_war_is_not_made_worse() {
        let mut w = pair(8, 8, 100);
        let mut m = mind(&w, 60);
        m.change(&w, P0, P1, 100, Action::Friendship, w.round());
        assert!(m.friendship(&w, P0, P1) <= 65, "no treaty: at most 65");
        w.declare_war(P1, P0);
        let mut m = mind(&w, -50);
        m.change(&w, P0, P1, -20, Action::AttackedCity, w.round());
        assert_eq!(
            m.friendship(&w, P0, P1),
            -50,
            "a war is not made worse by what is done in it"
        );
        m.change(&w, P0, P1, 30, Action::Friendship, w.round());
        assert!(
            m.friendship(&w, P0, P1) <= -25,
            "in a war it stays at -25 or below"
        );
    }

    #[test]
    fn a_city_taken_costs_the_loser_forty_and_the_taker_nothing() {
        let w = pair(8, 8, 100);
        let city = w.economy().cities().into_iter().next().expect("a city").0;
        let event = Event::CityCaptured {
            city,
            from: P0,
            to: P1,
        };
        let mut loser = mind(&w, 0);
        loser.feel(&w, P0, &event, w.round());
        assert!(
            loser.friendship(&w, P0, P1) <= -40,
            "{}",
            loser.friendship(&w, P0, P1)
        );
        let mut taker = mind(&w, 0);
        taker.feel(&w, P1, &event, w.round());
        assert_eq!(taker.friendship(&w, P0, P1), 0);
    }

    #[test]
    fn a_war_sets_the_feeling_low_and_peace_lifts_it_by_forty() {
        let mut w = pair(8, 8, 100);
        let mut m = mind(&w, 20);
        let events = w.apply(Command::Diplomacy(
            f4x_core::diplomacy::DiplomacyCommand::DeclareWar { player: P1, on: P0 },
        ));
        for e in &events {
            m.feel(&w, P0, e, w.round());
        }
        assert_eq!(m.friendship(&w, P0, P1), -87);
        assert!(m.opinion(&w, P0, P1).war);
        let peace = Event::Diplomacy(DiplomacyEvent::TreatyChanged {
            a: P0,
            b: P1,
            treaty: Treaty::None,
        });
        m.feel(&w, P0, &peace, w.round());
        assert_eq!(m.friendship(&w, P0, P1), -47);
        m.feel(&w, P0, &peace, w.round());
        assert_eq!(
            m.friendship(&w, P0, P1),
            -47,
            "a second peace changes nothing"
        );
    }

    #[test]
    fn an_oath_broken_is_held_against_the_breaker_by_the_victim_more_than_by_the_rest() {
        let w = pair(8, 8, 100);
        let broke = Event::Diplomacy(DiplomacyEvent::TreatyBroken {
            breaker: P1,
            victim: P0,
            treaty: Treaty::Alliance,
        });
        let mut victim = mind(&w, 30);
        victim.feel(&w, P0, &broke, w.round());
        let o = victim.opinion(&w, P0, P1);
        assert!(o.broken && o.hidden <= -20 && o.friendship < 30, "{o:?}");
        let mut other = Memory::default();
        other.feel(&w, PlayerId(2), &broke, w.round());
        assert_eq!(other.opinion(&w, PlayerId(2), P1).hidden, -5);
        let mut breaker = mind(&w, 30);
        breaker.feel(&w, P1, &broke, w.round());
        assert_eq!(
            breaker.friendship(&w, P0, P1),
            30,
            "the breaker does not feel it"
        );
    }

    #[test]
    fn gifts_warm_the_receiver_and_a_spell_leaves_a_lasting_regard() {
        let w = pair(8, 8, 100);
        let spell = f4x_core::rules::SpellId::new(w.content().rules.spells.defs()[0].num);
        let mut m = mind(&w, 0);
        m.feel(
            &w,
            P0,
            &Event::Diplomacy(DiplomacyEvent::SpellGiven {
                from: P1,
                to: P0,
                spell,
            }),
            w.round(),
        );
        let o = m.opinion(&w, P0, P1);
        assert!(o.friendship > 0 && o.hidden == 5, "{o:?}");
        let mut other = mind(&w, 0);
        other.feel(
            &w,
            P1,
            &Event::Diplomacy(DiplomacyEvent::SpellGiven {
                from: P1,
                to: P0,
                spell,
            }),
            w.round(),
        );
        assert_eq!(
            other.friendship(&w, P0, P1),
            0,
            "only the receiver is moved"
        );
    }

    #[test]
    fn a_refused_demand_sours_the_demander() {
        let w = pair(8, 8, 100);
        let demand = |accepted| {
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                proposal: f4x_core::diplomacy::Proposal {
                    from: P0,
                    to: P1,
                    kind: ProposalKind::Demand { gold: 10 },
                    round: 0,
                },
                accepted,
                lapsed: false,
            })
        };
        let mut m = mind(&w, 0);
        m.feel(&w, P0, &demand(true), w.round());
        assert_eq!(m.friendship(&w, P0, P1), 0);
        m.feel(&w, P0, &demand(false), w.round());
        assert!(m.friendship(&w, P0, P1) < 0);
    }

    fn felt(w: &Game, event: DiplomacyEvent) -> i32 {
        let mut m = mind(w, 0);
        m.feel(w, P0, &Event::Diplomacy(event), w.round());
        m.friendship(w, P0, P1)
    }

    #[test]
    fn what_a_battle_cost_is_held_against_the_attacker_by_the_defender_only() {
        let w = pair(8, 8, 100);
        let lost = |defender| DiplomacyEvent::BattleLosses {
            attacker: P1,
            defender,
            heroes: 1,
            others: 2,
            settlers: 0,
        };
        assert_eq!(felt(&w, lost(P0)), -(20 + 19), "a hero and two units");
        assert_eq!(
            felt(&w, lost(P1)),
            0,
            "the attacker does not hold it against itself"
        );
    }

    #[test]
    fn a_failed_assault_costs_more_when_it_was_the_fortress() {
        let w = pair(8, 8, 100);
        let assault = |fortress| DiplomacyEvent::AssaultFailed {
            attacker: P1,
            defender: P0,
            fortress,
        };
        assert_eq!(felt(&w, assault(false)), -20);
        assert_eq!(felt(&w, assault(true)), -60);
    }

    #[test]
    fn a_fortress_taken_or_a_wizard_destroyed_is_judged_by_whom_one_likes() {
        let w = pair(8, 8, 100);
        let third = PlayerId(7);
        let judged = |event: DiplomacyEvent, winner: i32, loser: i32| {
            let mut m = mind(&w, winner);
            m.set_friendship(&w, P0, third, loser);
            m.feel(&w, P0, &Event::Diplomacy(event), w.round());
            m.friendship(&w, P0, P1) - winner
        };
        let taken = DiplomacyEvent::FortressTaken {
            winner: P1,
            loser: third,
        };
        let destroyed = DiplomacyEvent::WizardDestroyed {
            winner: P1,
            loser: third,
        };
        assert_eq!(judged(taken.clone(), 0, 0), 0, "it cares for neither");
        assert!(
            judged(taken.clone(), 50, -50) > 0,
            "a friend rids it of an enemy"
        );
        assert!(
            judged(taken.clone(), -50, 50) < 0,
            "an enemy strikes a friend"
        );
        assert!(
            judged(destroyed.clone(), 50, -50) > judged(taken.clone(), 50, -50),
            "destroying counts for more than a fortress"
        );
        let own = DiplomacyEvent::WizardDestroyed {
            winner: P1,
            loser: P0,
        };
        assert_eq!(
            judged(own, 0, 0),
            0,
            "the loser has no feelings left to change here"
        );
    }

    #[test]
    fn a_conquest_makes_the_winner_feared_whatever_one_thinks_of_it() {
        let w = pair(8, 8, 100);
        let k = crate::diplo_b::Knobs::current();
        let third = PlayerId(7);
        for (event, points) in [
            (
                DiplomacyEvent::FortressTaken {
                    winner: P1,
                    loser: third,
                },
                k.ruth_fortress,
            ),
            (
                DiplomacyEvent::WizardDestroyed {
                    winner: P1,
                    loser: third,
                },
                k.ruth_destroyed,
            ),
        ] {
            let mut m = mind(&w, 50);
            m.notice(P0, &w, w.round(), &[Event::Diplomacy(event)]);
            assert_eq!(m.ruthlessness(P1, w.round(), k.ruth_half), points);
        }
    }

    #[test]
    fn a_curse_on_a_city_and_a_hated_enchantment_are_held_against_the_caster() {
        let w = pair(8, 8, 100);
        let spell = |n| f4x_core::rules::SpellId::new(n);
        let curse = |n| DiplomacyEvent::CitySpellCast {
            caster: P1,
            owner: P0,
            spell: spell(n),
        };
        assert_eq!(felt(&w, curse(85)), -25, "corruption");
        assert_eq!(felt(&w, curse(1)), 0, "a spell that is no curse");
        let cast = |n| DiplomacyEvent::EnchantmentCast {
            caster: P1,
            spell: spell(n),
        };
        assert_eq!(felt(&w, cast(213)), -50, "the Spell of Mastery");
        assert_eq!(felt(&w, cast(120)), -25, "armageddon");
        assert!(felt(&w, cast(59)) > 0, "the Aura of Majesty pleases");
        assert_eq!(felt(&w, cast(1)), 0, "an enchantment nobody minds");
    }

    #[test]
    fn subversion_costs_the_target_the_set_penalty_with_everyone_who_met_it() {
        let w = pair(8, 8, 100);
        let penalty = w.content().rules.config.diplomacy.subversion_penalty;
        let by = |target| DiplomacyEvent::Subverted { caster: P1, target };
        // P0 has met P1 in the pair world: subverting P1 (by P1 itself or any caster) costs P1.
        assert_eq!(felt(&w, by(P1)), -penalty);
        assert_eq!(
            felt(&w, by(P0)),
            0,
            "its own target does not change its feeling"
        );
    }

    /// P0's feeling for P1 over `turns` looks, from `start` (a fresh turn each time).
    fn looks(w: &mut Game, m: &mut Memory, turns: u32) {
        for _ in 0..turns {
            w.set_round(w.round() + 1);
            m.look(w, P0, w.round());
        }
    }

    #[test]
    fn a_treaty_in_force_grows_the_feeling_and_an_alliance_faster_than_a_pact() {
        let grown = |treaty| {
            let mut w = pair(8, 8, 100);
            w.start_treaty(P0, P1, treaty);
            let mut m = mind(&w, 0);
            looks(&mut w, &mut m, 8);
            m.friendship(&w, P0, P1)
        };
        let (pact, alliance) = (grown(Treaty::Pact), grown(Treaty::Alliance));
        assert!(pact > 0, "{pact}");
        assert!(alliance > pact, "{alliance} against {pact}");
        let mut w = pair(8, 8, 100);
        let mut m = mind(&w, 0);
        m.slot(&w, P0, P1).default = 0;
        looks(&mut w, &mut m, 8);
        assert_eq!(m.friendship(&w, P0, P1), 0, "no treaty, no growth");
    }

    #[test]
    fn an_enchantment_kept_up_sours_the_feeling_every_turn_and_one_look_a_turn_counts() {
        let mut w = pair(8, 8, 100);
        w.scenario_add_global(P1, f4x_core::rules::SpellId::new(120 /* armageddon */));
        let mut m = mind(&w, 0);
        looks(&mut w, &mut m, 3);
        assert!(
            m.friendship(&w, P0, P1) <= -4,
            "{}",
            m.friendship(&w, P0, P1)
        );
        // A second look in the same turn changes nothing.
        let before = m.friendship(&w, P0, P1);
        m.look(&w, P0, w.round());
        assert_eq!(m.friendship(&w, P0, P1), before);
    }

    #[test]
    fn the_feeling_drifts_back_to_where_it_started_up_quickly_and_down_slowly() {
        let mut w = pair(8, 8, 100);
        let mut low = mind(&w, -30);
        low.slot(&w, P0, P1).default = 0;
        looks(&mut w, &mut low, 20);
        assert!(low.friendship(&w, P0, P1) > -30, "it climbs back");
        let mut w = pair(8, 8, 100);
        let mut high = mind(&w, 30);
        high.slot(&w, P0, P1).default = 0;
        looks(&mut w, &mut high, 9);
        assert_eq!(
            high.friendship(&w, P0, P1),
            30,
            "it comes down only every tenth turn"
        );
        looks(&mut w, &mut high, 12);
        assert!(high.friendship(&w, P0, P1) < 30);
    }

    #[test]
    fn units_next_to_its_city_cost_a_partner_a_warning_a_grace_and_a_fine() {
        let mut w = pair(8, 8, 100);
        w.start_treaty(P0, P1, Treaty::Pact);
        crate::testworld::spawn(&mut w, P1, crate::testworld::at(11, 10), 95, 2);
        w.explore(
            P0,
            crate::testworld::at(11, 10),
            f4x_def::half_int::HalfInt::from_raw(3),
        );
        let mut m = mind(&w, 0);
        let step = |w: &mut Game, m: &mut Memory| {
            w.set_round(w.round() + 1);
            m.look(w, P0, w.round());
            (m.opinion(w, P0, P1).trespass, m.friendship(w, P0, P1))
        };
        let (n1, f1) = step(&mut w, &mut m);
        assert_eq!(n1, 1, "the first turn is a warning");
        let first = f1;
        assert!(f1 <= 0);
        let (_, f2) = step(&mut w, &mut m);
        assert!(f2 >= first, "a turn of grace (the pact's growth may add)");
        let (n3, f3) = step(&mut w, &mut m);
        assert_eq!(n3, 0, "the third turn is the fine");
        assert!(f3 < f2, "{f3} against {f2}");
    }

    #[test]
    fn an_army_a_little_stronger_than_its_own_is_disliked_and_a_weaker_one_is_not() {
        let after = |mine, theirs| {
            let mut w = pair(mine, theirs, 101);
            let mut m = mind(&w, 0);
            m.slot(&w, P0, P1).default = -100;
            looks(&mut w, &mut m, 9);
            m.friendship(&w, P0, P1)
        };
        // The drift towards the default (-100) is the same for both: only the army differs.
        assert!(
            after(8, 12) < after(8, 8),
            "{} against {}",
            after(8, 12),
            after(8, 8)
        );
        assert_eq!(after(12, 8), after(8, 8));
    }
}
