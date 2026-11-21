use super::{capture, inventory::walk, spear::SpearSmoke};
use crate::{
    camera::Orbit,
    combat_feedback::{AttackImpact, BitePulse},
    loading::GameAssets,
    player::Player,
    spear::{GRIP_SIDE, Thrust},
    worm::{self, GROUND_OFFSET, Worm, WormPhase},
};
use avian3d::prelude::{Collider, SpatialQuery, SpatialQueryFilter};
use bevy::{gltf::Gltf, prelude::*};

#[derive(Resource, Default)]
pub(super) enum Encounter {
    #[default]
    Spawn,
    Pursuit(Vec3),
    Dodge {
        captured: bool,
    },
    TakeBite,
    Fight {
        captured: bool,
    },
    Collapse {
        captured: bool,
    },
}

pub(super) fn check_outpost_sites(
    worms: Query<(Entity, &Transform, &Collider), With<Worm>>,
    spatial: SpatialQuery,
) {
    let surface_worms = worms
        .iter()
        .filter(|(_, transform, _)| transform.translation.y > -4.0);
    assert_eq!(
        surface_worms.clone().count(),
        3,
        "the shipped map must spawn all three worms"
    );
    for (entity, transform, body) in surface_worms {
        let origin = transform.translation + Vec3::Y;
        let support = spatial
            .cast_ray(
                origin,
                Dir3::NEG_Y,
                2.0,
                true,
                &SpatialQueryFilter::from_excluded_entities([entity]),
            )
            .expect("each worm must be supported by the shipped map");
        let offset = support.distance - 1.0;
        assert!(
            (offset - GROUND_OFFSET).abs() < 0.03,
            "worm not grounded at {:?}",
            transform.translation
        );
        let obstacles = spatial.shape_intersections(
            body,
            transform.translation,
            transform.rotation,
            &SpatialQueryFilter::from_excluded_entities([entity, support.entity]),
        );
        assert!(
            obstacles.is_empty(),
            "worm overlaps an obstacle at {:?}",
            transform.translation
        );

        // The centerline remains open past each encounter.
        let lane = Vec3::new(0.0, origin.y, transform.translation.z + 2.0);
        assert!(
            spatial
                .cast_ray(lane, Dir3::NEG_Z, 4.0, true, &SpatialQueryFilter::default())
                .is_none(),
            "a worm encounter must leave a clear central route past {:?}",
            transform.translation
        );
    }
    info!("WORM SITES PASS: three grounded encounters with a clear route past each spawn");
}

pub(super) fn fight(
    mut phase: ResMut<Encounter>,
    mut smoke: ResMut<SpearSmoke>,
    assets: Option<Res<GameAssets>>,
    models: Res<Assets<Gltf>>,
    player: Query<(&Player, &Transform)>,
    worms: Query<(&Worm, &Transform, Option<&Collider>)>,
    thrust: Res<Thrust>,
    feedback: (
        Query<&Visibility, With<BitePulse>>,
        Query<(), With<AttackImpact>>,
    ),
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut commands: Commands,
) {
    if *smoke != SpearSmoke::WormEncounter {
        return;
    }
    let (controller, transform) = player.single().expect("worm smoke requires a player");
    let (pulse, impacts) = feedback;
    if matches!(*phase, Encounter::Spawn) {
        let fight_position = Vec3::new(9.5, 0.0, 0.0);
        if transform.translation.xz().distance(fight_position.xz()) > 0.2 {
            walk(&mut keys, &mut orbit, transform.translation, fight_position);
            return;
        }
        let home = (transform.translation - Vec3::Z * 4.0).with_y(GROUND_OFFSET);
        worm::spawn(
            &mut commands,
            &assets.expect("the loaded smoke arena must provide the worm asset"),
            &models,
            home,
        );
        *phase = Encounter::Pursuit(home);
        orbit.yaw = 0.0;
        orbit.distance = 3.8;
        return;
    }
    let Ok((worm, enemy, collider)) = worms.single() else {
        if matches!(*phase, Encounter::Collapse { .. }) {
            info!(
                "WORM SMOKE PASS: pursued, telegraphed a bite, missed a dodge, landed one bite, and died to the trident"
            );
            orbit.pitch = 0.22;
            keys.press(KeyCode::KeyI);
            *smoke = SpearSmoke::Unequip;
        }
        return;
    };
    match *phase {
        Encounter::Pursuit(home) if matches!(worm.phase, WormPhase::Windup(_)) => {
            assert!(
                enemy.translation.distance(home) > 1.0,
                "worm must pursue before biting"
            );
            assert_eq!(
                controller.vitality.health, 100,
                "wind-up must not deal damage"
            );
            *phase = Encounter::Dodge { captured: false };
        }
        Encounter::Dodge { captured } => {
            if let WormPhase::Windup(remaining) = worm.phase {
                if remaining <= 0.39 {
                    if captured {
                        orbit.yaw = 0.0;
                        keys.press(KeyCode::KeyS);
                        keys.press(KeyCode::ShiftLeft);
                    } else {
                        orbit.yaw = 1.1;
                        orbit.pitch = 0.44;
                        capture(&mut commands, "worm-windup");
                        *phase = Encounter::Dodge { captured: true };
                    }
                }
            } else if matches!(worm.phase, WormPhase::Recovery(_)) {
                assert_eq!(
                    controller.vitality.health, 100,
                    "backpedaling must evade the bite"
                );
                assert_eq!(
                    pulse.single().ok(),
                    Some(&Visibility::Hidden),
                    "a dodged attack must leave the screen clear"
                );
                *phase = Encounter::TakeBite;
            }
        }
        Encounter::TakeBite if controller.vitality.health < 100 => {
            assert_eq!(
                controller.vitality.health, 80,
                "one unarmored bite must deal 20 damage"
            );
            assert_eq!(
                pulse.single().ok(),
                Some(&Visibility::Visible),
                "an accepted bite must show the screen pulse"
            );
            assert!(matches!(worm.phase, WormPhase::Recovery(_)));
            orbit.yaw = 1.05;
            capture(&mut commands, "worm-bite");
            *phase = Encounter::Fight { captured: false };
        }
        Encounter::Fight { captured: false } if worm.hp == 20 => {
            assert_eq!(
                impacts.iter().count(),
                1,
                "one worm hit must spawn one impact"
            );
            orbit.yaw += 0.74;
            capture(&mut commands, "worm-hit");
            *phase = Encounter::Fight { captured: true };
        }
        Encounter::Fight { .. } if worm.hp == 0 => {
            assert!(
                collider.is_none(),
                "the collapsed worm must release its collider"
            );
            orbit.yaw = 1.05;
            *phase = Encounter::Collapse { captured: false };
        }
        Encounter::Fight { .. } if !thrust.active() && controller.grounded => {
            let delta = (enemy.translation - transform.translation).with_y(0.0);
            // Aim slightly to the right of the center to align the offset shaft.
            let offset = (-GRIP_SIDE / delta.length()).clamp(0.0, 1.0).asin();
            orbit.yaw = delta.x.atan2(delta.z) + offset - std::f32::consts::PI;
            mouse.press(MouseButton::Left);
        }
        Encounter::Collapse { captured: false } if matches!(worm.phase, WormPhase::Collapsing(seconds) if seconds < 0.33) =>
        {
            capture(&mut commands, "worm-collapse");
            *phase = Encounter::Collapse { captured: true };
        }
        _ => {}
    }
}
