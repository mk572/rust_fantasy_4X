//! What the other wizards say. The original's lines (`DIPLOMSG.LBX`) are its own prose and
//! are not part of this project; these are the game's own (`content/text/en/diplomacy.ron`), by
//! situation and mood. A wizard's mood is the relation level (0 hate to 10 harmony); `salt`
//! picks among the lines of one situation and varies from wizard to wizard.

use f4x_core::diplomacy::ProposalKind;
use f4x_core::text::{Args, Texts};
use f4x_core::wizard::objective::Personality;

/// One line of the original's talk (`Get_Diplomacy_Statement`) for `situation`: a variant picked
/// by `salt`, with the `{placeholders}` filled from `values`. The original draws one of the
/// situation's lines at random, one fewer in a game of two wizards; a variant that names
/// something `values` cannot fill is not drawn. `None` when the game has no such line.
pub fn original_line(
    lines: &std::collections::HashMap<u16, Vec<String>>,
    situation: u16,
    salt: usize,
    two_players: bool,
    values: &[(&str, String)],
) -> Option<String> {
    let all = lines.get(&situation)?;
    let count = all.len().saturating_sub(usize::from(two_players)).max(1);
    let fill = |line: &str| {
        let mut out = line.to_string();
        for (name, value) in values {
            out = out.replace(&format!("{{{name}}}"), value);
        }
        (!out.contains('{')).then_some(out)
    };
    let usable: Vec<String> = all.iter().take(count).filter_map(|l| fill(l)).collect();
    (!usable.is_empty()).then(|| usable[salt % usable.len()].clone())
}

/// The situation of a wizard's first greeting (`dipact_Greeting_*`): 15 plus the personality.
pub fn greeting_situation(personality: Personality) -> u16 {
    15 + match personality {
        Personality::Maniacal => 0,
        Personality::Ruthless => 1,
        Personality::Aggressive => 2,
        Personality::Chaotic => 3,
        Personality::Lawful => 4,
        Personality::Peaceful => 5,
    }
}

/// The situation of a wizard's answer to a proposal that was accepted (`do_*` 51 to 56), if the
/// original has one.
pub fn accepted_situation(kind: ProposalKind) -> Option<u16> {
    match kind {
        ProposalKind::Pact => Some(51),
        ProposalKind::Alliance => Some(52),
        ProposalKind::Peace => Some(53),
        ProposalKind::BreakAllianceWith(_) => Some(55),
        ProposalKind::WarOn(_) => Some(56),
        ProposalKind::Exchange { .. } | ProposalKind::Threat | ProposalKind::Demand { .. } => None,
    }
}

/// How warm a wizard is: cold (levels 0 to 3), wary (4 to 6) or warm (7 to 10).
fn warmth(level: usize) -> usize {
    match level {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    }
}

fn line(texts: &Texts, key: &str) -> String {
    texts.get(key, &Args::new())
}

/// One of the lines of `key`, by the wizard's salt.
fn pick(texts: &Texts, key: &str, salt: usize) -> String {
    texts.get(key, &Args::new().count("salt", salt))
}

pub fn greeting(texts: &Texts, level: usize, personality: Personality, salt: usize) -> String {
    let cold = warmth(level) == 0;
    match personality {
        Personality::Maniacal if cold => line(texts, "diplomacy.greeting_maniacal"),
        Personality::Peaceful if cold => line(texts, "diplomacy.greeting_peaceful"),
        _ => pick(
            texts,
            match warmth(level) {
                0 => "diplomacy.greeting_cold",
                1 => "diplomacy.greeting_wary",
                _ => "diplomacy.greeting_warm",
            },
            salt,
        ),
    }
}

pub fn menu_prompt(texts: &Texts) -> String {
    line(texts, "diplomacy.menu_prompt")
}

pub fn propose_prompt(texts: &Texts) -> String {
    line(texts, "diplomacy.propose_prompt")
}

pub fn threaten_prompt(texts: &Texts) -> String {
    line(texts, "diplomacy.threaten_prompt")
}

pub fn tribute_prompt(texts: &Texts) -> String {
    line(texts, "diplomacy.tribute_prompt")
}

pub fn exchange_prompt(texts: &Texts) -> String {
    line(texts, "diplomacy.exchange_prompt")
}

pub fn exchange_give_prompt(texts: &Texts) -> String {
    line(texts, "diplomacy.exchange_give_prompt")
}

pub fn third_prompt(texts: &Texts, war: bool) -> String {
    line(
        texts,
        if war {
            "diplomacy.third_war"
        } else {
            "diplomacy.third_break"
        },
    )
}

pub fn cold_shoulder(texts: &Texts, salt: usize) -> String {
    pick(texts, "diplomacy.cold_shoulder", salt)
}

pub fn accepted(texts: &Texts, kind: ProposalKind, salt: usize) -> String {
    match kind {
        ProposalKind::Pact => pick(texts, "diplomacy.accepted_pact", salt),
        ProposalKind::Alliance => pick(texts, "diplomacy.accepted_alliance", salt),
        ProposalKind::Peace => pick(texts, "diplomacy.accepted_peace", salt),
        ProposalKind::WarOn(_) => line(texts, "diplomacy.accepted_war"),
        ProposalKind::BreakAllianceWith(_) => line(texts, "diplomacy.accepted_break"),
        ProposalKind::Exchange { .. } => line(texts, "diplomacy.accepted_exchange"),
        ProposalKind::Threat => line(texts, "diplomacy.accepted_threat"),
        ProposalKind::Demand { .. } => line(texts, "diplomacy.gold_thanks"),
    }
}

pub fn refused(texts: &Texts, kind: ProposalKind, level: usize, salt: usize) -> String {
    let cold = warmth(level) == 0;
    match kind {
        ProposalKind::Pact | ProposalKind::Alliance if cold => {
            pick(texts, "diplomacy.refused_treaty_cold", salt)
        }
        ProposalKind::Pact | ProposalKind::Alliance => {
            pick(texts, "diplomacy.refused_treaty", salt)
        }
        ProposalKind::Peace if cold => line(texts, "diplomacy.refused_peace_cold"),
        ProposalKind::Peace => line(texts, "diplomacy.refused_peace"),
        ProposalKind::WarOn(_) => line(texts, "diplomacy.refused_war"),
        ProposalKind::BreakAllianceWith(_) => line(texts, "diplomacy.refused_break"),
        ProposalKind::Exchange { .. } => line(texts, "diplomacy.refused_exchange"),
        ProposalKind::Threat => line(texts, "diplomacy.refused_threat"),
        ProposalKind::Demand { .. } => line(texts, "diplomacy.refused_threat"),
    }
}

pub fn lapsed(texts: &Texts) -> String {
    line(texts, "diplomacy.lapsed")
}

pub fn gold_thanks(texts: &Texts, salt: usize) -> String {
    pick(texts, "diplomacy.gold_thanks", salt)
}

pub fn spell_thanks(texts: &Texts, salt: usize) -> String {
    pick(texts, "diplomacy.spell_thanks", salt)
}

pub fn treaty_broken(texts: &Texts) -> String {
    line(texts, "diplomacy.treaty_broken")
}

pub fn nothing_to_trade(texts: &Texts) -> String {
    line(texts, "diplomacy.nothing_to_trade")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lines() -> HashMap<u16, Vec<String>> {
        HashMap::from([(
            51,
            vec![
                "In the year {year}, {wizard} and {player} agree.".to_string(),
                "{wizard} agrees to {third}.".to_string(),
                "Plain.".to_string(),
            ],
        )])
    }

    #[test]
    fn a_line_is_filled_and_one_that_cannot_be_is_not_drawn() {
        let values = [
            ("year", "1402".to_string()),
            ("wizard", "Ariel".to_string()),
            ("player", "Merlin".to_string()),
        ];
        let said: Vec<_> = (0..4)
            .filter_map(|salt| original_line(&lines(), 51, salt, false, &values))
            .collect();
        assert_eq!(said.len(), 4);
        assert!(said.iter().all(|l| !l.contains('{')));
        assert!(said.contains(&"In the year 1402, Ariel and Merlin agree.".to_string()));
        assert!(original_line(&lines(), 99, 0, false, &values).is_none());
    }

    #[test]
    fn two_wizards_draw_from_one_line_fewer() {
        let values: [(&str, String); 0] = [];
        // Only "Plain." is usable with nothing to fill in, whatever the count.
        assert_eq!(
            original_line(&lines(), 51, 7, true, &values).as_deref(),
            None,
            "the first two lines need names and the last is cut in a two-wizard game"
        );
        assert_eq!(
            original_line(&lines(), 51, 7, false, &values).as_deref(),
            Some("Plain.")
        );
    }

    #[test]
    fn the_greeting_follows_the_personality() {
        assert_eq!(greeting_situation(Personality::Maniacal), 15);
        assert_eq!(greeting_situation(Personality::Peaceful), 20);
    }
}
