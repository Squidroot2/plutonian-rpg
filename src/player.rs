use crate::{
    GameState,
    camera::Orbit,
    combat::{CombatSystems, Dead, Defense, Health},
    dungeon::Area,
    equipment::Equipment,
    quest::Dialogue,
    spear::Thrust,
    world::WorldLayout,
};
use avian3d::prelude::*;
use bevy::{prelude::*, world_serialization::WorldInstanceReady};
use std::time::Duration;

pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementInput>()
            .add_systems(OnEnter(GameState::Paused), freeze)
            .add_systems(OnEnter(GameState::Inventory), freeze)
            .add_systems(OnExit(GameState::Paused), resume)
            .add_systems(OnExit(GameState::Inventory), resume)
            .add_systems(
                PreUpdate,
                input
                    .after(crate::ui::pause)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(FixedUpdate, movement.run_if(in_state(GameState::Playing)))
            .add_systems(Update, sync_defense.before(CombatSystems::Resolve))
            .add_systems(
                Update,
                (respawn_defeated, animate)
                    .chain()
                    .after(CombatSystems::Resolve)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
#[derive(Component, Default)]
#[require(Dodge, Health = Health::new(MAX_HEALTH), Defense, crate::combat::HitProtection = crate::combat::HitProtection::new(0.75))]
pub struct Player {
    pub(crate) area: Area,
    pub velocity: Vec3,
    pub grounded: bool,
    pub running: bool,
    jumping: bool,
}
impl Player {
    pub(crate) fn respawn(&mut self, dodge: &mut Dodge, transform: &mut Transform, start: Vec3) {
        self.area = Area::Surface;
        transform.translation = start;
        self.velocity = Vec3::ZERO;
        self.jumping = false;
        self.grounded = false;
        self.running = false;
        *dodge = Dodge::default();
    }

    fn launch(&mut self, speed: f32) {
        self.velocity.y = speed;
        self.jumping = true;
        self.grounded = false;
    }
}

pub(crate) const MAX_HEALTH: u16 = 100;
pub(crate) const DODGE_KEY: KeyCode = KeyCode::ControlLeft;
const DODGE_LAUNCH_SPEED: f32 = 1.7;
const DODGE_GRAVITY: f32 = 10.0;
const DODGE_SPEED: f32 = 6.0;
const DODGE_DURATION: f32 = 0.32;
const DODGE_COOLDOWN: f32 = 0.55;

#[derive(Component, Default)]
pub(crate) struct Dodge {
    hop: Option<DodgeHop>,
    cooldown: f32,
}

struct DodgeHop {
    heading: Vec3,
    remaining: f32,
}

impl Dodge {
    pub(crate) fn active(&self) -> bool {
        self.hop.is_some()
    }

    fn ready(&self) -> bool {
        self.cooldown <= 0.0 && !self.active()
    }

    fn start(&mut self, heading: Vec3) {
        self.hop = Some(DodgeHop {
            heading,
            remaining: DODGE_DURATION,
        });
        self.cooldown = DODGE_COOLDOWN;
    }

    fn advance(&mut self, delta: f32) {
        if let Some(hop) = &mut self.hop {
            hop.remaining -= delta;
            if hop.remaining <= 0.0 {
                self.hop = None;
            }
        }
    }
}

fn sync_defense(equipment: Res<Equipment>, mut players: Query<&mut Defense, With<Player>>) {
    for mut armor in &mut players {
        if equipment.is_changed() || armor.is_added() {
            armor.set_if_neq(Defense(equipment.total_defense()));
        }
    }
}

pub(crate) fn respawn_defeated(
    layout: Res<WorldLayout>,
    mut thrust: ResMut<Thrust>,
    mut input: ResMut<MovementInput>,
    mut players: Query<(Entity, &mut Player, &mut Dodge, &mut Transform, &mut Health), Added<Dead>>,
    mut commands: Commands,
) {
    for (entity, mut player, mut dodge, mut transform, mut health) in &mut players {
        player.respawn(&mut dodge, &mut transform, layout.start());
        health.restore();
        thrust.cancel();
        *input = MovementInput::default();
        commands.entity(entity).remove::<Dead>();
    }
}

#[derive(Component)]
pub struct ActorVisual {
    pub player: bool,
}
#[derive(Component)]
struct AnimatedActor {
    player: bool,
    current: usize,
}
pub const CLIPS: [&str; 9] = [
    "Idle",
    "Walk",
    "Run",
    "Jump",
    "SpearIdle",
    "SpearWalk",
    "SpearRun",
    "SpearJump",
    "SpearThrust",
];
const SPEAR_LOCOMOTION_START: usize = 4;
const SPEAR_THRUST_CLIP: usize = 8;

#[derive(Resource)]
pub struct AnimationLibrary {
    pub graph: Handle<AnimationGraph>,
    pub nodes: [AnimationNodeIndex; CLIPS.len()],
}
#[derive(Resource, Default)]
pub(crate) struct MovementInput {
    direction: Vec2,
    sprint: bool,
    jump: bool,
    dodge: Option<Vec2>,
}

impl MovementInput {
    // Frame-based attacks can run before the next fixed step consumes the request.
    pub(crate) fn dodge_pending(&self, grounded: bool, dodge: &Dodge) -> bool {
        self.dodge.is_some() && grounded && dodge.ready()
    }
}

pub(crate) fn input(
    keys: Res<ButtonInput<KeyCode>>,
    dialogue: Res<Dialogue>,
    mut input: ResMut<MovementInput>,
) {
    input.direction = Vec2::ZERO;
    input.sprint = false;
    if dialogue.text.is_some()
        || keys.just_pressed(KeyCode::KeyI)
        || keys.just_pressed(KeyCode::Escape)
    {
        input.jump = false;
        input.dodge = None;
        return;
    }
    input.direction = Vec2::new(
        keys.pressed(KeyCode::KeyD) as i8 as f32 - keys.pressed(KeyCode::KeyA) as i8 as f32,
        keys.pressed(KeyCode::KeyW) as i8 as f32 - keys.pressed(KeyCode::KeyS) as i8 as f32,
    )
    .normalize_or_zero();
    input.sprint = keys.pressed(KeyCode::ShiftLeft);
    input.jump |= keys.just_pressed(KeyCode::Space);
    if keys.just_pressed(DODGE_KEY) {
        input.dodge = Some(input.direction);
    }
}

fn camera_heading(direction: Vec2, yaw: f32) -> Vec3 {
    let (sin, cos) = yaw.sin_cos();
    let forward = Vec3::new(-sin, 0.0, -cos);
    let right = Vec3::new(cos, 0.0, -sin);
    forward * direction.y + right * direction.x
}

pub(crate) fn movement(
    time: Res<Time>,
    orbit: Res<Orbit>,
    dialogue: Res<Dialogue>,
    thrust: Res<crate::spear::Thrust>,
    layout: Res<WorldLayout>,
    mut input: ResMut<MovementInput>,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
    mut players: Query<(Entity, &mut Transform, &Collider, &mut Player, &mut Dodge)>,
) {
    let dodge_request = input.dodge.take();
    let jump_requested = std::mem::take(&mut input.jump);
    let delta = time.delta_secs();
    let can_move = dialogue.text.is_none() && !thrust.active();
    for (entity, mut transform, collider, mut player, mut dodge) in &mut players {
        dodge.cooldown = (dodge.cooldown - delta).max(0.0);
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        // Let the hop descend almost to the surface before ending its motion.
        let ground_distance = if dodge.active() { 0.025 } else { 0.12 };
        let ground = spatial.cast_shape(
            &Collider::capsule(0.30, 1.15),
            transform.translation,
            Quat::IDENTITY,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(ground_distance),
            &filter,
        );
        player.grounded = (!player.jumping || player.velocity.y <= 0.1)
            && ground.is_some_and(|h| h.normal1.y > 0.7);
        if player.grounded || dialogue.text.is_some() {
            dodge.hop = None;
        }
        if player.grounded {
            player.jumping = false;
            player.velocity.y = -0.6;
        }
        if can_move && player.grounded {
            if let Some(direction) = dodge_request.filter(|_| dodge.ready()) {
                let heading = if direction == Vec2::ZERO {
                    (transform.rotation * Vec3::NEG_Z)
                        .with_y(0.0)
                        .normalize_or_zero()
                } else {
                    camera_heading(direction, orbit.yaw)
                };
                dodge.start(heading);
                player.launch(DODGE_LAUNCH_SPEED);
            } else if jump_requested {
                player.launch(4.7);
            }
        }
        let (horizontal, gravity) = if let Some(hop) = &dodge.hop {
            player.running = false;
            (hop.heading * DODGE_SPEED, DODGE_GRAVITY)
        } else {
            let heading = if can_move {
                camera_heading(input.direction, orbit.yaw)
            } else {
                Vec3::ZERO
            };
            player.running = input.sprint && heading.length_squared() > 0.01;
            let speed = if player.running { 5.5 } else { 3.0 };
            if heading.length_squared() > 0.01 {
                let target = Quat::from_rotation_y(heading.x.atan2(heading.z));
                transform.rotation = transform.rotation.slerp(target, (delta * 12.0).min(1.0));
            }
            (heading * speed, 5.0)
        };
        player.velocity.x = horizontal.x;
        player.velocity.z = horizontal.z;
        player.velocity.y = (player.velocity.y - gravity * delta).max(-22.0);
        let velocity = player.velocity;
        let output = mover.move_and_slide(
            collider,
            transform.translation,
            Quat::IDENTITY,
            velocity,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &filter,
            |hit| {
                // Suppress upward motion introduced by pushing against steep surfaces.
                if hit.normal.y < 0.7 && hit.velocity.y > velocity.y.max(0.0) {
                    hit.velocity.y = velocity.y.max(0.0);
                }
                MoveAndSlideHitResponse::Accept
            },
        );
        transform.translation = output.position;
        player.velocity.y = output.projected_velocity.y;
        dodge.advance(delta);
        if transform.translation.y < -15.0
            || transform.translation.x.abs() > 55.0
            || transform.translation.z > 35.0
            || transform.translation.z < -140.0
        {
            player.respawn(&mut dodge, &mut transform, layout.start());
        }
    }
}
pub(crate) fn bind_actor_animation(
    event: On<WorldInstanceReady>,
    library: Res<AnimationLibrary>,
    visuals: Query<&ActorVisual>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mut commands: Commands,
) {
    let Ok(actor) = visuals.get(event.entity) else {
        return;
    };
    for child in children.iter_descendants(event.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            let mut transitions = AnimationTransitions::new();
            transitions
                .play(&mut player, library.nodes[0], Duration::ZERO)
                .repeat();
            commands.entity(child).insert((
                AnimationGraphHandle(library.graph.clone()),
                transitions,
                AnimatedActor {
                    player: actor.player,
                    current: 0,
                },
            ));
        }
    }
}

fn animate(
    library: Res<AnimationLibrary>,
    thrust: Res<crate::spear::Thrust>,
    equipment: Res<crate::equipment::Equipment>,
    players: Query<(&Player, &Dodge)>,
    mut animations: Query<(
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        &mut AnimatedActor,
    )>,
) {
    let Some((player, dodge)) = players.iter().next() else {
        return;
    };
    let locomotion = if !player.grounded {
        3
    } else if player.velocity.xz().length_squared() < 0.1 {
        0
    } else if player.running {
        2
    } else {
        1
    };
    let desired = if thrust.active() {
        SPEAR_THRUST_CLIP
    } else if equipment.has_spear() {
        SPEAR_LOCOMOTION_START + locomotion
    } else {
        locomotion
    };
    for (mut animation, mut transitions, mut actor) in &mut animations {
        let index = if actor.player { desired } else { 0 };
        if index != actor.current {
            if index == SPEAR_THRUST_CLIP {
                transitions
                    .play(&mut animation, library.nodes[index], Duration::ZERO)
                    .replay();
            } else {
                // Do not blend free arm swings into the two-hand grip.
                let changes_grip =
                    (actor.current >= SPEAR_LOCOMOTION_START) != (index >= SPEAR_LOCOMOTION_START);
                let blend = if changes_grip || actor.current == SPEAR_THRUST_CLIP {
                    Duration::ZERO
                } else if actor.player && dodge.active() {
                    Duration::from_millis(50)
                } else {
                    Duration::from_millis(160)
                };
                transitions
                    .play(&mut animation, library.nodes[index], blend)
                    .repeat();
            }
            actor.current = index;
        }
    }
}

fn freeze(mut players: Query<&mut AnimationPlayer>, mut input: ResMut<MovementInput>) {
    *input = MovementInput::default();
    for mut player in &mut players {
        player.pause_all();
    }
}
fn resume(mut players: Query<&mut AnimationPlayer>) {
    for mut player in &mut players {
        player.resume_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::{CombatPlugin, Hit},
        inventory::{Inventory, ItemKind},
        spear::SpearPlugin,
    };
    use bevy::{
        asset::AssetPlugin, mesh::MeshPlugin, state::app::StatesPlugin, time::TimeUpdateStrategy,
        transform::TransformPlugin, window::PrimaryWindow,
    };

    const STEP: Duration = Duration::from_micros(15_625);

    fn test_app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            TransformPlugin,
            AssetPlugin::default(),
            MeshPlugin,
            PhysicsPlugins::default(),
        ))
        .insert_state(GameState::Playing)
        .insert_resource(TimeUpdateStrategy::ManualDuration(STEP))
        .insert_resource(WorldLayout::SmokeArena)
        .init_resource::<Dialogue>()
        .init_resource::<Orbit>()
        .init_resource::<Equipment>()
        .init_resource::<MovementInput>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins((CombatPlugin, SpearPlugin))
        .add_systems(PreUpdate, input.run_if(in_state(GameState::Playing)))
        .add_systems(FixedUpdate, movement.run_if(in_state(GameState::Playing)))
        .add_systems(OnEnter(GameState::Paused), freeze)
        .add_systems(OnEnter(GameState::Inventory), freeze);
        let world = app.world_mut();
        world.spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        world.spawn((
            RigidBody::Static,
            Collider::cuboid(20.0, 0.2, 20.0),
            Transform::from_xyz(0.0, -0.1, 0.0),
        ));
        let player = world
            .spawn((
                Player::default(),
                RigidBody::Kinematic,
                CustomPositionIntegration,
                Collider::capsule(0.32, 1.15),
                Transform::from_xyz(0.0, 0.905, 0.0),
            ))
            .id();
        app.finish();
        app.cleanup();
        for _ in 0..3 {
            app.update();
        }
        assert!(
            app.world()
                .get::<Player>(player)
                .is_some_and(|player| player.grounded)
        );
        (app, player)
    }

    fn tick(app: &mut App, count: usize) {
        for _ in 0..count {
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .clear();
        }
    }

    fn press(app: &mut App, keys: &[KeyCode]) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        for &key in keys {
            input.press(key);
        }
    }

    fn release(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
    }

    fn transform(app: &App, player: Entity) -> Transform {
        *app.world()
            .get::<Transform>(player)
            .expect("the movement fixture must have a transform")
    }

    fn active(app: &App, player: Entity) -> bool {
        app.world()
            .get::<Dodge>(player)
            .expect("Player must supply its Dodge component")
            .active()
    }

    #[test]
    fn neutral_hop_is_backward_and_held_input_does_not_repeat() {
        let (mut app, player) = test_app();
        let facing = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("the player must have a transform to face sideways")
            .rotation = facing;
        let start = transform(&app, player).translation;
        press(&mut app, &[DODGE_KEY, KeyCode::Space]);
        tick(&mut app, 10);
        let rise = transform(&app, player).translation.y - start.y;
        assert!(
            (0.10..0.18).contains(&rise),
            "dodge must take priority over the higher jump, rise {rise}"
        );
        tick(&mut app, 15);
        let landed = transform(&app, player);
        assert!((1.6..2.05).contains(&(start.x - landed.translation.x)));
        assert!((landed.translation.z - start.z).abs() < 0.02);
        assert!(landed.rotation.abs_diff_eq(facing, 0.001));
        assert!(!active(&app, player));
        tick(&mut app, 45);
        assert!(
            transform(&app, player)
                .translation
                .distance(landed.translation)
                < 0.02
        );
    }

    #[test]
    fn diagonal_hop_locks_camera_relative_heading_and_stops_at_walls() {
        let (mut app, player) = test_app();
        let world = app.world_mut();
        world.resource_mut::<Orbit>().yaw = std::f32::consts::FRAC_PI_2;
        world.spawn((
            RigidBody::Static,
            Collider::cuboid(0.2, 3.0, 8.0),
            Transform::from_xyz(-1.0, 1.0, 0.0),
        ));
        tick(&mut app, 2);
        press(&mut app, &[DODGE_KEY, KeyCode::KeyW, KeyCode::KeyD]);
        tick(&mut app, 2);
        let first = transform(&app, player).translation;
        assert!((first.x - first.z).abs() < 0.01 && first.x < -0.1);
        app.world_mut().resource_mut::<Orbit>().yaw = 0.0;
        release(&mut app, KeyCode::KeyW);
        release(&mut app, KeyCode::KeyD);
        tick(&mut app, 24);
        let end = transform(&app, player);
        assert!(
            end.translation.z < -1.0,
            "the hop must keep its initial heading and slide along the wall"
        );
        assert!(
            end.translation.x > -0.63 && end.translation.x < -0.53,
            "wall must stop the player capsule at its face, got {:?}",
            end.translation
        );
        assert!(end.rotation.abs_diff_eq(Quat::IDENTITY, 0.001));
    }

    #[test]
    fn airborne_dodge_is_discarded_and_cooldown_blocks_an_early_second_hop() {
        let (mut app, player) = test_app();
        press(&mut app, &[KeyCode::Space]);
        tick(&mut app, 3);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 130);
        assert!(!active(&app, player));
        assert!(transform(&app, player).translation.z.abs() < 0.02);

        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 23);
        assert!(!active(&app, player));
        let landed = transform(&app, player).translation;
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 3);
        assert!(!active(&app, player));
        assert!(transform(&app, player).translation.distance(landed) < 0.02);
        tick(&mut app, 12);
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 1);
        assert!(active(&app, player));
    }

    #[test]
    fn pending_dodge_beats_a_new_thrust_and_an_active_thrust_blocks_dodge() {
        let (mut app, player) = test_app();
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::FieldSpear, 1));
        assert_eq!(
            app.world_mut()
                .resource_mut::<Equipment>()
                .equip(&mut bag, ItemKind::FieldSpear),
            Ok(true)
        );
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        press(&mut app, &[DODGE_KEY]);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        tick(&mut app, 1);
        assert!(!active(&app, player), "no fixed step has run yet");
        assert!(
            !app.world().resource::<Thrust>().active(),
            "a pending dodge must reserve the player's action"
        );
        app.insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
        tick(&mut app, 1);
        assert!(active(&app, player));
        tick(&mut app, 40);
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.release(MouseButton::Left);
        mouse.press(MouseButton::Left);
        tick(&mut app, 1);
        assert!(app.world().resource::<Thrust>().active());
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 1);
        assert!(!active(&app, player));
    }

    #[test]
    fn a_hit_that_connects_during_a_dodge_still_damages_the_player() {
        let (mut app, player) = test_app();
        let attacker = app.world_mut().spawn_empty().id();
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 1);
        app.world_mut().write_message(Hit {
            source: attacker,
            target: player,
            damage: 20,
            contact: None,
        });
        tick(&mut app, 1);
        assert!(active(&app, player));
        let health = app
            .world()
            .get::<Health>(player)
            .expect("Player must supply its Health component");
        assert_eq!(health.current(), MAX_HEALTH - 20);
    }

    #[test]
    fn pause_preserves_a_hop_while_dialogue_and_respawn_cancel_it() {
        let (mut app, player) = test_app();
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 4);
        let position = transform(&app, player).translation;
        for state in [GameState::Paused, GameState::Inventory] {
            app.world_mut()
                .resource_mut::<NextState<GameState>>()
                .set(state);
            tick(&mut app, 40);
            assert!(active(&app, player));
            assert_eq!(transform(&app, player).translation, position);
        }
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.world_mut().resource_mut::<Dialogue>().text = Some("Talking to Nemi");
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 1);
        assert!(!active(&app, player));
        assert_eq!(transform(&app, player).translation.z, position.z);
        app.world_mut().resource_mut::<Dialogue>().text = None;
        tick(&mut app, 40);
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 2);
        assert!(active(&app, player));

        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("the fixture must have a transform to fall below the map")
            .translation
            .y = -16.0;
        tick(&mut app, 1);
        assert_eq!(
            transform(&app, player).translation,
            WorldLayout::SmokeArena.start()
        );
        let dodge = app
            .world()
            .get::<Dodge>(player)
            .expect("respawn must preserve the Dodge component");
        assert!(!dodge.active() && dodge.ready());
    }
}
