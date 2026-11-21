use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*, window::PrimaryWindow};

use crate::{
    GameState,
    camera::Orbit,
    combat_feedback::CombatFeedback,
    equipment::Equipment,
    interaction::InteractionSystems,
    inventory::scene_ready,
    loading::{GameAssets, PendingScene},
    player::Player,
    quest::Dialogue,
    world::{SpawnWorld, WorldLayout},
};

pub const REACH: f32 = 2.5;
const IMPACT_AT: f32 = 6.0 / 24.0;
const RECOVERY_AT: f32 = 16.0 / 24.0;
const GRIP_HEIGHT: f32 = -0.05;
pub(crate) const GRIP_SIDE: f32 = -0.40;
const REACTION_DURATION: f32 = 0.42;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpearSystems;

#[derive(Clone, Copy, Debug, Message, PartialEq)]
pub(crate) struct SpearHit {
    pub(crate) entity: Entity,
    point: Vec3,
    normal: Vec3,
}

impl SpearHit {
    pub(crate) fn impact(self) -> CombatFeedback {
        CombatFeedback::Impact {
            position: self.point,
            normal: self.normal,
        }
    }
}

pub struct SpearPlugin;
impl Plugin for SpearPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Thrust>()
            .add_message::<SpearHit>()
            .add_observer(spawn_dummy)
            .add_systems(OnExit(GameState::Loading), ground_dummy)
            .add_systems(OnExit(GameState::Playing), |mut thrust: ResMut<Thrust>| {
                thrust.cancel();
            })
            .add_systems(
                Update,
                (attack, mark_dummy_hits, react)
                    .chain()
                    .after(InteractionSystems)
                    .in_set(SpearSystems)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Default, Resource)]
pub struct Thrust(Option<Swing>);
struct Swing {
    facing: Quat,
    elapsed: f32,
}
impl Thrust {
    pub fn active(&self) -> bool {
        self.0.is_some()
    }

    pub(crate) fn cancel(&mut self) {
        self.0 = None;
    }

    fn begin(&mut self, facing: Quat, grounded: bool, clicked: bool) {
        if !self.active() && grounded && clicked {
            self.0 = Some(Swing {
                facing,
                elapsed: 0.0,
            });
        }
    }

    fn advance(&mut self, delta: f32) -> bool {
        let Some(swing) = self.0.as_mut() else {
            return false;
        };
        let previous = swing.elapsed;
        swing.elapsed += delta;
        let impact = previous < IMPACT_AT && swing.elapsed >= IMPACT_AT;
        if swing.elapsed >= RECOVERY_AT {
            self.0 = None;
        }
        impact
    }
}

#[derive(Component, Default)]
pub struct PracticeDummy {
    pub(crate) remaining: f32,
}
#[derive(Component)]
struct DummyVisual;
#[derive(Component)]
struct DummyFlash;

fn spawn_dummy(
    _: On<SpawnWorld>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
    layout: Res<WorldLayout>,
) {
    let scene = models
        .get(&assets.dummy)
        .expect("the validated practice dummy model must be loaded")
        .scenes[0]
        .clone();
    commands
        .spawn((
            Name::new("Practice dummy"),
            PracticeDummy::default(),
            RigidBody::Static,
            Collider::cuboid(0.72, 1.5, 0.55),
            Transform::from_translation(layout.dummy()),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    DummyVisual,
                    PendingScene,
                    WorldAssetRoot(scene),
                    Transform::from_xyz(0.0, -0.75, 0.0),
                ))
                .observe(scene_ready);
            parent.spawn((
                DummyFlash,
                PointLight {
                    color: Color::srgb(1.0, 0.42, 0.1),
                    intensity: 0.0,
                    range: 4.0,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.25, -0.55),
            ));
        });
}

fn ground_dummy(
    spatial: SpatialQuery,
    mut dummies: Query<(Entity, &mut Transform), With<PracticeDummy>>,
) {
    for (entity, mut transform) in &mut dummies {
        let origin = transform.translation + Vec3::Y;
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        if let Some(hit) = spatial.cast_ray(origin, Dir3::NEG_Y, 15.0, true, &filter) {
            transform.translation.y = origin.y - hit.distance + 0.75;
        }
    }
}

fn attack(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    equipment: Res<Equipment>,
    dialogue: Res<Dialogue>,
    orbit: Res<Orbit>,
    time: Res<Time>,
    spatial: SpatialQuery,
    mut thrust: ResMut<Thrust>,
    mut player: Query<(Entity, &Player, &mut Transform)>,
    mut hits: MessageWriter<SpearHit>,
) {
    let Ok((entity, controller, mut transform)) = player.single_mut() else {
        return;
    };
    if dialogue.text.is_some()
        || !equipment.has_spear()
        || !windows.iter().any(|window| window.focused)
    {
        thrust.cancel();
        return;
    }
    if let Some(swing) = thrust.0.as_ref() {
        let facing = swing.facing;
        transform.rotation = facing;
        if thrust.advance(time.delta_secs())
            && let Some(hit) = thrust_contact(&spatial, entity, transform.translation, facing)
        {
            hits.write(hit);
        }
    } else {
        let facing = Quat::from_rotation_y(orbit.yaw + std::f32::consts::PI);
        thrust.begin(
            facing,
            controller.grounded,
            buttons.just_pressed(MouseButton::Left),
        );
        if thrust.active() {
            transform.rotation = facing;
        }
    }
}

fn mark_dummy_hits(
    mut hits: MessageReader<SpearHit>,
    mut dummies: Query<&mut PracticeDummy>,
    mut feedback: MessageWriter<CombatFeedback>,
) {
    for &hit in hits.read() {
        if let Ok(mut dummy) = dummies.get_mut(hit.entity) {
            dummy.remaining = REACTION_DURATION;
            feedback.write(hit.impact());
        }
    }
}

fn thrust_contact(
    spatial: &SpatialQuery,
    player: Entity,
    position: Vec3,
    facing: Quat,
) -> Option<SpearHit> {
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
        .map(|hit| SpearHit {
            entity: hit.entity,
            point: hit.point1,
            normal: hit.normal1,
        })
}

fn react(
    time: Res<Time>,
    mut dummies: Query<(&mut PracticeDummy, &Children)>,
    mut visuals: Query<&mut Transform, With<DummyVisual>>,
    mut lights: Query<&mut PointLight, With<DummyFlash>>,
) {
    for (mut dummy, children) in &mut dummies {
        if dummy.remaining <= 0.0 {
            continue;
        }
        let phase = 1.0 - dummy.remaining / REACTION_DURATION;
        let strength = 1.0 - phase;
        for child in children.iter() {
            if let Ok(mut transform) = visuals.get_mut(child) {
                transform.rotation = Quat::from_rotation_z(0.24 * (phase * 14.0).sin() * strength);
            }
            if let Ok(mut light) = lights.get_mut(child) {
                light.intensity = 180_000.0 * strength.powi(3);
            }
        }
        dummy.remaining = (dummy.remaining - time.delta_secs()).max(0.0);
        if dummy.remaining == 0.0 {
            for child in children.iter() {
                if let Ok(mut transform) = visuals.get_mut(child) {
                    transform.rotation = Quat::IDENTITY;
                }
                if let Ok(mut light) = lights.get_mut(child) {
                    light.intensity = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ItemKind;
    use bevy::{
        asset::AssetPlugin, ecs::system::RunSystemOnce, mesh::MeshPlugin, state::app::StatesPlugin,
        time::TimeUpdateStrategy, transform::TransformPlugin,
    };
    use std::time::Duration;

    #[test]
    fn thrust_has_one_impact_and_ignores_clicks_before_recovery() {
        let mut thrust = Thrust::default();
        thrust.begin(Quat::IDENTITY, false, true);
        assert!(!thrust.active(), "airborne attacks must be rejected");
        thrust.begin(Quat::IDENTITY, true, false);
        assert!(!thrust.active(), "holding the mouse is not a new click");
        thrust.begin(Quat::IDENTITY, true, true);
        assert!(!thrust.advance(0.2));
        thrust.begin(Quat::from_rotation_y(1.0), true, true);
        assert_eq!(
            thrust.0.as_ref().map(|swing| swing.facing),
            Some(Quat::IDENTITY)
        );
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
        .init_resource::<Orbit>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_message::<CombatFeedback>()
        .add_plugins(SpearPlugin);
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
        assert_eq!(hit.entity, dummy);
        assert!(
            (hit.point.z - 2.125).abs() < 0.02,
            "impact must lie on the dummy's front face"
        );
        assert!(hit.normal.dot(Vec3::NEG_Z) > 0.98);
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
            hit.map(|contact| contact.entity),
            Some(wall),
            "the caster must stop at the wall before the dummy"
        );
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
            PracticeDummy::default(),
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

        fn swing(app: &mut App, camera_yaw: f32) -> Vec<CombatFeedback> {
            app.world_mut().resource_mut::<Orbit>().yaw = camera_yaw;
            let mut hits = app
                .world()
                .resource::<Messages<CombatFeedback>>()
                .get_cursor_current();
            let mut feedback = Vec::new();
            let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            input.reset_all();
            input.press(MouseButton::Left);
            for _ in 0..40 {
                app.update();
                app.world_mut()
                    .resource_mut::<ButtonInput<MouseButton>>()
                    .clear_just_pressed(MouseButton::Left);
                feedback.extend(
                    hits.read(app.world().resource::<Messages<CombatFeedback>>())
                        .copied(),
                );
            }
            feedback
        }

        let forward_yaw = -std::f32::consts::PI;
        assert!(matches!(
            swing(&mut app, forward_yaw).as_slice(),
            [CombatFeedback::Impact { .. }]
        ));
        assert!(swing(&mut app, 0.0).is_empty());

        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(2.0, 2.0, 0.2),
            Transform::from_xyz(0.0, 0.0, 1.4),
        ));
        for _ in 0..3 {
            app.update();
        }
        assert!(
            swing(&mut app, forward_yaw).is_empty(),
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
