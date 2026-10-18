use super::*;
use crate::{
    Behaviors,
    entity::player::{GameType, ServerPlayer},
    level::{Level, LevelError, Region},
};
use ferraloom_data::{
    Block, Item,
    datapack::{TagSet, Tags},
};
use ferraloom_protocol::{
    Identifier,
    codec::{HolderSet, VarInt},
    component::{BlockSet, DataComponentPatch, Tool, ToolRule},
};
use ferraloom_util::{Uuid, Vec3};
use std::sync::Arc;

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    Level::new(
        Region::new(&EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    )
}
fn direct(blocks: &[Block]) -> BlockSet {
    BlockSet(HolderSet::Direct(
        blocks
            .iter()
            .map(|block| Identifier::parse(block.name()).unwrap())
            .collect(),
    ))
}
fn named(name: &str) -> BlockSet {
    BlockSet(HolderSet::Named(Identifier::parse(name).unwrap()))
}
fn rule(blocks: BlockSet, correct_for_drops: Option<bool>) -> ToolRule {
    ToolRule {
        blocks,
        speed: Some(2.0.into()),
        correct_for_drops,
    }
}
fn stack(item: Item) -> ItemStack {
    ItemStack::new(item, 1, DataComponentPatch::default())
}
fn custom(item: Item, rules: Vec<ToolRule>) -> ItemStack {
    ItemStack::new(
        item,
        1,
        DataComponentPatch {
            added: vec![DataComponent::Tool(Tool {
                rules,
                default_mining_speed: 1.0.into(),
                damage_per_block: VarInt(1),
                can_destroy_blocks_in_creative: true,
            })],
            removed: Vec::new(),
        },
    )
}
fn cases() -> Vec<(&'static str, ItemStack)> {
    let removed = ItemStack::new(
        Item::WOODEN_PICKAXE,
        1,
        DataComponentPatch {
            removed: vec![ComponentType::TOOL],
            added: Vec::new(),
        },
    );
    let mut zero = stack(Item::IRON_PICKAXE);
    set_count(&mut zero, 0);
    vec![
        ("empty", ItemStack::default()),
        ("stick", stack(Item::STICK)),
        ("wooden", stack(Item::WOODEN_PICKAXE)),
        ("iron", stack(Item::IRON_PICKAXE)),
        ("shears", stack(Item::SHEARS)),
        ("removed", removed),
        ("zero", zero),
        (
            "speed_and_deny",
            custom(
                Item::STICK,
                vec![
                    rule(direct(&[Block::STONE]), None),
                    rule(direct(&[Block::DIAMOND_ORE]), Some(false)),
                    rule(named("minecraft:mineable/pickaxe"), Some(true)),
                ],
            ),
        ),
        (
            "deny_first",
            custom(
                Item::STICK,
                vec![
                    rule(direct(&[Block::STONE]), Some(false)),
                    rule(direct(&[Block::STONE]), Some(true)),
                ],
            ),
        ),
        (
            "allow_first",
            custom(
                Item::STICK,
                vec![
                    rule(direct(&[Block::STONE]), Some(true)),
                    rule(direct(&[Block::STONE]), Some(false)),
                ],
            ),
        ),
        (
            "direct_empty",
            custom(Item::STICK, vec![rule(direct(&[]), Some(true))]),
        ),
        (
            "miss_then_allow",
            custom(
                Item::STICK,
                vec![
                    rule(direct(&[Block::DIRT]), Some(false)),
                    rule(direct(&[Block::STONE]), Some(true)),
                ],
            ),
        ),
        (
            "speed_only_tag",
            custom(
                Item::STICK,
                vec![
                    rule(named("minecraft:mineable/pickaxe"), None),
                    rule(direct(&[Block::STONE]), Some(true)),
                ],
            ),
        ),
        (
            "air_payload",
            custom(Item::AIR, vec![rule(direct(&[Block::STONE]), Some(true))]),
        ),
        (
            "named_empty",
            custom(
                Item::STICK,
                vec![rule(
                    named("minecraft:incorrect_for_netherite_tool"),
                    Some(true),
                )],
            ),
        ),
        ("netherite", stack(Item::NETHERITE_PICKAXE)),
    ]
}
fn player() -> ServerPlayer {
    let mut player = ServerPlayer::new(
        Uuid::new(1, 2),
        "tools".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    );
    player.inventory.set_selected_slot(5);
    player
}
fn install_tags(
    level: &mut Level,
    values: std::collections::BTreeMap<ferraloom_data::datapack::Identifier, Vec<Block>>,
) {
    level.set_block_tags(Arc::new(
        TagSet::from_tags(
            Tags {
                values,
                diagnostics: Vec::new(),
            },
            ferraloom_data::BLOCKS.len(),
        )
        .unwrap(),
    ));
}
#[test]
fn native_default_tools_component_patches_and_first_matching_rules() {
    let mut level = level();
    let tags = VECTORS
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if fields.next()? != "tool-tag" {
                return None;
            }
            Some((
                fields.next()?.parse().unwrap(),
                fields
                    .next()?
                    .split(',')
                    .filter(|name| *name != "-")
                    .map(|name| Block::from_name(name).unwrap())
                    .collect(),
            ))
        })
        .collect();
    install_tags(&mut level, tags);
    let mut player = player();
    let mut actual = Vec::new();
    for (name, stack) in cases() {
        player.inventory.set_selected_item(stack.clone());
        for block in [
            Block::STONE,
            Block::DIRT,
            Block::DIAMOND_ORE,
            Block::OAK_LOG,
            Block::COBWEB,
            Block::OAK_LEAVES,
            Block::WHITE_WOOL,
            Block::IRON_BLOCK,
        ] {
            let state = block.default_state();
            actual.push(format!(
                "tool-drops\t{name}\t{}\t{},{},{}",
                block.name(),
                is_correct_tool_for_drops(&stack, state, &level).unwrap(),
                player.has_correct_tool_for_drops(state, &level).unwrap(),
                state.data().requires_correct_tool
            ));
        }
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("tool-drops\t"))
        .collect();
    assert_eq!(actual, expected);
}
#[test]
fn named_tool_rules_use_current_tags_and_preserve_missing_and_empty_sets() {
    let mut level = level();
    let state = Block::STONE.default_state();
    let tool = custom(
        Item::STICK,
        vec![rule(named("entity_probe:live"), Some(true))],
    );
    assert_eq!(
        is_correct_tool_for_drops(&tool, state, &level),
        Err(LevelError::MissingBlockTags)
    );
    for (members, expected) in [
        (vec![Block::STONE], true),
        (vec![], false),
        (vec![Block::STONE], true),
    ] {
        install_tags(
            &mut level,
            [("entity_probe:live".parse().unwrap(), members)].into(),
        );
        assert_eq!(
            is_correct_tool_for_drops(&tool, state, &level).unwrap(),
            expected
        );
    }
    install_tags(&mut level, Default::default());
    assert_eq!(
        is_correct_tool_for_drops(&tool, state, &level),
        Err(LevelError::MissingBlockTag)
    );
    // Native Tool.CODEC rejects this state before a tool can enter gameplay.
    assert!(VECTORS.lines().any(|line| line == "tool-missing-tag\ttrue"));
}

#[test]
fn mining_speed_distinguishes_missing_empty_and_unobserved_named_rules() {
    use crate::block::UnportedBehavior;
    let state = Block::STONE.default_state();
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = level();
        level.set_unported_behavior(policy);
        let tool = custom(
            Item::STICK,
            vec![rule(named("mining_probe:live"), Some(true))],
        );
        assert_eq!(
            get_destroy_speed(&tool, state, &level),
            Err(LevelError::MissingBlockTags)
        );
        install_tags(
            &mut level,
            [("mining_probe:live".parse().unwrap(), vec![])].into(),
        );
        assert_eq!(get_destroy_speed(&tool, state, &level).unwrap(), 1.0);
        install_tags(&mut level, Default::default());
        assert_eq!(
            get_destroy_speed(&tool, state, &level),
            Err(LevelError::MissingBlockTag)
        );
        let ignored_missing = ToolRule {
            blocks: named("mining_probe:missing"),
            speed: None,
            correct_for_drops: Some(false),
        };
        let tool = custom(
            Item::STICK,
            vec![ignored_missing, rule(direct(&[Block::STONE]), Some(true))],
        );
        assert_eq!(get_destroy_speed(&tool, state, &level).unwrap(), 2.0);
        let tool = custom(
            Item::STICK,
            vec![
                rule(direct(&[Block::STONE]), Some(false)),
                rule(named("mining_probe:missing"), Some(true)),
            ],
        );
        assert_eq!(get_destroy_speed(&tool, state, &level).unwrap(), 2.0);
        assert!(!is_correct_tool_for_drops(&tool, state, &level).unwrap());
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
    assert!(
        include_str!("../../../../tests/java/level/mining.tsv")
            .lines()
            .any(|line| line == "missing-tag\ttrue")
    );
}
#[test]
fn tool_and_player_short_circuits_keep_later_required_tag_reads_unobserved() {
    let level = level();
    let state = Block::STONE.default_state();
    for correct in [false, true] {
        let tool = custom(
            Item::STICK,
            vec![
                rule(direct(&[Block::STONE]), Some(correct)),
                rule(named("minecraft:mineable/pickaxe"), Some(true)),
            ],
        );
        assert_eq!(
            is_correct_tool_for_drops(&tool, state, &level).unwrap(),
            correct
        );
    }
    let speed_only = custom(
        Item::STICK,
        vec![
            rule(named("minecraft:mineable/pickaxe"), None),
            rule(direct(&[Block::STONE]), Some(true)),
        ],
    );
    assert!(is_correct_tool_for_drops(&speed_only, state, &level).unwrap());
    let mut player = player();
    player.inventory.set_selected_item(custom(
        Item::STICK,
        vec![rule(named("minecraft:mineable/pickaxe"), Some(true))],
    ));
    assert!(
        player
            .has_correct_tool_for_drops(Block::DIRT.default_state(), &level)
            .unwrap()
    );
    assert_eq!(
        player.has_correct_tool_for_drops(state, &level),
        Err(LevelError::MissingBlockTags)
    );
}
