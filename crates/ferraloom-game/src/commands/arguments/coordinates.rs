//! Port of net.minecraft.commands.arguments.coordinates.
use crate::commands::{
    exceptions::error_at,
    source::{CommandSourceStack, Rotation},
};
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_util::{Vec3, mth};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCoordinate {
    pub relative: bool,
    pub value: f64,
}
impl WorldCoordinate {
    pub const RELATIVE_ZERO: Self = Self {
        relative: true,
        value: 0.,
    };
    pub fn get(self, original: f64) -> f64 {
        self.value + if self.relative { original } else { 0. }
    }
    pub fn parse(
        reader: &mut StringReader,
        integer: bool,
        center: bool,
    ) -> Result<Self, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == 94 {
            return Err(error_at(reader, "argument.pos.mixed", &[]));
        }
        if !reader.can_read() {
            return Err(error_at(
                reader,
                if integer {
                    "argument.pos.missing.int"
                } else {
                    "argument.pos.missing.double"
                },
                &[],
            ));
        }
        let relative = reader.peek() == 126;
        if relative {
            reader.skip();
        }
        let start = reader.cursor();
        let mut value = if reader.can_read() && reader.peek() != 32 {
            if integer && !relative {
                f64::from(reader.read_int()?)
            } else {
                reader.read_double()?
            }
        } else {
            0.
        };
        if !integer && center && !relative && !reader.slice(start, reader.cursor()).contains('.') {
            value += 0.5;
        }
        Ok(Self { relative, value })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Coordinates {
    World([WorldCoordinate; 3]),
    Local { left: f64, up: f64, forwards: f64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinateKind {
    BlockPos,
    ColumnPos,
    Vec3,
    Vec2,
    Rotation,
}
impl Coordinates {
    pub fn position(self, source: &CommandSourceStack, eye_height: Option<f64>) -> Vec3 {
        match self {
            Self::World([x, y, z]) => Vec3::new(
                x.get(source.position.x),
                y.get(source.position.y),
                z.get(source.position.z),
            ),
            Self::Local { left, up, forwards } => {
                let origin = source.anchor_position(eye_height);
                let rotation = source.rotation;
                let yc = mth::cos(f64::from((rotation.yaw + 90.) * 0.017453292));
                let ys = mth::sin(f64::from((rotation.yaw + 90.) * 0.017453292));
                let xc = mth::cos(f64::from(-rotation.pitch * 0.017453292));
                let xs = mth::sin(f64::from(-rotation.pitch * 0.017453292));
                let xcu = mth::cos(f64::from((-rotation.pitch + 90.) * 0.017453292));
                let xsu = mth::sin(f64::from((-rotation.pitch + 90.) * 0.017453292));
                let f = Vec3::new(f64::from(yc * xc), f64::from(xs), f64::from(ys * xc));
                let u = Vec3::new(f64::from(yc * xcu), f64::from(xsu), f64::from(ys * xcu));
                let l = Vec3::new(
                    -(f.y * u.z - f.z * u.y),
                    -(f.z * u.x - f.x * u.z),
                    -(f.x * u.y - f.y * u.x),
                );
                Vec3::new(
                    origin.x + f.x * forwards + u.x * up + l.x * left,
                    origin.y + f.y * forwards + u.y * up + l.y * left,
                    origin.z + f.z * forwards + u.z * up + l.z * left,
                )
            }
        }
    }
    pub fn rotation(self, source: &CommandSourceStack) -> Rotation {
        match self {
            Self::World([x, y, _]) => Rotation {
                pitch: x.get(f64::from(source.rotation.pitch)) as f32,
                yaw: y.get(f64::from(source.rotation.yaw)) as f32,
            },
            Self::Local { .. } => Rotation::default(),
        }
    }
    pub fn parse(
        reader: &mut StringReader,
        kind: CoordinateKind,
    ) -> Result<Self, CommandSyntaxException> {
        let start = reader.cursor();
        let two = matches!(
            kind,
            CoordinateKind::ColumnPos | CoordinateKind::Vec2 | CoordinateKind::Rotation
        );
        let incomplete = match kind {
            CoordinateKind::ColumnPos | CoordinateKind::Vec2 => "argument.pos2d.incomplete",
            CoordinateKind::Rotation => "argument.rotation.incomplete",
            _ => "argument.pos3d.incomplete",
        };
        if two && !reader.can_read() {
            return Err(error_at(reader, incomplete, &[]));
        }
        let local = !two && reader.can_read() && reader.peek() == 94;
        let mut result = [WorldCoordinate::RELATIVE_ZERO; 3];
        let count = if two { 2 } else { 3 };
        for (i, coordinate) in result.iter_mut().take(count).enumerate() {
            if i > 0 {
                if !reader.can_read() || reader.peek() != 32 {
                    reader.set_cursor(start);
                    return Err(error_at(reader, incomplete, &[]));
                }
                reader.skip();
            }
            *coordinate = if local {
                if !reader.can_read() {
                    return Err(error_at(reader, "argument.pos.missing.double", &[]));
                }
                if reader.peek() != 94 {
                    reader.set_cursor(start);
                    return Err(error_at(reader, "argument.pos.mixed", &[]));
                }
                reader.skip();
                WorldCoordinate {
                    relative: true,
                    value: if reader.can_read() && reader.peek() != 32 {
                        reader.read_double()?
                    } else {
                        0.
                    },
                }
            } else {
                WorldCoordinate::parse(
                    reader,
                    matches!(kind, CoordinateKind::BlockPos | CoordinateKind::ColumnPos),
                    matches!(kind, CoordinateKind::Vec2) || kind == CoordinateKind::Vec3 && i != 1,
                )?
            };
        }
        if local {
            return Ok(Self::Local {
                left: result[0].value,
                up: result[1].value,
                forwards: result[2].value,
            });
        }
        if two {
            match kind {
                CoordinateKind::Rotation => result.swap(0, 1),
                _ => result.swap(1, 2),
            }
        }
        Ok(Self::World(result))
    }
}
