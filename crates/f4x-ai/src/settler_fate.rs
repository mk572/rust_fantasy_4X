//! Why a settler's plan failed. The 100-turn clock used to count every dropped settler plan as one
//! number; the soak needs to know which of them it was: a settler that was never picked up by a
//! ship is a different fault from one whose site a rival took first. Each place that ends a
//! settler's plan says why, and the soak prints the counts by reason (`Ai::settler_failures`).
//!
//! A settler that is lost in battle or let go for lack of food or gold leaves the AI's view
//! without a word; the soak sees those from the game's events (`f4x-tools`, `metrics.rs`).
//!
//! Tests: in `plan_clock.rs`, `dispatch.rs` and `expand.rs`, where each reason is noted.

/// How a settler's plan ended without a city.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettlerFate {
    /// It waited for a ship (`Crossing::Sea`) and none took it aboard in time.
    NoShip,
    /// It walked for the whole time (`SETTLER_TIMEOUT_TURNS`) and did not get there.
    TooSlow,
    /// Another city stands on the site: a rival settled first.
    SiteTaken,
    /// The site is no longer a place for a city (found on arrival, or given up as a bad site).
    SiteUnusable,
    /// The way to the site is blocked: the settler did not move, or no walk could be ordered.
    WayBlocked,
    /// No explored square will do for a city (counted for each turn the settler has none).
    NoSite,
}

impl SettlerFate {
    pub const ALL: [SettlerFate; 6] = [
        SettlerFate::NoShip,
        SettlerFate::TooSlow,
        SettlerFate::SiteTaken,
        SettlerFate::SiteUnusable,
        SettlerFate::WayBlocked,
        SettlerFate::NoSite,
    ];

    /// The words of the soak's line.
    pub fn name(self) -> &'static str {
        match self {
            SettlerFate::NoShip => "never picked up by a ship",
            SettlerFate::TooSlow => "did not arrive in time",
            SettlerFate::SiteTaken => "site settled by someone else",
            SettlerFate::SiteUnusable => "site not fit for a city",
            SettlerFate::WayBlocked => "way to the site blocked",
            SettlerFate::NoSite => "turns with no suitable site",
        }
    }
}

/// How often each reason has ended a settler's plan.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SettlerFates {
    counts: Vec<u32>,
}

impl SettlerFates {
    pub fn note(&mut self, fate: SettlerFate) {
        let i = fate as usize;
        if self.counts.len() <= i {
            self.counts.resize(SettlerFate::ALL.len(), 0);
        }
        self.counts[i] += 1;
    }

    /// Every reason with its count, in a fixed order.
    pub fn counts(&self) -> Vec<(&'static str, u32)> {
        SettlerFate::ALL
            .iter()
            .map(|&f| (f.name(), self.counts.get(f as usize).copied().unwrap_or(0)))
            .collect()
    }
}
