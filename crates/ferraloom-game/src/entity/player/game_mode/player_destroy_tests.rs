use super::*;
use crate::{
    Behaviors, ServerShared,
    block::{
        BASE_BLOCK, BlockBehavior, BlockMethod, UnportedBehavior, UpdateFlags,
        breaking::PlayerDestroyContext, containers::storage::BaseContainerBlockEntity,
        entity::create,
    },
    entity::{EntityState, item::item_entity::ItemEntity, player::GameType},
    inventory::{ContainerHandle, ContainerSlot, StackOperation},
    level::{
        LevelContext, Region,
        chunk::LevelChunk,
        effects::LevelEffect,
        game_event::test_support::{self as game_events, Observation},
    },
    stats::Stat,
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::{Block, Item, properties as p};
use ferraloom_protocol::{
    chat::{Component, Contents},
    codec::Holder,
    component::{DataComponent, ItemStack, Persistent},
    play::clientbound as cb,
};
use ferraloom_util::{
    ChunkPos, Uuid, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::sync::Arc;

const POS: BlockPos = BlockPos::new(8, 64, 8);
const VECTORS: &str = include_str!("../../../../tests/java/level/player-destroy.tsv");
static EMPTY: Behaviors = Behaviors::EMPTY;

pub(super) fn fixture(behaviors: &'static Behaviors) -> (Level, EntityRef) {
    let context = LevelContext::fixture(0, 63);
    let mut level = Level::new(Region::new(behaviors), context.clone());
    let data = context.generator.access().data();
    level.set_block_tags(data.block_tags.clone());
    level.set_fluid_tags(data.fluid_tags.clone());
    for x in -1..=1 {
        for z in -1..=1 {
            let section = LevelChunkSection::new(
                Strategy::block_states(),
                Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                BiomeId(0),
            );
            let pos = ChunkPos::new(x, z);
            level
                .add_chunk(LevelChunk::new(pos, -4..20, section))
                .unwrap();
            level
                .set_chunk_loading_status(
                    pos,
                    crate::level::distance_manager::FullChunkStatus::BlockTicking,
                    true,
                )
                .unwrap();
            level.start_ticking_chunk(pos).unwrap();
            // Native BLOCK_TICKING chunks expose their tracked entities.
            level
                .update_entity_chunk_status(pos, crate::level::entity::Visibility::Tracked)
                .unwrap();
        }
    }
    level.install_shared(Box::new(ServerShared::default()));
    crate::level::drop_fixtures::install(&mut level);
    let player = level.next_entity_ref();
    let mut miner = ServerPlayer::new(
        Uuid::offline_player("Destroy"),
        "Destroy".into(),
        level.next_entity_id(),
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    miner.on_ground = true;
    level
        .add_entity(player, EntityState::Player(Box::new(miner)))
        .unwrap();
    level.take_effects();
    game_events::observe(&mut level);
    (level, player)
}
fn seed(level: &mut Level, seed: &str) {
    level.set_random_streams(
        Random::Legacy(LegacyRandomSource::new(seed.parse().unwrap())),
        0,
    );
    level.set_sound_seed(0);
    game_events::take(level);
}
fn rows(kind: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    VECTORS
        .lines()
        .map(|row| row.split('\t').collect::<Vec<_>>())
        .filter(move |fields| fields[0] == kind)
}
fn tool(item: Item, damage: i32) -> ItemStack {
    ItemStack::from_nbt(&ferraloom_nbt::Tag::Compound(ferraloom_nbt::parse_compound(&format!(
        "{{id:\"{}\",count:1,components:{{\"minecraft:tool\":{{rules:[],damage_per_block:{damage}}}}}}}", item.name()
    )).unwrap())).unwrap()
}
fn put_named_container(
    level: &mut Level,
    block: Block,
    name: &str,
) -> crate::block::entity::BlockEntityRef {
    let state = block.default_state();
    level.set_block_state(POS, state).unwrap();
    level.set_block_entity(create(POS, state).unwrap()).unwrap();
    let handle = level.block_entity_ref(POS).unwrap().unwrap();
    level
        .block_entity_mut::<BaseContainerBlockEntity>(handle)
        .unwrap()
        .custom_name = Some(Component::literal(name));
    handle
}
fn literal(component: &Component) -> &str {
    let Contents::Text(text) = &component.contents else {
        panic!("fixture literal component")
    };
    text
}
fn item_trace(level: &Level) -> Vec<String> {
    let mut result = Vec::new();
    for entity in level.all_entities() {
        let Ok(item) = level.entity::<ItemEntity>(entity) else {
            continue;
        };
        let stack = item.item().0.as_ref().unwrap();
        assert_eq!(item.pickup_delay, 10);
        let position = item.parent.position;
        result.push(format!(
            "item:{}:{}:{:x},{:x},{:x}:private-rng",
            stack.item.name(),
            stack.count,
            position.x.to_bits(),
            position.y.to_bits(),
            position.z.to_bits()
        ));
        if let Some(DataComponent::CustomName(name)) = inventory::component(
            item.item(),
            ferraloom_data::registries::data_component_type::Id::CUSTOM_NAME,
        ) {
            result.push(format!("name:{}", literal(name)));
        }
    }
    result
}
fn assert_output(level: &mut Level, player: EntityRef, fields: &[&str], start: usize) {
    assert_eq!(
        level.get_block_state(POS).unwrap().id().to_string(),
        fields[start],
        "{fields:?}"
    );
    assert_eq!(
        level.random().next_int().to_string(),
        fields[start + 1],
        "{fields:?}"
    );
    assert_eq!(
        format!(
            "{:x}",
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .food_data
                .exhaustion_level()
                .to_bits()
        ),
        fields[start + 2],
        "{fields:?}"
    );
    assert_eq!(
        item_trace(level),
        fields[start + 3]
            .split(';')
            .filter(|entry| entry.starts_with("item:") || entry.starts_with("name:"))
            .collect::<Vec<_>>(),
        "{fields:?}"
    );
}
fn effects(level: &mut Level, player: EntityRef) -> Vec<String> {
    game_events::take(level)
        .into_iter()
        .filter_map(|effect| match effect {
            Observation::Effect(LevelEffect::PlayerPacket { recipients, packet }) => {
                match *packet {
                    cb::Packet::LevelEvent(event) => Some(format!(
                        "event:{}:{}:{}",
                        event.event_type,
                        // playerWillDestroy excludes the player; later egg events do not.
                        if event.event_type == 2001 && !recipients.contains(&player) {
                            assert!(recipients.is_empty());
                            "breaker"
                        } else {
                            assert_eq!(recipients, [player]);
                            "none"
                        },
                        event.data
                    )),
                    cb::Packet::Sound(sound) => {
                        assert_eq!(recipients, [player]);
                        let Holder::Reference(id) = sound.sound.0 else {
                            panic!("registered sound")
                        };
                        Some(format!(
                            "sound:{id}:{:x}:{:x}",
                            sound.volume.to_bits(),
                            sound.pitch.to_bits()
                        ))
                    }
                    _ => None,
                }
            }
            Observation::GameEvent {
                event,
                source,
                affected_state,
                ..
            } => Some(format!(
                "game:{}:{}:{}",
                event.name(),
                if source == Some(player) {
                    "breaker"
                } else {
                    assert_eq!(source, None);
                    "none"
                },
                affected_state.unwrap().id()
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn base_player_destroy_keeps_original_receiver_and_detached_be_loot_with_native_rng() {
    let mut count = 0;
    for fields in rows("base") {
        let (mut level, player) = fixture(&EMPTY);
        let block = Block::from_name(fields[1]).unwrap();
        let original = if block == Block::CHEST {
            let original = put_named_container(&mut level, Block::CHEST, "original");
            level.retain_block_entity(original).unwrap();
            level.remove_block_entity(POS).unwrap();
            put_named_container(&mut level, Block::BARREL, "replacement");
            Some(original)
        } else {
            None
        };
        level.rules_mut().block_drops = Some(fields[2].parse().unwrap());
        seed(&mut level, fields[3]);
        BASE_BLOCK
            .player_destroy(
                PlayerDestroyContext {
                    block: Block::STONE,
                    state: block.default_state(),
                    pos: POS,
                    player,
                    block_entity: original,
                    destroyed_with: &ItemStack::default(),
                },
                &mut level,
            )
            .unwrap();
        let miner = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(miner.stats.get_value(Stat::Mined(Block::STONE)), 1);
        if block != Block::STONE {
            assert_eq!(miner.stats.get_value(Stat::Mined(block)), 0);
        }
        assert_output(&mut level, player, &fields, 4);
        if let Some(original) = original {
            level.release_block_entity(original);
        }
        count += 1;
    }
    assert_eq!(count, 18);
}

#[test]
fn required_loot_failure_keeps_the_preceding_stat_and_exhaustion_in_both_policies() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let (mut level, player) = fixture(&EMPTY);
        level.set_unported_behavior(policy);
        level.container_runtime_mut().resources = None;
        seed(&mut level, "0");
        let error = BASE_BLOCK
            .player_destroy(
                PlayerDestroyContext {
                    block: Block::STONE,
                    state: Block::OAK_PLANKS.default_state(),
                    pos: POS,
                    player,
                    block_entity: None,
                    destroyed_with: &ItemStack::default(),
                },
                &mut level,
            )
            .unwrap_err();
        assert_eq!(error, LevelError::Inventory("container datapack resources"));
        let miner = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(miner.stats.get_value(Stat::Mined(Block::STONE)), 1);
        assert_eq!(
            miner.food_data.exhaustion_level().to_bits(),
            0.005_f32.to_bits()
        );
        assert!(item_trace(&level).is_empty());
        assert_eq!(level.random().next_int(), -1155484576);
    }
}

struct RemovalCallback {
    replace: bool,
}
static SWAP: RemovalCallback = RemovalCallback { replace: false };
static REPLACE: RemovalCallback = RemovalCallback { replace: true };
static SWAP_BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::CHEST, &SWAP);
static REPLACE_BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::CHEST, &REPLACE);
impl BlockBehavior for RemovalCallback {
    fn player_will_destroy(
        &self,
        state: BlockState,
        pos: BlockPos,
        player: EntityRef,
        level: &mut Level,
    ) -> Result<BlockState, LevelError> {
        let state = BASE_BLOCK.player_will_destroy(state, pos, player, level)?;
        // Same controlled boundary as the native removeBlock fixture: after
        // playerWillDestroy's effects, before the first removal operation.
        level
            .entity_mut::<ServerPlayer>(player)?
            .inventory
            .set_selected_item(tool(Item::BLAZE_ROD, 0));
        if self.replace {
            level.set_block(
                pos,
                Block::BARREL.default_state(),
                UpdateFlags::KNOWN_SHAPE,
                512,
            )?;
            let current = level.block_entity_ref(pos)?.unwrap();
            level
                .block_entity_mut::<BaseContainerBlockEntity>(current)?
                .custom_name = Some(Component::literal("replacement"));
        }
        Ok(state)
    }
    fn player_destroy(
        &self,
        context: PlayerDestroyContext<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        assert_eq!(context.block, Block::CHEST);
        assert_eq!(context.state.block(), Block::CHEST);
        assert_eq!(context.destroyed_with.item(), Item::BLAZE_ROD);
        let original = context.block_entity.unwrap();
        assert!(level.block_entity_state(original)?.data().removed);
        assert_eq!(
            literal(
                level
                    .block_entity::<BaseContainerBlockEntity>(original)?
                    .custom_name
                    .as_ref()
                    .unwrap()
            ),
            "original"
        );
        BASE_BLOCK.player_destroy(context, level)
    }
}
#[test]
fn removal_callbacks_keep_original_be_and_use_the_post_callback_hand_before_wear() {
    for fields in rows("callback") {
        let (mut level, player) = fixture(if fields[1] == "true" {
            &REPLACE_BEHAVIORS
        } else {
            &SWAP_BEHAVIORS
        });
        put_named_container(&mut level, Block::CHEST, "original");
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .inventory
            .set_selected_item(tool(Item::STICK, 0));
        seed(&mut level, "0");
        assert!(destroy_block(player, POS, &mut level).unwrap());
        assert_output(&mut level, player, &fields, 2);
        let miner = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(miner.stats.get_value(Stat::Used(Item::STICK)), 0);
        assert_eq!(miner.stats.get_value(Stat::Used(Item::BLAZE_ROD)), 1);
        assert_eq!(miner.stats.get_value(Stat::Mined(Block::CHEST)), 1);
        assert_eq!(miner.inventory.selected_item().item(), Item::BLAZE_ROD);
    }
}

#[test]
fn mine_block_native_tool_guards_award_used_only_after_the_wear_effect() {
    let mut count = 0;
    for fields in rows("mine") {
        let (mut level, player) = fixture(&EMPTY);
        let item = Item::from_name(fields[1]).unwrap();
        let stack = if fields[3] == "-" {
            ItemStack::new(item, 1, Default::default())
        } else {
            tool(item, fields[3].parse().unwrap())
        };
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .inventory
            .set_selected_item(stack);
        seed(&mut level, "0");
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation
            .pin(ContainerSlot {
                container: ContainerHandle::PlayerInventory(player),
                slot: 0,
            })
            .unwrap();
        mine_block(
            player,
            receiver,
            Block::from_name(fields[2]).unwrap().default_state(),
            operation.level,
        )
        .unwrap();
        assert_output(operation.level, player, &fields, 4);
        assert_eq!(
            operation
                .level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .stats
                .get_value(Stat::Used(item)),
            i32::from(fields[3] != "-")
        );
        count += 1;
    }
    assert_eq!(count, 7);
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let (mut level, player) = fixture(&EMPTY);
        level.set_unported_behavior(policy);
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .inventory
            .set_selected_item(tool(Item::STICK, 1));
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation
            .pin(ContainerSlot {
                container: ContainerHandle::PlayerInventory(player),
                slot: 0,
            })
            .unwrap();
        let result = mine_block(
            player,
            receiver,
            Block::STONE.default_state(),
            operation.level,
        );
        // Native STICK has no DAMAGE/MAX_DAMAGE, so the wear call is a no-op.
        result.unwrap();
        assert_eq!(operation.level.unported_behavior_stats().skipped_calls, 0);
        assert_eq!(
            operation
                .level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .stats
                .get_value(Stat::Used(Item::STICK)),
            1
        );
    }
}

#[test]
fn turtle_egg_player_destroy_matches_native_pitch_state_and_event_order() {
    let mut count = 0;
    for fields in rows("egg") {
        let (mut level, player) = fixture(&EMPTY);
        let eggs = fields[1].parse::<u8>().unwrap();
        level
            .set_block_state(
                POS.below(1),
                if fields[2] == "true" {
                    Block::SAND
                } else {
                    Block::STONE
                }
                .default_state(),
            )
            .unwrap();
        level
            .set_block_state(
                POS,
                Block::TURTLE_EGG
                    .default_state()
                    .with(p::EGGS, eggs)
                    .unwrap(),
            )
            .unwrap();
        seed(&mut level, fields[3]);
        assert!(destroy_block(player, POS, &mut level).unwrap());
        assert_output(&mut level, player, &fields, 4);
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .stats
                .get_value(Stat::Mined(Block::TURTLE_EGG)),
            1
        );
        // LevelEvent packets carry the captured audience, not the source. The
        // first 2001 excludes the miner; the later decreaseEggs 2001 does not.
        let actual = effects(&mut level, player);
        let expected = fields[7]
            .split(';')
            .filter(|entry| {
                entry.starts_with("event:")
                    || entry.starts_with("game:")
                    || entry.starts_with("sound:")
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{fields:?}");
        count += 1;
    }
    assert_eq!(count, 12);
}

#[test]
fn dragon_egg_attack_matches_native_candidates_packed_event_and_rng() {
    let mut count = 0;
    for fields in rows("dragon") {
        let (mut level, player) = fixture(&EMPTY);
        if fields[1] != "air" {
            for x in -7..=23 {
                for z in -7..=23 {
                    level
                        .set_block_state(BlockPos::new(x, 63, z), Block::STONE.default_state())
                        .unwrap();
                }
            }
        }
        if fields[1] == "border" {
            let mut border = crate::level::border::WorldBorder::default();
            border.set_size(1.0);
            level.set_entity_world_border(border);
        }
        let state = Block::DRAGON_EGG.default_state();
        level.set_block_state(POS, state).unwrap();
        seed(&mut level, fields[2]);
        level
            .required_block_behavior(Block::DRAGON_EGG, BlockMethod::Attack)
            .unwrap()
            .attack(state, POS, player, &mut level)
            .unwrap();
        assert_output(&mut level, player, &fields, 3);
        let expected_pos = rows("dragon-position")
            .find(|entry| entry[1..3] == fields[1..3])
            .unwrap();
        let target = expected_pos[3]
            .split(',')
            .map(|v| v.parse().unwrap())
            .collect::<Vec<i32>>();
        let target = BlockPos::new(target[0], target[1], target[2]);
        assert_eq!(level.get_block_state(target).unwrap(), state);
        let expected = fields[6]
            .split(';')
            .filter(|entry| entry.starts_with("event:"))
            .collect::<Vec<_>>();
        assert_eq!(effects(&mut level, player), expected, "{fields:?}");
        if target != POS {
            assert!(level.has_scheduled_block_tick(target, Block::DRAGON_EGG));
            let mut saved = ferraloom_storage::SerializableChunkData::default();
            level
                .save_chunk_ticks(ChunkPos::containing(target), &mut saved)
                .unwrap();
            assert!(
                saved
                    .block_ticks
                    .unwrap()
                    .iter()
                    .any(|tick| tick.x == Some(target.x)
                        && tick.y == Some(target.y)
                        && tick.z == Some(target.z)
                        && tick.delay == Some(5))
            );
        }
        count += 1;
    }
    assert_eq!(count, 9);
}

#[test]
fn dragon_egg_required_neighbor_read_keeps_the_six_candidate_draws() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let (mut level, player) = fixture(&EMPTY);
        level.set_unported_behavior(policy);
        let missing = ChunkPos::new(1, 0);
        level.remove_chunk(missing).unwrap();
        seed(&mut level, "1");
        let state = Block::DRAGON_EGG.default_state();
        level.set_block_state(POS, state).unwrap();
        let error = level
            .required_block_behavior(Block::DRAGON_EGG, BlockMethod::Attack)
            .unwrap()
            .attack(state, POS, player, &mut level)
            .unwrap_err();
        assert_eq!(error, LevelError::MissingChunk(missing));
        // The native floor/seed=1 vector accepts this first candidate. The
        // required read fails here, before border/height and before all writes.
        assert_eq!(level.random().next_int(), 1429008869);
        assert_eq!(level.get_block_state(POS).unwrap(), state);
        assert!(effects(&mut level, player).is_empty());
    }
}
