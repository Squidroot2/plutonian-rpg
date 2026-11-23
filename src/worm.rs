use crate::{
    content::{Enemies, EnemyDefinition, Placements},
    dungeon::Area,
    enemy::{self, EnemyModel},
    loading::EnemyVisuals,
    save::EncounterId,
    world::{SpawnWorld, WorldLayout},
};
use avian3d::prelude::Collider;
use bevy::prelude::*;

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/ice_worm.glb",
    clips: ["WormIdle", "WormBite", "WormDeath"],
    visual_offset: Vec3::ZERO,
    locomotion: Some("WormCrawl"),
};
pub(crate) const GROUND_OFFSET: f32 = 0.56;
const HOMES: [(EncounterId, Vec3); 3] = [
    // Keep the trail between the paired waystones clear; these worms shelter
    // out toward the ice escarpments instead of occupying the marked route.
    (EncounterId::NearTrailWorm, Vec3::new(14.0, 2.0, -27.0)),
    (EncounterId::FarTrailWorm, Vec3::new(-14.0, 2.0, -71.0)),
    (EncounterId::CavernWorm, Vec3::new(3.4, 2.0, -110.0)),
];
#[derive(Component)]
pub(crate) struct Worm;

pub(crate) struct WormPlugin;

impl Plugin for WormPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_encounters);
    }
}

fn spawn_encounters(
    _: On<SpawnWorld>,
    mut commands: Commands,
    layout: Res<WorldLayout>,
    placements: Res<Placements>,
    enemies: Res<Enemies>,
    visuals: Res<EnemyVisuals>,
) {
    if *layout == WorldLayout::Outpost {
        for (id, home) in HOMES {
            let worm = spawn(
                &mut commands,
                &visuals,
                placements.enemy(&enemies, id),
                Transform::from_translation(home),
                Area::Surface,
            );
            commands.entity(worm).insert(id);
        }
    }
}

pub(crate) fn spawn(
    commands: &mut Commands,
    visuals: &EnemyVisuals,
    definition: &EnemyDefinition,
    home: Transform,
    area: Area,
) -> Entity {
    let body = Collider::compound(vec![
        (
            Vec3::new(0.0, 0.0, 0.30),
            Quat::IDENTITY,
            Collider::sphere(0.47),
        ),
        (
            Vec3::new(0.0, -0.14, -0.33),
            Quat::IDENTITY,
            Collider::sphere(0.39),
        ),
        (
            Vec3::new(0.0, -0.32, -0.75),
            Quat::IDENTITY,
            Collider::sphere(0.23),
        ),
    ]);
    enemy::spawn_ground_melee(
        commands,
        Worm,
        visuals.get(&MODEL),
        home,
        area,
        definition.melee(GROUND_OFFSET),
        body,
    )
}
