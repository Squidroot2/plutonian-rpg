use crate::{
    content::EnemyDefinition,
    dungeon::Area,
    enemy::{
        self, EnemyModel,
        ranged::{ChargeLight, RangedAttack},
    },
    loading::EnemyVisuals,
};
use avian3d::prelude::*;
use bevy::prelude::*;

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/hushcap.glb",
    clips: ["HushcapIdle", "HushcapSpit", "HushcapDeath"],
    visual_offset: Vec3::ZERO,
    locomotion: None,
};
const CHARGE_LUMENS: f32 = 2_600.0;
const CHARGE_RANGE_M: f32 = 2.4;

#[derive(Component)]
pub(crate) struct Hushcap;

pub(crate) fn spawn(
    commands: &mut Commands,
    visuals: &EnemyVisuals,
    definition: &EnemyDefinition,
    home: Transform,
    area: Area,
) -> Entity {
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
    let spit = definition.spit();
    let enemy = enemy::spawn_enemy(
        commands,
        Hushcap,
        visuals.get(&MODEL),
        home,
        area,
        definition.body(),
        (
            RangedAttack::new(area, spit),
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
            Transform::from_translation(spit.muzzle),
        ));
    });
    enemy
}
