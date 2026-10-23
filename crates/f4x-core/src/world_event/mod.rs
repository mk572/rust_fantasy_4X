//! Random events (`docs/events.md`): the state of the events that are going on, and the plain
//! rules about them (how likely one is, who it falls on, when a lasting one ends).
//!
//! The original keeps one table of statuses (`events_table`), not enchantments with timers, and
//! the rules that care ask it: the city's mana asks which moon shines, the wizard's power asks
//! whether mana is short. This module is that table. It holds no handlers: the rules read it
//! through [`Events`], and what an event does to the world when it happens is done by
//! `world_event/game.rs`, which draws one each turn and plays it out.

pub(crate) mod game;

use crate::game::serial::Serial;
use crate::game::setting::EventFrequency;
use crate::player::PlayerId;

/// The eighteen events, numbered as in `EVENTS.LBX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    GreatMeteor = 1,
    Gift,
    Disjunction,
    Marriage,
    Earthquake,
    Piracy,
    Plague,
    Rebellion,
    Donation,
    Depletion,
    NewMinerals,
    PopulationBoom,
    GoodMoon,
    BadMoon,
    ConjunctionChaos,
    ConjunctionNature,
    ConjunctionSorcery,
    ManaShort,
}

impl EventKind {
    pub const ALL: [EventKind; 18] = [
        EventKind::GreatMeteor,
        EventKind::Gift,
        EventKind::Disjunction,
        EventKind::Marriage,
        EventKind::Earthquake,
        EventKind::Piracy,
        EventKind::Plague,
        EventKind::Rebellion,
        EventKind::Donation,
        EventKind::Depletion,
        EventKind::NewMinerals,
        EventKind::PopulationBoom,
        EventKind::GoodMoon,
        EventKind::BadMoon,
        EventKind::ConjunctionChaos,
        EventKind::ConjunctionNature,
        EventKind::ConjunctionSorcery,
        EventKind::ManaShort,
    ];

    /// The event's number in `EVENTS.LBX` and `EVENTMSG.LBX`, from 1.
    pub fn number(self) -> u32 {
        self as u32
    }

    pub fn from_number(number: u32) -> Option<EventKind> {
        Self::ALL.into_iter().find(|e| e.number() == number)
    }

    /// The name a save file and the console use.
    pub fn id(self) -> &'static str {
        match self {
            EventKind::GreatMeteor => "great_meteor",
            EventKind::Gift => "gift",
            EventKind::Disjunction => "disjunction",
            EventKind::Marriage => "marriage",
            EventKind::Earthquake => "earthquake",
            EventKind::Piracy => "piracy",
            EventKind::Plague => "plague",
            EventKind::Rebellion => "rebellion",
            EventKind::Donation => "donation",
            EventKind::Depletion => "depletion",
            EventKind::NewMinerals => "new_minerals",
            EventKind::PopulationBoom => "population_boom",
            EventKind::GoodMoon => "good_moon",
            EventKind::BadMoon => "bad_moon",
            EventKind::ConjunctionChaos => "conjunction_chaos",
            EventKind::ConjunctionNature => "conjunction_nature",
            EventKind::ConjunctionSorcery => "conjunction_sorcery",
            EventKind::ManaShort => "mana_short",
        }
    }

    pub fn from_id(id: &str) -> Option<EventKind> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Good events fall more on the weak wizards, bad ones on the strong (`m_event_good_array`).
    pub fn is_good(self) -> bool {
        !matches!(
            self,
            EventKind::GreatMeteor
                | EventKind::Earthquake
                | EventKind::Piracy
                | EventKind::Plague
                | EventKind::Rebellion
                | EventKind::Depletion
                | EventKind::ManaShort
        )
    }

    /// The events that act on the whole world while they last: the moons, the conjunctions and
    /// Mana Short. Only one of them goes on at a time.
    pub fn is_world(self) -> bool {
        self.number() >= EventKind::GoodMoon.number()
    }

    /// Whether the event goes on for turns (the others happen and are over).
    pub fn lasts(self) -> bool {
        self.is_world() || matches!(self, EventKind::Plague | EventKind::PopulationBoom)
    }

    /// Events that fall on one of the victim's cities.
    pub fn needs_city(self) -> bool {
        matches!(
            self,
            EventKind::GreatMeteor
                | EventKind::Marriage
                | EventKind::Earthquake
                | EventKind::Plague
                | EventKind::Rebellion
                | EventKind::PopulationBoom
        )
    }
}

/// A lasting event that is going on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lasting {
    pub kind: EventKind,
    /// Turns it has gone on; it cannot end before the fourth.
    pub turns: u32,
    /// The wizard it fell on (a plague, a population boom); none for a world event.
    pub victim: Option<hecs::Entity>,
    /// The city it fell on, by creation number so that a save finds it again.
    pub city: Option<Serial>,
}

/// Which events are going on, and when the last one happened.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Events {
    ongoing: Vec<Lasting>,
    /// The turn the last event was drawn (`last_event_turn`).
    last_event: u32,
    /// Turns counted since the game began (`_event_delay_count`, which stops at five).
    quiet: u32,
}

/// No events going on, for the views that are made without a game.
pub(crate) static NONE: Events = Events {
    ongoing: Vec::new(),
    last_event: 0,
    quiet: 0,
};

impl Events {
    pub fn is_ongoing(&self, kind: EventKind) -> bool {
        self.ongoing.iter().any(|e| e.kind == kind)
    }

    pub fn get(&self, kind: EventKind) -> Option<&Lasting> {
        self.ongoing.iter().find(|e| e.kind == kind)
    }

    /// Every event that is going on, in the order of the table.
    pub fn ongoing(&self) -> impl Iterator<Item = &Lasting> {
        self.ongoing.iter()
    }

    /// Whether the plague or the population boom is on this city.
    pub fn on_city(&self, kind: EventKind, city: Serial) -> bool {
        self.ongoing
            .iter()
            .any(|e| e.kind == kind && e.city == Some(city))
    }

    /// The world event going on, if any.
    pub fn world_event(&self) -> Option<EventKind> {
        self.ongoing.iter().map(|e| e.kind).find(|k| k.is_world())
    }

    /// Start a lasting event (nothing changes if it is going on already).
    pub(crate) fn start(
        &mut self,
        kind: EventKind,
        victim: Option<hecs::Entity>,
        city: Option<Serial>,
    ) {
        if !self.is_ongoing(kind) {
            self.ongoing.push(Lasting {
                kind,
                turns: 0,
                victim,
                city,
            });
            self.ongoing.sort_by_key(|e| e.kind);
        }
    }

    pub(crate) fn end(&mut self, kind: EventKind) {
        self.ongoing.retain(|e| e.kind != kind);
    }

    /// Every lasting event's age goes up by a turn.
    pub(crate) fn age(&mut self, kind: EventKind) {
        for e in self.ongoing.iter_mut().filter(|e| e.kind == kind) {
            e.turns += 1;
        }
    }

    /// An event has been drawn this turn.
    pub(crate) fn drawn(&mut self, turn: u32) {
        self.last_event = turn;
        self.quiet = 0;
    }

    /// The pressure for this turn's draw, and the count of quiet turns goes up.
    pub(crate) fn pressure(&mut self, turn: u32, percent: u16, frequency: EventFrequency) -> u32 {
        let waited = turn.saturating_sub(self.last_event);
        if self.quiet < 5 {
            self.quiet += 1;
            return 0;
        }
        scaled_pressure(waited, percent, frequency)
    }

    /// The saved form: for each event the number, the turns, the victim and the city.
    pub fn to_save(&self, id_of: impl Fn(hecs::Entity) -> Option<PlayerId>) -> EventsSave {
        EventsSave {
            last_event: self.last_event,
            quiet: self.quiet,
            ongoing: self
                .ongoing
                .iter()
                .map(|e| LastingSave {
                    kind: e.kind.id().to_string(),
                    turns: e.turns,
                    victim: e.victim.and_then(&id_of).map(|p| p.0),
                    city: e.city.map(|s| s.0),
                })
                .collect(),
        }
    }

    /// Rebuild from the saved form; `None` for an event this version does not know.
    pub fn from_save(
        saved: &EventsSave,
        entity_of: impl Fn(PlayerId) -> Option<hecs::Entity>,
    ) -> Option<Events> {
        let ongoing = saved
            .ongoing
            .iter()
            .map(|e| {
                Some(Lasting {
                    kind: EventKind::from_id(&e.kind)?,
                    turns: e.turns,
                    victim: e.victim.and_then(|p| entity_of(PlayerId(p))),
                    city: e.city.map(Serial),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Events {
            ongoing,
            last_event: saved.last_event,
            quiet: saved.quiet,
        })
    }

    /// What the moons add to the mana of a city's religious buildings, which together give
    /// `religious` mana (`CITYCALC.c`, city magic power). A moon takes half of it from the
    /// wizards of the opposite realm (Good Moon: death; Bad Moon: life) and gives the wizards of
    /// its own realm (Good Moon: life; Bad Moon: death) as much again, so their buildings give
    /// twice. The help says 50 percent; the game follows the code (more mana, a faster game).
    /// A wizard with books of both realms gets both.
    pub fn moon_mana(&self, religious: i32, life: bool, death: bool) -> i32 {
        let good = self.is_ongoing(EventKind::GoodMoon);
        let bad = self.is_ongoing(EventKind::BadMoon);
        let mut change = 0;
        if (good && death) || (bad && life) {
            change -= religious / 2;
        }
        if (good && life) || (bad && death) {
            change += religious;
        }
        change
    }

    /// The power of a node of `realm` after the conjunctions: a conjunction doubles the nodes of
    /// its own colour and halves the others (`CITYCALC.c`, phase 3e).
    pub fn node_power(&self, realm: &str, power: i32) -> i32 {
        let mut power = power;
        for (kind, colour) in [
            (EventKind::ConjunctionChaos, "chaos"),
            (EventKind::ConjunctionSorcery, "sorcery"),
            (EventKind::ConjunctionNature, "nature"),
        ] {
            if self.is_ongoing(kind) {
                power = if realm == colour {
                    power * 2
                } else {
                    power / 2
                };
            }
        }
        power
    }

    /// Whether every source of magic power has dried up.
    pub fn mana_is_short(&self) -> bool {
        self.is_ongoing(EventKind::ManaShort)
    }
}

/// The saved form of [`Events`].
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EventsSave {
    pub last_event: u32,
    pub quiet: u32,
    pub ongoing: Vec<LastingSave>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LastingSave {
    pub kind: String,
    pub turns: u32,
    pub victim: Option<u8>,
    pub city: Option<u64>,
}

/// The pressure after `waited` quiet turns, for a game whose level counts `percent` of the turns
/// waited (`events.pressure_percent`: 50 on Intro up to 100 on Impossible, so a harder level has
/// events more often), with the game's event frequency: halved or doubled. An event is drawn with
/// the chance of this out of 512.
pub fn scaled_pressure(waited: u32, percent: u16, frequency: EventFrequency) -> u32 {
    let percent = u32::from(percent);
    let (num, den) = match frequency {
        EventFrequency::Half => (percent, 200),
        EventFrequency::Normal => (percent, 100),
        EventFrequency::Double => (percent * 2, 100),
    };
    waited * num / den
}

/// Whether an event is drawn: a roll of 1 to 512 not over the pressure.
pub fn fires(pressure: u32, roll_512: u32) -> bool {
    roll_512 <= pressure
}

/// Each wizard's weight for being the victim (`Get_Event_Victim`): the strong for a bad event,
/// the weak (600 less the power, at least 0) for a good one, all halved until they add up to
/// 500 or less.
pub fn victim_weights(powers: &[i32], good: bool) -> Vec<i32> {
    let mut weights: Vec<i32> = powers
        .iter()
        .map(|&p| if good { (600 - p).max(0) } else { p })
        .collect();
    while weights.iter().sum::<i32>() > 500 {
        for w in &mut weights {
            *w /= 2;
        }
    }
    weights
}

/// The wizard a roll of 1 to the sum of the weights lands on, if there is one.
pub fn pick_victim(weights: &[i32], roll: i32) -> Option<usize> {
    let mut left = roll - 1;
    for (i, &w) in weights.iter().enumerate() {
        if left < w {
            return Some(i);
        }
        left -= w;
    }
    None
}

/// Whether a plague or a population boom ends: a 1 on a d20, once it has gone on over three
/// turns.
pub fn flat_end(roll_20: u32, turns: u32) -> bool {
    roll_20 == 1 && turns > 3
}

/// Whether a moon, a conjunction or Mana Short ends: the roll of a d20 not over two less than
/// the turns it has gone on, once that is over three, so the chance climbs with its age.
pub fn world_end(roll_20: u32, turns: u32) -> bool {
    turns > 3 && i64::from(roll_20) <= i64::from(turns) - 2
}

/// The gold the pirates take: 30 to 50 percent of the reserve (the roll is of 1 to 21), in
/// tens.
pub fn pirates_take(gold: i32, roll_21: u32) -> i32 {
    let loss = gold * (29 + roll_21 as i32) / 100;
    loss - loss % 10
}

/// The gold of a donation: 100 more than five times a roll of 1 to 100.
pub fn donation(roll_100: u32) -> i32 {
    100 + roll_100 as i32 * 5
}

/// How an event's message addresses the player: the index within the event's seven lines of
/// `EVENTMSG.LBX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// It has fallen on you.
    Started = 0,
    /// A lasting event that fell on you is over.
    Ended = 1,
    /// A world event, or one that happens to everyone.
    Happened = 3,
    /// It has fallen on a wizard you have met.
    Elsewhere = 4,
}

/// What the player is told when an event happens or ends; the client composes the message from
/// `EVENTMSG.LBX` and these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventNotice {
    pub kind: EventKind,
    pub voice: Voice,
    /// The wizard it fell on; none for a world event.
    pub player: Option<PlayerId>,
    pub player_name: String,
    /// The city it fell on, with its name and size class.
    pub city: Option<hecs::Entity>,
    pub city_name: String,
    pub city_size: String,
    /// Gold (piracy, donation).
    pub number: i32,
    /// The item of a gift.
    pub item: String,
    /// The mineral of a depletion or a find, as the message words it ("n iron", " coal").
    pub ore: String,
    pub lost_units: u32,
    pub lost_population: u32,
    pub lost_buildings: u32,
}

impl EventNotice {
    pub fn new(kind: EventKind, voice: Voice) -> EventNotice {
        EventNotice {
            kind,
            voice,
            player: None,
            player_name: String::new(),
            city: None,
            city_name: String::new(),
            city_size: String::new(),
            number: 0,
            item: String::new(),
            ore: String::new(),
            lost_units: 0,
            lost_population: 0,
            lost_buildings: 0,
        }
    }
}

/// The keys of the names of the city size classes, outpost to capital (`_city_size_names`).
pub const CITY_SIZE_KEYS: [&str; 6] = [
    "city.size_outpost",
    "city.size_hamlet",
    "city.size_village",
    "city.size_town",
    "city.size_city",
    "city.size_capital",
];

/// The minerals an event can put on the map: the feature id and the key of the word for it,
/// what follows the article in "a{ore} deposit".
pub const ORES: [(&str, &str); 9] = [
    ("iron_ore", "event.ore_iron_ore"),
    ("coal", "event.ore_coal"),
    ("silver_ore", "event.ore_silver_ore"),
    ("gold_ore", "event.ore_gold_ore"),
    ("gem", "event.ore_gem"),
    ("mithril_ore", "event.ore_mithril_ore"),
    ("adamantium_ore", "event.ore_adamantium_ore"),
    ("quork_crystal", "event.ore_quork_crystal"),
    ("crysx_crystal", "event.ore_crysx_crystal"),
];

/// One line of `EVENTMSG.LBX`, as the importer writes it to `event_texts.ron`: the event, its
/// voice (the line within the event's seven) and the text with `{city}`, `{size}`, `{player}`,
/// `{number}`, `{ore}`, `{item}`, `{units}`, `{people}`, `{buildings}` and the plural endings
/// `{units_s}` and `{buildings_s}` where the original has its data bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct EventText {
    pub event: u32,
    pub voice: u32,
    pub text: String,
}

/// The message of a notice, if the texts have one for it (`Get_Event_Message`). A count of
/// nothing reads "no", the people lost are given in thousands, and the plural ending is
/// dropped for exactly one.
pub fn compose(
    lines: &[EventText],
    words: &crate::text::Texts,
    notice: &EventNotice,
) -> Option<String> {
    use crate::text::Args;
    let line = lines
        .iter()
        .find(|t| t.event == notice.kind.number() && t.voice == notice.voice as u32)?;
    let count = |n: u32, key: &str| {
        if n == 0 {
            words.get("event.none", &Args::new())
        } else {
            words.get(key, &Args::new().count("n", n as i32))
        }
    };
    let plural = |n: u32| words.get("event.plural_ending", &Args::new().count("n", n as i32));
    Some(
        line.text
            .replace("{city}", &notice.city_name)
            .replace("{size}", &notice.city_size)
            .replace("{player}", &notice.player_name)
            .replace("{number}", &notice.number.to_string())
            .replace("{ore}", &notice.ore)
            .replace("{item}", &notice.item)
            .replace("{units}", &count(notice.lost_units, "event.number"))
            .replace("{people}", &count(notice.lost_population, "event.thousand"))
            .replace("{buildings}", &count(notice.lost_buildings, "event.number"))
            .replace("{units_s}", &plural(notice.lost_units))
            .replace("{buildings_s}", &plural(notice.lost_buildings)),
    )
}

#[cfg(test)]
mod tests;
