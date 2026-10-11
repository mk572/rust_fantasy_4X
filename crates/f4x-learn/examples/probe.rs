//! What does a checkpoint do at the first decision of the wizard: how often does it flee, by the
//! odds of the scenario? `probe CHECKPOINT [BANK]`.

use std::path::Path;

use burn::module::Module;
use burn::prelude::*;
use f4x_core::battle::{BattleCommand, Side};
use f4x_learn::agent::{Driver, Request};
use f4x_learn::bank::Bank;
use f4x_learn::env::BattleEnv;
use f4x_learn::net::PolicyNet;
use f4x_learn::obs::{Brief, ObsSpec};
use f4x_learn::scenario::Odds;
use f4x_learn::train::choose;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let ckpt = args.next().ok_or("probe CHECKPOINT [BANK]")?;
    let bank_path = args
        .next()
        .unwrap_or_else(|| "models-local/bank.ron".into());
    let content =
        f4x_data::load_content_layers(&[Path::new("content")]).map_err(|e| e.to_string())?;
    let spec = ObsSpec::new(&content);
    let bank = Bank::from_text(&std::fs::read_to_string(&bank_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let device = Device::flex();
    let net = PolicyNet::new(spec.unit_features(), &device).load_file(&ckpt);
    let mut rng = Pcg64Mcg::seed_from_u64(1);
    for odds in [Odds::Even, Odds::Overwhelming, Odds::Overwhelmed] {
        let (mut asks, mut flees, mut casts) = (0, 0, 0);
        for s in bank.scenarios.iter().filter(|s| s.odds == odds).take(60) {
            let Ok(env) = BattleEnv::new(&content, s.clone()) else {
                continue;
            };
            let brief = Brief {
                may_flee: true,
                stakes: 0.5,
                ..Brief::default()
            };
            let mut d = Driver::new(env, &spec, brief);
            let mut none = |_: &f4x_core::battle::Battle, _: Side| BattleCommand::EndTurn;
            // The first wizard decision of the attacker.
            while let Ok(Some(p)) = d.advance([true, false], &mut none) {
                if let Request::Wizard { .. } = p.request {
                    for _ in 0..20 {
                        let c = choose(&net, &spec, &device, &[&p], &[false], false, &mut rng)[0];
                        asks += 1;
                        flees += usize::from(c.index == f4x_learn::act::FLEE);
                        casts += usize::from(c.index >= f4x_learn::act::SPELL);
                    }
                    break;
                }
                match &p.request {
                    Request::Unit { advice, .. } => {
                        d.answer_unit(&p, advice.unwrap_or(f4x_learn::act::DONE))?
                    }
                    Request::Wizard { .. } => {}
                }
            }
        }
        println!("{odds:?}: first wizard decision flees {flees}/{asks}, casts {casts}/{asks}");
        // What fleeing at once is worth, against the scripted AI playing the attacker to the end.
        let (mut flee_r, mut fight_r, mut n) = (0.0f32, 0.0f32, 0);
        for s in bank.scenarios.iter().filter(|s| s.odds == odds).take(60) {
            for flee in [true, false] {
                let Ok(env) = BattleEnv::new(&content, s.clone()) else {
                    continue;
                };
                let mut d = Driver::new(
                    env,
                    &spec,
                    Brief {
                        may_flee: true,
                        stakes: 0.5,
                        ..Brief::default()
                    },
                );
                let mut scripted =
                    |b: &f4x_core::battle::Battle, _: Side| f4x_learn::policy::scripted_command(b);
                let mut first = true;
                while let Ok(Some(p)) = d.advance([true, false], &mut scripted) {
                    match &p.request {
                        Request::Wizard { .. } => {
                            let c = if flee && first {
                                f4x_learn::act::FLEE
                            } else {
                                f4x_learn::act::SKIP
                            };
                            first = false;
                            d.answer_wizard(&p, c, 0, 0)?;
                        }
                        Request::Unit { advice, .. } => {
                            d.answer_unit(&p, advice.unwrap_or(f4x_learn::act::DONE))?
                        }
                    }
                }
                let r = f4x_learn::reward::reward(&d.env.outcome, Side::Attacker, 0.5);
                if flee {
                    flee_r += r;
                    n += 1;
                } else {
                    fight_r += r;
                }
            }
        }
        println!(
            "   scripted fights on: reward {:.3}; flees at once: reward {:.3} ({n} battles)",
            fight_r / n.max(1) as f32,
            flee_r / n.max(1) as f32
        );
    }
    Ok(())
}
