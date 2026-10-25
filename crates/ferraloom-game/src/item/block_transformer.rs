//! Immutable BlockTransformer data and its live synchronous item-use pipeline.
use crate::{
    block::{InteractionResult, LevelAccessRef, UPDATE_LIMIT, UpdateFlags, UseOnContext},
    entity::player::{ServerPlayer, inventory},
    inventory::StackOperation,
    level::LevelError,
};
use ferraloom_data::{
    BlockClass, Item,
    datapack::{Identifier, ResourceManager},
    properties as p,
    registries::{game_event, sound_event},
};
use ferraloom_protocol::{
    codec::Holder,
    component::{ComponentTypeId, EquipmentSlot, Persistent},
    play::{InteractionHand, SoundEventHolder, SoundSource},
};
use ferraloom_util::{Direction, Vec3};
use ferraloom_worldgen::{
    Error,
    feature::{
        FeatureContext, FeatureLoader, FeatureRandomSelector, WorldGenLevel,
        state_provider::BlockStateProvider,
    },
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DropStrategy {
    ClickedFace,
    FromMiddle,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransformType {
    SingleBlock,
    CopperChest,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransformParticle {
    None,
    Scrape,
    WaxOn,
    WaxOff,
}
impl TransformParticle {
    fn event(self) -> Option<i32> {
        match self {
            Self::None => None,
            Self::Scrape => Some(3005),
            Self::WaxOn => Some(3003),
            Self::WaxOff => Some(3004),
        }
    }
}
#[derive(Debug)]
struct BlockTransformData {
    provider: Arc<BlockStateProvider>,
    sound: SoundEventHolder,
    particle: TransformParticle,
    disallowed_faces: Vec<Direction>,
    loot: Option<ferraloom_protocol::Identifier>,
    drop_strategy: DropStrategy,
    update_from_neighbors: bool,
    transform_type: TransformType,
    consume_on_use: bool,
    item_damage_per_use: i32,
}
#[derive(Debug)]
pub struct BlockTransformer {
    transforms: Vec<BlockTransformData>,
}

/// Registry resources are merged by ResourceManager before decoding; providers
/// and predicates use the existing worldgen loader, including named holders.
#[derive(Debug)]
pub struct BlockTransformerRegistry {
    entries: BTreeMap<Identifier, Arc<BlockTransformer>>,
}
impl BlockTransformerRegistry {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        let mut loader = FeatureLoader::new(resources)?;
        let mut entries = BTreeMap::new();
        for (id, resource) in resources.registry("block_transformer") {
            let value = resource.json()?;
            let transforms = value
                .as_array()
                .filter(|values| (1..=200).contains(&values.len()))
                .ok_or_else(|| {
                    Error(format!(
                        "{id}: block transformer requires 1..=200 transforms"
                    ))
                })?;
            let transforms = transforms
                .iter()
                .map(|value| BlockTransformData::decode(value, &mut loader))
                .collect::<Result<_, _>>()?;
            entries.insert(id, Arc::new(BlockTransformer { transforms }));
        }
        Ok(Self { entries })
    }
    fn get(&self, id: &Identifier) -> Option<Arc<BlockTransformer>> {
        self.entries.get(id).cloned()
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, Error> {
    value
        .get(key)
        .ok_or_else(|| Error(format!("block transformer missing {key}")))
}
fn string(value: &Value) -> Result<&str, Error> {
    value
        .as_str()
        .ok_or_else(|| Error("expected block transformer string".into()))
}
fn flag(value: &Value, key: &str, default: bool) -> Result<bool, Error> {
    value
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| Error(format!("expected {key} boolean")))
        })
        .unwrap_or(Ok(default))
}
fn decode_sound(value: Option<&Value>) -> Result<SoundEventHolder, Error> {
    Ok(if let Some(sound) = value {
        if let Some(name) = sound.as_str() {
            let name: ferraloom_protocol::Identifier = name
                .parse()
                .map_err(|error: ferraloom_protocol::Error| Error(error.to_string()))?;
            let registered = sound_event::Id::from_name(&name.to_string())
                .ok_or_else(|| Error(format!("missing sound event {name}")))?;
            SoundEventHolder(Holder::Reference(
                registered.name().parse().expect("registered sound"),
            ))
        } else {
            // SoundEvent.DIRECT_CODEC uses lenientOptionalFieldOf for range.
            let location = ferraloom_protocol::Identifier::from_json(field(sound, "sound_id")?)
                .map_err(|error| Error(error.to_string()))?;
            let fixed_range = sound
                .get("range")
                .and_then(|value| f32::from_json(value).ok())
                .map(ferraloom_protocol::component::JavaF32);
            SoundEventHolder(Holder::Direct(Box::new(
                ferraloom_protocol::play::SoundEvent {
                    location,
                    fixed_range,
                },
            )))
        }
    } else {
        SoundEventHolder(Holder::Reference(
            sound_event::Id::INTENTIONALLY_EMPTY
                .name()
                .parse()
                .expect("registered empty sound"),
        ))
    })
}

impl BlockTransformData {
    /// Port of BlockTransformData#CODEC; shared integer and provider codecs
    /// retain Java's numeric narrowing and datapack holder semantics.
    fn decode(value: &Value, loader: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        let provider = loader.state_provider(field(value, "block_state_provider")?)?;
        let sound = decode_sound(value.get("sound"))?;
        let particle = match value
            .get("particle")
            .map(string)
            .transpose()?
            .unwrap_or("none")
        {
            "none" => TransformParticle::None,
            "scrape" => TransformParticle::Scrape,
            "wax_on" => TransformParticle::WaxOn,
            "wax_off" => TransformParticle::WaxOff,
            other => return Err(Error(format!("invalid transform particle {other}"))),
        };
        let disallowed_faces = value
            .get("disallowed_faces")
            .map(|values| {
                values
                    .as_array()
                    .ok_or_else(|| Error("disallowed_faces must be a list".into()))?
                    .iter()
                    .map(|v| {
                        let name = string(v)?;
                        Direction::VALUES
                            .into_iter()
                            .find(|d| d.name() == name)
                            .ok_or_else(|| Error(format!("invalid transform face {name}")))
                    })
                    .collect::<Result<Vec<_>, Error>>()
            })
            .transpose()?
            .unwrap_or_default();
        let loot = value
            .get("loot")
            .map(|value| -> Result<ferraloom_protocol::Identifier, Error> {
                string(value)?
                    .parse()
                    .map_err(|error: ferraloom_protocol::Error| Error(error.to_string()))
            })
            .transpose()?;
        let drop_strategy = match value
            .get("drop_strategy")
            .map(string)
            .transpose()?
            .unwrap_or("from_middle")
        {
            "from_middle" => DropStrategy::FromMiddle,
            "clicked_face" => DropStrategy::ClickedFace,
            other => return Err(Error(format!("invalid transform drop strategy {other}"))),
        };
        let transform_type = match value
            .get("transform_type")
            .map(string)
            .transpose()?
            .unwrap_or("single_block")
        {
            "single_block" => TransformType::SingleBlock,
            "copper_chest" => TransformType::CopperChest,
            other => return Err(Error(format!("invalid transform type {other}"))),
        };
        let item_damage_per_use = value
            .get("item_damage_per_use")
            .map(|value| i32::from_json(value).map_err(|error| Error(error.to_string())))
            .transpose()?
            .unwrap_or(0);
        if item_damage_per_use < 0 {
            return Err(Error("item_damage_per_use must be nonnegative".into()));
        }
        Ok(Self {
            provider,
            sound,
            particle,
            disallowed_faces,
            loot,
            drop_strategy,
            update_from_neighbors: flag(value, "update_from_neighbors", true)?,
            transform_type,
            consume_on_use: flag(value, "consume_on_use", true)?,
            item_damage_per_use,
        })
    }
}

/// Port of BlockTransformer#transformBlock. The provider's random stream is
/// Level.random; callbacks never take or replace that canonical stream.
pub(super) fn transform_block(
    item: Item,
    id: &Identifier,
    context: &UseOnContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<Option<InteractionResult>, LevelError> {
    let player = context.player();
    if context.hand() == InteractionHand::MainHand {
        let player = player.ok_or(LevelError::MissingPlacementPlayer)?;
        let player = operation.level.entity::<ServerPlayer>(player)?;
        if inventory::component(player.get_offhand_item(), ComponentTypeId::BLOCKS_ATTACKS)
            .is_some()
            && !player.is_secondary_use_active()
        {
            return Ok(Some(InteractionResult::Pass));
        }
    }
    let services = operation.level.context().clone();
    let transformer =
        services
            .block_transformers()?
            .get(id)
            .ok_or_else(|| LevelError::ItemData {
                item,
                source: format!("missing block transformer {id}"),
            })?;
    let pos = context.clicked_pos();
    let face = context.clicked_face();
    for transform in &transformer.transforms {
        if transform.disallowed_faces.contains(&face) {
            continue;
        }
        let new_state = transform
            .provider
            .get_optional_state(
                &mut FeatureContext {
                    level: WorldGenLevel::Live(operation.level),
                    random: FeatureRandomSelector::Canonical,
                    origin: [pos.x, pos.y, pos.z],
                    generator: services.generator.access(),
                    registry: &services.features,
                },
                [pos.x, pos.y, pos.z],
            )
            .map_err(|source| LevelError::WorldgenMutationFailed { pos, source })?;
        let Some(new_state) = new_state else {
            continue;
        };
        let updated = if transform.update_from_neighbors {
            LevelAccessRef::Server(operation.level).update_from_neighbor_shapes(new_state, pos)?
        } else {
            new_state
        };
        let receiver = super::block_item::retained(context.stack_view())?;
        if let Some(player) = player {
            // Native ITEM_USED_ON_BLOCK captures a value before synchronous
            // rewards; the complete-use root keeps the live receiver afterwards.
            let stack = operation
                .level
                .stack(receiver)
                .map_err(super::stack::receiver_error)?;
            let captured = stack.copy_with_count(inventory::count(stack));
            crate::advancements::criteria::item_used_on_block(
                operation.level,
                player,
                pos,
                &captured,
            )?;
        }
        let old = operation.level.get_block_state(pos)?;
        if let Some(table) = &transform.loot {
            let block_entity = operation.level.block_entity_ref(pos)?;
            // TOOL is an owned LootParams value. Its synchronous predicates and
            // functions only read it and cannot observe stack identity; capture
            // at this native construction point, then reread the live receiver
            // for wear/consume after all loot and delivery callbacks finish.
            let captured = operation
                .level
                .stack(receiver)
                .map_err(super::stack::receiver_error)?
                .clone();
            crate::block::drops::drop_from_block_interact_loot_table_with_popper(
                operation.level,
                table,
                pos,
                old,
                block_entity,
                Some(&captured),
                player,
                |level, stack| {
                    match transform.drop_strategy {
                        DropStrategy::FromMiddle => {
                            crate::block::drops::pop_resource(level, pos, stack)?;
                        }
                        DropStrategy::ClickedFace => {
                            crate::block::drops::pop_resource_from_face(level, pos, face, stack)?;
                        }
                    }
                    Ok(())
                },
            )?;
        }
        let stackable = inventory::is_stackable(
            operation
                .level
                .stack(receiver)
                .map_err(super::stack::receiver_error)?,
        );
        if stackable {
            super::stack::consume(
                receiver,
                i32::from(transform.consume_on_use),
                player,
                operation,
            )?;
        } else if let Some(player) = player {
            let slot = match context.hand() {
                InteractionHand::MainHand => EquipmentSlot::Mainhand,
                InteractionHand::OffHand => EquipmentSlot::Offhand,
            };
            super::stack::hurt_and_break(
                receiver,
                transform.item_damage_per_use,
                player,
                slot,
                operation,
            )?;
        }
        operation
            .level
            .set_block(pos, updated, UpdateFlags::ALL_IMMEDIATE, UPDATE_LIMIT)?;
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        operation.level.play_sound_holder(
            player,
            center,
            transform.sound.clone(),
            SoundSource::Blocks,
            1.0,
            1.0,
        )?;
        if let Some(event) = transform.particle.event() {
            operation.level.level_event(player, event, pos, 0)?;
        }
        operation
            .level
            .game_event(game_event::Id::BLOCK_CHANGE, center, player, Some(updated))?;
        if transform.transform_type == TransformType::CopperChest
            && old.block().is_instance_of(BlockClass::CopperChestBlock)
            && old.get(p::TYPE_).is_some_and(|v| v.as_str() != "single")
        {
            let facing =
                Direction::by_name(old.get(p::FACING).expect("copper chest facing").as_str())
                    .expect("registered copper chest direction");
            let direction = if old.get(p::TYPE_).unwrap().as_str() == "left" {
                facing.clockwise()
            } else {
                facing.counter_clockwise()
            };
            let neighbor = pos.relative(direction, 1);
            let state = operation.level.get_block_state(neighbor)?;
            let at = Vec3::new(
                f64::from(neighbor.x) + 0.5,
                f64::from(neighbor.y) + 0.5,
                f64::from(neighbor.z) + 0.5,
            );
            operation
                .level
                .game_event(game_event::Id::BLOCK_CHANGE, at, player, Some(state))?;
            if let Some(event) = transform.particle.event() {
                operation.level.level_event(player, event, neighbor, 0)?;
            }
        }
        return Ok(Some(InteractionResult::SUCCESS));
    }
    Ok(Some(InteractionResult::Pass))
}

#[cfg(test)]
mod tests;
