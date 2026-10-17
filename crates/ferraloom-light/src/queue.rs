use ferraloom_util::core::Direction;

/// Port of `LightEngine.QueueEntry`. Bits 0..4 hold light, 4..10 directions.
#[derive(Clone, Copy, Debug)]
pub(crate) struct QueueEntry(u16);

impl QueueEntry {
    const DIRECTIONS: u16 = 1008;
    const EMPTY: u16 = 1024;
    const EMISSION: u16 = 2048;

    pub fn decrease(level: u8) -> Self {
        Self(Self::DIRECTIONS | u16::from(level))
    }
    pub fn decrease_skip(level: u8, skip: Direction) -> Self {
        Self(Self::decrease(level).0 & !(1 << (skip as u8 + 4)))
    }
    pub fn emission(level: u8, empty: bool) -> Self {
        Self(Self::decrease(level).0 | Self::EMISSION | if empty { Self::EMPTY } else { 0 })
    }
    pub fn increase_skip(level: u8, empty: bool, skip: Direction) -> Self {
        Self(Self::decrease_skip(level, skip).0 | if empty { Self::EMPTY } else { 0 })
    }
    pub fn only(level: u8, direction: Direction) -> Self {
        Self(u16::from(level) | 1 << (direction as u8 + 4))
    }
    pub fn sky(directions: [bool; 5]) -> Self {
        let mut entry = Self(15);
        for (enabled, direction) in directions.into_iter().zip([
            Direction::Down,
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
        ]) {
            if enabled {
                entry.0 |= 1 << (direction as u8 + 4);
            }
        }
        entry
    }
    pub fn level(self) -> u8 {
        (self.0 & 15) as u8
    }
    pub fn empty(self) -> bool {
        self.0 & Self::EMPTY != 0
    }
    pub fn is_from_emission(self) -> bool {
        self.0 & Self::EMISSION != 0
    }
    pub fn allows(self, direction: Direction) -> bool {
        self.0 & (1 << (direction as u8 + 4)) != 0
    }
}
