//! Glowing entities (`EntityRenderer.extractRenderState`'s outlineColor): which entities
//! appear glowing, in which team colour, and their silhouettes for the renderer's entity
//! outline (`OutlineBufferSource`).
use super::Game;

impl Game<'_> {
    /// `Entity.isCurrentlyGlowing` of an entity that is not living (an item, a projectile):
    /// its `Glowing` tag, or shared flag 6 as a server sent it.
    pub(super) fn entity_glowing(&self, id: u32) -> bool {
        self.persist_raw(id)
            .and_then(|t| horizon_client::save::number(t.get("Glowing")))
            .is_some_and(|b| b != 0)
    }
}
