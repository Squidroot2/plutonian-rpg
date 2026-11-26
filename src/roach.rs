use crate::{
    content::EnemyDefinition,
    dungeon::Area,
    enemy::{self, EnemyClip, EnemyModel},
    loading::EnemyVisuals,
};
use avian3d::prelude::Collider;
use bevy::prelude::*;

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/relay_roach.glb",
    clips: [
        EnemyClip::new("RoachIdle", 2.0),
        EnemyClip::new("RoachBite", 1.2),
        EnemyClip::new("RoachDeath", 0.7),
    ],
    visual_offset: Vec3::ZERO,
    locomotion: Some(EnemyClip::new("RoachCrawl", 0.6)),
};
pub(crate) const GROUND_OFFSET: f32 = 0.32;
#[derive(Component)]
pub(crate) struct Roach;

pub(crate) fn spawn(
    commands: &mut Commands,
    visuals: &EnemyVisuals,
    definition: &EnemyDefinition,
    home: Transform,
    area: Area,
) -> Entity {
    enemy::spawn_ground_melee(
        commands,
        Roach,
        visuals.get(&MODEL),
        home,
        area,
        definition.melee(GROUND_OFFSET),
        collider(),
    )
}

pub(crate) fn collider() -> Collider {
    let height = 1.10;
    // Extend the collider upward from the floor at the roach's existing model height.
    Collider::compound(vec![(
        Vec3::Y * (height / 2.0 - GROUND_OFFSET + 0.01),
        Quat::IDENTITY,
        Collider::cuboid(1.0, height, 1.2),
    )])
}
