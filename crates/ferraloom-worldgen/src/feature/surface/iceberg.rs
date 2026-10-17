//! Port of IcebergFeature. Keep Java float intermediates and random call order.
use super::*;
use crate::region::WorldGenRegion;
use ferraloom_util::random::WorldgenRandom;

#[derive(Debug)]
pub(super) struct IcebergFeature {
    pub state: BlockState,
}
#[derive(Clone, Copy)]
struct Shape {
    origin: [i32; 3],
    height: i32,
    width: i32,
    ellipse: bool,
    a: i32,
    c: i32,
    angle: f64,
    snow: bool,
    state: BlockState,
}
impl Feature for IcebergFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let origin = [ctx.origin[0], ctx.generator.sea_level(), ctx.origin[2]];
        let r = &mut *ctx.random;
        let snow = r.next_double() > 0.7;
        let angle = r.next_double() * 2.0 * std::f64::consts::PI;
        let ellipse_a = 11 - r.next_int_bound(5);
        let c = 3 + r.next_int_bound(3);
        let ellipse = r.next_double() > 0.7;
        let mut height = if ellipse {
            r.next_int_bound(6) + 6
        } else {
            r.next_int_bound(15) + 3
        };
        if !ellipse && r.next_double() > 0.9 {
            height += r.next_int_bound(19) + 7;
        }
        let depth = (height + r.next_int_bound(11)).min(18);
        let width = (height + r.next_int_bound(7) - r.next_int_bound(5)).min(11);
        let a = if ellipse { ellipse_a } else { 11 };
        let shape = Shape {
            origin,
            height,
            width,
            ellipse,
            a,
            c,
            angle,
            snow,
            state: self.state,
        };
        for x in -a..a {
            for z in -a..a {
                for y in 0..height {
                    let radius = if ellipse {
                        radius_ellipse(y, height, width)
                    } else {
                        radius_round(r, y, height, width)
                    };
                    if ellipse || x < radius {
                        shape.generate(ctx.region, r, [x, y, z], radius)?;
                    }
                }
            }
        }
        shape.smooth(ctx.region, ellipse_a)?;
        for x in -a..a {
            for z in -a..a {
                for y in (-depth + 1..=-1).rev() {
                    let new_a = if ellipse {
                        (a as f32 * (1.0 - (f64::from(y).powi(2) as f32) / (depth as f32 * 8.0)))
                            .ceil() as i32
                    } else {
                        a
                    };
                    let radius = radius_steep(r, -y, depth, width);
                    if x < radius {
                        Shape {
                            height: depth,
                            a: new_a,
                            ..shape
                        }
                        .generate(ctx.region, r, [x, y, z], radius)?;
                    }
                }
            }
        }
        let cut = if ellipse {
            r.next_double() > 0.1
        } else {
            r.next_double() > 0.7
        };
        if cut {
            shape.cut_out(ctx.region, r, ellipse_a)?;
        }
        Ok(true)
    }
}
impl Shape {
    fn generate(
        self,
        level: &mut WorldGenRegion<'_>,
        r: &mut WorldgenRandom,
        [x, y, z]: [i32; 3],
        radius: i32,
    ) -> Result<(), Error> {
        let distance = if self.ellipse {
            distance_ellipse(
                x,
                z,
                [0; 3],
                self.a,
                self.c
                    - if y > 0 && self.height - y <= 3 {
                        4 - (self.height - y)
                    } else {
                        0
                    },
                self.angle,
            )
        } else {
            let off = 10.0 * r.next_float().clamp(0.2, 0.8) / radius as f32;
            f64::from(off) + f64::from(x).powi(2) + f64::from(z).powi(2) - f64::from(radius).powi(2)
        };
        if distance < 0.0 {
            let p = offset(self.origin, [x, y, z]);
            let compare = if self.ellipse {
                -0.5
            } else {
                f64::from(-6 - r.next_int_bound(3))
            };
            if distance > compare && r.next_double() > 0.9 {
                return Ok(());
            }
            let state = level.block(p)?;
            if state.is_air()
                || matches!(state.block(), Block::SNOW_BLOCK | Block::ICE | Block::WATER)
            {
                let randomness = !self.ellipse || r.next_double() > 0.05;
                let divisor = if self.ellipse { 3 } else { 2 };
                let snow = self.snow
                    && state.block() != Block::WATER
                    && f64::from(self.height - y)
                        <= f64::from(r.next_int_bound((self.height / divisor).max(1)))
                            + f64::from(self.height) * 0.6
                    && randomness;
                level.set_block(
                    p,
                    if snow {
                        Block::SNOW_BLOCK.default_state()
                    } else {
                        self.state
                    },
                    3,
                )?;
            }
        }
        Ok(())
    }
    fn smooth(self, level: &mut WorldGenRegion<'_>, ellipse_a: i32) -> Result<(), Error> {
        let a = if self.ellipse {
            ellipse_a
        } else {
            self.width / 2
        };
        for x in -a..=a {
            for z in -a..=a {
                for y in 0..=self.height {
                    let p = offset(self.origin, [x, y, z]);
                    let state = level.block(p)?;
                    if iceberg_state(state) || state.block() == Block::SNOW {
                        if level.block(relative(p, Direction::Down))?.is_air() {
                            level.set_block(p, Block::AIR.default_state(), 3)?;
                            level.set_block(
                                relative(p, Direction::Up),
                                Block::AIR.default_state(),
                                3,
                            )?;
                        } else if iceberg_state(state) {
                            let mut count = 0;
                            for d in [
                                Direction::West,
                                Direction::East,
                                Direction::North,
                                Direction::South,
                            ] {
                                if !iceberg_state(level.block(relative(p, d))?) {
                                    count += 1;
                                }
                            }
                            if count >= 3 {
                                level.set_block(p, Block::AIR.default_state(), 3)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn cut_out(
        self,
        level: &mut WorldGenRegion<'_>,
        r: &mut WorldgenRandom,
        ellipse_a: i32,
    ) -> Result<(), Error> {
        let sign_x = if r.next_boolean() { -1 } else { 1 };
        let sign_z = if r.next_boolean() { -1 } else { 1 };
        let mut x = r.next_int_bound((self.width / 2 - 2).max(1));
        if r.next_boolean() {
            x = self.width / 2 + 1 - r.next_int_bound((self.width - self.width / 2 - 1).max(1));
        }
        let mut z = r.next_int_bound((self.width / 2 - 2).max(1));
        if r.next_boolean() {
            z = self.width / 2 + 1 - r.next_int_bound((self.width - self.width / 2 - 1).max(1));
        }
        if self.ellipse {
            z = r.next_int_bound((ellipse_a - 5).max(1));
            x = z;
        }
        let local = [sign_x * x, 0, sign_z * z];
        let angle = if self.ellipse {
            self.angle + std::f64::consts::FRAC_PI_2
        } else {
            r.next_double() * 2.0 * std::f64::consts::PI
        };
        for y in 0..self.height - 3 {
            let radius = radius_round(r, y, self.height, self.width);
            self.carve(level, radius, y, false, angle, local, ellipse_a)?;
        }
        let mut y = -1;
        // The loop condition itself consumes RNG, including the terminating check.
        while y > -self.height + r.next_int_bound(5) {
            let radius = radius_steep(r, -y, self.height, self.width);
            self.carve(level, radius, y, true, angle, local, ellipse_a)?;
            y -= 1;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn carve(
        self,
        level: &mut WorldGenRegion<'_>,
        radius: i32,
        y: i32,
        water: bool,
        angle: f64,
        local: [i32; 3],
        ellipse_a: i32,
    ) -> Result<(), Error> {
        let a = radius + 1 + ellipse_a / 3;
        let c = (radius - 3).min(3) + self.c / 2 - 1;
        for x in -a..a {
            for z in -a..a {
                if distance_ellipse(x, z, local, a, c, angle) < 0.0 {
                    let p = offset(self.origin, [x, y, z]);
                    let state = level.block(p)?;
                    if iceberg_state(state) || state.block() == Block::SNOW_BLOCK {
                        level.set_block(
                            p,
                            if water {
                                Block::WATER.default_state()
                            } else {
                                Block::AIR.default_state()
                            },
                            3,
                        )?;
                        if !water && level.block(relative(p, Direction::Up))?.block() == Block::SNOW
                        {
                            level.set_block(
                                relative(p, Direction::Up),
                                Block::AIR.default_state(),
                                3,
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
fn iceberg_state(state: BlockState) -> bool {
    matches!(
        state.block(),
        Block::PACKED_ICE | Block::SNOW_BLOCK | Block::BLUE_ICE
    )
}
fn distance_ellipse(x: i32, z: i32, origin: [i32; 3], a: i32, c: i32, angle: f64) -> f64 {
    ((f64::from(x - origin[0]) * angle.cos() - f64::from(z - origin[2]) * angle.sin())
        / f64::from(a))
    .powi(2)
        + ((f64::from(x - origin[0]) * angle.sin() + f64::from(z - origin[2]) * angle.cos())
            / f64::from(c))
        .powi(2)
        - 1.0
}
fn radius_round(r: &mut WorldgenRandom, y: i32, height: i32, width: i32) -> i32 {
    let k = 3.5 - r.next_float();
    let mut scale = (1.0 - f64::from(y).powi(2) as f32 / (height as f32 * k)) * width as f32;
    if height > 15 + r.next_int_bound(5) {
        let y = if y < 3 + r.next_int_bound(6) {
            y / 2
        } else {
            y
        };
        scale = (1.0 - y as f32 / (height as f32 * k * 0.4)) * width as f32;
    }
    (scale / 2.0).ceil() as i32
}
fn radius_ellipse(y: i32, height: i32, width: i32) -> i32 {
    (((1.0 - f64::from(y).powi(2) as f32 / height as f32) * width as f32) / 2.0).ceil() as i32
}
fn radius_steep(r: &mut WorldgenRandom, y: i32, height: i32, width: i32) -> i32 {
    let k = 1.0 + r.next_float() / 2.0;
    (((1.0 - y as f32 / (height as f32 * k)) * width as f32) / 2.0).ceil() as i32
}
