use crate::{GameState, camera::Orbit, quest::Dialogue, world::START};
use avian3d::prelude::*;
use bevy::prelude::*;
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
            .add_systems(
                Update,
                (attach_animations, animate)
                    .chain()
                    .after(crate::spear::SpearSystems)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
#[derive(Component, Default)]
pub struct Player {
    pub velocity: Vec3,
    pub grounded: bool,
    pub running: bool,
    jumping: bool,
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
        return;
    }
    input.direction = Vec2::new(
        keys.pressed(KeyCode::KeyD) as i8 as f32 - keys.pressed(KeyCode::KeyA) as i8 as f32,
        keys.pressed(KeyCode::KeyW) as i8 as f32 - keys.pressed(KeyCode::KeyS) as i8 as f32,
    )
    .normalize_or_zero();
    input.sprint = keys.pressed(KeyCode::ShiftLeft);
    input.jump |= keys.just_pressed(KeyCode::Space);
}
fn movement(
    time: Res<Time>,
    orbit: Res<Orbit>,
    dialogue: Res<Dialogue>,
    thrust: Res<crate::spear::Thrust>,
    mut input: ResMut<MovementInput>,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
    mut players: Query<(Entity, &mut Transform, &Collider, &mut Player)>,
) {
    for (entity, mut transform, collider, mut player) in &mut players {
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        let ground = spatial.cast_shape(
            &Collider::capsule(0.30, 1.15),
            transform.translation,
            Quat::IDENTITY,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(0.12),
            &filter,
        );
        player.grounded = (!player.jumping || player.velocity.y <= 0.1)
            && ground.is_some_and(|h| h.normal1.y > 0.7);
        let direction = if dialogue.text.is_some() || thrust.active() {
            Vec2::ZERO
        } else {
            input.direction
        };
        let forward = Vec3::new(-orbit.yaw.sin(), 0.0, -orbit.yaw.cos());
        let right = Vec3::new(orbit.yaw.cos(), 0.0, -orbit.yaw.sin());
        let heading = forward * direction.y + right * direction.x;
        player.running = input.sprint && heading.length_squared() > 0.01;
        let speed = if player.running { 5.5 } else { 3.0 };
        player.velocity.x = heading.x * speed;
        player.velocity.z = heading.z * speed;
        if heading.length_squared() > 0.01 {
            let target = Quat::from_rotation_y(heading.x.atan2(heading.z));
            transform.rotation = transform
                .rotation
                .slerp(target, (time.delta_secs() * 12.0).min(1.0));
        }
        if player.grounded {
            player.jumping = false;
            player.velocity.y = -0.6;
        }
        if input.jump && player.grounded && dialogue.text.is_none() && !thrust.active() {
            player.velocity.y = 4.7;
            player.jumping = true;
            player.grounded = false;
        }
        player.velocity.y = (player.velocity.y - 5.0 * time.delta_secs()).max(-22.0);
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
        if transform.translation.y < -15.0
            || transform.translation.x.abs() > 55.0
            || transform.translation.z > 35.0
            || transform.translation.z < -140.0
        {
            transform.translation = START;
            player.velocity = Vec3::ZERO;
            player.jumping = false;
        }
    }
    input.jump = false;
}
fn attach_animations(
    mut commands: Commands,
    library: Res<AnimationLibrary>,
    visuals: Query<(Entity, &ActorVisual)>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer, Without<AnimatedActor>>,
) {
    for (root, actor) in &visuals {
        for child in children.iter_descendants(root) {
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
}
fn animate(
    library: Res<AnimationLibrary>,
    thrust: Res<crate::spear::Thrust>,
    equipment: Res<crate::equipment::Equipment>,
    players: Query<&Player>,
    mut animations: Query<(
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        &mut AnimatedActor,
    )>,
) {
    let Some(player) = players.iter().next() else {
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
