//! The `.ron` files of the crate are the ones recorded in `content.sha256` (`f4x_def::content_hash`).

#[test]
fn the_ron_files_are_the_recorded_ones() {
    f4x_def::content_hash::assert_unchanged(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
}
