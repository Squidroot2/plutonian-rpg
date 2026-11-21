use crate::{
    GameState,
    camera::Orbit,
    combat::{CombatSystems, Dead, Defense, Health, HealthRecovery},
    combat_feedback::DeathAnimation,
    controls::{Control, Keybinds},
    dungeon::Area,
    equipment::Equipment,
    interaction::InteractionSystems,
    inventory::{CARRY_CAPACITY, IMMOBILE_WEIGHT, Inventory},
    menu_ui::PlayerMenu,
    nemi::NemiPlayback,
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
            .add_systems(OnEnter(GameState::Looting), freeze)
            .add_systems(OnExit(GameState::Paused), resume)
            .add_systems(OnExit(GameState::Looting), resume)
            .add_systems(
                PreUpdate,
                input
                    .after(crate::ui::pause)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(FixedUpdate, movement.run_if(in_state(GameState::Playing)))
            .add_systems(Update, footsteps.run_if(in_state(GameState::Playing)))
            .add_systems(
                Update,
                face_camera
                    .after(InteractionSystems)
                    .before(CombatSystems::Detect)
                    .run_if(in_state(GameState::Playing)),
            )
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
#[require(HealthRecovery = HealthRecovery::new(Duration::from_secs(5), Duration::from_millis(100)))]
pub struct Player {
    pub(crate) area: Area,
    pub velocity: Vec3,
    pub grounded: bool,
    pub running: bool,
    jumping: bool,
    movement_direction: Vec2,
}
impl Player {
    pub(crate) fn respawn(&mut self, dodge: &mut Dodge, transform: &mut Transform, start: Vec3) {
        self.area = Area::Surface;
        transform.translation = start;
        self.velocity = Vec3::ZERO;
        self.jumping = false;
        self.grounded = false;
        self.running = false;
        self.movement_direction = Vec2::ZERO;
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
const WALK_SPEED: f32 = 3.0;
const SPRINT_SPEED: f32 = 5.5;

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

fn footsteps(
    time: Res<Time>,
    assets: Res<AssetServer>,
    players: Query<&Player>,
    mut elapsed: Local<f32>,
    mut commands: Commands,
) {
    let Some(player) = players.iter().next() else {
        return;
    };
    if !player.grounded || player.movement_direction == Vec2::ZERO {
        *elapsed = 0.0;
        return;
    }
    *elapsed -= time.delta_secs();
    if *elapsed <= 0.0 {
        commands.spawn((
            AudioPlayer::new(assets.load("sounds/footstep05.ogg")),
            PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(0.2)),
        ));
        *elapsed = if player.running { 0.32 } else { 0.5 };
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
    current: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gait {
    Idle,
    Walk,
    Run,
    Jump,
    WalkBack,
    StrafeLeft,
    StrafeRight,
    WalkForwardLeft,
    WalkForwardRight,
    WalkBackLeft,
    WalkBackRight,
}

impl Gait {
    fn for_player(player: &Player) -> Self {
        if !player.grounded {
            return Self::Jump;
        }
        // The normalized keyboard axes are each zero, ±1, or ±1/√2.
        match (
            player.movement_direction.x.round() as i8,
            player.movement_direction.y.round() as i8,
        ) {
            (0, 1) if player.running => Self::Run,
            (0, 1) => Self::Walk,
            (0, -1) => Self::WalkBack,
            (-1, 0) => Self::StrafeLeft,
            (1, 0) => Self::StrafeRight,
            (-1, 1) => Self::WalkForwardLeft,
            (1, 1) => Self::WalkForwardRight,
            (-1, -1) => Self::WalkBackLeft,
            (1, -1) => Self::WalkBackRight,
            _ => Self::Idle,
        }
    }

    fn playback_speed(self, running: bool) -> f32 {
        if running && matches!(self, Self::WalkForwardLeft | Self::WalkForwardRight) {
            SPRINT_SPEED / WALK_SPEED
        } else {
            1.0
        }
    }
}

const SPEAR_LOCOMOTION_START: usize = Gait::WalkBackRight as usize + 1;
pub(crate) const WALK_CLIP: usize = Gait::Walk as usize;
pub(crate) const RUN_CLIP: usize = Gait::Run as usize;
const SPEAR_THRUST_CLIP: usize = SPEAR_LOCOMOTION_START * 2;
const NEMI_DEATH_CLIP: usize = SPEAR_THRUST_CLIP + 1;
pub const CLIPS: [&str; NEMI_DEATH_CLIP + 1] = [
    "Idle",
    "Walk",
    "Run",
    "Jump",
    "WalkBack",
    "StrafeLeft",
    "StrafeRight",
    "WalkForwardLeft",
    "WalkForwardRight",
    "WalkBackLeft",
    "WalkBackRight",
    "SpearIdle",
    "SpearWalk",
    "SpearRun",
    "SpearJump",
    "SpearWalkBack",
    "SpearStrafeLeft",
    "SpearStrafeRight",
    "SpearWalkForwardLeft",
    "SpearWalkForwardRight",
    "SpearWalkBackLeft",
    "SpearWalkBackRight",
    "SpearThrust",
    "NemiDeath",
];

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
    bindings: Res<Keybinds>,
    dialogue: Res<Dialogue>,
    mut input: ResMut<MovementInput>,
    log: Option<Res<PlayerMenu>>,
) {
    input.direction = Vec2::ZERO;
    input.sprint = false;
    if dialogue.text.is_some()
        || log.is_some_and(|log| log.is_open())
        || keys.just_pressed(KeyCode::Escape)
    {
        input.jump = false;
        input.dodge = None;
        return;
    }
    input.direction = Vec2::new(
        bindings.pressed(Control::Right, &keys) as i8 as f32
            - bindings.pressed(Control::Left, &keys) as i8 as f32,
        bindings.pressed(Control::Forward, &keys) as i8 as f32
            - bindings.pressed(Control::Backward, &keys) as i8 as f32,
    )
    .normalize_or_zero();
    input.sprint = bindings.pressed(Control::Sprint, &keys);
    input.jump |= bindings.just_pressed(Control::Jump, &keys);
    if bindings.just_pressed(Control::Dodge, &keys) {
        input.dodge = Some(input.direction);
    }
}

fn camera_heading(direction: Vec2, yaw: f32) -> Vec3 {
    let (sin, cos) = yaw.sin_cos();
    let forward = Vec3::new(-sin, 0.0, -cos);
    let right = Vec3::new(cos, 0.0, -sin);
    forward * direction.y + right * direction.x
}

pub(crate) fn face_camera(orbit: Res<Orbit>, mut players: Query<&mut Transform, With<Player>>) {
    // The character's model faces +Z. The camera looks inward along its orbit.
    let facing = Quat::from_rotation_y(orbit.yaw + std::f32::consts::PI);
    for mut transform in &mut players {
        if transform.rotation != facing {
            transform.rotation = facing;
        }
    }
}

pub(crate) fn movement(
    time: Res<Time>,
    orbit: Res<Orbit>,
    dialogue: Res<Dialogue>,
    thrust: Res<crate::spear::Thrust>,
    layout: Res<WorldLayout>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    mut input: ResMut<MovementInput>,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
    mut players: Query<(Entity, &mut Transform, &Collider, &mut Player, &mut Dodge)>,
) {
    let dodge_request = input.dodge.take();
    let jump_requested = std::mem::take(&mut input.jump);
    let delta = time.delta_secs();
    let weight = inventory.carried_weight(&equipment);
    let agile = weight <= CARRY_CAPACITY;
    let can_move = weight <= IMMOBILE_WEIGHT && dialogue.text.is_none() && !thrust.active();
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
        if player.grounded || !can_move || !agile {
            dodge.hop = None;
        }
        if player.grounded {
            player.jumping = false;
            player.velocity.y = -0.6;
        }
        if can_move && agile && player.grounded {
            if let Some(direction) = dodge_request.filter(|_| dodge.ready()) {
                let heading = if direction == Vec2::ZERO {
                    camera_heading(Vec2::NEG_Y, orbit.yaw)
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
            player.movement_direction = Vec2::ZERO;
            (hop.heading * DODGE_SPEED, DODGE_GRAVITY)
        } else {
            let direction = if can_move {
                input.direction
            } else {
                Vec2::ZERO
            };
            player.movement_direction = direction;
            player.running = agile && input.sprint && direction.y > 0.0;
            let speed = if player.running {
                SPRINT_SPEED
            } else {
                WALK_SPEED
            };
            (camera_heading(direction, orbit.yaw) * speed, 5.0)
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
    visuals: Query<(&ActorVisual, &ChildOf)>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mut commands: Commands,
) {
    let Ok((actor, parent)) = visuals.get(event.entity) else {
        return;
    };
    let mut death_players = Vec::new();
    for child in children.iter_descendants(event.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            let mut transitions = AnimationTransitions::new();
            transitions
                .play(&mut player, library.nodes[0], Duration::ZERO)
                .repeat();
            commands
                .entity(child)
                .insert((AnimationGraphHandle(library.graph.clone()), transitions));
            if actor.player {
                commands.entity(child).insert(AnimatedActor { current: 0 });
            } else {
                death_players.push(child);
                commands.entity(child).insert(NemiPlayback {
                    owner: parent.parent(),
                    clip: 0,
                });
            }
        }
    }
    if !death_players.is_empty() {
        commands.entity(parent.parent()).insert(DeathAnimation {
            players: death_players,
            clip: library.nodes[NEMI_DEATH_CLIP],
        });
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
    let gait = Gait::for_player(player);
    let desired = if thrust.active() {
        SPEAR_THRUST_CLIP
    } else if equipment.has_spear() {
        SPEAR_LOCOMOTION_START + gait as usize
    } else {
        gait as usize
    };
    for (mut animation, mut transitions, mut actor) in &mut animations {
        let index = desired;
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
                } else if dodge.active() {
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
        if index != SPEAR_THRUST_CLIP {
            let speed = gait.playback_speed(player.running);
            if let Some(clip) = animation.animation_mut(library.nodes[index])
                && clip.speed() != speed
            {
                clip.set_speed(speed);
            }
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
        inventory::ItemKind,
        spear::SpearPlugin,
    };
    use bevy::{
        asset::{AssetApp, AssetPlugin},
        mesh::MeshPlugin,
        state::app::StatesPlugin,
        time::TimeUpdateStrategy,
        transform::TransformPlugin,
        window::PrimaryWindow,
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
        .init_asset::<AudioSource>()
        .insert_state(GameState::Playing)
        .insert_resource(TimeUpdateStrategy::ManualDuration(STEP))
        .insert_resource(WorldLayout::SmokeArena)
        .init_resource::<Dialogue>()
        .init_resource::<Orbit>()
        .init_resource::<Equipment>()
        .init_resource::<Inventory>()
        .init_resource::<MovementInput>()
        .init_resource::<Keybinds>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins((CombatPlugin, SpearPlugin))
        .add_systems(PreUpdate, input.run_if(in_state(GameState::Playing)))
        .add_systems(FixedUpdate, movement.run_if(in_state(GameState::Playing)))
        .add_systems(Update, footsteps.run_if(in_state(GameState::Playing)))
        .add_systems(Update, face_camera.before(CombatSystems::Detect))
        .add_systems(OnEnter(GameState::Paused), freeze);
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
    fn movement_uses_the_rebound_forward_key() {
        let (mut app, player) = test_app();
        app.world_mut()
            .resource_mut::<Keybinds>()
            .assign(
                Control::Forward,
                crate::controls::Binding::Key(KeyCode::ArrowUp),
            )
            .expect("the up arrow must be available for forward movement");

        let start = transform(&app, player).translation.z;
        press(&mut app, &[KeyCode::KeyW]);
        tick(&mut app, 3);
        assert_eq!(transform(&app, player).translation.z, start);
        press(&mut app, &[KeyCode::ArrowUp]);
        tick(&mut app, 3);
        assert!(transform(&app, player).translation.z < start - 0.1);
    }

    #[test]
    fn idle_player_faces_camera_yaw_even_without_a_fixed_step() {
        let (mut app, player) = test_app();
        let start = transform(&app, player).translation;
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        let mut orbit = app.world_mut().resource_mut::<Orbit>();
        orbit.yaw = std::f32::consts::FRAC_PI_2;
        orbit.pitch = 0.8;
        tick(&mut app, 1);
        let facing = transform(&app, player);
        assert_eq!(facing.translation, start);
        assert!((facing.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_X, 0.001));
        assert!((facing.rotation * Vec3::Y).abs_diff_eq(Vec3::Y, 0.001));
    }

    #[test]
    fn camera_relative_walking_and_sprint_choose_the_expected_gaits() {
        let (mut app, player) = test_app();
        app.world_mut().resource_mut::<Orbit>().yaw = std::f32::consts::FRAC_PI_2;
        let cases: &[(&[KeyCode], Vec3, Gait, bool)] = &[
            (&[KeyCode::KeyW], Vec3::NEG_X, Gait::Run, true),
            (&[KeyCode::KeyA], Vec3::Z, Gait::StrafeLeft, false),
            (&[KeyCode::KeyD], Vec3::NEG_Z, Gait::StrafeRight, false),
            (&[KeyCode::KeyS], Vec3::X, Gait::WalkBack, false),
            (
                &[KeyCode::KeyW, KeyCode::KeyA],
                Vec3::NEG_X + Vec3::Z,
                Gait::WalkForwardLeft,
                true,
            ),
            (
                &[KeyCode::KeyW, KeyCode::KeyD],
                Vec3::NEG_X + Vec3::NEG_Z,
                Gait::WalkForwardRight,
                true,
            ),
            (
                &[KeyCode::KeyS, KeyCode::KeyA],
                Vec3::X + Vec3::Z,
                Gait::WalkBackLeft,
                false,
            ),
            (
                &[KeyCode::KeyS, KeyCode::KeyD],
                Vec3::X + Vec3::NEG_Z,
                Gait::WalkBackRight,
                false,
            ),
        ];
        for &(keys, heading, gait, sprint) in cases {
            press(&mut app, &[KeyCode::ShiftLeft]);
            press(&mut app, keys);
            let start = transform(&app, player).translation;
            tick(&mut app, 3);
            let end = transform(&app, player);
            let controller = app
                .world()
                .get::<Player>(player)
                .expect("the movement fixture must have a controller");
            let meters = if sprint { 5.5 } else { 3.0 } * STEP.as_secs_f32() * 3.0;
            assert!(
                (end.translation - start)
                    .xz()
                    .abs_diff_eq((heading.normalize() * meters).xz(), 0.003)
            );
            assert!((end.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_X, 0.001));
            assert_eq!(Gait::for_player(controller), gait);
            assert_eq!(controller.running, sprint);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
        }
        assert_eq!(
            Gait::WalkForwardRight.playback_speed(true),
            SPRINT_SPEED / WALK_SPEED
        );
        assert_eq!(Gait::WalkForwardRight.playback_speed(false), 1.0);
    }

    #[test]
    fn neutral_hop_is_backward_and_held_input_does_not_repeat() {
        let (mut app, player) = test_app();
        app.world_mut().resource_mut::<Orbit>().yaw = std::f32::consts::FRAC_PI_2;
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
        assert!((1.6..2.05).contains(&(landed.translation.x - start.x)));
        assert!((landed.translation.z - start.z).abs() < 0.02);
        assert!((landed.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_X, 0.001));
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
        tick(&mut app, 1);
        assert!(active(&app, player));
        assert!((transform(&app, player).rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 0.001));
        tick(&mut app, 23);
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
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
        tick(&mut app, 40);
        assert!(active(&app, player));
        assert_eq!(transform(&app, player).translation, position);
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
