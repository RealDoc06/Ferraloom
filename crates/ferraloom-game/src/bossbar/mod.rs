//! Port of CustomBossEvents, CustomBossEvent and ServerBossEvent.
mod save;
mod server;
pub use save::{BossEventsSaveData, PackedBossEvent};
#[cfg(test)]
mod tests;
pub use server::{BossEventUpdate, BossPackets, BossPlayer, ServerBossEvent};

use ferraloom_protocol::{
    Identifier, Uuid,
    chat::{Argument, Component, HoverEvent, TextColor},
    play::{BossBarColor, BossBarOverlay},
};
use ferraloom_util::collections::{HashMap, HashSet, JavaHash, identifier_hash_code};

#[derive(Debug, Clone, Eq, PartialEq)]
struct Key(Identifier);
impl JavaHash for Key {
    fn java_hash_code(&self) -> i32 {
        identifier_hash_code(self.0.namespace(), self.0.path())
    }
}

/// Port of CustomBossEvent. Persistent UUID membership is separate from online ServerPlayers.
#[derive(Debug)]
pub struct CustomBossEvent {
    server: ServerBossEvent,
    custom_id: Identifier,
    players: HashSet<Uuid>,
    value: i32,
    max: i32,
    dirty: bool,
}
impl CustomBossEvent {
    pub fn new(id: Uuid, custom_id: Identifier, name: Component) -> Self {
        let mut server =
            ServerBossEvent::new(id, name, BossBarColor::White, BossBarOverlay::Progress);
        server.update(BossEventUpdate::Progress(0.0), &mut Vec::new());
        Self {
            server,
            custom_id,
            players: HashSet::new(),
            value: 0,
            max: 100,
            dirty: true,
        }
    }
    pub fn server(&self) -> &ServerBossEvent {
        &self.server
    }
    pub fn custom_id(&self) -> &Identifier {
        &self.custom_id
    }
    pub fn value(&self) -> i32 {
        self.value
    }
    pub fn max(&self) -> i32 {
        self.max
    }
    pub fn persistent_players(&self) -> impl ExactSizeIterator<Item = &Uuid> {
        self.players.iter()
    }
    pub fn update(&mut self, update: BossEventUpdate, packets: &mut BossPackets) {
        self.dirty |= self.server.update(update, packets);
    }
    /// Port of CustomBossEvent#setValue. Casts precede division, including overflow/NaN semantics.
    pub fn set_value(&mut self, value: i32, packets: &mut BossPackets) {
        self.value = value;
        self.update_progress(packets);
    }
    pub fn set_max(&mut self, max: i32, packets: &mut BossPackets) {
        self.max = max;
        self.update_progress(packets);
    }
    fn update_progress(&mut self, packets: &mut BossPackets) {
        let ratio = self.value as f32 / self.max as f32;
        self.server.update(
            BossEventUpdate::Progress(ferraloom_util::mth::clamp_f32(ratio, 0.0, 1.0)),
            packets,
        );
        self.dirty = true;
    }
    /// Port of CustomBossEvent#getDisplayName. Style belongs to the bracket wrapper.
    pub fn display_name(&self) -> Component {
        let mut result = Component::translatable(
            "chat.square_brackets",
            vec![Argument::Component(Box::new(self.server.name().clone()))],
        );
        let color = match self.server.color() {
            BossBarColor::Pink => "red",
            BossBarColor::Blue => "blue",
            BossBarColor::Red => "dark_red",
            BossBarColor::Green => "green",
            BossBarColor::Yellow => "yellow",
            BossBarColor::Purple => "dark_blue",
            BossBarColor::White => "white",
        };
        result.style.color = Some(TextColor::parse(color).expect("native bossbar color"));
        result.style.hover_event = Some(HoverEvent::ShowText(Box::new(Component::literal(
            self.custom_id.as_str(),
        ))));
        result.style.insertion = Some(self.custom_id.to_string());
        result
    }
    pub fn add_player(&mut self, player: BossPlayer, packets: &mut BossPackets) {
        self.server.add_player(player, packets);
        self.dirty |= self.players.insert(player.uuid);
    }
    pub fn remove_player(&mut self, player: BossPlayer, packets: &mut BossPackets) {
        self.server.remove_player(player, packets);
        self.dirty |= self.players.remove(&player.uuid);
    }
    pub fn remove_all_players(&mut self, packets: &mut BossPackets) {
        // Native virtual removePlayer removes each online UUID before the remaining offline set.
        let online: Vec<_> = self.server.players().copied().collect();
        for player in online {
            self.remove_player(player, packets);
        }
        if !self.players.is_empty() {
            self.players.clear();
            self.dirty = true;
        }
    }
    /// Port of CustomBossEvent#setPlayers, including removal of disconnected UUIDs.
    pub fn set_players(&mut self, players: &[BossPlayer], packets: &mut BossPackets) -> bool {
        let mut to_remove = HashSet::new();
        let mut to_add = HashMap::new();
        for uuid in self.players.iter() {
            if !players.iter().any(|player| player.uuid == *uuid) {
                to_remove.insert(*uuid);
            }
        }
        for player in players {
            if !self.players.contains(&player.uuid) {
                to_add.insert(player.network_id, *player);
            }
        }
        for uuid in to_remove.iter() {
            let online = self
                .server
                .players()
                .find(|player| player.uuid == *uuid)
                .copied();
            if let Some(player) = online {
                self.remove_player(player, packets);
            }
            self.players.remove(uuid);
        }
        for player in to_add.values() {
            self.add_player(*player, packets);
        }
        let changed = !to_remove.is_empty() || !to_add.is_empty();
        self.dirty |= changed;
        changed
    }
    pub fn on_player_connect(&mut self, player: BossPlayer, packets: &mut BossPackets) {
        if self.players.contains(&player.uuid) {
            self.add_player(player, packets);
        }
    }
    pub fn on_player_disconnect(&mut self, player: BossPlayer, packets: &mut BossPackets) {
        self.server.remove_player(player, packets);
    }
}

/// Port of CustomBossEvents. This is the single canonical owner held by ServerShared.
#[derive(Debug, Default)]
pub struct CustomBossEvents {
    events: HashMap<Key, CustomBossEvent>,
    packets: BossPackets,
    dirty: bool,
}
impl CustomBossEvents {
    pub const STORAGE_ID: &'static str = "minecraft:custom_boss_events";
    pub fn get(&self, id: &Identifier) -> Option<&CustomBossEvent> {
        self.events.peek(&Key(id.clone()))
    }
    pub fn events(&self) -> impl ExactSizeIterator<Item = &CustomBossEvent> {
        self.events.values()
    }
    /// The UUID is drawn by the caller from the live level RNG at native createBar's call site.
    pub fn create(&mut self, id: Uuid, custom_id: Identifier, name: Component) {
        self.events.insert(
            Key(custom_id.clone()),
            CustomBossEvent::new(id, custom_id, name),
        );
        self.dirty = true;
    }
    pub fn remove(&mut self, id: &Identifier) {
        if let Some(mut event) = self.events.remove(&Key(id.clone())) {
            event.remove_all_players(&mut self.packets);
            self.dirty = true;
        }
    }
    /// Short owner borrow; the callback receives only the event and its typed output buffer.
    pub fn update<T>(
        &mut self,
        id: &Identifier,
        update: impl FnOnce(&mut CustomBossEvent, &mut BossPackets) -> T,
    ) -> Option<T> {
        let event = self.events.get_mut(&Key(id.clone()))?;
        let result = update(event, &mut self.packets);
        self.dirty |= event.dirty;
        event.dirty = false;
        Some(result)
    }
    pub fn on_player_connect(&mut self, player: BossPlayer) {
        let ids: Vec<_> = self.events.keys().map(|key| key.0.clone()).collect();
        for id in ids {
            self.update(&id, |event, packets| {
                event.on_player_connect(player, packets)
            });
        }
    }
    pub fn on_player_disconnect(&mut self, player: BossPlayer) {
        let ids: Vec<_> = self.events.keys().map(|key| key.0.clone()).collect();
        for id in ids {
            self.update(&id, |event, packets| {
                event.on_player_disconnect(player, packets)
            });
        }
    }
    pub fn drain_packets(&mut self) -> BossPackets {
        std::mem::take(&mut self.packets)
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }
}
