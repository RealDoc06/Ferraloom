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
}
impl Level {
    pub(super) fn send_block_updated(&mut self, pos: BlockPos, old: BlockState, new: BlockState) {
        self.region_mut()
            .block_changes
            .chunks
            .entry(ChunkPos::containing(pos))
            .or_default()
            .entry(pos.y >> 4)
            .or_default()
            .insert(SectionPos::section_relative_pos(pos));
        self.emit(LevelEffect::BlockUpdated { pos, old, new });
    }
    /// The server calls this during ServerChunkCache's broadcast-changed-chunks
    /// phase and sends these packets to the chunk's tracking players. Calling
    /// with no audience still clears changes, as vanilla does.
    pub fn take_block_updates(&mut self, pos: ChunkPos) -> Result<Vec<Packet>, LevelError> {
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
            }
        }
        Ok(packets)
    }
}
