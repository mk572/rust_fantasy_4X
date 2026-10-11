//! How fast can battles be built and played? `cargo run --release -p f4x-learn --example bench [N]`
//! (run it through `scripts/cool_run.sh`).

use std::path::Path;
use std::time::Instant;

use f4x_learn::{generate, policy, BattleEnv};
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let content = f4x_data::load_content_layers(&[Path::new("content")]).expect("content/");
    let (mut build, mut play, mut steps, mut done) = (0.0f64, 0.0f64, 0u64, 0u64);
    for seed in 0..n {
        let t = Instant::now();
        let scenario = generate(&content, seed);
        let Ok(mut env) = BattleEnv::new(&content, scenario) else {
            continue;
        };
        build += t.elapsed().as_secs_f64();
        let t = Instant::now();
        let mut rng = Pcg64Mcg::seed_from_u64(seed);
        while !env.done() {
            let a = env.actions();
            let c = policy::random_command(&a, &mut rng);
            env.step(c).expect("offered commands are accepted");
        }
        play += t.elapsed().as_secs_f64();
        steps += u64::from(env.steps);
        done += 1;
    }
    println!(
        "{done} battles: build {:.1} ms each, play {:.1} ms each ({:.0} steps each, {:.0} us a step incl. listing the legal commands)",
        1000.0 * build / done as f64,
        1000.0 * play / done as f64,
        steps as f64 / done as f64,
        1e6 * play / steps as f64
    );
}
