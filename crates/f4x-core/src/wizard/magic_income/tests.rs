use super::*;
use crate::game::testutil::shipped;
use crate::testdir::TestResult;
use crate::wizard::magic::Retort;
use crate::wizard::sheet::{Books, Retorts, WizardMut};

const P0: PlayerId = PlayerId(0);

/// A world seen as numbers: what each source gives, no cities or nodes behind it.
struct Fake {
    world: hecs::World,
    wizard: hecs::Entity,
    content: Content,
    casting_return: bool,
    city: (i32, i32),
    fortress: Option<Plane>,
    volcanoes: i32,
    nodes: i32,
    upkeep: i32,
}

fn fake() -> Fake {
    let mut world = hecs::World::new();
    let wizard = world.spawn(crate::wizard::sheet::bundle());
    WizardMut::new(&mut world, wizard)
        .get_mut::<Books>()
        .0
        .insert("life".into(), 2);
    Fake {
        world,
        wizard,
        content: shipped(),
        casting_return: false,
        city: (4, 2),
        fortress: None,
        volcanoes: 0,
        nodes: 0,
        upkeep: 0,
    }
}

impl IncomeView for Fake {
    fn wizard(&self, _: PlayerId) -> Option<Wizard<'_>> {
        Some(Wizard::new(&self.world, self.wizard))
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
fn the_power_is_split_by_the_percentages_and_the_cities_add_research() -> TestResult {
    let mut v = fake();
    v.fortress = Some(Plane::Arcanus);
    let i = income(&v, P0);
    assert_eq!(i.city_research, 2);
    assert_eq!(
        i.split,
        PowerShares {
            mana: 2,
            skill: 2,
            research: 2 + 2
        }
    );
    assert_eq!(i.research, 4);
    WizardMut::new(&mut v.world, v.wizard)
        .set_split(100, 0, 0)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        income(&v, P0).split,
        PowerShares {
            mana: 6,
            skill: 0,
            research: 2
        }
    );
    WizardMut::new(&mut v.world, v.wizard)
        .get_mut::<Retorts>()
        .0
        .insert(Retort::ManaFocusing);
    assert_eq!(income(&v, P0).split.mana, 7, "a quarter more mana");
    Ok(())
}

#[test]
fn the_spell_of_return_stops_all_income() {
    let mut v = fake();
    v.fortress = Some(Plane::Arcanus);
    v.upkeep = 6;
    v.casting_return = true;
    let i = income(&v, P0);
    assert_eq!(i.power_base, 0);
    assert_eq!(i.split, PowerShares::default());
    assert_eq!(i.research, 0);
    assert_eq!(i.mana_upkeep, 6, "the upkeep is still owed");
}

#[test]
fn a_channeler_pays_half_the_upkeep() {
    let mut v = fake();
    v.upkeep = 9;
    assert_eq!(income(&v, P0).mana_upkeep, 9);
    WizardMut::new(&mut v.world, v.wizard)
        .get_mut::<Retorts>()
        .0
        .insert(Retort::Channeler);
    assert_eq!(income(&v, P0).mana_upkeep, 4);
}
