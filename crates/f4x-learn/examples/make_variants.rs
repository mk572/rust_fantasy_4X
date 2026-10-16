//! Make variants of trained networks for a tournament (`docs/PLANNED.md` item 25).
//!
//! `make_variants OUT_DIR --parent NAME=CHECKPOINT.bpk [--parent ...] [--auto N] [--recipes FILE]
//!  [--strengths 1,2,4] [--noise 0.01,0.03,0.1] [--seed N]`
//!
//! Each parent is copied into OUT_DIR with its variants. A variant is the parent plus a *nudge*
//! (offsets to the final-layer biases of the policy: more spells, earlier flight, more melee, and
//! so on, see `net::Nudge`) and/or Gaussian *noise* on the weights. Recipes come from
//! `--auto N` (N random ones per parent: one or two nudges with random signs and strengths, and
//! noise on half of them) and from `--recipes FILE`, whose lines are
//! `NAME knob=value ... [noise=SIGMA] [seed=N]` (`#` starts a comment), applied to every parent.
//! Knobs: spell flee attack shoot walk done wall more_mana.
//!
//! It writes `OUT_DIR/entries.txt`, one line per entry (the parents first): `name<TAB>checkpoint
//! <TAB>parent<TAB>recipe`, which the tournament reads.

use std::path::{Path, PathBuf};

use burn::prelude::*;
use f4x_learn::net::{load_checkpoint, save_checkpoint, Nudge, PolicyNet};
use f4x_learn::obs::ObsSpec;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

const KNOBS: [&str; 8] = [
    "spell",
    "flee",
    "attack",
    "shoot",
    "walk",
    "done",
    "wall",
    "more_mana",
];

fn knob_mut<'a>(n: &'a mut Nudge, name: &str) -> Option<&'a mut f32> {
    Some(match name {
        "spell" => &mut n.spell,
        "flee" => &mut n.flee,
        "attack" => &mut n.attack,
        "shoot" => &mut n.shoot,
        "walk" => &mut n.walk,
        "done" => &mut n.done,
        "wall" => &mut n.wall,
        "more_mana" => &mut n.more_mana,
        _ => return None,
    })
}

#[derive(Debug, Clone, Default)]
struct Recipe {
    nudge: Nudge,
    noise: f32,
    seed: Option<u64>,
}

impl Recipe {
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        let mut n = self.nudge;
        for k in KNOBS {
            let v = *knob_mut(&mut n, k).unwrap();
            if v != 0.0 {
                parts.push(format!("{k}={v:+}"));
            }
        }
        if self.noise > 0.0 {
            parts.push(format!("noise={}", self.noise));
        }
        if parts.is_empty() {
            "none".into()
        } else {
            parts.join(" ")
        }
    }
}

fn parse_recipe(line: &str) -> Result<(String, Recipe), String> {
    let mut words = line.split_whitespace();
    let name = words.next().ok_or("an empty recipe")?.to_string();
    let mut r = Recipe::default();
    for w in words {
        let (k, v) = w
            .split_once('=')
            .ok_or_else(|| format!("{line}: '{w}' is not knob=value"))?;
        let num: f32 = v.parse().map_err(|_| format!("{line}: bad number '{v}'"))?;
        match k {
            "noise" => r.noise = num,
            "seed" => r.seed = Some(num as u64),
            _ => {
                *knob_mut(&mut r.nudge, k).ok_or_else(|| {
                    format!("{line}: unknown knob '{k}' (knobs: {})", KNOBS.join(" "))
                })? = num;
            }
        }
    }
    Ok((name, r))
}

fn parse_list(s: &str) -> Result<Vec<f32>, String> {
    s.split(',')
        .map(|x| x.trim().parse().map_err(|_| format!("bad number '{x}'")))
        .collect()
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let out: PathBuf = args
        .next()
        .ok_or("usage: make_variants OUT_DIR --parent NAME=CHECKPOINT [options]")?
        .into();
    let mut parents: Vec<(String, PathBuf)> = Vec::new();
    let mut auto = 0usize;
    let mut recipe_file: Option<PathBuf> = None;
    let mut strengths = vec![1.0f32, 2.0, 4.0];
    let mut noises = vec![0.01f32, 0.03, 0.1];
    let mut seed = 1u64;
    while let Some(a) = args.next() {
        let mut v = |n: &str| args.next().ok_or(format!("{n} needs a value"));
        match a.as_str() {
            "--parent" => {
                let s = v("--parent")?;
                let (n, p) = s.split_once('=').ok_or("--parent wants NAME=CHECKPOINT")?;
                parents.push((n.to_string(), p.into()));
            }
            "--auto" => auto = v("--auto")?.parse().map_err(|_| "bad number")?,
            "--recipes" => recipe_file = Some(v("--recipes")?.into()),
            "--strengths" => strengths = parse_list(&v("--strengths")?)?,
            "--noise" => noises = parse_list(&v("--noise")?)?,
            "--seed" => seed = v("--seed")?.parse().map_err(|_| "bad number")?,
            other => return Err(format!("unknown option {other}")),
        }
    }
    if parents.is_empty() {
        return Err("give at least one --parent NAME=CHECKPOINT".into());
    }
    if strengths.is_empty() || noises.is_empty() {
        return Err("--strengths and --noise need at least one number".into());
    }
    if out.join("entries.txt").exists() {
        return Err(format!(
            "{} already has an entries.txt; pick a new folder (nothing is overwritten)",
            out.display()
        ));
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let content =
        f4x_data::load_content_layers(&[Path::new("content")]).map_err(|e| e.to_string())?;
    let spec = ObsSpec::new(&content);
    let device = Device::flex();
    let fixed: Vec<(String, Recipe)> = match &recipe_file {
        Some(f) => std::fs::read_to_string(f)
            .map_err(|e| format!("{}: {e}", f.display()))?
            .lines()
            .map(|l| l.split('#').next().unwrap_or("").trim())
            .filter(|l| !l.is_empty())
            .map(parse_recipe)
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };

    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    let mut manifest = String::new();
    let mut made = Vec::new();
    for (pname, path) in &parents {
        let base = load_checkpoint(&spec, path, &device)?;
        // The parents themselves, first.
        let file = out.join(pname);
        save_checkpoint(base.clone(), &spec, &file)?;
        manifest.push_str(&format!(
            "{pname}\t{}\t-\tparent ({})\n",
            format_args!("{}.bpk", file.display()),
            path.display()
        ));
        let mut recipes: Vec<(String, Recipe)> = fixed.clone();
        for k in 0..auto {
            let mut r = Recipe::default();
            for _ in 0..rng.gen_range(1..=2) {
                let knob = KNOBS[rng.gen_range(0..KNOBS.len())];
                let sign = if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
                let s = strengths[rng.gen_range(0..strengths.len())];
                *knob_mut(&mut r.nudge, knob).unwrap() = sign * s;
            }
            if rng.gen_bool(0.5) {
                r.noise = noises[rng.gen_range(0..noises.len())];
            }
            recipes.push((format!("v{:02}", k + 1), r));
        }
        for (vname, r) in recipes {
            let name = format!("{pname}-{vname}");
            let mut noise_rng = Pcg64Mcg::seed_from_u64(r.seed.unwrap_or_else(|| rng.gen()));
            let net: PolicyNet = base
                .clone()
                .nudged(&r.nudge)
                .jittered(r.noise, &mut noise_rng);
            let file = out.join(&name);
            save_checkpoint(net, &spec, &file)?;
            manifest.push_str(&format!(
                "{name}\t{}\t{pname}\t{}\n",
                format_args!("{}.bpk", file.display()),
                r.describe()
            ));
            made.push(name);
        }
    }
    std::fs::write(out.join("entries.txt"), manifest).map_err(|e| e.to_string())?;
    println!(
        "{} parents and {} variants in {}; entries in {}",
        parents.len(),
        made.len(),
        out.display(),
        out.join("entries.txt").display()
    );
    Ok(())
}
