impl crate::registries::game_event::Id {
    /// Port of GameEvent#notificationRadius, from immutable native registry facts.
    pub const fn notification_radius(self) -> i32 {
        crate::GAME_EVENT_NOTIFICATION_RADII[self.protocol_id() as usize]
    }
}
