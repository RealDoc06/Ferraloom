use super::*;
use crate::{
    Behaviors,
    entity::{
        Entity, EntityPersistenceError,
        item::{ItemEntity, ItemEntityBehavior},
        player::{GameType, ServerPlayer},
    },
    level::LevelError,
};
use ferraloom_data::{EntityType, Item};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{Uuid, Vec3};
use std::cell::RefCell;

thread_local! {
    static CALLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
fn row() -> Vec<&'static str> {
    include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .find(|row| row.starts_with("entity-live-load-reentrant\t"))
        .unwrap()
        .split('\t')
        .collect()
}
fn record(stage: &str, marker: i32, entity: EntityRef, level: &Level) {
    let item = level.entity::<ItemEntity>(entity).unwrap();
    let pos = item.parent.position;
    CALLS.with_borrow_mut(|calls| {
        calls.push(format!(
            "{stage}:{marker}:{}:{}:{:x},{:x},{:x}:{}",
            item.item().item().name(),
            crate::entity::player::inventory::count(item.item()),
            pos.x.to_bits(),
            pos.y.to_bits(),
            pos.z.to_bits(),
            item.parent.uuid,
        ));
    });
}
struct ReentrantItem;
impl Entity for ReentrantItem {
    fn read_additional_save_data(
        &self,
        entity: EntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), EntityPersistenceError> {
        let marker = tag.get_int_or("marker", 0);
        record("before", marker, entity, level);
        if tag.get_boolean_or("reenter", false) {
            load_in_place(
                entity,
                &ferraloom_nbt::parse_compound(row()[2]).unwrap(),
                level,
            )?;
        }
        ItemEntityBehavior.read_additional_save_data(entity, tag, level)?;
        record("after", marker, entity, level);
        Ok(())
    }
    fn post_data_manipulated(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        record("post", 0, entity, level);
        Ok(())
    }
}

#[test]
fn native_accessor_restores_uuid_after_reentrant_load_before_post_data_manipulated() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::ITEM, &ReentrantItem);
    let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
    let entity = level.next_entity_ref();
    let mut item = ItemEntity::new(1, 123);
    let uuid = Uuid::new(0, 1);
    item.parent.uuid = uuid;
    item.set_item(ItemStack::new(Item::DIAMOND, 3, Default::default()));
    level
        .add_entity(entity, EntityState::Item(Box::new(item)))
        .unwrap();
    level.retain_entity(entity).unwrap();
    CALLS.with_borrow_mut(Vec::clear);
    let fields = row();
    set_entity_data(
        entity,
        &ferraloom_nbt::parse_compound(fields[1]).unwrap(),
        &mut level,
    )
    .unwrap();
    assert_eq!(CALLS.with_borrow_mut(std::mem::take).join(";"), fields[3]);
    assert_eq!(level.entity::<EntityData>(entity).unwrap().uuid, uuid);
    assert_eq!(level.entity_by_uuid(uuid), Some(entity));
    level.release_entity(entity);
}

#[test]
fn accessor_rejects_player_before_loading_or_changing_uuid() {
    let mut level = crate::entity::tests::level(false);
    let entity = level.next_entity_ref();
    let uuid = Uuid::new(1, 2);
    level
        .add_entity(
            entity,
            EntityState::Player(Box::new(ServerPlayer::new(
                uuid,
                "data".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            ))),
        )
        .unwrap();
    let tag = ferraloom_nbt::parse_compound("{UUID:[I;0,0,0,3],Pos:[8.0d,9.0d,10.0d]}").unwrap();
    let error = set_entity_data(entity, &tag, &mut level).unwrap_err();
    assert!(
        matches!(error, DataError::Syntax(error) if error.raw_message.text() == "Unable to modify player data")
    );
    let state = level.entity::<EntityData>(entity).unwrap();
    assert_eq!(state.uuid, uuid);
    assert_eq!(state.position, Vec3::ZERO);
}
