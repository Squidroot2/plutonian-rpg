//! Check the shipped basin terrain and the walk-in cave with the normal controller.
use super::{
    SmokeCommands, SmokeRun, capture,
    inventory::{click, walk},
};
use crate::{
    GameState,
    camera::{FollowCamera, Orbit},
    cave::{CHAMBER_FLOOR_Y, surface_location},
    combat::{ConfirmedHit, Dead, Hit},
    enemy::{Enemy, ranged::SpitProjectile},
    experience::Experience,
    hushcap::Hushcap,
    interaction::{CorpseLoot, InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind},
    player::PlayerRead,
    player::{PLAYER_GROUND_OFFSET, Player},
    save::EncounterId,
    terrain::walkable_height,
    ui::loot_ui::LootAction,
    world::WorldLayout,
};
use avian3d::prelude::*;
use bevy::{app::AppExit, prelude::*, render::view::screenshot::Screenshot};

const OUTBOUND: [(f32, f32); 14] = [
    (6.0, -2.7),
    (4.0, 7.0),
    (0.0, 20.0),
    (-7.0, 35.0),
    (-12.5, 46.0),
    (-14.0, 57.0),
    (-15.0, 72.0),
    (-13.0, 77.5),
    (8.0, 78.5),
    (14.0, 80.7),
    (14.0, 86.0),
    (13.0, 90.5),
    (11.7, 95.0),
    (13.0, 99.2),
];
const VILLAGE_VIEW: usize = 1;
const BEND_VIEW: usize = 7;
const REVEAL: usize = 9;
const FIRST_HUSHCAP: EncounterId =
    EncounterId::Added(crate::content::ContentId::new("cave_first_west_hushcap"));
const LOOT_APPROACH: Vec3 = Vec3::new(9.6, 0.0, 102.8);

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaveCheck {
    #[default]
    Begin,
    Approach(usize),
    GuideView(Option<Entity>),
    BendView(Option<Entity>),
    Reveal(Option<Entity>),
    Interior(Option<Entity>),
    WatchSpit,
    ApproachCorpse,
    TakeLoot,
    CloseLoot,
    Return(usize),
}

pub(super) fn check_geometry(
    spatial: SpatialQuery,
    actors: Query<Entity, Or<(With<Enemy>, With<Player>)>>,
    player: Query<&Collider, With<Player>>,
    encounters: Query<(&EncounterId, &Transform, &Collider)>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    placements: Res<crate::content::Placements>,
) {
    let filter = SpatialQueryFilter::from_excluded_entities(actors.iter());
    let capsule = player
        .single()
        .expect("the cave smoke player has a capsule");
    check_entrance_views(&spatial, &filter);
    check_cover(&spatial, &filter, &names, &parents, &placements);
    for entry in placements
        .encounters()
        .filter(|entry| entry.id.as_str().starts_with("cave_"))
    {
        let id = entry.id;
        let home = Vec3::from_array(entry.position);
        let (_, pose, body) = encounters
            .iter()
            .find(|(encounter, ..)| **encounter == EncounterId::Added(id))
            .expect("each authored cave encounter has a body");
        assert!(
            pose.translation.distance(home) < 0.08,
            "{id:?} is displaced from its home"
        );
        let ground = walkable_height(&spatial, &filter, home + Vec3::Y * 2.0, 4.0, 0.0);
        assert!((ground.expect("encounter has floor") - CHAMBER_FLOOR_Y).abs() < 0.02);
        assert!(
            spatial
                .shape_intersections(body, home + Vec3::Y * 0.10, Quat::IDENTITY, &filter)
                .is_empty(),
            "{id:?} intersects cave terrain at {home}"
        );
    }
    let first = [(11.0, 105.1), (13.0, 111.0), (13.0, 119.0)];
    let west = [
        (13.0, 119.0),
        (6.0, 122.0),
        (6.4, 130.0),
        (8.0, 133.0),
        (13.0, 137.0),
    ];
    let east = [(13.0, 119.0), (19.0, 122.5), (19.0, 131.0), (13.0, 137.0)];
    let gallery = [
        (13.0, 137.0),
        (12.0, 141.0),
        (10.0, 144.0),
        (8.0, 147.0),
        (8.0, 152.0),
        (14.0, 154.0),
        (15.0, 158.0),
    ];
    for route in [first.as_slice(), &west, &east, &gallery] {
        check_route(&spatial, &filter, capsule, route, CHAMBER_FLOOR_Y, 0.05);
    }
    let east_basin = [
        (-7.0, 35.0),
        (6.0, 37.0),
        (9.5, 47.0),
        (8.0, 55.7),
        (-8.0, 55.7),
        (-14.0, 57.0),
    ];
    check_route(&spatial, &filter, capsule, &east_basin, -1.9, 0.8);
}

fn check_entrance_views(spatial: &SpatialQuery, filter: &SpatialQueryFilter) {
    let mouth_samples = [
        Vec3::new(14.0, 0.0, 84.0),
        Vec3::new(10.9, -0.9, 84.0),
        Vec3::new(17.1, -0.9, 84.0),
        Vec3::new(14.0, 2.45, 84.0),
    ];
    let mut origins = vec![
        WorldLayout::Outpost.initial_spawn(),
        WorldLayout::Outpost.start(),
    ];
    for x in [-17.0, 0.0, 17.0] {
        for z in [-8.0, 4.0, 17.0] {
            origins.push(Vec3::new(x, 0.1, z));
        }
    }
    for origin in origins {
        // The outer samples enclose the orbit's eight-meter horizontal reach.
        for offset in [
            Vec3::Y,
            Vec3::new(-8.0, 2.0, 0.0),
            Vec3::new(8.0, 2.0, 0.0),
            Vec3::new(0.0, 2.0, -8.0),
            Vec3::new(0.0, 2.0, 8.0),
            Vec3::new(0.0, 9.0, 0.0),
        ] {
            let origin = origin + offset;
            for target in mouth_samples {
                let delta = target - origin;
                assert!(
                    spatial
                        .cast_ray(
                            origin,
                            Dir3::new(delta).expect("the mouth is away from spawn"),
                            delta.length(),
                            true,
                            filter
                        )
                        .is_some(),
                    "cave mouth is visible from {origin} toward {target}"
                );
            }
        }
    }
    let reveal = Vec3::new(14.0, 0.10, 79.0);
    let delta = mouth_samples[0] - reveal;
    assert!(
        spatial
            .cast_ray(
                reveal,
                Dir3::new(delta).expect("the mouth is beyond the bend"),
                delta.length(),
                true,
                filter
            )
            .is_none(),
        "the cave mouth should be visible after rounding the basin ridge"
    );
}

fn check_cover(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    names: &Query<&Name>,
    parents: &Query<&ChildOf>,
    placements: &crate::content::Placements,
) {
    let entry = placements
        .encounters()
        .find(|entry| EncounterId::Added(entry.id) == FIRST_HUSHCAP)
        .expect("the first Hushcap has an authored home");
    let home = Vec3::from_array(entry.position);
    let origin = home + Vec3::Y * super::SHIPPED_HUSHCAP_MUZZLE_HEIGHT_M;
    let delta = Vec3::new(15.3, -4.6, 102.5) - origin;
    let cover = spatial
        .cast_shape(
            &Collider::sphere(super::SHIPPED_HUSHCAP_SPIT_RADIUS_M),
            origin,
            Quat::IDENTITY,
            Dir3::new(delta).expect("the cover is away from the Hushcap"),
            &ShapeCastConfig::from_max_distance(delta.length()),
            filter,
        )
        .expect("the entrance rock must intercept the first Hushcap's shot");
    assert!(
        std::iter::once(cover.entity)
            .chain(parents.iter_ancestors(cover.entity))
            .any(|node| names
                .get(node)
                .is_ok_and(|name| name.as_str() == "Entrance cover")),
        "the entrance rock must be the first thing that blocks the shot"
    );
}

pub(super) fn check_route(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    capsule: &Collider,
    route: &[(f32, f32)],
    expected_floor: f32,
    floor_tolerance: f32,
) {
    for pair in route.windows(2) {
        let a = Vec2::from(pair[0]);
        let b = Vec2::from(pair[1]);
        let steps = (a.distance(b) / 0.45).ceil() as u32;
        for i in 0..=steps {
            let p = a.lerp(b, i as f32 / steps as f32);
            let origin = Vec3::new(p.x, expected_floor + 2.5, p.y);
            let feet = walkable_height(spatial, filter, origin, 4.0, 0.0)
                .unwrap_or_else(|| panic!("missing floor below {origin}"));
            assert!(
                (feet - expected_floor).abs() < floor_tolerance,
                "unexpected floor height {feet} below {origin}"
            );
            let center = origin.with_y(feet + PLAYER_GROUND_OFFSET + 0.10);
            assert!(
                spatial
                    .shape_intersections(capsule, center, Quat::IDENTITY, filter)
                    .is_empty(),
                "walking route is blocked at {center}"
            );
        }
    }
}

fn advance_approach(index: usize, position: Vec3, orbit: &mut Orbit) -> CaveCheck {
    if index == VILLAGE_VIEW {
        let to_guide = crate::cave::GUIDE_MUSHROOM_CAP_POS.xz() - position.xz();
        // Offset yaw slightly so the player's silhouette sits beside the distant mushroom.
        orbit.yaw = (-to_guide.x).atan2(-to_guide.y) + 0.14;
        orbit.distance = 4.5;
        orbit.pitch = 0.08;
        CaveCheck::GuideView(None)
    } else if index == BEND_VIEW {
        assert_eq!(surface_location(position), Some("SOUTH BASIN"));
        let to_mouth = Vec2::new(14.0, 84.0) - position.xz();
        orbit.yaw = (-to_mouth.x).atan2(-to_mouth.y);
        orbit.distance = 5.0;
        orbit.pitch = 0.14;
        CaveCheck::BendView(None)
    } else if index == REVEAL {
        assert_eq!(surface_location(position), Some("SOUTH BASIN"));
        orbit.yaw = std::f32::consts::PI;
        orbit.distance = 3.8;
        orbit.pitch = 0.30;
        CaveCheck::Reveal(None)
    } else if index + 1 == OUTBOUND.len() {
        assert_eq!(surface_location(position), Some("HUSHCAP CAVERNS"));
        orbit.yaw = std::f32::consts::PI - 0.40;
        CaveCheck::Interior(None)
    } else {
        CaveCheck::Approach(index + 1)
    }
}

fn finish_capture(orbit: &mut Orbit, next: CaveCheck) -> CaveCheck {
    orbit.distance = Orbit::default().distance;
    orbit.pitch = Orbit::default().pitch;
    next
}

pub(super) fn drive(
    mut phase: ResMut<CaveCheck>,
    run: Res<SmokeRun>,
    state: Res<State<GameState>>,
    control: (ResMut<ButtonInput<KeyCode>>, ResMut<Orbit>),
    player: Query<(Entity, &Transform, &Player, &Experience)>,
    caps: Query<(Entity, &EncounterId, &CorpseLoot, Has<Dead>), With<Hushcap>>,
    view: (
        Query<Entity, With<Window>>,
        Query<Entity, With<FollowCamera>>,
        Query<(), With<Screenshot>>,
    ),
    projectiles: Query<(), With<SpitProjectile>>,
    combat: (MessageReader<ConfirmedHit>, MessageWriter<Hit>),
    loot: (
        PlayerRead<Inventory>,
        Res<Nearby>,
        Query<(Entity, &LootAction)>,
    ),
    mut commands: SmokeCommands,
    mut exit: MessageWriter<AppExit>,
) {
    let (mut keys, mut orbit) = control;
    let (window, camera, captures) = view;
    let (mut confirmed, mut hits) = combat;
    let (inventory, nearby, actions) = loot;
    let (player, pose, controller, xp) = player.single().expect("cave smoke player");
    let position = pose.translation;
    let previous = *phase;
    let (cap, _, corpse, dead) = caps
        .iter()
        .find(|(_, id, ..)| **id == FIRST_HUSHCAP)
        .expect("first cave Hushcap");
    for key in [
        KeyCode::KeyW,
        KeyCode::ShiftLeft,
        KeyCode::KeyE,
        KeyCode::Escape,
    ] {
        keys.release(key);
    }

    match *phase {
        CaveCheck::Begin if controller.grounded => {
            assert!(
                position
                    .xz()
                    .distance(WorldLayout::Outpost.initial_spawn().xz())
                    < 0.2
            );
            *phase = CaveCheck::Approach(0);
        }
        CaveCheck::Approach(index) => {
            if reached(&mut keys, &mut orbit, position, waypoint(index)) {
                *phase = advance_approach(index, position, &mut orbit);
            }
        }
        CaveCheck::GuideView(None) => {
            *phase = CaveCheck::GuideView(Some(capture(&mut commands, "south-basin-guide")));
        }
        CaveCheck::GuideView(Some(image)) if captures.get(image).is_err() => {
            *phase = finish_capture(&mut orbit, CaveCheck::Approach(VILLAGE_VIEW + 1));
        }
        CaveCheck::BendView(None) => {
            *phase = CaveCheck::BendView(Some(capture(&mut commands, "south-basin-bend")));
        }
        CaveCheck::BendView(Some(image)) if captures.get(image).is_err() => {
            *phase = finish_capture(&mut orbit, CaveCheck::Approach(BEND_VIEW + 1));
        }
        CaveCheck::Reveal(None) => {
            *phase = CaveCheck::Reveal(Some(capture(&mut commands, "south-basin-entrance")));
        }
        CaveCheck::Reveal(Some(image)) if captures.get(image).is_err() => {
            *phase = finish_capture(&mut orbit, CaveCheck::Approach(REVEAL + 1));
        }
        CaveCheck::Interior(None) => {
            *phase = CaveCheck::Interior(Some(capture(&mut commands, "hushcap-caverns")));
        }
        CaveCheck::Interior(Some(image)) if captures.get(image).is_err() => {
            *phase = CaveCheck::WatchSpit;
        }
        CaveCheck::WatchSpit
            if !projectiles.is_empty()
                || confirmed
                    .read()
                    .any(|hit| hit.source == cap && hit.target == player) =>
        {
            // Resolve a deterministic finishing blow so the scenario can exercise the loot UI.
            hits.write(Hit {
                source: player,
                target: cap,
                damage: super::SHIPPED_HUSHCAP_HEALTH,
                contact: None,
            });
            *phase = CaveCheck::ApproachCorpse;
        }
        CaveCheck::ApproachCorpse if dead => {
            assert_eq!(xp.0, u64::from(super::SHIPPED_HUSHCAP_XP));
            assert_eq!(corpse.stack.expect("Hushcap loot").item, ItemKind::Mycelium);
            if nearby.0 == Some((cap, InteractionTarget::Corpse)) {
                keys.press(KeyCode::KeyE);
                *phase = CaveCheck::TakeLoot;
            } else {
                reached(&mut keys, &mut orbit, position, LOOT_APPROACH);
            }
        }
        CaveCheck::TakeLoot if *state.get() == GameState::Looting => {
            assert_eq!(inventory.count(ItemKind::Mycelium), 0);
            click(
                &mut commands.ecs,
                LootAction::Take,
                &actions,
                window.single().expect("cave smoke window"),
                camera.single().expect("cave smoke camera"),
            );
            *phase = CaveCheck::CloseLoot;
        }
        CaveCheck::CloseLoot if inventory.count(ItemKind::Mycelium) == 1 => {
            assert!(corpse.stack.is_none());
            keys.press(KeyCode::Escape);
            *phase = CaveCheck::Return(OUTBOUND.len() - 1);
        }
        CaveCheck::Return(index)
            if *state.get() == GameState::Playing
                && reached(&mut keys, &mut orbit, position, waypoint(index)) =>
        {
            if index > 1 {
                *phase = CaveCheck::Return(index - 1);
            } else {
                info!(
                    "CAVE SMOKE PASS: basin approach, concealed mouth, chamber routes, spit cover and loot ({:.2}s)",
                    run.0.elapsed().as_secs_f32()
                );
                exit.write(AppExit::Success);
            }
        }
        _ => {}
    }
    if *phase != previous {
        info!("CAVE SMOKE: {:?} at {position:?}", *phase);
    }
}

fn waypoint(index: usize) -> Vec3 {
    let (x, z) = OUTBOUND[index];
    Vec3::new(x, 0.0, z)
}

fn reached(keys: &mut ButtonInput<KeyCode>, orbit: &mut Orbit, position: Vec3, goal: Vec3) -> bool {
    if position.xz().distance(goal.xz()) < 0.38 {
        true
    } else {
        walk(keys, orbit, position, goal);
        false
    }
}
