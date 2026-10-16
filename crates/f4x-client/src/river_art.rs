//! The six river pictures of a battlefield (shape: left-right, up-down, up-left, up-right,
//! down-right, down-left; five frames each), embedded in the client. They are the second variant
//! of the original's `cmbtcity` entries 103 to 108, saved as PNG files in `assets/rivers/`
//! (committed, at the project owner's choice). The first variant (entries 97 to 102) and Myrror's
//! (`chriver` 0 to 11) are empty in the original's files.

/// Frames of each picture.
pub const FRAMES: usize = 5;

/// `river_<shape>_<frame>.png`, shape by shape.
const FILES: [&[u8]; 30] = [
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

/// The PNG bytes of a river picture; `frame` wraps round.
pub fn png(shape: usize, frame: usize) -> Option<&'static [u8]> {
    (shape < 6).then(|| FILES[shape * FRAMES + frame % FRAMES])
}
