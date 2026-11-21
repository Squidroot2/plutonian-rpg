use crate::{
    GameState,
    combat::{CombatSystems, Dead, DeathLifetime, Health, Hit, HitReaction, RemoveColliderOnDeath},
    combat_feedback::DeathAnimation,
    inventory::scene_ready,
    loading::{GameAssets, PendingScene},
    player::Player,
    quest::Dialogue,
};
use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};
use std::time::Duration;

pub const CLIPS: [&str; 4] = ["RoachIdle", "RoachCrawl", "RoachBite", "RoachDeath"];
pub(crate) const GROUND_OFFSET: f32 = 0.32;

const SIGHT_RANGE: f32 = 7.0;
const LEASH: f32 = 10.0;
const CHASE_SPEED: f32 = 3.5;
const BITE_RANGE: f32 = 1.5;
const WINDUP: f32 = 0.4;
const RECOVERY: f32 = 0.8;
const COLLAPSE: f32 = 0.7;
const BITE_DAMAGE: u16 = 12;

#[derive(Resource)]
pub struct RoachAnimations {
    pub graph: Handle<AnimationGraph>,
    pub nodes: [AnimationNodeIndex; CLIPS.len()],
}

#[derive(Component)]
pub struct Roach {
    pub(crate) phase: RoachPhase,
    home: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RoachPhase {
    Waiting,
    Pursuing,
    Returning,
    Windup(f32),
    Recovery(f32),
}

impl RoachPhase {
    /// Report the bite on the single tick that crosses the impact time.
    fn tick_attack(&mut self, delta: f32) -> bool {
        match *self {
            Self::Windup(remaining) if remaining <= delta => {
                // Carry the fractional tick into recovery to keep the clip and hit aligned.
                *self = Self::Recovery((RECOVERY + remaining - delta).max(0.0));
                true
            }
            Self::Windup(remaining) => {
                *self = Self::Windup(remaining - delta);
                false
            }
            Self::Recovery(remaining) if remaining <= delta => {
                *self = Self::Pursuing;
                false
            }
            Self::Recovery(remaining) => {
                *self = Self::Recovery(remaining - delta);
                false
            }
            _ => false,
        }
    }
}

#[derive(Component, Default)]
struct RoachMotion {
    destination: Option<Vec3>,
    moving: bool,
}

#[derive(Component)]
struct RoachGlow;
#[derive(Component)]
struct RoachPlayback {
    owner: Entity,
    clip: usize,
}

pub struct RoachPlugin;
impl Plugin for RoachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(GameState::Loading), ground_encounters)
            .add_systems(
                FixedUpdate,
                (plan_actions, move_roaches)
                    .chain()
                    .after(crate::player::movement)
                    .run_if(in_state(GameState::Playing))
                    .run_if(no_dialogue),
            )
            .add_systems(
                Update,
                (animate, light_hits)
                    .after(CombatSystems::Resolve)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn no_dialogue(dialogue: Res<Dialogue>) -> bool {
    dialogue.text.is_none()
}

pub(crate) fn spawn(
    commands: &mut Commands,
    assets: &GameAssets,
    models: &Assets<Gltf>,
    home: Vec3,
) -> Entity {
    let scene = models
        .get(&assets.roach)
        .expect("the validated relay roach model must remain loaded")
        .scenes[0]
        .clone();
    let body = Collider::cuboid(1.0, 0.65, 1.2);
    commands
        .spawn((
            Name::new("Relay roach"),
            Roach {
                phase: RoachPhase::Waiting,
                home,
            },
            Health::new(20),
            RoachMotion::default(),
            HitReaction::new(0.16),
            DeathLifetime(COLLAPSE),
            RemoveColliderOnDeath,
            Visibility::default(),
            Transform::from_translation(home),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            body,
        ))
        .with_children(|root| {
            root.spawn((PendingScene, WorldAssetRoot(scene), Transform::default()))
                .observe(scene_ready)
                .observe(bind_animation);
            root.spawn((
                RoachGlow,
                PointLight {
                    color: Color::srgb(1.0, 0.38, 0.14),
                    intensity: 280.0,
                    range: 2.2,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.35, 0.68),
            ));
        })
        .id()
}

fn ground_encounters(
    spatial: SpatialQuery,
    actors: Query<Entity, Or<(With<Player>, With<Roach>, With<crate::worm::Worm>)>>,
    mut roaches: Query<(&mut Roach, &mut Transform)>,
) {
    let filter = SpatialQueryFilter::from_excluded_entities(actors.iter());
    for (mut roach, mut transform) in &mut roaches {
        let origin = transform.translation + Vec3::Y * 2.0;
        if let Some(y) = surface(&spatial, &filter, origin, 15.0) {
            transform.translation.y = y;
            roach.home = transform.translation;
        }
    }
}

fn surface(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    origin: Vec3,
    range: f32,
) -> Option<f32> {
    spatial
        .cast_ray(origin, Dir3::NEG_Y, range, true, filter)
        .filter(|hit| hit.normal.y > 0.65)
        .map(|hit| origin.y - hit.distance + GROUND_OFFSET)
}

fn plan_actions(
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    player: Query<(Entity, &Transform), With<Player>>,
    mut roaches: Query<
        (Entity, &mut Roach, &mut Transform, &mut RoachMotion),
        (Without<Player>, Without<Dead>),
    >,
    mut hits: MessageWriter<Hit>,
) {
    let Ok((player_entity, player_transform)) = player.single() else {
        return;
    };
    let delta = time.delta_secs();
    let target = player_transform.translation;
    for (entity, mut roach, mut transform, mut motion) in &mut roaches {
        motion.destination = None;
        let position = transform.translation;
        let to_target = target - position;
        let sight = SpatialQueryFilter::from_excluded_entities([entity, player_entity]);
        if matches!(roach.phase, RoachPhase::Windup(_) | RoachPhase::Recovery(_)) {
            if roach.phase.tick_attack(delta) && bite_contact(&spatial, &sight, &transform, target)
            {
                hits.write(Hit {
                    source: entity,
                    target: player_entity,
                    damage: BITE_DAMAGE,
                    contact: None,
                });
            }
            continue;
        }

        let within_leash = position.xz().distance(roach.home.xz()) < LEASH
            && target.xz().distance(roach.home.xz()) < LEASH;
        let sees_player = within_leash
            && to_target.length_squared() < SIGHT_RANGE * SIGHT_RANGE
            && clear_path(&spatial, &sight, position + Vec3::Y * 0.15, target);
        let near_home = position.xz().distance(roach.home.xz()) < 0.2;

        let destination = if sees_player && (roach.phase != RoachPhase::Returning || near_home) {
            roach.phase = RoachPhase::Pursuing;
            if to_target.length_squared() <= BITE_RANGE * BITE_RANGE {
                transform.rotation = heading(to_target);
                roach.phase = RoachPhase::Windup(WINDUP);
                continue;
            }
            target
        } else if !near_home {
            roach.phase = RoachPhase::Returning;
            roach.home
        } else {
            roach.phase = RoachPhase::Waiting;
            continue;
        };

        motion.destination = Some(destination);
    }
}

fn move_roaches(
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
    actors: Query<Entity, Or<(With<Player>, With<Roach>, With<crate::worm::Worm>)>>,
    mut roaches: Query<(Entity, &mut Transform, &Collider, &mut RoachMotion), Without<Dead>>,
) {
    let delta = time.delta_secs();
    let support = SpatialQueryFilter::from_excluded_entities(actors.iter());
    for (entity, mut transform, collider, mut motion) in &mut roaches {
        motion.moving = false;
        let Some(destination) = motion.destination else {
            continue;
        };
        let position = transform.translation;
        let toward = (destination - position).with_y(0.0);
        if toward.length_squared() < 0.0001 {
            continue;
        }
        transform.rotation = transform
            .rotation
            .slerp(heading(toward), (10.0 * delta).min(1.0));
        let velocity = toward.clamp_length_max(CHASE_SPEED * delta) / delta;
        let proposed = position + velocity * delta;
        if surface(&spatial, &support, proposed + Vec3::Y * 0.25, 1.1)
            .is_none_or(|y| (y - position.y).abs() > 0.25)
        {
            continue;
        }
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        let output = mover.move_and_slide(
            collider,
            position,
            transform.rotation,
            velocity,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &filter,
            |_| MoveAndSlideHitResponse::Accept,
        );
        if let Some(y) = surface(&spatial, &support, output.position + Vec3::Y * 0.25, 1.1) {
            transform.translation = output.position.with_y(y);
            motion.moving = output.position.xz().distance_squared(position.xz()) > 0.000_001;
        }
    }
}

fn heading(delta: Vec3) -> Quat {
    Quat::from_rotation_y(delta.x.atan2(delta.z))
}

fn clear_path(spatial: &SpatialQuery, filter: &SpatialQueryFilter, from: Vec3, to: Vec3) -> bool {
    let delta = to - from;
    Dir3::new(delta).is_ok_and(|direction| {
        spatial
            .cast_ray(from, direction, delta.length(), true, filter)
            .is_none()
    })
}

fn bite_contact(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    roach: &Transform,
    target: Vec3,
) -> bool {
    let delta = target - roach.translation;
    let forward = roach.rotation * Vec3::Z;
    delta.length_squared() <= BITE_RANGE * BITE_RANGE
        && delta.xz().normalize_or_zero().dot(forward.xz()) >= 0.819 // 35 degrees.
        && clear_path(
            spatial,
            filter,
            roach.translation + forward * 0.34 + Vec3::Y * 0.15,
            target,
        )
}

fn bind_animation(
    event: On<WorldInstanceReady>,
    library: Res<RoachAnimations>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut animations: Query<&mut AnimationPlayer>,
    mut commands: Commands,
) {
    let Ok(parent) = parents.get(event.entity) else {
        return;
    };
    let mut players = Vec::new();
    for child in children.iter_descendants(event.entity) {
        if let Ok(mut animation) = animations.get_mut(child) {
            players.push(child);
            let mut transitions = AnimationTransitions::new();
            transitions
                .play(&mut animation, library.nodes[0], Duration::ZERO)
                .repeat();
            commands.entity(child).insert((
                AnimationGraphHandle(library.graph.clone()),
                transitions,
                RoachPlayback {
                    owner: parent.parent(),
                    clip: 0,
                },
            ));
        }
    }
    commands.entity(parent.parent()).insert(DeathAnimation {
        players,
        clip: library.nodes[3],
    });
}

fn animate(
    library: Res<RoachAnimations>,
    dialogue: Res<Dialogue>,
    roaches: Query<(&Roach, &RoachMotion, Has<Dead>)>,
    mut playing: Query<(
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        &mut RoachPlayback,
    )>,
) {
    for (mut animation, mut transitions, mut playback) in &mut playing {
        if dialogue.text.is_some() {
            animation.pause_all();
            continue;
        }
        animation.resume_all();
        let Ok((roach, motion, dead)) = roaches.get(playback.owner) else {
            continue;
        };
        if dead {
            continue;
        }
        let clip = match roach.phase {
            RoachPhase::Windup(_) | RoachPhase::Recovery(_) => 2,
            _ if motion.moving => 1,
            _ => 0,
        };
        if clip != playback.clip {
            let fade = if clip < 2 {
                Duration::from_millis(100)
            } else {
                Duration::ZERO
            };
            let active = transitions.play(&mut animation, library.nodes[clip], fade);
            active.replay();
            if clip < 2 {
                active.repeat();
            }
            playback.clip = clip;
        }
    }
}

fn light_hits(
    roaches: Query<
        (&HitReaction, &Children, Has<Dead>),
        (With<Roach>, Or<(Changed<HitReaction>, Added<Dead>)>),
    >,
    mut lights: Query<&mut PointLight, With<RoachGlow>>,
) {
    for (reaction, children, dead) in &roaches {
        for child in children.iter() {
            if let Ok(mut light) = lights.get_mut(child) {
                light.intensity = if dead {
                    0.0
                } else {
                    280.0 + 3500.0 * reaction.strength()
                };
            }
        }
    }
}
