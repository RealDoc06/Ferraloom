use super::tests::level;
use crate::{
    block::{
        BlockMethod, BlockPlaceContext, DirectionalPlaceContext, LevelReaderRef, PlacementHit,
        UseOnContext,
    },
    commands::permissions::PermissionLevel,
    entity::{EntityData, player::ServerPlayer},
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    component::{
        BlockEntityType, BlockItemStateProperties, CustomData, DataComponent, DataComponentPatch,
        ItemStack, TypedBlockEntityData,
    },
    login::GameProfile,
    play::InteractionHand,
};
use ferraloom_util::{
    BlockPos, Direction, Uuid, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{io::Read, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 5, 8);
fn held(block: Block, component: &str) -> ItemStack {
    let mut patch = DataComponentPatch::default();
    if let Some(mode) = component.strip_prefix("mode:") {
        patch.set(DataComponent::BlockState(BlockItemStateProperties(vec![(
            "mode".into(),
            mode.into(),
        )])));
    } else if component.starts_with("book:") {
        let mut tag = CompoundTag::new();
        match component {
            "book:compound" | "book:wrong_type" => {
                tag.insert("Book", Tag::Compound(CompoundTag::new()));
            }
            "book:int" => {
                tag.insert("Book", Tag::Int(7));
            }
            "book:wrong_key" => {
                tag.insert("book", Tag::Compound(CompoundTag::new()));
            }
            "book:absent" => {}
            _ => unreachable!(),
        }
        patch.set(DataComponent::BlockEntityData(TypedBlockEntityData {
            entity_type: BlockEntityType(
                ferraloom_protocol::Identifier::parse(if component == "book:wrong_type" {
                    "minecraft:chest"
                } else {
                    "minecraft:lectern"
                })
                .unwrap(),
            ),
            tag: CustomData(Tag::Compound(tag)),
        }));
    }
    ItemStack::new(block.as_item(), 1, patch)
}
/// Native masks distinguish legacy-solid wall support, full/center support,
/// hanging-sign axis chains, the captured fluid, signals, rotation boundaries,
/// and real game-master permissions independent of the player's Survival mode.
#[test]
fn native_sign_skull_and_utility_placement_masks_match_vanilla() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/furniture-placement.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let (mut level, player) = level();
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    level.set_fluid_tags(Arc::new(
        TagSet::<ferraloom_data::registries::fluid::Id>::load(&resources).unwrap(),
    ));
    level.install_shared(Box::<crate::ServerShared>::default());
    let profile = GameProfile {
        id: Uuid::new(1, 2),
        name: "metadata".into(),
        properties: vec![],
    };
    let mut blocks = std::collections::BTreeSet::new();
    let mut groups = std::collections::BTreeMap::new();
    for line in vectors.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[2].parse().unwrap()).unwrap();
        let yaw = f32::from_bits(u32::from_str_radix(f[3], 16).unwrap());
        let pitch = f32::from_bits(u32::from_str_radix(f[4], 16).unwrap());
        let face = Direction::VALUES[f[6].parse::<usize>().unwrap()];
        let directional: i32 = f[7].parse().unwrap();
        let capability: u8 = f[10].parse().unwrap();
        {
            let data = level.entity_mut::<EntityData>(player).unwrap();
            data.yaw = yaw;
            data.pitch = pitch;
            data.set_shared_flag(1, f[5] == "true");
        }
        level.entity_mut::<ServerPlayer>(player).unwrap().instabuild = matches!(capability, 1 | 2);
        level.shared_mut().unwrap().ops.set_operator(
            &profile,
            Some(PermissionLevel::by_id(if capability >= 2 { 2 } else { 0 })),
        );
        level
            .set_block_state(POS, BlockState::from_id(f[8].parse().unwrap()).unwrap())
            .unwrap();
        for (side, state) in Direction::VALUES.into_iter().zip(f[9].split(',')) {
            level
                .set_block_state(
                    POS.relative(side, 1),
                    BlockState::from_id(state.parse().unwrap()).unwrap(),
                )
                .unwrap();
        }
        let stack = held(block, f[11]);
        let reader = LevelReaderRef::Server(&level);
        let actual = if directional >= 0 {
            let context = DirectionalPlaceContext::new(
                &reader,
                POS,
                Direction::VALUES[directional as usize],
                &stack,
                face,
            )
            .unwrap();
            level.set_random_streams(Random::Legacy(LegacyRandomSource::new(263777)), 0);
            level.get_state_for_placement(block, &context)
        } else {
            let context = BlockPlaceContext::new(
                &reader,
                UseOnContext::new(
                    (directional != -2).then_some(player),
                    InteractionHand::MainHand,
                    &stack,
                    PlacementHit {
                        pos: POS,
                        face,
                        location: Vec3::new(8.5, 5.5, 8.5),
                        inside: false,
                    },
                ),
            )
            .unwrap();
            level.set_random_streams(Random::Legacy(LegacyRandomSource::new(263777)), 0);
            level.get_state_for_placement(block, &context)
        }
        .unwrap_or_else(|error| panic!("{line}: {error:?}"));
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[12].parse::<i32>().unwrap(),
            "{line}"
        );
        assert_eq!(
            level.random().next_int(),
            f[13].parse::<i32>().unwrap(),
            "RNG {line}"
        );
        blocks.insert(block);
        *groups.entry(f[1]).or_insert(0) += 1;
    }
    assert_eq!(blocks.len(), 135);
    assert_eq!(groups.len(), 14);
    for block in blocks {
        assert!(super::supports(block, BlockMethod::GetStateForPlacement));
    }
}
