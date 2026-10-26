//! Immutable BlockBehaviour#getDescriptionId, including Properties overrides.
impl crate::Block {
    pub const fn description_id(self) -> &'static str {
        crate::BLOCK_DESCRIPTION_IDS[self.protocol_id() as usize]
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_block_descriptions_keep_wall_and_skull_overrides() {
        use crate::Block;
        for (block, description) in [
            (Block::STONE, "block.minecraft.stone"),
            (Block::WALL_TORCH, "block.minecraft.torch"),
            (Block::OAK_WALL_SIGN, "block.minecraft.oak_sign"),
            (Block::WHITE_WALL_BANNER, "block.minecraft.white_banner"),
            (Block::SKELETON_SKULL, "block.minecraft.skeleton_skull"),
            (Block::SKELETON_WALL_SKULL, "block.minecraft.skeleton_skull"),
        ] {
            assert_eq!(block.description_id(), description);
        }
    }
}
