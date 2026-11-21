use crate::{
    dungeon::Area,
    enemy::{self, Bite, EnemyModel, GroundMelee, GroundMovement},
    inventory::ItemKind,
    loading::EnemyVisuals,
    save::EncounterId,
    world::{SpawnWorld, WorldLayout},
};
use avian3d::prelude::Collider;
use bevy::prelude::*;

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/ice_worm.glb",
    clips: ["WormIdle", "WormCrawl", "WormBite", "WormDeath"],
};
pub(crate) const GROUND_OFFSET: f32 = 0.56;
const HOMES: [(EncounterId, Vec3); 3] = [
    (EncounterId::NearTrailWorm, Vec3::new(6.0, 2.0, -27.0)),
    (EncounterId::FarTrailWorm, Vec3::new(-7.0, 2.0, -71.0)),
    (EncounterId::CavernWorm, Vec3::new(3.4, 2.0, -110.0)),
];
pub(crate) const CONFIG: GroundMelee = GroundMelee {
    name: "Ice worm",
    health: 30,
    loot: ItemKind::WormMeat,
    movement: GroundMovement::new(2.4, GROUND_OFFSET),
    bite: Bite {
        range: 1.5,
        windup: 0.55,
        recovery: 0.85,
        damage: 20,
    },
};

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
    visuals: Res<EnemyVisuals>,
) {
    if *layout == WorldLayout::Outpost {
        for (id, home) in HOMES {
            let worm = spawn(&mut commands, &visuals, home, Area::Surface);
            commands.entity(worm).insert(id);
        }
    }
}

pub(crate) fn spawn(
    commands: &mut Commands,
    visuals: &EnemyVisuals,
    home: Vec3,
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
        CONFIG,
        body,
    )
}
