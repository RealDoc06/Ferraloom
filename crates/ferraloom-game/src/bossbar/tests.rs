use super::*;
use crate::entity::EntityRef;
use ferraloom_protocol::{Wire, play::clientbound as cb};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn vanilla_boss_event_lifecycle_and_packet_bytes() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/java/bossbar/events.json")).unwrap();
    assert_eq!(cases.len(), 51);
    let mut event = CustomBossEvent::new(
        Uuid::new(0, 17),
        Identifier::parse("test:bar").unwrap(),
        Component::literal("Boss"),
    );
    let players = [
        BossPlayer {
            entity: EntityRef(1),
            uuid: Uuid::new(0, 1),
            network_id: 16,
        },
        BossPlayer {
            entity: EntityRef(2),
            uuid: Uuid::new(0, 2),
            network_id: 32,
        },
    ];
    let colors = [
        BossBarColor::Pink,
        BossBarColor::Blue,
        BossBarColor::Red,
        BossBarColor::Green,
        BossBarColor::Yellow,
        BossBarColor::Purple,
        BossBarColor::White,
    ];
    let overlays = [
        BossBarOverlay::Progress,
        BossBarOverlay::Notched6,
        BossBarOverlay::Notched10,
        BossBarOverlay::Notched12,
        BossBarOverlay::Notched20,
    ];
    for expected in cases {
        let action = expected["action"].as_str().unwrap();
        let argument = expected["argument"].as_str().unwrap();
        let mut packets = Vec::new();
        event.dirty = false;
        match action {
            "add" => event.add_player(players[argument.parse::<usize>().unwrap()], &mut packets),
            "remove" => {
                event.remove_player(players[argument.parse::<usize>().unwrap()], &mut packets)
            }
            "connect" => {
                event.on_player_connect(players[argument.parse::<usize>().unwrap()], &mut packets)
            }
            "disconnect" => event
                .on_player_disconnect(players[argument.parse::<usize>().unwrap()], &mut packets),
            "players" => {
                let selected: Vec<_> = if argument.is_empty() {
                    Vec::new()
                } else {
                    argument
                        .split(',')
                        .map(|index| players[index.parse::<usize>().unwrap()])
                        .collect()
                };
                event.set_players(&selected, &mut packets);
            }
            "clear" => event.remove_all_players(&mut packets),
            "value" => event.set_value(argument.parse().unwrap(), &mut packets),
            "max" => event.set_max(argument.parse().unwrap(), &mut packets),
            _ => {
                let update = match action {
                    "progress" => BossEventUpdate::Progress(argument.parse().unwrap()),
                    "name" => BossEventUpdate::Name(Component::literal(argument)),
                    "color" => {
                        BossEventUpdate::Color(colors[expected["color"].as_u64().unwrap() as usize])
                    }
                    "overlay" => BossEventUpdate::Overlay(
                        overlays[expected["overlay"].as_u64().unwrap() as usize],
                    ),
                    "visible" => BossEventUpdate::Visible(argument.parse().unwrap()),
                    "darken" => BossEventUpdate::DarkenScreen(argument.parse().unwrap()),
                    "music" => BossEventUpdate::PlayBossMusic(argument.parse().unwrap()),
                    "fog" => BossEventUpdate::CreateWorldFog(argument.parse().unwrap()),
                    _ => panic!("native action {action}"),
                };
                event.update(update, &mut packets);
            }
        }
        let mut online: Vec<_> = event
            .server()
            .players()
            .map(|player| player.uuid.to_string())
            .collect();
        let mut persistent: Vec<_> = event
            .persistent_players()
            .map(ToString::to_string)
            .collect();
        online.sort();
        persistent.sort();
        let mut sends = BTreeMap::<String, Vec<String>>::new();
        for (entity, packet) in packets {
            let cb::Packet::BossEvent(packet) = packet else {
                panic!("non-boss packet");
            };
            let hex = packet
                .to_bytes()
                .unwrap()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let player = players
                .iter()
                .find(|player| player.entity == entity)
                .unwrap();
            sends.entry(player.uuid.to_string()).or_default().push(hex);
        }
        let progress = event.server().progress();
        let progress_bits = if progress.is_nan() {
            0x7fc0_0000
        } else {
            progress.to_bits()
        } as i32;
        let display: Value =
            serde_json::from_str(&event.display_name().to_json().unwrap()).unwrap();
        let actual = json!({"action":action,"argument":argument,"dirty":event.dirty,"value":event.value(),"max":event.max(),
            "progressBits":progress_bits,"visible":event.server().is_visible(),"color":event.server().color() as u8,
            "overlay":event.server().overlay() as u8,"display":display,"online":online,"persistent":persistent,"packets":sends});
        assert_eq!(actual, expected, "{action} {argument}");
    }
}
