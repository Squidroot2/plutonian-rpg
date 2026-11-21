use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*};

use crate::{
    GameState,
    combat::{CombatSystems, HitReaction, TrainingTarget},
    inventory::scene_ready,
    loading::{GameAssets, PendingScene},
    world::{SessionEntity, SpawnWorld, WorldLayout},
};

const REACTION_DURATION: f32 = 0.42;

#[derive(Component, Default)]
pub struct PracticeDummy;

#[derive(Component)]
struct DummyVisual;

#[derive(Component)]
struct DummyFlash;

pub struct PracticeDummyPlugin;

impl Plugin for PracticeDummyPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_dummy)
            .add_systems(OnExit(GameState::Loading), ground_dummy)
            .add_systems(
                Update,
                react
                    .after(CombatSystems::Resolve)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn spawn_dummy(
    _: On<SpawnWorld>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
    layout: Res<WorldLayout>,
) {
    let scene = models
        .get(&assets.dummy)
        .expect("the validated practice dummy model must be loaded")
        .scenes[0]
        .clone();
    commands
        .spawn((
            SessionEntity,
            Name::new("Practice dummy"),
            PracticeDummy,
            TrainingTarget,
            HitReaction::new(REACTION_DURATION),
            RigidBody::Static,
            Collider::cuboid(0.72, 1.5, 0.55),
            Transform::from_translation(layout.dummy()),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    DummyVisual,
                    PendingScene,
                    WorldAssetRoot(scene),
                    Transform::from_xyz(0.0, -0.75, 0.0),
                ))
                .observe(scene_ready);
            parent.spawn((
                DummyFlash,
                PointLight {
                    color: Color::srgb(1.0, 0.42, 0.1),
                    intensity: 0.0,
                    range: 4.0,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.25, -0.55),
            ));
        });
}

fn ground_dummy(
    spatial: SpatialQuery,
    mut dummies: Query<(Entity, &mut Transform), With<PracticeDummy>>,
) {
    for (entity, mut transform) in &mut dummies {
        let origin = transform.translation + Vec3::Y;
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        if let Some(hit) = spatial.cast_ray(origin, Dir3::NEG_Y, 15.0, true, &filter) {
            transform.translation.y = origin.y - hit.distance + 0.75;
        }
    }
}

fn react(
    dummies: Query<(&HitReaction, &Children), (With<PracticeDummy>, Changed<HitReaction>)>,
    mut visuals: Query<&mut Transform, With<DummyVisual>>,
    mut lights: Query<&mut PointLight, With<DummyFlash>>,
) {
    for (reaction, children) in &dummies {
        let strength = reaction.strength();
        let phase = 1.0 - strength;
        for child in children.iter() {
            if let Ok(mut transform) = visuals.get_mut(child) {
                transform.rotation = Quat::from_rotation_z(0.24 * (phase * 14.0).sin() * strength);
            }
            if let Ok(mut light) = lights.get_mut(child) {
                light.intensity = 180_000.0 * strength.powi(3);
            }
        }
    }
}
