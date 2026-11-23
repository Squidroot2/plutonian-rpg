//! The two wagon parties share a route and carry their own pose and animation state.
use super::{CrossingGate, GATE_CENTER};
use crate::{
    dialogue::{DialogueRegistry, ReplyId, Speaker},
    dungeon::Area,
    inventory::{Inventory, ItemKind},
    loading::GameAssets,
    player::{AnimationLibrary, Player},
    quest::{MYCELIUM_GOAL, QuestAction, QuestJournal},
    terrain::{ground_heading, walkable_height},
    world::SessionEntity,
};
use avian3d::prelude::*;
use bevy::{
    gltf::{Gltf, GltfMaterialName},
    prelude::*,
    world_serialization::WorldInstanceReady,
};
use std::time::Duration;

const TOVAN: &str = "tovan";
const SHOW_MYCELIUM: &str = "show_mycelium";
const SAMPLES_NEEDED: ReplyId = ReplyId("samples_needed");
const SAMPLES_EXAMINED: ReplyId = ReplyId("samples_examined");
const ARRIVAL_DISTANCE_M: f32 = 20.0;
const CARAVAN_SPEED_MPS: f32 = 2.1;
const VELLIK_LEAD_M: f32 = 4.20;
const STAGING_Y: f32 = -1.46;
const SUPPORT_PROBE_Y: f32 = 2.0;
const SUPPORT_PROBE_RANGE_M: f32 = 5.0;

pub(crate) fn register_topics(handlers: &mut DialogueRegistry) {
    handlers.register(
        TOVAN,
        SHOW_MYCELIUM,
        |journal, _| journal.expects(QuestAction::ShowedTovanMycelium),
        &[SAMPLES_NEEDED, SAMPLES_EXAMINED],
        examine_samples,
    );
}

fn examine_samples(journal: &mut QuestJournal, inventory: &mut Inventory) -> ReplyId {
    if !journal.expects(QuestAction::ShowedTovanMycelium)
        || inventory.count(ItemKind::Mycelium) < MYCELIUM_GOAL
    {
        return SAMPLES_NEEDED;
    }
    journal.record(QuestAction::ShowedTovanMycelium);
    SAMPLES_EXAMINED
}

struct Villager {
    profile: &'static str,
    name: &'static str,
    color: Color,
    scale: f32,
    side: f32,
    kit: Vec3,
}

const VILLAGERS: [Villager; 4] = [
    Villager {
        profile: "isera",
        name: "Isera",
        color: Color::srgb(0.42, 0.27, 0.18),
        scale: 1.09,
        side: 1.0,
        kit: Vec3::new(0.40, 0.18, 0.12),
    },
    Villager {
        profile: "vey",
        name: "Vey",
        color: Color::srgb(0.22, 0.37, 0.37),
        scale: 0.97,
        side: -1.0,
        kit: Vec3::new(0.25, 0.31, 0.14),
    },
    Villager {
        profile: TOVAN,
        name: "Tovan",
        color: Color::srgb(0.37, 0.30, 0.23),
        scale: 1.03,
        side: -1.0,
        kit: Vec3::new(0.46, 0.20, 0.16),
    },
    Villager {
        profile: "orla",
        name: "Orla",
        color: Color::srgb(0.35, 0.24, 0.37),
        scale: 0.90,
        side: 1.0,
        kit: Vec3::new(0.26, 0.24, 0.12),
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arrival {
    Waiting,
    Moving,
    Parked,
}

#[derive(Component)]
pub(crate) struct Caravan {
    arrival: Arrival,
    distance: f32,
}

impl Caravan {
    pub(crate) fn crossed_threshold(&self) -> bool {
        self.distance >= 8.0
    }
}

#[derive(Component)]
pub(super) struct WagonUnit {
    index: usize,
    distance: f32,
    walking: bool,
}

#[derive(Component)]
pub(super) struct CaravanBody {
    size: Vec3,
    profile: Option<&'static str>,
}

#[derive(Component)]
struct ConvoyVisual;

#[derive(Component)]
struct VillagerVisual {
    wagon: Entity,
    coat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub(super) struct VillagerPlayback {
    wagon: Entity,
    walking: bool,
}

#[derive(Component)]
pub(super) struct MovingPart {
    wagon: Entity,
    rest_y: f32,
    kind: Mechanism,
}

enum Mechanism {
    Leg(f32),
    Crown,
    Wheel,
}

const PATHS: [[Vec2; 4]; 2] = [
    [
        Vec2::new(-30.0, 34.2),
        Vec2::new(-30.0, 24.1),
        Vec2::new(-31.3, 21.9),
        Vec2::new(-35.0, 19.1),
    ],
    [
        Vec2::new(-30.0, 42.8),
        Vec2::new(-30.0, 24.1),
        Vec2::new(-28.7, 21.9),
        Vec2::new(-25.0, 19.1),
    ],
];

fn route_pose(route: &[Vec2; 4], mut distance: f32) -> (Vec2, Vec3, f32, bool) {
    let traveled = distance;
    let mut total = 0.0;
    for [a, b] in route.array_windows::<2>().copied() {
        let edge = b - a;
        let length = edge.length();
        if distance < length {
            return (
                a + edge * (distance / length),
                Vec3::new(edge.x, 0.0, edge.y),
                traveled,
                false,
            );
        }
        distance -= length;
        total += length;
    }
    let delta = route[3] - route[2];
    (route[3], Vec3::new(delta.x, 0.0, delta.y), total, true)
}

pub(super) fn stage_caravan(
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
    gates: Query<&CrossingGate>,
    existing: Query<(), With<Caravan>>,
    journal: Res<QuestJournal>,
    spatial: SpatialQuery,
    players: Query<Entity, With<Player>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Some(assets_crossing) = &assets.crossing else {
        return;
    };
    if !existing.is_empty() || !gates.single().is_ok_and(CrossingGate::open) {
        return;
    }
    let resolve = |handle| {
        models
            .get(handle)
            .expect("validated caravan models must remain loaded")
    };
    let wagon = resolve(&assets_crossing.wagon).scenes[0].clone();
    let vellik = resolve(&assets_crossing.vellik).scenes[0].clone();
    let character = resolve(&assets.character);
    let parked = journal.has_occurred(QuestAction::CaravanArrived);
    let filter = SpatialQueryFilter::from_excluded_entities(players.iter());
    commands
        .spawn((
            SessionEntity,
            Name::new("Crossing caravan"),
            Transform::default(),
            Visibility::default(),
            Caravan {
                arrival: if parked {
                    Arrival::Parked
                } else {
                    Arrival::Waiting
                },
                distance: 0.0,
            },
        ))
        .with_children(|caravan| {
            for (index, route) in PATHS.iter().enumerate() {
                let (point, direction, distance, _) =
                    route_pose(route, if parked { f32::INFINITY } else { 0.0 });
                let unit =
                    Transform::from_translation(ground_pose(&spatial, &filter, point, STAGING_Y))
                        .with_rotation(ground_heading(direction));
                caravan
                    .spawn((
                        WagonUnit {
                            index,
                            distance,
                            walking: false,
                        },
                        unit,
                        Visibility::default(),
                    ))
                    .with_children(|unit| {
                        spawn_rig(
                            unit,
                            "Crossing wagon",
                            wagon.clone(),
                            Vec3::ZERO,
                            Vec3::new(3.16, 2.9, 3.8),
                            parked,
                        );
                        spawn_rig(
                            unit,
                            "Vellik",
                            vellik.clone(),
                            Vec3::Z * VELLIK_LEAD_M,
                            Vec3::new(1.95, 2.2, 3.1),
                            parked,
                        );
                        unit.spawn((
                            Name::new("Crossing wagon lantern"),
                            Transform::from_xyz(0.0, 2.55, -2.1),
                            PointLight {
                                color: Color::srgb(1.0, 0.57, 0.27),
                                intensity: 11_000.0,
                                range: 9.0,
                                ..default()
                            },
                        ));
                        for villager in &VILLAGERS[index * 2..index * 2 + 2] {
                            spawn_villager(
                                unit,
                                villager,
                                character.scenes[0].clone(),
                                &mut meshes,
                                &mut materials,
                            );
                        }
                    });
            }
        });
}

fn spawn_rig(
    unit: &mut ChildSpawnerCommands,
    name: &'static str,
    model: Handle<WorldAsset>,
    at: Vec3,
    size: Vec3,
    parked: bool,
) {
    let body = CaravanBody {
        size,
        profile: None,
    };
    let mut entity = unit.spawn((
        Name::new(name),
        Transform::from_translation(at),
        Visibility::default(),
        ConvoyVisual,
        WorldAssetRoot(model),
    ));
    if parked {
        enable_parked(&mut entity, &body);
    }
    entity.insert(body).observe(bind_moving_parts);
}

fn spawn_villager(
    unit: &mut ChildSpawnerCommands,
    villager: &Villager,
    model: Handle<WorldAsset>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let coat = materials.add(StandardMaterial {
        base_color: villager.color,
        perceptual_roughness: 0.91,
        ..default()
    });
    let kit_color = materials.add(StandardMaterial {
        base_color: Color::srgb(0.34, 0.24, 0.16),
        perceptual_roughness: 0.84,
        ..default()
    });
    let body = CaravanBody {
        size: Vec3::new(0.67, 1.66, 0.6),
        profile: Some(villager.profile),
    };
    let wagon = unit.target_entity();
    let mut entity = unit.spawn((
        Name::new(villager.name),
        Transform::from_xyz(villager.side * 2.38, 0.0, -0.2)
            .with_scale(Vec3::splat(villager.scale)),
        Visibility::default(),
    ));
    entity.insert(body).with_children(|villager_root| {
        villager_root
            .spawn((
                Transform::default(),
                WorldAssetRoot(model),
                VillagerVisual { wagon, coat },
            ))
            .observe(bind_villager);
        villager_root.spawn((
            Mesh3d(meshes.add(Cuboid::from_size(villager.kit))),
            MeshMaterial3d(kit_color),
            Transform::from_xyz(0.0, 0.92, -0.22),
        ));
    });
}

fn enable_parked(entity: &mut EntityCommands, body: &CaravanBody) {
    entity.insert((
        RigidBody::Static,
        Collider::compound(vec![(
            Vec3::Y * body.size.y * 0.5,
            Quat::IDENTITY,
            Collider::cuboid(body.size.x, body.size.y, body.size.z),
        )]),
    ));
    if let Some(profile) = body.profile {
        entity.insert(Speaker::new(profile, Area::Surface, 0.95));
    }
}

pub(super) fn move_caravan(
    mut caravan: Query<&mut Caravan>,
    mut wagons: Query<(&mut WagonUnit, &mut Transform)>,
    bodies: Query<(Entity, &CaravanBody)>,
    players: Query<(Entity, &Player, &Transform), Without<WagonUnit>>,
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mut journal: ResMut<QuestJournal>,
    mut commands: Commands,
) {
    let Ok(mut caravan) = caravan.single_mut() else {
        return;
    };
    let Ok((player_entity, player, player_at)) = players.single() else {
        return;
    };
    if caravan.arrival == Arrival::Parked {
        return;
    }
    if caravan.arrival == Arrival::Waiting {
        if player.area != Area::Surface
            || player_at.translation.xz().distance(GATE_CENTER.xz()) > ARRIVAL_DISTANCE_M
        {
            return;
        }
        caravan.arrival = Arrival::Moving;
    }
    caravan.distance += CARAVAN_SPEED_MPS * time.delta_secs();
    let mut all_parked = true;
    let filter = SpatialQueryFilter::from_excluded_entities([player_entity]);
    for (mut unit, mut transform) in &mut wagons {
        let (point, direction, distance, parked) = route_pose(&PATHS[unit.index], caravan.distance);
        if unit.distance == distance && !unit.walking {
            continue;
        }
        unit.distance = distance;
        unit.walking = !parked;
        transform.translation = ground_pose(&spatial, &filter, point, transform.translation.y);
        transform.rotation = transform.rotation.slerp(
            ground_heading(direction),
            (time.delta_secs() * 5.0).min(1.0),
        );
        if parked {
            transform.rotation = ground_heading(direction);
        }
        all_parked &= parked;
    }
    if all_parked {
        for (entity, body) in &bodies {
            if body.profile.is_none() {
                enable_parked(&mut commands.entity(entity), body);
            }
        }
        caravan.arrival = Arrival::Parked;
        journal.record(QuestAction::CaravanArrived);
    }
}

/// Parked speakers use world poses, as do other interactive actors.
pub(super) fn settle_villagers(
    caravan: Query<&Caravan>,
    wagons: Query<&Transform, With<WagonUnit>>,
    members: Query<(Entity, &CaravanBody, &ChildOf, &Transform), Without<Speaker>>,
    mut commands: Commands,
) {
    if !caravan
        .single()
        .is_ok_and(|caravan| caravan.arrival == Arrival::Parked)
    {
        return;
    }
    for (entity, body, parent, local) in &members {
        if body.profile.is_none() {
            continue;
        }
        let Ok(wagon) = wagons.get(parent.parent()) else {
            continue;
        };
        let mut world = wagon.mul_transform(*local);
        let outward = wagon.rotation * (Vec3::X * local.translation.x.signum());
        world.rotation = ground_heading(outward);
        let mut entity = commands.entity(entity);
        entity.remove::<ChildOf>().insert((SessionEntity, world));
        enable_parked(&mut entity, body);
    }
}

fn ground_pose(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    point: Vec2,
    fallback: f32,
) -> Vec3 {
    let probe = Vec3::new(point.x, SUPPORT_PROBE_Y, point.y);
    probe.with_y(
        walkable_height(spatial, filter, probe, SUPPORT_PROBE_RANGE_M, 0.0).unwrap_or(fallback),
    )
}

fn bind_moving_parts(
    ready: On<WorldInstanceReady>,
    visuals: Query<(), With<ConvoyVisual>>,
    parts: Query<(&Name, &Transform)>,
    parents: Query<&ChildOf>,
    wagons: Query<(), With<WagonUnit>>,
    hierarchy: Query<&Children>,
    mut commands: Commands,
) {
    if visuals.get(ready.entity).is_err() {
        return;
    }
    let Some(wagon) = parents
        .iter_ancestors(ready.entity)
        .find(|entity| wagons.contains(*entity))
    else {
        return;
    };
    for child in hierarchy.iter_descendants(ready.entity) {
        let Ok((name, transform)) = parts.get(child) else {
            continue;
        };
        let part = match name.as_str() {
            "Vellik Crown" => Mechanism::Crown,
            "Vellik Leg L0" | "Vellik Leg R1" | "Vellik Leg L2" => Mechanism::Leg(0.0),
            "Vellik Leg R0" | "Vellik Leg L1" | "Vellik Leg R2" => {
                Mechanism::Leg(std::f32::consts::PI)
            }
            name if name.starts_with("Wagon Wheel ") => Mechanism::Wheel,
            _ => continue,
        };
        commands.entity(child).insert(MovingPart {
            wagon,
            rest_y: transform.translation.y,
            kind: part,
        });
    }
}

fn bind_villager(
    ready: On<WorldInstanceReady>,
    costumes: Query<&VillagerVisual>,
    hierarchy: Query<&Children>,
    library: Res<AnimationLibrary>,
    mut players: Query<&mut AnimationPlayer>,
    mut coats: Query<(&GltfMaterialName, &mut MeshMaterial3d<StandardMaterial>)>,
    mut commands: Commands,
) {
    let Ok(costume) = costumes.get(ready.entity) else {
        return;
    };
    for child in hierarchy.iter_descendants(ready.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            let mut transition = AnimationTransitions::new();
            transition
                .play(&mut player, library.walking_node(false), Duration::ZERO)
                .repeat();
            commands.entity(child).insert((
                VillagerPlayback {
                    wagon: costume.wagon,
                    walking: false,
                },
                transition,
                AnimationGraphHandle(library.graph.clone()),
            ));
        }
        if let Ok((name, mut mat)) = coats.get_mut(child)
            && name.0 == "Woven basalt cloak"
        {
            mat.0 = costume.coat.clone();
        }
    }
}

pub(super) fn animate(
    wagons: Query<&WagonUnit>,
    time: Res<Time>,
    library: Res<AnimationLibrary>,
    mut parts: Query<(&MovingPart, &mut Transform)>,
    mut villagers: Query<(
        &mut VillagerPlayback,
        &mut AnimationTransitions,
        &mut AnimationPlayer,
    )>,
) {
    for (part, mut transform) in &mut parts {
        let Ok(wagon) = wagons.get(part.wagon) else {
            continue;
        };
        let (rotation, lift) = match part.kind {
            Mechanism::Leg(phase) if wagon.walking => {
                let stride = wagon.distance * 4.4 + phase;
                (stride.sin() * 0.27, stride.cos().max(0.0) * 0.12)
            }
            Mechanism::Leg(_) => (0.0, 0.0),
            Mechanism::Crown => (
                (time.elapsed_secs() * 1.55).sin() * if wagon.walking { 0.135 } else { 0.052 },
                0.0,
            ),
            Mechanism::Wheel => (wagon.distance / 0.63, 0.0),
        };
        transform.rotation = Quat::from_rotation_x(rotation);
        transform.translation.y = part.rest_y + lift;
    }
    for (mut playback, mut transitions, mut player) in &mut villagers {
        let Ok(wagon) = wagons.get(playback.wagon) else {
            continue;
        };
        let walking = wagon.walking;
        if playback.walking != walking {
            playback.walking = walking;
            transitions
                .play(
                    &mut player,
                    library.walking_node(walking),
                    Duration::from_millis(150),
                )
                .repeat();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quest::{CONTAIN_THE_HUSHCAPS, test_relay_journal};

    #[test]
    fn the_craftsman_examines_five_carried_samples_once_without_taking_them() {
        let mut journal = test_relay_journal();
        journal.offer_cavern_report();
        journal.record(QuestAction::ReportedArchiveHushcap);
        journal.record(QuestAction::DefeatedFiveCavernHushcaps);
        let mut inventory = Inventory::default();
        assert!(inventory.add(ItemKind::Mycelium, 4));
        assert_eq!(
            examine_samples(&mut journal, &mut inventory),
            SAMPLES_NEEDED
        );
        assert!(!journal.is_complete(CONTAIN_THE_HUSHCAPS));
        assert!(inventory.add(ItemKind::Mycelium, 1));
        assert_eq!(
            examine_samples(&mut journal, &mut inventory),
            SAMPLES_EXAMINED
        );
        assert_eq!(inventory.count(ItemKind::Mycelium), 5);
        assert!(journal.is_complete(CONTAIN_THE_HUSHCAPS));
        assert_eq!(journal.take_completion_xp(), 150);
        assert_eq!(
            examine_samples(&mut journal, &mut inventory),
            SAMPLES_NEEDED
        );
        assert_eq!(journal.take_completion_xp(), 0);
    }
}
