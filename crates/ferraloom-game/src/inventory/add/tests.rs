use super::*;
use crate::{
    entity::{EntityState, player::GameType},
    level::{LevelContext, Region},
};
use ferraloom_data::Item;
use ferraloom_protocol::component::{DataComponent, DataComponentPatch};
use ferraloom_util::{Uuid, Vec3};

fn describe(stack: &ItemStack) -> String {
    if stack.is_empty() {
        return "-".into();
    }
    format!(
        "{},{},{}",
        stack.item().protocol_id(),
        count(stack),
        max_stack_size(stack)
    )
}
fn player(kind: &str) -> ServerPlayer {
    let mut player = ServerPlayer::new(
        Uuid::new(0, 1),
        "insertion".into(),
        1,
        Vec3::ZERO,
        if kind == "infinite" {
            GameType::Creative
        } else {
            GameType::Survival
        },
    );
    for slot in 0..36 {
        player.inventory.set_item(
            slot,
            ItemStack::new(Item::DIRT, 64, Default::default()),
            &mut player.parent.equipment,
        );
    }
    for (slot, count) in [(0, 60), (40, 63)] {
        player.inventory.set_item(
            slot,
            ItemStack::new(Item::STONE, count, Default::default()),
            &mut player.parent.equipment,
        );
    }
    if matches!(kind, "empty" | "damaged") {
        player
            .inventory
            .set_item(1, ItemStack::default(), &mut player.parent.equipment);
    }
    player
}
fn input(kind: &str) -> ItemStack {
    if kind == "damaged" {
        let mut patch = DataComponentPatch::default();
        patch.set(DataComponent::Damage(ferraloom_protocol::codec::VarInt(7)));
        ItemStack::new(Item::IRON_PICKAXE, 1, patch)
    } else {
        ItemStack::new(Item::STONE, 10, Default::default())
    }
}
fn observe<C: InventoryAddContext>(
    context: &mut C,
    input: C::Stack,
    added: bool,
    kind: &str,
) -> Result<String> {
    let mut events = vec![
        format!("added:{added}"),
        format!("left:{}", describe(context.stack(input)?)),
    ];
    for slot in [0, 1, 40] {
        let target = context.inventory_stack(slot)?;
        events.push(format!("{slot}:{}", describe(context.stack(target)?)));
    }
    let total = count(context.stack(input)?) + 2;
    context.set_stack_count(input, total)?;
    events.push(format!("held:{}", describe(context.stack(input)?)));
    let owner = if kind == "detached" {
        describe(&ItemStack::new(Item::DIRT, 7, Default::default()))
    } else {
        describe(context.stack(input)?)
    };
    events.push(format!("owner:{owner}"));
    Ok(events.join(";"))
}

#[test]
fn value_and_retained_inventory_add_match_native_remainders_and_detached_sources() {
    static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
    let rows = include_str!("../../../tests/java/inventory/listeners.tsv");
    let mut checked = 0;
    for row in rows.lines() {
        let Some(row) = row.strip_prefix("listener\tinventory-add-") else {
            continue;
        };
        let (kind, expected) = row.split_once('\t').unwrap();
        let mut state = player(kind);
        let infinite_materials = state.instabuild();
        let mut value = input(kind);
        // Exercise Inventory's unchanged value API, which delegates to the kernel.
        let added =
            state
                .inventory
                .add(&mut value, &mut state.parent.equipment, infinite_materials);
        assert_eq!(
            observe(
                &mut ValueInventoryAdd {
                    inventory: &mut state.inventory,
                    equipment: &mut state.parent.equipment,
                    input: &mut value,
                    infinite_materials,
                },
                ValueStack::Input,
                added,
                kind,
            )
            .unwrap(),
            expected,
            "value {kind}"
        );

        let player_id = EntityRef(1);
        let mut state = player(kind);
        state.menus.current_mut().set_carried(input(kind));
        let menu = state.menus.current_ref();
        let mut level = {
            let mut fixture_level =
                Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        level
            .add_entity(player_id, EntityState::Player(Box::new(state)))
            .unwrap();
        let address = ContainerSlot {
            container: ContainerHandle::MenuCarried {
                player: player_id,
                menu,
            },
            slot: 0,
        };
        let mut context = LiveInventoryAdd {
            operation: StackOperation::new(&mut level),
            player: player_id,
        };
        let input = context.operation.pin(address).unwrap();
        if kind == "detached" {
            context
                .operation
                .level
                .replace_stack_object(
                    address,
                    StackValue::New(ItemStack::new(Item::DIRT, 7, Default::default())),
                )
                .unwrap();
        }
        let added = add_item_stack(player_id, input, context.operation.level).unwrap();
        assert_eq!(observe(&mut context, input, added, kind).unwrap(), expected);
        let owner = context.operation.pin(address).unwrap();
        assert_eq!(
            describe(context.operation.level.stack(owner).unwrap()),
            expected.rsplit_once("owner:").unwrap().1,
            "live owner {kind}"
        );
        checked += 1;
    }
    assert_eq!(checked, 5);
}
