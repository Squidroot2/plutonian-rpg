use crate::{
    GameState,
    dungeon::{DungeonTarget, UseDungeonTarget},
    inventory::{Inventory, Pickup},
    player::Player,
    quest::{Dialogue, Quest},
};
use avian3d::prelude::*;
use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Keeper,
    Core,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionTarget {
    Quest(Target),
    Pickup(Pickup),
    Dungeon(DungeonTarget),
}
#[derive(Resource, Default, PartialEq, Eq)]
pub struct Nearby(pub Option<(Entity, InteractionTarget)>);
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractionSystems;

pub struct InteractionPlugin;
impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Nearby>().add_systems(
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
        ),
        Or<(With<Target>, With<Pickup>, With<DungeonTarget>)>,
    >,
    quest: Res<Quest>,
    dialogue: Res<Dialogue>,
    spatial: SpatialQuery,
    mut nearby: ResMut<Nearby>,
) {
    if dialogue.text.is_some() {
        nearby.set_if_neq(Nearby::default());
        return;
    }
    let Ok((player_entity, transform, player)) = players.single() else {
        nearby.set_if_neq(Nearby::default());
        return;
    };
    let mut selected = None;
    let origin = transform.translation + Vec3::Y * 0.45;
    let mut closest = 3.0;
    for (entity, transform, target, pickup, dungeon) in &targets {
        let (interaction, point) = if let Some(target) = target {
            if *target == Target::Core && *quest != Quest::FindCore {
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
    nearby: Res<Nearby>,
    mut dialogue: ResMut<Dialogue>,
    mut quest: ResMut<Quest>,
    mut inventory: ResMut<Inventory>,
    mut commands: Commands,
    pickups: Query<&Pickup>,
) {
    if !keys.just_pressed(KeyCode::KeyE) && !keys.just_pressed(KeyCode::Enter) {
        return;
    }
    if dialogue.text.take().is_some() {
        return;
    }
    if let Some((entity, target)) = nearby.0 {
        match target {
            InteractionTarget::Quest(target) => {
                let before = *quest;
                dialogue.text = Some(quest.interact(target, &mut inventory));
                if target == Target::Core
                    && before == Quest::FindCore
                    && *quest == Quest::ReturnCore
                {
                    commands.entity(entity).despawn();
                }
            }
            InteractionTarget::Pickup(_) if keys.just_pressed(KeyCode::KeyE) => {
                // Re-read the live component before transferring the stack.
                if let Ok(pickup) = pickups.get(entity) {
                    if inventory.add(pickup.item, pickup.quantity) {
                        commands.entity(entity).despawn();
                    }
                }
            }
            InteractionTarget::Dungeon(target) => commands.trigger(UseDungeonTarget(target)),
            _ => (),
        }
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
        .init_resource::<Quest>()
        .init_resource::<Dialogue>()
        .init_resource::<Inventory>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins(InteractionPlugin);
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
        assert_eq!(*app.world().resource::<Quest>(), Quest::MeetKeeper);
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
