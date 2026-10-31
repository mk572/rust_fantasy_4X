//! The six river pictures of a battlefield (shape: left-right, up-down, up-left, up-right,
//! down-right, down-left; five frames each), embedded in the client, for Arcanus and for Myrror.
//!
//! Arcanus's are the second variant of the original's `cmbtcity` entries 103 to 108, saved as PNG
//! files in `assets/rivers/` (`river_<shape>_<frame>.png`). The first variant (entries 97 to 102)
//! and Myrror's own (`chriver` 0 to 11) are empty in the original's files. Myrror's
//! (`river_myrror_<shape>_<frame>.png`) were made once from Arcanus's, by repainting the blue
//! water in the mean colour of Myrror's ocean (`chriver` 12, purple) and the grassy banks in that
//! of its ground (`cmbgrasc` 0), each pixel keeping its share of light, and saved as files. Both
//! sets are committed, at the project owner's choice.

/// Frames of each picture.
pub const FRAMES: usize = 5;

const ARCANUS: [&[u8]; 30] = [
    include_bytes!("../assets/rivers/river_0_0.png"),
    include_bytes!("../assets/rivers/river_0_1.png"),
    include_bytes!("../assets/rivers/river_0_2.png"),
    include_bytes!("../assets/rivers/river_0_3.png"),
    include_bytes!("../assets/rivers/river_0_4.png"),
    include_bytes!("../assets/rivers/river_1_0.png"),
    include_bytes!("../assets/rivers/river_1_1.png"),
    include_bytes!("../assets/rivers/river_1_2.png"),
    include_bytes!("../assets/rivers/river_1_3.png"),
    include_bytes!("../assets/rivers/river_1_4.png"),
    include_bytes!("../assets/rivers/river_2_0.png"),
    include_bytes!("../assets/rivers/river_2_1.png"),
    include_bytes!("../assets/rivers/river_2_2.png"),
    include_bytes!("../assets/rivers/river_2_3.png"),
    include_bytes!("../assets/rivers/river_2_4.png"),
    include_bytes!("../assets/rivers/river_3_0.png"),
    include_bytes!("../assets/rivers/river_3_1.png"),
    include_bytes!("../assets/rivers/river_3_2.png"),
    include_bytes!("../assets/rivers/river_3_3.png"),
    include_bytes!("../assets/rivers/river_3_4.png"),
    include_bytes!("../assets/rivers/river_4_0.png"),
    include_bytes!("../assets/rivers/river_4_1.png"),
    include_bytes!("../assets/rivers/river_4_2.png"),
    include_bytes!("../assets/rivers/river_4_3.png"),
    include_bytes!("../assets/rivers/river_4_4.png"),
    include_bytes!("../assets/rivers/river_5_0.png"),
    include_bytes!("../assets/rivers/river_5_1.png"),
    include_bytes!("../assets/rivers/river_5_2.png"),
    include_bytes!("../assets/rivers/river_5_3.png"),
    include_bytes!("../assets/rivers/river_5_4.png"),
];

const MYRROR: [&[u8]; 30] = [
    include_bytes!("../assets/rivers/river_myrror_0_0.png"),
    include_bytes!("../assets/rivers/river_myrror_0_1.png"),
    include_bytes!("../assets/rivers/river_myrror_0_2.png"),
    include_bytes!("../assets/rivers/river_myrror_0_3.png"),
    include_bytes!("../assets/rivers/river_myrror_0_4.png"),
    include_bytes!("../assets/rivers/river_myrror_1_0.png"),
    include_bytes!("../assets/rivers/river_myrror_1_1.png"),
    include_bytes!("../assets/rivers/river_myrror_1_2.png"),
    include_bytes!("../assets/rivers/river_myrror_1_3.png"),
    include_bytes!("../assets/rivers/river_myrror_1_4.png"),
    include_bytes!("../assets/rivers/river_myrror_2_0.png"),
    include_bytes!("../assets/rivers/river_myrror_2_1.png"),
    include_bytes!("../assets/rivers/river_myrror_2_2.png"),
    include_bytes!("../assets/rivers/river_myrror_2_3.png"),
    include_bytes!("../assets/rivers/river_myrror_2_4.png"),
    include_bytes!("../assets/rivers/river_myrror_3_0.png"),
    include_bytes!("../assets/rivers/river_myrror_3_1.png"),
    include_bytes!("../assets/rivers/river_myrror_3_2.png"),
    include_bytes!("../assets/rivers/river_myrror_3_3.png"),
    include_bytes!("../assets/rivers/river_myrror_3_4.png"),
    include_bytes!("../assets/rivers/river_myrror_4_0.png"),
    include_bytes!("../assets/rivers/river_myrror_4_1.png"),
    include_bytes!("../assets/rivers/river_myrror_4_2.png"),
    include_bytes!("../assets/rivers/river_myrror_4_3.png"),
    include_bytes!("../assets/rivers/river_myrror_4_4.png"),
    include_bytes!("../assets/rivers/river_myrror_5_0.png"),
    include_bytes!("../assets/rivers/river_myrror_5_1.png"),
    include_bytes!("../assets/rivers/river_myrror_5_2.png"),
    include_bytes!("../assets/rivers/river_myrror_5_3.png"),
    include_bytes!("../assets/rivers/river_myrror_5_4.png"),
];

/// The PNG bytes of a river picture; `frame` wraps round.
pub fn png(shape: usize, frame: usize, myrror: bool) -> Option<&'static [u8]> {
    let files = if myrror { &MYRROR } else { &ARCANUS };
    (shape < 6).then(|| files[shape * FRAMES + frame % FRAMES])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_of_the_six_shapes_has_five_frames_that_wrap_for_both_planes() {
        for myrror in [false, true] {
            for shape in 0..6 {
                let first = png(shape, 0, myrror);
                assert!(first.is_some_and(|b| b.starts_with(b"\x89PNG")), "{shape}");
                assert_eq!(
                    png(shape, FRAMES, myrror),
                    first,
                    "frame {FRAMES} wraps to 0"
                );
                assert_ne!(png(shape, 1, myrror), first);
            }
        }
        assert_eq!(png(6, 0, false), None);
        assert_ne!(
            png(0, 0, false),
            png(0, 0, true),
            "Myrror's water is its own"
        );
    }
}
