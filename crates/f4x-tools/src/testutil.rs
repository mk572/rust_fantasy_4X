//! The shipped content for the tests, read once per run.
//!
//! Tests: it is itself test support; its users are the tests of `metrics` and `screens`.

use f4x_def::content::Content;

pub fn shipped() -> &'static Content {
    f4x_def::testdir::shipped()
}
