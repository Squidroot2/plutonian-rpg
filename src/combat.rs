//! Shared damage and death processing. Attack detection and presentation live in their own systems.
use crate::{GameState, dialogue::Dialogue};
use avian3d::prelude::Collider;
use bevy::prelude::*;
use std::time::Duration;

#[derive(Component)]
pub(crate) struct Health {
    current: u16,
    maximum: u16,
}

impl Health {
    pub(crate) fn new(maximum: u16) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }

    pub(crate) fn current(&self) -> u16 {
        self.current
    }

    pub(crate) fn maximum(&self) -> u16 {
        self.maximum
    }

    pub(crate) fn restore(&mut self) {
        self.current = self.maximum;
    }

    pub(crate) fn set_maximum(&mut self, maximum: u16) {
        self.maximum = maximum;
        self.current = self.current.min(maximum);
    }

    pub(crate) fn set_current(&mut self, health: u16) {
        self.current = health.min(self.maximum);
    }
}

#[derive(Component)]
pub(crate) struct HealthRecovery {
    delay: Duration,
    wait: Duration,
    points_per_second: f64,
    accrued: f64,
}

impl HealthRecovery {
    pub(crate) fn player(points_per_second: f64) -> Self {
        Self::new(Duration::from_secs(5), points_per_second)
    }

    pub(crate) fn new(delay: Duration, points_per_second: f64) -> Self {
        Self {
            delay,
            wait: Duration::ZERO,
            points_per_second,
            accrued: 0.0,
        }
    }

    pub(crate) fn rate(&self) -> f64 {
        self.points_per_second
    }

    pub(crate) fn set_rate(&mut self, points_per_second: f64) {
        self.points_per_second = points_per_second;
    }
}

#[derive(Component, Default, PartialEq, Eq)]
pub(crate) struct Defense(pub u16);

#[derive(Component)]
pub(crate) struct HitProtection {
    duration: f32,
    remaining: f32,
}

impl HitProtection {
    pub(crate) fn new(duration: f32) -> Self {
        Self {
            duration,
            remaining: 0.0,
        }
    }
}

/// A healthless target can accept an impact without taking damage.
#[derive(Component)]
pub(crate) struct TrainingTarget;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Contact {
    pub(crate) position: Vec3,
    pub(crate) normal: Vec3,
}

#[derive(Clone, Copy, Debug, Message, PartialEq)]
pub(crate) struct Hit {
    pub(crate) source: Entity,
    pub(crate) target: Entity,
    pub(crate) damage: u16,
    pub(crate) contact: Option<Contact>,
}

#[derive(Clone, Copy, Debug, Message, PartialEq)]
pub(crate) struct ConfirmedHit {
    pub(crate) source: Entity,
    pub(crate) target: Entity,
    pub(crate) contact: Option<Contact>,
}

#[derive(Component)]
pub(crate) struct Dead;

/// A new death in combat resolution. Loading a corpse only restores its marker.
#[derive(Message)]
pub(crate) struct ActorDefeated(pub(crate) Entity);

#[derive(Component)]
pub(crate) struct RemoveColliderOnDeath;

/// Optional corpse lifetime, advanced only while the entity is dead.
#[derive(Component)]
pub(crate) struct DeathLifetime(pub(crate) f32);

/// A corpse whose visuals are fading after its loot was taken.
#[derive(Component)]
pub(crate) struct LootedCorpseFade(pub(crate) f32);

#[derive(Component)]
pub(crate) struct HitReaction {
    duration: f32,
    pub(crate) remaining: f32,
}

impl HitReaction {
    pub(crate) fn new(duration: f32) -> Self {
        Self {
            duration,
            remaining: 0.0,
        }
    }

    pub(crate) fn strength(&self) -> f32 {
        (self.remaining / self.duration).clamp(0.0, 1.0)
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CombatSystems {
    Detect,
    Resolve,
}

pub(crate) struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Hit>()
            .add_message::<ConfirmedHit>()
            .add_message::<ActorDefeated>()
            .configure_sets(
                Update,
                (CombatSystems::Detect, CombatSystems::Resolve).chain(),
            )
            .add_systems(
                FixedUpdate,
                (tick_protection, recover_health, expire_corpses)
                    .run_if(in_state(GameState::Playing))
                    .run_if(|dialogue: Res<Dialogue>| !dialogue.is_open()),
            )
            .add_systems(
                Update,
                (
                    resolve_hits,
                    delay_recovery_on_hit,
                    mark_deaths,
                    release_dead_colliders,
                    update_hit_reactions,
                    fade_looted_corpses,
                )
                    .chain()
                    .in_set(CombatSystems::Resolve)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn resolve_hits(
    mut hits: MessageReader<Hit>,
    mut confirmed: MessageWriter<ConfirmedHit>,
    mut health: Query<(&mut Health, Option<&Defense>, Option<&mut HitProtection>)>,
    training: Query<(), (With<TrainingTarget>, Without<Health>)>,
) {
    for &hit in hits.read() {
        // Fixed-step attacks arrive before frame-step thrusts. A lethal hit cancels
        // any later attack from that actor, even before the death marker is applied.
        if health
            .get(hit.source)
            .is_ok_and(|(source, _, _)| source.current == 0)
        {
            continue;
        }
        if let Ok((mut target, defense, protection)) = health.get_mut(hit.target) {
            if hit.damage == 0
                || target.current == 0
                || protection
                    .as_ref()
                    .is_some_and(|window| window.remaining > 0.0)
            {
                continue;
            }
            let defense = defense.map_or(0, |armor| armor.0);
            target.current = target
                .current
                .saturating_sub(hit.damage.saturating_sub(defense).max(1));
            if let Some(mut protection) = protection {
                protection.remaining = protection.duration;
            }
        } else if !training.contains(hit.target) {
            continue;
        }

        confirmed.write(ConfirmedHit {
            source: hit.source,
            target: hit.target,
            contact: hit.contact,
        });
    }
}

fn delay_recovery_on_hit(
    mut hits: MessageReader<ConfirmedHit>,
    mut recovery: Query<&mut HealthRecovery>,
) {
    for hit in hits.read() {
        if let Ok(mut recovery) = recovery.get_mut(hit.target) {
            recovery.wait = recovery.delay;
            recovery.accrued = 0.0;
        }
    }
}

fn recover_health(
    time: Res<Time<Fixed>>,
    mut actors: Query<(&mut Health, &mut HealthRecovery), Without<Dead>>,
) {
    for (mut health, mut recovery) in &mut actors {
        if health.current == 0 || health.current >= health.maximum {
            continue;
        }
        let delta = time.delta();
        let active = delta.saturating_sub(recovery.wait);
        recovery.wait = recovery.wait.saturating_sub(delta);
        recovery.accrued += active.as_secs_f64() * recovery.points_per_second;
        let points = recovery.accrued.floor();
        recovery.accrued -= points;
        if points > 0.0 {
            health.current += (points as u16).min(health.maximum - health.current);
        }
    }
}

fn tick_protection(time: Res<Time<Fixed>>, mut windows: Query<&mut HitProtection>) {
    for mut window in &mut windows {
        if window.remaining > 0.0 {
            window.remaining = (window.remaining - time.delta_secs()).max(0.0);
        }
    }
}

fn mark_deaths(
    mut commands: Commands,
    health: Query<(Entity, &Health), (Changed<Health>, Without<Dead>)>,
    mut defeats: MessageWriter<ActorDefeated>,
) {
    for (entity, health) in &health {
        if health.current == 0 {
            commands.entity(entity).insert(Dead);
            defeats.write(ActorDefeated(entity));
        }
    }
}

fn release_dead_colliders(
    mut commands: Commands,
    bodies: Query<Entity, (Added<Dead>, With<RemoveColliderOnDeath>, With<Collider>)>,
) {
    for entity in &bodies {
        commands.entity(entity).remove::<Collider>();
    }
}

fn expire_corpses(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut corpses: Query<(Entity, &mut DeathLifetime), With<Dead>>,
) {
    for (entity, mut lifetime) in &mut corpses {
        lifetime.0 -= time.delta_secs();
        if lifetime.0 <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn update_hit_reactions(
    time: Res<Time>,
    mut hits: MessageReader<ConfirmedHit>,
    mut reactions: Query<&mut HitReaction>,
) {
    for mut reaction in &mut reactions {
        if reaction.remaining > 0.0 {
            reaction.remaining = (reaction.remaining - time.delta_secs()).max(0.0);
        }
    }
    for hit in hits.read() {
        if let Ok(mut reaction) = reactions.get_mut(hit.target) {
            reaction.remaining = reaction.duration;
        }
    }
}

fn fade_looted_corpses(
    time: Res<Time>,
    mut commands: Commands,
    mut corpses: Query<(Entity, &mut LootedCorpseFade, Option<&Children>)>,
    mut visuals: Query<&mut Transform>,
) {
    const FADE_SECONDS: f32 = 0.65;
    for (entity, mut fade, children) in &mut corpses {
        fade.0 = (fade.0 + time.delta_secs() / FADE_SECONDS).clamp(0.0, 1.0);
        let scale = 1.0 - fade.0;
        for &child in children.into_iter().flatten() {
            if let Ok(mut transform) = visuals.get_mut(child) {
                transform.scale = Vec3::splat(scale);
            }
        }
        if fade.0 >= 1.0 {
            // Keep the defeated encounter available to the bed snapshot.
            commands
                .entity(entity)
                .insert(Visibility::Hidden)
                .remove::<LootedCorpseFade>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{state::app::StatesPlugin, time::TimeUpdateStrategy};
    use std::time::Duration;

    #[test]
    fn generic_health_targets_die_once_and_expire_with_their_own_policy() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CombatPlugin))
            .init_state::<GameState>()
            .init_resource::<Dialogue>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                20,
            )));
        let world = app.world_mut();
        let attacker = world.spawn(Health::new(100)).id();
        let target = world
            .spawn((
                Health::new(10),
                Defense(2),
                RemoveColliderOnDeath,
                Collider::sphere(0.5),
                DeathLifetime(0.1),
            ))
            .id();
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        let mut confirmed = world
            .resource::<Messages<ConfirmedHit>>()
            .get_cursor_current();
        let strike = Hit {
            source: attacker,
            target,
            damage: 12,
            contact: None,
        };
        world.write_message(strike);
        world.write_message(strike);
        world.write_message(Hit {
            source: target,
            target: attacker,
            ..strike
        });
        app.update();

        let world = app.world();
        assert_eq!(
            world
                .get::<Health>(target)
                .expect("corpse still has health data")
                .current(),
            0
        );
        assert!(world.get::<Dead>(target).is_some());
        assert!(world.get::<Collider>(target).is_none());
        assert_eq!(
            world
                .get::<Health>(attacker)
                .expect("attacker survives")
                .current(),
            100
        );
        assert_eq!(
            confirmed
                .read(world.resource::<Messages<ConfirmedHit>>())
                .copied()
                .collect::<Vec<_>>(),
            [ConfirmedHit {
                source: attacker,
                target,
                contact: None
            }],
        );

        for _ in 0..8 {
            app.update();
        }
        assert!(app.world().get_entity(target).is_err());
    }

    #[test]
    fn defense_has_a_minimum_damage_and_protection_rejects_the_next_hit() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CombatPlugin))
            .init_state::<GameState>()
            .init_resource::<Dialogue>();
        let world = app.world_mut();
        let source = world.spawn_empty().id();
        let target = world
            .spawn((Health::new(5), Defense(u16::MAX), HitProtection::new(0.75)))
            .id();
        let strike = Hit {
            source,
            target,
            damage: 20,
            contact: None,
        };
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        world.write_message(strike);
        world.write_message(strike);
        app.update();
        let health = app
            .world()
            .get::<Health>(target)
            .expect("target must have health");
        assert_eq!(health.current(), 4);
    }
}
