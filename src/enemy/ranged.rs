use super::{EnemyPose, clear_path, no_dialogue};
use crate::{
    GameState,
    combat::{CombatSystems, Contact, Dead, Hit},
    dungeon::Area,
    interaction::InteractionSystems,
    player::Player,
    terrain::ground_heading,
    world::SessionEntity,
};
use avian3d::prelude::*;
use bevy::prelude::*;

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Spit {
    pub(crate) sight_m: f32,
    pub(crate) windup_seconds: f32,
    pub(crate) recovery_seconds: f32,
    pub(crate) speed_mps: f32,
    pub(crate) travel_m: f32,
    pub(crate) radius_m: f32,
    pub(crate) damage: u16,
    #[serde(deserialize_with = "deserialize_muzzle")]
    pub(crate) muzzle: Vec3,
}

fn deserialize_muzzle<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec3, D::Error> {
    <[f32; 3] as serde::Deserialize>::deserialize(deserializer).map(Vec3::from)
}

impl Spit {
    pub(crate) fn is_valid(self) -> bool {
        self.damage != 0
            && self.muzzle.is_finite()
            && [
                self.sight_m,
                self.windup_seconds,
                self.recovery_seconds,
                self.speed_mps,
                self.travel_m,
                self.radius_m,
            ]
            .into_iter()
            .all(crate::content::positive)
    }
}

#[derive(Component)]
pub(crate) struct RangedAttack {
    area: Area,
    spit: Spit,
    phase: Phase,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Windup { remaining: f32, aim: Dir3 },
    Recovery(f32),
}

impl RangedAttack {
    pub(crate) const fn new(area: Area, spit: Spit) -> Self {
        Self {
            area,
            spit,
            phase: Phase::Idle,
        }
    }
}

#[derive(Component)]
pub(crate) struct ChargeLight {
    pub(crate) peak_lumens: f32,
}

#[derive(Component)]
pub(crate) struct SpitProjectile {
    source: Entity,
    area: Area,
    aim: Dir3,
    remaining_m: f32,
    speed_mps: f32,
    damage: u16,
    shape: Collider,
}

#[derive(Resource)]
struct SpitVisuals {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

pub(super) struct RangedPlugin;

impl Plugin for RangedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, prepare_spit_visuals)
            .add_systems(
                Update,
                (advance_spit, plan_shots)
                    .chain()
                    .in_set(CombatSystems::Detect)
                    .after(InteractionSystems)
                    .run_if(in_state(GameState::Playing))
                    .run_if(no_dialogue),
            )
            .add_systems(
                Update,
                (clear_defeated_shots, light_charge)
                    .after(CombatSystems::Resolve)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn prepare_spit_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(SpitVisuals {
        mesh: meshes.add(
            Sphere::new(1.0)
                .mesh()
                .ico(2)
                .expect("two subdivisions fit an icosphere"),
        ),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.67, 0.97, 0.76),
            emissive: LinearRgba::rgb(0.48, 2.8, 1.36),
            perceptual_roughness: 0.32,
            ..default()
        }),
    });
}

fn plan_shots(
    time: Res<Time>,
    spatial: SpatialQuery,
    player: Query<(Entity, &Player, &Transform), Without<Dead>>,
    mut enemies: Query<
        (Entity, &mut RangedAttack, &mut Transform, &mut EnemyPose),
        (Without<Player>, Without<Dead>),
    >,
    visuals: Res<SpitVisuals>,
    mut commands: Commands,
) {
    let delta = time.delta_secs();
    let player = player.single().ok();
    for (source, mut attack, mut transform, mut pose) in &mut enemies {
        let Some((target, _, player_pose)) =
            player.filter(|(_, player, _)| player.area == attack.area)
        else {
            if attack.phase != Phase::Idle {
                attack.phase = Phase::Idle;
            }
            pose.set_if_neq(EnemyPose::Idle);
            continue;
        };
        let spit = attack.spit;
        let phase = match attack.phase {
            Phase::Idle => aim_shot(&spatial, source, target, &mut transform, player_pose, spit),
            Phase::Windup { remaining, aim } if remaining <= delta => {
                spawn_spit(
                    &mut commands,
                    &visuals,
                    source,
                    attack.area,
                    &transform,
                    aim,
                    spit,
                );
                Phase::Recovery((spit.recovery_seconds + remaining - delta).max(0.0))
            }
            Phase::Windup { remaining, aim } => Phase::Windup {
                remaining: remaining - delta,
                aim,
            },
            Phase::Recovery(remaining) if remaining <= delta => Phase::Idle,
            Phase::Recovery(remaining) => Phase::Recovery(remaining - delta),
        };
        if attack.phase != phase {
            attack.phase = phase;
        }
        pose.set_if_neq(match phase {
            Phase::Idle => EnemyPose::Idle,
            _ => EnemyPose::Attacking,
        });
    }
}

fn aim_shot(
    spatial: &SpatialQuery,
    source: Entity,
    target: Entity,
    transform: &mut Transform,
    player: &Transform,
    spit: Spit,
) -> Phase {
    let toward = player.translation - transform.translation;
    if toward.length_squared() > spit.sight_m * spit.sight_m {
        return Phase::Idle;
    }
    let heading = ground_heading(toward);
    let muzzle = transform.translation + heading * spit.muzzle;
    let Ok(aim) = Dir3::new(player.translation - muzzle) else {
        return Phase::Idle;
    };
    let filter = SpatialQueryFilter::from_excluded_entities([source, target]);
    if !clear_path(spatial, &filter, muzzle, player.translation) {
        return Phase::Idle;
    }
    if transform.rotation != heading {
        transform.rotation = heading;
    }
    Phase::Windup {
        remaining: spit.windup_seconds,
        aim,
    }
}

fn spawn_spit(
    commands: &mut Commands,
    visuals: &SpitVisuals,
    source: Entity,
    area: Area,
    transform: &Transform,
    aim: Dir3,
    spit: Spit,
) {
    commands.spawn((
        SessionEntity,
        Name::new("Spit projectile"),
        SpitProjectile {
            source,
            area,
            aim,
            remaining_m: spit.travel_m,
            speed_mps: spit.speed_mps,
            damage: spit.damage,
            shape: Collider::sphere(spit.radius_m),
        },
        Mesh3d(visuals.mesh.clone()),
        MeshMaterial3d(visuals.material.clone()),
        Transform::from_translation(transform.translation + transform.rotation * spit.muzzle)
            .with_rotation(ground_heading(*aim))
            .with_scale(Vec3::new(
                spit.radius_m,
                spit.radius_m,
                spit.radius_m * 1.55,
            )),
    ));
}

fn advance_spit(
    time: Res<Time>,
    spatial: SpatialQuery,
    player: Query<(Entity, &Player), Without<Dead>>,
    sources: Query<(), (With<RangedAttack>, Without<Dead>)>,
    mut projectiles: Query<(Entity, &mut SpitProjectile, &mut Transform)>,
    mut hits: MessageWriter<Hit>,
    mut commands: Commands,
) {
    let target = player.single().ok();
    for (entity, mut shot, mut transform) in &mut projectiles {
        let Some((target, _)) = target.filter(|(_, player)| player.area == shot.area) else {
            commands.entity(entity).despawn();
            continue;
        };
        if !sources.contains(shot.source) {
            commands.entity(entity).despawn();
            continue;
        }
        let distance = shot.remaining_m.min(shot.speed_mps * time.delta_secs());
        if distance == 0.0 {
            continue;
        }
        let filter = SpatialQueryFilter::from_excluded_entities([shot.source]);
        if let Some(hit) = spatial.cast_shape(
            &shot.shape,
            transform.translation,
            Quat::IDENTITY,
            shot.aim,
            &ShapeCastConfig {
                compute_contact_on_penetration: true,
                ..ShapeCastConfig::from_max_distance(distance)
            },
            &filter,
        ) {
            if hit.entity == target {
                hits.write(Hit {
                    source: shot.source,
                    target,
                    damage: shot.damage,
                    contact: Some(Contact {
                        position: hit.point1,
                        normal: hit.normal1,
                    }),
                });
            }
            commands.entity(entity).despawn();
        } else if distance >= shot.remaining_m {
            commands.entity(entity).despawn();
        } else {
            transform.translation += *shot.aim * distance;
            shot.remaining_m -= distance;
        }
    }
}

fn clear_defeated_shots(
    defeated: Query<(), (With<RangedAttack>, Added<Dead>)>,
    projectiles: Query<(Entity, &SpitProjectile)>,
    mut commands: Commands,
) {
    for (entity, shot) in &projectiles {
        if defeated.contains(shot.source) {
            commands.entity(entity).despawn();
        }
    }
}

fn light_charge(
    enemies: Query<(&RangedAttack, &Children, Has<Dead>)>,
    mut lights: Query<(&ChargeLight, &mut PointLight)>,
) {
    for (attack, children, dead) in &enemies {
        let charge = match attack.phase {
            Phase::Windup { remaining, .. } if !dead => {
                1.0 - remaining / attack.spit.windup_seconds
            }
            _ => 0.0,
        };
        for child in children.iter() {
            if let Ok((light, mut lamp)) = lights.get_mut(child) {
                let intensity = light.peak_lumens * charge;
                if lamp.intensity != intensity {
                    lamp.intensity = intensity;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::Health,
        content::{EnemyArchetype, test_enemies},
        dialogue::Dialogue,
        enemy::tests::test_app,
    };

    fn spit() -> Spit {
        test_enemies()
            .get(EnemyArchetype::Hushcap.id())
            .expect("shipped Hushcap definition")
            .spit()
    }

    fn encounter() -> (App, Entity, Entity) {
        let mut app = test_app();
        let world = app.world_mut();
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        let mut player = Player::default();
        player.area = Area::OldRelay;
        let player = world
            .spawn((
                player,
                Transform::from_xyz(0.0, 0.91, 5.0),
                RigidBody::Static,
                Collider::capsule(0.32, 1.15),
            ))
            .id();
        let enemy = world
            .spawn((
                RangedAttack::new(Area::OldRelay, spit()),
                EnemyPose::Idle,
                Transform::IDENTITY,
                Health::new(42),
                RigidBody::Static,
                Collider::cuboid(0.8, 1.2, 0.8),
            ))
            .id();
        (app, player, enemy)
    }

    fn tick(app: &mut App, frames: usize) -> usize {
        let mut shots = 0;
        let mut projectiles = app
            .world_mut()
            .query_filtered::<Entity, With<SpitProjectile>>();
        let mut previous = projectiles.single(app.world()).ok();
        for _ in 0..frames {
            app.update();
            let current = projectiles.single(app.world()).ok();
            shots += usize::from(current.is_some() && current != previous);
            previous = current;
        }
        shots
    }

    fn health(app: &App, entity: Entity) -> u16 {
        app.world()
            .get::<Health>(entity)
            .expect("actor health")
            .current()
    }

    fn cover(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_xyz(0.0, 1.0, 2.6),
                RigidBody::Static,
                Collider::cuboid(1.1, 2.0, 0.2),
            ))
            .id()
    }

    #[test]
    fn an_unobstructed_shot_fires_once_and_damages_the_player_once() {
        let (mut app, player, enemy) = encounter();
        let initial_health = health(&app, player);
        assert_eq!(tick(&mut app, 38), 0);
        assert_eq!(health(&app, player), initial_health);
        assert_eq!(tick(&mut app, 58), 1);
        assert_eq!(health(&app, player), initial_health - spit().damage);
        assert_eq!(
            app.world()
                .get::<Transform>(enemy)
                .expect("rooted enemy")
                .translation,
            Vec3::ZERO
        );
    }

    #[test]
    fn a_sidestep_during_windup_escapes_the_locked_shot() {
        let (mut app, player, _) = encounter();
        let initial_health = health(&app, player);
        assert_eq!(tick(&mut app, 12), 0);
        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("player pose")
            .translation
            .x = 1.6;
        assert_eq!(tick(&mut app, 84), 1);
        assert_eq!(health(&app, player), initial_health);
    }

    #[test]
    fn cover_prevents_aiming_and_intercepts_a_committed_shot() {
        let (mut app, player, _) = encounter();
        let initial_health = health(&app, player);
        let barrier = cover(&mut app);
        assert_eq!(tick(&mut app, 70), 0);
        app.world_mut().despawn(barrier);
        assert_eq!(tick(&mut app, 17), 0);
        cover(&mut app);
        assert_eq!(tick(&mut app, 55), 1);
        assert_eq!(health(&app, player), initial_health);
        let mut shots = app.world_mut().query::<&SpitProjectile>();
        assert_eq!(shots.iter(app.world()).count(), 0);
    }

    #[test]
    fn dialogue_pauses_shots_and_death_clears_them() {
        let (mut app, player, enemy) = encounter();
        let initial_health = health(&app, player);
        assert_eq!(tick(&mut app, 48), 1);
        app.world_mut()
            .resource_mut::<Dialogue>()
            .show_notice_text("Survey log");
        assert_eq!(tick(&mut app, 60), 0);
        assert_eq!(health(&app, player), initial_health);
        let mut shots = app.world_mut().query::<&SpitProjectile>();
        assert_eq!(shots.iter(app.world()).count(), 1);
        app.world_mut().write_message(Hit {
            source: player,
            target: enemy,
            damage: 42,
            contact: None,
        });
        app.update();
        assert!(app.world().get::<Dead>(enemy).is_some());
        assert_eq!(shots.iter(app.world()).count(), 0);
    }

    #[test]
    fn paused_flight_resumes_and_leaving_the_area_discards_the_shot() {
        let (mut app, player, _) = encounter();
        let initial_health = health(&app, player);
        assert_eq!(tick(&mut app, 48), 1);
        let mut shots = app
            .world_mut()
            .query_filtered::<&Transform, With<SpitProjectile>>();
        let before_pause = *shots.single(app.world()).expect("the shot is in flight");
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
        assert_eq!(tick(&mut app, 60), 0);
        assert_eq!(
            *shots.single(app.world()).expect("the shot is paused"),
            before_pause
        );

        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        assert_eq!(tick(&mut app, 3), 0);
        assert_ne!(
            *shots.single(app.world()).expect("the shot has resumed"),
            before_pause
        );
        app.world_mut()
            .get_mut::<Player>(player)
            .expect("player area")
            .area = Area::Surface;
        assert_eq!(tick(&mut app, 60), 0);
        assert!(shots.iter(app.world()).next().is_none());
        assert_eq!(health(&app, player), initial_health);
    }
}
