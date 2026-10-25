//! Actual native advancement rewards make producer ordering visible as book
//! packets; the furnace predicate also distinguishes RNG before/after XP.
use super::*;
use crate::{
    advancements::{Advancement, AdvancementHolder, PlayerAdvancements, ServerAdvancementManager},
    block::{
        UnportedBehavior,
        containers::{furnace::AbstractFurnaceBlockEntity, runtime::ContainerResources},
        entity::create,
    },
    entity::{
        EntityRef, EntityState,
        experience_orb::ExperienceOrb,
        player::{GameType, ServerPlayer},
    },
    level::{Level, effects::LevelEffect, entity::EntityManagementEvent},
};
use ferraloom_data::{Block, Item};
use ferraloom_protocol::play::{ContainerInput, clientbound::Packet};
use ferraloom_util::{BlockPos, Uuid, Vec3, random::RandomSource};
use serde_json::json;
use std::sync::Arc;

const KEYS: &[&str] = &[
    "oak_planks",
    "stick",
    "crafting_table",
    "furnace",
    "diamond_block",
    "emerald_block",
    "torch",
    "iron_ingot_from_smelting_raw_iron",
];
fn describe(stack: &ItemStack) -> String {
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
fn definition(
    name: &str,
    trigger: &str,
    conditions: serde_json::Value,
    reward: &str,
) -> AdvancementHolder {
    AdvancementHolder {
        id: ferraloom_protocol::Identifier::parse(&format!("test:{name}")).unwrap(),
        value: Advancement::decode(&json!({
            "criteria":{"a":{"trigger":trigger,"conditions":conditions}},
            "rewards":{"recipes":[reward]}
        }))
        .unwrap(),
    }
}
fn fixture(name: &str) -> (Level, EntityRef, MenuRef, LiveRecipeMenuHooks) {
    let furnace = name.starts_with("furnace");
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    level.install_shared(Box::default());
    crate::level::drop_fixtures::install(&mut level);
    let previous = level.container_resources().unwrap();
    let definitions = if name == "inventory" {
        vec![
            definition(
                "stone",
                "inventory_changed",
                json!({"items":[{"items":"stone","count":4}]}),
                "oak_planks",
            ),
            definition(
                "dirt",
                "inventory_changed",
                json!({"items":[{"items":"dirt"}]}),
                "crafting_table",
            ),
            definition(
                "iron",
                "inventory_changed",
                json!({"items":[{"items":"iron_ingot"}]}),
                "furnace",
            ),
        ]
    } else {
        let used = if furnace {
            "iron_ingot_from_smelting_raw_iron"
        } else {
            "oak_planks"
        };
        let mut unlocked = json!({"recipes":used});
        if furnace {
            unlocked["player"] = json!({"type":"random_chance","chance":0.5});
        }
        vec![
            definition(
                "crafted",
                "recipe_crafted",
                json!({"recipes":used}),
                "stick",
            ),
            definition("unlocked", "recipe_unlocked", unlocked, "crafting_table"),
        ]
    };
    let manager = Arc::new(ServerAdvancementManager::new(definitions));
    let resources = Arc::new(ContainerResources {
        recipes: Arc::clone(&previous.recipes),
        loot: Arc::clone(&previous.loot),
        crafting: Arc::clone(&previous.crafting),
        advancements: Arc::clone(&manager),
        recipe_tags: previous.recipe_tags.clone(),
        recipe_synchronization: Arc::clone(&previous.recipe_synchronization),
    });
    level.set_container_resources(Arc::clone(&resources), level.seed());
    let player = level.next_entity_ref();
    let mut state = ServerPlayer::new(
        Uuid::new(0, 1),
        "producers".into(),
        level.next_entity_id(),
        Vec3::new(12.5, 64.0, 8.5),
        GameType::Survival,
    );
    state.advancements = PlayerAdvancements::new(&manager);
    let kind = if furnace {
        MenuKind::Furnace
    } else {
        MenuKind::Inventory
    };
    if furnace {
        let pos = BlockPos::new(8, 64, 8);
        level
            .set_block_state(pos, Block::FURNACE.default_state())
            .unwrap();
        let mut entity = create(pos, Block::FURNACE.default_state()).unwrap();
        entity.container_mut().unwrap().items[2] =
            ItemStack::new(Item::IRON_INGOT, 3, Default::default());
        level.set_block_entity(entity).unwrap();
        let handle = level.block_entity_ref(pos).unwrap().unwrap();
        level
            .block_entity_mut::<AbstractFurnaceBlockEntity>(handle)
            .unwrap()
            .recipes_used
            .insert(
                ferraloom_protocol::Identifier::parse(
                    "minecraft:iron_ingot_from_smelting_raw_iron",
                )
                .unwrap(),
                7,
            );
        let mut menu = AbstractContainerMenu::new(kind, 7).unwrap();
        for slot in &mut menu.slots {
            if slot.container == ContainerId::Local(0) {
                slot.container = ContainerId::External(0);
            }
        }
        menu.block_entities.push(handle);
        state.menus.install_container_menu(menu);
    }
    let menu = state.menus.current_ref();
    if name == "inventory" {
        let receiver = state.menus.menu_mut(menu).unwrap();
        receiver.slots[37].kind = SlotKind::Result(ResultKind::Crafting);
        receiver.slots[38].kind = SlotKind::Result(ResultKind::Furnace);
    } else if !furnace {
        state.menus.menu_mut(menu).unwrap().local[0]
            .set_item(0, ItemStack::new(Item::OAK_LOG, 1, Default::default()));
    }
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    if !furnace {
        refresh_live_crafting_result(player, menu, false, &mut level).unwrap();
    }
    synchronize_menu(player, menu, MenuSynchronization::Initialize, &mut level).unwrap();
    level.take_effects();
    level.take_entity_management_events();
    level.random().set_seed(263777);
    (
        level,
        player,
        menu,
        LiveRecipeMenuHooks {
            resources,
            kind,
            limited_crafting: false,
        },
    )
}

/// A listener preceding ServerPlayer mutates the memoized copy. Native matching
/// and subsequent synchronization must see that copy, while the slot stays at 1.
struct EarlierListener<'a>(LiveMenuContext<'a>);
impl MenuSynchronizationContext for EarlierListener<'_> {
    type Receiver = StackRef;
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        self.0.menu()
    }
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu> {
        self.0.menu_mut()
    }
    fn capture_slot(&mut self, index: usize) -> Result<StackRef> {
        self.0.capture_slot(index)
    }
    fn capture_carried(&mut self) -> Result<StackRef> {
        self.0.capture_carried()
    }
    fn current_stack<'a>(&'a self, receiver: &'a StackRef) -> Result<&'a ItemStack> {
        self.0.current_stack(receiver)
    }
    fn release_current(&mut self, receiver: StackRef) {
        self.0.release_current(receiver);
    }
    fn listener_copies(&self) -> Result<&ListenerCopyArena> {
        self.0.listener_copies()
    }
    fn listener_copies_mut(&mut self) -> Result<&mut ListenerCopyArena> {
        self.0.listener_copies_mut()
    }
    fn data_at(&mut self, index: usize) -> Result<i32> {
        self.0.data_at(index)
    }
    fn slot_changed(&mut self, index: usize, copy: ListenerCopyRef) -> Result<()> {
        if index == 36 {
            let reference = StackRef {
                container: ContainerHandle::MenuListenerCopies {
                    player: self.0.player,
                },
                slot: copy.slot,
                generation: copy.generation,
            };
            let stack = self.0.level.stack_mut(reference)?;
            let total = count(stack) + 3;
            set_count(stack, total);
        }
        self.0.slot_changed(index, copy)
    }
    fn send_update(&mut self, update: ContainerUpdate) -> Result<()> {
        self.0.send_update(update)
    }
}
fn packets(level: &mut Level, resources: &ContainerResources) -> String {
    level
        .take_effects()
        .into_iter()
        .filter_map(|effect| {
            let LevelEffect::PlayerPacket { packet, .. } = effect else {
                return None;
            };
            match *packet {
                Packet::ContainerSetSlot(packet) => Some(format!(
                    "S:{}:{}",
                    packet.slot,
                    describe(&packet.item_stack)
                )),
                Packet::SetCursorItem(packet) => Some(format!("C:{}", describe(&packet.contents))),
                Packet::RecipeBookAdd(packet) => Some(format!(
                    "B:{}",
                    packet
                        .entries
                        .iter()
                        .map(|entry| {
                            resources
                                .recipe_synchronization
                                .get_recipe_from_display(entry.contents.id)
                                .unwrap()
                                .parent
                                .path()
                                .to_owned()
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                )),
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn slots(player: EntityRef, menu: MenuRef, level: &mut Level) -> String {
    with_menu_callback(player, menu, level, |context| {
        let mut result = Vec::new();
        for index in 0..context.menu()?.slots.len() {
            let stack = context.level.stack_ref(context.slot_address(index)?)?;
            let value = context.level.stack(stack)?;
            if !value.is_empty() {
                result.push(format!("{index}:{}", describe(value)));
            }
        }
        Ok(if result.is_empty() {
            "-".into()
        } else {
            result.join(";")
        })
    })
    .unwrap()
}

#[test]
fn native_producer_packets_retained_listener_copy_furnace_xp_and_recipe_order() {
    for row in include_str!("../../tests/java/inventory/producers.tsv").lines() {
        let fields = row.split('\t').collect::<Vec<_>>();
        let name = fields[1];
        let (mut level, player, menu, mut hooks) = fixture(name);
        if name == "inventory" {
            for (slot, item) in [Item::STONE, Item::DIRT, Item::IRON_INGOT]
                .into_iter()
                .enumerate()
            {
                level
                    .replace_stack_object(
                        ContainerSlot {
                            container: ContainerHandle::PlayerInventory(player),
                            slot,
                        },
                        StackValue::New(ItemStack::new(item, 1, Default::default())),
                    )
                    .unwrap();
            }
            with_menu_callback(player, menu, &mut level, |context| {
                broadcast_menu_changes(&mut EarlierListener(LiveMenuContext {
                    level: context.level,
                    player,
                    menu,
                }))
            })
            .unwrap();
        } else {
            clicked_live_menu(
                player,
                menu,
                if name.starts_with("furnace") { 2 } else { 0 },
                0,
                if name.ends_with("quick") {
                    ContainerInput::QuickMove
                } else {
                    ContainerInput::Pickup
                },
                &mut hooks,
                &mut level,
            )
            .unwrap();
            broadcast_container_menu(player, &mut level).unwrap();
        }
        assert_eq!(
            packets(&mut level, &hooks.resources),
            fields[2],
            "{name} callback/packet order"
        );
        assert_eq!(slots(player, menu, &mut level), fields[3], "{name} slots");
        let state = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(
            describe(&state.menus.menu(menu).unwrap().carried),
            fields[4],
            "{name} carried"
        );
        let known = KEYS
            .iter()
            .filter(|id| {
                state
                    .recipe_book
                    .contains(&format!("minecraft:{id}").parse().unwrap())
            })
            .copied()
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(known, fields[5], "{name} reward/unlock order");
        let stat = crate::stats::Stat::Crafted(if name.starts_with("furnace") {
            Item::IRON_INGOT
        } else {
            Item::OAK_PLANKS
        });
        assert_eq!(
            state.stats.get_value(stat),
            fields[6].parse::<i32>().unwrap(),
            "{name} crafted stat"
        );
        let orbs = level
            .take_entity_management_events()
            .into_iter()
            .filter_map(|event| {
                let EntityManagementEvent::Created(entity) = event else {
                    return None;
                };
                level
                    .entity::<ExperienceOrb>(entity)
                    .ok()
                    .map(|orb| format!("{}:{}", orb.value(), orb.count))
            })
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(orbs, fields[7], "{name} native experience orbs");
        assert_eq!(
            level.random().next_int(),
            fields[8].parse::<i32>().unwrap(),
            "{name} next RNG"
        );
    }
}
