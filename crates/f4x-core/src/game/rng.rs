//! The game's random numbers: one seeded generator that counts what it hands out.
//!
//! All randomness in the rules comes from a [`GameRng`] (the base rule in `design.md`: logic takes
//! the pieces it needs). It is a struct of its own, a field of `Game`, so a domain can take
//! `&mut GameRng` next to other borrows of the game. The count of draws is what a save keeps: a
//! loaded game replays that many values from the seed to resume the stream
//! ([`GameRng::resume`]).

use rand::{Error, RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;
use serde::{Deserialize, Serialize};

/// What is written of a generator: where it started and how far it has gone.
#[derive(Serialize, Deserialize)]
struct RngState {
    seed: u64,
    draws: u64,
}

impl From<GameRng> for RngState {
    fn from(rng: GameRng) -> Self {
        Self {
            seed: rng.seed,
            draws: rng.draws,
        }
    }
}

impl From<RngState> for GameRng {
    fn from(state: RngState) -> Self {
        Self::resume(state.seed, state.draws)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "RngState", from = "RngState")]
pub struct GameRng {
    seed: u64,
    /// Values drawn so far.
    draws: u64,
    inner: Pcg64Mcg,
    /// Stands in the game while the real generator is lent out for a call (spell resolution,
    /// diplomacy): drawing from it would be a draw that is not part of the game's stream, so a
    /// debug build stops at it.
    lent: bool,
}

impl GameRng {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            draws: 0,
            inner: Pcg64Mcg::seed_from_u64(seed),
            lent: false,
        }
    }

    /// A stand-in for the generator while it is lent out (see the `lent` field).
    pub(crate) fn lent() -> Self {
        Self {
            lent: true,
            ..Self::new(0)
        }
    }

    /// The generator of a game that has drawn `draws` values from `seed` already.
    pub fn resume(seed: u64, draws: u64) -> Self {
        let mut rng = Self::new(seed);
        for _ in 0..draws {
            rng.inner.next_u64();
        }
        rng.draws = draws;
        rng
    }

    /// The seed the game started from.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// How many values have been drawn.
    pub fn draws(&self) -> u64 {
        self.draws
    }

    fn check_not_lent(&self) {
        debug_assert!(
            !self.lent,
            "the game's generator is lent out: draw from the one the call was given"
        );
    }

    /// Uniform random integer in `1..=n`.
    pub fn roll(&mut self, n: u32) -> u32 {
        (self.next_u64() % u64::from(n.max(1))) as u32 + 1
    }
}

impl RngCore for GameRng {
    fn next_u32(&mut self) -> u32 {
        self.check_not_lent();
        self.draws += 1;
        self.inner.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.check_not_lent();
        self.draws += 1;
        self.inner.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.check_not_lent();
        self.draws += 1;
        self.inner.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        self.check_not_lent();
        self.draws += 1;
        self.inner.try_fill_bytes(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_stream() {
        let (mut a, mut b) = (GameRng::new(42), GameRng::new(42));
        for _ in 0..5 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(GameRng::new(1).next_u64(), GameRng::new(2).next_u64());
    }

    #[test]
    fn draws_are_counted_and_a_resumed_stream_continues_where_it_stopped() {
        let mut rng = GameRng::new(7);
        rng.next_u64();
        rng.roll(6);
        rng.next_u32();
        assert_eq!(rng.draws(), 3);
        let mut back = GameRng::resume(7, rng.draws());
        assert_eq!((back.seed(), back.draws()), (7, 3));
        for _ in 0..5 {
            assert_eq!(rng.next_u64(), back.next_u64());
        }
    }

    #[test]
    fn a_roll_stays_in_range_and_a_clone_replays_the_same_values() {
        let mut rng = GameRng::new(3);
        let mut copy = rng.clone();
        for _ in 0..200 {
            let r = rng.roll(6);
            assert!((1..=6).contains(&r));
            assert_eq!(r, copy.roll(6));
        }
        assert_eq!(rng.roll(0), 1, "a die with no sides rolls 1, as before");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "lent out")]
    fn a_lent_generator_cannot_be_drawn_from() {
        GameRng::lent().roll(6);
    }
}
