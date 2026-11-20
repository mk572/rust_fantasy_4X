//! The two sides of a battle.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Side {
    Attacker,
    Defender,
}

impl Side {
    pub fn index(self) -> usize {
        match self {
            Side::Attacker => 0,
            Side::Defender => 1,
        }
    }

    pub fn other(self) -> Side {
        match self {
            Side::Attacker => Side::Defender,
            Side::Defender => Side::Attacker,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_other_side_is_the_opposite_and_the_indexes_differ() {
        for side in [Side::Attacker, Side::Defender] {
            assert_ne!(side, side.other());
            assert_eq!(side, side.other().other());
            assert_ne!(side.index(), side.other().index());
        }
    }
}
