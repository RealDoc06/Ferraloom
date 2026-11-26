use crate::fast_hash::FxHashMap;
use horizon_core::random::{LegacyRandom, RandomSource};

/// `Direction` names by `get3DDataValue`.
pub const DIRECTIONS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

fn step(direction: u8) -> [i32; 3] {
    match direction {
        0 => [0, -1, 0],
        1 => [0, 1, 0],
        2 => [0, 0, -1],
        3 => [0, 0, 1],
        4 => [-1, 0, 0],
        _ => [1, 0, 0],
    }
}

/// `Direction.from3DDataValue`: `BY_3D_DATA[Mth.abs(data % 6)]`.
pub fn from_3d_data_value(data: i32) -> u8 {
    (data % 6).unsigned_abs() as u8
}

/// The 3D data value of a direction name.
pub fn direction_index(name: &str) -> Option<u8> {
    DIRECTIONS.iter().position(|d| *d == name).map(|i| i as u8)
}

/// `ChestLidController`, shared by `ChestBlockEntity` (chests, trapped and
/// copper chests) and `EnderChestBlockEntity`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChestLid {
    pub should_be_open: bool,
    pub openness: f32,
    pub o_openness: f32,
}

impl ChestLid {
    /// `triggerEvent(1, openCount)`: the lid opens while anyone has it open.
    pub fn trigger_event(&mut self, b0: i32, b1: i32) -> bool {
        if b0 == 1 {
            self.should_be_open = b1 > 0;
            true
        } else {
            false
        }
    }
}

/// `ChestRenderer.submit` and `ChestModel.setupAnim`: the lid and lock
/// `xRot` for a render-state openness.
pub fn chest_lid_x_rot(open: f32) -> f32 {
    let open = 1.0 - open;
    let open = 1.0 - open * open * open;
    -(open * std::f32::consts::FRAC_PI_2)
}

/// `EnchantingTableBlockEntity` book fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnchantingBook {
    pub time: i32,
    pub flip: f32,
    pub o_flip: f32,
    pub flip_t: f32,
    pub flip_a: f32,
    pub open: f32,
    pub o_open: f32,
    pub rot: f32,
    pub o_rot: f32,
    pub t_rot: f32,
}

/// `ShulkerBoxBlockEntity.AnimationStatus`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShulkerStatus {
    #[default]
    Closed,
    Opening,
    Opened,
    Closing,
}

/// `ShulkerBoxBlockEntity` lid state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shulker {
    pub open_count: i32,
    pub status: ShulkerStatus,
    pub progress: f32,
    pub progress_old: f32,
}

/// `Shulker.getProgressDeltaAabb(size, direction, from, to, position)`.
pub fn progress_delta_aabb(
    size: f32,
    direction: u8,
    from: f32,
    to: f32,
    position: [f64; 3],
) -> [f64; 6] {
    let size_d = f64::from(size);
    let mut b = [
        -size_d * 0.5,
        0.0,
        -size_d * 0.5,
        size_d * 0.5,
        size_d,
        size_d * 0.5,
    ];
    let max = f64::from(from.max(to));
    let min = f64::from(from.min(to));
    let s = step(direction);
    for axis in 0..3 {
        // expandTowards
        let a = f64::from(s[axis]) * max * size_d;
        if a < 0.0 {
            b[axis] += a;
        } else if a > 0.0 {
            b[axis + 3] += a;
        }
    }
    for axis in 0..3 {
        // contract
        let a = -f64::from(s[axis]) * (1.0 + min) * size_d;
        if a < 0.0 {
            b[axis] -= a;
        } else if a > 0.0 {
            b[axis + 3] -= a;
        }
    }
    // new AABB keeps min <= max; then move to the position.
    let mut out = [0.0; 6];
    for axis in 0..3 {
        out[axis] = b[axis].min(b[axis + 3]) + position[axis];
        out[axis + 3] = b[axis].max(b[axis + 3]) + position[axis];
    }
    out
}

impl Shulker {
    /// `triggerEvent(1, openCount)`.
    pub fn trigger_event(&mut self, b0: i32, b1: i32) -> bool {
        if b0 != 1 {
            return false;
        }
        self.open_count = b1;
        if b1 == 0 {
            self.status = ShulkerStatus::Closing;
        }
        if b1 == 1 {
            self.status = ShulkerStatus::Opening;
        }
        true
    }
    /// `startOpen`/`stopOpen` for a non-spectator user: the new open count to
    /// send as block event 1, and whether this was the first opener (open) or
    /// the last closer (close), which play the sound and game event.
    pub fn start_open(&mut self) -> (i32, bool) {
        if self.open_count < 0 {
            self.open_count = 0;
        }
        self.open_count += 1;
        (self.open_count, self.open_count == 1)
    }
    pub fn stop_open(&mut self) -> (i32, bool) {
        self.open_count -= 1;
        (self.open_count, self.open_count <= 0)
    }
    /// `ShulkerBoxBlock.canOpen`: a closed box only opens when its lid has room
    /// (`level.noCollision` of the half-block lid box, deflated by 1e-6).
    pub fn lid_box(pos: [i32; 3], facing: u8) -> [f64; 6] {
        let bottom = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        ];
        let mut b = progress_delta_aabb(1.0, facing, 0.0, 0.5, bottom);
        for axis in 0..3 {
            b[axis] += 1.0e-6;
            b[axis + 3] -= 1.0e-6;
        }
        b
    }
}

/// `BellBlockEntity`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bell {
    pub ticks: i32,
    pub shaking: bool,
    /// `clickDirection` as a 3D data value.
    pub click_direction: Option<u8>,
    pub last_ring_timestamp: i64,
    /// `nearbyEntities`: the living entities around the bell when it was last
    /// gathered, as caller-defined handles.
    pub nearby: Option<Vec<u64>>,
    pub resonating: bool,
    pub resonation_ticks: i32,
}

impl Bell {
    /// `triggerEvent(1, direction)`. `gather` lists the living entities whose
    /// position is inside the bell's block box inflated by 48
    /// (`updateEntities`, refreshed at most every 60 ticks).
    pub fn trigger_event(
        &mut self,
        b0: i32,
        b1: i32,
        game_time: i64,
        gather: impl FnOnce() -> Vec<u64>,
    ) -> bool {
        if b0 != 1 {
            return false;
        }
        if game_time > self.last_ring_timestamp + 60 || self.nearby.is_none() {
            self.last_ring_timestamp = game_time;
            self.nearby = Some(gather());
        }
        self.resonation_ticks = 0;
        self.click_direction = Some(from_3d_data_value(b1));
        self.ticks = 0;
        self.shaking = true;
        true
    }
}

/// `ConduitBlockEntity` (client side).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conduit {
    pub tick_count: i32,
    pub active_rotation: f32,
    pub is_active: bool,
    pub is_hunting: bool,
    pub effect_blocks: Vec<[i32; 3]>,
}

impl Conduit {
    /// `updateShape`: water all around, then the frame blocks
    /// (`BlockTags.CONDUIT_EFFECT_BLOCK`) in the three 5x5 rings.
    fn update_shape(
        &mut self,
        pos: [i32; 3],
        is_water: &impl Fn([i32; 3]) -> bool,
        is_frame: &impl Fn([i32; 3]) -> bool,
    ) -> bool {
        self.effect_blocks.clear();
        for ox in -1..=1 {
            for oy in -1..=1 {
                for oz in -1..=1 {
                    if !is_water([pos[0] + ox, pos[1] + oy, pos[2] + oz]) {
                        return false;
                    }
                }
            }
        }
        for ox in -2i32..=2 {
            for oy in -2i32..=2 {
                for oz in -2i32..=2 {
                    let (ax, ay, az) = (ox.abs(), oy.abs(), oz.abs());
                    if (ax > 1 || ay > 1 || az > 1)
                        && (ox == 0 && (ay == 2 || az == 2)
                            || oy == 0 && (ax == 2 || az == 2)
                            || oz == 0 && (ax == 2 || ay == 2))
                    {
                        let p = [pos[0] + ox, pos[1] + oy, pos[2] + oz];
                        if is_frame(p) {
                            self.effect_blocks.push(p);
                        }
                    }
                }
            }
        }
        self.effect_blocks.len() >= 16
    }
}

/// What `ConduitBlockEntity.serverTick`'s 40-tick check decided.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ConduitCheck {
    /// The shape changed: `CONDUIT_ACTIVATE` (true) or `CONDUIT_DEACTIVATE`.
    pub toggled: Option<bool>,
    /// The frame blocks counted, while the conduit is active: `applyEffects`
    /// and `updateAndAttackTarget` follow (the attack needs 42 or more).
    pub frames: Option<usize>,
}

/// `ConduitBlockEntity` (server side): what `serverTick` keeps between ticks.
#[derive(Clone, Debug, Default)]
pub struct ConduitServer {
    shape: Conduit,
    /// `destroyTarget`: the mob id of the enemy being struck.
    pub target: Option<u32>,
    next_ambient_sound: i64,
}

impl ConduitServer {
    /// The `gameTime % 40` part of `serverTick`: the shape is checked again.
    pub fn check(
        &mut self,
        pos: [i32; 3],
        game_time: i64,
        is_water: impl Fn([i32; 3]) -> bool,
        is_frame: impl Fn([i32; 3]) -> bool,
    ) -> Option<ConduitCheck> {
        if game_time % 40 != 0 {
            return None;
        }
        let active = self.shape.update_shape(pos, &is_water, &is_frame);
        let toggled = (active != self.shape.is_active).then_some(active);
        self.shape.is_active = active;
        Some(ConduitCheck {
            toggled,
            frames: active.then_some(self.shape.effect_blocks.len()),
        })
    }
    /// The sounds of an active conduit at the end of `serverTick`: whether
    /// `CONDUIT_AMBIENT` (every 80 ticks) and `CONDUIT_AMBIENT_SHORT` (every
    /// 60 to 99 ticks) play.
    pub fn ambient(&mut self, game_time: i64, random: &mut impl RandomSource) -> (bool, bool) {
        if !self.shape.is_active {
            return (false, false);
        }
        let short = game_time > self.next_ambient_sound;
        if short {
            self.next_ambient_sound =
                game_time + 60 + i64::from(random.next_int_bound(40).unwrap_or(0));
        }
        (game_time % 80 == 0, short)
    }
}

/// `SkullBlockEntity` animation (client ticker of dragon and piglin heads).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Skull {
    pub animating: bool,
    pub tick_count: i32,
}

/// `DecoratedPotBlockEntity.WobbleStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WobbleStyle {
    Positive,
    Negative,
}

/// `DecoratedPotBlockEntity` wobble fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PotWobble {
    pub started_at: i64,
    pub style: Option<WobbleStyle>,
}
impl PotWobble {
    /// `triggerEvent(1, style)`.
    pub fn trigger_event(&mut self, event: i32, data: i32, game_time: i64) -> bool {
        if event == 1 && (0..2).contains(&data) {
            self.started_at = game_time;
            self.style = Some(if data == 0 {
                WobbleStyle::Positive
            } else {
                WobbleStyle::Negative
            });
            true
        } else {
            false
        }
    }
}

/// `TheEndGatewayBlockEntity` fields the beam reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndGateway {
    pub age: i64,
    pub teleport_cooldown: i32,
}
impl EndGateway {
    /// `portalTick` on the server copy: whether it called `triggerCooldown`,
    /// whose block event 1 reaches the client copy.
    pub fn portal_tick(&mut self) -> bool {
        let cooling = self.teleport_cooldown > 0;
        self.age = self.age.wrapping_add(1);
        if cooling {
            self.teleport_cooldown -= 1;
            false
        } else if self.age % 2400 == 0 {
            self.teleport_cooldown = 40;
            true
        } else {
            false
        }
    }
    /// `EndGatewayBlock.entityInside` on the server copy: an entity that can
    /// use the portal starts the cooldown unless it is already running.
    pub fn enter(&mut self) -> bool {
        if self.teleport_cooldown > 0 {
            return false;
        }
        self.teleport_cooldown = 40;
        true
    }
    /// `triggerEvent`: event 1 starts the 40 tick cooldown.
    pub fn trigger_event(&mut self, event: i32, _data: i32) -> bool {
        if event == 1 {
            self.teleport_cooldown = 40;
            true
        } else {
            false
        }
    }
}

/// The client ticker a block entity runs (`getTicker` on a client level),
/// or [`Ticker::Pot`] for decorated pots, which keep wobble state unticked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ticker {
    /// `ChestBlockEntity` / `EnderChestBlockEntity.lidAnimateTick`.
    Lid,
    /// `EnchantingTableBlockEntity.bookAnimationTick`.
    Book,
    /// `ShulkerBoxBlockEntity.tick`, with the box's facing.
    Shulker {
        facing: u8,
    },
    /// `BellBlockEntity.clientTick`.
    Bell,
    /// `ConduitBlockEntity.clientTick`.
    Conduit,
    /// `SkullBlockEntity.animation` (dragon and piglin heads only).
    Skull {
        powered: bool,
    },
    Pot,
    /// `TheEndGatewayBlockEntity.beamAnimationTick`.
    Gateway,
}

/// The [`Ticker`] of a block state, by block name and property lookup.
pub fn ticker(name: &str, prop: impl Fn(&str) -> Option<String>) -> Option<Ticker> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let unwaxed = name.strip_prefix("waxed_").unwrap_or(name);
    Some(match unwaxed {
        "chest"
        | "trapped_chest"
        | "ender_chest"
        | "copper_chest"
        | "exposed_copper_chest"
        | "weathered_copper_chest"
        | "oxidized_copper_chest" => Ticker::Lid,
        "enchanting_table" => Ticker::Book,
        "bell" => Ticker::Bell,
        "conduit" => Ticker::Conduit,
        "decorated_pot" => Ticker::Pot,
        "end_gateway" => Ticker::Gateway,
        "dragon_head" | "dragon_wall_head" | "piglin_head" | "piglin_wall_head" => Ticker::Skull {
            powered: prop("powered").as_deref() == Some("true"),
        },
        n if n.ends_with("shulker_box") => Ticker::Shulker {
            facing: prop("facing")
                .as_deref()
                .and_then(direction_index)
                .unwrap_or(1),
        },
        _ => return None,
    })
}

/// The animated block entities of a level, by position.
#[derive(Clone, Debug)]
pub struct BlockEntityAnims {
    pub chests: FxHashMap<[i32; 3], ChestLid>,
    pub books: FxHashMap<[i32; 3], EnchantingBook>,
    /// `EnchantingTableBlockEntity.RANDOM`, shared by every table.
    pub book_random: LegacyRandom,
    pub shulkers: FxHashMap<[i32; 3], Shulker>,
    pub bells: FxHashMap<[i32; 3], Bell>,
    pub conduits: FxHashMap<[i32; 3], Conduit>,
    pub skulls: FxHashMap<[i32; 3], Skull>,
    pub pots: FxHashMap<[i32; 3], PotWobble>,
    pub gateways: FxHashMap<[i32; 3], EndGateway>,
    /// The integrated server's copies of the gateways (`portalTick`).
    pub gateway_servers: FxHashMap<[i32; 3], EndGateway>,
    /// The integrated server's copies of the conduits (`serverTick`).
    pub conduit_servers: FxHashMap<[i32; 3], ConduitServer>,
    /// The client level's random (conduit particles).
    pub random: LegacyRandom,
    /// [`Ticker`] by block state id (see [`BlockEntityAnims::classify`]).
    pub kinds: Vec<Option<Ticker>>,
    /// The drawn block entities with a ticker, sorted by position, and the
    /// renderer's block entity revision they were read at.
    pub present: Vec<([i32; 3], Ticker)>,
    pub present_revision: Option<u64>,
}
impl Default for BlockEntityAnims {
    fn default() -> Self {
        Self {
            chests: FxHashMap::default(),
            books: FxHashMap::default(),
            book_random: LegacyRandom::new(0x626f6f6b),
            shulkers: FxHashMap::default(),
            bells: FxHashMap::default(),
            conduits: FxHashMap::default(),
            skulls: FxHashMap::default(),
            pots: FxHashMap::default(),
            gateways: FxHashMap::default(),
            gateway_servers: FxHashMap::default(),
            conduit_servers: FxHashMap::default(),
            random: LegacyRandom::new(0x6c6576656c),
            kinds: Vec::new(),
            present: Vec::new(),
            present_revision: None,
        }
    }
}
impl BlockEntityAnims {
    /// Fills [`Self::kinds`] for every state of the registry.
    pub fn classify(&mut self, registry: &horizon_world::registry::BlockRegistry) {
        self.kinds = (0..registry.len() as u32)
            .map(|id| {
                registry
                    .state(horizon_world::registry::BlockStateId(id))
                    .and_then(|s| ticker(&s.name, |k| s.properties.get(k).cloned()))
            })
            .collect();
        self.present_revision = None;
    }
    /// The ticker of a block state.
    pub fn ticker(&self, state: u32) -> Option<Ticker> {
        self.kinds.get(state as usize).copied().flatten()
    }
}
