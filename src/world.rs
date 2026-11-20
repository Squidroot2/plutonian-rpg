use crate::{
    loading::{GameAssets, LoadStatus},
    player::{ActorVisual, Player},
    quest::{Beacon, Target},
};
use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};

pub const START: Vec3 = Vec3::new(4.0, 0.5, 8.0);
pub const KEEPER: Vec3 = Vec3::new(3.0, -0.95, -1.0);
pub const CORE: Vec3 = Vec3::new(0.0, 2.3, -122.0);

pub struct WorldPlugin;
#[derive(Event)]
pub struct SpawnWorld;
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_world);
    }
}
fn ready(_: On<WorldInstanceReady>, mut status: ResMut<LoadStatus>) {
    status.ready += 1;
    info!("World scene ready: {} / 5", status.ready);
}
fn spawn_world(
    _: On<SpawnWorld>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
) {
    commands
        .spawn((
            Name::new("Outpost visuals"),
            WorldAssetRoot(gltfs.get(&assets.outpost).unwrap().scenes[0].clone()),
        ))
        .observe(ready);
    commands
        .spawn((
            Name::new("Outpost collision"),
            WorldAssetRoot(gltfs.get(&assets.collision).unwrap().scenes[0].clone()),
            RigidBody::Static,
            Visibility::Hidden,
            ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMesh),
        ))
        .observe(ready)
        .observe(
            |_: On<ColliderConstructorHierarchyReady>, mut status: ResMut<LoadStatus>| {
                status.collision_ready = true;
                info!("Collision hierarchy ready");
            },
        );
    let character = gltfs.get(&assets.character).unwrap().scenes[0].clone();
    commands
        .spawn((
            Name::new("Player"),
            Player::default(),
            Visibility::default(),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            Collider::capsule(0.32, 1.15),
            Transform::from_translation(START),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    ActorVisual { player: true },
                    WorldAssetRoot(character.clone()),
                    Transform::from_xyz(0.0, -0.9, 0.0),
                ))
                .observe(ready);
        });
    commands
        .spawn((
            Name::new("Keeper Nemi"),
            Target::Keeper,
            Transform::from_translation(KEEPER + Vec3::Y * 0.9),
            Visibility::default(),
            RigidBody::Static,
            Collider::capsule(0.38, 1.3),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    ActorVisual { player: false },
                    WorldAssetRoot(character),
                    Transform::from_xyz(0.0, -0.9, 0.0)
                        .with_rotation(Quat::from_rotation_y(0.5))
                        .with_scale(Vec3::splat(1.12)),
                ))
                .observe(ready);
        });
    commands
        .spawn((
            Name::new("Power core"),
            Target::Core,
            WorldAssetRoot(gltfs.get(&assets.core).unwrap().scenes[0].clone()),
            Transform::from_translation(CORE),
        ))
        .observe(ready);
    commands.spawn((
        Beacon,
        PointLight {
            color: Color::srgb(0.2, 0.85, 1.0),
            intensity: 0.0,
            range: 45.0,
            ..default()
        },
        Transform::from_xyz(0.0, 7.5, 5.0),
    ));
    commands.spawn((
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
            Vec3::new(8.0, 3.0, -2.0),
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
        commands.spawn((
            PointLight {
                color,
                intensity,
                range,
                ..default()
            },
            Transform::from_translation(position),
        ));
    }
    // Visible beacon lens, independent of the authored unlit tower.
    commands.trigger(crate::quest::SpawnBeaconLens);
}
