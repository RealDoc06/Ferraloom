use super::*;
use crate::{
    entity::{EntityRef, EntityState, item::ItemEntity, player::ServerPlayer},
    level::{Level, effects::LevelEffect},
};
use ferraloom_data::Item;
use ferraloom_protocol::play::clientbound as cb;
use ferraloom_util::{BlockPos, Uuid, Vec3};

fn kind(value: &str) -> MenuKind {
    match value {
        "crafting" => MenuKind::Crafting,
        "anvil" => MenuKind::Anvil,
        "loom" => MenuKind::Loom,
        "stonecutter" => MenuKind::Stonecutter,
        "grindstone" => MenuKind::Grindstone,
        "smithing" => MenuKind::Smithing,
        "cartography" => MenuKind::Cartography,
        _ => panic!("unexpected native workstation {value}"),
    }
}
fn stack(item: Item, amount: i32) -> ItemStack {
    ItemStack::new(item, amount, Default::default())
}
fn item(stack: &ItemStack) -> String {
    if stack.is_empty() {
        "-".into()
    } else {
        format!(
            "{},{},{}",
            stack.item().protocol_id(),
            count(stack),
            max_stack_size(stack)
        )
    }
}
fn fixture(kind: MenuKind, bound: bool, mode: &str) -> (Level, EntityRef, MenuRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
    level.install_shared(Box::default());
    let player = level.next_entity_ref();
    let mut owner = ServerPlayer::new(
        Uuid::new(1, 2),
        "workstation".into(),
        level.next_entity_id().unwrap(),
        Vec3::ZERO,
        crate::entity::player::GameType::Survival,
    );
    let mut menu = if bound {
        AbstractContainerMenu::new_bound_workstation(kind, 7, BlockPos::ZERO, &level).unwrap()
    } else {
        AbstractContainerMenu::new(kind, 7).unwrap()
    };
    for slot in 0..menu.local[0].size() {
        menu.local[0].set_item(
            slot,
            stack(
                if slot % 2 == 0 {
                    Item::STONE
                } else {
                    Item::OAK_LOG
                },
                slot as i32 + 3,
            ),
        );
    }
    menu.local[1].set_item(0, stack(Item::DIAMOND, 2));
    menu.carried = stack(Item::STONE, 5);
    if !matches!(mode, "space" | "dimension") {
        for slot in 0..36 {
            owner.inventory.items[slot] = stack(Item::DIRT, 64);
        }
        if mode == "partial" {
            owner.inventory.items[0] = stack(Item::STONE, 62);
        }
    }
    let receiver = owner.menus.install_container_menu(menu);
    level
        .add_entity(player, EntityState::Player(Box::new(owner)))
        .unwrap();
    let state = level.entity_mut::<EntityData>(player).unwrap();
    if matches!(mode, "removed" | "dimension") {
        state.removed = true;
        state.removal_reason = Some(if mode == "dimension" {
            RemovalReason::ChangedDimension
        } else {
            RemovalReason::Discarded
        });
    }
    level.take_effects();
    (level, player, receiver)
}
fn visible_slots(level: &Level, player: EntityRef, menu: MenuRef) -> String {
    let owner = level.entity::<ServerPlayer>(player).unwrap();
    let menu = owner.menus.menu(menu).unwrap();
    let items: Vec<_> = menu
        .slots
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            let value = match slot.container {
                ContainerId::Local(container) => {
                    menu.local[container].get_item(slot.container_slot)
                }
                ContainerId::Player => owner
                    .inventory
                    .get_item(slot.container_slot, &owner.parent.equipment)
                    .clone(),
                ContainerId::External(_) => unreachable!(),
            };
            (!value.is_empty()).then(|| format!("{index}:{}", item(&value)))
        })
        .collect();
    if items.is_empty() {
        "-".into()
    } else {
        items.join(";")
    }
}
fn visible_inventory(level: &Level, player: EntityRef) -> String {
    let owner = level.entity::<ServerPlayer>(player).unwrap();
    (0..36)
        .filter_map(|slot| {
            let stack = owner.inventory.get_item(slot, &owner.parent.equipment);
            (!stack.is_empty()).then(|| format!("{slot}:{}", item(stack)))
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn workstation_removal_matches_native_return_drop_and_null_access_order() {
    let mut cases = 0;
    for row in include_str!("../../../tests/java/blocks/entities/workstation-removed.tsv").lines() {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, player, menu) = fixture(kind(f[1]), f[2] == "true", f[3]);
        let mut transfer = super::super::transfer::LiveMenuTransfer {
            operation: StackOperation::new(&mut level),
            player,
            menu,
        };
        let mut original = Vec::new();
        original.push(transfer.carried_stack().unwrap());
        for slot in 0..transfer.menu().unwrap().local[0].size() {
            original.push(
                transfer
                    .operation
                    .pin(ContainerSlot {
                        container: ContainerHandle::MenuLocal {
                            player,
                            menu,
                            container: 0,
                        },
                        slot,
                    })
                    .unwrap(),
            );
        }
        let mut events = Vec::new();
        let mut drops = Vec::new();
        removed(&mut transfer, f[3] == "disconnected", |context, effect| {
            match effect {
                RemovalEffect::Slot(slot) => {
                    let owner = context.operation.level.entity::<ServerPlayer>(player)?;
                    events.push(format!(
                        "slot:{slot}:{}",
                        item(owner.inventory.get_item(slot, &owner.parent.equipment))
                    ));
                }
                RemovalEffect::Drop(destination) => {
                    let current = context
                        .operation
                        .level
                        .stack(context.operation.level.stack_ref(destination)?)?;
                    assert!(
                        original.iter().any(|&held| std::ptr::eq(
                            context.operation.level.stack(held).unwrap(),
                            current
                        )),
                        "native original receiver: {row}"
                    );
                    drops.push(item(current));
                    events.push(format!("drop:{}", item(current)));
                }
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            events.join(";"),
            f[4].strip_prefix("close:7;").unwrap_or(""),
            "{row}"
        );
        assert_eq!(
            visible_slots(transfer.operation.level, player, menu),
            f[5],
            "{row}"
        );
        assert_eq!(item(&transfer.menu().unwrap().carried), f[6], "{row}");
        assert_eq!(
            visible_inventory(transfer.operation.level, player),
            f[7],
            "{row}"
        );
        assert_eq!(drops.join(";"), f[8], "{row}");
        assert_eq!(f[9], "true", "native object identity: {row}");
        cases += 1;
    }
    assert_eq!(cases, 84);
}

#[test]
fn workstation_close_emits_close_first_and_preserves_held_dropped_receivers() {
    let (mut level, player, menu) = fixture(MenuKind::Crafting, true, "full");
    let mut operation = StackOperation::new(&mut level);
    let carried = operation
        .pin(ContainerSlot {
            container: ContainerHandle::MenuCarried { player, menu },
            slot: 0,
        })
        .unwrap();
    close_container_live(player, operation.level, true, false).unwrap();
    assert_ne!(
        operation
            .level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .menus
            .current_ref(),
        menu
    );
    assert!(
        operation
            .level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .menus
            .menu(menu)
            .unwrap()
            .carried
            .is_empty()
    );
    let dropped = operation
        .level
        .all_entities()
        .into_iter()
        .find(|&id| {
            operation
                .level
                .entity::<ItemEntity>(id)
                .is_ok_and(|entity| {
                    std::ptr::eq(entity.item(), operation.level.stack(carried).unwrap())
                })
        })
        .expect("held carried receiver moved into its canonical ItemEntity");
    assert_eq!(
        count(
            operation
                .level
                .entity::<ItemEntity>(dropped)
                .unwrap()
                .item()
        ),
        5
    );
    assert!(
        matches!(&operation.level.take_effects()[0], LevelEffect::PlayerPacket { packet, .. } if matches!(packet.as_ref(), cb::Packet::ContainerClose(_)))
    );
    drop(operation);
    assert!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .menus
            .menu(menu)
            .is_err()
    );
}

#[test]
fn standalone_workstation_removal_uses_the_same_native_return_kernel() {
    use crate::entity::player::inventory::InventoryEffect;
    let mut cases = 0;
    for row in include_str!("../../../tests/java/blocks/entities/workstation-removed.tsv").lines() {
        let f: Vec<_> = row.split('\t').collect();
        if f[2] != "true" {
            continue;
        }
        let (mut level, player, menu) = fixture(kind(f[1]), true, f[3]);
        let owner = level.entity_mut::<ServerPlayer>(player).unwrap();
        let mut context =
            MenuContext::player(&mut owner.inventory, &mut owner.parent.equipment, false);
        let effects = owner
            .menus
            .menu_mut(menu)
            .unwrap()
            .removed(&mut context, matches!(f[3], "disconnected" | "removed"));
        let events = effects
            .iter()
            .map(|effect| match effect {
                InventoryEffect::SetSlot { slot, stack } => format!("slot:{slot}:{}", item(stack)),
                InventoryEffect::Drop(stack) => format!("drop:{}", item(stack)),
            })
            .collect::<Vec<_>>();
        let drops = effects
            .iter()
            .filter_map(|effect| match effect {
                InventoryEffect::Drop(stack) => Some(item(stack)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            events.join(";"),
            f[4].strip_prefix("close:7;").unwrap(),
            "{row}"
        );
        assert_eq!(drops.join(";"), f[8], "{row}");
        assert_eq!(visible_slots(&level, player, menu), f[5], "{row}");
        assert_eq!(visible_inventory(&level, player), f[7], "{row}");
        cases += 1;
    }
    assert_eq!(cases, 42);
}
