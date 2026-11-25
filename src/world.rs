use crate::{
    GameState,
    attributes::{AttributeId, AttributeRanks, CharacterProgress, SavedProgress},
    camera::Orbit,
    content::{Items, NpcId, Npcs},
    equipment::bind_equipment_attachments,
    experience::Experience,
    interaction::Target,
    inventory::ItemKind,
    loading::{GameAssets, LoadStatus, PendingScene, scene_ready, spawn_static_scenes},
    nemi::{self, Nemi},
    player::{
        ActorVisual, PLAYER_COLLIDER_LENGTH, PLAYER_COLLIDER_RADIUS, PLAYER_GROUND_OFFSET, Player,
        bind_actor_animation,
    },
    quest::Beacon,
    save::{Bed, RunSave},
};
use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*};
#[cfg(feature = "smoke")]
use std::time::Instant;

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum WorldLayout {
    #[default]
    Outpost,
    SmokeArena,
}

impl WorldLayout {
    pub(crate) fn recovery_x(self) -> std::ops::RangeInclusive<f32> {
        const EAST_RECOVERY_X: f32 = 55.0;
        match self {
            Self::Outpost => crate::crossing::WEST_RECOVERY_X..=EAST_RECOVERY_X,
            Self::SmokeArena => -EAST_RECOVERY_X..=EAST_RECOVERY_X,
        }
    }

    pub(crate) fn recovery_max_z(self) -> f32 {
        const EDGE_MARGIN_M: f32 = 7.0;
        match self {
            Self::Outpost => crate::cave::FAR_WALL_Z + EDGE_MARGIN_M,
            Self::SmokeArena => 35.0,
        }
    }

    pub fn start(self) -> Vec3 {
        match self {
            Self::Outpost => Vec3::new(4.0, 0.5, 8.0),
            Self::SmokeArena => Vec3::Y * PLAYER_GROUND_OFFSET,
        }
    }
    pub(crate) fn initial_spawn(self) -> Vec3 {
        match self {
            Self::Outpost => Vec3::new(9.7, 0.08, -2.72),
            Self::SmokeArena => self.start(),
        }
    }
    pub fn keeper(self) -> Vec3 {
        match self {
            Self::Outpost => Vec3::new(3.0, -0.95, -1.0),
            Self::SmokeArena => Vec3::new(-4.0, 0.0, 5.0),
        }
    }
    pub fn core(self) -> Vec3 {
        match self {
            Self::Outpost => Vec3::new(0.0, 2.3, -122.0),
            Self::SmokeArena => Vec3::new(-1.0, 0.445, 5.0),
        }
    }
    pub fn dummy(self) -> Vec3 {
        match self {
            Self::Outpost => Vec3::new(6.0, 2.0, 11.0),
            Self::SmokeArena => Vec3::new(6.0, 2.0, 4.0),
        }
    }
    fn beacon(self) -> Vec3 {
        match self {
            Self::Outpost => Vec3::new(0.0, 7.5, 5.0),
            Self::SmokeArena => Vec3::new(-4.0, 2.8, 5.0),
        }
    }
}

pub struct WorldPlugin;
/// A world-session root that is despawned when starting or restoring a run.
#[derive(Component)]
pub(crate) struct SessionEntity;

/// Identifies the active desktop smoke run, when one is in progress.
/// The pause system checks for this resource so a smoke run is never
/// auto-paused by a window focus change.
#[derive(Resource)]
pub(crate) struct SmokeRun(#[cfg(feature = "smoke")] pub(crate) Instant);

/// The arena floor spawned in place of the outpost in the smoke arena layout.
#[derive(Component)]
pub(crate) struct ArenaFloor;

pub(crate) const ARENA_BOULDER_SITES: [Vec3; 2] =
    [Vec3::new(2.5, 0.0, -2.0), Vec3::new(2.5, 0.0, 2.0)];

/// Loot placed around the smoke arena, as (item, x, z) triples.
pub(crate) const ARENA_LOOT: [(ItemKind, f32, f32); 14] = [
    (ItemKind::MethanePouch, -2.0, 0.0),
    (ItemKind::Scrap, -3.0, 1.3),
    (ItemKind::Battery, -4.0, 0.0),
    (ItemKind::Battery, -5.0, 1.3),
    (ItemKind::FieldSpear, 4.0, 0.0),
    (ItemKind::Trident, 7.5, 1.0),
    (ItemKind::Shortsword, 4.0, 2.5),
    (ItemKind::Shortsword, 7.5, 2.7),
    (ItemKind::FieldArmor, -3.0, -4.0),
    (ItemKind::FieldHelmet, 0.0, -4.0),
    (ItemKind::ReinforcedArmor, 3.0, -4.0),
    (ItemKind::ReinforcedHelmet, 6.0, -4.0),
    (
        ItemKind::LargeBoulder,
        ARENA_BOULDER_SITES[0].x,
        ARENA_BOULDER_SITES[0].z,
    ),
    (
        ItemKind::LargeBoulder,
        ARENA_BOULDER_SITES[1].x,
        ARENA_BOULDER_SITES[1].z,
    ),
];

pub(crate) const ARENA_WALL: Vec3 = Vec3::new(0.0, 1.0, 9.0);

/// Spawn the standalone arena floor and wall used by the smoke arena layout
/// when no outpost scene is loaded.
pub(crate) fn spawn_arena(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let floor = Vec3::new(28.0, 1.0, 28.0);
    commands.spawn((
        SessionEntity,
        ArenaFloor,
        Name::new("Smoke arena floor"),
        Mesh3d(meshes.add(Cuboid::from_size(floor))),
        MeshMaterial3d(materials.add(Color::srgb(0.16, 0.23, 0.32))),
        RigidBody::Static,
        Collider::cuboid(floor.x, floor.y, floor.z),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    let wall = Vec3::new(4.0, 2.0, 0.4);
    commands.spawn((
        SessionEntity,
        Name::new("Smoke arena wall"),
        Mesh3d(meshes.add(Cuboid::from_size(wall))),
        MeshMaterial3d(materials.add(Color::srgb(0.4, 0.24, 0.12))),
        RigidBody::Static,
        Collider::cuboid(wall.x, wall.y, wall.z),
        Transform::from_translation(ARENA_WALL),
    ));
}

#[derive(Event)]
pub struct SpawnWorld;
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldLayout>()
            .add_observer(spawn_world)
            .add_systems(Update, configure_sky_meshes)
            .add_systems(OnExit(GameState::Loading), ground_spawned_models);
    }
}
fn spawn_world(
    _: On<SpawnWorld>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    layout: Res<WorldLayout>,
    run: Res<RunSave>,
    new_name: Res<crate::player::NewPlayerName>,
    items: Res<Items>,
    npcs: Res<Npcs>,
    mut orbit: ResMut<Orbit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if let Some(outpost) = &assets.outpost {
        orbit.yaw = std::f32::consts::FRAC_PI_2;
        spawn_outpost(&mut commands, &gltfs, outpost);
    } else {
        spawn_arena(&mut commands, &mut meshes, &mut materials);
    }
    let fresh = if *layout == WorldLayout::SmokeArena {
        SavedProgress::capture(
            &AttributeRanks::default().with_rank(AttributeId::Strength, 10),
            CharacterProgress::default(),
        )
    } else {
        SavedProgress::default()
    };
    let progress = run.checkpoint().map_or(&fresh, |save| &save.progression);
    let experience = run
        .checkpoint()
        .map_or_else(Experience::default, |save| save.player_experience());
    spawn_actors(
        (&items, &npcs),
        &mut commands,
        &gltfs,
        &assets,
        *layout,
        progress,
        experience,
        crate::player::PlayerName(
            run.checkpoint()
                .map_or_else(|| new_name.0.clone(), |save| save.player_name.clone()),
        ),
        run.checkpoint()
            .map(|save| save.player_loadout())
            .unwrap_or_default(),
    );
    spawn_core(&mut commands, &gltfs, &assets, *layout);
    spawn_beacon(&mut commands, &mut meshes, &mut materials, *layout);
    spawn_lights(&mut commands, *layout);
    commands.trigger(crate::quest::SpawnBeaconLens(layout.beacon()));
}

fn spawn_outpost(
    commands: &mut Commands,
    gltfs: &Assets<Gltf>,
    outpost: &(Handle<Gltf>, Handle<Gltf>),
) {
    let collision = spawn_static_scenes(
        commands,
        gltfs,
        outpost,
        ("Outpost visuals", "Outpost collision"),
        Vec3::ZERO,
    );
    commands.entity(collision).observe(
        |_: On<ColliderConstructorHierarchyReady>, mut status: ResMut<LoadStatus>| {
            status.collision_ready = true;
            info!("Collision hierarchy ready");
        },
    );
    commands.spawn((
        SessionEntity,
        Name::new("Starting tent lantern"),
        PointLight {
            color: Color::srgb(1.0, 0.57, 0.28),
            intensity: 7_500.0,
            range: 6.0,
            ..default()
        },
        Transform::from_xyz(7.82, 0.32, -4.50),
    ));
    commands.spawn((
        SessionEntity,
        Bed,
        Name::new("Starting tent bed"),
        // The interaction point sits just above the mattress on its aisle side.
        Transform::from_xyz(9.45, -0.12, -4.40),
    ));
    crate::crossing::village::light_homes(commands);
    for x in [-36.55, -23.45] {
        commands.spawn((
            SessionEntity,
            Name::new("Perimeter gate lamp"),
            PointLight {
                color: Color::srgb(1.0, 0.53, 0.22),
                intensity: 32_000.0,
                range: 16.0,
                ..default()
            },
            Transform::from_xyz(x, 3.6, 23.5),
        ));
    }
    commands.spawn((
        SessionEntity,
        Name::new("Perimeter gate skylight"),
        PointLight {
            color: Color::srgb(0.42, 0.60, 0.88),
            intensity: 55_000.0,
            range: 24.0,
            ..default()
        },
        Transform::from_xyz(-30.0, 7.0, 20.0),
    ));
}

fn spawn_actors(
    (items, npcs): (&Items, &Npcs),
    commands: &mut Commands,
    gltfs: &Assets<Gltf>,
    assets: &GameAssets,
    layout: WorldLayout,
    progress: &SavedProgress,
    experience: Experience,
    player_name: crate::player::PlayerName,
    loadout: (crate::inventory::Inventory, crate::equipment::Equipment),
) {
    let character = gltfs
        .get(&assets.character)
        .expect("the validated character glTF must remain loaded when spawning the world")
        .scenes[0]
        .clone();
    commands
        .spawn((
            SessionEntity,
            Name::new("Player"),
            Player::default(),
            player_name,
            Visibility::default(),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            Collider::capsule(PLAYER_COLLIDER_RADIUS, PLAYER_COLLIDER_LENGTH),
            Transform::from_translation(layout.initial_spawn()),
            progress.player_components(),
            experience,
            loadout,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    ActorVisual::Player,
                    PendingScene,
                    WorldAssetRoot(character.clone()),
                    Transform::from_xyz(0.0, -0.9, 0.0),
                ))
                .observe(scene_ready)
                .observe(bind_actor_animation)
                .observe(bind_equipment_attachments);
        });
    let (ranks, inventory, equipment) = npcs.loadout(NpcId::Nemi, items);
    commands
        .spawn((
            SessionEntity,
            Name::new("Keeper Nemi"),
            nemi::speaker(),
            Nemi,
            NpcId::Nemi,
            ranks.actor_components(),
            (
                inventory,
                equipment,
                crate::weapon::WeaponStance::Holstered,
                crate::weapon::WeaponAttack::default(),
                crate::player::NpcGait::default(),
            ),
            Transform::from_translation(layout.keeper() + Vec3::Y * nemi::GROUND_OFFSET)
                .with_rotation(Quat::from_rotation_y(0.5)),
            Visibility::default(),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            Collider::capsule(0.38, 1.3),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    ActorVisual::Npc,
                    PendingScene,
                    WorldAssetRoot(character),
                    Transform::from_xyz(0.0, -0.9, 0.0).with_scale(Vec3::splat(1.12)),
                ))
                .observe(scene_ready)
                .observe(bind_actor_animation)
                .observe(bind_equipment_attachments);
        });
}

fn spawn_core(
    commands: &mut Commands,
    gltfs: &Assets<Gltf>,
    assets: &GameAssets,
    layout: WorldLayout,
) {
    commands
        .spawn((
            SessionEntity,
            Name::new("Power core"),
            Target::Core,
            PendingScene,
            WorldAssetRoot(
                gltfs
                    .get(&assets.core)
                    .expect(
                        "the validated beacon core glTF must remain loaded when spawning the world",
                    )
                    .scenes[0]
                    .clone(),
            ),
            Transform::from_translation(layout.core()),
        ))
        .observe(scene_ready);
}

fn spawn_beacon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    layout: WorldLayout,
) {
    commands.spawn((
        SessionEntity,
        Beacon,
        PointLight {
            color: Color::srgb(0.2, 0.85, 1.0),
            intensity: 0.0,
            range: 45.0,
            ..default()
        },
        Transform::from_translation(layout.beacon()),
    ));
    let socket_offset = if layout == WorldLayout::Outpost {
        3.0
    } else {
        -2.0
    };
    commands
        .spawn((
            SessionEntity,
            Name::new("Beacon core socket"),
            Target::BeaconSocket,
            Transform::from_translation(layout.beacon().with_y(0.5) + Vec3::Z * socket_offset),
            Mesh3d(meshes.add(Cuboid::new(0.65, 0.60, 0.50))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.14, 0.22, 0.27),
                metallic: 0.7,
                ..default()
            })),
        ))
        .with_child((
            Mesh3d(meshes.add(Torus::new(0.12, 0.20))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.62, 0.68),
                emissive: LinearRgba::rgb(0.02, 0.30, 0.38),
                ..default()
            })),
            Transform::from_xyz(0.0, 0.31, 0.0),
        ));
}

fn spawn_lights(commands: &mut Commands, layout: WorldLayout) {
    commands.spawn((
        SessionEntity,
        DirectionalLight {
            illuminance: 1800.0,
            color: Color::srgb(0.45, 0.64, 0.9),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.65, -0.5, 0.0)),
    ));
    for (position, color, intensity, range) in [
        (
            Vec3::new(-7.0, 3.0, 2.0),
            Color::srgb(1.0, 0.45, 0.15),
            60000.0,
            20.0,
        ),
        (
            if layout == WorldLayout::Outpost {
                Vec3::new(4.5, 3.0, -1.5)
            } else {
                Vec3::new(8.0, 3.0, -2.0)
            },
            Color::srgb(1.0, 0.45, 0.15),
            60000.0,
            20.0,
        ),
        (
            Vec3::new(0.0, 4.5, -117.0),
            Color::srgb(0.15, 0.8, 1.0),
            40000.0,
            20.0,
        ),
    ] {
        if layout == WorldLayout::SmokeArena && position.z < -20.0 {
            continue;
        }
        commands.spawn((
            SessionEntity,
            PointLight {
                color,
                intensity,
                range,
                ..default()
            },
            Transform::from_translation(position),
        ));
    }
}

/// Charon stays visible beyond the terrain fog, including from the Flats overlook.
fn configure_sky_meshes(
    mut commands: Commands,
    meshes: Query<
        (
            Entity,
            &Name,
            &MeshMaterial3d<StandardMaterial>,
            Option<&ChildOf>,
        ),
        Added<Mesh3d>,
    >,
    names: Query<&Name>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, name, material, parent) in &meshes {
        let charon = parent
            .and_then(|parent| names.get(parent.parent()).ok())
            .is_some_and(|name| name.as_str() == "Charon");
        if charon || name.as_str().starts_with("Distant star field.") {
            commands.entity(entity).insert(bevy::light::NotShadowCaster);
        }
        if charon
            && let Some(mut material) = materials.get_mut(material)
            && material.fog_enabled
        {
            material.fog_enabled = false;
        }
    }
}

// Resolve support against the loaded collision scene once, before play starts.
// Running this on resume would incorrectly teleport a player paused mid-jump.
fn ground_spawned_models(
    spatial: SpatialQuery,
    layout: Res<WorldLayout>,
    mut actors: Query<
        (Entity, &mut Transform, Option<&Target>, Has<Nemi>),
        (
            Or<(With<Player>, With<Target>, With<Nemi>)>,
            Without<Beacon>,
        ),
    >,
    mut beacons: Query<&mut Transform, With<Beacon>>,
    nodes: Query<(&Name, &GlobalTransform)>,
) {
    for (entity, mut transform, target, is_nemi) in &mut actors {
        // The new-game position is inside the shelter, below its roof.
        let origin = if target.is_none() && !is_nemi && *layout == WorldLayout::Outpost {
            transform.translation
        } else {
            transform.translation + Vec3::Y * 3.0
        };
        if let Some(hit) = spatial.cast_ray(
            origin,
            Dir3::NEG_Y,
            20.0,
            true,
            &SpatialQueryFilter::from_excluded_entities([entity]),
        ) {
            let support = origin.y - hit.distance;
            let offset = match target {
                None if is_nemi => nemi::GROUND_OFFSET,
                Some(Target::Core) => 0.445,
                Some(Target::BeaconSocket) => 0.30,
                None => PLAYER_GROUND_OFFSET,
            };
            transform.translation.y = support + offset;
        }
    }
    if let Some((_, spindle)) = nodes
        .iter()
        .find(|(name, _)| name.as_str() == "Beacon spindle")
    {
        for mut transform in &mut beacons {
            transform.translation = spindle.translation() + Vec3::Y * 3.5;
        }
    }
}
