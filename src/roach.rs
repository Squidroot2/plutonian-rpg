use crate::{
    dungeon::Area,
    enemy::{self, Bite, EnemyBody, EnemyModel, GroundMelee, GroundMovement},
    inventory::ItemKind,
    loading::EnemyVisuals,
};
use avian3d::prelude::Collider;
use bevy::prelude::*;

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/relay_roach.glb",
    clips: ["RoachIdle", "RoachBite", "RoachDeath"],
    locomotion: Some("RoachCrawl"),
};
pub(crate) const GROUND_OFFSET: f32 = 0.32;
pub(crate) const CONFIG: GroundMelee = GroundMelee {
    body: EnemyBody {
        name: "Relay roach",
        health: 20,
        experience: 25,
        loot: ItemKind::RoachMeat,
    },
    movement: GroundMovement::new(3.5, GROUND_OFFSET),
    bite: Bite {
        range: 1.5,
        windup: 0.4,
        recovery: 0.8,
        damage: 12,
    },
};

#[derive(Component)]
pub(crate) struct Roach;

pub(crate) fn spawn(
    commands: &mut Commands,
    visuals: &EnemyVisuals,
    home: Vec3,
    area: Area,
) -> Entity {
    enemy::spawn_ground_melee(
        commands,
        Roach,
        visuals.get(&MODEL),
        home,
        area,
        CONFIG,
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
