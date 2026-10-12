use super::*;
use crate::magic::Retort;
use crate::testutil::content_with_magic;

const P0: PlayerId = PlayerId(0);

/// A world seen as numbers: what each source gives, no cities or nodes behind it.
struct Fake {
    magic: Magic,
    content: Content,
    casting_return: bool,
    city: (i32, i32),
    fortress: Option<Plane>,
    volcanoes: i32,
    nodes: i32,
    upkeep: i32,
}

fn fake() -> Fake {
    let mut magic = Magic::default();
    magic.books.insert("life".into(), 2);
    Fake {
        magic,
        content: content_with_magic(),
        casting_return: false,
        city: (4, 2),
        fortress: None,
        volcanoes: 0,
        nodes: 0,
        upkeep: 0,
    }
}

impl IncomeView for Fake {
    fn magic(&self, _: PlayerId) -> Option<&Magic> {
        Some(&self.magic)
    }
    fn content(&self) -> &Content {
        &self.content
    }
    fn casting_return(&self, _: PlayerId) -> bool {
        self.casting_return
    }
    fn city_yield(&self, _: PlayerId) -> (i32, i32) {
        self.city
    }
    fn fortress_plane(&self, _: PlayerId) -> Option<Plane> {
        self.fortress
    }
    fn volcanoes(&self, _: PlayerId) -> i32 {
        self.volcanoes
    }
    fn node_income(&self, _: PlayerId) -> i32 {
        self.nodes
    }
    fn upkeep_total(&self, _: PlayerId) -> i32 {
        self.upkeep
    }
}

#[test]
fn the_power_base_adds_cities_fortress_volcanoes_and_nodes() {
    let mut v = fake();
    assert_eq!(income(&v, P0).power_base, 4);
    v.fortress = Some(Plane::Arcanus);
    assert_eq!(income(&v, P0).power_base, 4 + 2, "a point per book");
    v.fortress = Some(Plane::Myrror);
    assert_eq!(income(&v, P0).power_base, 4 + 2 + 5, "five more on Myrror");
    v.volcanoes = 1;
    v.nodes = 3;
    assert_eq!(income(&v, P0).power_base, 4 + 2 + 5 + 1 + 3);
    v.city = (-30, 0);
    assert_eq!(income(&v, P0).power_base, 0, "never below nothing");
}

#[test]
fn the_power_is_split_by_the_percentages_and_the_cities_add_research() {
    let mut v = fake();
    v.fortress = Some(Plane::Arcanus);
    let i = income(&v, P0);
    assert_eq!(i.city_research, 2);
    assert_eq!(
        i.split,
        PowerSplit {
            mana: 2,
            skill: 2,
            research: 2 + 2
        }
    );
    assert_eq!(i.research, 4);
    v.magic.set_split(100, 0, 0).unwrap();
    assert_eq!(
        income(&v, P0).split,
        PowerSplit {
            mana: 6,
            skill: 0,
            research: 2
        }
    );
    v.magic.retorts.insert(Retort::ManaFocusing);
    assert_eq!(income(&v, P0).split.mana, 7, "a quarter more mana");
}

#[test]
fn the_spell_of_return_stops_all_income() {
    let mut v = fake();
    v.fortress = Some(Plane::Arcanus);
    v.upkeep = 6;
    v.casting_return = true;
    let i = income(&v, P0);
    assert_eq!(i.power_base, 0);
    assert_eq!(i.split, PowerSplit::default());
    assert_eq!(i.research, 0);
    assert_eq!(i.mana_upkeep, 6, "the upkeep is still owed");
}

#[test]
fn a_channeler_pays_half_the_upkeep() {
    let mut v = fake();
    v.upkeep = 9;
    assert_eq!(income(&v, P0).mana_upkeep, 9);
    v.magic.retorts.insert(Retort::Channeler);
    assert_eq!(income(&v, P0).mana_upkeep, 4);
}
