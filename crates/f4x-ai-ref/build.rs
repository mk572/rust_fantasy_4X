// Written by scripts/fork_ai.py. A stale reference is not built: fork it again.
fn main() {
    println!("cargo:rerun-if-changed=STALE");
    if std::path::Path::new("STALE").exists() {
        panic!(
            "f4x-ai-ref is stale (crates/f4x-ai-ref/STALE): run `python3 scripts/ai_battle.py ...`              or `python3 scripts/fork_ai.py`"
        );
    }
}
