use crate::{
    dungeon::{Area, ORIGIN},
    enemy::{
        self, EnemyBody, EnemyModel,
        ranged::{ChargeLight, RangedAttack, Spit},
    },
    inventory::ItemKind,
    loading::EnemyVisuals,
};
use avian3d::prelude::*;
use bevy::prelude::*;

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/hushcap.glb",
    clips: ["HushcapIdle", "HushcapSpit", "HushcapDeath"],
    locomotion: None,
};
pub(crate) const HEALTH: u16 = 42;
pub(crate) const EXPERIENCE: u16 = 45;
pub(crate) const HOME: Vec3 = Vec3::new(ORIGIN.x + 3.2, ORIGIN.y, ORIGIN.z - 26.0);
pub(crate) const SPIT: Spit = Spit {
    sight_m: 8.0,
    windup_seconds: 0.85,
    recovery_seconds: 1.25,
    speed_mps: 9.0,
    travel_m: 10.0,
    radius_m: 0.14,
    damage: 24,
    muzzle: Vec3::new(0.0, 1.01, 0.56),
};
const CHARGE_LUMENS: f32 = 2_600.0;
const CHARGE_RANGE_M: f32 = 2.4;

#[derive(Component)]
pub(crate) struct Hushcap;

pub(crate) fn spawn(commands: &mut Commands, visuals: &EnemyVisuals) -> Entity {
    let area = Area::OldRelay;
    let stalk = (
        Vec3::Y * 0.53,
        Quat::IDENTITY,
        Collider::cylinder(0.42, 1.04),
    );
    let cap = (
        Vec3::Y * 1.27,
        Quat::IDENTITY,
        Collider::cylinder(0.75, 0.48),
    );
    let enemy = enemy::spawn_enemy(
        commands,
        Hushcap,
        visuals.get(&MODEL),
        Transform::from_translation(HOME),
        area,
        EnemyBody {
            name: "Hushcap",
            health: HEALTH,
            experience: EXPERIENCE,
            loot: ItemKind::Mycelium,
        },
        (
            RangedAttack::new(area, SPIT),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            Collider::compound(vec![stalk, cap]),
        ),
    );
    commands.entity(enemy).with_children(|root| {
        root.spawn((
            ChargeLight {
                peak_lumens: CHARGE_LUMENS,
            },
            PointLight {
                color: Color::srgb(0.29, 1.0, 0.69),
                intensity: 0.0,
                range: CHARGE_RANGE_M,
                ..default()
            },
            Transform::from_translation(SPIT.muzzle),
        ));
    });
    enemy
}
