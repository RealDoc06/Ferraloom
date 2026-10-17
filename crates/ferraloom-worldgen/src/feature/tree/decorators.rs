use super::*;
use crate::{Identifier, placement::predicate::direction, region::Heightmap};
/// Port of every registered 26.3 TreeDecorator.
#[derive(Debug)]
pub(super) enum Decorator {
    AlterGround(Provider),
    AttachedLeaves {
        probability: f32,
        xz: i32,
        y: i32,
        provider: Provider,
        empty: i32,
        directions: Vec<Direction>,
    },
    AttachedLogs {
        probability: f32,
        provider: Provider,
        directions: Vec<Direction>,
    },
    Beehive(f32),
    Cocoa(f32),
    CreakingHeart {
        probability: f32,
        logs: Vec<Block>,
    },
    LeaveVine(f32),
    TrunkVine,
    PaleMoss {
        leaves: f32,
        trunk: f32,
        ground: f32,
        patch: Identifier,
    },
    Ground {
        tries: i32,
        radius: i32,
        height: i32,
        provider: Provider,
    },
    Shelf(f32),
}
fn directions(v: &Value) -> Result<Vec<Direction>, Error> {
    let dirs = list(field(v, "directions")?)?
        .iter()
        .map(direction)
        .collect::<Result<Vec<_>, _>>()?;
    if dirs.is_empty() {
        Err(Error("empty decorator directions".into()))
    } else {
        Ok(dirs)
    }
}
impl Decorator {
    pub fn list(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Vec<Self>, Error> {
        list(v)?.iter().map(|v| Self::decode(v, l)).collect()
    }
    fn decode(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        let ty = kind(v)?;
        Ok(match ty.to_string().as_str() {
            "minecraft:alter_ground" => Self::AlterGround(l.state_provider(field(v, "provider")?)?),
            "minecraft:attached_to_leaves" => Self::AttachedLeaves {
                probability: chance(v, "probability")?,
                xz: int(v, "exclusion_radius_xz", 0, 16)?,
                y: int(v, "exclusion_radius_y", 0, 16)?,
                provider: l.state_provider(field(v, "block_provider")?)?,
                empty: int(v, "required_empty_blocks", 1, 16)?,
                directions: directions(v)?,
            },
            "minecraft:attached_to_logs" => Self::AttachedLogs {
                probability: chance(v, "probability")?,
                provider: l.state_provider(field(v, "block_provider")?)?,
                directions: directions(v)?,
            },
            "minecraft:beehive" => Self::Beehive(chance(v, "probability")?),
            "minecraft:cocoa" => Self::Cocoa(chance(v, "probability")?),
            "minecraft:creaking_heart" => Self::CreakingHeart {
                probability: chance(v, "probability")?,
                logs: tag(l, "logs")?,
            },
            "minecraft:leave_vine" => Self::LeaveVine(chance(v, "probability")?),
            "minecraft:trunk_vine" => Self::TrunkVine,
            "minecraft:pale_moss" => Self::PaleMoss {
                leaves: chance(v, "leaves_probability")?,
                trunk: chance(v, "trunk_probability")?,
                ground: chance(v, "ground_probability")?,
                patch: "pale_moss_patch".parse()?,
            },
            "minecraft:place_on_ground" => Self::Ground {
                tries: optional(v, "tries", 128, |v| int_range(v, 1, i32::MAX))?,
                radius: optional(v, "radius", 2, |v| int_range(v, 0, i32::MAX))?,
                height: optional(v, "height", 1, |v| int_range(v, 0, i32::MAX))?,
                provider: l.state_provider(field(v, "block_state_provider")?)?,
            },
            "minecraft:shelf_mushroom" => Self::Shelf(chance(v, "probability")?),
            _ => return Err(Error(format!("unknown tree decorator {ty}"))),
        })
    }
    fn place(
        &self,
        c: &mut FeatureContext<'_, '_>,
        logs: &[Pos],
        leaves: &[Pos],
        roots: &[Pos],
        out: &mut Positions,
    ) -> Result<(), Error> {
        match self {
            Self::TrunkVine | Self::LeaveVine(_) => {
                let positions = if matches!(self, Self::TrunkVine) {
                    logs
                } else {
                    leaves
                };
                for &p in positions {
                    for d in [
                        Direction::West,
                        Direction::East,
                        Direction::North,
                        Direction::South,
                    ] {
                        let yes = if let Self::LeaveVine(chance) = self {
                            c.random.next_float() < *chance
                        } else {
                            c.random.next_int_bound(3) > 0
                        };
                        if yes {
                            let mut p = step(p, d);
                            if is_air(c.region.block(p)?) {
                                vine(c, p, d.opposite(), out)?;
                                if matches!(self, Self::LeaveVine(_)) {
                                    for _ in 0..4 {
                                        p[1] -= 1;
                                        if !is_air(c.region.block(p)?) {
                                            break;
                                        }
                                        vine(c, p, d.opposite(), out)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Self::Cocoa(probability) => {
                if c.random.next_float() >= *probability {
                    return Ok(());
                }
                if let Some(base) = logs.first() {
                    for &p in logs.iter().filter(|p| p[1] - base[1] <= 2) {
                        for d in HORIZONTAL {
                            if c.random.next_float() <= 0.25 {
                                let p = step(p, d.opposite());
                                if is_air(c.region.block(p)?) {
                                    let age = c.random.next_int_bound(3);
                                    let s = Block::COCOA
                                        .default_state()
                                        .with_integer_property("age", age)
                                        .map_err(|e| Error(format!("{e:?}")))?;
                                    put(c, p, facing(s, d), out)?;
                                }
                            }
                        }
                    }
                }
            }
            Self::AttachedLogs {
                probability,
                provider,
                directions,
            } => {
                let mut positions = logs.to_vec();
                shuffle(&mut positions, c.random);
                for p in positions {
                    let d = directions[c.random.next_int_bound(directions.len() as i32) as usize];
                    let p = step(p, d);
                    if c.random.next_float() <= *probability && is_air(c.region.block(p)?) {
                        let s = provider.get_state(c, p)?;
                        put(c, p, s, out)?;
                    }
                }
            }
            Self::AttachedLeaves {
                probability,
                xz,
                y,
                provider,
                empty,
                directions,
            } => {
                let mut positions = leaves.to_vec();
                shuffle(&mut positions, c.random);
                let mut blacklist = BTreeSet::new();
                for leaf in positions {
                    let d = directions[c.random.next_int_bound(directions.len() as i32) as usize];
                    let p = step(leaf, d);
                    if blacklist.contains(&p) || c.random.next_float() >= *probability {
                        continue;
                    }
                    let mut air = true;
                    for i in 1..=*empty {
                        if !is_air(c.region.block(relative(leaf, d, i))?) {
                            air = false;
                            break;
                        }
                    }
                    if air {
                        for dx in -*xz..=*xz {
                            for dy in -*y..=*y {
                                for dz in -*xz..=*xz {
                                    blacklist.insert(add(p, [dx, dy, dz]));
                                }
                            }
                        }
                        let s = provider.get_state(c, p)?;
                        put(c, p, s, out)?;
                    }
                }
            }
            Self::Beehive(probability) => {
                let Some(base) = logs.first() else {
                    return Ok(());
                };
                if c.random.next_float() >= *probability {
                    return Ok(());
                }
                let y = if let Some(leaf) = leaves.first() {
                    (leaf[1] - 1).max(base[1] + 1)
                } else {
                    (base[1] + 1 + c.random.next_int_bound(3)).min(logs.last().unwrap()[1])
                };
                let mut positions = logs
                    .iter()
                    .filter(|p| p[1] == y)
                    .flat_map(|&p| {
                        [Direction::East, Direction::South, Direction::West].map(|d| step(p, d))
                    })
                    .collect::<Vec<_>>();
                shuffle(&mut positions, c.random);
                for p in positions {
                    if is_air(c.region.block(p)?)
                        && is_air(c.region.block(step(p, Direction::South))?)
                    {
                        put(
                            c,
                            p,
                            facing(Block::BEE_NEST.default_state(), Direction::South),
                            out,
                        )?;
                        c.region.populate_worldgen_beehive(p, c.random)?;
                        break;
                    }
                }
            }
            Self::CreakingHeart {
                probability,
                logs: log_blocks,
            } => {
                if logs.is_empty() || c.random.next_float() >= *probability {
                    return Ok(());
                }
                let mut positions = logs.to_vec();
                shuffle(&mut positions, c.random);
                for p in positions {
                    let mut enclosed = true;
                    for d in Direction::VALUES {
                        if !log_blocks.contains(&c.region.block(step(p, d))?.block()) {
                            enclosed = false;
                            break;
                        }
                    }
                    if enclosed {
                        let s = Block::CREAKING_HEART
                            .default_state()
                            .with(
                                properties::CREAKING_HEART_STATE,
                                properties::CreakingHeartState::Dormant,
                            )
                            .unwrap()
                            .with(properties::NATURAL, true)
                            .unwrap();
                        put(c, p, s, out)?;
                        break;
                    }
                }
            }
            Self::AlterGround(provider) => {
                let positions = lowest(logs, roots);
                if let Some(base) = positions.first() {
                    for &p in positions.iter().filter(|p| p[1] == base[1]) {
                        for delta in [[-1, 0, -1], [2, 0, -1], [-1, 0, 2], [2, 0, 2]] {
                            circle(c, add(p, delta), provider, out)?;
                        }
                        for _ in 0..5 {
                            let n = c.random.next_int_bound(64);
                            let x = n % 8;
                            let z = n / 8;
                            if x == 0 || x == 7 || z == 0 || z == 7 {
                                circle(c, add(p, [x - 3, 0, z - 3]), provider, out)?;
                            }
                        }
                    }
                }
            }
            Self::Ground {
                tries,
                radius,
                height,
                provider,
            } => {
                let positions = lowest(logs, roots);
                if let Some(base) = positions.first() {
                    let positions = positions
                        .iter()
                        .filter(|p| p[1] == base[1])
                        .collect::<Vec<_>>();
                    let min = [
                        positions.iter().map(|p| p[0]).min().unwrap() - radius,
                        base[1] - height,
                        positions.iter().map(|p| p[2]).min().unwrap() - radius,
                    ];
                    let max = [
                        positions.iter().map(|p| p[0]).max().unwrap() + radius,
                        base[1] + height,
                        positions.iter().map(|p| p[2]).max().unwrap() + radius,
                    ];
                    for _ in 0..*tries {
                        let p = std::array::from_fn::<_, 3, _>(|i| {
                            min[i] + c.random.next_int_bound(max[i] - min[i] + 1)
                        });
                        let above = add(p, [0, 1, 0]);
                        let s = c.region.block(above)?;
                        if (is_air(s) || s.block() == Block::VINE)
                            && c.region.block(p)?.data().solid_render
                            && c.region
                                .height_at(Heightmap::MotionBlockingNoLeaves, p[0], p[2])?
                                <= above[1]
                        {
                            let s = provider.get_state(c, above)?;
                            put(c, above, s, out)?;
                        }
                    }
                }
            }
            Self::PaleMoss {
                leaves: leaf_chance,
                trunk,
                ground,
                patch,
            } => {
                let mut positions = logs.to_vec();
                shuffle(&mut positions, c.random);
                if let Some(&origin) = positions.iter().min_by_key(|p| p[1]) {
                    if c.random.next_float() < *ground
                        && let Some(feature) = c.registry.features.get(patch).cloned()
                    {
                        let old = c.origin;
                        c.origin = add(origin, [0, 1, 0]);
                        let result = feature.place(c);
                        c.origin = old;
                        result?;
                    }
                    for (&p, chance) in logs
                        .iter()
                        .map(|p| (p, *trunk))
                        .chain(leaves.iter().map(|p| (p, *leaf_chance)))
                    {
                        if c.random.next_float() < chance {
                            let mut p = add(p, [0, -1, 0]);
                            if is_air(c.region.block(p)?) {
                                while is_air(c.region.block(add(p, [0, -1, 0]))?)
                                    && f64::from(c.random.next_float()) >= 0.5
                                {
                                    put(
                                        c,
                                        p,
                                        Block::PALE_HANGING_MOSS
                                            .default_state()
                                            .with(properties::TIP, false)
                                            .unwrap(),
                                        out,
                                    )?;
                                    p[1] -= 1;
                                }
                                put(
                                    c,
                                    p,
                                    Block::PALE_HANGING_MOSS
                                        .default_state()
                                        .with(properties::TIP, true)
                                        .unwrap(),
                                    out,
                                )?;
                            }
                        }
                    }
                }
            }
            Self::Shelf(probability) => {
                if c.random.next_float() >= *probability || logs.is_empty() {
                    return Ok(());
                }
                let first = logs[0];
                let last = *logs.last().unwrap();
                let fallen = first[1] == last[1];
                let dirs = if fallen {
                    if first[0] != last[0] {
                        [Direction::North, Direction::South]
                    } else {
                        [Direction::East, Direction::West]
                    }
                } else {
                    let d = horizontal(c);
                    [d, d.clockwise()]
                };
                for &p in logs {
                    if !fallen && !(1..=4).contains(&(p[1] - first[1])) {
                        continue;
                    }
                    for d in dirs {
                        if c.random.next_float() > 0.25 {
                            continue;
                        }
                        let q = step(p, d);
                        if !shelf_replaceable(c, q)? {
                            continue;
                        }
                        if fallen {
                            if shelf_near(c, q)? || shelf_near(c, p)? {
                                continue;
                            }
                        } else if c.region.block(add(q, [0, -1, 0]))?.block()
                            == Block::SHELF_MUSHROOM
                        {
                            continue;
                        }
                        let age = c.random.next_int_bound(2);
                        let s = Block::SHELF_MUSHROOM
                            .default_state()
                            .with_integer_property("age", age)
                            .map_err(|e| Error(format!("{e:?}")))?;
                        put(c, q, facing(s, d), out)?;
                        if !fallen {
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
pub(super) fn decorate(
    decorators: &[Decorator],
    c: &mut FeatureContext<'_, '_>,
    logs: &Positions,
    leaves: &Positions,
    roots: &Positions,
    out: &mut Positions,
) -> Result<(), Error> {
    if decorators.is_empty() {
        return Ok(());
    }
    let logs = logs.sorted();
    let leaves = leaves.sorted();
    let roots = roots.sorted();
    for decorator in decorators {
        decorator.place(c, &logs, &leaves, &roots, out)?;
    }
    Ok(())
}
fn lowest(logs: &[Pos], roots: &[Pos]) -> Vec<Pos> {
    if roots.is_empty() {
        logs.to_vec()
    } else if !logs.is_empty() && roots[0][1] == logs[0][1] {
        logs.iter().chain(roots).copied().collect()
    } else {
        roots.to_vec()
    }
}
fn facing(s: BlockState, d: Direction) -> BlockState {
    s.with(
        properties::FACING,
        match d {
            Direction::Down => properties::Facing::Down,
            Direction::Up => properties::Facing::Up,
            Direction::North => properties::Facing::North,
            Direction::South => properties::Facing::South,
            Direction::West => properties::Facing::West,
            Direction::East => properties::Facing::East,
        },
    )
    .unwrap_or(s)
}
fn vine(
    c: &mut FeatureContext<'_, '_>,
    p: Pos,
    d: Direction,
    out: &mut Positions,
) -> Result<(), Error> {
    let s = Block::VINE.default_state();
    let s = match d {
        Direction::North => s.with(properties::NORTH, properties::North::True),
        Direction::South => s.with(properties::SOUTH, properties::South::True),
        Direction::West => s.with(properties::WEST, properties::West::True),
        Direction::East => s.with(properties::EAST, properties::East::True),
        _ => unreachable!(),
    }
    .unwrap();
    put(c, p, s, out)
}
fn circle(
    c: &mut FeatureContext<'_, '_>,
    p: Pos,
    provider: &Provider,
    out: &mut Positions,
) -> Result<(), Error> {
    for x in -2i32..=2 {
        for z in -2i32..=2 {
            if x.abs() == 2 && z.abs() == 2 {
                continue;
            }
            for y in (-3..=2).rev() {
                let q = add(p, [x, y, z]);
                if let Some(s) = provider.get_optional_state(c, q)? {
                    put(c, q, s, out)?;
                    break;
                }
                if y < 0 && !is_air(c.region.block(q)?) {
                    break;
                }
            }
        }
    }
    Ok(())
}
fn shelf_replaceable(c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
    if !c.region.block(p)?.data().replaceable {
        return Ok(false);
    }
    for q in std::iter::once(p).chain(
        [
            Direction::East,
            Direction::West,
            Direction::North,
            Direction::South,
        ]
        .map(|d| step(p, d)),
    ) {
        if c.region.block(q)?.block() == Block::WATER {
            return Ok(false);
        }
    }
    Ok(true)
}
fn shelf_near(c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
    for d in HORIZONTAL {
        if c.region.block(step(p, d))?.block() == Block::SHELF_MUSHROOM {
            return Ok(true);
        }
    }
    Ok(false)
}
