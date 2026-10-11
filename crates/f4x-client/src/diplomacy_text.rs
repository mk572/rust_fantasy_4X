//! What the other wizards say. The original's lines (`DIPLOMSG.LBX`) are its own prose and
//! are not part of this project; these are the game's own, by situation and mood. A wizard's
//! mood is the relation level (0 hate to 10 harmony).

use f4x_core::diplomacy::{Personality, ProposalKind};

/// How warm a wizard is: cold (levels 0 to 3), wary (4 to 6) or warm (7 to 10).
fn warmth(level: usize) -> usize {
    match level {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    }
}

/// Pick one of a few lines by a number that varies from wizard to wizard.
fn pick<'a>(lines: &[&'a str], salt: usize) -> &'a str {
    lines[salt % lines.len()]
}

pub fn greeting(level: usize, personality: Personality, salt: usize) -> String {
    let cold = [
        "So. You dare to stand before me. Say what you came for, and be brief.",
        "I had hoped never to look on your face again. Speak.",
        "My patience with you is thin. Do not waste it.",
    ];
    let wary = [
        "Greetings. I will hear what you have to say.",
        "We meet at last. What is it you want of me?",
        "I have been expecting you. Speak, and we shall see.",
    ];
    let warm = [
        "Welcome, friend! It is always a pleasure to speak with you.",
        "Ah, you honour me with your company. What may I do for you?",
        "A fine day for a visit! Tell me what is on your mind.",
    ];
    let lines = match warmth(level) {
        0 => &cold,
        1 => &wary,
        _ => &warm,
    };
    let line = pick(lines, salt);
    match personality {
        Personality::Maniacal if warmth(level) == 0 => {
            "You will not live to regret this visit! Speak, while you still can.".to_string()
        }
        Personality::Peaceful if warmth(level) == 0 => {
            "I would rather we were not enemies. Say what you will.".to_string()
        }
        _ => line.to_string(),
    }
}

pub fn menu_prompt() -> &'static str {
    "How may I serve you:"
}

pub fn propose_prompt() -> &'static str {
    "You propose a treaty:"
}

pub fn threaten_prompt() -> &'static str {
    "Your actions:"
}

pub fn tribute_prompt() -> &'static str {
    "What do you offer as tribute?"
}

pub fn exchange_prompt() -> &'static str {
    "What type of spell interests you?"
}

pub fn exchange_give_prompt() -> &'static str {
    "What will you trade for it?"
}

pub fn third_prompt(war: bool) -> &'static str {
    if war {
        "Against which wizard?"
    } else {
        "Whose alliance should be broken?"
    }
}

pub fn cold_shoulder(salt: usize) -> &'static str {
    pick(
        &[
            "I have nothing more to say to you. Leave me.",
            "No. We have spoken enough. Begone.",
            "Do not trouble me again, not until you have learned some manners.",
        ],
        salt,
    )
}

pub fn accepted(kind: ProposalKind, salt: usize) -> &'static str {
    match kind {
        ProposalKind::Pact => pick(
            &[
                "Very well. A pact between us shall stand.",
                "Agreed. Let our two houses keep the peace.",
            ],
            salt,
        ),
        ProposalKind::Alliance => pick(
            &[
                "An alliance it is! Together we shall be hard to stop.",
                "Agreed. Your enemies are mine, and mine are yours.",
            ],
            salt,
        ),
        ProposalKind::Peace => pick(
            &[
                "Enough blood has been spilled. Peace, then.",
                "I accept. The fighting is over, for now.",
            ],
            salt,
        ),
        ProposalKind::WarOn(_) => "So be it. Our armies will march against them.",
        ProposalKind::BreakAllianceWith(_) => "You have persuaded me. That alliance is ended.",
        ProposalKind::Exchange { .. } => "A fair trade. Here is what I promised.",
    }
}

pub fn refused(kind: ProposalKind, level: usize, salt: usize) -> &'static str {
    let cold = warmth(level) == 0;
    match kind {
        ProposalKind::Pact | ProposalKind::Alliance => {
            if cold {
                pick(
                    &[
                        "A treaty with you? You are mad to ask.",
                        "Never. I would sooner trust a viper.",
                    ],
                    salt,
                )
            } else {
                pick(
                    &[
                        "I am not ready for such a bond. Perhaps another time.",
                        "No. I do not know you well enough for that.",
                    ],
                    salt,
                )
            }
        }
        ProposalKind::Peace => {
            if cold {
                "There will be no peace until you have paid for what you did."
            } else {
                "Not yet. I am not done with you."
            }
        }
        ProposalKind::WarOn(_) => "That war is not mine to fight. I decline.",
        ProposalKind::BreakAllianceWith(_) => "I keep my word to my allies. I will not break it.",
        ProposalKind::Exchange { .. } => "I do not care for that trade.",
    }
}

pub fn lapsed() -> &'static str {
    "I will think on it."
}

pub fn gold_thanks(salt: usize) -> &'static str {
    pick(
        &[
            "Your generosity is noted, and will not be forgotten.",
            "Gold is always welcome. You are wiser than I thought.",
        ],
        salt,
    )
}

pub fn spell_thanks(salt: usize) -> &'static str {
    pick(
        &[
            "A gift of knowledge! I am in your debt.",
            "You share your secrets freely. I shall remember it.",
        ],
        salt,
    )
}

pub fn treaty_broken() -> &'static str {
    "So our agreement is at an end. Do not expect me to forget it."
}

pub fn nothing_to_trade() -> &'static str {
    "We have nothing to trade: you have no spell that I lack, or I none that you lack."
}
