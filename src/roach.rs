use crate::{
    dungeon::Area,
    enemy::{self, Bite, EnemyModel, GroundMelee, GroundMovement},
    inventory::ItemKind,
    loading::EnemyVisuals,
};
use avian3d::prelude::Collider;
use bevy::prelude::*;

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/relay_roach.glb",
    clips: ["RoachIdle", "RoachCrawl", "RoachBite", "RoachDeath"],
};
pub(crate) const CONFIG: GroundMelee = GroundMelee {
    name: "Relay roach",
    health: 20,
    loot: ItemKind::RoachMeat,
    movement: GroundMovement::new(3.5, 0.32),
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
        Collider::cuboid(1.0, 0.65, 1.2),
    )
}
