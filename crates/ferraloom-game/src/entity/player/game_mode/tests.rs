use super::*;
use crate::{
    Behaviors,
    block::{BASE_BLOCK, BlockBehavior, UnportedBehavior},
    entity::{EntityData, EntityState, player::GameType},
    level::{LevelContext, Region, chunk::LevelChunk},
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::Block;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::component::{ItemStack, Persistent};
use ferraloom_util::{ChunkPos, Uuid, Vec3};
use std::sync::Arc;

const POS: BlockPos = BlockPos::new(8, 64, 8);
const VECTORS: &str = include_str!("../../../../tests/java/level/mining.tsv");
static EMPTY: Behaviors = Behaviors::EMPTY;

pub(super) fn fixture(water: bool, ground: bool) -> (Level, EntityRef) {
    let context = LevelContext::fixture(0, 63);
    let mut level = Level::new(Region::new(&EMPTY), context.clone());
    let data = context.generator.access().data();
    level.set_block_tags(data.block_tags.clone());
    level.set_fluid_tags(data.fluid_tags.clone());
    let section = LevelChunkSection::new(
        Strategy::block_states(),
        Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
        BiomeId(0),
    );
    let mut chunk = LevelChunk::new(ChunkPos::ZERO, -4..20, section);
    if water {
        for y in 64..=67 {
            chunk
                .set_block_state(BlockPos::new(8, y, 8), Block::WATER.default_state())
                .unwrap();
        }
    }
    level.add_chunk(chunk).unwrap();
    let player = level.next_entity_ref();
    let mut state = ServerPlayer::new(
        Uuid::offline_player("Mining"),
        "Mining".into(),
        level.next_entity_id(),
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    state.on_ground = ground;
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    crate::entity::fluid::update_fluid_interaction(
        level.entity_behavior(player).unwrap(),
        player,
        &mut level,
    )
    .unwrap();
    assert_eq!(
        level
            .entity::<EntityData>(player)
            .unwrap()
            .fluid_interaction
            .eyes_inside(FluidTags::WATER, &level)
            .unwrap(),
        water
    );
    (level, player)
}
fn double(bits: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
}
fn float(bits: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(bits, 16).unwrap())
}

#[test]
fn player_speed_and_block_progress_match_real_native_float_bits() {
    let mut count = 0;
    for line in VECTORS.lines().filter(|line| line.starts_with("mining\t")) {
        let fields: Vec<_> = line.split('\t').collect();
        let (mut level, player) = fixture(fields[10] == "true", fields[11] == "true");
        let stack = if fields[3] == "-" {
            ItemStack::default()
        } else {
            ItemStack::from_nbt(&Tag::Compound(
                ferraloom_nbt::parse_compound(fields[3]).unwrap(),
            ))
            .unwrap()
        };
        let entity = level.entity_mut::<ServerPlayer>(player).unwrap();
        entity.inventory.set_selected_item(stack);
        for (attribute, value) in [
            (Attribute::MINING_EFFICIENCY, fields[4]),
            (Attribute::BLOCK_BREAK_SPEED, fields[5]),
            (Attribute::SUBMERGED_MINING_SPEED, fields[6]),
        ] {
            if value != "-" {
                entity
                    .attributes
                    .get_instance(attribute)
                    .unwrap()
                    .set_base_value(double(value));
            }
        }
        // Hydrate the canonical effects map; no second mining-effect snapshot.
        let effects = [
            (MobEffect::HASTE, fields[7]),
            (MobEffect::CONDUIT_POWER, fields[8]),
            (MobEffect::MINING_FATIGUE, fields[9]),
        ]
        .into_iter()
        .filter_map(|(effect, amplifier)| {
            let amplifier: i32 = amplifier.parse().unwrap();
            (amplifier >= 0).then(|| {
                let mut tag = CompoundTag::new();
                tag.insert("id", Tag::String(effect.name().into()));
                tag.insert("amplifier", Tag::Int(amplifier));
                tag.insert("duration", Tag::Int(200));
                Tag::Compound(tag)
            })
        })
        .collect();
        assert!(
            entity
                .active_effects
                .load(Some(&Tag::List(ListTag(effects))))
                .is_empty()
        );
        let state = Block::from_name(fields[2]).unwrap().default_state();
        let tool_speed = inventory::get_destroy_speed(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .inventory
                .selected_item(),
            state,
            &level,
        )
        .unwrap();
        assert_eq!(tool_speed.to_bits(), float(fields[12]).to_bits(), "{line}");
        assert_eq!(
            get_destroy_speed(player, state, &mut level)
                .unwrap()
                .to_bits(),
            float(fields[13]).to_bits(),
            "{line}"
        );
        assert_eq!(
            level
                .get_destroy_progress(state, POS, player)
                .unwrap()
                .to_bits(),
            float(fields[14]).to_bits(),
            "{line}"
        );
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .has_correct_tool_for_drops(state, &level)
                .unwrap(),
            fields[15] == "true",
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 308);
}

#[test]
fn native_delayed_progress_operand_and_wrapping_clock_are_preserved() {
    let per_tick = VECTORS
        .lines()
        .find(|line| line.starts_with("progress\t"))
        .map(|line| float(line.split('\t').nth(1).unwrap()))
        .unwrap();
    let mut counts = (0, 0);
    for line in VECTORS.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        match fields[0] {
            "tick" => {
                let mut state = BlockBreakingState {
                    is_destroying_block: true,
                    has_delayed_destroy: fields[1] == "true",
                    game_ticks: fields[2].parse().unwrap(),
                    destroy_progress_start: fields[3].parse().unwrap(),
                    delayed_tick_start: fields[4].parse().unwrap(),
                    ..Default::default()
                };
                state.game_ticks = state.game_ticks.wrapping_add(1);
                let (_, stage) =
                    accumulated_destroy_progress(per_tick, state.progress_ticks().unwrap());
                assert_eq!(
                    state.game_ticks,
                    fields[5].parse::<i32>().unwrap(),
                    "{line}"
                );
                assert_eq!(stage, fields[6].parse::<i32>().unwrap(), "{line}");
                counts.0 += 1;
            }
            "progress" => {
                let (total, stage) =
                    accumulated_destroy_progress(float(fields[1]), fields[2].parse().unwrap());
                assert_eq!(total.to_bits(), float(fields[3]).to_bits(), "{line}");
                assert_eq!(stage, fields[4].parse::<i32>().unwrap(), "{line}");
                counts.1 += 1;
            }
            _ => (),
        }
    }
    assert_eq!(counts, (8, 10));
    assert!(BlockBreakingState::default().progress_ticks().is_none());
}

#[test]
fn first_underwater_query_creates_and_saves_the_canonical_attribute_instance() {
    let fields: Vec<_> = VECTORS
        .lines()
        .find(|line| line.starts_with("lazy-attribute\t"))
        .unwrap()
        .split('\t')
        .collect();
    let (mut level, player) = fixture(true, true);
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .inventory
        .set_selected_item(ItemStack::new(
            ferraloom_data::Item::DIAMOND_PICKAXE,
            1,
            Default::default(),
        ));
    let before = level
        .entity::<ServerPlayer>(player)
        .unwrap()
        .attributes
        .save_nbt();
    // These actual native packed lists are empty -> precisely the new instance;
    // unlike value reads, getInstance affects persistent player attributes.
    let parse = |text: &str| {
        ferraloom_nbt::parse_compound(&format!("{{attributes:{text}}}"))
            .unwrap()
            .get("attributes")
            .unwrap()
            .clone()
    };
    assert_eq!(before, parse(fields[1]));
    let speed = get_destroy_speed(player, Block::STONE.default_state(), &mut level).unwrap();
    assert_eq!(speed.to_bits(), float(fields[3]).to_bits());
    assert_eq!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .attributes
            .save_nbt(),
        parse(fields[2])
    );
}

#[test]
fn unbreakable_progress_skips_player_reads_and_player_destroy_requires_actor() {
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    let player = EntityRef(u64::MAX);
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        level.set_unported_behavior(policy);
        assert_eq!(
            level
                .get_destroy_progress(Block::BEDROCK.default_state(), POS, player)
                .unwrap()
                .to_bits(),
            0
        );
        assert!(matches!(
            BASE_BLOCK.player_destroy(
                crate::block::breaking::PlayerDestroyContext {
                    block: Block::STONE, state: Block::STONE.default_state(), pos: POS, player,
                    block_entity: None, destroyed_with: &ItemStack::default(),
                },
                &mut level
            ),
            Err(LevelError::MissingEntity(entity)) if entity==player
        ));
    }
    assert!(VECTORS.lines().any(|line| line == "hardness-null\t0"));
}
