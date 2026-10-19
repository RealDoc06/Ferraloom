use super::distance_manager::{FullChunkStatus, MAX_LOADED_LEVEL, full_status};
use ferraloom_util::ChunkPos;
/// The native 26.3 status chain: TERRAIN combines noise, surface and carvers.
pub use ferraloom_worldgen::region::ChunkStatus;
/// Port of GenerationChunkHolder's request identity and status visibility.
#[derive(Debug)]
pub struct ChunkHolder {
    pub position: ChunkPos,
    pub generation: super::generation::HolderGeneration,
    /// ChunkMap-assigned incarnation, stable across requests and status changes.
    /// Zero is reserved for standalone holders outside a canonical ChunkMap.
    pub epoch: u64,
    pub ticket_level: i32,
    old_ticket_level: i32,
    confirmed_full_status: FullChunkStatus,
    full_status_generation: u64,
    pending_full_status: Option<FullStatusPromotion>,
}
/// Cancellable confirmation for ChunkHolder#scheduleFullChunkPromotion. Only
/// the pipeline that has satisfied the required chunk-range future confirms it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullStatusPromotion {
    pub status: FullChunkStatus,
    generation: u64,
}
impl ChunkHolder {
    pub fn new(position: ChunkPos) -> Self {
        Self {
            position,
            generation: Default::default(),
            epoch: 0,
            ticket_level: MAX_LOADED_LEVEL + 1,
            old_ticket_level: MAX_LOADED_LEVEL + 1,
            confirmed_full_status: FullChunkStatus::Inaccessible,
            full_status_generation: 0,
            pending_full_status: None,
        }
    }
    /// Desired status from loading tickets, independently of generation and
    /// independently of the simulation tracker.
    pub fn requested_full_status(&self) -> FullChunkStatus {
        full_status(self.ticket_level)
    }
    pub fn confirmed_full_status(&self) -> FullChunkStatus {
        self.confirmed_full_status
    }
    /// Port of updateFutures' full-status transitions. Call after all distance
    /// changes have updated holder ticket levels. Demotions notify immediately;
    /// promotions wait for the corresponding accessible/ticking range future.
    /// Later transitions cancel outstanding confirmation tokens.
    pub fn update_full_status(&mut self) -> Option<FullChunkStatus> {
        let old = full_status(self.old_ticket_level);
        let new = self.requested_full_status();
        self.old_ticket_level = self.ticket_level;
        if old == new {
            return None;
        }
        self.full_status_generation = self.full_status_generation.wrapping_add(1);
        self.pending_full_status = None;
        if new > old {
            self.pending_full_status = Some(FullStatusPromotion {
                status: new,
                generation: self.full_status_generation,
            });
            None
        } else {
            self.confirmed_full_status = new;
            Some(new)
        }
    }
    pub fn pending_full_status(&self) -> Option<FullStatusPromotion> {
        self.pending_full_status
    }
    /// Complete only the latest successful range future. A stale IO completion
    /// cannot promote a demoted or newly rescheduled holder.
    pub fn confirm_full_status(
        &mut self,
        promotion: FullStatusPromotion,
    ) -> Option<FullChunkStatus> {
        if self.pending_full_status != Some(promotion) {
            return None;
        }
        self.pending_full_status = None;
        self.confirmed_full_status = promotion.status;
        Some(promotion.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_status_promotion_waits_for_its_current_range_future_and_demotes_immediately() {
        let mut holder = ChunkHolder::new(ChunkPos::new(0, 0));
        holder.ticket_level = 31;
        assert_eq!(holder.update_full_status(), None);
        let first = holder.pending_full_status().unwrap();
        assert_eq!(
            holder.confirmed_full_status(),
            FullChunkStatus::Inaccessible
        );
        holder.ticket_level = 33;
        assert_eq!(holder.update_full_status(), Some(FullChunkStatus::Full));
        assert_eq!(holder.confirm_full_status(first), None);
        holder.ticket_level = 31;
        assert_eq!(holder.update_full_status(), None);
        let second = holder.pending_full_status().unwrap();
        assert_eq!(holder.confirm_full_status(first), None);
        assert_eq!(
            holder.confirm_full_status(second),
            Some(FullChunkStatus::EntityTicking)
        );
        holder.ticket_level = 32;
        assert_eq!(
            holder.update_full_status(),
            Some(FullChunkStatus::BlockTicking)
        );
        holder.ticket_level = 45;
        assert_eq!(
            holder.update_full_status(),
            Some(FullChunkStatus::Inaccessible)
        );
    }
}
