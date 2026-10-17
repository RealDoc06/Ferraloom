//! Port of BlockState.CODEC and StateDefinition's lenient property decoding.
use super::{Error, Identifier};
use crate::{Block, BlockState, properties};
use serde_json::Value;

/// String block IDs select the default state. Object properties are lenient:
/// unknown properties and invalid property values retain their defaults.
pub fn decode_block_state(value: &Value) -> Result<BlockState, Error> {
    let name = if let Some(name) = value.as_str() {
        name
    } else {
        value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::InvalidData("expected block state id".into()))?
    };
    let id: Identifier = name.parse()?;
    let mut state = Block::from_name(&id.to_string())
        .ok_or_else(|| Error::InvalidData(format!("unknown block {id}")))?
        .default_state();
    if let Some(properties) = value.get("properties").and_then(Value::as_object) {
        for (name, value) in properties {
            let Some(value) = value.as_str() else {
                continue;
            };
            let changed = match name.as_str() {
                "age" => (value.parse::<u8>().ok()).and_then(|v| state.with(properties::AGE, v)),
                "attached" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::ATTACHED, v)),
                "attachment" => (match value {
                    "ceiling" => Some(properties::Attachment::Ceiling),
                    "double_wall" => Some(properties::Attachment::DoubleWall),
                    "floor" => Some(properties::Attachment::Floor),
                    "single_wall" => Some(properties::Attachment::SingleWall),
                    _ => None,
                })
                .and_then(|v| state.with(properties::ATTACHMENT, v)),
                "axis" => (match value {
                    "x" => Some(properties::Axis::X),
                    "y" => Some(properties::Axis::Y),
                    "z" => Some(properties::Axis::Z),
                    _ => None,
                })
                .and_then(|v| state.with(properties::AXIS, v)),
                "berries" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::BERRIES, v)),
                "bites" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::BITES, v))
                }
                "bloom" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::BLOOM, v)),
                "bottom" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::BOTTOM, v)),
                "can_summon" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::CAN_SUMMON, v)),
                "candles" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::CANDLES, v))
                }
                "charges" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::CHARGES, v))
                }
                "conditional" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::CONDITIONAL, v)),
                "copper_golem_pose" => (match value {
                    "running" => Some(properties::CopperGolemPose::Running),
                    "sitting" => Some(properties::CopperGolemPose::Sitting),
                    "standing" => Some(properties::CopperGolemPose::Standing),
                    "star" => Some(properties::CopperGolemPose::Star),
                    _ => None,
                })
                .and_then(|v| state.with(properties::COPPER_GOLEM_POSE, v)),
                "cracked" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::CRACKED, v)),
                "crafting" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::CRAFTING, v)),
                "creaking_heart_state" => (match value {
                    "awake" => Some(properties::CreakingHeartState::Awake),
                    "dormant" => Some(properties::CreakingHeartState::Dormant),
                    "uprooted" => Some(properties::CreakingHeartState::Uprooted),
                    _ => None,
                })
                .and_then(|v| state.with(properties::CREAKING_HEART_STATE, v)),
                "delay" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::DELAY, v))
                }
                "disarmed" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::DISARMED, v)),
                "distance" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::DISTANCE, v))
                }
                "down" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::DOWN, v)),
                "drag" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::DRAG, v)),
                "dusted" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::DUSTED, v))
                }
                "east" => (match value {
                    "false" => Some(properties::East::False),
                    "low" => Some(properties::East::Low),
                    "none" => Some(properties::East::None),
                    "side" => Some(properties::East::Side),
                    "tall" => Some(properties::East::Tall),
                    "true" => Some(properties::East::True),
                    "up" => Some(properties::East::Up),
                    _ => None,
                })
                .and_then(|v| state.with(properties::EAST, v)),
                "eggs" => (value.parse::<u8>().ok()).and_then(|v| state.with(properties::EGGS, v)),
                "enabled" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::ENABLED, v)),
                "extended" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::EXTENDED, v)),
                "eye" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::EYE, v)),
                "face" => (match value {
                    "ceiling" => Some(properties::Face::Ceiling),
                    "floor" => Some(properties::Face::Floor),
                    "wall" => Some(properties::Face::Wall),
                    _ => None,
                })
                .and_then(|v| state.with(properties::FACE, v)),
                "facing" => (match value {
                    "down" => Some(properties::Facing::Down),
                    "east" => Some(properties::Facing::East),
                    "north" => Some(properties::Facing::North),
                    "south" => Some(properties::Facing::South),
                    "up" => Some(properties::Facing::Up),
                    "west" => Some(properties::Facing::West),
                    _ => None,
                })
                .and_then(|v| state.with(properties::FACING, v)),
                "flower_amount" => (value.parse::<u8>().ok())
                    .and_then(|v| state.with(properties::FLOWER_AMOUNT, v)),
                "half" => (match value {
                    "bottom" => Some(properties::Half::Bottom),
                    "lower" => Some(properties::Half::Lower),
                    "top" => Some(properties::Half::Top),
                    "upper" => Some(properties::Half::Upper),
                    _ => None,
                })
                .and_then(|v| state.with(properties::HALF, v)),
                "hanging" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::HANGING, v)),
                "has_book" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::HAS_BOOK, v)),
                "has_bottle_0" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::HAS_BOTTLE_0, v)),
                "has_bottle_1" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::HAS_BOTTLE_1, v)),
                "has_bottle_2" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::HAS_BOTTLE_2, v)),
                "has_record" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::HAS_RECORD, v)),
                "hatch" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::HATCH, v))
                }
                "hinge" => (match value {
                    "left" => Some(properties::Hinge::Left),
                    "right" => Some(properties::Hinge::Right),
                    _ => None,
                })
                .and_then(|v| state.with(properties::HINGE, v)),
                "honey_level" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::HONEY_LEVEL, v))
                }
                "hydration" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::HYDRATION, v))
                }
                "in_wall" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::IN_WALL, v)),
                "instrument" => (match value {
                    "banjo" => Some(properties::Instrument::Banjo),
                    "basedrum" => Some(properties::Instrument::Basedrum),
                    "bass" => Some(properties::Instrument::Bass),
                    "bell" => Some(properties::Instrument::Bell),
                    "bit" => Some(properties::Instrument::Bit),
                    "chime" => Some(properties::Instrument::Chime),
                    "cow_bell" => Some(properties::Instrument::CowBell),
                    "creeper" => Some(properties::Instrument::Creeper),
                    "custom_head" => Some(properties::Instrument::CustomHead),
                    "didgeridoo" => Some(properties::Instrument::Didgeridoo),
                    "dragon" => Some(properties::Instrument::Dragon),
                    "flute" => Some(properties::Instrument::Flute),
                    "guitar" => Some(properties::Instrument::Guitar),
                    "harp" => Some(properties::Instrument::Harp),
                    "hat" => Some(properties::Instrument::Hat),
                    "iron_xylophone" => Some(properties::Instrument::IronXylophone),
                    "piglin" => Some(properties::Instrument::Piglin),
                    "pling" => Some(properties::Instrument::Pling),
                    "skeleton" => Some(properties::Instrument::Skeleton),
                    "snare" => Some(properties::Instrument::Snare),
                    "trumpet" => Some(properties::Instrument::Trumpet),
                    "trumpet_exposed" => Some(properties::Instrument::TrumpetExposed),
                    "trumpet_oxidized" => Some(properties::Instrument::TrumpetOxidized),
                    "trumpet_weathered" => Some(properties::Instrument::TrumpetWeathered),
                    "wither_skeleton" => Some(properties::Instrument::WitherSkeleton),
                    "xylophone" => Some(properties::Instrument::Xylophone),
                    "zombie" => Some(properties::Instrument::Zombie),
                    _ => None,
                })
                .and_then(|v| state.with(properties::INSTRUMENT, v)),
                "inverted" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::INVERTED, v)),
                "layers" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::LAYERS, v))
                }
                "leaves" => (match value {
                    "large" => Some(properties::Leaves::Large),
                    "none" => Some(properties::Leaves::None),
                    "small" => Some(properties::Leaves::Small),
                    _ => None,
                })
                .and_then(|v| state.with(properties::LEAVES, v)),
                "level" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::LEVEL, v))
                }
                "lit" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::LIT, v)),
                "locked" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::LOCKED, v)),
                "mode" => (match value {
                    "accept" => Some(properties::Mode::Accept),
                    "compare" => Some(properties::Mode::Compare),
                    "corner" => Some(properties::Mode::Corner),
                    "data" => Some(properties::Mode::Data),
                    "fail" => Some(properties::Mode::Fail),
                    "load" => Some(properties::Mode::Load),
                    "log" => Some(properties::Mode::Log),
                    "save" => Some(properties::Mode::Save),
                    "start" => Some(properties::Mode::Start),
                    "subtract" => Some(properties::Mode::Subtract),
                    _ => None,
                })
                .and_then(|v| state.with(properties::MODE, v)),
                "moisture" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::MOISTURE, v))
                }
                "natural" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::NATURAL, v)),
                "north" => (match value {
                    "false" => Some(properties::North::False),
                    "low" => Some(properties::North::Low),
                    "none" => Some(properties::North::None),
                    "side" => Some(properties::North::Side),
                    "tall" => Some(properties::North::Tall),
                    "true" => Some(properties::North::True),
                    "up" => Some(properties::North::Up),
                    _ => None,
                })
                .and_then(|v| state.with(properties::NORTH, v)),
                "note" => (value.parse::<u8>().ok()).and_then(|v| state.with(properties::NOTE, v)),
                "occupied" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::OCCUPIED, v)),
                "ominous" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::OMINOUS, v)),
                "open" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::OPEN, v)),
                "orientation" => (match value {
                    "down_east" => Some(properties::Orientation::DownEast),
                    "down_north" => Some(properties::Orientation::DownNorth),
                    "down_south" => Some(properties::Orientation::DownSouth),
                    "down_west" => Some(properties::Orientation::DownWest),
                    "east_up" => Some(properties::Orientation::EastUp),
                    "north_up" => Some(properties::Orientation::NorthUp),
                    "south_up" => Some(properties::Orientation::SouthUp),
                    "up_east" => Some(properties::Orientation::UpEast),
                    "up_north" => Some(properties::Orientation::UpNorth),
                    "up_south" => Some(properties::Orientation::UpSouth),
                    "up_west" => Some(properties::Orientation::UpWest),
                    "west_up" => Some(properties::Orientation::WestUp),
                    _ => None,
                })
                .and_then(|v| state.with(properties::ORIENTATION, v)),
                "part" => (match value {
                    "foot" => Some(properties::Part::Foot),
                    "head" => Some(properties::Part::Head),
                    _ => None,
                })
                .and_then(|v| state.with(properties::PART, v)),
                "persistent" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::PERSISTENT, v)),
                "pickles" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::PICKLES, v))
                }
                "potent_sulfur_state" => (match value {
                    "continuous" => Some(properties::PotentSulfurState::Continuous),
                    "dormant" => Some(properties::PotentSulfurState::Dormant),
                    "dry" => Some(properties::PotentSulfurState::Dry),
                    "erupting" => Some(properties::PotentSulfurState::Erupting),
                    "wet" => Some(properties::PotentSulfurState::Wet),
                    _ => None,
                })
                .and_then(|v| state.with(properties::POTENT_SULFUR_STATE, v)),
                "power" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::POWER, v))
                }
                "powered" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::POWERED, v)),
                "rotation" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::ROTATION, v))
                }
                "sculk_sensor_phase" => (match value {
                    "active" => Some(properties::SculkSensorPhase::Active),
                    "cooldown" => Some(properties::SculkSensorPhase::Cooldown),
                    "inactive" => Some(properties::SculkSensorPhase::Inactive),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SCULK_SENSOR_PHASE, v)),
                "segment_amount" => (value.parse::<u8>().ok())
                    .and_then(|v| state.with(properties::SEGMENT_AMOUNT, v)),
                "shape" => (match value {
                    "ascending_east" => Some(properties::Shape::AscendingEast),
                    "ascending_north" => Some(properties::Shape::AscendingNorth),
                    "ascending_south" => Some(properties::Shape::AscendingSouth),
                    "ascending_west" => Some(properties::Shape::AscendingWest),
                    "east_west" => Some(properties::Shape::EastWest),
                    "inner_left" => Some(properties::Shape::InnerLeft),
                    "inner_right" => Some(properties::Shape::InnerRight),
                    "north_east" => Some(properties::Shape::NorthEast),
                    "north_south" => Some(properties::Shape::NorthSouth),
                    "north_west" => Some(properties::Shape::NorthWest),
                    "outer_left" => Some(properties::Shape::OuterLeft),
                    "outer_right" => Some(properties::Shape::OuterRight),
                    "south_east" => Some(properties::Shape::SouthEast),
                    "south_west" => Some(properties::Shape::SouthWest),
                    "straight" => Some(properties::Shape::Straight),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SHAPE, v)),
                "short" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SHORT, v)),
                "shrieking" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SHRIEKING, v)),
                "side_chain" => (match value {
                    "center" => Some(properties::SideChain::Center),
                    "left" => Some(properties::SideChain::Left),
                    "right" => Some(properties::SideChain::Right),
                    "unconnected" => Some(properties::SideChain::Unconnected),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SIDE_CHAIN, v)),
                "signal_fire" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SIGNAL_FIRE, v)),
                "slot_0_occupied" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SLOT_0_OCCUPIED, v)),
                "slot_1_occupied" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SLOT_1_OCCUPIED, v)),
                "slot_2_occupied" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SLOT_2_OCCUPIED, v)),
                "slot_3_occupied" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SLOT_3_OCCUPIED, v)),
                "slot_4_occupied" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SLOT_4_OCCUPIED, v)),
                "slot_5_occupied" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SLOT_5_OCCUPIED, v)),
                "snowy" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SNOWY, v)),
                "south" => (match value {
                    "false" => Some(properties::South::False),
                    "low" => Some(properties::South::Low),
                    "none" => Some(properties::South::None),
                    "side" => Some(properties::South::Side),
                    "tall" => Some(properties::South::Tall),
                    "true" => Some(properties::South::True),
                    "up" => Some(properties::South::Up),
                    _ => None,
                })
                .and_then(|v| state.with(properties::SOUTH, v)),
                "stage" => {
                    (value.parse::<u8>().ok()).and_then(|v| state.with(properties::STAGE, v))
                }
                "thickness" => (match value {
                    "base" => Some(properties::Thickness::Base),
                    "frustum" => Some(properties::Thickness::Frustum),
                    "middle" => Some(properties::Thickness::Middle),
                    "tip" => Some(properties::Thickness::Tip),
                    "tip_merge" => Some(properties::Thickness::TipMerge),
                    _ => None,
                })
                .and_then(|v| state.with(properties::THICKNESS, v)),
                "tilt" => (match value {
                    "full" => Some(properties::Tilt::Full),
                    "none" => Some(properties::Tilt::None),
                    "partial" => Some(properties::Tilt::Partial),
                    "unstable" => Some(properties::Tilt::Unstable),
                    _ => None,
                })
                .and_then(|v| state.with(properties::TILT, v)),
                "tip" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::TIP, v)),
                "trial_spawner_state" => (match value {
                    "active" => Some(properties::TrialSpawnerState::Active),
                    "cooldown" => Some(properties::TrialSpawnerState::Cooldown),
                    "ejecting_reward" => Some(properties::TrialSpawnerState::EjectingReward),
                    "inactive" => Some(properties::TrialSpawnerState::Inactive),
                    "waiting_for_players" => Some(properties::TrialSpawnerState::WaitingForPlayers),
                    "waiting_for_reward_ejection" => {
                        Some(properties::TrialSpawnerState::WaitingForRewardEjection)
                    }
                    _ => None,
                })
                .and_then(|v| state.with(properties::TRIAL_SPAWNER_STATE, v)),
                "triggered" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::TRIGGERED, v)),
                "type" => (match value {
                    "bottom" => Some(properties::Type::Bottom),
                    "double" => Some(properties::Type::Double),
                    "left" => Some(properties::Type::Left),
                    "normal" => Some(properties::Type::Normal),
                    "right" => Some(properties::Type::Right),
                    "single" => Some(properties::Type::Single),
                    "sticky" => Some(properties::Type::Sticky),
                    "top" => Some(properties::Type::Top),
                    _ => None,
                })
                .and_then(|v| state.with(properties::TYPE, v)),
                "unstable" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::UNSTABLE, v)),
                "up" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::UP, v)),
                "vault_state" => (match value {
                    "active" => Some(properties::VaultState::Active),
                    "ejecting" => Some(properties::VaultState::Ejecting),
                    "inactive" => Some(properties::VaultState::Inactive),
                    "unlocking" => Some(properties::VaultState::Unlocking),
                    _ => None,
                })
                .and_then(|v| state.with(properties::VAULT_STATE, v)),
                "vertical_direction" => (match value {
                    "down" => Some(properties::VerticalDirection::Down),
                    "up" => Some(properties::VerticalDirection::Up),
                    _ => None,
                })
                .and_then(|v| state.with(properties::VERTICAL_DIRECTION, v)),
                "waterlogged" => (match value {
                    "false" => Some(false),
                    "true" => Some(true),
                    _ => None,
                })
                .and_then(|v| state.with(properties::WATERLOGGED, v)),
                "west" => (match value {
                    "false" => Some(properties::West::False),
                    "low" => Some(properties::West::Low),
                    "none" => Some(properties::West::None),
                    "side" => Some(properties::West::Side),
                    "tall" => Some(properties::West::Tall),
                    "true" => Some(properties::West::True),
                    "up" => Some(properties::West::Up),
                    _ => None,
                })
                .and_then(|v| state.with(properties::WEST, v)),
                _ => None,
            };
            if let Some(changed) = changed {
                state = changed;
            }
        }
    }
    Ok(state)
}
