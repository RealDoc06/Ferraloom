use crate::{
    Behaviors,
    block::{
        BlockMethod, LevelAccessRef,
        containers::{
            command::CommandBlockEntity, sign::SignBlockEntity, structure::StructureBlockEntity,
        },
        entity::{self, BlockEntityState},
        placement::StackView,
    },
    entity::{EntityData, EntityRef, player::ServerPlayer},
    inventory::{ContainerHandle, ContainerSlot, StackOperation, StackRef, StackValue},
    level::{Level, LevelContext, LevelError, Region, chunk::LevelChunk, effects::LevelEffect},
};
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    chat::Component,
    component::{
        BlockEntityType, CustomData, DataComponent, DataComponentPatch, ItemStack,
        TypedBlockEntityData,
    },
    play::{Difficulty, SignTextSlot, clientbound as cb},
};
use ferraloom_util::{BlockPos, ChunkPos, Direction, Uuid};
use std::{
    io::Read,
    sync::{Arc, OnceLock},
};

const POS: BlockPos = BlockPos::new(8, 5, 8);
static BEHAVIORS: Behaviors = Behaviors::EMPTY;
fn vectors() -> String {
    let mut data = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/post-placement.tsv.gz").as_slice(),
    )
    .read_to_string(&mut data)
    .unwrap();
    data
}
fn state(id: &str) -> BlockState {
    BlockState::from_id(id.parse().unwrap()).unwrap()
}
fn bare_level() -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..1,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                )),
                ferraloom_chunk::BiomeId(0),
            ),
        ))
        .unwrap();
    static TAGS: OnceLock<Arc<ferraloom_data::datapack::TagSet<Block>>> = OnceLock::new();
    let tags = TAGS.get_or_init(|| {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
        let resources = ferraloom_data::datapack::ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
        ]);
        Arc::new(ferraloom_data::datapack::TagSet::<Block>::load(&resources).unwrap())
    });
    level.set_block_tags(Arc::clone(tags));
    level
}
fn install(level: &mut Level, state: BlockState) -> entity::BlockEntityRef {
    level.set_block_state(POS, state).unwrap();
    level
        .set_block_entity(entity::create(POS, state).unwrap())
        .unwrap();
    level.block_entity_ref(POS).unwrap().unwrap()
}
fn ticks(level: &Level) -> String {
    let mut data = ferraloom_storage::SerializableChunkData::default();
    level
        .save_chunk_ticks(ChunkPos::new(0, 0), &mut data)
        .unwrap();
    data.block_ticks
        .unwrap()
        .into_iter()
        .map(|tick| {
            let block = Block::from_name(&tick.id.unwrap().to_string_checked().unwrap()).unwrap();
            format!("{}:{}", tick.delay.unwrap(), block.protocol_id())
        })
        .collect::<Vec<_>>()
        .join(",")
}
fn held(block: Block, tag: Option<CompoundTag>) -> ItemStack {
    let mut patch = DataComponentPatch::default();
    if let Some(tag) = tag {
        patch.set(DataComponent::BlockEntityData(TypedBlockEntityData {
            entity_type: BlockEntityType(Identifier::parse("minecraft:chest").unwrap()),
            tag: CustomData(Tag::Compound(tag)),
        }));
    }
    ItemStack::new(block.as_item(), 1, patch)
}
fn unread(player: EntityRef) -> StackView<'static> {
    StackView::Retained(StackRef {
        container: ContainerHandle::PlayerInventory(player),
        slot: 0,
        generation: 0,
    })
}

/// All bed partners and all sign variants use their actual placement callbacks;
/// sign editor packets retain BlockUpdate -> OpenSignEditor(FRONT) ordering.
#[test]
fn native_bed_and_sign_placement_callbacks() {
    let (mut level, player) = super::tests::level();
    let mut beds = 0;
    let mut shapes = 0;
    let mut signs = 0;
    for line in vectors().lines() {
        let f: Vec<_> = line.split('\t').collect();
        match f[0] {
            "bed-placed" => {
                let state = state(f[1]);
                let head = POS.relative(super::super::menu::facing(state), 1);
                level.set_block_state(POS, state).unwrap();
                level
                    .set_block_state(head, Block::AIR.default_state())
                    .unwrap();
                level
                    .set_placed_by(state, POS, None, unread(player))
                    .unwrap();
                assert_eq!(
                    level.get_block_state(head).unwrap(),
                    self::state(f[2]),
                    "{line}"
                );
                assert!(level.block_entity_ref(head).unwrap().is_none());
                beds += 1;
            }
            "bed-shape" => {
                let current = state(f[1]);
                let side = Direction::VALUES[f[2].parse::<usize>().unwrap()];
                let behavior = level
                    .required_block_behavior(current.block(), BlockMethod::UpdateShape)
                    .unwrap();
                let actual = behavior
                    .update_shape(
                        current,
                        POS,
                        side,
                        POS.relative(side, 1),
                        state(f[3]),
                        &mut LevelAccessRef::Server(&mut level),
                    )
                    .unwrap();
                assert_eq!(actual, state(f[4]), "{line}");
                shapes += 1;
            }
            "sign-placed" => {
                let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
                let handle = install(&mut level, block.default_state());
                let message: u8 = f[4].parse().unwrap();
                let sign = level.block_entity_mut::<SignBlockEntity>(handle).unwrap();
                sign.is_waxed = f[2] == "true";
                sign.allowed_player_editor = Some(Uuid::new(3, 4));
                sign.front_text.messages[0] = if matches!(message, 1 | 3) {
                    Component::translatable("test.message", vec![])
                } else {
                    Component::literal("raw")
                };
                let mut filtered = sign.front_text.messages.clone();
                filtered[0] = if message >= 2 {
                    Component::translatable("test.filtered", vec![])
                } else {
                    Component::literal("filtered")
                };
                sign.front_text.filtered_messages = Some(filtered);
                let player_state = level.entity_mut::<ServerPlayer>(player).unwrap();
                player_state.text_filtering_enabled = f[3] == "true";
                player_state.abilities.may_build = false;
                level.take_effects();
                level
                    .set_placed_by(block.default_state(), POS, Some(player), unread(player))
                    .unwrap();
                assert_eq!(
                    level
                        .block_entity::<SignBlockEntity>(handle)
                        .unwrap()
                        .allowed_player_editor
                        == Some(Uuid::new(1, 2)),
                    f[5] == "true",
                    "{line}"
                );
                let packets = level
                    .take_effects()
                    .into_iter()
                    .filter_map(|effect| match effect {
                        LevelEffect::PlayerPacket { recipients, packet } => {
                            assert_eq!(recipients, [player]);
                            Some(match *packet {
                                cb::Packet::BlockUpdate(value) => {
                                    assert_eq!(value.pos, POS);
                                    "block"
                                }
                                cb::Packet::OpenSignEditor(value) => {
                                    assert_eq!(value.pos, POS);
                                    assert_eq!(value.slot, SignTextSlot::Front);
                                    "editor:FRONT"
                                }
                                packet => panic!("unexpected editor packet {packet:?}"),
                            })
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                assert_eq!(packets, f[6], "{line}");
                signs += 1;
            }
            _ => {}
        }
    }
    assert_eq!((beds, shapes, signs), (136, 11_424, 832));
}

/// Constructor defaults, live power and conditional receiver reads determine
/// command scheduling. Crafter schedules once even after repeated callbacks.
#[test]
fn native_command_and_utility_placement_callbacks() {
    let mut commands = 0;
    let mut jukebox_queries = 0;
    for line in vectors().lines() {
        let f: Vec<_> = line.split('\t').collect();
        match f[0] {
            "command-placed" => {
                let mut level = bare_level();
                let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
                let mask: u8 = f[2].parse().unwrap();
                let behind: u8 = f[3].parse().unwrap();
                let state = block
                    .default_state()
                    .with(p::CONDITIONAL, mask & 1 != 0)
                    .unwrap();
                let mut entity = entity::create(POS, state).unwrap();
                let mut tag = CompoundTag::new();
                for (key, bit) in [("auto", 16), ("powered", 32), ("TrackOutput", 64)] {
                    tag.insert(key, Tag::Byte(i8::from(mask & bit != 0)));
                }
                entity.load_with_components(
                    &tag,
                    crate::level::LevelContext::fixture(0, 63)
                        .registry_context()
                        .unwrap(),
                );
                level.set_block_state(POS, state).unwrap();
                level.set_block_entity(entity).unwrap();
                let handle = level.block_entity_ref(POS).unwrap().unwrap();
                if behind != 0 {
                    let pos = POS.relative(super::super::menu::facing(state).opposite(), 1);
                    let mut other =
                        entity::create(pos, Block::COMMAND_BLOCK.default_state()).unwrap();
                    if let BlockEntityState::Command(entity) = &mut other {
                        entity.command_block.success_count = i32::from(behind - 1);
                    }
                    level
                        .set_block_state(pos, Block::COMMAND_BLOCK.default_state())
                        .unwrap();
                    level.set_block_entity(other).unwrap();
                }
                level
                    .set_block_state(
                        POS.above(1),
                        if mask & 4 != 0 {
                            Block::REDSTONE_BLOCK
                        } else {
                            Block::AIR
                        }
                        .default_state(),
                    )
                    .unwrap();
                level.rules_mut().send_command_feedback = mask & 2 != 0;
                let stack = held(block, (mask & 8 != 0).then(CompoundTag::new));
                level
                    .set_placed_by(state, POS, None, StackView::Borrowed(&stack))
                    .unwrap();
                assert_eq!(
                    level
                        .block_entity_state(handle)
                        .unwrap()
                        .save_without_metadata()
                        .unwrap(),
                    ferraloom_nbt::parse_compound(f[4]).unwrap(),
                    "{line}"
                );
                assert_eq!(ticks(&level), f[5], "{line}");
                commands += 1;
            }
            "crafter-placed" => {
                let mut level = bare_level();
                let state = Block::CRAFTER
                    .default_state()
                    .with(p::TRIGGERED, f[1] == "true")
                    .unwrap();
                let empty = ItemStack::default();
                for _ in 0..2 {
                    level
                        .set_placed_by(state, POS, None, StackView::Borrowed(&empty))
                        .unwrap();
                }
                assert_eq!(ticks(&level), f[2], "{line}");
            }
            "jukebox-placed" => {
                let mut level = bare_level();
                // Level#setBlock only sends CLIENTS updates for a block-ticking
                // server chunk, matching the native placement write observer.
                level
                    .set_chunk_loading_status(
                        ChunkPos::new(0, 0),
                        crate::level::distance_manager::FullChunkStatus::BlockTicking,
                        true,
                    )
                    .unwrap();
                let state = Block::JUKEBOX.default_state();
                level.set_block_state(POS, state).unwrap();
                let kind: u8 = f[1].parse().unwrap();
                let mut tag = CompoundTag::new();
                match kind {
                    1 => {
                        tag.insert("RecordItem", Tag::Int(7));
                    }
                    2 => {
                        tag.insert("RecordItem", Tag::Compound(CompoundTag::new()));
                    }
                    3 => {
                        tag.insert("recorditem", Tag::String("ignored".into()));
                    }
                    _ => {}
                }
                let stack = held(Block::JUKEBOX, (kind != 0).then_some(tag));
                level
                    .set_placed_by(state, POS, None, StackView::Borrowed(&stack))
                    .unwrap();
                let expected = self::state(f[2]);
                assert_eq!(level.get_block_state(POS).unwrap(), expected, "{line}");
                // The native vectors include malformed RecordItem values and
                // the exact CLIENTS update. Placement sets the flag, not a song.
                let updates: Vec<_> = level
                    .take_effects()
                    .into_iter()
                    .filter_map(|effect| match effect {
                        LevelEffect::BlockUpdated { pos, old, new } => {
                            assert_eq!(pos, POS, "{line}");
                            assert_eq!(old, state, "{line}");
                            Some(new.id())
                        }
                        _ => None,
                    })
                    .collect();
                let native_updates: Vec<u16> = f[3]
                    .split(',')
                    .filter(|write| !write.is_empty())
                    .map(|write| {
                        let (state, flags) = write.split_once(':').unwrap();
                        assert_eq!(flags, "2", "native CLIENTS flag: {line}");
                        state.parse().unwrap()
                    })
                    .collect();
                assert_eq!(updates, native_updates, "{line}");
                if matches!(kind, 1 | 2) {
                    let handle = level.block_entity_ref(POS).unwrap().unwrap();
                    let jukebox = level
                        .block_entity::<entity::jukebox::JukeboxBlockEntity>(handle)
                        .unwrap();
                    assert_eq!(jukebox.parent.block_state, expected, "{line}");
                    assert!(jukebox.item.is_empty(), "{line}");
                    assert!(jukebox.song_player.song.is_none(), "{line}");
                }
                jukebox_queries += 1;
            }
            "structure-author" => {
                let (mut level, player) = super::tests::level();
                level
                    .entity_mut::<EntityData>(player)
                    .unwrap()
                    .entity_data
                    .set(
                        crate::entity::synched_data::CUSTOM_NAME,
                        Some(Box::new(Component::literal("different display"))),
                    );
                let state = Block::STRUCTURE_BLOCK.default_state();
                let handle = install(&mut level, state);
                level
                    .set_placed_by(state, POS, Some(player), unread(player))
                    .unwrap();
                assert_eq!(
                    level
                        .block_entity::<StructureBlockEntity>(handle)
                        .unwrap()
                        .author,
                    f[1]
                );
            }
            _ => {}
        }
    }
    assert_eq!((commands, jukebox_queries), (1152, 5));
}

/// A replaced hand must not change the captured setPlacedBy receiver. The held
/// component read also sees an in-place edit made after the replacement.
#[test]
fn command_placement_reads_the_retained_stack_after_hand_replacement() {
    let (mut level, player) = super::tests::level();
    let state = Block::COMMAND_BLOCK.default_state();
    let handle = install(&mut level, state);
    level
        .block_entity_mut::<CommandBlockEntity>(handle)
        .unwrap()
        .command_block
        .track_output = false;
    level.rules_mut().send_command_feedback = true;
    let slot = ContainerSlot {
        container: ContainerHandle::PlayerInventory(player),
        slot: 0,
    };
    level
        .replace_stack_object(slot, StackValue::New(held(Block::COMMAND_BLOCK, None)))
        .unwrap();
    let mut operation = StackOperation::new(&mut level);
    let original = operation.pin(slot).unwrap();
    operation
        .level
        .replace_stack_object(slot, StackValue::New(held(Block::COMMAND_BLOCK, None)))
        .unwrap();
    operation
        .level
        .stack_mut(original)
        .unwrap()
        .0
        .as_mut()
        .unwrap()
        .components
        .set(DataComponent::BlockEntityData(TypedBlockEntityData {
            entity_type: BlockEntityType(Identifier::parse("minecraft:chest").unwrap()),
            tag: CustomData(Tag::Compound(CompoundTag::new())),
        }));
    operation
        .level
        .set_placed_by(state, POS, Some(player), StackView::Retained(original))
        .unwrap();
    assert!(
        !operation
            .level
            .block_entity::<CommandBlockEntity>(handle)
            .unwrap()
            .command_block
            .track_output
    );
}

/// Vanilla's first match depends on origin/orientation traversal, with wall and
/// standing skulls, tag members and all nine single-cell obstructions.
#[test]
fn native_wither_pattern_and_placement_guards() {
    let (mut level, _) = super::tests::level();
    let mut count = 0;
    for line in vectors().lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "wither-pattern" {
            continue;
        }
        for pos in BlockPos::between_closed(POS.offset(-3, -3, -3), POS.offset(5, 5, 5)) {
            level
                .set_block_state(pos, Block::AIR.default_state())
                .unwrap();
        }
        let forwards = Direction::VALUES[f[1].parse::<usize>().unwrap()];
        let up = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let changed: i32 = f[3].parse().unwrap();
        let origin = POS.offset(1, 1, 1);
        let right = forwards.step().cross(up.step());
        for x in 0..3 {
            for y in 0..3 {
                let pos = origin.offset(
                    right.x * x - up.step().x * y,
                    right.y * x - up.step().y * y,
                    right.z * x - up.step().z * y,
                );
                let block = if x * 3 + y == changed {
                    Block::STONE
                } else if y == 0 {
                    if x & 1 == 0 {
                        Block::WITHER_SKELETON_SKULL
                    } else {
                        Block::WITHER_SKELETON_WALL_SKULL
                    }
                } else if y == 1 || x == 1 {
                    Block::SOUL_SAND
                } else {
                    Block::AIR
                };
                level.set_block_state(pos, block.default_state()).unwrap();
            }
        }
        let actual = super::super::wither::find_pattern(&level, POS).unwrap();
        let formatted = actual.map_or_else(
            || "none".into(),
            |value| {
                format!(
                    "{}, {}, {}:{}:{}",
                    value.origin.x,
                    value.origin.y,
                    value.origin.z,
                    value.forwards.data_3d(),
                    value.up.data_3d()
                )
            },
        );
        assert_eq!(formatted, f[4], "{line}");
        count += 1;
    }
    assert_eq!(count, 240);
    // No-match and Peaceful return normally; a successful pattern reaches the
    // required WITHER factory before any source block is cleared.
    let skull = Block::WITHER_SKELETON_SKULL.default_state();
    let handle = install(&mut level, skull);
    level.set_difficulty(Difficulty::Normal);
    for pos in BlockPos::between_closed(POS.offset(-3, -3, -3), POS.offset(5, 5, 5)) {
        level
            .set_block_state(pos, Block::AIR.default_state())
            .unwrap();
    }
    super::super::wither::check_spawn(POS, &mut level).unwrap();
    for x in 0..3 {
        level.set_block_state(POS.offset(x, 0, 0), skull).unwrap();
        level
            .set_block_state(POS.offset(x, -1, 0), Block::SOUL_SAND.default_state())
            .unwrap();
    }
    level
        .set_block_state(POS.offset(1, -2, 0), Block::SOUL_SAND.default_state())
        .unwrap();
    assert!(
        super::super::wither::find_pattern(&level, POS)
            .unwrap()
            .is_some()
    );
    level.set_difficulty(Difficulty::Peaceful);
    super::super::wither::check_spawn(POS, &mut level).unwrap();
    level.set_difficulty(Difficulty::Normal);
    assert!(matches!(
        super::super::wither::check_spawn(POS, &mut level),
        Err(LevelError::UnsupportedEntity(
            ferraloom_data::EntityType::WITHER
        ))
    ));
    assert!(!level.block_entity_state(handle).unwrap().data().removed);
    assert_eq!(level.get_block_state(POS).unwrap(), skull);
}
