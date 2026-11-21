use crate::{
    GameState,
    combat::Dead,
    controls::{Control, Keybinds},
    dungeon::{Area, DungeonTarget, UseDungeonTarget},
    inventory::{Inventory, ItemKind, Pickup},
    nemi::Nemi,
    player::Player,
    quest::{Dialogue, LAST_LIGHT, QuestAction, QuestJournal},
    quest_ui::QuestLog,
};
use avian3d::prelude::*;
use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Keeper,
    Core,
    BeaconSocket,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionTarget {
    Quest(Target),
    Pickup(Pickup),
    Dungeon(DungeonTarget),
    Corpse,
}
#[derive(Component)]
pub(crate) struct CorpseLoot {
    pub(crate) stack: Option<Pickup>,
    pub(crate) area: Area,
}

impl CorpseLoot {
    pub(crate) fn new(item: ItemKind, area: Area) -> Self {
        Self {
            stack: Some(Pickup { item, quantity: 1 }),
            area,
        }
    }
}

pub(crate) const INTERACT_RANGE: f32 = 3.0;
pub(crate) const INTERACT_HEIGHT: f32 = 0.45;

#[derive(Resource, Default)]
pub(crate) struct LootTarget(pub(crate) Option<Entity>);

#[derive(Resource, Default, PartialEq, Eq)]
pub struct Nearby(pub Option<(Entity, InteractionTarget)>);
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractionSystems;

pub struct InteractionPlugin;
impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Nearby>()
            .init_resource::<LootTarget>()
            .add_systems(
                Update,
                (find_target, interact)
                    .chain()
                    .in_set(InteractionSystems)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
fn find_target(
    players: Query<(Entity, &Transform, &Player)>,
    targets: Query<
        (
            Entity,
            &Transform,
            Option<&Target>,
            Option<&Pickup>,
            Option<&DungeonTarget>,
            Option<&CorpseLoot>,
            Has<Dead>,
            Option<&Nemi>,
        ),
        Or<(
            With<Target>,
            With<Pickup>,
            With<DungeonTarget>,
            With<CorpseLoot>,
        )>,
    >,
    quest: Res<QuestJournal>,
    dialogue: Res<Dialogue>,
    spatial: SpatialQuery,
    log: Option<Res<QuestLog>>,
    mut nearby: ResMut<Nearby>,
) {
    if dialogue.text.is_some() || log.is_some_and(|log| log.is_open()) {
        nearby.set_if_neq(Nearby::default());
        return;
    }
    let Ok((player_entity, transform, player)) = players.single() else {
        nearby.set_if_neq(Nearby::default());
        return;
    };
    let mut selected = None;
    let origin = transform.translation + Vec3::Y * INTERACT_HEIGHT;
    let mut closest = INTERACT_RANGE;
    for (entity, transform, target, pickup, dungeon, corpse, dead, nemi) in &targets {
        let (interaction, point) = if let Some(target) = target {
            let available = match target {
                Target::Keeper => !dead && nemi.is_none_or(Nemi::can_talk),
                Target::Core => quest.expects(QuestAction::RecoveredCore),
                Target::BeaconSocket => quest.expects(QuestAction::InstalledCore),
            };
            if player.area != Area::Surface || !available {
                continue;
            }
            (
                InteractionTarget::Quest(*target),
                transform.translation
                    + if *target == Target::Keeper {
                        Vec3::Y * 0.35
                    } else {
                        Vec3::ZERO
                    },
            )
        } else if let Some(pickup) = pickup {
            (
                InteractionTarget::Pickup(*pickup),
                transform.translation + Vec3::Y * 0.18,
            )
        } else if let Some(&dungeon) = dungeon {
            if player.area != dungeon.area() {
                continue;
            }
            let height = match dungeon {
                DungeonTarget::Hatch | DungeonTarget::Exit => 0.65,
                _ => 0.0,
            };
            (
                InteractionTarget::Dungeon(dungeon),
                transform.translation + Vec3::Y * height,
            )
        } else if corpse
            .is_some_and(|loot| dead && loot.area == player.area && loot.stack.is_some())
        {
            (InteractionTarget::Corpse, transform.translation)
        } else {
            continue;
        };
        let delta = point - origin;
        let distance = delta.length();
        if distance >= closest {
            continue;
        }
        let Ok(direction) = Dir3::new(delta) else {
            continue;
        };
        if spatial
            .cast_ray(
                origin,
                direction,
                distance,
                true,
                &SpatialQueryFilter::from_excluded_entities([player_entity, entity]),
            )
            .is_none()
        {
            closest = distance;
            selected = Some((entity, interaction));
        }
    }
    nearby.set_if_neq(Nearby(selected));
}
fn interact(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    nearby: Res<Nearby>,
    mut dialogue: ResMut<Dialogue>,
    mut quest: ResMut<QuestJournal>,
    mut inventory: ResMut<Inventory>,
    mut commands: Commands,
    pickups: Query<&Pickup>,
    keepers: Query<&Nemi, Without<Dead>>,
    mut loot: ResMut<LootTarget>,
    mut next: ResMut<NextState<GameState>>,
    log: Option<Res<QuestLog>>,
) {
    if log.is_some_and(|log| log.is_open()) {
        return;
    }
    let interact_pressed = bindings.just_pressed(Control::Interact, &keys);
    if !interact_pressed && !keys.just_pressed(KeyCode::Enter) {
        return;
    }
    if dialogue.text.take().is_some() {
        return;
    }
    if let Some((entity, target)) = nearby.0 {
        match target {
            InteractionTarget::Quest(Target::Keeper) => {
                dialogue.text = Some(if keepers.get(entity).is_ok_and(Nemi::refuses) {
                    "NEMI\n\nStay away from me."
                } else {
                    speak_to_nemi(&mut quest, &mut inventory)
                });
            }
            InteractionTarget::Quest(Target::Core) => {
                if quest.expects(QuestAction::RecoveredCore)
                    && inventory.add(ItemKind::BeaconCore, 1)
                {
                    quest.record(QuestAction::RecoveredCore);
                    dialogue.text = Some(if quest.expects(QuestAction::InstalledCore) {
                        "POWER CORE RECOVERED\n\nInstall it in the socket at the south side of the beacon tower."
                    } else {
                        "POWER CORE RECOVERED\n\nIts light pulses against your hands. There is still warmth inside.\n\nReturn to Nemi at the outpost to restore the beacon."
                    });
                    commands.entity(entity).despawn();
                    commands.trigger(crate::combat_audio::ItemPickedUp);
                }
            }
            InteractionTarget::Quest(Target::BeaconSocket) => {
                dialogue.text = Some(install_core(&mut quest, &mut inventory));
            }
            InteractionTarget::Pickup(_) if interact_pressed => {
                // Re-read the live component before transferring the stack.
                if let Ok(pickup) = pickups.get(entity) {
                    if inventory.add(pickup.item, pickup.quantity) {
                        commands.entity(entity).despawn();
                        commands.trigger(crate::combat_audio::ItemPickedUp);
                    }
                }
            }
            InteractionTarget::Dungeon(target) => commands.trigger(UseDungeonTarget(target)),
            InteractionTarget::Corpse
                if interact_pressed && matches!(*next, NextState::Unchanged) =>
            {
                loot.0 = Some(entity);
                next.set(GameState::Looting);
            }
            _ => (),
        }
    }
}

fn install_core(quest: &mut QuestJournal, inventory: &mut Inventory) -> &'static str {
    if quest.expects(QuestAction::InstalledCore) && inventory.consume_core() {
        quest.record(QuestAction::InstalledCore);
        "BEACON RESTORED\n\nThe core locks into the socket and the tower lights up. The Old Relay hatch northwest of the tower now has power."
    } else {
        "BEACON SOCKET\n\nThe socket needs a charged beacon core."
    }
}

fn speak_to_nemi(quest: &mut QuestJournal, inventory: &mut Inventory) -> &'static str {
    if quest.expects(QuestAction::ReturnedCore) {
        if inventory.consume_core() {
            quest.record(QuestAction::ReturnedCore);
            return "NEMI\n\nThere it is. I thought we had lost that light for good. The crossing is safe again.\n\nThe beacon also powers the Old Relay. Look for the round hatch west and a little south of here. Its archive may still hold the first survey log.\n\nQUEST COMPLETE / THE LAST LIGHT";
        }
        return "NEMI\n\nBring the beacon power core back to me first.";
    }
    let first_meeting = quest.expects(QuestAction::TalkedToNemi);
    quest.record(QuestAction::TalkedToNemi);
    if first_meeting {
        "NEMI / BEACON KEEPER\n\nYou made it home before the long dark. Our beacon has gone silent. The families here depend on its signal to cross the ice.\n\nA charged core waits in the blue cavern, beyond the waystones. Follow their lights north, then bring it back."
    } else if quest.is_complete(LAST_LIGHT) {
        "NEMI\n\nThe light reaches across the ice again. Look for the round Old Relay hatch west and a little south of here."
    } else {
        "NEMI\n\nFollow the blue waystones north from the outpost. The core is inside the cavern at the end of the trail."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::AssetPlugin, mesh::MeshPlugin, state::app::StatesPlugin, time::TimeUpdateStrategy,
        transform::TransformPlugin,
    };
    use std::time::Duration;
    #[test]
    fn turn_in_requires_and_consumes_the_core() {
        let mut journal = crate::quest::test_journal();
        let mut inventory = Inventory::default();
        speak_to_nemi(&mut journal, &mut inventory);
        journal.record(QuestAction::RecoveredCore);
        speak_to_nemi(&mut journal, &mut inventory);
        assert!(journal.expects(QuestAction::ReturnedCore));
        assert!(inventory.add(ItemKind::BeaconCore, 1));
        speak_to_nemi(&mut journal, &mut inventory);
        assert!(journal.is_complete(LAST_LIGHT));
        assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
    }

    #[test]
    fn installing_the_core_consumes_it_once_and_requires_the_solo_objective() {
        let mut journal = crate::quest::test_journal();
        let mut inventory = Inventory::default();
        journal.record(QuestAction::TalkedToNemi);
        assert!(inventory.add(ItemKind::BeaconCore, 1));
        journal.record(QuestAction::RecoveredCore);
        install_core(&mut journal, &mut inventory);
        assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
        journal.record(QuestAction::AttackedNemi);
        install_core(&mut journal, &mut inventory);
        assert!(journal.beacon_powered());
        assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
        assert!(inventory.add(ItemKind::BeaconCore, 1));
        install_core(&mut journal, &mut inventory);
        assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
    }

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
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            1.0 / 60.0,
        )))
        .init_state::<GameState>()
        .insert_resource(crate::quest::test_journal())
        .init_resource::<Dialogue>()
        .init_resource::<Inventory>()
        .init_resource::<Keybinds>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins((InteractionPlugin, crate::loot_ui::LootUiPlugin));
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.world_mut()
            .spawn((Player::default(), Transform::default()));
        app.finish();
        app.cleanup();
        for _ in 0..3 {
            app.update();
        }
        app
    }
    fn press_interact(app: &mut App) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.reset_all();
        input.press(KeyCode::KeyE);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
    }

    #[test]
    fn only_dead_unobstructed_corpses_in_the_same_area_and_in_range_are_targeted() {
        let mut app = test_app();
        let corpse = app
            .world_mut()
            .spawn((
                Name::new("Ice worm"),
                Transform::from_xyz(0.0, 0.0, -2.0),
                CorpseLoot::new(ItemKind::WormMeat, Area::Surface),
            ))
            .id();

        press_interact(&mut app);
        assert!(app.world().resource::<Nearby>().0.is_none());

        app.world_mut().entity_mut(corpse).insert(Dead);
        app.world_mut()
            .get_mut::<CorpseLoot>(corpse)
            .expect("loot")
            .area = Area::OldRelay;
        press_interact(&mut app);
        assert!(app.world().resource::<Nearby>().0.is_none());

        app.world_mut()
            .get_mut::<CorpseLoot>(corpse)
            .expect("loot")
            .area = Area::Surface;
        app.world_mut()
            .get_mut::<Transform>(corpse)
            .expect("position")
            .translation
            .z = -4.0;
        press_interact(&mut app);
        assert!(app.world().resource::<Nearby>().0.is_none());

        app.world_mut()
            .get_mut::<Transform>(corpse)
            .expect("position")
            .translation
            .z = -2.0;
        let wall = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(3.0, 3.0, 0.2),
                Transform::from_xyz(0.0, 0.0, -1.0),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }
        press_interact(&mut app);
        assert!(app.world().resource::<Nearby>().0.is_none());

        app.world_mut().despawn(wall);
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<Nearby>().0,
            Some((corpse, InteractionTarget::Corpse))
        );
    }

    #[test]
    fn loot_panel_transfers_each_corpse_stack_once_and_keeps_the_body() {
        use crate::{loot_ui::LootAction, smoke::pointer_click};

        let mut app = test_app();
        let window = app.world_mut().spawn_empty().id();
        let camera = app.world_mut().spawn_empty().id();
        let click = |app: &mut App, action| {
            let world = app.world_mut();
            let button = world
                .query::<(Entity, &LootAction)>()
                .iter(world)
                .find(|(_, current)| **current == action)
                .expect("loot action")
                .0;
            world.trigger(pointer_click(button, window, camera));
        };
        for item in [ItemKind::WormMeat, ItemKind::RoachMeat] {
            let corpse = app
                .world_mut()
                .spawn((
                    Dead,
                    Name::new(item.name()),
                    Transform::from_xyz(0.0, 0.0, -2.0),
                    CorpseLoot::new(item, Area::Surface),
                ))
                .id();

            press_interact(&mut app);
            assert_eq!(
                *app.world().resource::<State<GameState>>().get(),
                GameState::Looting
            );
            click(&mut app, LootAction::Close);
            app.update();
            assert_eq!(app.world().resource::<Inventory>().count(item), 0);

            press_interact(&mut app);
            // A full stack must leave the loot available for a later attempt.
            assert!(
                app.world_mut()
                    .resource_mut::<Inventory>()
                    .add(item, u32::MAX)
            );
            click(&mut app, LootAction::Take);
            assert!(
                app.world()
                    .get::<CorpseLoot>(corpse)
                    .expect("loot")
                    .stack
                    .is_some()
            );
            assert!(
                app.world_mut()
                    .resource_mut::<Inventory>()
                    .remove(item, u32::MAX)
            );

            click(&mut app, LootAction::Take);
            assert_eq!(app.world().resource::<Inventory>().count(item), 1);
            assert!(
                app.world()
                    .get::<CorpseLoot>(corpse)
                    .expect("corpse remains")
                    .stack
                    .is_none()
            );
            click(&mut app, LootAction::Close);
            app.update();
            press_interact(&mut app);
            assert!(app.world().resource::<Nearby>().0.is_none());
            assert_eq!(app.world().resource::<Inventory>().count(item), 1);
        }
    }

    #[test]
    fn one_press_performs_one_interaction_and_dialogue_dismissal_takes_precedence() {
        let mut app = test_app();
        let item = app
            .world_mut()
            .spawn((
                Pickup {
                    item: crate::inventory::ItemKind::Battery,
                    quantity: 2,
                },
                Transform::from_xyz(0.0, 0.0, -1.0),
            ))
            .id();
        app.world_mut()
            .spawn((Target::Keeper, Transform::from_xyz(0.0, 0.0, -2.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(crate::inventory::ItemKind::Battery),
            2
        );
        assert!(
            app.world()
                .resource::<QuestJournal>()
                .expects(QuestAction::TalkedToNemi)
        );
        assert!(app.world().get_entity(item).is_err());
        // Dismissal must not collect another nearby stack on the same key press.
        app.world_mut().resource_mut::<Dialogue>().text = Some("A conversation");
        app.world_mut().spawn((
            Pickup {
                item: crate::inventory::ItemKind::Battery,
                quantity: 1,
            },
            Transform::from_xyz(0.0, 0.0, -1.0),
        ));
        app.update();
        assert!(app.world().resource::<Dialogue>().text.is_none());
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(crate::inventory::ItemKind::Battery),
            2
        );
    }
    #[test]
    fn range_walls_and_inventory_state_block_collection() {
        let mut app = test_app();
        let item = app
            .world_mut()
            .spawn((
                Pickup {
                    item: crate::inventory::ItemKind::Scrap,
                    quantity: 1,
                },
                Transform::from_xyz(0.0, 0.0, -4.0),
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert!(app.world().resource::<Nearby>().0.is_none());
        app.world_mut()
            .get_mut::<Transform>(item)
            .expect(
                "an out-of-range pickup must remain spawned with its Transform after interaction",
            )
            .translation
            .z = -2.0;
        let wall = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(3.0, 3.0, 0.2),
                Transform::from_xyz(0.0, 0.0, -1.0),
            ))
            .id();
        // Let the physics query tree synchronize before pressing E.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        for _ in 0..3 {
            app.update();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert!(app.world().resource::<Nearby>().0.is_none());
        assert!(app.world().get_entity(item).is_ok());
        app.world_mut().despawn(wall);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Inventory);
        for _ in 0..3 {
            app.update();
        }
        assert!(app.world().get_entity(item).is_ok());
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(crate::inventory::ItemKind::Scrap),
            0
        );
    }
    #[test]
    fn opening_inventory_wins_over_same_frame_pickup() {
        let mut app = test_app();
        app.add_systems(PreUpdate, crate::ui::pause);
        app.world_mut().spawn((
            Pickup {
                item: crate::inventory::ItemKind::Scrap,
                quantity: 1,
            },
            Transform::from_xyz(0.0, 0.0, -1.0),
        ));
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyI);
        keys.press(KeyCode::KeyE);
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Inventory
        );
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(crate::inventory::ItemKind::Scrap),
            0
        );
    }
    #[test]
    fn drop_requires_support_and_cannot_pass_through_walls() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = test_app();
        let player = app.world_mut().spawn_empty().id();
        let transform = Transform::from_xyz(0.0, 0.905, 0.0);
        let check = move |spatial: SpatialQuery| {
            crate::inventory::find_drop_position(
                &spatial,
                player,
                &transform,
                crate::inventory::ItemKind::Battery,
            )
        };
        assert!(
            app.world_mut()
                .run_system_once(check)
                .expect(
                    "test_app must initialize the physics resources required to run the drop query"
                )
                .is_none()
        );
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(20.0, 1.0, 20.0),
            Transform::from_xyz(0.0, -0.5, 0.0),
        ));
        for _ in 0..3 {
            app.update();
        }
        let position = app
            .world_mut()
            .run_system_once(check)
            .expect("test_app must initialize the physics resources required to run the drop query")
            .expect("clear floor should allow dropping");
        assert!((position.y - 0.012).abs() < 0.001);
        for (position, size) in [
            (Vec3::new(0.0, 1.0, 0.65), Vec3::new(2.0, 3.0, 0.2)),
            (Vec3::new(0.0, 1.0, -0.65), Vec3::new(2.0, 3.0, 0.2)),
            (Vec3::new(0.65, 1.0, 0.0), Vec3::new(0.2, 3.0, 2.0)),
            (Vec3::new(-0.65, 1.0, 0.0), Vec3::new(0.2, 3.0, 2.0)),
        ] {
            app.world_mut().spawn((
                RigidBody::Static,
                Collider::cuboid(size.x, size.y, size.z),
                Transform::from_translation(position),
            ));
        }
        for _ in 0..3 {
            app.update();
        }
        assert!(
            app.world_mut()
                .run_system_once(check)
                .expect(
                    "test_app must initialize the physics resources required to run the drop query"
                )
                .is_none()
        );
    }
}
