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
use f4x_core::spell::global_enchantment::GlobalEffect;
use f4x_core::spell::special::SPELL_OF_MASTERY;
use f4x_core::wizard::magic::Casting;
use f4x_core::wizard::objective::Personality;
use f4x_core::world::{Event, World};
use f4x_def::rules::ai_strategy::OpinionLevers;
use serde::{Deserialize, Serialize};

use crate::lever::Levered;
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
    /// What a treaty has grown the feeling by and not yet moved it, in hundredths of a point.
    #[serde(default)]
    pub growth: i32,
    /// The last turn the other changed this feeling by what it did (the drift waits for a turn).
    #[serde(default)]
    pub moved: u32,
}

/// How a book of realm `x` stands to a book of realm `y` on the colour wheel (kin, next to it, opposed).
fn wheel_affinity(x: RealmId, y: RealmId, k: &OpinionLevers) -> i32 {
    // The realms' numbers in wheel order: chaos, nature, life, sorcery, death.
    const WHEEL: [u16; 5] = [2, 0, 3, 1, 4];
    let place = |r: RealmId| WHEEL.iter().position(|&n| n == r.num());
    let (Some(i), Some(j)) = (place(x), place(y)) else {
        return 0;
    };
    match (i + 5 - j) % 5 {
        0 => k.wheel_kin,
        1 | 4 => k.wheel_next,
        _ => -k.wheel_opposed,
    }
}

/// The feeling two wizards start with, from their books (`Init_Diplomatic_Relations`), or from the
/// colour wheel when the setting `color_wheel_relations` is on.
pub(crate) fn start_feeling(world: &World, a: PlayerId, b: PlayerId) -> i32 {
    let books = |p: PlayerId, r: RealmId| -> i32 { world.wizard(p).map_or(0, |m| m.books_in(r)) };
    let k = &world.levers().opinion;
    if world.settings().rules.color_wheel_relations {
        let realms = [2u16, 0, 3, 1, 4].map(RealmId::new);
        let score: i32 = realms
            .iter()
            .flat_map(|&x| realms.iter().map(move |&y| (x, y)))
            .map(|(x, y)| books(a, x).min(books(b, y)) * wheel_affinity(x, y, k))
            .sum();
        return score.max(-k.start_floor);
    }
    let (death, life, chaos) = (RealmId::new(4), RealmId::new(3), RealmId::new(2));
    let max_death = books(a, death).max(books(b, death));
    let sum_life = books(a, life) + books(b, life);
    let max_chaos = books(a, chaos).max(books(b, chaos));
    let mut score = if sum_life > 0 && max_death > 0 {
        -(sum_life + max_death) * k.life_death_clash
    } else {
        sum_life * k.life_book - max_death * k.death_book
    };
    score -= max_chaos * k.chaos_book;
    for realm in [RealmId::new(1), chaos, RealmId::new(0), life] {
        score += books(a, realm).min(books(b, realm)) * k.shared_book;
    }
    score.max(-k.start_floor)
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
        let v = self.weighed(world, me, from, value, 1);
        let k = &world.levers().opinion;
        let treaty = world.treaty(me, from);
        let o = self.slot(world, me, from);
        if action != Action::AuraOfMajesty {
            o.moved = round;
        }
        // Wars are not made worse by what one does in them.
        if !(treaty == Treaty::War && action != Action::Trespass && v < 0) {
            o.friendship = (o.friendship + v).clamp(-100, 100);
        }
        if treaty == Treaty::War {
            o.friendship = o.friendship.min(-k.war_hostility);
        }
        if treaty != Treaty::Alliance {
            o.friendship = o.friendship.min(k.no_alliance_most);
        }
    }

    /// `value`, counted in `point`ths of a point, as `me` takes it from `from`: diminishing
    /// returns, its temper and the other's gift for diplomacy.
    fn weighed(&self, world: &World, me: PlayerId, from: PlayerId, value: i32, point: i32) -> i32 {
        let rel = self.friendship(world, me, from);
        let k = &world.levers().opinion;
        let mut v = value;
        if v > 0 {
            if rel < 0 {
                v *= k.contrast_factor;
                v = v.min((k.good_low_ceiling - rel) * point);
            } else {
                v /= rel / k.diminish_step + 1;
            }
        } else if rel > 0 {
            v *= k.contrast_factor;
        } else {
            v /= -rel / k.diminish_step + 1;
        }
        match world.personality(me) {
            Personality::Lawful if v < 0 => v = v * k.lawful_bad_pct / 100,
            Personality::Maniacal if v < 0 => v = v * k.maniacal_bad_pct / 100,
            Personality::Maniacal if v > 0 => v = v * k.maniacal_good_pct / 100,
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
        v
    }

    /// A pact or an alliance warms the feeling by `hundredths` of a point a turn, with the same
    /// diminishing returns as anything else that pleases. The parts of a point add up, so that it
    /// goes on growing when a turn's share is less than one.
    fn grow(&mut self, world: &World, me: PlayerId, other: PlayerId, hundredths: i32) {
        if hundredths <= 0 || !world.have_met(me, other) {
            return;
        }
        let v = self.weighed(world, me, other, hundredths, 100);
        let alliance = world.treaty(me, other) == Treaty::Alliance;
        let o = self.slot(world, me, other);
        o.growth += v;
        let whole = o.growth / 100;
        o.growth -= whole * 100;
        o.friendship = (o.friendship + whole).clamp(-100, 100);
        if !alliance {
            o.friendship = o.friendship.min(world.levers().opinion.no_alliance_most);
        }
    }

    /// What an event does to `me`'s feelings (task 780): the facts the core reports, which the AI weighs by
    /// the table of `docs/diplomacy.md` section 5. (The rest of the table comes with 782 and 783.)
    pub(crate) fn feel(&mut self, world: &World, me: PlayerId, event: &Event, round: u32) {
        let k = &world.levers().opinion;
        match *event {
            Event::CityCaptured { from, to, .. } if from == me => {
                self.change(world, me, to, -k.city_taken, Action::AttackedCity, round);
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
            // A partner's units next to a city of `me`: the first warning costs a little, the
            // turn after is given in grace, the last one costs more (`DIPL_GetOffMyLawn`: 2, 5).
            Event::Diplomacy(DiplomacyEvent::Trespassing {
                owner,
                intruder,
                warning,
            }) if owner == me => {
                let last = world.content().rules.config.diplomacy.trespass_warnings;
                let value = match warning {
                    1 => -k.trespass_first,
                    w if w >= last => -k.trespass_last,
                    _ => 0,
                };
                self.change(world, me, intruder, value, Action::Trespass, round);
            }
            Event::Diplomacy(DiplomacyEvent::TreatyBroken {
                breaker,
                victim,
                treaty,
            }) if me != breaker => {
                let mut penalty = match treaty {
                    Treaty::Pact => k.broken_pact,
                    Treaty::Alliance => k.broken_alliance,
                    _ => return,
                };
                if world.personality(breaker) == Personality::Lawful {
                    penalty *= k.lawful_breaker_times;
                }
                if me == victim {
                    let o = self.slot(world, me, breaker);
                    o.hidden = (o.hidden - penalty).max(-100);
                    // The original takes `Random(20)` off the relation: its average.
                    o.friendship = (o.friendship - k.broken_victim).max(-100);
                    if treaty == Treaty::Alliance {
                        o.default = (o.default - penalty).max(-100);
                    }
                    o.broken = true;
                } else {
                    // Every other wizard thinks less of the oath-breaker.
                    let o = self.slot(world, me, breaker);
                    o.hidden = (o.hidden - k.broken_bystander).max(-100);
                }
            }
            Event::Diplomacy(DiplomacyEvent::GoldGiven { from, to, gold }) if to == me => {
                // A larger share of what it holds, the warmer the thanks; the original rolls
                // `Random(3)` (0..2) over ten: one tenth, rounded.
                let reserve = world
                    .player(me)
                    .map_or(0, |p| p.treasury().gold.floor() - gold)
                    .max(1);
                let choice = ((gold * k.gift_steps + reserve - 1) / reserve).clamp(1, k.gift_steps);
                let base = if world.army_strength(me) > 0 {
                    choice * k.gift_per_step
                } else {
                    k.gift_no_army
                };
                let o = self.slot(world, me, from);
                let thanks = (base + k.gift_divisor / 2) / k.gift_divisor;
                o.friendship = (o.friendship + thanks).min(100);
            }
            Event::Diplomacy(DiplomacyEvent::SpellGiven { from, to, .. }) if to == me => {
                let rel = self.friendship(world, me, from);
                let base = if rel < 0 {
                    k.spell_low
                } else {
                    (100 - rel) / k.spell_divisor
                };
                // `base * (Random(8) + Random(8)) / 10`, the two rolls at their average of 3.5; and
                // the hidden regard `2 + Random(8)`.
                let o = self.slot(world, me, from);
                o.friendship = (o.friendship + base * k.spell_pct / 100).min(100);
                o.hidden = (o.hidden + k.spell_regard).min(k.spell_regard_most);
            }
            // An ally that asks to be paid for joining a war is thought less of.
            Event::Diplomacy(DiplomacyEvent::ProposalCountered { new, .. })
                if new.to == me && matches!(new.kind, ProposalKind::WarPrice { .. }) =>
            {
                let penalty = world.levers().diplomacy_b.call_price_penalty;
                self.change(world, me, new.from, -penalty, Action::Threat, round);
            }
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                proposal,
                accepted,
                lapsed: false,
            }) => match proposal.kind {
                // Turning a demand down sours the one who made it (`Random(10) + 1`).
                ProposalKind::Demand { .. } if !accepted && proposal.from == me => {
                    self.change(
                        world,
                        me,
                        proposal.to,
                        -k.demand_refused,
                        Action::Threat,
                        round,
                    );
                }
                // A threat costs the threatener the goodwill of the one threatened (`Random(15)`).
                ProposalKind::Threat if proposal.to == me => {
                    self.change(
                        world,
                        me,
                        proposal.from,
                        -k.threatened,
                        Action::Threat,
                        round,
                    );
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
                    self.change(world, me, other, k.trade_agreed, Action::Friendship, round);
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
                let lost = k.loss_hero * heroes
                    + (k.loss_unit_halves * others + k.loss_settler_halves * settlers) / 2;
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
                let value = if fortress {
                    -k.assault_fortress
                } else {
                    -k.assault_city
                };
                self.change(world, me, attacker, value, Action::AttackedCity, round);
            }
            Event::Diplomacy(DiplomacyEvent::FortressTaken { winner, loser })
                if me != winner && me != loser =>
            {
                self.conquest(world, me, winner, loser, k.fortress_taken, round);
            }
            Event::Diplomacy(DiplomacyEvent::WizardDestroyed { winner, loser })
                if me != winner && me != loser =>
            {
                self.conquest(world, me, winner, loser, k.wizard_destroyed, round);
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
                if let Some(value) = bad_city_spell(world, spell.num()) {
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

    /// `winner` took the fortress of `loser` or destroyed it (`base`: what the original took off every
    /// bystander's feeling, `fortress_taken` and `wizard_destroyed`). Here it goes by whom `me` likes: a friend of the
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
        let treaties = world.levers().diplomacy_b;
        let k = &world.levers().opinion;
        let intruders = world.trespassers(me);
        for other in world.wizards() {
            if other == me || !world.have_met(me, other) {
                continue;
            }
            let treaty = world.treaty(me, other);
            // A treaty grows the feeling: `Random(3)` on half the turns for a pact, `Random(6)` for an
            // alliance, here their averages.
            let growth = match treaty {
                Treaty::Pact => treaties.pact_growth,
                Treaty::Alliance => treaties.alliance_growth,
                _ => 0,
            };
            self.grow(world, me, other, growth);
            // The enchantments it keeps going, a tenth of the value of the cast each turn; the Spell of
            // Mastery while it is cast, a half.
            if let Some(w) = world.wizard(other) {
                let mut active: Vec<(u16, i32)> = w
                    .global()
                    .iter()
                    .map(|s| (s.num(), k.ongoing_divisor))
                    .collect();
                let mastery = |c: &Casting| world.spell_has_hook(c.spell, SPELL_OF_MASTERY);
                if let Some(c) = w.casting().filter(mastery) {
                    active.push((c.spell.num(), k.mastery_ongoing_divisor));
                }
                for (spell, divisor) in active {
                    if let Some((value, action)) = enchantment_reaction(world, spell, me) {
                        self.change(world, me, other, value / divisor, action, round);
                    }
                }
            }
            // An army much stronger than its own: `roll(100) >= strength * army_threshold_pct / its own`,
            // then one in twenty, -10, here the average.
            let theirs = world.army_strength(other);
            if mine > 0 && theirs > mine {
                let threshold = theirs * k.army_threshold_pct / mine;
                if threshold <= 100 {
                    self.drip(
                        world,
                        me,
                        other,
                        -(101 - threshold) / k.army_fear_divisor,
                        Action::Threat,
                        round,
                    );
                }
            }
            // Too many cities, looked at every few turns: the surplus over the allowance.
            let cities = world.city_count(other) as i32;
            let allowance = world.city_allowance();
            if turn.is_multiple_of(k.overcrowd_every)
                && allowance < cities
                && treaty != Treaty::Alliance
            {
                let divisor =
                    (world.land_size() + k.overcrowd_base - i32::from(world.difficulty())).max(1);
                let value =
                    (-(cities - allowance) / divisor * k.overcrowd_per).min(-k.overcrowd_least);
                let hundredths = value * k.overcrowd_hundredths;
                self.drip(world, me, other, hundredths, Action::Overextension, round);
            }
            // Units next to its cities without a treaty, where there are no warnings
            // (`DIPL_GetOffMyLawn`): a turn in sixteen costs `Random(5)`, here the average.
            if treaty == Treaty::None && intruders.contains(&other) {
                let hundredths = -k.intruder_hundredths;
                self.drip(world, me, other, hundredths, Action::Trespass, round);
            }
            // Drift back to where the feeling started, unless the other has just changed it.
            let o = self.opinion(world, me, other);
            if o.moved.saturating_add(k.drift_wait) < turn {
                // `Random(drift_range) > |feeling|` then `Random(2)`, at its average.
                let want =
                    (k.drift_range - o.friendship.abs()).max(0) * k.drift_most / k.drift_range;
                if o.friendship < o.default {
                    self.drift(world, me, other, want, o.default);
                } else if turn.is_multiple_of(k.drift_down_every) && o.friendship > o.default {
                    let far = want * k.drift_down_every as i32;
                    self.drift(world, me, other, far, o.default);
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

    /// A war with `other` began: the feeling falls to `war_start` below 0 (the middle of the
    /// original's `-75 - Random(25)`).
    pub(crate) fn war_began(&mut self, world: &World, me: PlayerId, other: PlayerId) {
        let k = &world.levers().opinion;
        let o = self.slot(world, me, other);
        o.friendship = -k.war_start;
        o.default = (o.default - k.war_default_penalty).max(-100);
        o.war = true;
    }

    /// The war with `other` ended in peace: the feeling is `peace_gain` better.
    pub(crate) fn war_ended(&mut self, world: &World, me: PlayerId, other: PlayerId) {
        let o = self.slot(world, me, other);
        if o.war {
            o.friendship = (o.friendship + world.levers().opinion.peace_gain).min(100);
            o.war = false;
        }
    }
}

/// What the overland enchantment `spell` (by number) does to `me`'s feeling for its caster
/// (`Change_Relations_For_Enchantments`; the table is `opinion.enchantments`): some are disliked
/// only by wizards of certain books.
fn enchantment_reaction(world: &World, spell: u16, me: PlayerId) -> Option<(i32, Action)> {
    let has_books = |realms: &[u16]| {
        realms.iter().any(|&r| {
            world
                .wizard(me)
                .is_some_and(|m| m.books_in(RealmId::new(r)) > 0)
        })
    };
    let found = world.levers().opinion.enchantments.iter().find(|e| {
        e.spell == spell
            && (e.with_books.is_empty() || has_books(&e.with_books))
            && !has_books(&e.without_books)
    })?;
    let def = world.content().rules.spells.id_numbered(spell);
    let def = def.map(|s| world.content().rules.spells.get(s))?;
    let action = if def.has_hook(SPELL_OF_MASTERY) {
        Action::SpellOfMastery
    } else if GlobalEffect::AuraOfMajesty.is_in(def) {
        Action::AuraOfMajesty
    } else {
        Action::Enchantment
    };
    Some((found.value, action))
}

/// What a curse on a city costs its owner's feeling for the caster
/// (`Change_Relations_For_Bad_City_Spell`; the table is `opinion.city_spells`).
fn bad_city_spell(world: &World, spell: u16) -> Option<i32> {
    world
        .levers()
        .opinion
        .city_spells
        .iter()
        .find(|&&(n, _)| n == spell)
        .map(|&(_, value)| value)
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
        growth: 0,
        moved: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplo_b_stance::tests::pair;
    use crate::testworld::{P0, P1};
    use f4x_core::diplomacy::{Goods, Proposal};
    use f4x_core::world::Command;
    use f4x_master::Game;

    fn levers(w: &World) -> &OpinionLevers {
        &w.levers().opinion
    }

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
        let times = levers(&w).contrast_factor;
        let mut low = mind(&w, -40);
        low.change(&w, P0, P1, 5, Action::Friendship, w.round());
        assert!(
            low.friendship(&w, P0, P1) >= -40 + 5 * times,
            "counted more when it is negative"
        );
        let mut high = mind(&w, 60);
        high.change(&w, P0, P1, -5, Action::AttackedCity, w.round());
        assert!(
            high.friendship(&w, P0, P1) <= 60 - 5 * times,
            "counted more when it is positive"
        );
        let mut warm = mind(&w, 60);
        warm.change(&w, P0, P1, 10, Action::Friendship, w.round());
        assert!(
            warm.friendship(&w, P0, P1) < 60 + 10,
            "less when it is already good"
        );
    }

    #[test]
    fn the_feeling_is_capped_and_a_war_is_not_made_worse() {
        let mut w = pair(8, 8, 100);
        let mut m = mind(&w, 60);
        m.change(&w, P0, P1, 100, Action::Friendship, w.round());
        assert!(
            m.friendship(&w, P0, P1) <= levers(&w).no_alliance_most,
            "no treaty: no higher"
        );
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
            m.friendship(&w, P0, P1) <= -levers(&w).war_hostility,
            "in a war it stays hostile"
        );
    }

    #[test]
    fn a_city_taken_costs_the_loser_and_the_taker_nothing() {
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
            loser.friendship(&w, P0, P1) <= -levers(&w).city_taken,
            "{}",
            loser.friendship(&w, P0, P1)
        );
        let mut taker = mind(&w, 0);
        taker.feel(&w, P1, &event, w.round());
        assert_eq!(taker.friendship(&w, P0, P1), 0);
    }

    #[test]
    fn a_war_sets_the_feeling_low_and_peace_lifts_it() {
        let mut w = pair(8, 8, 100);
        let mut m = mind(&w, 20);
        let events = w.command(Command::Diplomacy(
            f4x_core::diplomacy::DiplomacyCommand::DeclareWar { player: P1, on: P0 },
        ));
        for e in &events {
            m.feel(&w, P0, e, w.round());
        }
        let (start, gain) = (levers(&w).war_start, levers(&w).peace_gain);
        assert_eq!(m.friendship(&w, P0, P1), -start);
        assert!(m.opinion(&w, P0, P1).war);
        let peace = Event::Diplomacy(DiplomacyEvent::TreatyChanged {
            a: P0,
            b: P1,
            treaty: Treaty::None,
        });
        m.feel(&w, P0, &peace, w.round());
        assert_eq!(m.friendship(&w, P0, P1), gain - start);
        m.feel(&w, P0, &peace, w.round());
        assert_eq!(
            m.friendship(&w, P0, P1),
            gain - start,
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
        let k = levers(&w);
        assert!(
            o.broken && o.hidden <= -k.broken_alliance && o.friendship < 30,
            "{o:?}"
        );
        let mut other = Memory::default();
        other.feel(&w, PlayerId(2), &broke, w.round());
        assert_eq!(
            other.opinion(&w, PlayerId(2), P1).hidden,
            -k.broken_bystander
        );
        let mut breaker = mind(&w, 30);
        breaker.feel(&w, P1, &broke, w.round());
        assert_eq!(
            breaker.friendship(&w, P0, P1),
            30,
            "the breaker does not feel it"
        );
    }

    #[test]
    fn an_ally_that_asks_to_be_paid_for_joining_a_war_is_thought_less_of_by_the_caller() {
        let w = pair(8, 8, 100);
        let third = PlayerId(2);
        let proposal = |from, to, kind| Proposal {
            from,
            to,
            kind,
            round: 0,
        };
        let countered = Event::Diplomacy(DiplomacyEvent::ProposalCountered {
            old: proposal(P0, P1, ProposalKind::WarOn(third)),
            new: proposal(
                P1,
                P0,
                ProposalKind::WarPrice {
                    third,
                    price: Goods::gold(1),
                },
            ),
        });
        let mut caller = mind(&w, 30);
        caller.feel(&w, P0, &countered, w.round());
        let after = caller.friendship(&w, P0, P1);
        assert!(after < 30 && after > 30 - 20, "a small penalty: {after}");
        let mut ally = mind(&w, 30);
        ally.feel(&w, P1, &countered, w.round());
        assert_eq!(ally.friendship(&w, P0, P1), 30);
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
        assert!(
            o.friendship > 0 && o.hidden == levers(&w).spell_regard,
            "{o:?}"
        );
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
        let k = levers(&w);
        let cost = k.loss_hero + 2 * k.loss_unit_halves / 2;
        assert_eq!(felt(&w, lost(P0)), -(cost as i32), "a hero and two units");
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
        let k = levers(&w);
        assert_eq!(felt(&w, assault(false)), -k.assault_city);
        assert_eq!(felt(&w, assault(true)), -k.assault_fortress);
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
        let k = crate::testworld::levers().diplomacy_b;
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
        let listed = |n: u16| {
            let k = levers(&w);
            k.city_spells
                .iter()
                .find(|&&(s, _)| s == n)
                .map(|&(_, v)| v)
                .or_else(|| {
                    k.enchantments
                        .iter()
                        .find(|e| e.spell == n)
                        .map(|e| e.value)
                })
                .expect("a spell the levers list")
        };
        assert_eq!(felt(&w, curse(85)), listed(85), "corruption");
        assert_eq!(felt(&w, curse(1)), 0, "a spell that is no curse");
        let cast = |n| DiplomacyEvent::EnchantmentCast {
            caster: P1,
            spell: spell(n),
        };
        assert_eq!(felt(&w, cast(213)), listed(213), "the Spell of Mastery");
        assert_eq!(felt(&w, cast(120)), listed(120), "armageddon");
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
    fn a_pact_goes_on_warming_the_feeling_past_what_an_alliance_asks() {
        let grown = |treaty| {
            let mut w = pair(8, 8, 100);
            w.start_treaty(P0, P1, treaty);
            let mut m = mind(&w, 0);
            m.slot(&w, P0, P1).default = 0;
            looks(&mut w, &mut m, 400);
            m.friendship(&w, P0, P1)
        };
        let floor = pair(8, 8, 100)
            .content()
            .rules
            .config
            .diplomacy
            .alliance_relation;
        let (pact, alliance) = (grown(Treaty::Pact), grown(Treaty::Alliance));
        assert!(pact > floor, "{pact} against {floor}");
        assert!(alliance > pact, "{alliance} against {pact}");
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
        let every = levers(&w).drift_down_every;
        // To the turn before the next on which it may come down.
        let short = every - 1 - w.round() % every;
        looks(&mut w, &mut high, short);
        assert_eq!(
            high.friendship(&w, P0, P1),
            30,
            "it comes down only now and then"
        );
        looks(&mut w, &mut high, every + 2);
        assert!(high.friendship(&w, P0, P1) < 30);
    }

    #[test]
    fn a_partners_first_and_last_warning_cost_it_and_the_one_between_does_not() {
        let w = pair(8, 8, 100);
        let last = w.content().rules.config.diplomacy.trespass_warnings;
        let warned = |warning| {
            felt(
                &w,
                DiplomacyEvent::Trespassing {
                    owner: P0,
                    intruder: P1,
                    warning,
                },
            )
        };
        assert!(warned(1) < 0, "the first warning");
        for between in 2..last {
            assert_eq!(warned(between), 0, "a turn of grace: {between}");
        }
        assert!(warned(last) < warned(1), "the last costs most");
    }

    /// P0's feeling for P1 after some turns with `unit` of P1 next to P0's city, without a treaty.
    fn beside_the_city(unit: u16) -> i32 {
        let mut w = pair(8, 8, 101);
        crate::testworld::spawn(&mut w, P1, crate::testworld::at(11, 10), unit, 2);
        w.explore(
            P0,
            crate::testworld::at(11, 10),
            f4x_def::half_int::HalfInt::from_raw(3),
        );
        let mut m = mind(&w, 0);
        m.slot(&w, P0, P1).default = 0;
        looks(&mut w, &mut m, 40);
        m.friendship(&w, P0, P1)
    }

    #[test]
    fn units_next_to_its_city_sour_a_stranger_but_engineers_do_not() {
        let fighters = beside_the_city(104 /* high_men_spearmen */);
        let engineers = beside_the_city(110 /* high_men_engineers */);
        assert!(fighters < engineers, "{fighters} against {engineers}");
    }

    #[test]
    fn an_army_a_little_stronger_than_its_own_is_disliked_and_a_weaker_one_is_not() {
        let after = |mine, theirs| {
            let mut w = pair(mine, theirs, 101);
            let mut m = mind(&w, 0);
            m.slot(&w, P0, P1).default = 0;
            looks(&mut w, &mut m, 9);
            m.friendship(&w, P0, P1)
        };
        // It starts where it drifts to: only the army moves it.
        assert!(
            after(8, 12) < after(8, 8),
            "{} against {}",
            after(8, 12),
            after(8, 8)
        );
        assert_eq!(after(12, 8), after(8, 8));
    }
}
