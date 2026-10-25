use crate::{
    Behaviors,
    entity::{
        EntityRef, EntityState,
        animal::Cow,
        damage::{DamageSource, DamageTypeKey},
        player::{GameType, ServerPlayer},
    },
    level::{Level, LevelContext, LevelError, Region, effects::LevelEffect},
};
use ferraloom_protocol::{Identifier, codec::RegistryContext, play::clientbound::Packet};
use ferraloom_util::{Uuid, Vec3};
use std::{collections::BTreeMap, sync::Arc};

struct WireRegistry(BTreeMap<Identifier, i32>);
impl RegistryContext for WireRegistry {
    fn entry_id(&self, registry: &Identifier, entry: &Identifier) -> Option<i32> {
        (registry.as_str() == "minecraft:damage_type")
            .then(|| self.0.get(entry).copied())
            .flatten()
    }
    fn entry_key(&self, registry: &Identifier, id: i32) -> Option<Identifier> {
        (registry.as_str() == "minecraft:damage_type")
            .then(|| {
                self.0
                    .iter()
                    .find_map(|(key, &number)| (number == id).then(|| key.clone()))
            })
            .flatten()
    }
    fn has_tag(&self, _: &Identifier, _: &Identifier) -> bool {
        panic!("damage packet construction must not inspect tags")
    }
}
fn fixture(context: Arc<LevelContext>) -> (Level, EntityRef, EntityRef) {
    let mut level = Level::new(Region::new(&Behaviors::EMPTY), context);
    let player = level.next_entity_ref();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "viewer".into(),
                7,
                Vec3::new(0.0, 64.0, 0.0),
                GameType::Spectator,
            ))),
        )
        .unwrap();
    let direct = level.next_entity_ref();
    let mut cow = Cow::default();
    cow.as_living_mut().parent.network_id = 12;
    cow.as_living_mut().parent.network_id_allocated = true;
    level
        .add_entity(direct, EntityState::Cow(Box::new(cow)))
        .unwrap();
    level.take_effects();
    (level, player, direct)
}
#[test]
fn native_damage_packet_preserves_entity_roles_and_raw_source_position() {
    let rows: Vec<_> = include_str!("../../../tests/java/entity/tracker/vectors.tsv")
        .lines()
        .filter_map(|line| line.strip_prefix("damage\t"))
        .map(|row| row.split('\t').collect::<Vec<_>>())
        .collect();
    let keys = [
        DamageTypeKey::Generic,
        DamageTypeKey::PlayerAttack,
        DamageTypeKey::Arrow,
        DamageTypeKey::Fall,
    ];
    let names = [
        "minecraft:generic",
        "minecraft:player_attack",
        "minecraft:arrow",
        "minecraft:fall",
    ];
    let map = names
        .into_iter()
        .zip(&rows)
        .map(|(name, row)| (name.parse().unwrap(), row[1].parse().unwrap()))
        .collect();
    let context = Arc::new(
        LevelContext::fixture(0, 63)
            .as_ref()
            .clone()
            .with_registry_context(Arc::new(WireRegistry(map))),
    );
    let (mut level, player, direct) = fixture(context);
    for (index, row) in rows.into_iter().enumerate() {
        let holder = level.damage_types().holder(keys[index]).clone();
        let source = match row[0] {
            "generic" => DamageSource::new(holder),
            "player" => DamageSource::with_cause(holder, Some(player)),
            "indirect" => DamageSource::with_entities(holder, Some(direct), Some(player)),
            "position" => DamageSource::at_position(holder, Vec3::new(1.25, 63.0, -0.0)),
            _ => unreachable!(),
        };
        level.broadcast_damage_event(player, &source).unwrap();
        let effects = level.take_effects();
        let [LevelEffect::PlayerPacket { recipients, packet }] = effects.as_slice() else {
            panic!("one captured damage event")
        };
        assert_eq!(recipients, &[player]);
        let Packet::DamageEvent(packet) = packet.as_ref() else {
            panic!("damage event")
        };
        assert_eq!(packet.entity_id.0, 7);
        assert_eq!(packet.source_type.0.to_string(), row[1]);
        assert_eq!(packet.source_cause_id.0.unwrap_or(-1).to_string(), row[2]);
        assert_eq!(packet.source_direct_id.0.unwrap_or(-1).to_string(), row[3]);
        let position = packet.source_position.as_ref().map_or_else(
            || "-".into(),
            |pos| {
                format!(
                    "{:x},{:x},{:x}",
                    pos.x.to_bits(),
                    pos.y.to_bits(),
                    pos.z.to_bits()
                )
            },
        );
        assert_eq!(position, row[4]);
    }
}
#[test]
fn damage_type_wire_number_is_resolved_through_the_published_registry() {
    let registry = WireRegistry([("minecraft:generic".parse().unwrap(), 123)].into());
    let context = Arc::new(
        LevelContext::fixture(0, 63)
            .as_ref()
            .clone()
            .with_registry_context(Arc::new(registry)),
    );
    let (mut level, player, _) = fixture(context);
    let source = DamageSource::new(level.damage_types().holder(DamageTypeKey::Generic).clone());
    level.broadcast_damage_event(player, &source).unwrap();
    assert!(level.take_effects().iter().any(|effect| matches!(effect, LevelEffect::PlayerPacket {packet,..} if matches!(packet.as_ref(), Packet::DamageEvent(packet) if packet.source_type.0 == 123))));
}
#[test]
fn missing_registry_context_is_required_and_entity_events_include_self() {
    let context = LevelContext::fixture(0, 63);
    let missing = Arc::new(LevelContext::new(
        context.seed,
        context.sea_level,
        context.generator.clone(),
        context.features.clone(),
        context.structures.clone(),
        context.damage_types().clone(),
        context.initial_difficulty,
    ));
    let (mut level, player, _) = fixture(missing);
    let source = DamageSource::new(level.damage_types().holder(DamageTypeKey::Generic).clone());
    assert_eq!(
        level.broadcast_damage_event(player, &source),
        Err(LevelError::MissingRegistryContext)
    );
    assert!(level.take_effects().is_empty());
    level.broadcast_entity_event(player, 47).unwrap();
    let effects = level.take_effects();
    assert!(
        matches!(effects.as_slice(), [LevelEffect::PlayerPacket {recipients, packet}] if recipients == &[player] && matches!(packet.as_ref(), Packet::EntityEvent(packet) if packet.entity_id == 7 && packet.event_id == 47))
    );
}
