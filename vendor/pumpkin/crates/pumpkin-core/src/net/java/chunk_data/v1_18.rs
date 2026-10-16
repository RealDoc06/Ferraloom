use super::util::write_compound_nbt;
use pumpkin_protocol::codec::bit_set::BitSet;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::ser::NetworkWriteExt;
use pumpkin_protocol::ser::WritingError;
use pumpkin_util::math::position::get_local_cord;
use pumpkin_util::version::JavaMinecraftVersion;
use pumpkin_world::chunk::ChunkData;
use pumpkin_world::chunk::format::LightContainer;
use pumpkin_world::chunk::palette::NetworkPalette;
use std::io::Write;

/// Serializes chunk data for Minecraft 1.18+ through 26.2+.
#[expect(clippy::too_many_lines)]
pub fn write_chunk_data(
    chunk: &ChunkData,
    mut write: impl Write,
    version: &JavaMinecraftVersion,
) -> Result<(), WritingError> {
    write.write_i32_be(chunk.x)?;
    write.write_i32_be(chunk.z)?;

    let heightmaps = chunk
        .heightmap
        .lock()
        .map_err(|_| WritingError::Message("heightmap lock poisoned".into()))?;
    if version >= &JavaMinecraftVersion::V_1_21_5 {
        write.write_var_int(&VarInt(3))?; // Map size

        let mut write_heightmap = |index: i32, data: &[i64]| -> Result<(), WritingError> {
            write.write_var_int(&VarInt(index))?;
            write.write_var_int(&VarInt(data.len() as i32))?;
            for val in data {
                write.write_i64_be(*val)?;
            }
            Ok(())
        };

        write_heightmap(1, heightmaps.world_surface.as_deref().unwrap_or(&[0; 37]))?;
        write_heightmap(4, heightmaps.motion_blocking.as_deref().unwrap_or(&[0; 37]))?;
        write_heightmap(
            5,
            heightmaps
                .motion_blocking_no_leaves
                .as_deref()
                .unwrap_or(&[0; 37]),
        )?;
    } else {
        let mut comp = pumpkin_nbt::compound::NbtCompound::new();
        if let Some(ref ws) = heightmaps.world_surface {
            comp.put(
                "WORLD_SURFACE",
                pumpkin_nbt::tag::NbtTag::LongArray(ws.to_vec()),
            );
        }
        if let Some(ref mb) = heightmaps.motion_blocking {
            comp.put(
                "MOTION_BLOCKING",
                pumpkin_nbt::tag::NbtTag::LongArray(mb.to_vec()),
            );
        }
        if let Some(ref mbnl) = heightmaps.motion_blocking_no_leaves {
            comp.put(
                "MOTION_BLOCKING_NO_LEAVES",
                pumpkin_nbt::tag::NbtTag::LongArray(mbnl.to_vec()),
            );
        }
        write_compound_nbt(&mut write, &comp, *version)?;
    }
    drop(heightmaps);

    {
        let mut blocks_and_biomes_buf = Vec::new();
        let block_sections = chunk
            .section
            .block_sections
            .read()
            .map_err(|_| WritingError::Message("block_sections read lock poisoned".into()))?;
        let biome_sections = chunk
            .section
            .biome_sections
            .read()
            .map_err(|_| WritingError::Message("biome_sections read lock poisoned".into()))?;

        let mut zero_bytes_count = 0;

        for (block_palette, biome_palette) in block_sections.iter().zip(biome_sections.iter()) {
            let non_empty_block_count = block_palette.non_air_block_count() as i16;
            blocks_and_biomes_buf.write_i16_be(non_empty_block_count)?;
            if version >= &JavaMinecraftVersion::V_26_1 {
                // New in 26.1, fluid count
                let liquid_count = block_palette.liquid_block_count() as i16;
                blocks_and_biomes_buf.write_i16_be(liquid_count)?;
            }

            let block_network = block_palette.convert_network();
            blocks_and_biomes_buf.write_u8(block_network.bits_per_entry)?;

            match block_network.palette {
                NetworkPalette::Single(registry_id) => {
                    blocks_and_biomes_buf.write_var_int(&registry_id.into())?;
                }
                NetworkPalette::Indirect(palette) => {
                    blocks_and_biomes_buf.write_var_int(&palette.len().try_into().map_err(
                        |_| {
                            WritingError::Message(format!(
                                "{} is not representable as a VarInt!",
                                palette.len()
                            ))
                        },
                    )?)?;
                    for registry_id in palette {
                        blocks_and_biomes_buf.write_var_int(&registry_id.into())?;
                    }
                }
                NetworkPalette::Direct => {}
            }

            if version <= &JavaMinecraftVersion::V_1_21_4 {
                blocks_and_biomes_buf.write_list(&block_network.packed_data, |buf, &packed| {
                    buf.write_i64_be(packed)
                })?;
            } else {
                for packed in &block_network.packed_data {
                    blocks_and_biomes_buf.write_i64_be(*packed)?;
                }
            }

            let biome_network = biome_palette.convert_network();
            blocks_and_biomes_buf.write_u8(biome_network.bits_per_entry)?;

            match biome_network.palette {
                NetworkPalette::Single(registry_id) => {
                    blocks_and_biomes_buf.write_var_int(&registry_id.into())?;
                }
                NetworkPalette::Indirect(palette) => {
                    blocks_and_biomes_buf.write_var_int(&palette.len().try_into().map_err(
                        |_| {
                            WritingError::Message(format!(
                                "{} is not representable as a VarInt!",
                                palette.len()
                            ))
                        },
                    )?)?;
                    for registry_id in palette {
                        blocks_and_biomes_buf.write_var_int(&registry_id.into())?;
                    }
                }
                NetworkPalette::Direct => {}
            }

            if version <= &JavaMinecraftVersion::V_1_21_4 {
                blocks_and_biomes_buf.write_list(&biome_network.packed_data, |buf, &packed| {
                    buf.write_i64_be(packed)
                })?;
            } else {
                for packed in &biome_network.packed_data {
                    blocks_and_biomes_buf.write_i64_be(*packed)?;
                }
            }

            if version == &JavaMinecraftVersion::V_1_21_5 {
                let block_storage_len = block_network.packed_data.len() as i32;
                let biome_storage_len = biome_network.packed_data.len() as i32;
                zero_bytes_count += VarInt(block_storage_len).written_size()
                    + VarInt(biome_storage_len).written_size();
            }
        }

        if version == &JavaMinecraftVersion::V_1_21_5 && zero_bytes_count > 0 {
            blocks_and_biomes_buf.resize(blocks_and_biomes_buf.len() + zero_bytes_count, 0);
        }

        write.write_var_int(&blocks_and_biomes_buf.len().try_into().map_err(|_| {
            WritingError::Message(format!(
                "{} is not representable as a VarInt!",
                blocks_and_biomes_buf.len()
            ))
        })?)?;
        write.write_slice(&blocks_and_biomes_buf)?;
    };

    let block_entities = chunk
        .pending_block_entities
        .lock()
        .map_err(|_| WritingError::Message("block_entities lock poisoned".into()))?;
    write.write_var_int(&VarInt(block_entities.len() as i32))?;
    for (pos, nbt) in block_entities.iter() {
        let local_xz = ((get_local_cord(pos.0.x) & 0xF) << 4) | (get_local_cord(pos.0.z) & 0xF);

        write.write_u8(local_xz as u8)?;
        write.write_i16_be(pos.0.y as i16)?;

        let id = nbt.get_string("id").map_or(0, |id_str| {
            let name = id_str.split(':').next_back().unwrap_or(id_str);
            pumpkin_data::block_properties::BLOCK_ENTITY_TYPES
                .iter()
                .position(|&n| n == name)
                .unwrap_or(0)
        });
        write.write_var_int(&VarInt(id as i32))?;

        let mut client_nbt = match pumpkin_data::block_properties::BLOCK_ENTITY_TYPES[id] {
            "chest" | "trapped_chest" | "barrel" => {
                // These types inherit Java's empty BlockEntity.getUpdateTag.
                // BlockEntityInfo.create encodes it as absent NBT, never a save
                // snapshot. The pending map retains private state for live BEs too.
                write.write_compound_nbt_with_version(None, version)?;
                continue;
            }
            "sign" | "hanging_sign" => crate::block::entities::sign::client_nbt_from_persisted(nbt),
            _ => nbt.clone(),
        };
        client_nbt.child_tags.remove("id");
        client_nbt.child_tags.remove("x");
        client_nbt.child_tags.remove("y");
        client_nbt.child_tags.remove("z");
        client_nbt.child_tags.remove("LootTable");
        client_nbt.child_tags.remove("LootTableSeed");
        client_nbt.child_tags.remove("PumpkinCustomData");
        client_nbt.child_tags.remove("BukkitValues");
        write_compound_nbt(&mut write, &client_nbt, *version)?;
    }

    {
        // Light masks include sections from -1 (below world) to num_sections (above world)
        // This means we need to account for 2 extra sections in the bitset
        let light_engine = chunk
            .light_engine
            .lock()
            .map_err(|_| WritingError::Message("light_engine lock poisoned".into()))?;
        let num_sections = light_engine.sky_light.len();

        let mut sky_light_empty_mask = 0u64;
        let mut block_light_empty_mask = 0u64;
        let mut sky_light_mask = 0u64;
        let mut block_light_mask = 0u64;

        // Bit 0 represents the section below the world (always empty)
        sky_light_empty_mask |= 1 << 0;
        block_light_empty_mask |= 1 << 0;

        // Bits 1..=num_sections represent the actual world sections
        for section_index in 0..num_sections {
            let bit_index = section_index + 1; // Offset by 1 for the below-world section

            if let LightContainer::Full(_) = &light_engine.sky_light[section_index] {
                sky_light_mask |= 1 << bit_index;
            } else {
                sky_light_empty_mask |= 1 << bit_index;
            }

            if let LightContainer::Full(_) = &light_engine.block_light[section_index] {
                block_light_mask |= 1 << bit_index;
            } else {
                block_light_empty_mask |= 1 << bit_index;
            }
        }

        // Bit num_sections+1 represents the section above the world (always empty)
        sky_light_empty_mask |= 1 << (num_sections + 1);
        block_light_empty_mask |= 1 << (num_sections + 1);

        // Trust edges (1.18 - 1.19.4; removed in 1.20)
        if version < &JavaMinecraftVersion::V_1_20 {
            write.write_bool(true)?;
        }

        // Write Sky Light Mask
        BitSet(Box::new([sky_light_mask as i64])).encode_with_version(&mut write, version)?;
        // Write Block Light Mask
        BitSet(Box::new([block_light_mask as i64])).encode_with_version(&mut write, version)?;
        // Write Empty Sky Light Mask
        BitSet(Box::new([sky_light_empty_mask as i64])).encode_with_version(&mut write, version)?;
        // Write Empty Block Light Mask
        BitSet(Box::new([block_light_empty_mask as i64]))
            .encode_with_version(&mut write, version)?;

        let light_data_size: VarInt = VarInt(LightContainer::ARRAY_SIZE as i32);

        // Write Sky Light arrays
        write.write_var_int(&VarInt(sky_light_mask.count_ones() as i32))?;
        for section_index in 0..num_sections {
            if let LightContainer::Full(data) = &light_engine.sky_light[section_index] {
                write.write_var_int(&light_data_size)?;
                write.write_slice(data.as_ref())?;
            }
        }

        // Write Block Light arrays
        write.write_var_int(&VarInt(block_light_mask.count_ones() as i32))?;
        for section_index in 0..num_sections {
            if let LightContainer::Full(data) = &light_engine.block_light[section_index] {
                write.write_var_int(&light_data_size)?;
                write.write_slice(data.as_ref())?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::entities::block_entity_from_nbt;
    use crate::command::commands::data::tests::Fixture;
    use crate::net::java::chunk_data::CChunkData;
    use pumpkin_command::{snbt::SnbtParser, string_reader::StringReader};
    use pumpkin_data::Block;
    use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
    use pumpkin_protocol::{ClientPacket, ser::NetworkReadExt};
    use pumpkin_util::math::position::BlockPos;
    use std::sync::Arc;

    const POS: BlockPos = BlockPos::new(0, 64, 0);
    const CONTAINERS: [(&str, &Block); 3] = [
        ("minecraft:chest", &Block::CHEST),
        ("minecraft:trapped_chest", &Block::TRAPPED_CHEST),
        ("minecraft:barrel", &Block::BARREL),
    ];

    struct ChunkBlockEntity {
        packed_xz: u8,
        y: i16,
        type_id: i32,
        tag: Option<NbtTag>,
    }

    fn compound(snbt: &str) -> NbtCompound {
        let NbtTag::Compound(nbt) =
            SnbtParser::parse_for_commands(&mut StringReader::new(snbt)).unwrap()
        else {
            panic!("expected compound fixture");
        };
        nbt
    }

    fn private_container_nbt(id: &str, pos: BlockPos) -> NbtCompound {
        let mut nbt = compound(
            "{CustomName:'Private vault',lock:{components:{'minecraft:custom_name':'Vault key'}},components:{'minecraft:custom_data':{secret:'Map secret'}},Items:[{Slot:0b,id:'minecraft:diamond',count:3,components:{'minecraft:custom_data':{secret:'Item secret'}}}],LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:42L,PumpkinCustomData:{secret:'Plugin secret'},BukkitValues:{secret:'Legacy plugin secret'},future_private_field:'Do not publish'}",
        );
        nbt.put_string("id", id.to_owned());
        nbt.put_int("x", pos.0.x);
        nbt.put_int("y", pos.0.y);
        nbt.put_int("z", pos.0.z);
        nbt
    }

    /// Decode the actual packet body, including the trailing light payload, so
    /// absent NBT must preserve the next entry and the packet's field boundaries.
    fn encoded_block_entities(chunk: &ChunkData) -> Vec<ChunkBlockEntity> {
        let version = JavaMinecraftVersion::V_26_3;
        let mut bytes = Vec::new();
        CChunkData::new(chunk)
            .write_packet_data(&mut bytes, &version)
            .unwrap();
        let mut reader = bytes.as_slice();
        assert_eq!(reader.get_i32_be().unwrap(), chunk.x);
        assert_eq!(reader.get_i32_be().unwrap(), chunk.z);
        let heightmaps = reader.get_var_int().unwrap().0;
        for _ in 0..heightmaps {
            reader.get_var_int().unwrap();
            reader.get_list(NetworkReadExt::get_i64_be).unwrap();
        }
        let section_bytes = reader.get_var_int().unwrap().0 as usize;
        reader = &reader[section_bytes..];
        let entities = reader
            .get_list(|reader| {
                Ok(ChunkBlockEntity {
                    packed_xz: reader.get_u8()?,
                    y: reader.get_i16_be()?,
                    type_id: reader.get_var_int()?.0,
                    tag: reader.get_nbt_with_version(&version)?,
                })
            })
            .unwrap();
        // Protocol 777 BIT_SET is a trimmed little-endian byte array.
        let light_masks: [Vec<u8>; 4] =
            std::array::from_fn(|_| reader.get_list(NetworkReadExt::get_u8).unwrap());
        for mask in &light_masks[2..] {
            assert_eq!(mask.first().map(|byte| byte & 1), Some(1));
        }
        for _ in 0..2 {
            reader
                .get_list(|reader| reader.get_list(NetworkReadExt::get_u8))
                .unwrap();
        }
        assert!(reader.is_empty(), "entire chunk packet must decode");
        entities
    }

    fn assert_container_entry(chunk: &ChunkData, pos: BlockPos, id: &str) {
        let entities = encoded_block_entities(chunk);
        let packed_xz = (((pos.0.x & 15) << 4) | (pos.0.z & 15)) as u8;
        let entry = entities
            .iter()
            .find(|entry| entry.packed_xz == packed_xz && entry.y == pos.0.y as i16)
            .unwrap();
        let name = id.strip_prefix("minecraft:").unwrap_or(id);
        let type_id = pumpkin_data::block_properties::BLOCK_ENTITY_TYPES
            .iter()
            .position(|&candidate| candidate == name)
            .unwrap();
        assert_eq!(entry.type_id, type_id as i32);
        assert_eq!(entry.tag, None, "{id} must have Java's absent update tag");
    }

    #[test]
    fn chunk_container_entries_omit_private_pending_nbt() {
        let chunk = ChunkData::empty(-2, 3);
        *chunk.light_engine.lock().unwrap() = pumpkin_world::chunk::ChunkLight {
            sky_light: vec![LightContainer::new_empty(0); 24].into_boxed_slice(),
            block_light: vec![LightContainer::new_empty(0); 24].into_boxed_slice(),
        };
        for (index, (id, _)) in CONTAINERS.iter().enumerate() {
            let pos = BlockPos::new(-30 + index as i32, -32, 51);
            chunk
                .pending_block_entities
                .lock()
                .unwrap()
                .insert(pos, private_container_nbt(id, pos));
        }
        let before = chunk.pending_block_entities.lock().unwrap().clone();
        assert_eq!(encoded_block_entities(&chunk).len(), CONTAINERS.len());
        for (pos, nbt) in &before {
            assert_container_entry(&chunk, *pos, nbt.get_string("id").unwrap());
        }
        assert_eq!(*chunk.pending_block_entities.lock().unwrap(), before);

        // The registry lookup also accepts unqualified vanilla IDs. Its client
        // projection must be the same as the type ID actually sent on the wire.
        for (id, _) in CONTAINERS {
            let id = id.strip_prefix("minecraft:").unwrap();
            let pos = BlockPos::new(-32, 64, 48);
            chunk.pending_block_entities.lock().unwrap().clear();
            chunk
                .pending_block_entities
                .lock()
                .unwrap()
                .insert(pos, private_container_nbt(id, pos));
            assert_container_entry(&chunk, pos, id);
        }
    }

    #[tokio::test]
    async fn chunk_container_entries_omit_private_nbt_after_live_data_edits() {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        for (id, block) in CONTAINERS {
            let mut nbt = private_container_nbt(id, POS);
            nbt.child_tags.remove("LootTable");
            nbt.child_tags.remove("LootTableSeed");
            let live = block_entity_from_nbt(&nbt).unwrap();
            fixture.install(block, live.clone());

            let edit = "{CustomName:'Edited vault',lock:{components:{'minecraft:custom_name':'Edited key'}},components:{'minecraft:custom_data':{secret:'Edited secret'}},Items:[{Slot:0b,id:'minecraft:diamond',count:7,components:{'minecraft:custom_data':{secret:'Edited item secret'}}}]}";
            fixture
                .execute(&format!("data merge block 0 64 0 {edit}"))
                .unwrap();
            let published = fixture.pending_data();
            let edited = compound(edit);
            for field in ["CustomName", "lock", "components", "Items"] {
                assert_eq!(published.get(field), edited.get(field));
            }
            world
                .level
                .read_chunk_sync(&POS.chunk_position(), |chunk| {
                    assert_container_entry(chunk, POS, id);
                })
                .unwrap();
            assert_eq!(fixture.pending_data(), published);

            fixture.execute("data merge block 0 64 0 {LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:84L}").unwrap();
            let published = fixture.pending_data();
            assert_eq!(
                published.get_string("LootTable"),
                Some("minecraft:chests/simple_dungeon")
            );
            assert_eq!(published.get_long("LootTableSeed"), Some(84));
            assert!(live.has_loot_table());
            world
                .level
                .read_chunk_sync(&POS.chunk_position(), |chunk| {
                    assert_container_entry(chunk, POS, id);
                })
                .unwrap();
            assert!(live.has_loot_table(), "encoding must not unpack loot");
            assert_eq!(fixture.pending_data(), published);
            assert!(Arc::ptr_eq(&world.get_block_entity(&POS).unwrap(), &live));
        }
    }

    #[test]
    fn chunk_sign_entries_keep_modeled_client_projection() {
        let chunk = ChunkData::empty(0, 0);
        for (index, id) in ["minecraft:sign", "minecraft:hanging_sign"]
            .into_iter()
            .enumerate()
        {
            let pos = BlockPos::new(index as i32, 64, 0);
            let mut nbt = compound(
                "{front_text:{messages:[{text:'Visible',color:'red',bold:1b},{selector:'@a'},'third','fourth'],color:'blue',has_glowing_text:1b},back_text:{messages:['Back','','','']},is_waxed:1b,allow_op_features:1b,PumpkinCustomData:{secret:1b},BukkitValues:{secret:1b}}",
            );
            nbt.put_string("id", id.to_owned());
            nbt.put_int("x", pos.0.x);
            nbt.put_int("y", pos.0.y);
            nbt.put_int("z", pos.0.z);
            chunk
                .pending_block_entities
                .lock()
                .unwrap()
                .insert(pos, nbt);
        }
        let before = chunk.pending_block_entities.lock().unwrap().clone();
        let entries = encoded_block_entities(&chunk);
        assert_eq!(entries.len(), 2);
        for entry in entries {
            let tag = entry.tag.unwrap();
            let nbt = tag.extract_compound().unwrap();
            let front = nbt.get_compound("front_text").unwrap();
            let messages = front.get_list("messages").unwrap();
            let first = messages[0].extract_compound().unwrap();
            assert_eq!(first.get_string("text"), Some("Visible"));
            assert_eq!(first.get_string("color"), Some("red"));
            assert_eq!(first.get_bool("bold"), Some(true));
            assert_eq!(
                messages[1].extract_string(),
                Some("[Unsupported sign text]")
            );
            assert_eq!(messages[2].extract_string(), Some("third"));
            assert_eq!(front.get_string("color"), Some("blue"));
            assert_eq!(front.get_bool("has_glowing_text"), Some(true));
            assert_eq!(nbt.get_bool("is_waxed"), Some(true));
            assert_eq!(nbt.get_bool("allow_op_features"), Some(true));
            for field in ["id", "x", "y", "z", "PumpkinCustomData", "BukkitValues"] {
                assert_eq!(nbt.get(field), None);
            }
        }
        assert_eq!(*chunk.pending_block_entities.lock().unwrap(), before);
    }
}
