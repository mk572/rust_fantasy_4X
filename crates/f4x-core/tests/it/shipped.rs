//! The shipped content, for the tests of this folder.

use f4x_def::content::Content;

pub fn content() -> Content {
    f4x_def::testdir::shipped().clone()
}
