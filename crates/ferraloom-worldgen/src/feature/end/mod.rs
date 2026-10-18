//! End features. Context-dependent block/entity operations use the game bridge.
use super::{Feature, FeatureContext, FeatureLoader};
use crate::{
    Error,
    codec::*,
    generator::is_air,
    placement::predicate::{offset, position},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use std::sync::Arc;

pub fn register(l: &mut FeatureLoader<'_>) -> Result<(), Error> {
    l.register("end_island".parse()?, |_, _| Ok(Arc::new(EndIslandFeature)))?;
    l.register("end_platform".parse()?, |_, _| {
        Ok(Arc::new(EndPlatformFeature))
    })?;
    l.register("end_podium".parse()?, |v, _| {
        Ok(Arc::new(EndPodiumFeature {
            active: optional(v, "active", false, boolean)?,
        }))
    })?;
    l.register("end_gateway".parse()?, |v, _| {
        Ok(Arc::new(EndGatewayFeature {
            exit: v.get("exit").map(|v| position(v, None)).transpose()?,
            exact: boolean(field(v, "exact")?)?,
        }))
    })?;
    l.register("chorus_plant".parse()?, |_, l| {
        Ok(Arc::new(ChorusPlantFeature {
            supports: l
                .data
                .blocks(&serde_json::json!("#minecraft:supports_chorus_plant"))?,
        }))
    })?;
    l.register("end_spike".parse()?, |v, _| {
        Ok(Arc::new(EndSpikeFeature {
            spikes: list(field(v, "spikes")?)?
                .iter()
                .map(|v| {
                    Ok(EndSpike {
                        x: optional(v, "centerX", 0, integer)?,
                        z: optional(v, "centerZ", 0, integer)?,
                        radius: optional(v, "radius", 0, integer)?,
                        height: optional(v, "height", 0, integer)?,
                        guarded: optional(v, "guarded", false, boolean)?,
                    })
                })
                .collect::<Result<_, Error>>()?,
            invulnerable: optional(v, "crystal_invulnerable", false, boolean)?,
            beam: v
                .get("crystal_beam_target")
                .map(|v| position(v, None))
                .transpose()?,
        }))
    })?;
    Ok(())
}
/// Port of EndIslandFeature.
#[derive(Debug)]
struct EndIslandFeature;
impl Feature for EndIslandFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut size = ctx.random.next_int_bound(3) as f32 + 4.0;
        let mut y = 0;
        while size > 0.5 {
            for x in (-size).floor() as i32..=size.ceil() as i32 {
                for z in (-size).floor() as i32..=size.ceil() as i32 {
                    if (x * x + z * z) as f32 <= (size + 1.0) * (size + 1.0) {
                        ctx.level.set_block(
                            offset(ctx.origin, [x, y, z]),
                            Block::END_STONE.default_state(),
                            2,
                        )?;
                    }
                }
            }
            size -= ctx.random.next_int_bound(2) as f32 + 0.5;
            y -= 1;
        }
        Ok(true)
    }
}
/// Port of EndPlatformFeature#createEndPlatform, with dropResources=false.
#[derive(Debug)]
struct EndPlatformFeature;
impl Feature for EndPlatformFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        for z in -2..=2 {
            for x in -2..=2 {
                for y in -1..3 {
                    let block = if y == -1 { Block::OBSIDIAN } else { Block::AIR };
                    let p = offset(ctx.origin, [x, y, z]);
                    if ctx.level.block(p)?.block() != block {
                        ctx.level.set_block(p, block.default_state(), 3)?;
                    }
                }
            }
        }
        Ok(true)
    }
}
/// Port of EndGatewayFeature.
#[derive(Debug)]
struct EndGatewayFeature {
    exit: Option<[i32; 3]>,
    exact: bool,
}
impl Feature for EndGatewayFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        for z in -1i32..=1 {
            for y in -2i32..=2 {
                for x in -1i32..=1 {
                    let p = offset(ctx.origin, [x, y, z]);
                    let block = if x == 0 && y == 0 && z == 0 {
                        Block::END_GATEWAY
                    } else if y == 0 {
                        Block::AIR
                    } else if y.abs() == 2 && x == 0 && z == 0 || y.abs() != 2 && (x == 0 || z == 0)
                    {
                        Block::BEDROCK
                    } else {
                        Block::AIR
                    };
                    ctx.level.set_block(p, block.default_state(), 2)?;
                    if block == Block::END_GATEWAY
                        && let Some(exit) = self.exit
                    {
                        ctx.level.configure_end_gateway(p, exit, self.exact)?;
                    }
                }
            }
        }
        Ok(true)
    }
}
/// Port of ChorusPlantFeature. ChorusFlowerBlock owns plant branching and shapes.
#[derive(Debug)]
struct ChorusPlantFeature {
    supports: Vec<Block>,
}
impl Feature for ChorusPlantFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if is_air(ctx.level.block(ctx.origin)?)
            && self
                .supports
                .contains(&ctx.level.block(offset(ctx.origin, [0, -1, 0]))?.block())
        {
            ctx.level
                .generate_chorus_plant(ctx.origin, &mut ctx.random, 8)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
/// Port of EndPodiumFeature.
#[derive(Debug)]
struct EndPodiumFeature {
    active: bool,
}
impl Feature for EndPodiumFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        for z in -4..=4 {
            for y in -1..=32 {
                for x in -4..=4 {
                    let distance = x * x + y * y + z * z;
                    let inside = (distance as f64) < 6.25;
                    if distance as f64 >= 12.25 {
                        continue;
                    }
                    let block = if y < 0 {
                        if inside {
                            Block::BEDROCK
                        } else {
                            Block::END_STONE
                        }
                    } else if y > 0 {
                        Block::AIR
                    } else if !inside {
                        Block::BEDROCK
                    } else if self.active {
                        Block::END_PORTAL
                    } else {
                        Block::AIR
                    };
                    let p = offset(ctx.origin, [x, y, z]);
                    if self.active && (y > 0 || y < 0 && !inside || y == 0 && inside) {
                        if ctx.level.block(p)?.block() != block {
                            ctx.level.destroy_block(p, true)?;
                            ctx.level.set_block(p, block.default_state(), 2)?;
                        }
                    } else {
                        ctx.level.set_block(p, block.default_state(), 2)?;
                    }
                }
            }
        }
        for y in 0..4 {
            ctx.level.set_block(
                offset(ctx.origin, [0, y, 0]),
                Block::BEDROCK.default_state(),
                2,
            )?;
        }
        for (delta, facing) in [
            ([0, 2, -1], "north"),
            ([1, 2, 0], "east"),
            ([0, 2, 1], "south"),
            ([-1, 2, 0], "west"),
        ] {
            let state =
                BlockState::from_properties(Block::WALL_TORCH, [("facing", facing)]).unwrap();
            ctx.level.set_block(offset(ctx.origin, delta), state, 2)?;
        }
        Ok(true)
    }
}
/// Port of EndSpikeFeature.EndSpike.
#[derive(Clone, Debug)]
struct EndSpike {
    x: i32,
    z: i32,
    radius: i32,
    height: i32,
    guarded: bool,
}
/// Port of EndSpikeFeature.
#[derive(Debug)]
struct EndSpikeFeature {
    spikes: Vec<EndSpike>,
    invulnerable: bool,
    beam: Option<[i32; 3]>,
}
impl EndSpikeFeature {
    /// Port of EndSpikeFeature#getSpikesForLevel and SpikeCacheLoader#load.
    fn for_seed(seed: i64) -> Vec<EndSpike> {
        let key = LegacyRandomSource::new(seed).next_long() & 65535;
        let mut random = LegacyRandomSource::new(key);
        let mut sizes: Vec<_> = (0..10).collect();
        for i in (2..=10).rev() {
            let j = random.next_int_bound(i) as usize;
            sizes.swap(i as usize - 1, j);
        }
        sizes
            .into_iter()
            .enumerate()
            .map(|(i, size)| {
                let angle = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 10.0 * i as f64);
                EndSpike {
                    x: (42.0 * angle.cos()).floor() as i32,
                    z: (42.0 * angle.sin()).floor() as i32,
                    radius: 2 + size / 3,
                    height: 76 + size * 3,
                    guarded: size == 1 || size == 2,
                }
            })
            .collect()
    }
    fn place_spike(&self, ctx: &mut FeatureContext<'_, '_>, spike: &EndSpike) -> Result<(), Error> {
        let r = spike.radius;
        // betweenClosed(BlockPos, BlockPos) normalizes both corners, including
        // negative radii and heights accepted by EndSpike's unrestricted codec.
        let a = [
            spike.x.wrapping_sub(r),
            ctx.level.min_y(),
            spike.z.wrapping_sub(r),
        ];
        let b = [
            spike.x.wrapping_add(r),
            spike.height.wrapping_add(10),
            spike.z.wrapping_add(r),
        ];
        let radius_squared = f64::from(r.wrapping_mul(r).wrapping_add(1));
        for z in a[2].min(b[2])..=a[2].max(b[2]) {
            for y in a[1].min(b[1])..=a[1].max(b[1]) {
                for x in a[0].min(b[0])..=a[0].max(b[0]) {
                    let dx = f64::from(x) - f64::from(spike.x);
                    let dz = f64::from(z) - f64::from(spike.z);
                    if dx * dx + dz * dz <= radius_squared && y < spike.height {
                        ctx.level
                            .set_block([x, y, z], Block::OBSIDIAN.default_state(), 2)?;
                    } else if y > 65 {
                        ctx.level
                            .set_block([x, y, z], Block::AIR.default_state(), 2)?;
                    }
                }
            }
        }
        if spike.guarded {
            for x in -2i32..=2 {
                for z in -2i32..=2 {
                    for y in 0..=3 {
                        let top = y == 3;
                        if x.abs() == 2 || z.abs() == 2 || top {
                            let xe = x.abs() == 2 || top;
                            let ze = z.abs() == 2 || top;
                            let state = BlockState::from_properties(
                                Block::IRON_BARS,
                                [
                                    ("north", if xe && z != -2 { "true" } else { "false" }),
                                    ("south", if xe && z != 2 { "true" } else { "false" }),
                                    ("west", if ze && x != -2 { "true" } else { "false" }),
                                    ("east", if ze && x != 2 { "true" } else { "false" }),
                                ],
                            )
                            .unwrap();
                            ctx.level.set_block(
                                [
                                    spike.x.wrapping_add(x),
                                    spike.height.wrapping_add(y),
                                    spike.z.wrapping_add(z),
                                ],
                                state,
                                2,
                            )?;
                        }
                    }
                }
            }
        }
        ctx.level.create_end_spike_crystal(
            [spike.x, spike.height.wrapping_add(1), spike.z],
            self.beam,
            self.invulnerable,
            &mut ctx.random,
        )
    }
}
impl Feature for EndSpikeFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let generated;
        let spikes = if self.spikes.is_empty() {
            generated = Self::for_seed(ctx.level.seed());
            &generated
        } else {
            &self.spikes
        };
        for spike in spikes {
            if ctx.origin[0] >> 4 == spike.x >> 4 && ctx.origin[2] >> 4 == spike.z >> 4 {
                self.place_spike(ctx, spike)?;
            }
        }
        Ok(true)
    }
}
