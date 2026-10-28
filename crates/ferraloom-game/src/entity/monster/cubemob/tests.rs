use super::*;
use crate::{
    entity::{
        EntityData, LivingEntity,
        ai::{AiOperation, MoveControlKind, NavigationKind},
    },
    level::Level,
};
use ferraloom_data::{EntityType, registries::attribute::Id as Attribute};
use ferraloom_nbt::{CompoundTag, Tag};

const VECTORS: &str = include_str!("../../../../tests/java/entity/monster/vectors.tsv");

enum Cube {
    Slime(Slime),
    Magma(MagmaCube),
}
impl Cube {
    fn new(kind: EntityType, level: &Level) -> Self {
        let base = EntityData::new(kind, 1, 0);
        match kind {
            EntityType::SLIME => Self::Slime(Slime::with_level(base, level).unwrap()),
            EntityType::MAGMA_CUBE => Self::Magma(MagmaCube::with_level(base, level).unwrap()),
            _ => panic!("not a concrete cube"),
        }
    }
    fn parent(&self) -> &AbstractCubeMob {
        match self {
            Self::Slime(s) => &s.parent,
            Self::Magma(m) => &m.parent,
        }
    }
    fn parent_mut(&mut self) -> &mut AbstractCubeMob {
        match self {
            Self::Slime(s) => &mut s.parent,
            Self::Magma(m) => &mut m.parent,
        }
    }
    fn can_be_a_baby(&self) -> bool {
        match self {
            Self::Slime(s) => s.can_be_a_baby(),
            Self::Magma(m) => m.can_be_a_baby(),
        }
    }
    fn apply_attributes(&mut self, change: SizeChange, heal: bool) {
        match self {
            Self::Slime(s) => s.apply_size_attributes(change, heal),
            Self::Magma(m) => m.apply_size_attributes(change, heal),
        }
    }
    fn read_size_then_parent(&mut self, tag: &CompoundTag) {
        let change = self.parent_mut().begin_size_read(tag);
        // The vectors use new actors out of water, so size callbacks have no
        // world/RNG effects on the attributes measured here. Live root wiring
        // must still perform those callbacks before this tail.
        self.apply_attributes(change, false);
        self.parent_mut()
            .as_living_mut()
            .read_additional_save_data(tag, LivingEntity::internal_set_absorption_amount);
        // Both concrete cube classes have canBeABaby=false; Ageable's baby-only
        // NBT reads are skipped, then AbstractCubeMob reads its own ground flag.
        self.parent_mut().read_after_parent(tag);
    }
}

fn check_state(cube: &Cube, fields: &[&str]) {
    let parent = cube.parent();
    let living = parent.as_living();
    assert_eq!(parent.size(), fields[0].parse::<i32>().unwrap());
    for (attribute, expected) in [
        Attribute::MAX_HEALTH,
        Attribute::MOVEMENT_SPEED,
        Attribute::ATTACK_DAMAGE,
        Attribute::ARMOR,
    ]
    .into_iter()
    .zip(&fields[1..5])
    {
        assert_eq!(
            living.attributes.base_value(attribute).unwrap().to_bits(),
            u64::from_str_radix(expected, 16).unwrap(),
            "{attribute:?}"
        );
    }
    assert_eq!(
        living.health().to_bits(),
        u32::from_str_radix(fields[5], 16).unwrap()
    );
    assert_eq!(parent.as_mob().xp_reward, fields[6].parse::<i32>().unwrap());
}

#[test]
fn native_cube_constructors_use_ageable_parent_and_do_not_apply_size_attributes() {
    let level = crate::entity::tests::level(false);
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|s| s.starts_with("monster-constructor\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let kind = EntityType::from_name(f[1]).unwrap();
        if !matches!(kind, EntityType::SLIME | EntityType::MAGMA_CUBE) {
            continue;
        }
        let cube = Cube::new(kind, &level);
        let parent = cube.parent();
        assert_eq!(f[2], "AbstractCubeMob");
        assert_eq!(f[3], "GroundPathNavigation");
        assert_eq!(f[4], "CubeMobMoveControl");
        assert_eq!(parent.as_mob().xp_reward, f[5].parse::<i32>().unwrap());
        assert_eq!(
            parent.as_living().dimensions.width.to_bits(),
            u32::from_str_radix(f[6], 16).unwrap()
        );
        assert_eq!(
            parent.as_living().dimensions.height.to_bits(),
            u32::from_str_radix(f[7], 16).unwrap()
        );
        assert_eq!(
            parent.as_living().eye_height.to_bits(),
            u32::from_str_radix(f[8], 16).unwrap()
        );
        assert_eq!(
            parent.as_living().health().to_bits(),
            u32::from_str_radix(f[9], 16).unwrap()
        );
        assert_eq!(
            parent.as_mob().ai.navigation.require(),
            Err(AiOperation::Navigation(NavigationKind::Ground))
        );
        assert_eq!(
            parent.as_mob().ai.move_control.kind(),
            MoveControlKind::AbstractCube
        );
        assert_eq!(parent.parent.age, 0);
        assert_eq!(parent.parent.forced_age, 0);
        assert_eq!(parent.size(), 1);
        assert_eq!(
            (parent.target_squish, parent.squish, parent.o_squish),
            (0.0, 0.0, 0.0)
        );
        assert!(!parent.was_on_ground);
        checked += 1;
    }
    assert_eq!(checked, 2);
}

#[test]
fn native_set_size_retains_original_argument_and_rereads_metadata_after_callbacks() {
    let level = crate::entity::tests::level(false);
    let mut checked = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("cube-resize\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut cube = Cube::new(EntityType::from_name(f[1]).unwrap(), &level);
        let change = cube.parent_mut().begin_size_change(f[2].parse().unwrap());
        assert_eq!(change.requested_size(), f[2].parse::<i32>().unwrap());
        if f[4] == "true" {
            assert!(change.metadata_changed);
            // Actual Java metadata callback recursively calls setSize(2,false)
            // before the original setSize tail resumes with its captured input.
            let nested = cube.parent_mut().begin_size_change(2);
            cube.apply_attributes(nested, false);
        }
        cube.apply_attributes(change, f[3] == "true");
        check_state(&cube, &f[5..12]);
        checked += 1;
    }
    assert_eq!(checked, 36);
}

fn input(saved_size: i32, override_attributes: bool) -> CompoundTag {
    let mut tag = CompoundTag::new();
    tag.insert("Size", Tag::Int(saved_size));
    tag.insert("wasOnGround", Tag::Byte(1));
    if override_attributes {
        let attributes = ferraloom_nbt::parse("[{id:'minecraft:max_health',base:81.0d},{id:'minecraft:movement_speed',base:0.777d},{id:'minecraft:attack_damage',base:55.0d}]").unwrap();
        tag.insert("attributes", attributes);
    }
    tag
}

#[test]
fn native_cube_nbt_size_precedes_parent_attributes_including_wrapping_add() {
    let level = crate::entity::tests::level(false);
    let mut checked = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("cube-load\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut cube = Cube::new(EntityType::from_name(f[1]).unwrap(), &level);
        cube.read_size_then_parent(&input(f[2].parse().unwrap(), f[3] == "true"));
        check_state(&cube, &f[4..11]);
        assert_eq!(cube.parent().was_on_ground, f[11] == "true");
        checked += 1;
    }
    assert_eq!(checked, 36);
}

#[test]
fn native_cube_nbt_round_trips_preserve_parent_attribute_overrides() {
    let level = crate::entity::tests::level(false);
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|s| s.starts_with("cube-roundtrip\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let kind = EntityType::from_name(f[1]).unwrap();
        let mut cube = Cube::new(kind, &level);
        cube.read_size_then_parent(&input(f[2].parse().unwrap(), f[3] == "true"));
        let mut saved = CompoundTag::new();
        let living = cube.parent().as_living();
        living
            .add_additional_save_data(&mut saved, &level, LivingEntity::absorption_amount)
            .unwrap();
        cube.parent().add_cube_save_data(&mut saved);
        let mut again = Cube::new(kind, &level);
        again.read_size_then_parent(&saved);
        check_state(&again, &f[4..11]);
        let mut repeated = CompoundTag::new();
        again.parent().add_cube_save_data(&mut repeated);
        assert_eq!(
            repeated.get_int_or("Size", 0),
            f[11].parse::<i32>().unwrap()
        );
        assert_eq!(
            repeated.get_boolean_or("wasOnGround", false),
            f[12] == "true"
        );
        checked += 1;
    }
    assert_eq!(checked, 36);
}

#[test]
fn native_concrete_cubes_ignore_ageable_nbt_and_do_not_write_baby_fields() {
    let level = crate::entity::tests::level(false);
    let mut checked = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("cube-age\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut cube = Cube::new(EntityType::from_name(f[1]).unwrap(), &level);
        let mut tag = CompoundTag::new();
        tag.insert("Age", Tag::Int(f[2].parse().unwrap()));
        tag.insert("ForcedAge", Tag::Int(123));
        tag.insert("AgeLocked", Tag::Byte(1));
        assert!(!cube.can_be_a_baby());
        cube.read_size_then_parent(&tag);
        let parent = cube.parent();
        assert_eq!(parent.parent.age, f[3].parse::<i32>().unwrap());
        assert_eq!(
            *parent
                .as_living()
                .entity_data
                .get(crate::entity::synched_data::BABY),
            f[4] == "true"
        );
        assert_eq!(parent.parent.forced_age, 0);
        assert!(
            !*parent
                .as_living()
                .entity_data
                .get(crate::entity::synched_data::AGE_LOCKED)
        );
        let mut saved = CompoundTag::new();
        parent
            .as_living()
            .add_additional_save_data(&mut saved, &level, LivingEntity::absorption_amount)
            .unwrap();
        parent.add_cube_save_data(&mut saved);
        for (field, expected) in ["Age", "ForcedAge", "AgeLocked"].into_iter().zip(&f[5..8]) {
            assert_eq!(saved.contains_key(field), *expected == "true");
        }
        checked += 1;
    }
    assert_eq!(checked, 6);
}
