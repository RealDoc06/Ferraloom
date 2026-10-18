use super::*;
use ferraloom_data::{
    EntityType,
    properties::{self, Facing},
};

/// Port of MonsterRoomFeature. Chest/spawner initialization calls the shared
/// block-entity bridge, so an unported game implementation fails explicitly.
#[derive(Debug)]
struct MonsterRoomFeature {
    protected: Vec<Block>,
}
pub(super) fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("monster_room".parse()?, |_, l| {
        Ok(Arc::new(MonsterRoomFeature {
            protected: l
                .data
                .blocks(&Value::String("#minecraft:features_cannot_replace".into()))?,
        }))
    })
}
impl MonsterRoomFeature {
    fn safe_set(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        p: [i32; 3],
        state: BlockState,
    ) -> Result<(), Error> {
        if !self.protected.contains(&ctx.level.block(p)?.block()) {
            ctx.level.set_block(p, state, 2)?;
        }
        Ok(())
    }
    /// Port of StructurePiece#reorient; solid render is a cached state fact.
    fn chest_state(&self, ctx: &FeatureContext<'_, '_>, p: [i32; 3]) -> Result<BlockState, Error> {
        let state = Block::CHEST.default_state();
        let mut solid = None;
        for d in HORIZONTAL {
            let old = ctx.level.block(move_direction(p, d))?;
            if old.block() == Block::CHEST {
                return Ok(state);
            }
            if old.data().solid_render {
                if solid.is_some() {
                    solid = None;
                    break;
                }
                solid = Some(d);
            }
        }
        let direction = if let Some(d) = solid {
            d.opposite()
        } else {
            let mut d = Direction::North;
            if ctx.level.block(move_direction(p, d))?.data().solid_render {
                d = d.opposite();
            }
            if ctx.level.block(move_direction(p, d))?.data().solid_render {
                d = d.clockwise();
            }
            if ctx.level.block(move_direction(p, d))?.data().solid_render {
                d = d.opposite();
            }
            d
        };
        let facing = match direction {
            Direction::North => Facing::North,
            Direction::East => Facing::East,
            Direction::South => Facing::South,
            Direction::West => Facing::West,
            _ => unreachable!(),
        };
        Ok(state.with(properties::FACING, facing).unwrap())
    }
}
impl Feature for MonsterRoomFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let xr = ctx.random.next_int_bound(2) + 2;
        let zr = ctx.random.next_int_bound(2) + 2;
        let mut holes = 0;
        for dx in -xr - 1..=xr + 1 {
            for dy in -1..=4 {
                for dz in -zr - 1..=zr + 1 {
                    let p = offset(ctx.origin, [dx, dy, dz]);
                    let solid = ctx.level.is_solid(p)?;
                    if (dy == -1 || dy == 4) && !solid {
                        return Ok(false);
                    }
                    if (dx == -xr - 1 || dx == xr + 1 || dz == -zr - 1 || dz == zr + 1)
                        && dy == 0
                        && is_air(ctx.level.block(p)?)
                        && is_air(ctx.level.block(move_direction(p, Direction::Up))?)
                    {
                        holes += 1;
                    }
                }
            }
        }
        if !(1..=5).contains(&holes) {
            return Ok(false);
        }
        for dx in -xr - 1..=xr + 1 {
            for dy in (-1..=3).rev() {
                for dz in -zr - 1..=zr + 1 {
                    let p = offset(ctx.origin, [dx, dy, dz]);
                    let old = ctx.level.block(p)?;
                    if dx == -xr - 1 || dy == -1 || dz == -zr - 1 || dx == xr + 1 || dz == zr + 1 {
                        if p[1] >= ctx.level.min_y()
                            && !ctx.level.is_solid(move_direction(p, Direction::Down))?
                        {
                            ctx.level.set_block(p, Block::CAVE_AIR.default_state(), 2)?;
                        } else if ctx.level.is_solid(p)? && old.block() != Block::CHEST {
                            let block = if dy == -1 && ctx.random.next_int_bound(4) != 0 {
                                Block::MOSSY_COBBLESTONE
                            } else {
                                Block::COBBLESTONE
                            };
                            self.safe_set(ctx, p, block.default_state())?;
                        }
                    } else if ![Block::CHEST, Block::SPAWNER].contains(&old.block()) {
                        self.safe_set(ctx, p, Block::CAVE_AIR.default_state())?;
                    }
                }
            }
        }
        for _ in 0..2 {
            for _ in 0..3 {
                let p = [
                    ctx.origin[0] + ctx.random.next_int_bound(xr * 2 + 1) - xr,
                    ctx.origin[1],
                    ctx.origin[2] + ctx.random.next_int_bound(zr * 2 + 1) - zr,
                ];
                if !is_air(ctx.level.block(p)?) {
                    continue;
                }
                let mut walls = 0;
                for d in HORIZONTAL {
                    if ctx.level.is_solid(move_direction(p, d))? {
                        walls += 1;
                    }
                }
                if walls == 1 {
                    self.safe_set(ctx, p, self.chest_state(ctx, p)?)?;
                    ctx.level.set_loot_table(
                        p,
                        &"chests/simple_dungeon".parse()?,
                        &mut ctx.random,
                    )?;
                    break;
                }
            }
        }
        self.safe_set(ctx, ctx.origin, Block::SPAWNER.default_state())?;
        // The placed vanilla spawner always creates a SpawnerBlockEntity.
        // A protected replacement can leave a different block in the origin;
        // vanilla then skips both mob selection and spawner initialization.
        if ctx.level.block(ctx.origin)?.block() == Block::SPAWNER
            && ctx.level.has_block_entity_at(ctx.origin)?
        {
            let mob = [
                EntityType::SKELETON,
                EntityType::ZOMBIE,
                EntityType::ZOMBIE,
                EntityType::SPIDER,
            ][ctx.random.next_int_bound(4) as usize];
            ctx.level
                .set_spawner_entity(ctx.origin, mob, &mut ctx.random)?;
        }
        Ok(true)
    }
}
