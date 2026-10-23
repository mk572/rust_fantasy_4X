//! What the other wizards say. The original's lines (`DIPLOMSG.LBX`) are its own prose and
//! are not part of this project; these are the game's own (`content/text/en/diplomacy.ron`), by
//! situation and mood. A wizard's mood is the relation level (0 hate to 10 harmony); `salt`
//! picks among the lines of one situation and varies from wizard to wizard.

use f4x_core::diplomacy::ProposalKind;
use f4x_core::text::{Args, Texts};
use f4x_core::wizard::objective::Personality;

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
