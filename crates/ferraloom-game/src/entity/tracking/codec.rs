//! Port of VecDeltaCodec, UpdateInterval and SteppedInterpolationTracker.
use ferraloom_protocol::{
    codec::VarInt,
    play::{DeltaStep, PositionPath, PositionStep, VecDelta},
};
use ferraloom_util::Vec3;

/// Math.round(double), including negative half ties and values just below +0.5.
fn java_round(value: f64) -> i64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        (floor as i64).saturating_add(1)
    } else {
        floor as i64
    }
}
fn encode(value: f64) -> i64 {
    java_round(value * 4096.0)
}
pub fn encoding_precision_loss(value: f64) -> f64 {
    encode(value) as f64 / 4096.0 - value
}

/// Native rounding is applied separately to the endpoints before subtraction.
#[derive(Clone, Copy, Debug, Default)]
pub struct VecDeltaCodec {
    pub base: Vec3,
}
impl VecDeltaCodec {
    pub fn delta(&self, position: Vec3) -> Vec3 {
        position - self.base
    }
    pub fn try_encode(&self, position: Vec3) -> Option<VecDelta> {
        let xa = encode(position.x).wrapping_sub(encode(self.base.x));
        let ya = encode(position.y).wrapping_sub(encode(self.base.y));
        let za = encode(position.z).wrapping_sub(encode(self.base.z));
        Some(VecDelta::Linear {
            xa: xa.try_into().ok()?,
            ya: ya.try_into().ok()?,
            za: za.try_into().ok()?,
        })
    }
    pub fn try_encode_path(&self, path: &PositionPath) -> Option<VecDelta> {
        match path {
            PositionPath::Linear(position) => {
                self.try_encode(Vec3::new(position.x, position.y, position.z))
            }
            PositionPath::Stepped(steps) => {
                let mut codec = *self;
                let mut output = Vec::with_capacity(steps.len());
                for step in steps {
                    let position = Vec3::new(step.position.x, step.position.y, step.position.z);
                    let VecDelta::Linear { xa, ya, za } = codec.try_encode(position)? else {
                        unreachable!()
                    };
                    output.push(DeltaStep {
                        ticks: step.tick_offset,
                        xa,
                        ya,
                        za,
                    });
                    codec.base = position;
                }
                Some(VecDelta::Stepped(output))
            }
        }
    }
    pub fn decode(&self, xa: i64, ya: i64, za: i64) -> Vec3 {
        let component = |base, delta| {
            if delta == 0 {
                base
            } else {
                encode(base).wrapping_add(delta) as f64 / 4096.0
            }
        };
        Vec3::new(
            component(self.base.x, xa),
            component(self.base.y, ya),
            component(self.base.z, za),
        )
    }
}

/// EntityType's MAX_VALUE interval is represented by UpdateInterval.NEVER.
#[derive(Clone, Copy, Debug)]
pub enum UpdateInterval {
    Never,
    Periodic(std::num::NonZeroI32),
}
impl UpdateInterval {
    pub fn for_entity_type(kind: ferraloom_data::EntityType) -> Self {
        let period = kind.data().update_interval as i32;
        if period == i32::MAX {
            Self::Never
        } else {
            Self::Periodic(std::num::NonZeroI32::new(period).expect("positive vanilla interval"))
        }
    }
    pub fn test(self, tick: i32) -> bool {
        match self {
            Self::Never => false,
            Self::Periodic(period) => tick % period.get() == 0,
        }
    }
    pub fn next_interval(self, tick: i32) -> i32 {
        match self {
            Self::Never => 0,
            Self::Periodic(period) => (tick / period.get())
                .wrapping_mul(period.get())
                .wrapping_add(period.get()),
        }
    }
}

#[derive(Debug)]
pub struct SteppedInterpolationTracker {
    steps: Vec<PositionStep>,
    ticks_since_last_step: i32,
    predicted_delta: Vec3,
}
impl Default for SteppedInterpolationTracker {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            ticks_since_last_step: -1,
            predicted_delta: Vec3::ZERO,
        }
    }
}
impl SteppedInterpolationTracker {
    pub fn apply_predicted_movement(&mut self, delta: Vec3) {
        self.predicted_delta = self.predicted_delta + delta;
    }
    fn add_step(&mut self, position: Vec3) {
        if self.ticks_since_last_step > 0 {
            self.steps.push(PositionStep {
                position: ferraloom_protocol::play::Vec3 {
                    x: position.x,
                    y: position.y,
                    z: position.z,
                },
                tick_offset: VarInt(self.ticks_since_last_step),
            });
            self.ticks_since_last_step = 0;
        }
    }
    pub fn update_tracking(&mut self, position: Vec3, sync_position: &mut bool) {
        if self.predicted_delta.length_sqr() > 9.999999747378752E-6 {
            for step in &mut self.steps {
                step.position.x += self.predicted_delta.x;
                step.position.y += self.predicted_delta.y;
                step.position.z += self.predicted_delta.z;
            }
        }
        self.predicted_delta = Vec3::ZERO;
        self.ticks_since_last_step = self.ticks_since_last_step.wrapping_add(1);
        if *sync_position {
            *sync_position = false;
            self.add_step(position);
        }
    }
    pub fn position_path(&mut self, current_position: Vec3) -> PositionPath {
        self.add_step(current_position);
        if self.steps.is_empty() {
            PositionPath::Linear(ferraloom_protocol::play::Vec3 {
                x: current_position.x,
                y: current_position.y,
                z: current_position.z,
            })
        } else {
            PositionPath::Stepped(self.steps.clone())
        }
    }
    pub fn clear(&mut self) {
        self.steps.clear();
        self.ticks_since_last_step = 0;
    }
}

#[cfg(test)]
mod tests;
