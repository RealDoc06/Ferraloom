//! Ordered integration points for subsystem ports. Effects are captured at the
//! vanilla call site and consumed at its phase boundary, never from hash order.
use ferraloom_data::BlockState;
use ferraloom_util::{BlockPos, ChunkPos};

#[derive(Clone, Debug, PartialEq)]
pub enum LevelEffect {
    /// Accepted transition captured at setEntityPos, including A -> B -> A.
    PlayerSectionChange {
        player: crate::entity::EntityRef,
        section: ferraloom_util::SectionPos,
    },
    SectionEmptiness {
        chunk: ChunkPos,
        section_y: i32,
        empty: bool,
    },
    CheckLight(BlockPos),
    PoiChanged {
        pos: BlockPos,
        old: BlockState,
        new: BlockState,
    },
    BlockUpdated {
        pos: BlockPos,
        old: BlockState,
        new: BlockState,
    },
    DestroyParticles {
        pos: BlockPos,
        state: BlockState,
    },
    BlockDestroyed {
        pos: BlockPos,
        state: BlockState,
    },
    LevelEvent {
        pos: BlockPos,
        id: i32,
        data: i32,
    },
    WeatherEvent {
        event: u8,
        param: f32,
    },
    PendingPrecipitation(BlockPos),
    /// Audience is captured at the call site, before subsequent movement or
    /// removal; the runtime appends to each connection's ordinary outbound stream.
    PlayerPacket {
        recipients: Vec<crate::entity::EntityRef>,
        packet: Box<ferraloom_protocol::play::clientbound::Packet>,
    },
    /// Native PlayerList#broadcastAll, ordered with individual player output.
    BroadcastAll(Box<ferraloom_protocol::play::clientbound::Packet>),
}

impl super::Level {
    /// Publish queued canonical ServerScoreboard output at its native producer
    /// point; each connection observes these between surrounding player packets.
    pub fn emit_scoreboard_packets(&mut self) -> Result<(), super::LevelError> {
        let packets = self.shared_mut()?.scoreboard.drain_packets();
        for packet in packets {
            self.emit(LevelEffect::BroadcastAll(Box::new(packet)));
        }
        Ok(())
    }

    /// Consume every captured accepted player transition in source order. The
    /// listener invokes this before synchronous ticket-dependent work.
    pub fn take_player_section_changes(
        &mut self,
    ) -> Vec<(crate::entity::EntityRef, ferraloom_util::SectionPos)> {
        let mut changes = Vec::new();
        self.region_mut().effects.retain(|effect| {
            if let LevelEffect::PlayerSectionChange { player, section } = effect {
                changes.push((*player, *section));
                false
            } else {
                true
            }
        });
        changes
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn captured_player_sections_preserve_intermediate_transitions() {
        use super::*;
        use crate::{
            Behaviors,
            entity::EntityRef,
            level::{Level, Region},
        };
        use ferraloom_util::SectionPos;
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let player = EntityRef(1);
        let a = SectionPos::new(0, 4, 0);
        let b = SectionPos::new(1, 4, 0);
        for section in [a, b, a] {
            level.emit(LevelEffect::PlayerSectionChange { player, section });
            level.emit(LevelEffect::CheckLight(BlockPos::ZERO));
        }
        assert_eq!(
            level.take_player_section_changes(),
            vec![(player, a), (player, b), (player, a)]
        );
        assert_eq!(
            level.take_effects(),
            vec![LevelEffect::CheckLight(BlockPos::ZERO); 3]
        );
    }
}
