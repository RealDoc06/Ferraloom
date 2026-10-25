//! Port of ServerBossEvent. Mutation outputs are appended synchronously in native order.
use crate::entity::EntityRef;
use ferraloom_protocol::{
    Uuid,
    chat::Component,
    play::{BossBarColor, BossBarOverlay, BossOperation, BossProperties, clientbound as cb},
};
use ferraloom_util::collections::HashMap;

/// Immutable identity captured from a canonical ServerPlayer, without cached names or gameplay state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BossPlayer {
    pub entity: EntityRef,
    pub uuid: Uuid,
    pub network_id: i32,
}

/// Native per-player sends. The owner drains this in emission order before feedback/callbacks.
pub type BossPackets = Vec<(EntityRef, cb::Packet)>;

/// Port of BossEvent's virtual setters.
pub enum BossEventUpdate {
    Name(Box<Component>),
    Progress(f32),
    Color(BossBarColor),
    Overlay(BossBarOverlay),
    Visible(bool),
    DarkenScreen(bool),
    PlayBossMusic(bool),
    CreateWorldFog(bool),
}

/// Port of ServerBossEvent. Online membership uses Entity#hashCode (the network ID).
#[derive(Debug)]
pub struct ServerBossEvent {
    id: Uuid,
    name: Component,
    progress: f32,
    color: BossBarColor,
    overlay: BossBarOverlay,
    properties: BossProperties,
    visible: bool,
    players: HashMap<i32, BossPlayer>,
}
impl ServerBossEvent {
    /// Port of BossEvent's constructor: initial progress is 1, visibility is true.
    pub fn new(id: Uuid, name: Component, color: BossBarColor, overlay: BossBarOverlay) -> Self {
        Self {
            id,
            name,
            progress: 1.0,
            color,
            overlay,
            properties: BossProperties {
                darken_screen: false,
                play_music: false,
                create_world_fog: false,
            },
            visible: true,
            players: HashMap::new(),
        }
    }
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn name(&self) -> &Component {
        &self.name
    }
    pub fn progress(&self) -> f32 {
        self.progress
    }
    pub fn color(&self) -> BossBarColor {
        self.color
    }
    pub fn overlay(&self) -> BossBarOverlay {
        self.overlay
    }
    pub fn properties(&self) -> &BossProperties {
        &self.properties
    }
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    pub fn players(&self) -> impl ExactSizeIterator<Item = &BossPlayer> {
        self.players.values()
    }
    fn add_operation(&self) -> BossOperation {
        BossOperation::Add {
            name: self.name.clone(),
            progress: self.progress,
            color: self.color,
            overlay: self.overlay,
            properties: self.properties.clone(),
        }
    }
    fn send(&self, player: EntityRef, operation: BossOperation, packets: &mut BossPackets) {
        packets.push((
            player,
            cb::Packet::BossEvent(Box::new(cb::BossEvent {
                id: self.id,
                operation,
            })),
        ));
    }
    fn broadcast(&self, operation: BossOperation, packets: &mut BossPackets) {
        if self.visible {
            for player in self.players.values() {
                self.send(player.entity, operation.clone(), packets);
            }
        }
    }
    /// Port of ServerBossEvent's setters. The bool is its virtual setDirty notification.
    pub fn update(&mut self, update: BossEventUpdate, packets: &mut BossPackets) -> bool {
        let operation = match update {
            BossEventUpdate::Name(value) => {
                if self.name == *value {
                    return false;
                }
                self.name = *value;
                BossOperation::UpdateName {
                    name: self.name.clone(),
                }
            }
            BossEventUpdate::Progress(value) => {
                // Java != deliberately treats NaN as changed and signed zeros as equal.
                if self.progress == value {
                    return false;
                }
                self.progress = value;
                BossOperation::UpdateProgress { progress: value }
            }
            BossEventUpdate::Color(value) => {
                if self.color == value {
                    return false;
                }
                self.color = value;
                BossOperation::UpdateStyle {
                    color: self.color,
                    overlay: self.overlay,
                }
            }
            BossEventUpdate::Overlay(value) => {
                if self.overlay == value {
                    return false;
                }
                self.overlay = value;
                BossOperation::UpdateStyle {
                    color: self.color,
                    overlay: self.overlay,
                }
            }
            BossEventUpdate::Visible(value) => {
                if self.visible == value {
                    return false;
                }
                self.visible = value;
                for player in self.players.values() {
                    self.send(
                        player.entity,
                        if value {
                            self.add_operation()
                        } else {
                            BossOperation::Remove {}
                        },
                        packets,
                    );
                }
                return true;
            }
            BossEventUpdate::DarkenScreen(value) => {
                if self.properties.darken_screen == value {
                    return false;
                }
                self.properties.darken_screen = value;
                BossOperation::UpdateProperties {
                    properties: self.properties.clone(),
                }
            }
            BossEventUpdate::PlayBossMusic(value) => {
                if self.properties.play_music == value {
                    return false;
                }
                self.properties.play_music = value;
                BossOperation::UpdateProperties {
                    properties: self.properties.clone(),
                }
            }
            BossEventUpdate::CreateWorldFog(value) => {
                if self.properties.create_world_fog == value {
                    return false;
                }
                self.properties.create_world_fog = value;
                BossOperation::UpdateProperties {
                    properties: self.properties.clone(),
                }
            }
        };
        self.broadcast(operation, packets);
        true
    }
    /// Port of ServerBossEvent#addPlayer: existing membership sends nothing.
    pub fn add_player(&mut self, player: BossPlayer, packets: &mut BossPackets) {
        if !self.players.contains_key(&player.network_id) {
            self.players.insert(player.network_id, player);
            if self.visible {
                self.send(player.entity, self.add_operation(), packets);
            }
        }
    }
    /// Port of ServerBossEvent#removePlayer. Visibility suppresses the remove packet, not removal.
    pub fn remove_player(&mut self, player: BossPlayer, packets: &mut BossPackets) {
        if self.players.remove(&player.network_id).is_some() && self.visible {
            self.send(player.entity, BossOperation::Remove {}, packets);
        }
    }
    /// Port of ServerBossEvent#removeAllPlayers's captured native iteration order.
    pub fn remove_all_players(&mut self, packets: &mut BossPackets) {
        let players: Vec<_> = self.players.values().copied().collect();
        for player in players {
            self.remove_player(player, packets);
        }
    }
}
