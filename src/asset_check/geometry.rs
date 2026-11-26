use super::{Bounds, Model, Transform};
use bevy::math::{Mat4, Quat, Vec3};

impl Transform {
    pub(super) fn matrix(&self) -> Mat4 {
        match self {
            Self::Matrix(matrix) => Mat4::from_cols_array_2d(matrix),
            Self::Trs {
                translation,
                rotation,
                scale,
            } => trs(*translation, *rotation, *scale),
        }
    }

    pub(super) fn translation(&self) -> Vec3 {
        self.matrix().w_axis.truncate()
    }
}

pub(super) fn trs(translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        scale.into(),
        Quat::from_array(rotation),
        translation.into(),
    )
}

pub(super) fn world_bounds(model: &Model) -> Result<Bounds, &'static str> {
    let [roots] = model.scenes.as_slice() else {
        return Err("expected one scene for model bounds");
    };
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut visited = vec![false; model.nodes.len()];
    let mut pending: Vec<_> = roots.iter().map(|&node| (node, Mat4::IDENTITY)).collect();
    while let Some((index, parent)) = pending.pop() {
        if std::mem::replace(&mut visited[index], true) {
            return Err("scene hierarchy revisits a node");
        }
        let node = &model.nodes[index];
        let world = parent * node.transform.matrix();
        if let Some(mesh) = node.mesh {
            for bounds in &model.meshes[mesh] {
                // A rotated primitive's local AABB needs all eight corners.
                for x in [bounds.min[0], bounds.max[0]] {
                    for y in [bounds.min[1], bounds.max[1]] {
                        for z in [bounds.min[2], bounds.max[2]] {
                            let point = world.transform_point3(Vec3::new(x, y, z));
                            min = min.min(point);
                            max = max.max(point);
                        }
                    }
                }
            }
        }
        pending.extend(node.children.iter().map(|&child| (child, world)));
    }
    if !min.is_finite() || !max.is_finite() {
        return Err("scene has no readable mesh bounds");
    }
    Ok(Bounds {
        min: min.into(),
        max: max.into(),
    })
}
