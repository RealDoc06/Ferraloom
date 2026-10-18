//! Port of EntityDimensions, EntityAttachments and Entity's size-change helpers.
use std::borrow::Cow;

pub use ferraloom_data::EntityAttachment;
use ferraloom_data::EntityType;
use ferraloom_protocol::entity_data::Pose;
use ferraloom_util::{Aabb, Vec3, shapes};

use super::{Entity, EntityData, EntityRef, collision_context::EntityCollisionView, synched_data};
use crate::{
    block::BlockCollisionContext,
    level::{Level, LevelError},
};

const ATTACHMENTS: [EntityAttachment; 4] = [
    EntityAttachment::Passenger,
    EntityAttachment::Vehicle,
    EntityAttachment::NameTag,
    EntityAttachment::WardenChest,
];

/// Immutable attachment points. Native type defaults borrow extracted points;
/// scaling owns new points so repeated float-to-double multiplications keep
/// their original order rather than collapsing into a combined scale factor.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityAttachments {
    points: [Cow<'static, [Vec3]>; 4],
}
impl EntityAttachments {
    fn for_type(kind: EntityType) -> Self {
        Self {
            points: ATTACHMENTS
                .map(|attachment| Cow::Borrowed(kind.data().attachments.points(attachment))),
        }
    }
    pub fn create_default(_width: f32, height: f32) -> Self {
        Self {
            points: ATTACHMENTS.map(|attachment| {
                Cow::Owned(vec![match attachment {
                    EntityAttachment::Vehicle => Vec3::ZERO,
                    EntityAttachment::WardenChest => Vec3::new(0.0, f64::from(height) / 2.0, 0.0),
                    EntityAttachment::Passenger | EntityAttachment::NameTag => {
                        Vec3::new(0.0, f64::from(height), 0.0)
                    }
                }])
            }),
        }
    }
    pub fn points(&self, attachment: EntityAttachment) -> &[Vec3] {
        &self.points[attachment as usize]
    }
    pub fn scale(&self, x: f32, y: f32, z: f32) -> Self {
        Self {
            points: std::array::from_fn(|index| {
                Cow::Owned(
                    self.points[index]
                        .iter()
                        .map(|point| {
                            Vec3::new(
                                point.x * f64::from(x),
                                point.y * f64::from(y),
                                point.z * f64::from(z),
                            )
                        })
                        .collect(),
                )
            }),
        }
    }
    pub fn get_nullable(
        &self,
        attachment: EntityAttachment,
        index: i32,
        y_rot: f32,
    ) -> Option<Vec3> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.points(attachment).get(index))
            .map(|point| point.y_rot(-y_rot * 0.017_453_292_f32))
    }
    pub fn get(&self, attachment: EntityAttachment, index: i32, y_rot: f32) -> Vec3 {
        self.get_nullable(attachment, index, y_rot)
            .expect("native attachment point exists")
    }
    pub fn get_clamped(&self, attachment: EntityAttachment, index: i32, y_rot: f32) -> Vec3 {
        let points = self.points(attachment);
        assert!(!points.is_empty(), "native attachment list is not empty");
        let index = index.clamp(0, points.len() as i32 - 1);
        self.get(attachment, index, y_rot)
    }
    pub fn get_average(&self, attachment: EntityAttachment) -> Vec3 {
        let points = self.points(attachment);
        assert!(!points.is_empty(), "native attachment list is not empty");
        let sum = points.iter().fold(Vec3::ZERO, |sum, point| sum + *point);
        sum.scale(f64::from(1.0_f32 / points.len() as f32))
    }
    pub fn with_points(mut self, attachment: EntityAttachment, points: Vec<Vec3>) -> Self {
        self.points[attachment as usize] = Cow::Owned(points);
        self
    }
}

/// Port of EntityDimensions. Bounds and dimension scaling use float arithmetic;
/// attachment coordinates use the native double products of those float factors.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityDimensions {
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
    pub attachments: EntityAttachments,
    pub fixed: bool,
}
impl EntityDimensions {
    pub fn for_type(kind: EntityType) -> Self {
        let data = kind.data();
        Self {
            width: data.width,
            height: data.height,
            eye_height: data.eye_height,
            attachments: EntityAttachments::for_type(kind),
            fixed: data.fixed_dimensions,
        }
    }
    pub fn scalable(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            eye_height: height * 0.85,
            attachments: EntityAttachments::create_default(width, height),
            fixed: false,
        }
    }
    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            fixed: true,
            ..Self::scalable(width, height)
        }
    }
    pub fn with_eye_height(mut self, eye_height: f32) -> Self {
        self.eye_height = eye_height;
        self
    }
    pub fn with_attachments(mut self, attachments: EntityAttachments) -> Self {
        self.attachments = attachments;
        self
    }
    pub fn scale(&self, width: f32, height: f32) -> Self {
        if self.fixed || width == 1.0 && height == 1.0 {
            return self.clone();
        }
        Self {
            width: self.width * width,
            height: self.height * height,
            eye_height: self.eye_height * height,
            attachments: self.attachments.scale(width, height, width),
            fixed: false,
        }
    }
    pub fn make_bounding_box(&self, pos: Vec3) -> Aabb {
        let radius = f64::from(self.width / 2.0);
        Aabb::new(
            pos.x - radius,
            pos.y,
            pos.z - radius,
            pos.x + radius,
            pos.y + f64::from(self.height),
            pos.z + radius,
        )
    }
}

/// Port of Entity#setPose and its immediate onSyncedDataUpdated callback.
pub fn set_pose(entity: EntityRef, pose: Pose, level: &mut Level) -> Result<(), LevelError> {
    if level
        .entity_mut::<EntityData>(entity)?
        .entity_data
        .set(synched_data::POSE, pose)
    {
        let behavior = level.entity_behavior(entity)?;
        refresh_dimensions(behavior, entity, level)?;
    }
    Ok(())
}

/// Port of Entity#refreshDimensions. Size changes update the canonical bounds
/// before contextual collision queries, with the Player growth exception.
pub fn refresh_dimensions<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let pose = *level
        .entity::<EntityData>(entity)?
        .entity_data
        .get(synched_data::POSE);
    let dimensions = behavior.dimensions(entity, pose, level)?;
    let (previous, should_adjust) = {
        let base = level.entity_mut::<EntityData>(entity)?;
        let previous = std::mem::replace(&mut base.dimensions, dimensions);
        base.eye_height = base.dimensions.eye_height;
        base.set_pos(base.position);
        let small = base.dimensions.width <= 4.0 && base.dimensions.height <= 4.0;
        let grew =
            base.dimensions.width > previous.width || base.dimensions.height > previous.height;
        (
            previous,
            !base.first_tick
                && !base.no_physics
                && small
                && grew
                && base.entity_type != EntityType::PLAYER,
        )
    };
    if should_adjust {
        fudge_position_after_size_change(behavior, entity, &previous, level)?;
    }
    Ok(())
}

/// Port of Entity#fudgePositionAfterSizeChange, including the second candidate
/// that retains the previous vertical extent when both dimensions grew.
pub fn fudge_position_after_size_change<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    previous: &EntityDimensions,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let pose = *base.entity_data.get(synched_data::POSE);
    let center = base
        .position
        .add_xyz(0.0, f64::from(previous.height) / 2.0, 0.0);
    let dimensions = behavior.dimensions(entity, pose, level)?;
    let width_delta = f64::from((dimensions.width - previous.width).max(0.0)) + 1.0e-6;
    let height_delta = f64::from((dimensions.height - previous.height).max(0.0)) + 1.0e-6;
    let context =
        BlockCollisionContext::Entity(EntityCollisionView::capture(entity, level, false)?);
    let allowed = shapes::create_aabb(Aabb::of_size(
        center,
        width_delta,
        height_delta,
        width_delta,
    ));
    let size = Vec3::new(
        f64::from(dimensions.width),
        f64::from(dimensions.height),
        f64::from(dimensions.width),
    );
    if let Some(position) =
        super::collision::find_free_position_from_context(level, &context, &allowed, center, size)?
    {
        level.set_entity_pos(
            entity,
            position.add_xyz(0.0, -f64::from(dimensions.height) / 2.0, 0.0),
        )?;
        return Ok(true);
    }
    if dimensions.width > previous.width && dimensions.height > previous.height {
        let allowed = shapes::create_aabb(Aabb::of_size(center, width_delta, 1.0e-6, width_delta));
        let size = Vec3::new(
            f64::from(dimensions.width),
            f64::from(previous.height),
            f64::from(dimensions.width),
        );
        if let Some(position) = super::collision::find_free_position_from_context(
            level, &context, &allowed, center, size,
        )? {
            level.set_entity_pos(
                entity,
                position.add_xyz(0.0, -f64::from(previous.height) / 2.0 + 1.0e-6, 0.0),
            )?;
            return Ok(true);
        }
    }
    Ok(false)
}
