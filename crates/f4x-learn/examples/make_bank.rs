//! Make a bank of scenarios with measured odds: `make_bank OUT.ron [count] [first_seed]`.
//! Run it through `scripts/cool_run.sh`; it uses three threads.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use f4x_learn::bank::{calibrate, class, Bank};
use f4x_learn::scenario::{generate_raw, Odds};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let out = args
        .next()
        .ok_or("usage: make_bank OUT.ron [count] [first_seed]")?;
    let count: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3000);
    let first: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(500_000);
    let content =
        f4x_data::load_content_layers(&[Path::new("content")]).map_err(|e| e.to_string())?;
    let next = AtomicUsize::new(0);
    let found = Mutex::new(Bank::default());
    let want = count / 3;
    std::thread::scope(|sc| {
        for _ in 0..3 {
            sc.spawn(|| loop {
                let k = next.fetch_add(1, Ordering::SeqCst);
                if k > count * 20 {
                    return;
                }
                {
                    let b = found.lock().unwrap();
                    let n = |o: Odds| b.scenarios.iter().filter(|s| s.odds == o).count();
                    if n(Odds::Even) >= want
                        && n(Odds::Overwhelming) >= want
                        && n(Odds::Overwhelmed) >= want
                    {
                        return;
                    }
                }
                let mut s = generate_raw(&content, first + k as u64);
                let Some(wins) = calibrate(&content, &s) else {
                    continue;
                };
                s.odds = class(wins);
                let mut b = found.lock().unwrap();
                let have = b.scenarios.iter().filter(|x| x.odds == s.odds).count();
                if have < want {
                    b.scenarios.push(s);
                    let total = b.scenarios.len();
                    if total % 100 == 0 {
                        eprintln!("{total} of {} scenarios", want * 3);
                    }
                }
            });
        }
    });
    let bank = found.into_inner().unwrap();
    for o in [Odds::Even, Odds::Overwhelming, Odds::Overwhelmed] {
        println!(
            "{o:?}: {}",
            bank.scenarios.iter().filter(|s| s.odds == o).count()
        );
    }
    std::fs::write(&out, bank.to_text()).map_err(|e| e.to_string())
}
