use crate::{
    GameState,
    combat::{CombatSystems, Dead, DeathLifetime, Health, Hit, HitReaction, RemoveColliderOnDeath},
    combat_feedback::DeathAnimation,
    inventory::scene_ready,
    loading::{GameAssets, PendingScene},
    player::Player,
    quest::Dialogue,
    world::{SpawnWorld, WorldLayout},
};
use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};
use std::time::Duration;

pub const CLIPS: [&str; 4] = ["WormIdle", "WormCrawl", "WormBite", "WormDeath"];
pub(crate) const GROUND_OFFSET: f32 = 0.56;
const HOMES: [Vec3; 3] = [
    Vec3::new(6.0, 2.0, -27.0),
    Vec3::new(-7.0, 2.0, -71.0),
    Vec3::new(3.4, 2.0, -110.0),
];
const SIGHT_RANGE: f32 = 7.0;
const LEASH: f32 = 10.0;
const CHASE_SPEED: f32 = 2.4;
const BITE_RANGE: f32 = 1.5;
const WINDUP: f32 = 0.55;
const RECOVERY: f32 = 0.85;
const COLLAPSE: f32 = 0.70;
const BITE_DAMAGE: u16 = 20;

#[derive(Resource)]
pub struct WormAnimations {
    pub graph: Handle<AnimationGraph>,
    pub nodes: [AnimationNodeIndex; CLIPS.len()],
}

#[derive(Component)]
pub struct Worm {
    pub(crate) phase: WormPhase,
    home: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WormPhase {
    Waiting,
    Pursuing,
    Returning,
    Windup(f32),
    Recovery(f32),
}

impl WormPhase {
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
struct WormMotion {
    destination: Option<Vec3>,
    moving: bool,
}

#[derive(Component)]
struct WormGlow;
#[derive(Component)]
struct WormPlayback {
    owner: Entity,
    clip: usize,
}

pub struct WormPlugin;
impl Plugin for WormPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_encounters)
            .add_systems(OnExit(GameState::Loading), ground_encounters)
            .add_systems(
                FixedUpdate,
                (plan_actions, move_worms)
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

fn spawn_encounters(
    _: On<SpawnWorld>,
    mut commands: Commands,
    layout: Res<WorldLayout>,
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
) {
    if *layout == WorldLayout::Outpost {
        for home in HOMES {
            spawn(&mut commands, &assets, &models, home);
        }
    }
}

pub(crate) fn spawn(
    commands: &mut Commands,
    assets: &GameAssets,
    models: &Assets<Gltf>,
    home: Vec3,
) -> Entity {
    let scene = models
        .get(&assets.worm)
        .expect("the validated ice worm model must remain loaded")
        .scenes[0]
        .clone();
    let body = Collider::compound(vec![
        (
            Vec3::new(0.0, 0.0, 0.30),
            Quat::IDENTITY,
            Collider::sphere(0.47),
        ),
        (
            Vec3::new(0.0, -0.14, -0.33),
            Quat::IDENTITY,
            Collider::sphere(0.39),
        ),
        (
            Vec3::new(0.0, -0.32, -0.75),
            Quat::IDENTITY,
            Collider::sphere(0.23),
        ),
    ]);
    commands
        .spawn((
            Name::new("Ice worm"),
            Worm {
                phase: WormPhase::Waiting,
                home,
            },
            Health::new(30),
            WormMotion::default(),
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
                WormGlow,
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
    actors: Query<Entity, Or<(With<Player>, With<Worm>)>>,
    mut worms: Query<(&mut Worm, &mut Transform)>,
) {
    let filter = SpatialQueryFilter::from_excluded_entities(actors.iter());
    for (mut worm, mut transform) in &mut worms {
        let origin = transform.translation + Vec3::Y * 2.0;
        if let Some(y) = surface(&spatial, &filter, origin, 15.0) {
            transform.translation.y = y;
            worm.home = transform.translation;
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
    mut worms: Query<
        (Entity, &mut Worm, &mut Transform, &mut WormMotion),
        (Without<Player>, Without<Dead>),
    >,
    mut hits: MessageWriter<Hit>,
) {
    let Ok((player_entity, player_transform)) = player.single() else {
        return;
    };
    let delta = time.delta_secs();
    let target = player_transform.translation;
    for (entity, mut worm, mut transform, mut motion) in &mut worms {
        motion.destination = None;
        let position = transform.translation;
        let to_target = target - position;
        let sight = SpatialQueryFilter::from_excluded_entities([entity, player_entity]);
        if matches!(worm.phase, WormPhase::Windup(_) | WormPhase::Recovery(_)) {
            if worm.phase.tick_attack(delta) && bite_contact(&spatial, &sight, &transform, target) {
                hits.write(Hit {
                    source: entity,
                    target: player_entity,
                    damage: BITE_DAMAGE,
                    contact: None,
                });
            }
            continue;
        }

        let within_leash = position.xz().distance(worm.home.xz()) < LEASH
            && target.xz().distance(worm.home.xz()) < LEASH;
        let sees_player = within_leash
            && to_target.length_squared() < SIGHT_RANGE * SIGHT_RANGE
            && clear_path(&spatial, &sight, position + Vec3::Y * 0.15, target);
        let near_home = position.xz().distance(worm.home.xz()) < 0.2;

        let destination = if sees_player && (worm.phase != WormPhase::Returning || near_home) {
            worm.phase = WormPhase::Pursuing;
            if to_target.length_squared() <= BITE_RANGE * BITE_RANGE {
                transform.rotation = heading(to_target);
                worm.phase = WormPhase::Windup(WINDUP);
                continue;
            }
            target
        } else if !near_home {
            worm.phase = WormPhase::Returning;
            worm.home
        } else {
            worm.phase = WormPhase::Waiting;
            continue;
        };

        motion.destination = Some(destination);
    }
}

fn move_worms(
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
    actors: Query<Entity, Or<(With<Player>, With<Worm>)>>,
    mut worms: Query<(Entity, &mut Transform, &Collider, &mut WormMotion), Without<Dead>>,
) {
    let delta = time.delta_secs();
    let support = SpatialQueryFilter::from_excluded_entities(actors.iter());
    for (entity, mut transform, collider, mut motion) in &mut worms {
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
    worm: &Transform,
    target: Vec3,
) -> bool {
    let delta = target - worm.translation;
    let forward = worm.rotation * Vec3::Z;
    delta.length_squared() <= BITE_RANGE * BITE_RANGE
        && delta.xz().normalize_or_zero().dot(forward.xz()) >= 0.819 // 35 degrees.
        && clear_path(
            spatial,
            filter,
            worm.translation + forward * 0.34 + Vec3::Y * 0.15,
            target,
        )
}

fn bind_animation(
    event: On<WorldInstanceReady>,
    library: Res<WormAnimations>,
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
                WormPlayback {
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
    library: Res<WormAnimations>,
    dialogue: Res<Dialogue>,
    worms: Query<(&Worm, &WormMotion, Has<Dead>)>,
    mut playing: Query<(
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        &mut WormPlayback,
    )>,
) {
    for (mut animation, mut transitions, mut playback) in &mut playing {
        if dialogue.text.is_some() {
            animation.pause_all();
            continue;
        }
        animation.resume_all();
        let Ok((worm, motion, dead)) = worms.get(playback.owner) else {
            continue;
        };
        if dead {
            continue;
        }
        let clip = match worm.phase {
            WormPhase::Windup(_) | WormPhase::Recovery(_) => 2,
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
    worms: Query<
        (&HitReaction, &Children, Has<Dead>),
        (With<Worm>, Or<(Changed<HitReaction>, Added<Dead>)>),
    >,
    mut lights: Query<&mut PointLight, With<WormGlow>>,
) {
    for (reaction, children, dead) in &worms {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::{CombatPlugin, ConfirmedHit, Defense},
        equipment::Equipment,
        inventory::{Inventory, ItemKind},
        player::{MAX_HEALTH, respawn_defeated},
        quest::Quest,
        spear::Thrust,
        world::WorldLayout,
    };
    use bevy::{
        asset::AssetPlugin, ecs::system::RunSystemOnce, mesh::MeshPlugin, state::app::StatesPlugin,
        time::TimeUpdateStrategy, transform::TransformPlugin,
    };

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
        .insert_resource(WormAnimations {
            graph: Handle::default(),
            nodes: [AnimationNodeIndex::new(0); CLIPS.len()],
        })
        .init_state::<GameState>()
        .init_resource::<WorldLayout>()
        .init_resource::<Dialogue>()
        .init_resource::<Equipment>()
        .init_resource::<Thrust>()
        .add_plugins((CombatPlugin, WormPlugin))
        .add_systems(
            Update,
            respawn_defeated
                .after(CombatSystems::Resolve)
                .run_if(in_state(GameState::Playing)),
        );
        app.finish();
        app.cleanup();
        app
    }

    #[test]
    fn windup_produces_exactly_one_impact() {
        let mut attack = WormPhase::Windup(WINDUP);
        assert!(!attack.tick_attack(0.4));
        assert!(attack.tick_attack(0.2));
        assert!(!attack.tick_attack(0.3));
        assert!(!attack.tick_attack(0.6));
        assert_eq!(attack, WormPhase::Pursuing);
        assert!(!attack.tick_attack(0.6));
    }

    #[test]
    fn bite_requires_reach_facing_and_an_unobstructed_path() {
        let mut app = test_app();
        let facing = Transform::default();
        let check = |app: &mut App| {
            app.world_mut()
                .run_system_once(move |spatial: SpatialQuery| {
                    let filter = SpatialQueryFilter::default();
                    [
                        Vec3::Z * 1.4,
                        Vec3::Z * 1.6,
                        Vec3::NEG_Z,
                        Vec3::X,
                        Vec3::new(0.0, 1.8, 0.6),
                    ]
                    .map(|target| bite_contact(&spatial, &filter, &facing, target))
                })
                .expect("bite tests require an initialized Avian spatial query")
        };
        assert_eq!(check(&mut app), [true, false, false, false, false]);
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(1.0, 2.0, 0.12),
            Transform::from_xyz(0.0, 0.0, 0.8),
        ));
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(check(&mut app), [false; 5]);
    }

    #[test]
    fn simultaneous_bites_produce_one_damage_pulse() {
        let mut app = test_app();
        let world = app.world_mut();
        let player = world
            .spawn((Player::default(), Transform::from_xyz(0.0, 0.9, 1.2)))
            .id();
        for x in [-0.1, 0.1] {
            world.spawn((
                Worm {
                    phase: WormPhase::Windup(0.001),
                    home: Vec3::ZERO,
                },
                WormMotion::default(),
                Transform::from_xyz(x, GROUND_OFFSET, 0.0),
            ));
        }
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        let mut events = world
            .resource::<Messages<ConfirmedHit>>()
            .get_cursor_current();
        let mut pulses = 0;
        for _ in 0..5 {
            app.update();
            pulses += events
                .read(app.world().resource::<Messages<ConfirmedHit>>())
                .filter(|&&feedback| feedback.target == player)
                .count();
        }
        let health = app
            .world()
            .get::<Health>(player)
            .expect("the player must survive the bites");
        assert_eq!(health.current(), 80);
        assert_eq!(pulses, 1, "protection must suppress the second pulse");
    }

    #[test]
    fn paused_inventory_and_dialogue_freeze_windup_and_defeat_preserves_progress() {
        let mut app = test_app();
        let player = app
            .world_mut()
            .spawn((Player::default(), Transform::from_xyz(0.0, 0.9, 1.3)))
            .id();
        let worm = app
            .world_mut()
            .spawn((
                Worm {
                    phase: WormPhase::Windup(0.1),
                    home: Vec3::ZERO,
                },
                WormMotion::default(),
                Transform::from_xyz(0.0, GROUND_OFFSET, 0.0),
            ))
            .id();
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::FieldArmor, 1));
        assert!(bag.add(ItemKind::BeaconCore, 1));
        app.world_mut()
            .resource_mut::<Equipment>()
            .equip(&mut bag, ItemKind::FieldArmor)
            .expect("the armor fixture must be equipable");
        app.insert_resource(bag).insert_resource(Quest::ReturnCore);
        app.world_mut().entity_mut(player).insert((
            Health::new(MAX_HEALTH),
            Defense(2),
            crate::combat::HitProtection::new(0.0),
        ));
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.world_mut().resource_mut::<Dialogue>().text = Some("Talking to Nemi");
        app.world_mut().write_message(Hit {
            source: worm,
            target: player,
            damage: MAX_HEALTH,
            contact: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<Health>(player)
                .expect("test player must have health")
                .current(),
            2
        );

        app.world_mut().resource_mut::<Dialogue>().text = Some("Talking to Nemi");
        for state in [GameState::Paused, GameState::Inventory, GameState::Playing] {
            app.world_mut()
                .resource_mut::<NextState<GameState>>()
                .set(state);
            for _ in 0..10 {
                app.update();
            }
            assert_eq!(
                app.world()
                    .get::<Worm>(worm)
                    .expect("worm must survive while waiting")
                    .phase,
                WormPhase::Windup(0.1)
            );
        }
        app.world_mut().resource_mut::<Dialogue>().text = None;
        let mut events = app
            .world()
            .resource::<Messages<ConfirmedHit>>()
            .get_cursor_current();
        let mut pulses = 0;
        for _ in 0..10 {
            app.update();
            pulses += events
                .read(app.world().resource::<Messages<ConfirmedHit>>())
                .filter(|&&feedback| feedback.target == player)
                .count();
        }
        assert_eq!(
            pulses, 1,
            "a lethal bite must produce one pulse through respawn"
        );

        let world = app.world();
        assert_eq!(
            world
                .get::<Health>(player)
                .expect("defeat keeps the player entity")
                .current(),
            MAX_HEALTH
        );
        assert_eq!(
            world
                .get::<Transform>(player)
                .expect("the player must have a transform")
                .translation,
            WorldLayout::Outpost.start()
        );
        assert_eq!(*world.resource::<Quest>(), Quest::ReturnCore);
        assert_eq!(world.resource::<Inventory>().count(ItemKind::BeaconCore), 1);
        assert_eq!(world.resource::<Equipment>().total_defense(), 2);
    }
}
