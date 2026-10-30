//! The read handle on one player: what its components say, and its wizard when it has one.
//!
//! Tests: none, because it only reads components, which every game test goes through.

use crate::player::{
    Control, Defeat, GrandVizier, History, Neutral, PlayerId, Treasury, WizardLink, NEUTRAL_BANNER,
};
use crate::wizard::entity::Wizard;

/// A read handle on one player entity. Made by [`crate::game::Game::player`].
#[derive(Clone, Copy)]
pub struct Player<'a> {
    world: &'a hecs::World,
    entity: hecs::Entity,
    id: PlayerId,
}

impl<'a> Player<'a> {
    pub(crate) fn new(world: &'a hecs::World, entity: hecs::Entity, id: PlayerId) -> Self {
        Self { world, entity, id }
    }

    pub fn id(&self) -> PlayerId {
        self.id
    }

    pub fn entity(&self) -> hecs::Entity {
        self.entity
    }

    /// A component of the player.
    pub fn component<C: hecs::Component>(&self) -> Option<hecs::Ref<'a, C>> {
        self.world.get::<&C>(self.entity).ok()
    }

    /// The player's gold and mana.
    pub fn treasury(&self) -> Treasury {
        self.component::<Treasury>()
            .map_or(Treasury { gold: 0, mana: 0 }, |t| *t)
    }

    /// The monsters' owner or an independent city's player: no wizard.
    pub fn is_neutral(&self) -> bool {
        self.component::<Neutral>().is_some()
    }

    /// Whether the human at the screen or the AI decides for the player.
    pub fn control(&self) -> Option<Control> {
        self.component::<Control>().map(|c| *c)
    }

    /// The player's wizard, if it has one.
    pub fn wizard(&self) -> Option<Wizard<'a>> {
        let link = self.component::<WizardLink>()?;
        Some(Wizard::new(self.world, link.0))
    }

    /// How the player is called: its wizard's name, or its (oldest) city's.
    pub fn name(&self) -> String {
        crate::wizard::entity::player_name(self.world, self.id)
    }

    /// The banner colour; the neutral banner if the player has no wizard.
    pub fn banner(&self) -> u8 {
        self.wizard().map_or(NEUTRAL_BANNER, |w| w.banner())
    }

    /// The wizard picture; 0 if the player has no wizard.
    pub fn portrait(&self) -> u8 {
        self.wizard().map_or(0, |w| w.portrait())
    }

    /// The wizard's power at each turn, for the History screen.
    pub fn history(&self) -> Vec<u8> {
        self.component::<History>()
            .map_or_else(Vec::new, |h| h.0.clone())
    }

    /// Does the player's Grand Vizier choose what its cities build next?
    pub fn has_grand_vizier(&self) -> bool {
        self.component::<GrandVizier>().is_some()
    }

    /// Has the player lost everything?
    pub fn is_defeated(&self) -> bool {
        self.component::<Defeat>().is_some()
    }

    /// Who took the last city of a defeated player.
    pub fn defeated_by(&self) -> Option<PlayerId> {
        self.component::<Defeat>()?.by
    }
}
