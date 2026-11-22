//! Row-level degrade (`docs/content-policy.md`, R6): a bad row of the wizards, the unit ratings, the
//! unit efficiencies or the premade items is dropped, the game plays without it, and the load says so.
//! Everything else stays a refusal.

use crate::support::*;
use f4x_def::content::Content;
use f4x_def::issue::{ContentIssue, Element, IssueKind, Outcome};
use f4x_def::rules::*;
use f4x_def::testdir::shipped;
use serde::{de::DeserializeOwned, Serialize};

const NOWHERE: u16 = 64_000;

/// A named way to break one row.
type Breaker<T> = (&'static str, fn(&mut T));

/// Load a copy of the shipped content with one file changed; the content must load, with every issue
/// a `Degrade` of `element` in `file`, and at least one.
fn degrades<T: DeserializeOwned + Serialize>(
    file: &str,
    element: Element,
    change: impl FnOnce(&mut T),
) -> (Content, Vec<ContentIssue>) {
    let dir = copy_of_shipped("c7");
    edit::<T>(&dir, file, change);
    let content = load(&dir).unwrap_or_else(|e| panic!("{file}: the game should play on: {e}"));
    let issues = content.rules.degraded.clone();
    assert!(!issues.is_empty(), "{file}: nothing is reported");
    for i in &issues {
        assert_eq!(i.outcome, Outcome::Degrade(element), "{file}: {i}");
        assert_eq!(i.file, file, "{i}");
        assert!(i.row.is_some(), "the row is named: {i}");
    }
    (content, issues)
}

#[test]
fn the_shipped_content_degrades_nothing() {
    assert!(
        shipped().rules.degraded.is_empty(),
        "{:#?}",
        shipped().rules.degraded
    );
}

#[test]
fn a_bad_wizard_is_dropped_the_others_stay_and_no_number_changes() {
    let all: Vec<u16> = shipped()
        .rules
        .wizards
        .defs()
        .iter()
        .map(|w| w.num)
        .collect();
    assert!(all.len() >= 3, "the content has several wizards");
    let victim = all[1];
    let bad: [Breaker<WizardDef>; 4] = [
        ("an unknown book realm", |w| {
            w.books.push((Id::new(NOWHERE), 1))
        }),
        ("an unknown retort", |w| w.retorts.push(Id::new(NOWHERE))),
        ("too many picks", |w| {
            let realm = w.books[0].0;
            w.books.push((realm, 1));
        }),
        ("no picks", |w| w.books.clear()),
    ];
    for (what, change) in bad {
        let (content, issues) = degrades::<Vec<WizardDef>>("wizards.ron", Element::Wizard, |v| {
            let at = v.iter().position(|w| w.num == victim).expect("row");
            change(&mut v[at]);
        });
        assert!(
            issues.iter().all(|i| i.row == Some(victim)),
            "{what}: {issues:#?}"
        );
        let left: Vec<u16> = content.rules.wizards.defs().iter().map(|w| w.num).collect();
        let want: Vec<u16> = all.iter().copied().filter(|n| *n != victim).collect();
        assert_eq!(
            left, want,
            "{what}: only that wizard goes, and no number moves"
        );
    }
}

#[test]
fn a_bad_premade_item_is_dropped_the_others_stay() {
    let item = shipped()
        .rules
        .items
        .defs()
        .iter()
        .find(|i| !i.powers.is_empty())
        .expect("an item")
        .clone();
    let all = shipped().rules.items.len();
    let bad: [Breaker<ItemDef>; 4] = [
        ("an unknown power", |i| i.powers.push(Id::new(NOWHERE))),
        ("an unknown charged spell", |i| {
            i.charges = Some((Id::new(NOWHERE), 1))
        }),
        ("a charge of nothing", |i| {
            i.charges = Some((Id::new(shipped().rules.spells.defs()[0].num), 0))
        }),
        ("an item that does nothing", |i| {
            i.effects.clear();
            i.powers.clear();
            i.charges = None;
            i.spell_skill = 0;
            i.spell_save = 0;
        }),
    ];
    for (what, change) in bad {
        let (content, issues) = degrades::<Vec<ItemDef>>("items.ron", Element::Item, |v| {
            let at = v.iter().position(|i| i.num == item.num).expect("row");
            change(&mut v[at]);
        });
        assert!(
            issues.iter().all(|i| i.row == Some(item.num)),
            "{what}: {issues:#?}"
        );
        assert_eq!(content.rules.items.len(), all - 1, "{what}");
        assert!(
            content.rules.items.try_get(Id::new(item.num)).is_none(),
            "{what}"
        );
    }
}

#[test]
fn a_bad_effect_on_an_item_drops_the_item() {
    let first = shipped().rules.items.defs()[0].num;
    let (content, _) = degrades::<Vec<ItemDef>>("items.ron", Element::Item, |v| {
        v[0].effects.push(f4x_def::rules::effects::EffectRule {
            when: f4x_def::rules::effects::Trigger::Passive,
            condition: f4x_def::rules::effects::Condition::Always,
            target: f4x_def::rules::effects::TargetSel::This,
            effect: f4x_def::rules::effects::Effect::GrantAbility(Id::new(NOWHERE)),
        })
    });
    assert!(content.rules.items.try_get(Id::new(first)).is_none());
}

#[test]
fn a_bad_rating_or_efficiency_row_is_dropped_the_others_stay() {
    let ratings = shipped().rules.ratings.units.len();
    let first = shipped().rules.ratings.units[0].num;
    let bad: [Breaker<UnitRatings>; 5] = [
        ("a unit that does not exist", |r| r.units[0].num = NOWHERE),
        ("no numbers", |r| r.units[0].levels.clear()),
        ("a negative number", |r| r.units[0].levels[0] = -1.0),
        ("not a number", |r| r.units[0].levels[0] = f32::NAN),
        ("zero", |r| r.units[0].levels[0] = 0.0),
    ];
    for (what, change) in bad {
        let (content, _) = degrades::<UnitRatings>("unit_ratings.ron", Element::Rating, change);
        assert_eq!(content.rules.ratings.units.len(), ratings - 1, "{what}");
        assert!(
            content
                .rules
                .ratings
                .units
                .iter()
                .all(|r| r.num != first && r.num != NOWHERE),
            "{what}"
        );
    }
    // A number twice: the second row goes, the first stays.
    let (content, issues) = degrades::<UnitRatings>("unit_ratings.ron", Element::Rating, |r| {
        r.units[1].num = r.units[0].num
    });
    assert!(
        issues.iter().any(|i| i.kind == IssueKind::Duplicate),
        "{issues:#?}"
    );
    assert_eq!(content.rules.ratings.units.len(), ratings - 1);
    assert_eq!(content.rules.ratings.units[0].num, first);

    let eff = shipped().rules.efficiency.units.len();
    let bad: [Breaker<UnitEfficiencies>; 4] = [
        ("a unit that does not exist", |r| r.units[0].num = NOWHERE),
        ("a cost of zero", |r| r.units[0].cost = 0.0),
        ("not a number", |r| r.units[0].per_100 = f32::NAN),
        ("a number twice", |r| r.units[1].num = r.units[0].num),
    ];
    for (what, change) in bad {
        let (content, _) =
            degrades::<UnitEfficiencies>("unit_efficiency.ron", Element::Efficiency, change);
        assert_eq!(content.rules.efficiency.units.len(), eff - 1, "{what}");
    }
}

#[test]
fn what_is_not_a_row_of_a_degradable_file_still_refuses_the_game() {
    // The hero factor is no row.
    let dir = copy_of_shipped("c7-factor");
    edit::<UnitRatings>(&dir, "unit_ratings.ron", |r| r.hero_factor = 0.0);
    assert!(load(&dir).is_err());
    // A file that cannot be read as its type has no row to drop.
    for file in [
        "wizards.ron",
        "unit_ratings.ron",
        "unit_efficiency.ron",
        "items.ron",
    ] {
        let dir = copy_of_shipped("c7-file");
        let text = read(&dir, file);
        write(&dir, file, &text[..text.len() / 2]);
        let e = load(&dir).expect_err(file);
        assert!(e.mentions(file), "{file}: {e}");
    }
    // A bad row of a required file is a refusal, and the list holds the droppable problem too.
    let dir = copy_of_shipped("c7-both");
    edit::<Vec<WizardDef>>(&dir, "wizards.ron", |v| v[0].books.clear());
    edit::<Vec<UnitDef>>(&dir, "units.ron", |v| v[0].abilities.push(Id::new(NOWHERE)));
    let e = load(&dir).expect_err("a bad unit refuses");
    assert!(e
        .issues
        .iter()
        .any(|i| i.file == "units.ron" && i.outcome == Outcome::Refuse));
    assert!(
        e.issues.iter().any(|i| i.file == "wizards.ron"),
        "the wizard is listed too: {e}"
    );
}

// ---- the help ----

mod help {
    use super::*;
    use f4x_def::help::{Help, HelpTopic};
    use f4x_def::load::load_help_checked;
    use f4x_def::testdir::TestDir;

    fn topic(index: u16, words: &str) -> HelpTopic {
        HelpTopic {
            index,
            title: format!("t{index}"),
            text: words.to_string(),
            picture: None,
            include: None,
        }
    }

    fn help(topics: Vec<HelpTopic>) -> Help {
        Help {
            topics,
            screens: vec![],
        }
    }

    fn all_degrade(issues: &[ContentIssue]) {
        assert!(!issues.is_empty());
        for i in issues {
            assert_eq!(i.outcome, Outcome::Degrade(Element::Help), "{i}");
            assert!(i.row.is_some(), "{i}");
        }
    }

    #[test]
    fn a_help_that_is_fine_is_left_as_it_is() {
        let h = help(vec![topic(1, "a"), topic(2, "b")]);
        let (kept, issues) = h.clone().repaired();
        assert_eq!(kept, h);
        assert!(issues.is_empty());
    }

    #[test]
    fn a_topic_number_used_twice_keeps_the_first_topic() {
        let (kept, issues) =
            help(vec![topic(1, "first"), topic(2, "b"), topic(1, "second")]).repaired();
        all_degrade(&issues);
        assert_eq!(
            kept.topics
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "b"]
        );
        assert!(kept.problems().is_empty());
    }

    #[test]
    fn a_topic_with_the_words_of_an_earlier_one_goes() {
        let mut same = topic(2, "words");
        same.title = "T1".into();
        let (kept, issues) = help(vec![topic(1, "words"), same, topic(3, "c")]).repaired();
        all_degrade(&issues);
        assert_eq!(
            kept.topics.iter().map(|t| t.index).collect::<Vec<_>>(),
            [1, 3]
        );
    }

    #[test]
    fn an_include_of_nothing_or_in_a_circle_is_cleared_and_the_topics_stay() {
        let mut a = topic(1, "a");
        a.include = Some(99);
        let mut b = topic(2, "b");
        b.include = Some(3);
        let mut c = topic(3, "c");
        c.include = Some(4);
        let mut d = topic(4, "d");
        d.include = Some(2);
        let (kept, issues) = help(vec![a, b, c, d]).repaired();
        all_degrade(&issues);
        assert_eq!(kept.topics.len(), 4, "no topic is lost");
        assert!(kept.problems().is_empty(), "{:#?}", kept.problems());
        assert!(kept.topic(1).is_some_and(|t| t.include.is_none()));
        let includes = kept.topics.iter().filter(|t| t.include.is_some()).count();
        assert_eq!(
            includes, 2,
            "one include of the circle is enough to break it"
        );
    }

    #[test]
    fn a_picture_that_is_a_path_is_cleared_and_the_text_stays() {
        let mut t = topic(1, "a");
        t.picture = Some(("../secret".into(), 0));
        let mut ok = topic(2, "b");
        ok.picture = Some(("plain.png".into(), 3));
        let (kept, issues) = help(vec![t, ok]).repaired();
        all_degrade(&issues);
        assert_eq!(kept.topic(1).map(|t| t.picture.clone()), Some(None));
        assert_eq!(
            kept.topic(2).and_then(|t| t.picture.clone()),
            Some(("plain.png".into(), 3))
        );
    }

    #[test]
    fn a_help_file_with_bad_topics_loads_with_the_issues_a_file_that_is_not_ron_refuses() {
        let dir = TestDir::new("c7-help");
        let rows = vec![topic(1, "a"), topic(1, "again"), topic(2, "b")];
        let text =
            ron::ser::to_string_pretty(&rows, ron::ser::PrettyConfig::default()).expect("written");
        std::fs::create_dir_all(dir.join("text/en/help")).expect("folder");
        std::fs::write(dir.join("text/en/help/help.ron"), &text).expect("written");
        let (h, issues) = load_help_checked(&[&dir], "en").expect("loads");
        assert_eq!(h.topics.len(), 2);
        all_degrade(&issues);
        std::fs::write(dir.join("text/en/help/help.ron"), &text[..text.len() / 2]).expect("cut");
        assert!(load_help_checked(&[&dir], "en").is_err());
    }
}
