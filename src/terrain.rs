//! Shared ground movement probes for actors that walk on authored terrain.

use avian3d::prelude::*;
use bevy::prelude::*;

pub(crate) const MAX_GROUND_STEP: f32 = 0.25;
const MIN_WALKABLE_NORMAL_Y: f32 = 0.65;

/// Find walkable support and return the actor origin height above it.
pub(crate) fn walkable_height(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    origin: Vec3,
    range: f32,
    ground_offset: f32,
) -> Option<f32> {
    spatial
        .cast_ray(origin, Dir3::NEG_Y, range, true, filter)
        .filter(|hit| hit.normal.y > MIN_WALKABLE_NORMAL_Y)
        .map(|hit| origin.y - hit.distance + ground_offset)
}

/// Rotate a model whose forward axis is +Z toward a horizontal direction.
pub(crate) fn ground_heading(delta: Vec3) -> Quat {
    Quat::from_rotation_y(delta.x.atan2(delta.z))
}
