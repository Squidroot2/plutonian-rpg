use crate::{
    GameState,
    combat::{CombatSystems, Dead, Health, Hit, HitReaction, RemoveColliderOnDeath},
    combat_feedback::DeathAnimation,
    dungeon::Area,
    interaction::CorpseLoot,
    inventory::ItemKind,
    loading::{PendingScene, scene_ready},
    player::Player,
    quest::Dialogue,
    terrain::{MAX_GROUND_STEP, ground_heading, walkable_height},
    world::SessionEntity,
};
use avian3d::prelude::*;
use bevy::{prelude::*, world_serialization::WorldInstanceReady};
use std::time::Duration;

const SIGHT_RANGE: f32 = 7.0;
const LEASH: f32 = 10.0;
const GLOW_INTENSITY: f32 = 280.0;
const HIT_GLOW_INTENSITY: f32 = 3500.0;
const HIT_REACTION_SECONDS: f32 = 0.16;
const SIGHT_ORIGIN_HEIGHT: f32 = 0.15;
const HOME_ARRIVAL_RADIUS: f32 = 0.2;
const TURN_SPEED: f32 = 10.0;
const GROUND_PROBE_RANGE: f32 = 1.1;
const ENCOUNTER_GROUND_PROBE_HEIGHT: f32 = 2.0;
const ENCOUNTER_GROUND_PROBE_RANGE: f32 = 15.0;
const DESTINATION_EPSILON_SQUARED: f32 = 0.0001;
const ANIMATION_MOVEMENT_EPSILON_SQUARED: f32 = 0.000_001;
const LOCOMOTION_BLEND: Duration = Duration::from_millis(100);

pub(crate) struct EnemyModel {
    pub(crate) path: &'static str,
    pub(crate) clips: [&'static str; 4],
}

pub(crate) struct EnemyVisual {
    pub(crate) scene: Handle<WorldAsset>,
    pub(crate) animations: EnemyAnimations,
}

#[derive(Component, Clone)]
pub(crate) struct EnemyAnimations {
    pub(crate) graph: Handle<AnimationGraph>,
    pub(crate) nodes: [AnimationNodeIndex; 4],
}

#[derive(Component)]
pub(crate) struct Enemy;

#[derive(Clone, Copy)]
pub(crate) struct Bite {
    pub(crate) range: f32,
    pub(crate) windup: f32,
    pub(crate) recovery: f32,
    pub(crate) damage: u16,
}

#[derive(Component)]
pub(crate) struct MeleePursuit {
    pub(crate) phase: MeleePhase,
    home: Vec3,
    bite: Bite,
}

impl MeleePursuit {
    fn new(home: Vec3, bite: Bite) -> Self {
        Self {
            phase: MeleePhase::Waiting,
            home,
            bite,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MeleePhase {
    Waiting,
    Pursuing,
    Returning,
    Windup(f32),
    Recovery(f32),
}

impl MeleePhase {
    /// Report the bite on the single tick that crosses the impact time.
    fn tick_attack(&mut self, delta: f32, recovery: f32) -> bool {
        match *self {
            Self::Windup(remaining) if remaining <= delta => {
                // Carry the fractional tick into recovery to keep the clip and hit aligned.
                *self = Self::Recovery((recovery + remaining - delta).max(0.0));
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

#[derive(Component)]
pub(crate) struct GroundMovement {
    speed: f32,
    ground_offset: f32,
    destination: Option<Vec3>,
    moving: bool,
}

impl GroundMovement {
    pub(crate) const fn new(speed: f32, ground_offset: f32) -> Self {
        Self {
            speed,
            ground_offset,
            destination: None,
            moving: false,
        }
    }
}

#[derive(Component)]
struct HitGlow;

#[derive(Component)]
struct EnemyPlayback {
    owner: Entity,
    clip: usize,
}

pub(crate) struct EnemyPlugin;
impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(GameState::Loading), ground_encounters)
            .add_systems(
                FixedUpdate,
                (plan_actions, move_ground_enemies)
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

pub(crate) struct GroundMelee {
    pub(crate) name: &'static str,
    pub(crate) health: u16,
    pub(crate) experience: u16,
    pub(crate) loot: ItemKind,
    pub(crate) movement: GroundMovement,
    pub(crate) bite: Bite,
}

pub(crate) fn spawn_ground_melee(
    commands: &mut Commands,
    marker: impl Component,
    visual: &EnemyVisual,
    home: Vec3,
    area: Area,
    config: GroundMelee,
    body: Collider,
) -> Entity {
    commands
        .spawn((
            Enemy,
            marker,
            Name::new(config.name),
            MeleePursuit::new(home, config.bite),
            config.movement,
            visual.animations.clone(),
            Health::new(config.health),
            HitReaction::new(HIT_REACTION_SECONDS),
            CorpseLoot::new(config.loot, area),
            RemoveColliderOnDeath,
            Visibility::default(),
            Transform::from_translation(home),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            body,
        ))
        .insert((
            SessionEntity,
            crate::experience::ExperienceReward::new(config.experience),
        ))
        .with_children(|root| {
            root.spawn((
                PendingScene,
                WorldAssetRoot(visual.scene.clone()),
                Transform::default(),
            ))
            .observe(scene_ready)
            .observe(bind_animation);
            root.spawn((
                HitGlow,
                PointLight {
                    color: Color::srgb(1.0, 0.38, 0.14),
                    intensity: GLOW_INTENSITY,
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
    actors: Query<Entity, Or<(With<Player>, With<Enemy>)>>,
    mut enemies: Query<(&GroundMovement, &mut Transform, Option<&mut MeleePursuit>)>,
) {
    let filter = SpatialQueryFilter::from_excluded_entities(actors.iter());
    for (movement, mut transform, pursuit) in &mut enemies {
        let origin = transform.translation + Vec3::Y * ENCOUNTER_GROUND_PROBE_HEIGHT;
        if let Some(y) = walkable_height(
            &spatial,
            &filter,
            origin,
            ENCOUNTER_GROUND_PROBE_RANGE,
            movement.ground_offset,
        ) {
            transform.translation.y = y;
            if let Some(mut pursuit) = pursuit {
                pursuit.home = transform.translation;
            }
        }
    }
}

fn plan_actions(
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    player: Query<(Entity, &Transform), With<Player>>,
    mut enemies: Query<
        (
            Entity,
            &mut MeleePursuit,
            &mut Transform,
            &mut GroundMovement,
        ),
        (Without<Player>, Without<Dead>),
    >,
    mut hits: MessageWriter<Hit>,
) {
    let Ok((player_entity, player_transform)) = player.single() else {
        return;
    };
    let delta = time.delta_secs();
    let target = player_transform.translation;
    for (entity, mut pursuit, mut transform, mut motion) in &mut enemies {
        motion.destination = None;
        let position = transform.translation;
        let to_target = target - position;
        let sight = SpatialQueryFilter::from_excluded_entities([entity, player_entity]);
        let bite = pursuit.bite;
        if matches!(
            pursuit.phase,
            MeleePhase::Windup(_) | MeleePhase::Recovery(_)
        ) {
            if pursuit.phase.tick_attack(delta, bite.recovery)
                && bite_contact(&spatial, &sight, &transform, target, bite.range)
            {
                hits.write(Hit {
                    source: entity,
                    target: player_entity,
                    damage: bite.damage,
                    contact: None,
                });
            }
            continue;
        }

        let within_leash = position.xz().distance(pursuit.home.xz()) < LEASH
            && target.xz().distance(pursuit.home.xz()) < LEASH;
        let sees_player = within_leash
            && to_target.length_squared() < SIGHT_RANGE * SIGHT_RANGE
            && clear_path(
                &spatial,
                &sight,
                position + Vec3::Y * SIGHT_ORIGIN_HEIGHT,
                target,
            );
        let near_home = position.xz().distance(pursuit.home.xz()) < HOME_ARRIVAL_RADIUS;

        let destination = if sees_player && (pursuit.phase != MeleePhase::Returning || near_home) {
            pursuit.phase = MeleePhase::Pursuing;
            if to_target.length_squared() <= bite.range * bite.range {
                transform.rotation = ground_heading(to_target);
                pursuit.phase = MeleePhase::Windup(bite.windup);
                continue;
            }
            target
        } else if !near_home {
            pursuit.phase = MeleePhase::Returning;
            pursuit.home
        } else {
            pursuit.phase = MeleePhase::Waiting;
            continue;
        };

        motion.destination = Some(destination);
    }
}

fn move_ground_enemies(
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
    actors: Query<Entity, Or<(With<Player>, With<Enemy>)>>,
    mut enemies: Query<(Entity, &mut Transform, &Collider, &mut GroundMovement), Without<Dead>>,
) {
    let delta = time.delta_secs();
    let support = SpatialQueryFilter::from_excluded_entities(actors.iter());
    for (entity, mut transform, collider, mut motion) in &mut enemies {
        motion.moving = false;
        let Some(destination) = motion.destination else {
            continue;
        };
        let position = transform.translation;
        let toward = (destination - position).with_y(0.0);
        if toward.length_squared() < DESTINATION_EPSILON_SQUARED {
            continue;
        }
        transform.rotation = transform
            .rotation
            .slerp(ground_heading(toward), (TURN_SPEED * delta).min(1.0));
        let velocity = toward.clamp_length_max(motion.speed * delta) / delta;
        let proposed = position + velocity * delta;
        if walkable_height(
            &spatial,
            &support,
            proposed + Vec3::Y * MAX_GROUND_STEP,
            GROUND_PROBE_RANGE,
            motion.ground_offset,
        )
        .is_none_or(|y| (y - position.y).abs() > MAX_GROUND_STEP)
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
        if let Some(y) = walkable_height(
            &spatial,
            &support,
            output.position + Vec3::Y * MAX_GROUND_STEP,
            GROUND_PROBE_RANGE,
            motion.ground_offset,
        ) {
            transform.translation = output.position.with_y(y);
            motion.moving = output.position.xz().distance_squared(position.xz())
                > ANIMATION_MOVEMENT_EPSILON_SQUARED;
        }
    }
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
    transform: &Transform,
    target: Vec3,
    range: f32,
) -> bool {
    let delta = target - transform.translation;
    let forward = transform.rotation * Vec3::Z;
    delta.length_squared() <= range * range
        && delta.xz().normalize_or_zero().dot(forward.xz()) >= 0.819 // 35 degrees.
        && clear_path(
            spatial,
            filter,
            transform.translation + forward * 0.34 + Vec3::Y * 0.15,
            target,
        )
}

fn bind_animation(
    event: On<WorldInstanceReady>,
    libraries: Query<&EnemyAnimations>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut animations: Query<&mut AnimationPlayer>,
    mut commands: Commands,
) {
    let Ok(parent) = parents.get(event.entity) else {
        return;
    };
    let Ok(library) = libraries.get(parent.parent()) else {
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
                EnemyPlayback {
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
    dialogue: Res<Dialogue>,
    enemies: Query<(&EnemyAnimations, &MeleePursuit, &GroundMovement, Has<Dead>)>,
    mut playing: Query<(
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        &mut EnemyPlayback,
    )>,
) {
    for (mut animation, mut transitions, mut playback) in &mut playing {
        if dialogue.text.is_some() {
            animation.pause_all();
            continue;
        }
        animation.resume_all();
        let Ok((library, pursuit, motion, dead)) = enemies.get(playback.owner) else {
            continue;
        };
        if dead {
            continue;
        }
        let clip = match pursuit.phase {
            MeleePhase::Windup(_) | MeleePhase::Recovery(_) => 2,
            _ if motion.moving => 1,
            _ => 0,
        };
        if clip != playback.clip {
            let fade = if clip < 2 {
                LOCOMOTION_BLEND
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
    enemies: Query<(&HitReaction, &Children, Has<Dead>), Or<(Changed<HitReaction>, Added<Dead>)>>,
    mut lights: Query<&mut PointLight, With<HitGlow>>,
) {
    for (reaction, children, dead) in &enemies {
        for child in children.iter() {
            if let Ok(mut light) = lights.get_mut(child) {
                light.intensity = if dead {
                    0.0
                } else {
                    GLOW_INTENSITY + HIT_GLOW_INTENSITY * reaction.strength()
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
        player::MovementInput,
        quest::{QuestAction, QuestJournal, test_journal},
        roach::{self, Roach},
        save::recover_defeated,
        world::WorldLayout,
        worm::{CONFIG, GROUND_OFFSET, Worm},
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
        .init_state::<GameState>()
        .init_resource::<WorldLayout>()
        .init_resource::<Dialogue>()
        .init_resource::<Equipment>()
        .init_resource::<MovementInput>()
        .add_plugins((CombatPlugin, EnemyPlugin))
        .add_systems(
            Update,
            recover_defeated
                .after(CombatSystems::Resolve)
                .run_if(in_state(GameState::Playing)),
        );
        app.finish();
        app.cleanup();
        app
    }

    #[test]
    fn windup_produces_exactly_one_impact() {
        let mut attack = MeleePhase::Windup(CONFIG.bite.windup);
        assert!(!attack.tick_attack(0.4, CONFIG.bite.recovery));
        assert!(attack.tick_attack(0.2, CONFIG.bite.recovery));
        assert!(!attack.tick_attack(0.3, CONFIG.bite.recovery));
        assert!(!attack.tick_attack(0.6, CONFIG.bite.recovery));
        assert_eq!(attack, MeleePhase::Pursuing);
        assert!(!attack.tick_attack(0.6, CONFIG.bite.recovery));
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
                    .map(|target| {
                        bite_contact(&spatial, &filter, &facing, target, CONFIG.bite.range)
                    })
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
    fn simultaneous_roach_and_worm_bites_apply_only_one_hit() {
        let mut app = test_app();
        let world = app.world_mut();
        let player = world
            .spawn((Player::default(), Transform::from_xyz(0.0, 0.9, 1.2)))
            .id();
        let bite = |config: GroundMelee, position| {
            (
                MeleePursuit {
                    phase: MeleePhase::Windup(0.001),
                    ..MeleePursuit::new(position, config.bite)
                },
                config.movement,
                Transform::from_translation(position),
            )
        };
        let worm = world
            .spawn((Worm, bite(CONFIG, Vec3::new(-0.1, GROUND_OFFSET, 0.0))))
            .id();
        let roach = world
            .spawn((Roach, bite(roach::CONFIG, Vec3::new(0.1, 0.32, 0.0))))
            .id();
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        let mut events = world
            .resource::<Messages<ConfirmedHit>>()
            .get_cursor_current();
        let mut attacks = world.resource::<Messages<Hit>>().get_cursor_current();
        let mut bites = Vec::new();
        let mut pulses = 0;
        for _ in 0..5 {
            app.update();
            bites.extend(
                attacks
                    .read(app.world().resource::<Messages<Hit>>())
                    .filter(|hit| hit.target == player)
                    .map(|hit| (hit.source, hit.damage)),
            );
            pulses += events
                .read(app.world().resource::<Messages<ConfirmedHit>>())
                .filter(|&&feedback| feedback.target == player)
                .count();
        }
        let health = app
            .world()
            .get::<Health>(player)
            .expect("the player must survive the bites");
        let accepted_damage = bites.first().expect("a bite must reach combat").1;
        assert_eq!(health.current(), health.maximum() - accepted_damage);
        bites.sort_unstable_by_key(|&(_, damage)| damage);
        assert_eq!(bites, [(roach, 12), (worm, 20)]);
        assert_eq!(pulses, 1, "protection must suppress the second pulse");
    }

    #[test]
    fn pause_and_dialogue_freeze_windup_and_a_lethal_bite_requests_reload() {
        let mut app = test_app();
        let player = app
            .world_mut()
            .spawn((Player::default(), Transform::from_xyz(0.0, 0.9, 1.3)))
            .id();
        let worm = app
            .world_mut()
            .spawn((
                MeleePursuit {
                    phase: MeleePhase::Windup(0.1),
                    ..MeleePursuit::new(Vec3::ZERO, CONFIG.bite)
                },
                CONFIG.movement,
                Transform::from_xyz(0.0, GROUND_OFFSET, 0.0),
            ))
            .id();
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::FieldArmor, 1));
        assert!(bag.add(ItemKind::BeaconCore, 1));
        app.world_mut()
            .resource_mut::<Equipment>()
            .equip(&mut bag, ItemKind::FieldArmor, 10)
            .expect("the armor fixture must be equipable");
        let mut journal = test_journal();
        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::RecoveredCore);
        app.insert_resource(bag).insert_resource(journal);
        app.world_mut().entity_mut(player).insert((
            Health::new(100),
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
            damage: 100,
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
        for state in [GameState::Paused, GameState::Playing] {
            app.world_mut()
                .resource_mut::<NextState<GameState>>()
                .set(state);
            for _ in 0..10 {
                app.update();
            }
            assert_eq!(
                app.world()
                    .get::<MeleePursuit>(worm)
                    .expect("worm must survive while waiting")
                    .phase,
                MeleePhase::Windup(0.1)
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
            "a lethal bite must produce one pulse before reload"
        );

        let world = app.world();
        assert_eq!(
            world
                .get::<Health>(player)
                .expect("the fixture does not install the world loader")
                .current(),
            0
        );
        assert_eq!(
            *world.resource::<State<GameState>>().get(),
            GameState::Loading
        );
        assert!(
            world
                .resource::<QuestJournal>()
                .expects(QuestAction::ReturnedCore)
        );
        assert_eq!(world.resource::<Inventory>().count(ItemKind::BeaconCore), 1);
        assert_eq!(world.resource::<Equipment>().total_defense(), 2);
    }
}
