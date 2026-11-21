use crate::{
    GameState,
    combat::{CombatSystems, ConfirmedHit, Dead, Health, HitReaction, RemoveColliderOnDeath},
    enemy::{heading, surface},
    player::{AnimationLibrary, Player, RUN_CLIP, WALK_CLIP},
    quest::{Dialogue, QuestAction, QuestJournal},
};
use avian3d::prelude::*;
use bevy::prelude::*;
use std::time::Duration;

const FLEE_SPEED: f32 = 4.5;
const RETURN_SPEED: f32 = 3.0;
const SAFE_DISTANCE: f32 = 8.0;
const PURSUED_DISTANCE: f32 = 4.0;
const SAFE_TIME: f32 = 3.0;
pub(crate) const GROUND_OFFSET: f32 = 0.9;

pub(crate) struct NemiPlugin;

#[derive(Component, Default)]
#[require(Health = Health::new(30), HitReaction = HitReaction::new(0.16), RemoveColliderOnDeath = RemoveColliderOnDeath)]
pub(crate) struct Nemi {
    home: Option<Vec3>,
    behavior: Behavior,
    moving: bool,
}

#[derive(Default, Clone, Copy)]
enum Behavior {
    #[default]
    Peaceful,
    Fleeing {
        safe_for: f32,
    },
    Returning,
    Refuses,
}

impl Nemi {
    pub(crate) fn can_talk(&self) -> bool {
        matches!(self.behavior, Behavior::Peaceful | Behavior::Refuses)
    }

    pub(crate) fn refuses(&self) -> bool {
        matches!(self.behavior, Behavior::Refuses)
    }
}

#[derive(Component)]
pub(crate) struct NemiPlayback {
    pub(crate) owner: Entity,
    pub(crate) clip: usize,
}

impl Plugin for NemiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (react_to_player_hit, animate)
                .chain()
                .after(CombatSystems::Resolve)
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(
            FixedUpdate,
            move_nemi
                .after(crate::player::movement)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

fn react_to_player_hit(
    mut hits: MessageReader<ConfirmedHit>,
    players: Query<(), With<Player>>,
    mut keepers: Query<(&mut Nemi, &Transform, Has<Dead>)>,
    mut journal: ResMut<QuestJournal>,
    mut dialogue: ResMut<Dialogue>,
) {
    for hit in hits.read() {
        if !players.contains(hit.source) {
            continue;
        }
        let Ok((mut nemi, transform, dead)) = keepers.get_mut(hit.target) else {
            continue;
        };
        journal.record(QuestAction::AttackedNemi);
        dialogue.text = None;
        if !dead {
            nemi.home.get_or_insert(transform.translation);
            nemi.behavior = Behavior::Fleeing { safe_for: 0.0 };
        }
    }
}

fn move_nemi(
    time: Res<Time<Fixed>>,
    players: Query<(Entity, &Player, &Transform), Without<Nemi>>,
    mut keepers: Query<(Entity, &mut Nemi, &mut Transform, &Collider), Without<Dead>>,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
) {
    let Ok((player, controller, player_transform)) = players.single() else {
        return;
    };
    let delta = time.delta_secs();
    for (entity, mut nemi, mut transform, collider) in &mut keepers {
        nemi.moving = false;
        let Some(home) = nemi.home else { continue };
        let position = transform.translation;
        let away = (position - player_transform.translation).with_y(0.0);
        let distance = if controller.area == crate::dungeon::Area::Surface {
            away.length()
        } else {
            f32::INFINITY
        };
        if matches!(nemi.behavior, Behavior::Returning) && distance < PURSUED_DISTANCE {
            nemi.behavior = Behavior::Fleeing { safe_for: 0.0 };
        }
        let (direction, speed) = match nemi.behavior {
            Behavior::Fleeing { safe_for } if distance >= SAFE_DISTANCE => {
                let safe_for = safe_for + delta;
                nemi.behavior = if safe_for >= SAFE_TIME {
                    Behavior::Returning
                } else {
                    Behavior::Fleeing { safe_for }
                };
                continue;
            }
            Behavior::Fleeing { .. } => {
                nemi.behavior = Behavior::Fleeing { safe_for: 0.0 };
                (away.try_normalize().unwrap_or(Vec3::Z), FLEE_SPEED)
            }
            Behavior::Returning => {
                let toward_home = (home - position).with_y(0.0);
                if toward_home.length_squared() < 0.04 {
                    nemi.behavior = Behavior::Refuses;
                    continue;
                }
                (
                    toward_home.normalize(),
                    RETURN_SPEED.min(toward_home.length() / delta),
                )
            }
            Behavior::Peaceful | Behavior::Refuses => continue,
        };
        let collision = SpatialQueryFilter::from_excluded_entities([entity]);
        let support = SpatialQueryFilter::from_excluded_entities([entity, player]);
        // Sample alternate headings if a wall or the shelf's edge blocks the direct route.
        for angle in [0.0_f32, 45.0, -45.0, 75.0, -75.0, 100.0, -100.0] {
            let direction = Quat::from_rotation_y(angle.to_radians()) * direction;
            let velocity = direction * speed;
            let output = mover.move_and_slide(
                collider,
                position,
                transform.rotation,
                velocity,
                time.delta(),
                &MoveAndSlideConfig::default(),
                &collision,
                |_| MoveAndSlideHitResponse::Accept,
            );
            let movement = (output.position - position).with_y(0.0);
            if movement.dot(direction) < speed * delta * 0.35 {
                continue;
            }
            let Some(height) = surface(
                &spatial,
                &support,
                output.position + Vec3::Y * 0.25,
                GROUND_OFFSET + 0.6,
                GROUND_OFFSET,
            ) else {
                continue;
            };
            if (height - position.y).abs() > 0.25 {
                continue;
            }
            transform.translation = output.position.with_y(height);
            transform.rotation = transform
                .rotation
                .slerp(heading(movement), (12.0 * delta).min(1.0));
            nemi.moving = true;
            break;
        }
    }
}

fn animate(
    library: Res<AnimationLibrary>,
    keepers: Query<&Nemi, Without<Dead>>,
    mut animations: Query<(
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        &mut NemiPlayback,
    )>,
) {
    for (mut player, mut transitions, mut playback) in &mut animations {
        let Ok(nemi) = keepers.get(playback.owner) else {
            continue;
        };
        let clip = match (nemi.moving, nemi.behavior) {
            (true, Behavior::Fleeing { .. }) => RUN_CLIP,
            (true, Behavior::Returning) => WALK_CLIP,
            _ => 0,
        };
        if clip != playback.clip {
            transitions
                .play(&mut player, library.nodes[clip], Duration::from_millis(120))
                .repeat();
            playback.clip = clip;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::{CombatPlugin, Hit},
        quest::{LAST_LIGHT, QuestStatus, test_journal},
    };
    use bevy::state::app::StatesPlugin;

    #[test]
    fn only_confirmed_player_damage_fails_the_keeper_quest() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CombatPlugin))
            .insert_state(GameState::Playing)
            .insert_resource(test_journal())
            .init_resource::<Dialogue>()
            .add_systems(Update, react_to_player_hit.after(CombatSystems::Resolve));
        let world = app.world_mut();
        let player = world.spawn(Player::default()).id();
        let other = world.spawn_empty().id();
        let keeper = world
            .spawn((Nemi::default(), Transform::default(), Collider::sphere(0.4)))
            .id();
        let strike = Hit {
            source: player,
            target: keeper,
            damage: 0,
            contact: None,
        };
        world.write_message(strike);
        world.write_message(Hit {
            source: other,
            damage: 1,
            ..strike
        });
        app.update();
        assert_eq!(
            app.world().resource::<QuestJournal>().status(LAST_LIGHT),
            Some(QuestStatus::Active)
        );
        assert_eq!(
            app.world()
                .get::<Health>(keeper)
                .expect("Nemi health")
                .current(),
            29
        );

        app.world_mut().write_message(Hit {
            damage: 10,
            ..strike
        });
        app.update();
        assert_eq!(
            app.world().resource::<QuestJournal>().status(LAST_LIGHT),
            Some(QuestStatus::Failed)
        );
        assert!(!app.world().get::<Nemi>(keeper).expect("Nemi").can_talk());
        app.world_mut().write_message(Hit {
            damage: 30,
            ..strike
        });
        app.update();
        assert!(app.world().get::<Dead>(keeper).is_some());
        assert!(app.world().get::<Collider>(keeper).is_none());
    }
}
