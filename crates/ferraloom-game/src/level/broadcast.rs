//! Port of ChunkHolder#blockChanged / broadcastChanges. Tracking audiences and
//! socket writes belong to the server; capture packet states at broadcast time.
use super::{Level, LevelError, effects::LevelEffect};
use ferraloom_data::BlockState;
use ferraloom_protocol::play::{
    BlockStateId, SectionBlockChange, SectionPos as WireSectionPos,
    clientbound::{BlockUpdate, Packet, SectionBlocksUpdate},
};
use ferraloom_util::{BlockPos, ChunkPos, SectionPos, collections::ShortOpenHashSet};
use std::collections::{BTreeMap, HashMap};
#[derive(Default)]
pub(super) struct BlockChanges {
    chunks: HashMap<ChunkPos, BTreeMap<i32, ShortOpenHashSet>>,
    pending: ferraloom_util::collections::LongLinkedOpenHashSet,
}
impl Level {
    pub(super) fn send_block_updated(&mut self, pos: BlockPos, old: BlockState, new: BlockState) {
        let chunk_pos = ChunkPos::containing(pos);
        if self
            .chunk(chunk_pos)
            .is_some_and(|chunk| chunk.ticking_ready())
        {
            self.region_mut()
                .block_changes
                .pending
                .insert(chunk_pos.pack());
            self.region_mut()
                .block_changes
                .chunks
                .entry(chunk_pos)
                .or_default()
                .entry(pos.y >> 4)
                .or_default()
                .insert(SectionPos::section_relative_pos(pos));
        }
        self.path_type_cache_mut().invalidate(pos);
        self.emit(LevelEffect::BlockUpdated { pos, old, new });
    }
    /// Drain this phase's holder notifications separately from retained section
    /// changes. An unsuccessful ticking future suppresses that broadcast; merely
    /// becoming ready later does not enqueue another notification.
    pub fn take_block_update_chunks(&mut self) -> Vec<ChunkPos> {
        let pending = &mut self.region_mut().block_changes.pending;
        let mut positions = Vec::with_capacity(pending.len());
        while let Some(key) = pending.pop_first() {
            positions.push(ChunkPos::unpack(key));
        }
        positions
    }
    /// The server calls this during ServerChunkCache's broadcast-changed-chunks
    /// phase and sends these packets to the chunk's tracking players. Calling
    /// with no audience still clears changes, as vanilla does.
    pub fn take_block_updates(&mut self, pos: ChunkPos) -> Result<Vec<Packet>, LevelError> {
        self.region_mut().block_changes.pending.remove(&pos.pack());
        let Some(sections) = self.region_mut().block_changes.chunks.remove(&pos) else {
            return Ok(Vec::new());
        };
        let mut packets = Vec::new();
        for (y, changes) in sections {
            let section = SectionPos::new(pos.x, y, pos.z);
            if changes.len() == 1 {
                let block_pos = section.relative_to_block_pos(*changes.iter().next().unwrap());
                let state = self.get_block_state(block_pos)?;
                packets.push(Packet::BlockUpdate(BlockUpdate {
                    pos: ferraloom_protocol::BlockPos {
                        x: block_pos.x,
                        y: block_pos.y,
                        z: block_pos.z,
                    },
                    block_state: BlockStateId(i32::from(state.id())),
                }));
                self.append_block_entity_packet(block_pos, &mut packets)?;
            } else {
                let mut states = Vec::with_capacity(changes.len());
                for &relative in changes.iter() {
                    states.push(SectionBlockChange {
                        position: relative as u16,
                        state: self.get_block_state(section.relative_to_block_pos(relative))?,
                    });
                }
                packets.push(Packet::SectionBlocksUpdate(SectionBlocksUpdate {
                    section_pos: WireSectionPos {
                        x: pos.x,
                        y,
                        z: pos.z,
                    },
                    changes: states,
                }));
                for &relative in changes.iter() {
                    self.append_block_entity_packet(
                        section.relative_to_block_pos(relative),
                        &mut packets,
                    )?;
                }
            }
        }
        Ok(packets)
    }
    fn append_block_entity_packet(
        &mut self,
        pos: ferraloom_util::BlockPos,
        packets: &mut Vec<Packet>,
    ) -> Result<(), LevelError> {
        if self.get_block_state(pos)?.has_block_entity()
            && let Some(handle) = self.get_or_promote_block_entity(pos)?
            && let Some(packet) = self
                .block_entity_state(handle)?
                .get_update_packet()
                .map_err(|_| LevelError::Inventory("block entity update packet codec"))?
        {
            packets.push(Packet::BlockEntityData(packet));
        }
        Ok(())
    }
}
