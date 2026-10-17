//! Port of ClientboundMoveEntityPacket and VecDelta.
use super::*;
packet!(/// Port of VecDelta.Stepped.DeltaStep, whose tick count precedes coordinates.
DeltaStep {ticks:VarInt,xa:i16,ya:i16,za:i16});

#[derive(Clone, Debug, PartialEq)]
pub enum VecDelta {
    Linear { xa: i16, ya: i16, za: i16 },
    Stepped(Vec<DeltaStep>),
}
/// The packed properties prefix supplies the step count without a separate list length.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityDelta {
    pub on_ground: bool,
    pub delta: VecDelta,
}
impl Wire for EntityDelta {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let properties = r.var_int()? as u32;
        let count = (properties >> 1) as usize;
        let delta = if count == 0 {
            VecDelta::Linear {
                xa: r.i16()?,
                ya: r.i16()?,
                za: r.i16()?,
            }
        } else {
            if count > r.remaining().len() / 7 {
                return Err(Error::Length {
                    length: count as i64,
                    max: r.remaining().len() / 7,
                });
            }
            let mut steps = Vec::with_capacity(count);
            for _ in 0..count {
                steps.push(r.read()?);
            }
            VecDelta::Stepped(steps)
        };
        Ok(Self {
            on_ground: properties & 1 != 0,
            delta,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        let count = match &self.delta {
            VecDelta::Linear { .. } => 0,
            VecDelta::Stepped(steps) => {
                if steps.is_empty() {
                    return Err(Error::invalid("VecDelta", "empty steps encode as linear"));
                }
                steps.len()
            }
        };
        if count > i32::MAX as usize {
            return Err(Error::Length {
                length: count as i64,
                max: i32::MAX as usize,
            });
        }
        w.var_int(((count as u32) << 1 | u32::from(self.on_ground)) as i32);
        match &self.delta {
            VecDelta::Linear { xa, ya, za } => {
                w.i16(*xa);
                w.i16(*ya);
                w.i16(*za);
            }
            VecDelta::Stepped(steps) => {
                for step in steps {
                    step.encode(w)?;
                }
            }
        }
        Ok(())
    }
}
packet!(/// Port of NewMinecartBehavior.MinecartStep#STREAM_CODEC.
MinecartStep {position:Vec3,movement:Vec3,y_rot:Angle,x_rot:Angle,weight:f32});
