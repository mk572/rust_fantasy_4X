//! How well does the scenario generator's idea of "odds" predict who wins when the scripted AI
//! plays both sides? Prints the attacker's win rate by the generator's class and by the strength
//! ratio it aimed for.

use std::path::Path;

use f4x_learn::{generate, policy, BattleEnv};

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let content = f4x_data::load_content_layers(&[Path::new("content")]).expect("content/");
    let mut rows: Vec<(String, bool, usize, usize, bool, bool)> = Vec::new();
    for seed in 0..n {
        let s = generate(&content, seed);
        let Ok(mut env) = BattleEnv::new(&content, s.clone()) else {
            continue;
        };
        while !env.done() {
            let c = policy::scripted_command(env.battle().unwrap());
            env.step(c).unwrap();
        }
        let won = env.outcome.winner == Some(f4x_core::battle::Side::Attacker);
        rows.push((
            format!("{:?}", s.odds),
            won,
            s.attackers.len(),
            s.defenders.len(),
            matches!(s.ground, f4x_learn::scenario::Ground::City { .. }),
            s.attacker_wizard.is_some(),
        ));
    }
    for class in ["Even", "Overwhelming", "Overwhelmed"] {
        let of: Vec<_> = rows.iter().filter(|r| r.0 == class).collect();
        let w = of.iter().filter(|r| r.1).count();
        println!("{class}: {w}/{} attacker wins", of.len());
        for ground in [false, true] {
            let g: Vec<_> = of.iter().filter(|r| r.4 == ground).collect();
            let gw = g.iter().filter(|r| r.1).count();
            println!(
                "   {}: {gw}/{}",
                if ground { "city" } else { "field" },
                g.len()
            );
        }
    }
}
