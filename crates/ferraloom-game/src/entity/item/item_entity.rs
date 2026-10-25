//! Port of net.minecraft.world.entity.item.ItemEntity.
use crate::{
    block::LevelReaderRef,
    entity::{
        Entity, EntityData, EntityDataView, EntityRef, EntityReference, EntityState, RemovalReason,
        fluid, inside_block,
        movement::{self, MovementEmission, MoverType},
        persistence,
        player::{
            ServerPlayer,
            inventory::{self, copy_with_count, count, max_stack_size, set_count},
        },
        synched_data::{EntityDataAccessor, SynchedValue},
    },
    inventory::StackGenerations,
    level::{Level, LevelError},
};
use ferraloom_data::{EntityType, tags::FluidTags};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ItemStack, Persistent};
use ferraloom_util::{Axis, BlockPos, Direction, Uuid, Vec3, mth, random::RandomSource};

pub const ITEM: EntityDataAccessor<ItemStack> = EntityDataAccessor::new(8);
#[derive(Debug)]
pub struct ItemEntity {
    pub parent: EntityData,
    pub age: i32,
    pub pickup_delay: i32,
    pub health: i32,
    pub thrower: Option<EntityReference<EntityData>>,
    pub target: Option<Uuid>,
    pub bob_offs: f32,
    stack_generations: StackGenerations,
}
impl ItemEntity {
    pub(crate) fn with_parent(mut parent: EntityData) -> Self {
        parent.entity_data.define(ITEM, ItemStack::default());
        let bob_offs = parent.random.next_float() * std::f32::consts::PI * 2.0;
        parent.yaw = parent.random.next_float() * 360.0;
        Self {
            parent,
            age: 0,
            pickup_delay: 0,
            health: 5,
            thrower: None,
            target: None,
            bob_offs,
            stack_generations: StackGenerations::new(1),
        }
    }
    pub fn new(network_id: i32, seed: i64) -> Self {
        Self::with_parent(EntityData::new(EntityType::ITEM, network_id, seed))
    }
    pub fn new_at(level: &mut Level, position: Vec3, stack: ItemStack) -> Self {
        let mut entity = Self::with_parent(level.create_entity_data(EntityType::ITEM));
        entity.parent.set_pos(position);
        entity.set_item(stack);
        entity.parent.velocity = Vec3::new(
            entity.parent.random.next_double() * 0.2 - 0.1,
            0.2,
            entity.parent.random.next_double() * 0.2 - 0.1,
        );
        entity
    }
    /// Port of ItemEntity's explicit-velocity constructor. Unlike new_at, this
    /// overload does not consume the entity RNG for a random launch velocity.
    pub fn new_at_with_velocity(
        level: &mut Level,
        position: Vec3,
        stack: ItemStack,
        velocity: Vec3,
    ) -> Self {
        let mut entity = Self::with_parent(level.create_entity_data(EntityType::ITEM));
        entity.parent.set_pos(position);
        entity.set_item(stack);
        entity.parent.velocity = velocity;
        entity
    }
    /// Port of ItemEntity#makeFakeItem, used by GiveCommand before registration.
    pub fn make_fake_item(&mut self) {
        self.pickup_delay = 32767;
        self.age = 5999;
    }
    pub fn item(&self) -> &ItemStack {
        self.parent.entity_data.get(ITEM)
    }
    pub fn item_mut(&mut self) -> &mut ItemStack {
        self.parent.entity_data.get_mut(ITEM)
    }
    pub fn set_item(&mut self, stack: ItemStack) {
        if self.item().same_value(&stack) {
            return;
        }
        let mut current = std::mem::take(self.parent.entity_data.get_mut(ITEM));
        self.stack_generations.replace(0, &mut current, stack);
        self.parent.entity_data.set_forced(ITEM, current, true);
    }
    /// ItemEntity#getItem's canonical receiver, addressed as entity slot zero.
    pub fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        (slot == 0).then(|| (&self.stack_generations, self.item(), 0))
    }
    pub fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        (slot == 0).then(|| {
            (
                &mut self.stack_generations,
                self.parent.entity_data.get_mut(ITEM),
                0,
            )
        })
    }
    pub fn is_mergable(&self) -> bool {
        !self.parent.removed
            && self.pickup_delay != 32767
            && self.age != -32768
            && self.age < 6000
            && count(self.item()) < max_stack_size(self.item())
    }
    pub fn are_mergable(a: &ItemStack, b: &ItemStack) -> bool {
        count(a).wrapping_add(count(b)) <= max_stack_size(b)
            && inventory::is_same_item_same_components(a, b)
    }
    pub fn merge_stacks(to: &ItemStack, from: &mut ItemStack, max_count: i32) -> ItemStack {
        let delta = (max_stack_size(to).min(max_count) - count(to)).min(count(from));
        let result = copy_with_count(to, count(to).wrapping_add(delta));
        set_count(from, count(from).wrapping_sub(delta));
        result
    }
    pub fn save_without_id(&self, level: &Level) -> Result<CompoundTag, LevelError> {
        let mut tag = self.parent.save_without_id();
        tag.insert("Health", Tag::Short(self.health as i16));
        tag.insert("Age", Tag::Short(self.age as i16));
        tag.insert("PickupDelay", Tag::Short(self.pickup_delay as i16));
        for (name, uuid) in [
            (
                "Thrower",
                self.thrower
                    .map(|reference| reference.uuid(level))
                    .transpose()?,
            ),
            ("Owner", self.target),
        ] {
            tag.remove(name);
            if let Some(uuid) = uuid {
                tag.insert(name, Tag::IntArray(uuid.to_int_array().to_vec()));
            }
        }
        tag.remove("Item");
        if !self.item().is_empty() {
            tag.insert("Item", self.item().to_nbt().expect("canonical item stack"));
        }
        Ok(tag)
    }
    pub fn load_nbt(&mut self, tag: CompoundTag) -> Result<(), persistence::EntityLoadError> {
        self.health = i32::from(tag.get_short_or("Health", 5));
        self.age = i32::from(tag.get_short_or("Age", 0));
        self.pickup_delay = i32::from(tag.get_short_or("PickupDelay", 0));
        let uuid = |name| {
            tag.get_int_array(name)
                .and_then(|v| <[i32; 4]>::try_from(v).ok())
                .map(Uuid::from_int_array)
        };
        self.thrower = uuid("Thrower").map(EntityReference::unresolved);
        self.target = uuid("Owner");
        self.set_item(
            tag.get("Item")
                .and_then(|v| ItemStack::from_nbt(v).ok())
                .unwrap_or_default(),
        );
        self.parent.load_nbt(tag)?;
        if self.item().is_empty() {
            self.parent.set_removed(RemovalReason::Discarded);
        }
        Ok(())
    }
}
/// Port of ItemEntity#getOwner, including EntityReference's cache mutation.
pub fn get_owner(entity: EntityRef, level: &mut Level) -> Result<Option<EntityRef>, LevelError> {
    let Some(mut thrower) = level.entity::<ItemEntity>(entity)?.thrower else {
        return Ok(None);
    };
    let owner = thrower.resolve(level)?;
    level.entity_mut::<ItemEntity>(entity)?.thrower = Some(thrower);
    Ok(owner)
}
impl EntityDataView for ItemEntity {
    fn view(state: &EntityState) -> Option<&Self> {
        if let EntityState::Item(item) = state {
            Some(item)
        } else {
            None
        }
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        if let EntityState::Item(item) = state {
            Some(item)
        } else {
            None
        }
    }
}
/// Captured neighbor order is EntitySectionStorage's native section/insertion order.
pub fn merge_with_neighbours(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let item = level.entity::<ItemEntity>(entity)?;
    if !item.is_mergable() {
        return Ok(());
    }
    let bounds = item.parent.bounding_box.inflate(0.5, 0.0, 0.5);
    let neighbours = level
        .entities_of_class::<ItemEntity>(bounds, |id, item| id != entity && item.is_mergable());
    for other in neighbours {
        if !level.entity::<ItemEntity>(other)?.is_mergable() {
            continue;
        }
        let a = level.entity::<ItemEntity>(entity)?;
        let b = level.entity::<ItemEntity>(other)?;
        if a.target != b.target || !ItemEntity::are_mergable(a.item(), b.item()) {
            continue;
        }
        let (to, from) = if count(b.item()) < count(a.item()) {
            (entity, other)
        } else {
            (other, entity)
        };
        let to_item = level.entity::<ItemEntity>(to)?;
        let (to_stack, to_delay, to_age) =
            (to_item.item().clone(), to_item.pickup_delay, to_item.age);
        let from_item = level.entity_mut::<ItemEntity>(from)?;
        let (delay, age) = (from_item.pickup_delay, from_item.age);
        let result = ItemEntity::merge_stacks(&to_stack, from_item.item_mut(), 64);
        let empty = from_item.item().is_empty();
        let to_item = level.entity_mut::<ItemEntity>(to)?;
        to_item.set_item(result);
        to_item.pickup_delay = to_delay.max(delay);
        to_item.age = to_age.min(age);
        if empty {
            level.remove_entity_with_reason(from, RemovalReason::Discarded)?;
        }
        if level.entity::<EntityData>(entity)?.removed {
            break;
        }
    }
    Ok(())
}
/// Port of ItemEntity#playerTouch. Mutating the data-only inventory makes no
/// world callback; commit the resulting canonical stack before pickup callbacks.
pub fn player_touch(
    entity: EntityRef,
    player: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let item = level.entity::<ItemEntity>(entity)?;
    if item.pickup_delay != 0
        || item.target.is_some_and(|uuid| {
            level
                .entity::<ServerPlayer>(player)
                .is_ok_and(|p| p.uuid != uuid)
        })
    {
        return Ok(());
    }
    let mut stack = item.item().clone();
    let org_count = count(&stack);
    let kind = stack.item();
    let target = level.entity_mut::<ServerPlayer>(player)?;
    let infinite = target.instabuild();
    let accepted = target
        .inventory
        .add(&mut stack, &mut target.parent.equipment, infinite);
    *level.entity_mut::<ItemEntity>(entity)?.item_mut() = stack;
    if accepted {
        level.take_item(entity, player, org_count)?;
        if level.entity::<ItemEntity>(entity)?.item().is_empty() {
            level.remove_entity_with_reason(entity, RemovalReason::Discarded)?;
            set_count(
                level.entity_mut::<ItemEntity>(entity)?.item_mut(),
                org_count,
            );
        }
        level.emit_entity_effect(crate::entity::effects::EntityEffect::ItemPickup {
            player,
            entity,
            item: kind,
            count: org_count,
        });
    }
    Ok(())
}

pub struct ItemEntityBehavior;
impl Entity for ItemEntityBehavior {
    /// Port of ItemEntity#fireImmune and ItemStack#canBeHurtBy(inFire).
    fn fire_immune(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        use ferraloom_protocol::{
            codec::HolderSet,
            component::{ComponentTypeId, DataComponent},
        };
        let item = level.entity::<ItemEntity>(entity)?;
        let source = level.damage_sources().in_fire();
        let resistant = match item
            .item()
            .component(ComponentTypeId::DAMAGE_RESISTANT)
            .expect("canonical item components")
        {
            Some(DataComponent::DamageResistant(types)) => match &types.0 {
                HolderSet::Direct(ids) => {
                    let source_name = source.type_holder().name().to_string();
                    ids.iter().any(|id| id.as_str() == source_name)
                }
                HolderSet::Named(name) => {
                    let name =
                        ferraloom_data::datapack::Identifier::new(name.namespace(), name.path())
                            .map_err(|_| LevelError::MissingDamageTypeTag)?;
                    let tags = level.damage_types().tags();
                    // Registry-aware decoder 5a will reject an unbound named set before this query.
                    let tag = tags
                        .resolve_name(&name)
                        .ok_or(LevelError::MissingDamageTypeTag)?;
                    tags.contains(tag, source.type_holder().id())
                }
            },
            _ => false,
        };
        Ok(resistant || item.parent.entity_type.data().fire_immune)
    }
    fn name(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_protocol::chat::Component, LevelError> {
        crate::entity::names::item_name(entity, level)
    }
    fn player_touch(
        &self,
        entity: EntityRef,
        player: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        player_touch(entity, player, level)
    }
    fn sound_source(&self) -> ferraloom_protocol::play::SoundSource {
        ferraloom_protocol::play::SoundSource::Ambient
    }

    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<crate::entity::EntityRef>,
    ) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
        if let Some(item) = ItemEntity::view(state) {
            references.extend(item.thrower.and_then(EntityReference::cached_entity));
        }
    }

    fn movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }
    fn default_gravity(
        &self,
        _entity: crate::entity::EntityRef,
        _level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        Ok(0.04)
    }
    fn block_pos_below_that_affects_my_movement(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<BlockPos, LevelError> {
        movement::get_on_pos(entity, 0.999999_f32, level)
    }
    fn should_play_lava_hurt_sound(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<bool, LevelError> {
        let item = level.entity::<ItemEntity>(entity)?;
        Ok(item.health <= 0 || item.parent.tick_count % 10 == 0)
    }
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        if level.entity::<ItemEntity>(entity)?.item().is_empty() {
            return level.remove_entity(entity);
        }
        crate::entity::base_tick(self, entity, level)?;
        let item = level.entity_mut::<ItemEntity>(entity)?;
        if item.pickup_delay > 0 && item.pickup_delay != 32767 {
            item.pickup_delay -= 1;
        }
        item.parent.interpolation_old_position = item.parent.position;
        let from = item.parent.position;
        let old_velocity = item.parent.velocity;
        let base = level.entity::<EntityData>(entity)?;
        let water = base.was_touching_water
            && base.fluid_interaction.height(FluidTags::WATER, level)? > f64::from(0.1_f32);
        let lava = !water
            && level
                .entity::<EntityData>(entity)?
                .fluid_interaction
                .height(FluidTags::LAVA, level)?
                > f64::from(0.1_f32);
        if water || lava {
            let multiplier = if water {
                0.9900000095367432
            } else {
                0.949999988079071
            };
            let base = level.entity_mut::<EntityData>(entity)?;
            let v = base.velocity;
            base.velocity = Vec3::new(
                v.x * multiplier,
                v.y + f64::from(if v.y < 0.05999999865889549 {
                    5e-4_f32
                } else {
                    0.0
                }),
                v.z * multiplier,
            );
        } else {
            let gravity = self.effective_gravity(entity, level)?;
            if gravity != 0.0 {
                level.entity_mut::<EntityData>(entity)?.velocity.y -= gravity;
            }
        }
        let bounds = level
            .entity::<EntityData>(entity)?
            .bounding_box
            .deflate(1e-7, 1e-7, 1e-7);
        let no_physics = !level.no_collision(Some(entity), bounds, false)?;
        level.entity_mut::<EntityData>(entity)?.no_physics = no_physics;
        if no_physics {
            move_towards_closest_space(entity, level)?;
        }
        let base = level.entity::<EntityData>(entity)?;
        if base.on_ground
            && base.velocity.horizontal_distance_sqr() <= f64::from(1e-5_f32)
            && base.tick_count.wrapping_add(base.network_id) % 4 != 0
        {
            let movements = base.final_movements_this_tick.clone();
            inside_block::apply_effects(self, entity, &movements, level)?;
        } else {
            let delta = base.velocity;
            self.move_by(entity, level, MoverType::SelfMovement, delta)?;
            self.apply_effects_from_blocks(entity, level)?;
            let on_ground = level.entity::<EntityData>(entity)?.on_ground;
            let air = self.air_drag(entity, level)?;
            let friction = if on_ground {
                let pos = self.block_pos_below_that_affects_my_movement(entity, level)?;
                air * level.get_block_state(pos)?.block().data().friction
            } else {
                air
            };
            let base = level.entity_mut::<EntityData>(entity)?;
            base.velocity =
                base.velocity
                    .multiply(f64::from(friction), f64::from(air), f64::from(friction));
            if base.on_ground && base.velocity.y < 0.0 {
                base.velocity.y *= -0.5;
            }
        }
        let base = level.entity::<EntityData>(entity)?;
        let moved = mth::floor(from.x) != mth::floor(base.position.x)
            || mth::floor(from.y) != mth::floor(base.position.y)
            || mth::floor(from.z) != mth::floor(base.position.z);
        if base.tick_count % if moved { 2 } else { 40 } == 0
            && level.entity::<ItemEntity>(entity)?.is_mergable()
        {
            merge_with_neighbours(entity, level)?;
        }
        let item = level.entity_mut::<ItemEntity>(entity)?;
        if item.age != -32768 {
            item.age = item.age.wrapping_add(1);
        }
        let any = fluid::update_fluid_interaction(self, entity, level)?;
        let item = level.entity_mut::<ItemEntity>(entity)?;
        item.parent.needs_sync |= any;
        if (item.parent.velocity - old_velocity).length_sqr() > 0.01 {
            item.parent.needs_sync = true;
        }
        if item.age >= 6000 {
            level.remove_entity(entity)?;
        }
        Ok(())
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _source: crate::entity::DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        let item = level.entity_mut::<ItemEntity>(entity)?;
        if item.parent.permanently_invulnerable || item.parent.removed {
            return Ok(false);
        }
        item.parent.mark_hurt();
        item.health = (item.health as f32 - damage) as i32;
        if item.health <= 0 {
            level.remove_entity(entity)?;
        }
        Ok(true)
    }
}
pub(crate) fn move_towards_closest_space(
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let y = (base.bounding_box.min_y + base.bounding_box.max_y) / 2.0;
    let pos = BlockPos::containing(base.position.x, y, base.position.z);
    let delta = Vec3::new(
        base.position.x - f64::from(pos.x),
        y - f64::from(pos.y),
        base.position.z - f64::from(pos.z),
    );
    let mut closest = f64::MAX;
    let mut selected = Direction::Up;
    for direction in [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
        Direction::Up,
    ] {
        let neighbour = pos.relative(direction, 1);
        let state = level.get_block_state(neighbour)?;
        if !LevelReaderRef::Server(level).is_collision_shape_full_block(state, neighbour)? {
            let d = delta.get(direction.axis());
            let d = if direction.axis_direction() == ferraloom_util::AxisDirection::Positive {
                1.0 - d
            } else {
                d
            };
            if d < closest {
                closest = d;
                selected = direction;
            }
        }
    }
    let base = level.entity_mut::<EntityData>(entity)?;
    let speed = base.random.next_float() * 0.2 + 0.1;
    let movement = base.velocity.scale(0.75);
    let step = f64::from(selected.axis_direction().step() as f32 * speed);
    base.velocity = match selected.axis() {
        Axis::X => Vec3::new(step, movement.y, movement.z),
        Axis::Y => Vec3::new(movement.x, step, movement.z),
        Axis::Z => Vec3::new(movement.x, movement.y, step),
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_data::Item;

    #[test]
    fn item_slot_retains_replaced_receivers_without_dirtying_count_edits() {
        let mut entity = ItemEntity::new(1, 123);
        let initial = entity.stack_parts(0).unwrap().0.current(0).unwrap();
        assert_eq!(initial.generation, 1);
        entity.set_item(ItemStack::default());
        assert_eq!(entity.stack_parts(0).unwrap().0.current(0), Some(initial));
        assert!(entity.parent.entity_data.pack_dirty().is_none());
        assert!(entity.stack_parts(1).is_none());
        assert!(entity.stack_parts_mut(1).is_none());

        entity.set_item(ItemStack::new(Item::STONE, 7, Default::default()));
        assert_eq!(
            entity.parent.entity_data.pack_dirty().unwrap()[0].id,
            ITEM.id()
        );
        let (generations, stack, index) = entity.stack_parts_mut(0).unwrap();
        let original = generations.current(index).unwrap();
        assert!(generations.retain(original));
        set_count(generations.get_mut(original, stack).unwrap(), 6);
        assert_eq!(count(entity.item()), 6);
        assert!(entity.parent.entity_data.pack_dirty().is_none());

        entity.set_item(ItemStack::new(Item::STONE, 6, Default::default()));
        assert_eq!(
            entity.parent.entity_data.pack_dirty().unwrap()[0].id,
            ITEM.id()
        );
        let (generations, current, _) = entity.stack_parts_mut(0).unwrap();
        assert_ne!(generations.current(0), Some(original));
        set_count(generations.get_mut(original, current).unwrap(), 2);
        assert_eq!(count(current), 6);
        assert_eq!(count(generations.get(original, current).unwrap()), 2);
        assert!(generations.release(original).is_none());
        assert!(generations.get(original, current).is_none());
        assert!(entity.parent.entity_data.pack_dirty().is_none());
    }
}
