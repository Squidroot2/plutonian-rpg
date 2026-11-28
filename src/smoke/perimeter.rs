//! Probe the perimeter in isolation so existing terrain cannot hide a gap.
use crate::player::Player;
use avian3d::prelude::*;
use az::CheckedAs;
use bevy::prelude::*;

// Follow the inward faces through the main shelf, the shoulder and South Basin.
const EAST_RIM: &[(f32, f32)] = &[
    (-37.3, 27.0),
    (-46.0, 27.0),
    (-46.5, 8.0),
    (-45.5, -18.0),
    (-47.0, -48.0),
    (-55.0, -53.0),
    (-62.0, -60.0),
    (-63.0, -70.0),
    (-57.0, -79.0),
    (-47.0, -85.0),
    (-46.0, -94.0),
    (-47.0, -108.0),
    (-46.0, -125.0),
    (-40.0, -130.7),
    (-13.0, -130.8),
    (13.0, -130.8),
    (40.0, -130.7),
    (46.0, -125.0),
    (47.0, -104.0),
    (46.5, -78.0),
    (47.0, -50.0),
    (46.0, -22.0),
    (47.0, 9.0),
    (46.0, 27.0),
    (19.3, 27.0),
    (20.2, 30.0),
    (26.3, 41.0),
    (30.2, 51.0),
    (30.2, 62.0),
    (29.2, 74.0),
    (27.2, 83.85),
    (18.8, 83.85),
];
const WEST_RIM: &[(f32, f32)] = &[
    (9.2, 83.85),
    (-15.3, 83.85),
    (-23.2, 74.0),
    (-26.2, 62.0),
    (-28.2, 51.0),
    (-25.2, 41.0),
    (-20.2, 30.0),
    (-19.3, 27.0),
    (-22.7, 27.0),
];

pub(super) fn check_geometry(
    spatial: SpatialQuery,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    colliders: Query<Entity, With<Collider>>,
    player: Query<&Collider, With<Player>>,
) {
    let capsule = player
        .single()
        .expect("the world smoke player has a capsule");
    let filter = SpatialQueryFilter::from_excluded_entities(colliders.iter().filter(|entity| {
        !std::iter::once(*entity)
            .chain(parents.iter_ancestors(*entity))
            .any(|ancestor| {
                names
                    .get(ancestor)
                    .is_ok_and(|name| name.as_str().starts_with("Perimeter solid "))
            })
    }));

    for path in [EAST_RIM, WEST_RIM] {
        for segment in path.windows(2) {
            let start = Vec2::from(segment[0]);
            let end = Vec2::from(segment[1]);
            let edge = end - start;
            let outward = Dir3::new(Vec3::new(edge.y, 0.0, -edge.x))
                .expect("perimeter segments have positive length");
            let steps = edge
                .length()
                .ceil()
                .checked_as::<u32>()
                .expect("perimeter edges are finite and tens of metres long");
            for step in 0..=steps {
                // CheckedAs checks range, not precision; edge step counts are below 2^24.
                let point = start.lerp(
                    end,
                    step.checked_as::<f32>()
                        .expect("perimeter step is bounded by the short edge length")
                        / steps
                            .checked_as::<f32>()
                            .expect("perimeter step count is the short edge length"),
                );
                for y in [-1.5, 2.0, 4.5] {
                    assert_blocked(
                        &spatial,
                        capsule,
                        &filter,
                        Vec3::new(point.x, y, point.y),
                        outward,
                    );
                }
                // High above a bend the crest shifts outward along both faces.
                // Sample the straight stretches where a perpendicular cast crosses the crest.
                if point.distance(start).min(point.distance(end)) > 4.0 {
                    assert_blocked(
                        &spatial,
                        capsule,
                        &filter,
                        Vec3::new(point.x, 16.5, point.y),
                        outward,
                    );
                }
            }
        }
    }

    for x in -75..=-45 {
        for y in [-1.35, 1.3, 4.5] {
            assert_blocked(
                &spatial,
                capsule,
                &filter,
                // CheckedAs checks range, not precision; -75..=-45 is exactly representable.
                Vec3::new(
                    x.checked_as::<f32>()
                        .expect("perimeter sample x is between -75 and -45")
                        * 0.5,
                    y,
                    27.0,
                ),
                Dir3::Z,
            );
        }
    }
    for at in [Vec3::new(-30.0, 8.5, 27.0), Vec3::new(14.0, 4.5, 83.85)] {
        assert_blocked(&spatial, capsule, &filter, at, Dir3::Z);
    }
    assert!(
        spatial
            .cast_shape(
                capsule,
                Vec3::new(14.0, -0.75, 82.0),
                Quat::IDENTITY,
                Dir3::Z,
                &ShapeCastConfig::from_max_distance(3.0),
                &filter,
            )
            .is_none(),
        "the perimeter must leave the cave throat open",
    );
    info!("PERIMETER COLLISION PASS: cliffs, gate, and cave headwall seal the shelf");
}

pub(super) fn assert_blocked(
    spatial: &SpatialQuery,
    capsule: &Collider,
    filter: &SpatialQueryFilter,
    at: Vec3,
    outward: Dir3,
) {
    let origin = at - *outward * 2.0;
    assert!(
        spatial
            .cast_shape(
                capsule,
                origin,
                Quat::IDENTITY,
                outward,
                &ShapeCastConfig::from_max_distance(6.0),
                filter,
            )
            .is_some(),
        "missing perimeter collision at {at} heading {outward:?}",
    );
}
