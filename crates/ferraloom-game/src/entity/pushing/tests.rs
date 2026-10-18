use super::*;
use crate::{Behaviors, entity::item::PrimedTnt, level::Region};
use ferraloom_nbt::Tag;
use ferraloom_util::Uuid;
use std::cell::Cell;

const VECTORS: &str = include_str!("../../../tests/java/entity/vectors.tsv");
struct PushProbe;
impl Entity for PushProbe {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }
    fn is_pushable(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(level
            .entity::<EntityData>(entity)?
            .custom_data
            .get_boolean_or("pushable", false))
    }
}
fn level() -> Level {
    static BEHAVIORS: Behaviors =
        Behaviors::EMPTY.with_entity(ferraloom_data::EntityType::TNT, &PushProbe);
    Level::new(
        Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    )
}
fn add(level: &mut Level, id: u64, position: Vec3, velocity: Vec3, pushable: bool, physics: bool) {
    let mut tnt = PrimedTnt::new(id as i32, id as i64);
    tnt.parent.uuid = Uuid::new(0, id as i64);
    tnt.parent.set_pos(position);
    tnt.parent.velocity = velocity;
    tnt.parent.no_physics = !physics;
    tnt.parent
        .custom_data
        .insert("pushable", Tag::Byte(i8::from(pushable)));
    level
        .add_entity(EntityRef(id), EntityState::PrimedTnt(Box::new(tnt)))
        .unwrap();
}
fn vector(bits: &str) -> Vec3 {
    let mut values = bits
        .split(',')
        .map(|value| f64::from_bits(u64::from_str_radix(value, 16).unwrap()));
    Vec3::new(
        values.next().unwrap(),
        values.next().unwrap(),
        values.next().unwrap(),
    )
}
fn bits(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}

#[test]
fn native_pair_push_preserves_impulse_bits_vehicle_gates_and_sync_flags() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("entity-push\t"))
    {
        let row: Vec<_> = row.split('\t').collect();
        let mut level = level();
        add(
            &mut level,
            1,
            Vec3::new(0.0, 64.0, 0.0),
            vector(row[9]),
            row[5].parse().unwrap(),
            !row[3].parse::<bool>().unwrap(),
        );
        add(
            &mut level,
            2,
            vector(row[2]).add_xyz(0.0, 64.0, 0.0),
            vector(row[10]),
            row[6].parse().unwrap(),
            !row[4].parse::<bool>().unwrap(),
        );
        add(&mut level, 3, Vec3::ZERO, Vec3::ZERO, true, true);
        let a = EntityRef(1);
        let b = EntityRef(2);
        let mount = EntityRef(3);
        match row[7] {
            "1" => {
                crate::entity::start_riding(a, mount, true, &mut level).unwrap();
                crate::entity::start_riding(b, mount, true, &mut level).unwrap();
            }
            "2" => {
                crate::entity::start_riding(mount, a, true, &mut level).unwrap();
            }
            "3" => {
                crate::entity::start_riding(mount, b, true, &mut level).unwrap();
            }
            "0" => {}
            _ => panic!("unknown native setup"),
        }
        let other = if row[8].parse::<bool>().unwrap() {
            a
        } else {
            b
        };
        level
            .entity_behavior(a)
            .unwrap()
            .push(a, other, &mut level)
            .unwrap();
        let a = level.entity::<EntityData>(a).unwrap();
        let b = level.entity::<EntityData>(b).unwrap();
        assert_eq!(bits(a.velocity), row[11], "self case {}", row[1]);
        assert_eq!(bits(b.velocity), row[12], "other case {}", row[1]);
        assert_eq!(a.needs_sync.to_string(), row[13]);
        assert_eq!(b.needs_sync.to_string(), row[14]);
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn native_impulses_reject_nonfinite_components_and_mark_accepted_zero() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("entity-impulse\t"))
    {
        let row: Vec<_> = row.split('\t').collect();
        let mut base = EntityData::new(ferraloom_data::EntityType::TNT, 1, 0);
        base.velocity = vector(row[3]);
        base.push_impulse(vector(row[2]));
        assert_eq!(bits(base.velocity), row[4], "case {}", row[1]);
        assert_eq!(base.needs_sync.to_string(), row[5]);
        count += 1;
    }
    assert_eq!(count, 4);
}

struct Teams {
    own_rule: Cell<TeamCollisionRule>,
    reads: [Cell<u32>; 2],
}
impl TeamRead for &Teams {
    type Team = usize;
    fn team_by_scoreboard_name(&self, name: &str) -> Option<usize> {
        let index = if name == Uuid::new(0, 1).to_string() {
            0
        } else {
            assert_eq!(name, Uuid::new(0, 2).to_string());
            1
        };
        self.reads[index].set(self.reads[index].get() + 1);
        Some(index)
    }
    fn collision_rule(&self, team: usize) -> TeamCollisionRule {
        if team == 0 {
            self.own_rule.get()
        } else {
            TeamCollisionRule::Always
        }
    }
    fn is_allied_to(&self, team: usize, other: Option<usize>) -> bool {
        other == Some(team)
    }
}

#[test]
fn native_push_predicate_captures_source_rule_and_short_circuits_target_reads() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("push-team-capture\t"))
    {
        let row: Vec<_> = row.split('\t').collect();
        let mut level = level();
        add(&mut level, 1, Vec3::ZERO, Vec3::ZERO, true, true);
        add(&mut level, 2, Vec3::ZERO, Vec3::ZERO, true, true);
        let teams = Teams {
            own_rule: Cell::new(if row[1].parse().unwrap() {
                TeamCollisionRule::Never
            } else {
                TeamCollisionRule::PushOtherTeams
            }),
            reads: [Cell::new(0), Cell::new(0)],
        };
        let predicate = PushableBy::new(EntityRef(1), &level, &teams).unwrap();
        teams.own_rule.set(TeamCollisionRule::Always);
        assert_eq!(
            predicate.test(EntityRef(2), &level).unwrap().to_string(),
            row[2]
        );
        assert_eq!(
            format!("{},{}", teams.reads[0].get(), teams.reads[1].get()),
            row[3]
        );
        count += 1;
    }
    assert_eq!(count, 2);
}
