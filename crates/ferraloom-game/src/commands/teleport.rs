//! Port of TeleportCommand#getRelatives and #performTeleport's player movement values.
use super::{
    CommandSourceStack, arguments::coordinates::Coordinates, exceptions::error, source::Rotation,
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_protocol::play::PositionMoveRotation;
use ferraloom_util::{Vec3, mth};

#[derive(Clone, Debug)]
pub struct TeleportPlan {
    pub position: Vec3,
    pub rotation: Rotation,
    pub change: PositionMoveRotation,
    pub relatives: i32,
}
impl TeleportPlan {
    pub fn to_position(
        source: &CommandSourceStack,
        coordinates: Coordinates,
        rotation: Option<Coordinates>,
        current_position: Vec3,
        current_rotation: Rotation,
    ) -> Result<Self, CommandSyntaxException> {
        let relative_position = relative_axes(coordinates);
        let relative_rotation = rotation.map_or([true, true], |rotation| {
            let [pitch, yaw, _] = relative_axes(rotation);
            [pitch, yaw]
        });
        let mut relatives = 0;
        for (axis, relative) in relative_position.into_iter().enumerate() {
            if relative {
                relatives |= 1 << axis | 1 << (axis + 5);
            }
        }
        if relative_rotation[0] {
            relatives |= 1 << 4;
        }
        if relative_rotation[1] {
            relatives |= 1 << 3;
        }
        Self::new(
            coordinates.position(source, None),
            rotation.map_or(current_rotation, |rotation| rotation.rotation(source)),
            current_position,
            current_rotation,
            relatives,
        )
    }
    pub fn to_entity(
        position: Vec3,
        rotation: Rotation,
        current_position: Vec3,
        current_rotation: Rotation,
    ) -> Result<Self, CommandSyntaxException> {
        Self::new(position, rotation, current_position, current_rotation, 0)
    }
    fn new(
        position: Vec3,
        rotation: Rotation,
        current_position: Vec3,
        current_rotation: Rotation,
        relatives: i32,
    ) -> Result<Self, CommandSyntaxException> {
        let block = [
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        ];
        if !(-30_000_000..30_000_000).contains(&block[0])
            || !(-20_000_000..20_000_000).contains(&block[1])
            || !(-30_000_000..30_000_000).contains(&block[2])
        {
            return Err(error("commands.teleport.invalidPosition", &[]));
        }
        let yaw_relative = relatives & 8 != 0;
        let pitch_relative = relatives & 16 != 0;
        let yaw = mth::wrap_degrees_f32(
            rotation.yaw
                - if yaw_relative {
                    current_rotation.yaw
                } else {
                    0.
                },
        );
        let pitch = mth::wrap_degrees_f32(
            rotation.pitch
                - if pitch_relative {
                    current_rotation.pitch
                } else {
                    0.
                },
        );
        Ok(Self {
            position,
            rotation: Rotation {
                yaw: yaw
                    + if yaw_relative {
                        current_rotation.yaw
                    } else {
                        0.
                    },
                pitch: (pitch
                    + if pitch_relative {
                        current_rotation.pitch
                    } else {
                        0.
                    })
                .clamp(-90., 90.),
            },
            change: PositionMoveRotation {
                position: ferraloom_protocol::play::Vec3 {
                    x: position.x
                        - if relatives & 1 != 0 {
                            current_position.x
                        } else {
                            0.
                        },
                    y: position.y
                        - if relatives & 2 != 0 {
                            current_position.y
                        } else {
                            0.
                        },
                    z: position.z
                        - if relatives & 4 != 0 {
                            current_position.z
                        } else {
                            0.
                        },
                },
                delta_movement: ferraloom_protocol::play::Vec3 {
                    x: 0.,
                    y: 0.,
                    z: 0.,
                },
                y_rot: yaw,
                x_rot: pitch,
            },
            relatives,
        })
    }
}
fn relative_axes(coordinates: Coordinates) -> [bool; 3] {
    match coordinates {
        Coordinates::World(values) => values.map(|coordinate| coordinate.relative),
        Coordinates::Local { .. } => [true; 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{
        arguments::coordinates::CoordinateKind, exceptions::BrigadierExceptions,
    };
    use ferraloom_brigadier::StringReader;
    use std::sync::Arc;
    #[derive(serde::Deserialize)]
    struct Vector {
        input: String,
        rotation: String,
        position: Option<[f64; 3]>,
        change: Option<[f64; 5]>,
        absolute_rotation: Option<[f32; 2]>,
        relatives: Option<i32>,
        error: Option<String>,
    }
    #[test]
    fn relative_flags_wrapped_rotations_and_bounds_match_java() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/teleport-vectors.json")).unwrap();
        let source =
            CommandSourceStack::server(4, Vec3::new(80., 64., -16.)).with_rotation(Rotation {
                pitch: -60.,
                yaw: 35.,
            });
        for vector in vectors {
            let coordinates =
                Coordinates::parse(&mut StringReader::new(&vector.input), CoordinateKind::Vec3)
                    .unwrap();
            let rotation = (!vector.rotation.is_empty()).then(|| {
                Coordinates::parse(
                    &mut StringReader::with_provider(
                        &vector.rotation,
                        Arc::new(BrigadierExceptions),
                    ),
                    CoordinateKind::Rotation,
                )
                .unwrap()
            });
            let plan = TeleportPlan::to_position(
                &source,
                coordinates,
                rotation,
                Vec3::new(1., 2., 3.),
                Rotation {
                    pitch: 30.,
                    yaw: 370.,
                },
            );
            if let Some(expected) = vector.error {
                assert_eq!(
                    plan.unwrap_err().raw_message.text(),
                    expected,
                    "{} {:?}",
                    vector.input,
                    vector.rotation
                );
            } else {
                let plan = plan.unwrap();
                assert_eq!(
                    [plan.position.x, plan.position.y, plan.position.z],
                    vector.position.unwrap(),
                    "{} {:?}",
                    vector.input,
                    vector.rotation
                );
                assert_eq!(
                    [
                        plan.change.position.x,
                        plan.change.position.y,
                        plan.change.position.z,
                        f64::from(plan.change.y_rot),
                        f64::from(plan.change.x_rot)
                    ],
                    vector.change.unwrap(),
                    "{} {:?}",
                    vector.input,
                    vector.rotation
                );
                assert_eq!(
                    [plan.rotation.yaw, plan.rotation.pitch],
                    vector.absolute_rotation.unwrap()
                );
                assert_eq!(plan.relatives, vector.relatives.unwrap());
            }
        }
    }
}
