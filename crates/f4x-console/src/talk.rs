//! Talking to the other wizards: what is known of them, proposals, answers, gifts, treaties.

use f4x_core::diplomacy::{relation_word, DiplomacyCommand, DiplomacyError, ProposalKind, Treaty};
use f4x_core::game::Command;
use f4x_core::player::PlayerId;

use crate::args::{parse_player, resolve, Args};
use crate::{CommandDef, ConsoleError, Ctx, Kind};

pub const TABLE: &[CommandDef] = &[
    CommandDef {
        name: "diplomacy",
        kind: Kind::Query,
        usage: "diplomacy [wizard]",
        help: "The wizards you have met: relation, treaty and what you may propose; with a wizard, the details and the proposals waiting.",
        run: diplomacy,
    },
    CommandDef {
        name: "propose",
        kind: Kind::Legal,
        usage: "propose <wizard> <pact|alliance|peace|war <third>|break <third>|swap <give> <get>>",
        help: "Make a proposal: a wizard pact, an alliance, peace (at war), a declaration of war on a third wizard, breaking its alliance with a third, or swapping a spell of yours for one of theirs. They answer in their turn.",
        run: propose,
    },
    CommandDef {
        name: "answer",
        kind: Kind::Legal,
        usage: "answer <wizard> <yes|no>",
        help: "Answer the proposal that wizard made you.",
        run: answer,
    },
    CommandDef {
        name: "breaktreaty",
        kind: Kind::Legal,
        usage: "breaktreaty <wizard>",
        help: "End your wizard pact or alliance with them. Every wizard remembers an oath breaker.",
        run: breaktreaty,
    },
    CommandDef {
        name: "declarewar",
        kind: Kind::Legal,
        usage: "declarewar <wizard>",
        help: "Declare war on a wizard you have met (any treaty is broken first).",
        run: declarewar,
    },
    CommandDef {
        name: "givegold",
        kind: Kind::Legal,
        usage: "givegold <wizard> <amount>",
        help: "Give gold as tribute: the more of their treasury it is, the better they think of you.",
        run: givegold,
    },
    CommandDef {
        name: "givespell",
        kind: Kind::Legal,
        usage: "givespell <wizard> <spell>",
        help: "Give a spell you know that they lack as tribute.",
        run: givespell,
    },
];

pub fn treaty_word(t: Treaty) -> &'static str {
    match t {
        Treaty::None => "no treaty",
        Treaty::Pact => "wizard pact",
        Treaty::Alliance => "alliance",
        Treaty::War => "war",
    }
}

pub fn error_word(e: DiplomacyError) -> &'static str {
    match e {
        DiplomacyError::NotWizards => "not wizards",
        DiplomacyError::SameWizard => "that is you",
        DiplomacyError::NotContacted => "you have not met",
        DiplomacyError::NotAllowed => "not allowed now",
        DiplomacyError::AlreadyPending => "a proposal to them is already waiting",
        DiplomacyError::NothingPending => "they have proposed nothing",
        DiplomacyError::NotEnoughGold => "not enough gold",
        DiplomacyError::SpellUnavailable => "the spell is not yours or they know it",
    }
}

impl Ctx<'_> {
    /// A wizard other than the console's player.
    fn other_wizard(&self, a: &Args, i: usize) -> Result<PlayerId, ConsoleError> {
        let id = parse_player(a.need(i, "wizard number")?)?;
        if id == self.session.player {
            return Err(ConsoleError::Usage("that is you".into()));
        }
        if !self.game.is_wizard(id) {
            return Err(ConsoleError::NoSuch("wizard".into()));
        }
        Ok(id)
    }
}

fn diplomacy(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let me = ctx.session.player;
    let others: Vec<PlayerId> = match a.get(0) {
        Some(_) => vec![ctx.other_wizard(a, 0)?],
        None => ctx
            .game
            .wizards()
            .into_iter()
            .filter(|&w| w != me && ctx.game.have_met(me, w))
            .collect(),
    };
    if others.is_empty() {
        ctx.say("you have met no other wizard");
    }
    for &w in &others {
        let name = ctx
            .game
            .player(w)
            .map_or("?", |p| p.name.as_str())
            .to_string();
        if !ctx.game.have_met(me, w) {
            ctx.say(format!("{} {name}: not met", w.0));
            continue;
        }
        let rel = ctx.game.relation(me, w);
        ctx.say(format!(
            "{} {name}: {} ({rel}), {}",
            w.0,
            relation_word(rel),
            treaty_word(ctx.game.treaty(me, w))
        ));
        if a.get(0).is_some() {
            let can = |k| ctx.game.check_proposal(me, w, k).is_ok();
            let mut offers = Vec::new();
            for (word, kind) in [
                ("pact", ProposalKind::Pact),
                ("alliance", ProposalKind::Alliance),
                ("peace", ProposalKind::Peace),
            ] {
                if can(kind) {
                    offers.push(word);
                }
            }
            ctx.say(format!(
                "  you may propose: {}",
                if offers.is_empty() {
                    "nothing of the kind".to_string()
                } else {
                    offers.join(", ")
                }
            ));
        }
    }
    let waiting: Vec<String> = ctx
        .game
        .diplomacy
        .proposals()
        .iter()
        .filter(|p| p.from == me || p.to == me)
        .map(|p| {
            let (from, to) = (
                crate::describe::player(ctx.game, p.from),
                crate::describe::player(ctx.game, p.to),
            );
            format!(
                "waiting: {from} to {to}: {}",
                crate::describe::proposal(ctx.game, p.kind)
            )
        })
        .collect();
    for line in waiting {
        ctx.say(line);
    }
    Ok(())
}

fn propose(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let to = ctx.other_wizard(a, 0)?;
    let kind = match a.need(1, "pact, alliance, peace, war, break or swap")? {
        "pact" => ProposalKind::Pact,
        "alliance" => ProposalKind::Alliance,
        "peace" => ProposalKind::Peace,
        "war" => ProposalKind::WarOn(ctx.other_wizard(a, 2)?),
        "break" => ProposalKind::BreakAllianceWith(ctx.other_wizard(a, 2)?),
        "swap" => {
            let spells = &ctx.game.content().rules.spells;
            let give = resolve(spells, a.need(2, "your spell")?, |d| &d.name, "spell")?;
            let get = resolve(spells, a.need(3, "their spell")?, |d| &d.name, "spell")?;
            ProposalKind::Exchange { give, get }
        }
        other => {
            return Err(ConsoleError::Usage(format!(
                "propose pact, alliance, peace, war, break or swap, not {other:?}"
            )))
        }
    };
    let from = ctx.session.player;
    talk(ctx, DiplomacyCommand::Propose { from, to, kind })
}

fn answer(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let from = ctx.other_wizard(a, 0)?;
    let accept = match a.need(1, "yes or no")? {
        "yes" | "y" | "accept" => true,
        "no" | "n" | "refuse" => false,
        other => {
            return Err(ConsoleError::Usage(format!(
                "answer yes or no, not {other:?}"
            )))
        }
    };
    let player = ctx.session.player;
    talk(
        ctx,
        DiplomacyCommand::Answer {
            player,
            from,
            accept,
        },
    )
}

fn breaktreaty(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let with = ctx.other_wizard(a, 0)?;
    let player = ctx.session.player;
    talk(ctx, DiplomacyCommand::BreakTreaty { player, with })
}

fn declarewar(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let on = ctx.other_wizard(a, 0)?;
    let player = ctx.session.player;
    talk(ctx, DiplomacyCommand::DeclareWar { player, on })
}

fn givegold(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let to = ctx.other_wizard(a, 0)?;
    let amount = a.int(1, "amount")? as i32;
    let from = ctx.session.player;
    talk(ctx, DiplomacyCommand::GiveGold { from, to, amount })
}

fn givespell(ctx: &mut Ctx, a: &Args) -> Result<(), ConsoleError> {
    let to = ctx.other_wizard(a, 0)?;
    let spell = resolve(
        &ctx.game.content().rules.spells,
        a.need(1, "spell")?,
        |d| &d.name,
        "spell",
    )?;
    let from = ctx.session.player;
    talk(ctx, DiplomacyCommand::GiveSpell { from, to, spell })
}

/// Send the command and say why it was turned down in words.
fn talk(ctx: &mut Ctx, command: DiplomacyCommand) -> Result<(), ConsoleError> {
    match ctx.apply(Command::Diplomacy(command)) {
        Err(ConsoleError::Refused(_)) => {
            // `apply` has printed the event; find the reason among them.
            let why = ctx.events.iter().rev().find_map(|e| match e {
                f4x_core::game::Event::Rejected(f4x_core::game::Rejection::Diplomacy(d)) => {
                    Some(*d)
                }
                _ => None,
            });
            Err(ConsoleError::Refused(
                why.map_or("refused", error_word).to_string(),
            ))
        }
        other => other,
    }
}
