//! The client's help text (`crates/f4x-lang/content/text/en/help/help.ron` and `help_screens.ron`)
//! agrees with the rules: every building, race, retort, realm, wizard, ability and spell names a
//! topic that exists, and every topic is opened by an area or by a rules entry. The help is the
//! client's; these checks need the shipped rules, so they run here, not in `f4x-def`.

#[cfg(test)]
mod tests {
    use crate::test_app::shipped;
    use f4x_def::testdir::TestResult;

    /// A building's help topic is its description in the city builder: it names the topic,
    /// is titled with the building's name, and gives each plain effect with its number.
    #[test]
    fn every_building_has_a_help_topic_that_gives_its_effects_with_their_numbers() -> TestResult {
        use f4x_def::rules::effects::{Effect, Trigger};
        let help = shipped::help();
        let content = shipped::content();
        for (_, def) in content.rules.buildings.iter() {
            let topic = help
                .topic_of(def.help)
                .ok_or_else(|| format!("{} has no help topic", def.id))?;
            assert_eq!(
                topic.title,
                crate::test_app::shipped::names()
                    .building(def.num)
                    .to_string(),
                "{}",
                def.id
            );
            for rule in &def.effects {
                if let (Trigger::Passive, Effect::Modify { value, .. }) = (&rule.when, &rule.effect)
                {
                    assert!(
                        topic.text.contains(&value.abs().to_string()),
                        "the help of {} leaves out {value}: {}",
                        def.id,
                        topic.text
                    );
                }
            }
            if !def.requires.is_empty() {
                assert!(topic.text.contains("Needs"), "{}: {}", def.id, topic.text);
            }
        }
        Ok(())
    }

    #[test]
    fn each_townsfolk_topic_names_the_buildings_its_race_cannot_build() -> TestResult {
        let help = shipped::help();
        let content = shipped::content();
        assert_eq!(content.rules.races.iter().count(), 14);
        for (_, r) in content.rules.races.iter() {
            let index = r
                .help
                .ok_or_else(|| format!("{} has no help topic", r.id))?;
            let topic = help.topic(index).ok_or("the value is missing")?;
            for (_, b) in content.rules.buildings.iter() {
                if !b.buildable_by(f4x_def::rules::RaceId::new(r.num)) {
                    assert!(
                        topic
                            .text
                            .contains(crate::test_app::shipped::names().building(b.num)),
                        "topic {index} ({}) does not name the {}",
                        r.id,
                        crate::test_app::shipped::names().building(b.num)
                    );
                }
            }
        }
        Ok(())
    }

    /// A retort, a race and a spell realm each name a help topic that exists and has text; a
    /// retort's topic is titled with its name.
    #[test]
    fn every_retort_race_and_realm_has_a_help_topic() -> TestResult {
        let help = shipped::help();
        let content = shipped::content();
        for r in content.rules.retorts.defs() {
            let topic = help
                .topic_of(r.help)
                .ok_or_else(|| format!("retort {} has no help topic", r.id))?;
            assert_eq!(
                topic.title,
                crate::test_app::shipped::names().retort(r.num).to_string(),
                "retort {}",
                r.id
            );
            assert!(!topic.text.trim().is_empty(), "retort {}", r.id);
        }
        for r in content.rules.races.defs() {
            assert!(help.topic_of(r.help).is_some(), "race {}", r.id);
        }
        for r in content.rules.realms.defs() {
            // The arcane realm has no spells of its own to explain.
            if r.id != "arcane" {
                assert!(help.topic_of(r.help).is_some(), "realm {}", r.id);
            }
        }
        Ok(())
    }

    #[test]
    fn the_shipped_help_is_consistent_and_every_topic_is_opened_by_an_area() -> TestResult {
        let help = shipped::help();
        assert_eq!(help.screens.len(), 41);
        assert!(!help.topics.is_empty());
        // The game's own text is written for areas that open it: no orphan topic.
        let mut opened: std::collections::HashSet<i32> = help
            .screens
            .iter()
            .flat_map(|s| s.fields.iter().map(|f| f.help))
            .collect();
        // A topic that another topic includes is shown with it.
        opened.extend(help.topics.iter().filter_map(|t| t.include.map(i32::from)));
        // An ability, a spell, a building or a ready-made wizard, a retort, a race or a realm names its own topic.
        let content = shipped::content();
        opened.extend(
            content
                .rules
                .abilities
                .defs()
                .iter()
                .filter_map(|a| a.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .spells
                .defs()
                .iter()
                .filter_map(|s| s.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .wizards
                .defs()
                .iter()
                .map(|w| i32::from(w.help)),
        );
        opened.extend(
            content
                .rules
                .buildings
                .defs()
                .iter()
                .filter_map(|b| b.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .retorts
                .defs()
                .iter()
                .filter_map(|r| r.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .races
                .defs()
                .iter()
                .filter_map(|r| r.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .realms
                .defs()
                .iter()
                .filter_map(|r| r.help.map(i32::from)),
        );
        // The three switches of the Settings screen the original has no topic for (the screen's
        // `help_topic` in the client names them).
        opened.extend([807, 808, 809]);
        for t in &help.topics {
            assert!(
                opened.contains(&i32::from(t.index)),
                "topic {} has no area",
                t.index
            );
            assert!(!t.text.trim().is_empty(), "topic {} is empty", t.index);
        }
        // The main screen's Done button is first at a point that Next Turn also covers.
        let (topic, _) = help.at(1, 250, 178).ok_or("the value is missing")?;
        assert_eq!(topic.index, 223);
        let (topic, _) = help
            .at_where(1, 250, 178, |t| t != 223)
            .ok_or("the value is missing")?;
        assert_eq!(topic.index, 229);
        Ok(())
    }

    /// A ready-made wizard names its help topic (`help` in `wizards.ron`); the topic is titled
    /// with the wizard's name, and the text is ours.
    #[test]
    fn every_ready_made_wizard_has_a_help_topic_by_number() -> TestResult {
        let help = shipped::help();
        let content = shipped::content();
        for w in content.rules.wizards.defs() {
            let topic = help
                .topics
                .iter()
                .find(|t| t.index == w.help)
                .ok_or_else(|| format!("no help topic {} for the wizard {}", w.help, w.num))?;
            assert_eq!(
                topic.title,
                crate::test_app::shipped::names().wizard(w.num).to_string(),
                "wizard {}",
                w.num
            );
            assert!(!topic.text.trim().is_empty(), "wizard {}", w.num);
        }
        Ok(())
    }

    #[test]
    fn every_ability_and_every_spell_has_a_help_topic() -> TestResult {
        let help = shipped::help();
        let content = shipped::content();
        let mut used = std::collections::HashSet::new();
        for ability in content.rules.abilities.defs() {
            let topic = help.topic_of(ability.help);
            assert!(
                topic.is_some_and(|t| !t.text.trim().is_empty()),
                "no help for the ability {}",
                ability.id
            );
            assert!(
                used.insert(ability.help),
                "ability {} shares its topic",
                ability.id
            );
        }
        for spell in content.rules.spells.defs() {
            let topic = help.topic_of(spell.help);
            assert!(
                topic.is_some_and(|t| !t.text.trim().is_empty()),
                "no help for the spell {}",
                spell.id
            );
            assert!(
                used.insert(spell.help),
                "spell {} shares its topic",
                spell.id
            );
        }
        Ok(())
    }
}
