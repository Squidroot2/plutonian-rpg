//! The two outpost guards share an incident response and defend Nivalis.
use crate::{
    GameState,
    attributes::{CarryLimits, MeleeDamageMultiplier},
    combat::{
        ActorDefeated, CombatSystems, ConfirmedHit, Dead, Hit, HitReaction, RemoveColliderOnDeath,
    },
    content::{Enemies, Items, NpcId, Npcs, Placements},
    dialogue::{Dialogue, Speaker},
    dungeon::Area,
    enemy::{self, Enemy, GroundMovement},
    equipment::{Equipment, EquipmentSlot, bind_equipment_attachments},
    experience::ExperienceReward,
    interaction::{CorpseLoot, InteractionSystems},
    inventory::{Inventory, Pickup},
    loading::{GameAssets, PendingScene, scene_ready},
    nemi::Nemi,
    player::{
        ActorVisual, NpcGait, PLAYER_COLLIDER_LENGTH, PLAYER_COLLIDER_RADIUS, PLAYER_GROUND_OFFSET,
        Pace, Player, bind_actor_animation,
    },
    save::{EncounterId, save_at_bed},
    terrain::ground_heading,
    weapon::{self, Hand, WeaponAttack, WeaponStance},
    world::{SessionEntity, SpawnWorld, WorldLayout},
};
use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*};

const OUTPOST_CENTER_XZ: Vec2 = Vec2::ZERO;
const OUTPOST_RADIUS_M: f32 = 12.0;
const HOME_STOP_DISTANCE_M: f32 = 0.2;

#[derive(
    Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum GuardPost {
    West,
    East,
}

impl GuardPost {
    pub(crate) const ALL: [Self; 2] = [Self::West, Self::East];

    pub(crate) const fn home(self) -> Vec3 {
        match self {
            Self::West => Vec3::new(-4.22, 0.0, -8.77),
            Self::East => Vec3::new(-4.22, 0.0, -11.23),
        }
    }

    const fn npc(self) -> NpcId {
        match self {
            Self::West => NpcId::Sella,
            Self::East => NpcId::Orren,
        }
    }

    const fn interaction_height(self) -> f32 {
        0.35
    }

    pub(crate) fn facing(self) -> Quat {
        ground_heading(Vec3::new(1.0, 0.0, 0.30))
    }
}

#[derive(Resource, Default)]
pub(crate) struct OutpostSecurity {
    pub(crate) hostile: bool,
}

impl OutpostSecurity {
    pub(crate) fn defends(&self, area: Area, target: Vec3) -> bool {
        self.hostile
            && area == Area::Surface
            && target.xz().distance_squared(OUTPOST_CENTER_XZ)
                <= OUTPOST_RADIUS_M * OUTPOST_RADIUS_M
    }
}

pub(crate) struct GuardPlugin;

impl Plugin for GuardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OutpostSecurity>()
            .add_observer(spawn_guards)
            .add_systems(
                FixedUpdate,
                plan_guard_actions
                    .after(crate::player::movement)
                    .after(crate::crossing::CaravanArrivalSystems)
                    .before(enemy::move_ground_enemies)
                    .run_if(in_state(GameState::Playing))
                    .run_if(enemy::no_dialogue)
                    .run_if(any_with_component::<GuardAction>),
            )
            .add_systems(
                FixedUpdate,
                sync_gait
                    .after(enemy::move_ground_enemies)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                strike
                    .run_if(any_with_component::<GuardAction>)
                    .after(InteractionSystems)
                    .in_set(CombatSystems::Detect)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (react_to_player_hit, resolve_armed_loot)
                    .after(CombatSystems::Resolve)
                    .before(save_at_bed)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Component)]
struct GuardAction {
    chase_speed: f32,
    next_hand: Hand,
    ready: Option<Hand>,
}

fn spawn_guards(
    _: On<SpawnWorld>,
    mut commands: Commands,
    layout: Res<WorldLayout>,
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
    placements: Res<Placements>,
    enemies: Res<Enemies>,
    npcs: Res<Npcs>,
    items: Res<Items>,
) {
    if *layout != WorldLayout::Outpost {
        return;
    }
    let scene = models
        .get(&assets.character)
        .expect("the loaded character rig must remain available")
        .scenes[0]
        .clone();
    for post in GuardPost::ALL {
        let id = EncounterId::Guard(post);
        let npc = post.npc();
        let (station, facing) = (post.home(), post.facing());
        let (name, chase_speed, xp) = placements.enemy(&enemies, id).guard_role();
        let (ranks, inventory, equipment) = npcs.loadout(npc, &items);
        let item = equipment
            .get(EquipmentSlot::MainHand)
            .expect("a validated guard must have a main-hand weapon");
        commands
            .spawn((
                (
                    Enemy,
                    SessionEntity,
                    post,
                    id,
                    npc,
                    Name::new(name.to_owned()),
                ),
                (
                    ranks.actor_components(),
                    inventory,
                    equipment,
                    WeaponAttack::default(),
                    WeaponStance::Drawn,
                    NpcGait::default(),
                ),
                (
                    Collider::capsule(PLAYER_COLLIDER_RADIUS, PLAYER_COLLIDER_LENGTH),
                    RigidBody::Kinematic,
                    CustomPositionIntegration,
                    Transform::from_translation(station).with_rotation(facing),
                    Visibility::default(),
                ),
                (
                    GroundMovement::new(chase_speed, PLAYER_GROUND_OFFSET),
                    GuardAction {
                        chase_speed,
                        next_hand: Hand::Main,
                        ready: None,
                    },
                    HitReaction::new(enemy::HIT_REACTION_SECONDS),
                    RemoveColliderOnDeath,
                    ExperienceReward::new(xp),
                    CorpseLoot {
                        stack: Some(Pickup { item, quantity: 1 }),
                        area: Area::Surface,
                    },
                    Speaker::new(npc.key(), Area::Surface, post.interaction_height()),
                ),
            ))
            .with_children(|parent| {
                parent
                    .spawn((
                        ActorVisual::Npc,
                        PendingScene,
                        WorldAssetRoot(scene.clone()),
                        Transform::from_translation(-Vec3::Y * PLAYER_GROUND_OFFSET),
                    ))
                    .observe(scene_ready)
                    .observe(bind_actor_animation)
                    .observe(bind_equipment_attachments);
                enemy::spawn_hit_glow(parent);
            });
    }
}

fn plan_guard_actions(
    items: Res<Items>,
    security: Res<OutpostSecurity>,
    spatial: SpatialQuery,
    players: Query<(Entity, &Player, &Transform), (Without<GuardPost>, Without<Dead>)>,
    mut guards: Query<
        (
            Entity,
            &GuardPost,
            &mut Transform,
            &mut GroundMovement,
            &mut GuardAction,
            &WeaponAttack,
            &Equipment,
            &Inventory,
            &CarryLimits,
        ),
        (Without<Player>, Without<Dead>),
    >,
) {
    let Ok((player, controller, target)) = players.single() else {
        return;
    };
    for (entity, post, mut pose, mut movement, mut action, attack, equipment, bag, limits) in
        &mut guards
    {
        movement.destination = None;
        movement.speed =
            limits.ground_speed(bag.carried_weight(equipment, &items), action.chase_speed);
        action.ready = None;
        if !security.defends(controller.area, target.translation) {
            let (station, facing) = (post.home(), post.facing());
            if pose.translation.xz().distance(station.xz()) > HOME_STOP_DISTANCE_M {
                movement.destination = Some(station);
            } else {
                pose.set_if_neq(pose.with_rotation(facing));
            }
            continue;
        }
        if attack.active() {
            continue;
        }
        if let Some((hand, facing)) = choose_strike(
            &items,
            equipment,
            action.next_hand,
            &spatial,
            entity,
            player,
            pose.translation,
            target.translation,
        ) {
            pose.rotation = facing;
            action.ready = Some(hand);
        } else {
            movement.destination = Some(target.translation);
        }
    }
}

fn choose_strike(
    items: &Items,
    equipment: &Equipment,
    next_hand: Hand,
    spatial: &SpatialQuery,
    guard: Entity,
    target: Entity,
    position: Vec3,
    target_position: Vec3,
) -> Option<(Hand, Quat)> {
    [next_hand, other_hand(next_hand)]
        .into_iter()
        .find_map(|hand| {
            let (profile, stats) = items[equipment.get(hand.slot())?].attack()?;
            let delta = target_position - position;
            let facing = weapon::weapon_heading(profile, hand, delta);
            let hit =
                weapon::weapon_contact(profile, stats, hand, spatial, guard, position, facing)?;
            (hit.target == target).then_some((hand, facing))
        })
}

fn other_hand(hand: Hand) -> Hand {
    match hand {
        Hand::Main => Hand::Off,
        Hand::Off => Hand::Main,
    }
}

fn strike(
    items: Res<Items>,
    security: Res<OutpostSecurity>,
    dialogue: Res<Dialogue>,
    time: Res<Time>,
    spatial: SpatialQuery,
    player: Query<(Entity, &Player, &Transform), Without<Dead>>,
    mut guards: Query<
        (
            Entity,
            &Transform,
            &Equipment,
            &MeleeDamageMultiplier,
            &mut GuardAction,
            &mut WeaponAttack,
        ),
        (With<GuardPost>, Without<Dead>),
    >,
    mut hits: MessageWriter<Hit>,
) {
    let target = player.single().ok().filter(|(_, player, pose)| {
        !dialogue.is_open() && security.defends(player.area, pose.translation)
    });
    for (entity, pose, equipment, melee, mut action, mut attack) in &mut guards {
        let Some((player, _, _)) = target else {
            attack.cancel();
            action.ready = None;
            continue;
        };
        if attack.active() {
            for hand in Hand::ALL {
                if let Some(hit) = attack.strike(
                    hand,
                    equipment,
                    time.delta_secs(),
                    *melee,
                    &spatial,
                    entity,
                    pose,
                ) && hit.target == player
                {
                    hits.write(hit);
                }
            }
        } else if let Some(hand) = action.ready.take()
            && attack.begin(hand, equipment, &items)
        {
            action.next_hand = other_hand(hand);
        }
    }
}

fn sync_gait(mut guards: Query<(&GroundMovement, &mut NpcGait), With<GuardPost>>) {
    for (movement, mut gait) in &mut guards {
        gait.set_if_neq(if !movement.moving {
            NpcGait::Idle
        } else if movement.speed <= Pace::Walk.speed() {
            NpcGait::Walk(movement.speed)
        } else {
            NpcGait::Run(movement.speed)
        });
    }
}

fn resolve_armed_loot(
    mut defeats: MessageReader<ActorDefeated>,
    mut guards: Query<(&Equipment, &mut CorpseLoot), With<GuardPost>>,
) {
    for ActorDefeated(entity) in defeats.read() {
        if let Ok((equipment, mut loot)) = guards.get_mut(*entity) {
            loot.stack = equipment
                .get(EquipmentSlot::MainHand)
                .map(|item| Pickup { item, quantity: 1 });
        }
    }
}

pub(crate) fn react_to_player_hit(
    mut hits: MessageReader<ConfirmedHit>,
    players: Query<(), With<Player>>,
    protected: Query<(), Or<(With<GuardPost>, With<Nemi>)>>,
    mut security: ResMut<OutpostSecurity>,
) {
    for hit in hits.read() {
        if !security.hostile && players.contains(hit.source) && protected.contains(hit.target) {
            security.hostile = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::{CombatPlugin, Dead, Health, Hit},
        dialogue::Dialogue,
    };
    use bevy::state::app::StatesPlugin;

    fn armed_encounter(dual_wield: bool) -> (App, Entity, Entity) {
        use crate::content::{test_items, test_npcs};

        let mut app = crate::enemy::tests::test_app_builder();
        app.add_plugins(GuardPlugin);
        app.finish();
        app.cleanup();
        let items = test_items();
        let (ranks, mut bag, mut equipment) = test_npcs().loadout(NpcId::Sella, items);
        assert_eq!(ranks.effects().health, 140);
        assert_eq!(equipment.total_defense(items), 8);
        if dual_wield {
            assert!(bag.add(crate::inventory::ItemKind::Shortsword, 2));
            for slot in EquipmentSlot::HANDS {
                assert_eq!(
                    equipment.equip_in(
                        &mut bag,
                        slot,
                        crate::inventory::ItemKind::Shortsword,
                        98,
                        items
                    ),
                    Ok(true)
                );
            }
        }
        let weapon = equipment
            .get(EquipmentSlot::MainHand)
            .expect("guard is armed");
        let world = app.world_mut();
        world.resource_mut::<OutpostSecurity>().hostile = true;
        let player = world
            .spawn((
                Player::default(),
                Health::new(200),
                RigidBody::Static,
                Collider::capsule(0.32, 1.15),
                Transform::from_xyz(
                    0.0,
                    PLAYER_GROUND_OFFSET,
                    if dual_wield { 1.1 } else { 2.25 },
                ),
            ))
            .id();
        let guard = world
            .spawn((
                GuardPost::West,
                ranks.actor_components(),
                bag,
                equipment,
                WeaponAttack::default(),
                GroundMovement::new(5.5, PLAYER_GROUND_OFFSET),
                Transform::from_xyz(0.0, PLAYER_GROUND_OFFSET, 0.0),
                GuardAction {
                    chase_speed: 5.5,
                    next_hand: Hand::Main,
                    ready: None,
                },
                CorpseLoot {
                    stack: Some(Pickup {
                        item: weapon,
                        quantity: 1,
                    }),
                    area: Area::Surface,
                },
            ))
            .id();
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        (app, player, guard)
    }

    #[test]
    fn guard_uses_the_catalog_polearm_damage_and_drops_the_equipped_weapon() {
        let (mut app, player, guard) = armed_encounter(false);
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(
            app.world()
                .get::<Health>(player)
                .expect("player health")
                .current(),
            142
        );
        app.world_mut().write_message(Hit {
            source: player,
            target: guard,
            damage: u16::MAX,
            contact: None,
        });
        app.update();
        let world = app.world();
        assert!(world.get::<Dead>(guard).is_some());
        let loot = world
            .get::<CorpseLoot>(guard)
            .expect("guard corpse")
            .stack
            .expect("unlooted weapon");
        assert_eq!((loot.item.as_str(), loot.quantity), ("guard_polearm", 1));
    }

    #[test]
    fn guard_alternates_hands_with_a_dual_wield_loadout() {
        let (mut app, _, guard) = armed_encounter(true);
        let mut seen = Vec::new();
        for _ in 0..85 {
            app.update();
            let swing = app
                .world()
                .get::<WeaponAttack>(guard)
                .expect("guard attack state");
            for hand in Hand::ALL {
                if swing.current(hand).is_some() && seen.last() != Some(&hand) {
                    seen.push(hand);
                }
            }
        }
        assert!(
            seen.starts_with(&[Hand::Main, Hand::Off, Hand::Main]),
            "{seen:?}"
        );
    }

    #[test]
    fn only_confirmed_player_hits_on_the_keeper_or_either_guard_raise_the_alarm() {
        for target_index in 0..3 {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatesPlugin, CombatPlugin, GuardPlugin))
                .insert_state(GameState::Playing)
                .init_resource::<Dialogue>();
            let world = app.world_mut();
            let player = world.spawn(Player::default()).id();
            let stranger = world.spawn_empty().id();
            let other_target = world.spawn(Health::new(10)).id();
            let protected = [
                world.spawn((Nemi, Health::new(84))).id(),
                world.spawn((GuardPost::West, Health::new(140))).id(),
                world.spawn((GuardPost::East, Health::new(140))).id(),
            ];
            let target = protected[target_index];
            for (source, target, damage) in [
                (player, target, 0),
                (stranger, target, 1),
                (player, other_target, 1),
            ] {
                world.write_message(Hit {
                    source,
                    target,
                    damage,
                    contact: None,
                });
            }
            app.update();
            assert!(!app.world().resource::<OutpostSecurity>().hostile);

            app.world_mut().write_message(Hit {
                source: player,
                target,
                damage: 140,
                contact: None,
            });
            app.update();
            let world = app.world();
            assert!(world.resource::<OutpostSecurity>().hostile);
            assert!(world.get::<Dead>(target).is_some());
            assert!(
                protected
                    .iter()
                    .any(|&entity| world.get::<GuardPost>(entity).is_some()
                        && world.get::<Dead>(entity).is_none())
            );
        }
    }

    #[test]
    fn the_alarm_survives_leaving_the_outpost_and_applies_again_on_return() {
        let mut security = OutpostSecurity::default();
        let center = Vec3::new(OUTPOST_CENTER_XZ.x, 0.0, OUTPOST_CENTER_XZ.y);
        assert!(!security.defends(Area::Surface, center));
        security.hostile = true;
        for post in GuardPost::ALL {
            assert!(security.defends(Area::Surface, post.home()));
        }
        assert!(security.defends(Area::Surface, WorldLayout::Outpost.keeper()));
        assert!(security.defends(Area::Surface, center + Vec3::X * OUTPOST_RADIUS_M));
        assert!(!security.defends(Area::Surface, center + Vec3::X * (OUTPOST_RADIUS_M + 0.1)));
        assert!(!security.defends(Area::OldRelay, center));
        assert!(security.defends(Area::Surface, center));
    }
}
