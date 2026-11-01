use super::*;

struct Thing(u16, &'static str);

impl Def for Thing {
    fn id(&self) -> &str {
        self.1
    }
    fn num(&self) -> u16 {
        self.0
    }
}

struct Other(u16, &'static str);

impl Def for Other {
    fn id(&self) -> &str {
        self.1
    }
    fn num(&self) -> u16 {
        self.0
    }
}

#[test]
fn a_name_becomes_the_number_of_its_kind_and_an_unknown_name_stays_open() {
    let mut names = Names::new();
    names.add(&[Thing(4, "sword"), Thing(9, "axe")]);
    names.add(&[Other(1, "sword")]);
    let mut list: Vec<Ref<Thing>> = vec!["axe".into(), "bow".into(), "sword".into()];
    assert!(list.iter().all(|r| r.id().is_none()), "not resolved yet");
    names.resolve_all(&mut list);
    assert_eq!(
        list.iter().map(|r| r.id().map(Id::num)).collect::<Vec<_>>(),
        [Some(9), None, Some(4)]
    );
    // The same alias in another kind is another number.
    let mut other: Ref<Other> = "sword".into();
    names.resolve(&mut other);
    assert_eq!(other.id().map(Id::num), Some(1));
}

#[test]
fn a_repeated_alias_keeps_the_first_number_as_the_registry_does() {
    let mut names = Names::new();
    names.add(&[Thing(2, "twin"), Thing(3, "twin")]);
    let mut r: Ref<Thing> = "twin".into();
    names.resolve(&mut r);
    assert_eq!(r.id().map(Id::num), Some(2));
}

#[test]
fn an_optional_reference_is_resolved_only_when_there_is_one() {
    let mut names = Names::new();
    names.add(&[Thing(5, "gem")]);
    let (mut some, mut none): (Option<Ref<Thing>>, Option<Ref<Thing>>) = (Some("gem".into()), None);
    names.resolve_opt(&mut some);
    names.resolve_opt(&mut none);
    assert_eq!(some.and_then(|r| r.id()).map(Id::num), Some(5));
    assert!(none.is_none());
}

#[test]
fn a_reference_is_written_and_read_as_its_name_and_equal_by_name() -> Result<(), ron::Error> {
    let mut r: Ref<Thing> = "gem".into();
    r.set(Some(Id::new(5)));
    let text = ron::to_string(&r)?;
    assert_eq!(text, "\"gem\"");
    let back: Ref<Thing> = ron::from_str(&text).map_err(|e| e.code)?;
    assert_eq!(back, r, "equal by name, resolved or not");
    assert!(back.id().is_none(), "reading does not resolve");
    Ok(())
}
