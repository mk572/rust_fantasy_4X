//! Class 3: the content drifts without anyone intending it. The record `content.sha256` holds the
//! files, and the guard that compares them must itself see a changed, an added and a removed file.

use f4x_def::content_hash::{assert_unchanged, differences, hashes_of, recorded, RECORD};
use f4x_def::testdir::TestDir;
use std::path::Path;

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A folder with one content file and its record, as a crate has them.
fn small_crate(label: &str) -> TestDir {
    let dir = TestDir::new(label);
    std::fs::create_dir_all(dir.join("content")).expect("a folder");
    std::fs::write(dir.join("content/a.ron"), "(1)").expect("a file");
    let hashes = hashes_of(&dir).expect("hashes");
    let record: String = hashes.iter().map(|(p, h)| format!("{h}  {p}\n")).collect();
    std::fs::write(dir.join(RECORD), record).expect("the record");
    dir
}

#[test]
fn the_shipped_files_are_the_recorded_ones() {
    assert_unchanged(crate_dir());
}

#[test]
fn every_shipped_file_is_in_the_record_and_the_record_names_no_other() {
    let files = hashes_of(crate_dir()).expect("hashes");
    let record = recorded(crate_dir()).expect("record");
    assert!(!files.is_empty(), "the crate has content files");
    assert_eq!(files, record);
}

#[test]
fn a_crate_that_matches_its_record_has_no_difference() {
    let dir = small_crate("drift-same");
    assert_eq!(differences(&dir).expect("compared"), Vec::<String>::new());
    assert_unchanged(&dir);
}

#[test]
fn a_changed_file_is_reported_by_its_path() {
    let dir = small_crate("drift-changed");
    std::fs::write(dir.join("content/a.ron"), "(2)").expect("a file");
    let d = differences(&dir).expect("compared");
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(
        d[0].contains("content/a.ron") && d[0].contains("changed"),
        "{d:?}"
    );
}

#[test]
fn an_added_file_is_reported_by_its_path() {
    let dir = small_crate("drift-added");
    std::fs::write(dir.join("content/b.ron"), "()").expect("a file");
    let d = differences(&dir).expect("compared");
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(
        d[0].contains("content/b.ron") && d[0].contains("added"),
        "{d:?}"
    );
}

#[test]
fn a_removed_file_is_reported_by_its_path() {
    let dir = small_crate("drift-removed");
    std::fs::remove_file(dir.join("content/a.ron")).expect("removed");
    let d = differences(&dir).expect("compared");
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(
        d[0].contains("content/a.ron") && d[0].contains("removed"),
        "{d:?}"
    );
}

#[test]
#[should_panic(expected = "content.sha256")]
fn the_guard_fails_when_a_file_differs() {
    let dir = small_crate("drift-guard");
    std::fs::write(dir.join("content/a.ron"), "(3)").expect("a file");
    assert_unchanged(&dir);
}

#[test]
fn one_changed_byte_changes_the_hash() {
    let dir = small_crate("drift-byte");
    let before = hashes_of(&dir).expect("hashes");
    std::fs::write(dir.join("content/a.ron"), "(1) ").expect("a file");
    assert_ne!(before, hashes_of(&dir).expect("hashes"));
}

#[test]
fn only_ron_files_count_and_target_folders_are_left_out() {
    let dir = small_crate("drift-kinds");
    std::fs::write(dir.join("content/notes.txt"), "x").expect("a file");
    std::fs::create_dir_all(dir.join("target")).expect("a folder");
    std::fs::write(dir.join("target/b.ron"), "x").expect("a file");
    assert_eq!(differences(&dir).expect("compared"), Vec::<String>::new());
}

#[test]
fn files_in_nested_folders_are_found() {
    let dir = small_crate("drift-nested");
    std::fs::create_dir_all(dir.join("content/deep/er")).expect("folders");
    std::fs::write(dir.join("content/deep/er/c.ron"), "()").expect("a file");
    let d = differences(&dir).expect("compared");
    assert!(
        d.iter().any(|l| l.contains("content/deep/er/c.ron")),
        "{d:?}"
    );
}

#[test]
fn a_missing_or_damaged_record_is_an_error_not_a_pass() {
    let dir = small_crate("drift-record");
    std::fs::remove_file(dir.join(RECORD)).expect("removed");
    assert!(differences(&dir).is_err(), "no record must not pass");
    std::fs::write(dir.join(RECORD), "this line has no hash and path\n").expect("a record");
    assert!(differences(&dir).is_err(), "a damaged line must not pass");
}

#[test]
fn an_empty_record_reports_every_file_as_added() {
    let dir = small_crate("drift-empty");
    std::fs::write(dir.join(RECORD), "").expect("a record");
    let d = differences(&dir).expect("compared");
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].contains("added"), "{d:?}");
}

#[test]
fn a_folder_that_does_not_exist_is_an_error() {
    let dir = TestDir::new("drift-none");
    assert!(hashes_of(&dir.join("not-there")).is_err());
}

#[cfg(unix)]
#[test]
fn a_link_that_leads_back_up_does_not_make_the_guard_run_for_ever() {
    let dir = small_crate("drift-loop");
    std::os::unix::fs::symlink(&*dir, dir.join("content/back")).expect("a link");
    // A link to a folder is not followed: the files it shows are no files of the crate.
    assert_eq!(differences(&dir).expect("compared"), Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn a_link_to_a_file_outside_the_crate_is_not_a_file_of_the_crate() {
    let dir = small_crate("drift-outside");
    let outside = TestDir::new("drift-elsewhere");
    std::fs::write(outside.join("x.ron"), "()").expect("a file");
    std::os::unix::fs::symlink(outside.join("x.ron"), dir.join("content/x.ron")).expect("a link");
    let d = differences(&dir).expect("compared");
    assert!(d.iter().all(|l| !l.contains("x.ron")), "{d:?}");
}
