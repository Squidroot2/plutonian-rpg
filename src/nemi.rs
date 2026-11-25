pub(crate) mod dialogue;

use crate::{
    GameState,
    attributes::CarryLimits,
    combat::{CombatSystems, ConfirmedHit, Dead, HitReaction, RemoveColliderOnDeath},
    content::Items,
    crossing::village::{Doorway, Homebound},
    dialogue::{Availability, Dialogue, DialogueRegistry, Speaker},
    dungeon::Area,
    equipment::Equipment,
    interaction::InteractionSystems,
    inventory::Inventory,
    player::{NpcGait, Pace, Player},
    quest::{QuestAction, QuestJournal},
    terrain::{MAX_GROUND_STEP, ground_heading, walkable_height},
};
use avian3d::prelude::*;
use bevy::prelude::*;

const INTERACTION_HEIGHT: f32 = 0.35;
const FLEE_SPEED: f32 = 4.5;
const RETURN_SPEED: f32 = 3.0;
const SAFE_DISTANCE: f32 = 8.0;
const PURSUED_DISTANCE: f32 = 4.0;
const SAFE_TIME: f32 = 3.0;
const HOME_ARRIVAL_RADIUS_SQUARED: f32 = 0.04;
const AVOIDANCE_ANGLES_DEGREES: [f32; 7] = [0.0, 45.0, -45.0, 75.0, -75.0, 100.0, -100.0];
const MIN_TRAVEL_FRACTION: f32 = 0.35;
const GROUND_PROBE_MARGIN_M: f32 = 0.6;
const TURN_SPEED: f32 = 12.0;
const HIT_REACTION_SECONDS: f32 = 0.16;
const EXPERIENCE_REWARD: u16 = 35;
pub(crate) const GROUND_OFFSET: f32 = 0.9;

pub(crate) struct NemiPlugin;

pub(crate) fn speaker() -> Speaker {
    Speaker::new(dialogue::PROFILE, Area::Surface, INTERACTION_HEIGHT)
}

#[derive(Component, Default)]
#[require(HitReaction = HitReaction::new(HIT_REACTION_SECONDS), RemoveColliderOnDeath = RemoveColliderOnDeath, FleeState = FleeState::keeper())]
#[require(crate::experience::ExperienceReward = crate::experience::ExperienceReward::new(EXPERIENCE_REWARD))]
pub(crate) struct Nemi;

#[derive(Component)]
pub(crate) struct FleeState {
    home: Option<Vec3>,
    behavior: Behavior,
    ground_offset: f32,
    refuses_on_return: bool,
    entrance: Option<Doorway>,
}

impl FleeState {
    pub(crate) fn civilian() -> Self {
        Self {
            home: None,
            behavior: Behavior::Peaceful,
            ground_offset: 0.0,
            refuses_on_return: false,
            entrance: None,
        }
    }

    fn keeper() -> Self {
        Self {
            ground_offset: GROUND_OFFSET,
            refuses_on_return: true,
            ..Self::civilian()
        }
    }
}

#[derive(
    Default, Clone, Copy, bitcode::Encode, bitcode::Decode, serde::Serialize, serde::Deserialize,
)]
enum Behavior {
    #[default]
    Peaceful,
    Fleeing {
        safe_for: f32,
    },
    Returning,
    Refuses,
}

#[derive(Clone, bitcode::Encode, bitcode::Decode, serde::Serialize, serde::Deserialize)]
pub(crate) struct NemiState {
    home: Option<[f32; 3]>,
    behavior: Behavior,
}

impl NemiState {
    pub(crate) fn is_valid(&self) -> bool {
        self.home
            .is_none_or(|home| Vec3::from_array(home).is_finite())
            && match self.behavior {
                Behavior::Peaceful => true,
                Behavior::Fleeing { safe_for } => {
                    self.home.is_some() && (0.0..SAFE_TIME).contains(&safe_for)
                }
                Behavior::Returning | Behavior::Refuses => self.home.is_some(),
            }
    }
}

impl FleeState {
    pub(crate) fn snapshot(&self) -> NemiState {
        NemiState {
            home: self.home.map(|home| home.to_array()),
            behavior: self.behavior,
        }
    }

    pub(crate) fn restore(&mut self, state: &NemiState) {
        self.home = state.home.map(Vec3::from_array);
        self.behavior = state.behavior;
    }

    pub(crate) fn set_residence(&mut self, home: Vec3, entrance: Option<Doorway>) {
        self.home = Some(home);
        self.entrance = entrance;
    }

    pub(crate) fn can_talk(&self) -> bool {
        matches!(self.behavior, Behavior::Peaceful | Behavior::Refuses)
    }

    pub(crate) fn refuses(&self) -> bool {
        matches!(self.behavior, Behavior::Refuses)
    }

    /// Advance flee and return behavior, then choose the desired ground motion.
    fn plan_motion(
        &mut self,
        position: Vec3,
        away: Vec3,
        player_on_surface: bool,
        delta: f32,
    ) -> Option<(Vec3, f32)> {
        let home = self.home?;
        let distance = if player_on_surface {
            away.length()
        } else {
            f32::INFINITY
        };
        if matches!(self.behavior, Behavior::Returning) && distance < PURSUED_DISTANCE {
            self.behavior = Behavior::Fleeing { safe_for: 0.0 };
        }
        match self.behavior {
            Behavior::Fleeing { safe_for } if distance >= SAFE_DISTANCE => {
                let safe_for = safe_for + delta;
                self.behavior = if safe_for >= SAFE_TIME {
                    Behavior::Returning
                } else {
                    Behavior::Fleeing { safe_for }
                };
                None
            }
            Behavior::Fleeing { .. } => {
                self.behavior = Behavior::Fleeing { safe_for: 0.0 };
                Some((away.try_normalize().unwrap_or(Vec3::Z), FLEE_SPEED))
            }
            Behavior::Returning => {
                let toward_home = (home - position).with_y(0.0);
                if toward_home.length_squared() < HOME_ARRIVAL_RADIUS_SQUARED {
                    self.behavior = if self.refuses_on_return {
                        Behavior::Refuses
                    } else {
                        Behavior::Peaceful
                    };
                    return None;
                }
                let toward_waypoint = self.entrance.map_or(toward_home, |door| {
                    let point = door.approach(position.xz(), home.xz());
                    Vec3::new(point.x - position.x, 0.0, point.y - position.z)
                });
                Some((
                    toward_waypoint.normalize_or_zero(),
                    RETURN_SPEED.min(toward_waypoint.length() / delta),
                ))
            }
            Behavior::Peaceful | Behavior::Refuses => None,
        }
    }
}

impl Plugin for NemiPlugin {
    fn build(&self, app: &mut App) {
        dialogue::register_topics(
            &mut app
                .init_resource::<DialogueRegistry>()
                .world_mut()
                .resource_mut(),
        );
        app.add_systems(
            Update,
            sync_speaker
                .before(InteractionSystems)
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(
            Update,
            react_to_player_hit
                .after(CombatSystems::Resolve)
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(
            FixedUpdate,
            move_fleeing_actors
                .after(crate::player::movement)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

fn sync_speaker(mut speakers: Query<(&FleeState, &mut Speaker)>) {
    for (nemi, mut speaker) in &mut speakers {
        let availability = if nemi.refuses() {
            Availability::Refuses
        } else if nemi.can_talk() {
            Availability::Friendly
        } else {
            Availability::Unavailable
        };
        if speaker.availability != availability {
            speaker.availability = availability;
        }
    }
}

pub(crate) fn react_to_player_hit(
    mut hits: MessageReader<ConfirmedHit>,
    players: Query<(), With<Player>>,
    mut keepers: Query<(&mut FleeState, &Transform, Has<Nemi>, Has<Dead>)>,
    mut journal: ResMut<QuestJournal>,
    mut dialogue: ResMut<Dialogue>,
) {
    for hit in hits.read() {
        if !players.contains(hit.source) {
            continue;
        }
        let Ok((mut flee, transform, keeper, dead)) = keepers.get_mut(hit.target) else {
            continue;
        };
        if keeper {
            journal.record(QuestAction::AttackedNemi);
        }
        dialogue.interrupt(hit.target);
        if !dead {
            flee.home.get_or_insert(transform.translation);
            flee.behavior = Behavior::Fleeing { safe_for: 0.0 };
        }
    }
}

pub(crate) fn move_fleeing_actors(
    time: Res<Time<Fixed>>,
    items: Res<Items>,
    players: Query<(Entity, &Player, &Transform), Without<FleeState>>,
    mut actors: Query<
        (
            Entity,
            &mut FleeState,
            &mut Transform,
            &Collider,
            &mut NpcGait,
            Has<Homebound>,
            &CarryLimits,
            &Inventory,
            &Equipment,
        ),
        (Without<Dead>, Without<Player>),
    >,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
) {
    let Ok((player, controller, player_transform)) = players.single() else {
        return;
    };
    let delta = time.delta_secs();
    for (entity, mut flee, mut transform, collider, mut gait, homebound, limits, bag, equipment) in
        &mut actors
    {
        if homebound && flee.can_talk() {
            continue;
        }
        let mut next_gait = NpcGait::Idle;
        let position = transform.translation;
        let away = (position - player_transform.translation).with_y(0.0);
        let player_on_surface = controller.area == crate::dungeon::Area::Surface;
        let Some((direction, speed)) =
            flee.bypass_change_detection()
                .plan_motion(position, away, player_on_surface, delta)
        else {
            gait.set_if_neq(next_gait);
            continue;
        };
        let speed = limits.ground_speed(bag.carried_weight(equipment, &items), speed);
        if speed == 0.0 {
            gait.set_if_neq(next_gait);
            continue;
        }
        let collision = SpatialQueryFilter::from_excluded_entities([entity]);
        let support = SpatialQueryFilter::from_excluded_entities([entity, player]);
        // Sample alternate headings if a wall or the shelf's edge blocks the direct route.
        for angle in AVOIDANCE_ANGLES_DEGREES {
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
            if movement.dot(direction) < speed * delta * MIN_TRAVEL_FRACTION {
                continue;
            }
            let Some(height) = walkable_height(
                &spatial,
                &support,
                output.position + Vec3::Y * MAX_GROUND_STEP,
                flee.ground_offset + GROUND_PROBE_MARGIN_M,
                flee.ground_offset,
            ) else {
                continue;
            };
            if (height - position.y).abs() > MAX_GROUND_STEP {
                continue;
            }
            transform.translation = output.position.with_y(height);
            transform.rotation = transform
                .rotation
                .slerp(ground_heading(movement), (TURN_SPEED * delta).min(1.0));
            next_gait = if matches!(flee.behavior, Behavior::Fleeing { .. })
                && speed > Pace::Walk.speed()
            {
                NpcGait::Run(speed)
            } else {
                NpcGait::Walk(speed)
            };
            break;
        }
        gait.set_if_neq(next_gait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::{CombatPlugin, Health, Hit},
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
            .spawn((
                Nemi,
                Health::new(84),
                Transform::default(),
                Collider::sphere(0.4),
            ))
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
            83
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
        assert!(
            !app.world()
                .get::<FleeState>(keeper)
                .expect("Nemi")
                .can_talk()
        );
        app.world_mut().write_message(Hit {
            damage: u16::MAX,
            ..strike
        });
        app.update();
        assert!(app.world().get::<Dead>(keeper).is_some());
        assert!(app.world().get::<Collider>(keeper).is_none());
    }
}
