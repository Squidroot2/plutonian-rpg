use super::{
    capture,
    inventory::{click, walk},
    spear::SpearSmoke,
};
use crate::{
    GameState,
    camera::{FollowCamera, Orbit},
    combat::{Dead, Health},
    combat_feedback::{AttackImpact, BitePulse},
    content::{Enemies, EnemyArchetype},
    dungeon::Area,
    enemy::{MeleePhase, MeleePursuit},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind},
    loading::EnemyVisuals,
    player::PlayerRead,
    player::{DODGE_KEY, Player},
    save::EncounterId,
    ui::loot_ui::LootAction,
    ui::{HealthBar, HealthBarFill},
    weapon::{GRIP_SIDE, WeaponAttack},
    worm::{self, GROUND_OFFSET, Worm},
};
use avian3d::prelude::{Collider, SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(super) enum Encounter {
    #[default]
    Spawn,
    Pursuit(Vec3),
    Dodge {
        start: Option<Vec3>,
    },
    TakeBite,
    Fight {
        captured: bool,
    },
    Collapse {
        elapsed: f32,
        captured: bool,
    },
    Loot,
    CloseLoot,
    Finish,
}

pub(super) fn check_outpost_sites(
    worms: Query<(Entity, &Transform, &Collider, &EncounterId), With<Worm>>,
    spatial: SpatialQuery,
) {
    let surface_worms = worms.iter().filter(|(_, _, _, id)| {
        matches!(
            id,
            EncounterId::NearTrailWorm | EncounterId::FarTrailWorm | EncounterId::CavernWorm
        )
    });
    assert_eq!(
        surface_worms.clone().count(),
        3,
        "the shipped map must spawn all three worms"
    );
    for (entity, transform, body, _) in surface_worms {
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
    smoke: ResMut<SpearSmoke>,
    (visuals, enemies): (Option<Res<EnemyVisuals>>, Option<Res<Enemies>>),
    player: Query<(&Player, &Transform, &Health, &WeaponAttack)>,
    worms: Query<
        (
            &MeleePursuit,
            &Transform,
            &Health,
            Has<Dead>,
            Option<&Collider>,
        ),
        With<Worm>,
    >,
    (state, time, nearby, bag): (
        Res<State<GameState>>,
        Res<Time>,
        Res<Nearby>,
        PlayerRead<Inventory>,
    ),
    (actions, window, camera): (
        Query<(Entity, &LootAction)>,
        Query<Entity, With<Window>>,
        Query<Entity, With<FollowCamera>>,
    ),
    health_bar: Query<&Node, With<HealthBarFill>>,
    bar_visibility: Query<&Visibility, With<HealthBar>>,
    feedback: (
        Query<&Visibility, With<BitePulse>>,
        Query<(), With<AttackImpact>>,
    ),
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: ResMut<ButtonInput<MouseButton>>,
    mut commands: Commands,
) {
    if *smoke != SpearSmoke::WormEncounter {
        return;
    }
    let (controller, transform, player_health, thrust) =
        player.single().expect("worm smoke requires a player");
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
            &visuals.expect("the loaded smoke arena must provide the worm visuals"),
            enemies
                .as_ref()
                .expect("the smoke catalog must load")
                .get(EnemyArchetype::Worm.id())
                .expect("the shipped worm definition must exist"),
            Transform::from_translation(home),
            Area::Surface,
        );
        *phase = Encounter::Pursuit(home);
        orbit.yaw = 0.0;
        orbit.distance = 3.8;
        return;
    }
    let (worm, enemy, worm_health, dead, collider) = worms
        .single()
        .expect("the encounter worm must remain spawned");
    match *phase {
        Encounter::Pursuit(..)
        | Encounter::Dodge { .. }
        | Encounter::TakeBite
        | Encounter::Fight { .. } => check_worm_combat(
            phase,
            health_bar,
            bar_visibility,
            orbit,
            keys,
            mouse,
            commands,
            controller,
            transform,
            player_health,
            thrust,
            pulse,
            impacts,
            worm,
            enemy,
            worm_health,
            collider,
        ),
        Encounter::Collapse { .. } | Encounter::Loot | Encounter::CloseLoot | Encounter::Finish => {
            check_worm_loot(
                phase, smoke, state, time, nearby, bag, actions, window, camera, orbit, keys,
                commands, transform, enemy, dead, collider,
            )
        }
        _ => {}
    }
}

fn check_worm_combat(
    mut phase: ResMut<Encounter>,
    health_bar: Query<&Node, With<HealthBarFill>>,
    bar_visibility: Query<&Visibility, With<HealthBar>>,
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut commands: Commands,
    controller: &Player,
    transform: &Transform,
    player_health: &Health,
    thrust: &WeaponAttack,
    pulse: Query<&Visibility, With<BitePulse>>,
    impacts: Query<(), With<AttackImpact>>,
    worm: &MeleePursuit,
    enemy: &Transform,
    worm_health: &Health,
    collider: Option<&Collider>,
) {
    match *phase {
        Encounter::Pursuit(home) if matches!(worm.phase, MeleePhase::Windup(_)) => {
            assert!(
                enemy.translation.distance(home) > 1.0,
                "worm must pursue before biting"
            );
            assert_eq!(player_health.current(), 100, "wind-up must not deal damage");
            assert_eq!(bar_visibility.single().ok(), Some(&Visibility::Hidden));
            orbit.yaw = 1.1;
            orbit.pitch = 0.44;
            capture(&mut commands, "worm-windup");
            *phase = Encounter::Dodge { start: None };
        }
        Encounter::Dodge { start } => {
            if let MeleePhase::Windup(remaining) = worm.phase {
                if remaining <= 0.27 && start.is_none() && controller.grounded {
                    orbit.yaw = 0.0;
                    keys.press(KeyCode::KeyS);
                    keys.press(DODGE_KEY);
                    *phase = Encounter::Dodge {
                        start: Some(transform.translation),
                    };
                }
            } else if matches!(worm.phase, MeleePhase::Recovery(_)) {
                let start = start.expect("the player must dodge before the worm bites");
                assert!(
                    transform.translation.z - start.z > 0.95,
                    "one dodge press must hop back out of bite range"
                );
                assert_eq!(
                    player_health.current(),
                    100,
                    "the dodge hop must evade the bite"
                );
                assert_eq!(
                    pulse.single().ok(),
                    Some(&Visibility::Hidden),
                    "a dodged attack must leave the screen clear"
                );
                *phase = Encounter::TakeBite;
            }
        }
        Encounter::TakeBite if player_health.current() < 100 => {
            assert_eq!(
                player_health.current(),
                80,
                "one unarmored bite must deal 20 damage"
            );
            assert_eq!(
                health_bar.single().map(|bar| bar.width).ok(),
                Some(percent(80))
            );
            assert_eq!(bar_visibility.single().ok(), Some(&Visibility::Inherited));
            assert_eq!(
                pulse.single().ok(),
                Some(&Visibility::Visible),
                "an accepted bite must show the screen pulse"
            );
            assert!(matches!(worm.phase, MeleePhase::Recovery(_)));
            orbit.yaw = 1.05;
            capture(&mut commands, "worm-bite");
            *phase = Encounter::Fight { captured: false };
        }
        Encounter::Fight { captured: false }
            if (1..worm_health.maximum()).contains(&worm_health.current()) =>
        {
            assert_eq!(
                impacts.iter().count(),
                1,
                "one worm hit must spawn one impact"
            );
            orbit.yaw += 0.74;
            capture(&mut commands, "worm-hit");
            *phase = Encounter::Fight { captured: true };
        }
        Encounter::Fight { .. } if worm_health.current() == 0 => {
            assert!(
                collider.is_none(),
                "the collapsed worm must release its collider"
            );
            orbit.yaw = 1.05;
            *phase = Encounter::Collapse {
                elapsed: 0.0,
                captured: false,
            };
        }
        Encounter::Fight { .. } if !thrust.active() && controller.grounded => {
            let delta = (enemy.translation - transform.translation).with_y(0.0);
            // Aim slightly to the right of the center to align the offset shaft.
            let offset = (-GRIP_SIDE / delta.length()).clamp(0.0, 1.0).asin();
            orbit.yaw = delta.x.atan2(delta.z) + offset - std::f32::consts::PI;
            mouse.press(MouseButton::Left);
        }
        _ => {}
    }
}

fn check_worm_loot(
    mut phase: ResMut<Encounter>,
    mut smoke: ResMut<SpearSmoke>,
    state: Res<State<GameState>>,
    time: Res<Time>,
    nearby: Res<Nearby>,
    bag: PlayerRead<Inventory>,
    actions: Query<(Entity, &LootAction)>,
    window: Query<Entity, With<Window>>,
    camera: Query<Entity, With<FollowCamera>>,
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
    transform: &Transform,
    enemy: &Transform,
    dead: bool,
    collider: Option<&Collider>,
) {
    match *phase {
        Encounter::Collapse {
            elapsed,
            mut captured,
        } => {
            let elapsed = elapsed + time.delta_secs();
            if !captured && elapsed >= 0.37 {
                capture(&mut commands, "worm-collapse");
                captured = true;
            }
            if elapsed >= 0.90 && matches!(nearby.0, Some((_, InteractionTarget::Corpse))) {
                assert!(dead && collider.is_none());
                keys.press(KeyCode::KeyE);
                *phase = Encounter::Loot;
            } else {
                if elapsed >= 0.90 {
                    walk(
                        &mut keys,
                        &mut orbit,
                        transform.translation,
                        enemy.translation,
                    );
                }
                *phase = Encounter::Collapse { elapsed, captured };
            }
        }
        Encounter::Loot if *state.get() == GameState::Looting => {
            assert_eq!(bag.count(ItemKind::WormMeat), 0);
            capture(&mut commands, "worm-loot");
            click(
                &mut commands,
                LootAction::Take,
                &actions,
                window.single().expect("smoke window"),
                camera.single().expect("player camera"),
            );
            *phase = Encounter::CloseLoot;
        }
        Encounter::CloseLoot => {
            assert_eq!(bag.count(ItemKind::WormMeat), 1);
            keys.press(KeyCode::Escape);
            *phase = Encounter::Finish;
        }
        Encounter::Finish if *state.get() == GameState::Playing => {
            assert!(dead && collider.is_none());
            info!("WORM SMOKE PASS: dodged, fought, and looted a persistent corpse");
            orbit.pitch = 0.22;
            keys.press(KeyCode::KeyI);
            *smoke = SpearSmoke::Unequip;
        }
        _ => {}
    }
}
