//! The effects the engine reads from a wizard's global enchantments, by the `global_effect` hook.
//!
//! Tests: in `f4x-core`'s hook.rs (every effect is given by a shipped spell).

/// The hook global spells carry when the engine reads them from the wizard's spells instead of
/// running anything when they are cast.
pub const GLOBAL_EFFECT: &str = "global_effect";

crate::effect_enum! {
    /// The effects the engine reads from a wizard's global spells: the arguments of the
    /// `global_effect` hook (`Custom(name: "global_effect", args: ["crusade"])`). A spell carries
    /// the effects it has; the code asks for the effect, never for a spell number, so a mod can
    /// put an effect on another spell.
    pub enum GlobalEffect in GLOBAL_EFFECT;
    NatureAwareness = "nature_awareness",
    HerbMastery = "herb_mastery",
    NaturesWrath = "natures_wrath",
    WindMastery = "wind_mastery",
    AuraOfMajesty = "aura_of_majesty",
    SuppressMagic = "suppress_magic",
    TimeStop = "time_stop",
    MeteorStorm = "meteor_storm",
    GreatWasting = "great_wasting",
    ChaosSurge = "chaos_surge",
    DoomMastery = "doom_mastery",
    Armageddon = "armageddon",
    JustCause = "just_cause",
    PlanarSeal = "planar_seal",
    HolyArms = "holy_arms",
    LifeForce = "life_force",
    Tranquility = "tranquility",
    Crusade = "crusade",
    CharmOfLife = "charm_of_life",
    ZombieMastery = "zombie_mastery",
    EternalNight = "eternal_night",
    EvilOmens = "evil_omens",
    DetectMagic = "detect_magic",
    Awareness = "awareness",
}
