use super::*;
use ferraloom_data::registries::attribute::Id as Attribute;
use ferraloom_protocol::{
    codec::Either,
    play::{RgbColor, TeamColor, TrackedWaypointData, WaypointContents},
};
use ferraloom_util::{BlockPos, ChunkPos, mth};

#[derive(Clone, Copy, Debug)]
enum Position {
    Block(BlockPos),
    Chunk(ChunkPos),
    Azimuth(f32),
}
/// Port of WaypointTransmitter's three Entity connections. Cached position and
/// icon belong to the connection, while every eligibility read uses live state.
#[derive(Clone, Debug)]
pub(super) struct Connection {
    source: EntityRef,
    receiver: EntityRef,
    icon: WaypointIcon,
    position: Position,
}
fn distance(source: EntityRef, receiver: EntityRef, level: &Level) -> Result<f32, LevelError> {
    let delta = level.entity::<EntityData>(source)?.position
        - level.entity::<EntityData>(receiver)?.position;
    let (x, y, z) = (delta.x as f32, delta.y as f32, delta.z as f32);
    Ok(mth::sqrt(x * x + y * y + z * z))
}
fn ignores(source: EntityRef, receiver: EntityRef, level: &Level) -> Result<bool, LevelError> {
    if level
        .entity_behavior(receiver)?
        .is_spectator(receiver, level)?
    {
        return Ok(false);
    }
    if level.entity_behavior(source)?.is_spectator(source, level)? {
        return Ok(true);
    }
    let mut passengers = level.entity::<EntityData>(source)?.passengers.to_vec();
    while let Some(passenger) = passengers.pop() {
        if passenger == receiver {
            return Ok(true);
        }
        passengers.extend_from_slice(&level.entity::<EntityData>(passenger)?.passengers);
    }
    let range =
        crate::entity::living::attribute(source, Attribute::WAYPOINT_TRANSMIT_RANGE, level)?.min(
            crate::entity::living::attribute(receiver, Attribute::WAYPOINT_RECEIVE_RANGE, level)?,
        );
    Ok(f64::from(distance(source, receiver, level)?) >= range)
}
fn chunk_visible(
    position: ChunkPos,
    receiver: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    let receiver = level.entity::<ServerPlayer>(receiver)?;
    // isInViewDistance excludes the neighboring-chunk send buffer. It does not
    // consult chunkSender pending state, unlike entity pairing.
    let center = receiver.chunk_tracking_center;
    let Some(distance) = receiver.chunk_tracking_distance else {
        return Ok(false);
    };
    Ok(center.is_some_and(|center| {
        let x = (i64::from(position.x) - i64::from(center.x))
            .abs()
            .saturating_sub(1);
        let z = (i64::from(position.z) - i64::from(center.z))
            .abs()
            .saturating_sub(1);
        x * x + z * z < i64::from(distance) * i64::from(distance)
    }))
}
fn azimuth(source: EntityRef, receiver: EntityRef, level: &Level) -> Result<f32, LevelError> {
    let delta = level.entity::<EntityData>(receiver)?.position
        - level.entity::<EntityData>(source)?.position;
    Ok(mth::atan2(delta.x, -delta.z) as f32)
}
impl Connection {
    pub(super) fn make(
        source: EntityRef,
        receiver: EntityRef,
        level: &Level,
    ) -> Result<Option<Self>, LevelError> {
        if level.entity::<EntityData>(source)?.first_tick
            || source == receiver
            || ignores(source, receiver, level)?
        {
            return Ok(None);
        }
        let mut icon = level
            .entity::<LivingEntity>(source)?
            .locator_bar_icon
            .clone();
        if icon.color.is_none() {
            let name = level.entity_scoreboard_name(source)?;
            let team_color = level
                .shared()?
                .scoreboard
                .scoreboard()
                .get_players_team(name.as_ref())
                .and_then(|team| team.color);
            icon.color = team_color.map(|color| {
                RgbColor(if color == TeamColor::Black {
                    0x303030
                } else {
                    ferraloom_protocol::chat::TextColor::parse(
                        crate::commands::arguments::scoreboard::COLORS[color as usize],
                    )
                    .expect("native team color")
                    .value()
                })
            });
        }
        let position = if distance(source, receiver, level)? > 332.0_f32 {
            Position::Azimuth(azimuth(source, receiver, level)?)
        } else {
            let position = level.entity::<EntityData>(source)?;
            if chunk_visible(position.chunk_position, receiver, level)? {
                Position::Block(position.block_position)
            } else {
                Position::Chunk(position.chunk_position)
            }
        };
        Ok(Some(Self {
            source,
            receiver,
            icon,
            position,
        }))
    }
    pub(super) fn is_broken(&self, level: &Level) -> Result<bool, LevelError> {
        if ignores(self.source, self.receiver, level)? {
            return Ok(true);
        }
        let state = level.entity::<EntityData>(self.source)?;
        Ok(match self.position {
            Position::Block(previous) => {
                previous
                    .x
                    .wrapping_sub(state.block_position.x)
                    .wrapping_abs()
                    .wrapping_add(
                        previous
                            .y
                            .wrapping_sub(state.block_position.y)
                            .wrapping_abs(),
                    )
                    .wrapping_add(
                        previous
                            .z
                            .wrapping_sub(state.block_position.z)
                            .wrapping_abs(),
                    )
                    > 1
            }
            Position::Chunk(previous) => {
                previous
                    .x
                    .wrapping_sub(state.chunk_position.x)
                    .wrapping_abs()
                    .max(
                        previous
                            .z
                            .wrapping_sub(state.chunk_position.z)
                            .wrapping_abs(),
                    )
                    > 1
                    || chunk_visible(previous, self.receiver, level)?
            }
            Position::Azimuth(_) => {
                chunk_visible(state.chunk_position, self.receiver, level)?
                    || distance(self.source, self.receiver, level)? <= 332.0_f32
            }
        })
    }
    pub(super) fn update(&self, level: &Level) -> Result<Option<Self>, LevelError> {
        let state = level.entity::<EntityData>(self.source)?;
        let position = match self.position {
            Position::Block(previous) if previous != state.block_position => {
                Position::Block(state.block_position)
            }
            Position::Chunk(previous) if previous != state.chunk_position => {
                Position::Chunk(state.chunk_position)
            }
            Position::Azimuth(previous) => {
                let current = azimuth(self.source, self.receiver, level)?;
                if (current - previous).abs() > 0.008726646_f32 {
                    Position::Azimuth(current)
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(Self {
            position,
            ..self.clone()
        }))
    }
    pub(super) fn packet(
        &self,
        operation: WaypointOperation,
        level: &Level,
    ) -> Result<cb::TrackedWaypoint, LevelError> {
        let contents = if operation == WaypointOperation::Untrack {
            WaypointContents::Empty {}
        } else {
            match self.position {
                Position::Block(pos) => WaypointContents::Vec3i {
                    position: ferraloom_protocol::play::Vec3i {
                        x: pos.x.into(),
                        y: pos.y.into(),
                        z: pos.z.into(),
                    },
                },
                Position::Chunk(pos) => WaypointContents::Chunk {
                    x: pos.x.into(),
                    z: pos.z.into(),
                },
                Position::Azimuth(angle) => WaypointContents::Azimuth { angle },
            }
        };
        Ok(cb::TrackedWaypoint {
            operation,
            waypoint: TrackedWaypointData {
                identifier: Either::Left(level.entity::<EntityData>(self.source)?.uuid),
                icon: if operation == WaypointOperation::Untrack {
                    super::default_icon()
                } else {
                    self.icon.clone()
                },
                contents,
            },
        })
    }
}
