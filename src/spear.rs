use avian3d::prelude::*;
use bevy::{prelude::*, window::PrimaryWindow};

use crate::{
    GameState,
    combat::{CombatSystems, Contact, Hit},
    controls::{Binding, Control, Keybinds},
    equipment::Equipment,
    interaction::InteractionSystems,
    inventory::{CARRY_CAPACITY, Inventory},
    player::{Dodge, MovementInput, Player},
    quest::Dialogue,
    quest_ui::QuestLog,
};

/// Maximum distance the spear thrust's collision shape travels.
pub const REACH: f32 = 2.5;
/// Elapsed attack time when the thrust checks for a hit (6 frames at 24 FPS).
const IMPACT_AT: f32 = 6.0 / 24.0;
/// Elapsed attack time when the thrust recovery ends (16 frames at 24 FPS).
const RECOVERY_AT: f32 = 16.0 / 24.0;
/// Vertical offset of the thrust collision origin from the player position.
const GRIP_HEIGHT: f32 = -0.05;
/// Sideways offset of the thrust collision origin from the player position.
pub(crate) const GRIP_SIDE: f32 = -0.40;
/// Damage dealt by a spear thrust.
const THRUST_DAMAGE: u16 = 10;

pub struct SpearPlugin;
impl Plugin for SpearPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Thrust>()
            .add_systems(OnExit(GameState::Playing), |mut thrust: ResMut<Thrust>| {
                thrust.cancel();
            })
            .add_systems(
                Update,
                attack
                    .after(InteractionSystems)
                    .in_set(CombatSystems::Detect)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Default, Resource)]
pub struct Thrust {
    elapsed: Option<f32>,
}
impl Thrust {
    pub fn active(&self) -> bool {
        self.elapsed.is_some()
    }

    pub(crate) fn just_started(&self) -> bool {
        self.elapsed == Some(0.0)
    }

    pub(crate) fn cancel(&mut self) {
        self.elapsed = None;
    }

    fn begin(&mut self, grounded: bool, clicked: bool) {
        if !self.active() && grounded && clicked {
            self.elapsed = Some(0.0);
        }
    }

    fn advance(&mut self, delta: f32) -> bool {
        let Some(elapsed) = self.elapsed.as_mut() else {
            return false;
        };
        let previous = *elapsed;
        *elapsed += delta;
        let impact = previous < IMPACT_AT && *elapsed >= IMPACT_AT;
        if *elapsed >= RECOVERY_AT {
            self.cancel();
        }
        impact
    }
}

fn attack(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    windows: Query<&Window, With<PrimaryWindow>>,
    equipment: Res<Equipment>,
    inventory: Res<Inventory>,
    dialogue: Res<Dialogue>,
    time: Res<Time>,
    spatial: SpatialQuery,
    movement: Res<MovementInput>,
    mut thrust: ResMut<Thrust>,
    player: Query<(Entity, &Player, &Dodge, &Transform)>,
    mut hits: MessageWriter<Hit>,
    log: Option<Res<QuestLog>>,
) {
    let Ok((entity, controller, dodge, transform)) = player.single() else {
        return;
    };
    if dialogue.text.is_some()
        || !equipment.has_spear()
        || !windows.iter().any(|window| window.focused)
    {
        thrust.cancel();
        return;
    }
    if thrust.active() {
        if thrust.advance(time.delta_secs())
            && let Some(hit) =
                thrust_contact(&spatial, entity, transform.translation, transform.rotation)
        {
            hits.write(hit);
        }
    } else if !log.is_some_and(|log| log.is_open())
        && !dodge.active()
        && !(inventory.carried_weight(&equipment) <= CARRY_CAPACITY
            && movement.dodge_pending(controller.grounded, dodge))
    {
        let attack = match bindings.get(Control::Attack) {
            Binding::Key(key) => keys.just_pressed(key),
            Binding::Mouse(button) => buttons.just_pressed(button),
        };
        thrust.begin(controller.grounded, attack);
    }
}

fn thrust_contact(
    spatial: &SpatialQuery,
    player: Entity,
    position: Vec3,
    facing: Quat,
) -> Option<Hit> {
    let direction = Dir3::new(facing * Vec3::Z).ok()?;
    // Match the shaft's offset in the two-handed thrust pose.
    let origin = position + facing * Vec3::new(GRIP_SIDE, GRIP_HEIGHT, 0.0);
    spatial
        .cast_shape(
            &Collider::sphere(0.12),
            origin,
            Quat::IDENTITY,
            direction,
            &ShapeCastConfig {
                compute_contact_on_penetration: true,
                ..ShapeCastConfig::from_max_distance(REACH)
            },
            &SpatialQueryFilter::from_excluded_entities([player]),
        )
        .map(|hit| Hit {
            source: player,
            target: hit.entity,
            damage: THRUST_DAMAGE,
            contact: Some(Contact {
                position: hit.point1,
                normal: hit.normal1,
            }),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::TrainingTarget;
    use crate::practice_dummy::PracticeDummy;
    use crate::{camera::Orbit, inventory::ItemKind, player::face_camera};
    use bevy::{
        asset::AssetPlugin, ecs::system::RunSystemOnce, mesh::MeshPlugin, state::app::StatesPlugin,
        time::TimeUpdateStrategy, transform::TransformPlugin,
    };
    use std::time::Duration;

    #[test]
    fn thrust_has_one_impact_and_ignores_clicks_before_recovery() {
        let mut thrust = Thrust::default();
        thrust.begin(false, true);
        assert!(!thrust.active(), "airborne attacks must be rejected");
        thrust.begin(true, false);
        assert!(!thrust.active(), "holding the mouse is not a new click");
        thrust.begin(true, true);
        assert!(!thrust.advance(0.2));
        thrust.begin(true, true);
        assert!(
            thrust.advance(0.06),
            "crossing full extension must report an impact"
        );
        assert!(!thrust.advance(0.2));
        assert!(!thrust.advance(0.3));
        assert!(!thrust.active());
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            TransformPlugin,
            AssetPlugin::default(),
            MeshPlugin,
            PhysicsPlugins::default(),
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .init_state::<GameState>()
        .init_resource::<Dialogue>()
        .init_resource::<Equipment>()
        .init_resource::<Inventory>()
        .init_resource::<Orbit>()
        .init_resource::<MovementInput>()
        .init_resource::<Keybinds>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins((crate::combat::CombatPlugin, SpearPlugin))
        .add_systems(Update, face_camera.before(CombatSystems::Detect));
        app.finish();
        app.cleanup();
        app
    }

    #[test]
    fn thrust_respects_aim_reach_and_world_occlusion() {
        let mut app = test_app();
        let player = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::sphere(0.5),
                Transform::default(),
            ))
            .id();
        let dummy = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(0.72, 1.5, 0.55),
                Transform::from_xyz(GRIP_SIDE, 0.0, 2.4),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }
        let hit = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                thrust_contact(&spatial, player, Vec3::ZERO, Quat::IDENTITY)
            })
            .expect("physics resources must be initialized for the thrust cast");
        let hit = hit.expect("the cast must ignore the player collider and reach the dummy");
        assert_eq!(hit.target, dummy);
        let contact = hit.contact.expect("thrust must include a world contact");
        assert!(
            (contact.position.z - 2.125).abs() < 0.02,
            "impact must lie on the dummy's front face"
        );
        assert!(contact.normal.dot(Vec3::NEG_Z) > 0.98);
        let misses = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                [
                    thrust_contact(
                        &spatial,
                        player,
                        Vec3::ZERO,
                        Quat::from_rotation_y(std::f32::consts::PI),
                    ),
                    thrust_contact(&spatial, player, Vec3::NEG_Z, Quat::IDENTITY),
                ]
            })
            .expect("physics resources must be initialized for both miss cases");
        assert_eq!(misses, [None, None]);
        let wall = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(2.0, 2.0, 0.2),
                Transform::from_xyz(0.0, 0.0, 1.4),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }
        let hit = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                thrust_contact(&spatial, player, Vec3::ZERO, Quat::IDENTITY)
            })
            .expect("the thrust must query the updated world collision tree");
        assert_eq!(
            hit.map(|contact| contact.target),
            Some(wall),
            "the caster must stop at the wall before the dummy"
        );
    }

    #[test]
    fn grounded_thrust_reaches_the_roach_hitbox() {
        let mut app = test_app();
        let player_position = Vec3::Y * 0.905;
        let player = app
            .world_mut()
            .spawn(Transform::from_translation(player_position))
            .id();
        let roach = app
            .world_mut()
            .spawn((
                RigidBody::Kinematic,
                crate::roach::collider(),
                Transform::from_xyz(0.0, crate::roach::GROUND_OFFSET, 2.4),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }

        let hit = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                thrust_contact(&spatial, player, player_position, Quat::IDENTITY)
            })
            .expect("physics resources must be initialized for the thrust cast");
        assert_eq!(hit.map(|hit| hit.target), Some(roach));
    }

    #[test]
    fn only_a_landed_thrust_emits_one_impact() {
        let mut app = test_app();
        let world = app.world_mut();
        world.spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        let mut player = Player::default();
        player.grounded = true;
        world.spawn((player, Transform::default()));
        world.spawn((
            PracticeDummy,
            TrainingTarget,
            RigidBody::Static,
            Collider::cuboid(0.72, 1.5, 0.55),
            Transform::from_xyz(GRIP_SIDE, 0.0, 2.4),
        ));
        let mut bag = crate::inventory::Inventory::default();
        assert!(bag.add(ItemKind::FieldSpear, 1));
        assert_eq!(
            world
                .resource_mut::<Equipment>()
                .equip(&mut bag, ItemKind::FieldSpear),
            Ok(true)
        );
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        for _ in 0..3 {
            app.update();
        }

        fn swing(
            app: &mut App,
            camera_yaw: f32,
            turn_to: Option<f32>,
        ) -> Vec<crate::combat::ConfirmedHit> {
            app.world_mut().resource_mut::<Orbit>().yaw = camera_yaw;
            let mut hits = app
                .world()
                .resource::<Messages<crate::combat::ConfirmedHit>>()
                .get_cursor_current();
            let mut feedback = Vec::new();
            let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            input.reset_all();
            input.press(MouseButton::Left);
            for frame in 0..40 {
                if frame == 5
                    && let Some(yaw) = turn_to
                {
                    assert!(app.world().resource::<Thrust>().active());
                    app.world_mut().resource_mut::<Orbit>().yaw = yaw;
                }
                app.update();
                app.world_mut()
                    .resource_mut::<ButtonInput<MouseButton>>()
                    .clear_just_pressed(MouseButton::Left);
                feedback.extend(
                    hits.read(
                        app.world()
                            .resource::<Messages<crate::combat::ConfirmedHit>>(),
                    )
                    .copied(),
                );
            }
            feedback
        }

        let forward_yaw = -std::f32::consts::PI;
        assert!(matches!(
            swing(&mut app, 0.0, Some(forward_yaw)).as_slice(),
            [crate::combat::ConfirmedHit {
                contact: Some(_),
                ..
            }]
        ));
        assert!(swing(&mut app, 0.0, None).is_empty());

        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(2.0, 2.0, 0.2),
            Transform::from_xyz(0.0, 0.0, 1.4),
        ));
        for _ in 0..3 {
            app.update();
        }
        assert!(
            swing(&mut app, forward_yaw, None).is_empty(),
            "terrain must stop feedback along with the thrust"
        );
    }

    #[test]
    fn gameplay_requires_an_equipped_spear_and_cancels_on_dialogue_or_inventory() {
        let mut app = test_app();
        let world = app.world_mut();
        world.spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        let mut player = Player::default();
        player.grounded = true;
        world.spawn((player, Transform::default()));
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(!app.world().resource::<Thrust>().active());

        let world = app.world_mut();
        let mut bag = crate::inventory::Inventory::default();
        assert!(bag.add(ItemKind::FieldSpear, 1));
        assert_eq!(
            world
                .resource_mut::<Equipment>()
                .equip(&mut bag, ItemKind::FieldSpear),
            Ok(true)
        );
        world.resource_mut::<Dialogue>().text = Some("Talking");
        app.update();
        assert!(!app.world().resource::<Thrust>().active());
        app.world_mut().resource_mut::<Dialogue>().text = None;
        app.update();
        assert!(app.world().resource::<Thrust>().active());
        app.world_mut().resource_mut::<Dialogue>().text = Some("Talking");
        app.update();
        assert!(!app.world().resource::<Thrust>().active());

        app.world_mut().resource_mut::<Dialogue>().text = None;
        app.update();
        assert!(app.world().resource::<Thrust>().active());
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Inventory);
        app.update();
        assert!(!app.world().resource::<Thrust>().active());
    }
}
