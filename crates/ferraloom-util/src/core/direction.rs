use super::Vec3i;
use crate::{mth, phys::Vec3, random::RandomSource};

/// Port of Direction.Axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}
impl Axis {
    pub const VALUES: [Self; 3] = [Self::X, Self::Y, Self::Z];
    #[inline]
    pub fn choose<T>(self, x: T, y: T, z: T) -> T {
        match self {
            Self::X => x,
            Self::Y => y,
            Self::Z => z,
        }
    }
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "z" => Some(Self::Z),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        self.choose("x", "y", "z")
    }
    pub fn is_vertical(self) -> bool {
        self == Self::Y
    }
    pub fn is_horizontal(self) -> bool {
        self != Self::Y
    }
    pub fn positive(self) -> Direction {
        self.choose(Direction::East, Direction::Up, Direction::South)
    }
    pub fn negative(self) -> Direction {
        self.positive().opposite()
    }
    pub fn directions(self) -> [Direction; 2] {
        [self.positive(), self.negative()]
    }
    pub fn plane(self) -> Plane {
        if self.is_vertical() {
            Plane::Vertical
        } else {
            Plane::Horizontal
        }
    }
    pub fn random(random: &mut impl RandomSource) -> Self {
        Self::VALUES[random.next_int_bound(3) as usize]
    }
}
/// Port of Direction.AxisDirection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisDirection {
    Positive,
    Negative,
}
impl AxisDirection {
    pub fn step(self) -> i32 {
        if self == Self::Positive { 1 } else { -1 }
    }
    pub fn opposite(self) -> Self {
        if self == Self::Positive {
            Self::Negative
        } else {
            Self::Positive
        }
    }
}
/// Port of Direction. Discriminants are vanilla's 3D data values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}
impl Direction {
    pub const VALUES: [Self; 6] = [
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
        Self::West,
        Self::East,
    ];
    const BY_2D: [Self; 4] = [Self::South, Self::West, Self::North, Self::East];
    pub fn data_3d(self) -> i32 {
        self as i32
    }
    pub fn data_2d(self) -> i32 {
        match self {
            Self::Down | Self::Up => -1,
            Self::South => 0,
            Self::West => 1,
            Self::North => 2,
            Self::East => 3,
        }
    }
    pub fn from_3d_data_value(value: i32) -> Self {
        Self::VALUES[(value % 6).unsigned_abs() as usize]
    }
    pub fn from_2d_data_value(value: i32) -> Self {
        Self::BY_2D[(value % 4).unsigned_abs() as usize]
    }
    /// The network BY_ID mapper uses floor modulo, unlike from3DDataValue.
    pub fn by_id(value: i32) -> Self {
        Self::VALUES[value.rem_euclid(6) as usize]
    }
    pub fn from_y_rot(rotation: f64) -> Self {
        Self::from_2d_data_value(mth::floor(rotation / 90.0 + 0.5) & 3)
    }
    pub fn from_axis_and_direction(axis: Axis, direction: AxisDirection) -> Self {
        if direction == AxisDirection::Positive {
            axis.positive()
        } else {
            axis.negative()
        }
    }
    pub fn to_y_rot(self) -> f32 {
        ((self.data_2d() & 3) * 90) as f32
    }
    pub fn y_rot(self) -> f32 {
        match self {
            Self::North => 180.0,
            Self::South => 0.0,
            Self::West => 90.0,
            Self::East => -90.0,
            _ => panic!("no y rotation for vertical direction"),
        }
    }
    pub fn axis(self) -> Axis {
        match self {
            Self::Down | Self::Up => Axis::Y,
            Self::North | Self::South => Axis::Z,
            Self::West | Self::East => Axis::X,
        }
    }
    pub fn axis_direction(self) -> AxisDirection {
        match self {
            Self::Down | Self::North | Self::West => AxisDirection::Negative,
            _ => AxisDirection::Positive,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
            Self::North => "north",
            Self::South => "south",
            Self::West => "west",
            Self::East => "east",
        }
    }
    pub fn by_name(name: &str) -> Option<Self> {
        Self::VALUES.into_iter().find(|d| d.name() == name)
    }
    #[inline]
    pub fn opposite(self) -> Self {
        Self::VALUES[(self as usize) ^ 1]
    }
    #[inline]
    pub fn step(self) -> Vec3i {
        match self {
            Self::Down => Vec3i::new(0, -1, 0),
            Self::Up => Vec3i::new(0, 1, 0),
            Self::North => Vec3i::new(0, 0, -1),
            Self::South => Vec3i::new(0, 0, 1),
            Self::West => Vec3i::new(-1, 0, 0),
            Self::East => Vec3i::new(1, 0, 0),
        }
    }
    pub fn unit_vec3(self) -> Vec3 {
        Vec3::at_lower_corner_of(self.step())
    }
    pub fn clockwise(self) -> Self {
        assert!(
            self.axis() != Axis::Y,
            "cannot rotate vertical direction about Y"
        );
        self.clockwise_axis(Axis::Y)
    }
    pub fn counter_clockwise(self) -> Self {
        assert!(
            self.axis() != Axis::Y,
            "cannot rotate vertical direction about Y"
        );
        self.counter_clockwise_axis(Axis::Y)
    }
    pub fn clockwise_axis(self, axis: Axis) -> Self {
        match (axis, self) {
            (Axis::X, Self::Down) => Self::South,
            (Axis::X, Self::South) => Self::Up,
            (Axis::X, Self::Up) => Self::North,
            (Axis::X, Self::North) => Self::Down,
            (Axis::Y, Self::North) => Self::East,
            (Axis::Y, Self::East) => Self::South,
            (Axis::Y, Self::South) => Self::West,
            (Axis::Y, Self::West) => Self::North,
            (Axis::Z, Self::Down) => Self::West,
            (Axis::Z, Self::West) => Self::Up,
            (Axis::Z, Self::Up) => Self::East,
            (Axis::Z, Self::East) => Self::Down,
            _ => self,
        }
    }
    pub fn counter_clockwise_axis(self, axis: Axis) -> Self {
        self.clockwise_axis(axis)
            .clockwise_axis(axis)
            .clockwise_axis(axis)
    }
    /// Port of Direction#getApproximateNearest; float rounding and strict tie
    /// ordering are observable (including the North fallback at zero).
    pub fn get_approximate_nearest(dx: f64, dy: f64, dz: f64) -> Self {
        let (dx, dy, dz) = (dx as f32, dy as f32, dz as f32);
        let mut result = Self::North;
        let mut highest = f32::from_bits(1);
        for direction in Self::VALUES {
            let n = direction.step();
            let dot = dx * n.x as f32 + dy * n.y as f32 + dz * n.z as f32;
            if dot > highest {
                highest = dot;
                result = direction;
            }
        }
        result
    }
    /// Port of Direction#getNearest(int,int,int,Direction). Ties use `or_else`.
    pub fn get_nearest(x: i32, y: i32, z: i32, or_else: Option<Self>) -> Option<Self> {
        let (ax, ay, az) = (x.wrapping_abs(), y.wrapping_abs(), z.wrapping_abs());
        if ax > az && ax > ay {
            Some(if x < 0 { Self::West } else { Self::East })
        } else if az > ax && az > ay {
            Some(if z < 0 { Self::North } else { Self::South })
        } else if ay > ax && ay > az {
            Some(if y < 0 { Self::Down } else { Self::Up })
        } else {
            or_else
        }
    }
    pub fn axis_step_order(movement: Vec3) -> [Axis; 3] {
        if movement.x.abs() < movement.z.abs() {
            [Axis::Y, Axis::Z, Axis::X]
        } else {
            [Axis::Y, Axis::X, Axis::Z]
        }
    }
    pub fn is_facing_angle(self, angle: f32) -> bool {
        let radians = angle * mth::DEG_TO_RAD;
        let n = self.step();
        n.x as f32 * -mth::sin(f64::from(radians)) + n.z as f32 * mth::cos(f64::from(radians)) > 0.0
    }
    pub fn random(random: &mut impl RandomSource) -> Self {
        Self::VALUES[random.next_int_bound(6) as usize]
    }
    /// Entity-independent form of Direction#orderedByNearest.
    pub fn ordered_by_nearest(pitch: f32, yaw: f32) -> [Self; 6] {
        let pitch = pitch * mth::DEG_TO_RAD;
        let yaw = -yaw * mth::DEG_TO_RAD;
        let ps = mth::sin(f64::from(pitch));
        let pc = mth::cos(f64::from(pitch));
        let ys = mth::sin(f64::from(yaw));
        let yc = mth::cos(f64::from(yaw));
        let xp = ys > 0.0;
        let yp = ps < 0.0;
        let zp = yc > 0.0;
        let xy = if xp { ys } else { -ys };
        let ym = if yp { -ps } else { ps };
        let zy = if zp { yc } else { -yc };
        let xm = xy * pc;
        let zm = zy * pc;
        let x = if xp { Self::East } else { Self::West };
        let y = if yp { Self::Up } else { Self::Down };
        let z = if zp { Self::South } else { Self::North };
        let [a, b, c] = if xy > zy {
            if ym > xm {
                [y, x, z]
            } else if zm > ym {
                [x, z, y]
            } else {
                [x, y, z]
            }
        } else if ym > zm {
            [y, z, x]
        } else if xm > ym {
            [z, x, y]
        } else {
            [z, y, x]
        };
        [a, b, c, c.opposite(), b.opposite(), a.opposite()]
    }
}
/// Port of Direction.Plane. Horizontal iteration starts North, then East.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plane {
    Horizontal,
    Vertical,
}
impl Plane {
    pub fn faces(self) -> &'static [Direction] {
        match self {
            Self::Horizontal => &[
                Direction::North,
                Direction::East,
                Direction::South,
                Direction::West,
            ],
            Self::Vertical => &[Direction::Up, Direction::Down],
        }
    }
    pub fn axes(self) -> &'static [Axis] {
        match self {
            Self::Horizontal => &[Axis::X, Axis::Z],
            Self::Vertical => &[Axis::Y],
        }
    }
    pub fn contains(self, direction: Direction) -> bool {
        direction.axis().plane() == self
    }
    pub fn random_direction(self, random: &mut impl RandomSource) -> Direction {
        let faces = self.faces();
        faces[random.next_int_bound(faces.len() as i32) as usize]
    }
    pub fn random_axis(self, random: &mut impl RandomSource) -> Axis {
        let axes = self.axes();
        axes[random.next_int_bound(axes.len() as i32) as usize]
    }
    pub fn shuffled_copy(self, random: &mut impl RandomSource) -> Vec<Direction> {
        let mut result = self.faces().to_vec();
        for i in (2..=result.len()).rev() {
            result.swap(i - 1, random.next_int_bound(i as i32) as usize);
        }
        result
    }
}
impl IntoIterator for Plane {
    type Item = Direction;
    type IntoIter = std::iter::Copied<std::slice::Iter<'static, Direction>>;
    fn into_iter(self) -> Self::IntoIter {
        self.faces().iter().copied()
    }
}

/// Port of `world.level.block.Rotation`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Rotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    CounterClockwise90,
}
impl Rotation {
    pub const VALUES: [Self; 4] = [
        Self::None,
        Self::Clockwise90,
        Self::Clockwise180,
        Self::CounterClockwise90,
    ];
    pub fn rotated(self, rotation: Self) -> Self {
        Self::VALUES[(self as usize + rotation as usize) & 3]
    }
    pub fn rotate(self, direction: Direction) -> Direction {
        if direction.axis() == Axis::Y {
            return direction;
        }
        match self {
            Self::None => direction,
            Self::Clockwise90 => direction.clockwise(),
            Self::Clockwise180 => direction.opposite(),
            Self::CounterClockwise90 => direction.counter_clockwise(),
        }
    }
    pub fn rotate_index(self, rotation: i32, steps: i32) -> i32 {
        match self {
            Self::None => rotation,
            Self::Clockwise90 => rotation.wrapping_add(steps / 4).wrapping_rem(steps),
            Self::Clockwise180 => rotation.wrapping_add(steps / 2).wrapping_rem(steps),
            Self::CounterClockwise90 => rotation
                .wrapping_add(steps.wrapping_mul(3) / 4)
                .wrapping_rem(steps),
        }
    }
}
/// Port of `world.level.block.Mirror`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mirror {
    #[default]
    None,
    LeftRight,
    FrontBack,
}
impl Mirror {
    pub fn mirror(self, direction: Direction) -> Direction {
        if (self == Self::FrontBack && direction.axis() == Axis::X)
            || (self == Self::LeftRight && direction.axis() == Axis::Z)
        {
            direction.opposite()
        } else {
            direction
        }
    }
    pub fn rotation(self, direction: Direction) -> Rotation {
        if self.mirror(direction) != direction {
            Rotation::Clockwise180
        } else {
            Rotation::None
        }
    }
    pub fn mirror_index(self, rotation: i32, steps: i32) -> i32 {
        let half = steps / 2;
        let corrected = if rotation > half {
            rotation.wrapping_sub(steps)
        } else {
            rotation
        };
        match self {
            Self::None => rotation,
            Self::LeftRight => half
                .wrapping_sub(corrected)
                .wrapping_add(steps)
                .wrapping_rem(steps),
            Self::FrontBack => steps.wrapping_sub(corrected).wrapping_rem(steps),
        }
    }
}
