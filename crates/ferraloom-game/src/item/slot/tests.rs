use super::*;
use crate::{
    block::UnportedBehavior,
    entity::{
        EntityData, EntityState,
        animal::Cow,
        experience_orb::ExperienceOrb,
        item::{FallingBlockEntity, PrimedTnt},
        player::{GameType, equipment::slot_name},
        synched_data::SILENT,
    },
    inventory::{AbstractContainerMenu, MenuKind},
    level::{
        effects::LevelEffect,
        game_event::test_support::{self as game_events, Observation},
    },
};
use ferraloom_data::Item;
use ferraloom_protocol::play::clientbound::Packet;
use ferraloom_util::{Uuid, Vec3, random::RandomSource};

const VECTORS: &str = include_str!("../../../tests/java/entity/slots/vectors.tsv");
fn stack(item: Item, count: i32) -> ItemStack {
    ItemStack::new(item, count, Default::default())
}
fn describe(stack: &ItemStack) -> String {
    format!(
        "{}:{}",
        stack.item().name(),
        crate::entity::player::inventory::count(stack)
    )
}
fn put_equipment(state: &mut EntityState, slot: EquipmentSlot, stack: ItemStack) {
    if slot == EquipmentSlot::Mainhand
        && let EntityState::Player(player) = state
    {
        player.inventory.set_selected_item(stack);
    } else {
        state.as_living_mut().unwrap().equipment.set(slot, stack);
    }
}
fn create(kind: &str, level: &mut Level) -> EntityRef {
    let mut state = match kind {
        "player" => EntityState::Player(Box::new(ServerPlayer::new(
            Uuid::new(1, 2),
            "slot_owner".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        ))),
        "cow" => EntityState::Cow(Box::<Cow>::default()),
        "item" => EntityState::Item(Box::new(ItemEntity::new(1, 0))),
        "falling" => EntityState::FallingBlock(Box::new(FallingBlockEntity::new(1, 0))),
        "tnt" => EntityState::PrimedTnt(Box::new(PrimedTnt::new(1, 0))),
        "orb" => EntityState::ExperienceOrb(Box::new(ExperienceOrb::new(1, 0))),
        _ => panic!("unknown kind {kind}"),
    };
    state.as_entity_mut().network_id = 1;
    state.as_entity_mut().uuid = Uuid::new(1, 2);
    if let EntityState::Player(player) = &mut state {
        for i in 0..player.inventory.items.len() {
            player.inventory.set_item(
                i,
                stack(Item::STONE, i as i32 + 1),
                &mut player.parent.equipment,
            );
        }
        player.inventory.set_selected_slot(4);
        for i in 0..27 {
            player
                .get_ender_chest_inventory_mut()
                .contents_mut()
                .set_item(i, stack(Item::STONE, i as i32 + 1));
        }
        for i in 0..4 {
            player.menus.inventory_menu.local[0].set_item(i, stack(Item::STONE, 56 + i as i32));
        }
        player
            .menus
            .inventory_menu
            .set_carried(stack(Item::STONE, 55));
    }
    if state.as_living().is_some() {
        for (i, slot) in SLOTS.into_iter().enumerate() {
            put_equipment(&mut state, slot, stack(Item::STONE, 40 + i as i32));
        }
    }
    if let EntityState::Item(item) = &mut state {
        item.set_item(stack(Item::STONE, 9));
    }
    let entity = level.next_entity_ref();
    level.add_entity(entity, state).unwrap();
    entity
}
fn get(entity: EntityRef, slot: i32, level: &Level) -> Option<SlotAccess> {
    level
        .entity_behavior(entity)
        .unwrap()
        .get_slot(entity, slot, level)
        .unwrap()
}
fn observed(access: Option<SlotAccess>, level: &Level) -> String {
    access.map_or_else(
        || "-".into(),
        |access| describe(level.stack(access.get(level).unwrap()).unwrap()),
    )
}

#[test]
fn native_slot_ranges_use_current_inventory_equipment_ender_cursor_grid_and_item_receivers() {
    let mut checked = 0;
    for kind in ["player", "cow", "item", "falling", "tnt", "orb"] {
        let mut level = crate::entity::tests::level(false);
        level.set_unported_behavior(UnportedBehavior::Strict);
        let entity = create(kind, &mut level);
        for row in VECTORS.lines().filter(|row| row.starts_with("slot-read\t")) {
            let f: Vec<_> = row.split('\t').collect();
            if f[1] != kind {
                continue;
            }
            assert_eq!(
                observed(get(entity, f[2].parse().unwrap(), &level), &level),
                f[3],
                "{row}"
            );
            checked += 1;
        }
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
    assert_eq!(checked, 564);
}

#[test]
fn native_slot_setters_validate_equipment_and_preserve_container_count_rules() {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    crate::level::drop_fixtures::install(&mut level);
    let mut checked = 0;
    for row in VECTORS.lines().filter(|row| row.starts_with("slot-set\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let entity = create(f[1], &mut level);
        let access = get(entity, f[2].parse().unwrap(), &level);
        assert_eq!(observed(access, &level), f[4], "before {row}");
        let input = Item::from_name(f[3]).unwrap();
        let value = if input == Item::AIR {
            ItemStack::default()
        } else {
            stack(input, if input == Item::STONE { 77 } else { 1 })
        };
        let accepted = match access {
            Some(access) => access
                .set(StackValue::New(value), &mut StackOperation::new(&mut level))
                .unwrap_or_else(|e| panic!("{row}: {e:?}")),
            None => false,
        };
        assert_eq!(accepted.to_string(), f[5], "{row}");
        assert_eq!(observed(access, &level), f[6], "after {row}");
        level.remove_entity(entity).unwrap();
        level.finish_entity_management();
        level.take_effects();
        checked += 1;
    }
    assert_eq!(checked, 576);
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}

#[test]
fn native_slot_access_resolves_selection_and_current_menu_after_capture() {
    let mut level = crate::entity::tests::level(false);
    let player = create("player", &mut level);
    let hand = get(player, 98, &level).unwrap();
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .inventory
        .set_selected_slot(6);
    let before = observed(Some(hand), &level);
    hand.set(
        StackValue::New(stack(Item::DIAMOND, 3)),
        &mut StackOperation::new(&mut level),
    )
    .unwrap();
    let owner = level.entity::<ServerPlayer>(player).unwrap();
    let actual = format!(
        "slot-dynamic-hand\t{before}\t{}\t{}",
        describe(&owner.inventory.items[4]),
        describe(&owner.inventory.items[6])
    );
    assert_eq!(
        actual,
        VECTORS
            .lines()
            .find(|r| r.starts_with("slot-dynamic-hand\t"))
            .unwrap()
    );
    let cursor = get(player, 499, &level).unwrap();
    let mut menu = AbstractContainerMenu::new(MenuKind::Chest { rows: 3 }, 7).unwrap();
    menu.set_carried(stack(Item::GOLD_INGOT, 2));
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .menus
        .install_container_menu(menu);
    let before = observed(Some(cursor), &level);
    cursor
        .set(
            StackValue::New(stack(Item::DIAMOND, 3)),
            &mut StackOperation::new(&mut level),
        )
        .unwrap();
    let owner = level.entity::<ServerPlayer>(player).unwrap();
    let actual = format!(
        "slot-dynamic-cursor\t{before}\t{}\t{}",
        describe(&owner.menus.inventory_menu.carried),
        describe(&owner.menus.current().carried)
    );
    assert_eq!(
        actual,
        VECTORS
            .lines()
            .find(|r| r.starts_with("slot-dynamic-cursor\t"))
            .unwrap()
    );
}

#[test]
fn native_inventory_grid_slot_setters_refresh_twice_with_the_live_limited_crafting_rule() {
    use crate::gamerules::{GameRule, GameRuleValue, GameRules};
    let mut checked = 0;
    for limited in [false, true] {
        for known in [false, true] {
            let mut level = crate::entity::tests::level(false);
            level.set_unported_behavior(UnportedBehavior::Strict);
            crate::level::drop_fixtures::install(&mut level);
            let mut rules = GameRules::default();
            rules
                .set(GameRule::LimitedCrafting, GameRuleValue::Boolean(limited))
                .unwrap();
            rules.update_level_rules(level.rules_mut());
            let player = create("player", &mut level);
            {
                let owner = level.entity_mut::<ServerPlayer>(player).unwrap();
                owner.menus.inventory_menu.local[0].clear_content();
                owner.menus.inventory_menu.local[1].clear_content();
                if known {
                    owner
                        .recipe_book
                        .add("minecraft:oak_planks".parse().unwrap());
                }
                owner.menus.install_container_menu(
                    AbstractContainerMenu::new(MenuKind::Chest { rows: 3 }, 7).unwrap(),
                );
            }
            level.take_effects();
            for row in VECTORS
                .lines()
                .filter(|row| row.starts_with("slot-craft\t"))
            {
                let f: Vec<_> = row.split('\t').collect();
                if f[1].parse::<bool>().unwrap() != limited
                    || f[2].parse::<bool>().unwrap() != known
                {
                    continue;
                }
                let (item, count) = f[5].rsplit_once(':').unwrap();
                let item = Item::from_name(item).unwrap();
                let value = if item == Item::AIR {
                    ItemStack::default()
                } else {
                    stack(item, count.parse().unwrap())
                };
                let accepted = get(player, f[4].parse().unwrap(), &level)
                    .unwrap()
                    .set(StackValue::New(value), &mut StackOperation::new(&mut level))
                    .unwrap();
                assert_eq!(accepted.to_string(), f[6], "{row}");
                let owner = level.entity::<ServerPlayer>(player).unwrap();
                assert_eq!(
                    describe(&owner.menus.inventory_menu.local[1].get_item(0)),
                    f[7],
                    "result {row}"
                );
                assert_eq!(
                    owner.menus.current().container_id,
                    7,
                    "inventory result changes while a different menu is current"
                );
                let packets = level
                    .take_effects()
                    .into_iter()
                    .filter_map(|effect| match effect {
                        LevelEffect::PlayerPacket { recipients, packet } => match *packet {
                            Packet::ContainerSetSlot(packet) => {
                                assert_eq!(recipients, vec![player]);
                                Some(format!(
                                    "{}:{}:{}:{}",
                                    packet.container_id.0,
                                    packet.state_id.0,
                                    packet.slot,
                                    describe(&packet.item_stack)
                                ))
                            }
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(packets.join(";"), f[8], "packets {row}");
                checked += 1;
            }
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        }
    }
    assert_eq!(checked, 16);
}

#[test]
fn native_equipping_gates_seeded_sound_before_game_event_and_preserves_rng() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("slot-equip\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let scenario: i32 = f[3].parse().unwrap();
        let slot = *SLOTS.iter().find(|&&slot| slot_name(slot) == f[2]).unwrap();
        let mut level = crate::entity::tests::level(false);
        level.set_unported_behavior(UnportedBehavior::Strict);
        let mut state = if f[1] == "cow" {
            EntityState::Cow(Box::default())
        } else {
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "equip".into(),
                1,
                Vec3::ZERO,
                if f[1] == "spectator" {
                    GameType::Spectator
                } else {
                    GameType::Survival
                },
            )))
        };
        state.as_entity_mut().network_id = 1;
        state.as_entity_mut().uuid = Uuid::new(1, 2);
        state.as_entity_mut().set_pos(Vec3::new(8.5, 64.0, 8.5));
        if scenario == 1 || scenario == 4 {
            put_equipment(&mut state, slot, stack(Item::STONE, 2));
        }
        state.as_entity_mut().first_tick = scenario == 2;
        state.as_entity_mut().entity_data.set(SILENT, scenario == 3);
        state.as_entity_mut().random.set_seed(100);
        let entity = level.next_entity_ref();
        level.add_entity(entity, state).unwrap();
        game_events::observe(&mut level);
        game_events::take(&mut level);
        let item = Item::from_name(f[4]).unwrap();
        let value = if item == Item::AIR {
            ItemStack::default()
        } else {
            stack(item, if scenario == 1 { 7 } else { 1 })
        };
        {
            let mut operation = StackOperation::new(&mut level);
            operation.level.retain_entity(entity).unwrap();
            let result = operation
                .level
                .entity_behavior(entity)
                .unwrap()
                .set_item_slot(entity, slot, StackValue::New(value), &mut operation);
            operation.level.release_entity(entity);
            result.unwrap();
        }
        let value = level
            .entity_behavior(entity)
            .unwrap()
            .item_by_slot(entity, slot, &level)
            .unwrap()
            .unwrap();
        assert_eq!(describe(value), f[5], "{row}");
        let mut trace = Vec::new();
        for event in game_events::take(&mut level) {
            match event {
                Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) => {
                    if let Packet::Sound(sound) = *packet {
                        let ferraloom_protocol::codec::Holder::Reference(id) = sound.sound.0 else {
                            panic!("native registered equip sound")
                        };
                        assert_eq!((sound.volume, sound.pitch), (1.0, 1.0));
                        assert_eq!((sound.x, sound.y, sound.z), (68, 512, 68));
                        trace.push(format!(
                            "sound:{id}:{}:{:x}",
                            sound.source as i32, sound.seed as u64
                        ));
                    }
                }
                Observation::GameEvent {
                    event,
                    position,
                    source,
                    affected_state,
                } => {
                    assert_eq!(position, Vec3::new(8.5, 64.0, 8.5));
                    assert_eq!(source, Some(entity));
                    assert_eq!(affected_state, None);
                    trace.push(format!("event:{}", event.name()));
                }
                _ => {}
            }
        }
        assert_eq!(
            if trace.is_empty() {
                "-".into()
            } else {
                trace.join(";")
            },
            f[6],
            "events {row}"
        );
        assert_eq!(
            format!(
                "{:x}",
                level
                    .entity_mut::<EntityData>(entity)
                    .unwrap()
                    .random
                    .next_long() as u64
            ),
            f[7],
            "rng {row}"
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        checked += 1;
    }
    assert_eq!(checked, 144);
}

#[test]
fn native_equipment_callback_keeps_old_and_incoming_objects_after_nested_replacement_and_removal() {
    use crate::{
        Behaviors,
        entity::{Entity, animal::CowBehavior},
    };
    use ferraloom_data::EntityType;
    use std::cell::RefCell;
    thread_local! { static OBSERVED:RefCell<Option<String>>=const {RefCell::new(None)}; }
    struct Reentrant;
    impl crate::entity::Entity for Reentrant {
        fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
            CowBehavior.trace_entity_refs(state, refs);
        }
        fn on_equip_item(
            &self,
            owner: EntityRef,
            slot: EquipmentSlot,
            old: StackRef,
            new: StackRef,
            level: &mut Level,
        ) -> Result<(), LevelError> {
            if level
                .entity::<EntityData>(owner)?
                .custom_data
                .get_boolean_or("nested", false)
            {
                return Ok(());
            }
            level
                .entity_mut::<EntityData>(owner)?
                .custom_data
                .insert("nested", ferraloom_nbt::Tag::Byte(1));
            {
                let mut nested = StackOperation::new(level);
                self.set_item_slot(
                    owner,
                    slot,
                    StackValue::New(stack(Item::DIAMOND, 1)),
                    &mut nested,
                )?;
            }
            level.remove_entity(owner)?;
            level.finish_entity_management();
            let incoming = level.stack_mut(new).map_err(receiver_error)?;
            let amount = crate::entity::player::inventory::count(incoming) + 3;
            crate::entity::player::inventory::set_count(incoming, amount);
            let observed = format!(
                "{}\t{}\t{}\t{}",
                describe(level.stack(old).map_err(receiver_error)?),
                describe(level.stack(new).map_err(receiver_error)?),
                describe(level.entity::<LivingEntity>(owner)?.equipment.get(slot)),
                level.entity::<EntityData>(owner)?.removed
            );
            OBSERVED.with_borrow_mut(|value| *value = Some(observed));
            Ok(())
        }
    }
    static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::COW, &Reentrant);
    let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
    level.set_unported_behavior(UnportedBehavior::Strict);
    let mut state = EntityState::Cow(Box::default());
    put_equipment(&mut state, EquipmentSlot::Feet, stack(Item::STONE, 4));
    let owner = level.next_entity_ref();
    level.add_entity(owner, state).unwrap();
    let access = get(owner, 100, &level).unwrap();
    let accepted = access
        .set(
            StackValue::New(stack(Item::IRON_BOOTS, 2)),
            &mut StackOperation::new(&mut level),
        )
        .unwrap();
    let observed = OBSERVED.with_borrow_mut(Option::take).unwrap();
    assert_eq!(
        format!("slot-reentrant\t{accepted}\t{observed}"),
        VECTORS
            .lines()
            .find(|row| row.starts_with("slot-reentrant\t"))
            .unwrap()
    );
    level.finish_entity_management();
    assert!(level.entity_state(owner).is_err());
}
