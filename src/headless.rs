//! Headless app harnesses for checking gameplay logic without a window or renderer.
//! It also verifies that shipped GLB collision scenes create Avian colliders
//! and that bed-save rollback restores the shipped world's gameplay state.

use std::time::Duration;

use crate::{
    GameState,
    attributes::{AttributeId, AttributeRanks},
    combat::{Dead, Health, Hit},
    dungeon::{Area, DungeonProgress, DungeonTarget, UseDungeonTarget},
    experience::{Experience, ExperienceNotice},
    guard::{GuardPost, OutpostSecurity},
    interaction::CorpseLoot,
    inventory::{Inventory, ItemKind},
    level::Level,
    player::{CharacterProgress, Player, PlayerWorldExt},
    quest::{CONTAIN_THE_HUSHCAPS, OLD_RELAY, QuestAction, QuestJournal, QuestStatus},
    save::{Bed, EncounterId, RunSave, SaveAtBed},
    storage::{ChestId, ChestStorage},
    world::WorldLayout,
};
use avian3d::prelude::*;
use bevy::{
    asset::{AssetApp, AssetPlugin, io::AssetSourceBuilder},
    gltf::GltfPlugin,
    image::ImagePlugin,
    mesh::MeshPlugin,
    prelude::*,
    time::TimeUpdateStrategy,
    transform::TransformPlugin,
    world_serialization::WorldSerializationPlugin,
};

const STEP: Duration = Duration::from_millis(20);

fn app() -> App {
    let mut app = App::new();
    app.register_asset_source("data", AssetSourceBuilder::platform_default("data", None));
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        AssetPlugin::default(),
        ImagePlugin::default(),
        MeshPlugin,
        WorldSerializationPlugin,
        GltfPlugin::default(),
        bevy::input::InputPlugin,
        bevy::animation::AnimationPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
    app
}

fn game() -> App {
    let mut app = app();
    app.init_asset::<Font>()
        .init_asset::<StandardMaterial>()
        .init_asset::<bevy::shader::Shader>()
        .add_plugins(bevy::state::app::StatesPlugin)
        .insert_state(crate::GameState::Boot);
    // Audio playback needs an AudioOutput, which a headless app does not create.
    crate::add_game_plugins(
        &mut app,
        crate::save::SavePlugin {
            smoke: true,
            directory: None,
        },
        crate::CombatAudio::Disabled,
    );
    app.finish();
    app.cleanup();
    app
}

#[test]
fn game_boots_headlessly() {
    let mut app = game();
    step_until(&mut app, 10_000, |world| {
        *world.resource::<State<crate::GameState>>().get() == crate::GameState::Title
    });
    app.world_mut()
        .resource_mut::<NextState<crate::GameState>>()
        .set(crate::GameState::Loading);
    step_until(&mut app, 20_000, |world| {
        let state = *world.resource::<State<crate::GameState>>().get();
        assert_ne!(
            state,
            crate::GameState::Failed,
            "loading failed: {:?}",
            world.resource::<crate::loading::LoadStatus>().error
        );
        state == crate::GameState::Playing
    });
}

fn encounter(world: &mut World, wanted: EncounterId) -> Entity {
    world
        .query::<(Entity, &EncounterId)>()
        .iter(world)
        .find_map(|(entity, id)| (*id == wanted).then_some(entity))
        .expect("shipped encounter must be spawned")
}

#[test]
fn sleeping_six_days_replaces_killed_and_wounded_encounters_in_only_the_due_area() {
    let mut app = game();
    step_until(&mut app, 10_000, |world| {
        *world.resource::<State<GameState>>().get() == GameState::Title
    });
    app.world_mut()
        .resource_mut::<crate::player::NewPlayerName>()
        .0 = "Test Player".into();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Loading);
    step_until(&mut app, 20_000, |world| {
        *world.resource::<State<GameState>>().get() == GameState::Playing
    });
    let worm_id = EncounterId::Added(crate::content::ContentId::new("near_trail_worm"));
    let wounded_id = EncounterId::Added(crate::content::ContentId::new("far_trail_worm"));
    let relay_id = EncounterId::Added(crate::content::ContentId::new("relay_roach"));
    let world = app.world_mut();
    let worm = encounter(world, worm_id);
    let wounded = encounter(world, wounded_id);
    let relay = encounter(world, relay_id);
    world
        .get_mut::<Health>(wounded)
        .expect("worm health")
        .set_current(1);
    world
        .get_mut::<Transform>(wounded)
        .expect("worm pose")
        .translation
        .x += 3.0;
    world
        .get_mut::<Health>(relay)
        .expect("roach health")
        .set_current(1);
    let player = world.player_entity();
    world.write_message(Hit {
        source: player,
        target: worm,
        damage: u16::MAX,
        contact: None,
    });
    app.update();
    assert!(app.world().get::<Dead>(worm).is_some());
    let bed = {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<Bed>>()
            .single(world)
            .expect("bed")
    };
    for day in 1..=6 {
        app.world_mut().write_message(SaveAtBed(bed));
        app.update();
        if day < 6 {
            assert!(app.world().get::<Dead>(worm).is_some());
            assert_eq!(
                app.world()
                    .get::<Health>(wounded)
                    .expect("wounded worm")
                    .current(),
                1
            );
        }
    }
    let world = app.world_mut();
    assert!(world.get_entity(worm).is_err());
    let replacement = encounter(world, worm_id);
    let refreshed = encounter(world, wounded_id);
    assert!(
        (world
            .get::<Transform>(refreshed)
            .expect("renewed pose")
            .translation
            .x
            + 14.0)
            .abs()
            < 0.01
    );
    for entity in [replacement, refreshed] {
        let health = world.get::<Health>(entity).expect("renewed health");
        assert_eq!(health.current(), health.maximum());
        assert!(
            world
                .get::<CorpseLoot>(entity)
                .expect("renewed loot")
                .stack
                .is_some()
        );
    }
    assert_eq!(encounter(world, relay_id), relay);
    assert_eq!(
        world.get::<Health>(relay).expect("roach health").current(),
        1
    );
    world.write_message(Hit {
        source: player,
        target: replacement,
        damage: u16::MAX,
        contact: None,
    });
    app.update();
    assert_eq!(app.world().actor_data::<CharacterProgress>().level.get(), 2);
}

#[test]
fn bed_save_death_and_reload_restore_the_world() {
    let mut app = game();
    step_until(&mut app, 10_000, |world| {
        *world.resource::<State<GameState>>().get() == GameState::Title
    });
    app.world_mut()
        .resource_mut::<crate::player::NewPlayerName>()
        .0 = "Test Player".into();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Loading);
    step_until(&mut app, 20_000, |world| {
        *world.resource::<State<GameState>>().get() == GameState::Playing
    });
    prepare_saved_state(&mut app);
    let world = app.world_mut();
    assert_eq!(
        *world.resource::<State<GameState>>().get(),
        GameState::Playing
    );
    assert!(matches!(
        *world.resource::<NextState<GameState>>(),
        NextState::Unchanged
    ));
    assert_eq!(world.actor_data::<Player>().area, Area::Surface);
    assert!(world.get::<Dead>(world.player_entity()).is_none());
    let bed = world
        .query_filtered::<Entity, With<Bed>>()
        .single(world)
        .expect("outpost has a bed");
    world.write_message(SaveAtBed(bed));
    app.update();
    assert!(
        app.world().resource::<RunSave>().checkpoint().is_some(),
        "save notice: {:?}",
        app.world().resource::<crate::save::SaveNotice>().text
    );
    assert_eq!(app.world().resource::<crate::save::RespawnClock>().day(), 1);
    let old_player = app.world().player_entity();
    let world = app.world_mut();
    world.set_player_data((
        AttributeRanks::default().with_rank(AttributeId::Vitality, 8),
        Experience(90),
    ));
    assert!(
        world
            .actor_data_mut::<Inventory>()
            .remove(ItemKind::RoachMeat, 1)
    );
    assert!(
        world
            .actor_data_mut::<Inventory>()
            .remove(ItemKind::Mycelium, 1)
    );
    assert!(
        world
            .resource_mut::<ChestStorage>()
            .get_mut(ChestId::Lamp)
            .add(ItemKind::Battery, 1)
    );
    let nemi = world
        .query_filtered::<Entity, With<crate::nemi::Nemi>>()
        .single(world)
        .expect("outpost has Nemi");
    world.write_message(Hit {
        source: old_player,
        target: nemi,
        damage: 1,
        contact: None,
    });
    app.update();
    assert!(app.world().resource::<OutpostSecurity>().hostile);
    let world = app.world_mut();
    world.write_message(Hit {
        source: Entity::PLACEHOLDER,
        target: old_player,
        damage: u16::MAX,
        contact: None,
    });
    step_until(&mut app, 100, |world| {
        *world.resource::<State<GameState>>().get() == GameState::GameOver
    });
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Loading);
    step_until(&mut app, 20_000, |world| {
        *world.resource::<State<GameState>>().get() == GameState::Playing
    });
    step_until(&mut app, 1_000, |world| {
        world.actor_data::<Player>().grounded
    });
    assert_eq!(app.world().resource::<crate::save::RespawnClock>().day(), 1);
    assert_restored_state(app.world_mut(), old_player);
}

fn prepare_saved_state(app: &mut App) {
    let world = app.world_mut();
    for action in [
        QuestAction::TalkedToNemi,
        QuestAction::RecoveredCore,
        QuestAction::ReturnedCore,
        QuestAction::EnteredRelay,
    ] {
        world.resource_mut::<QuestJournal>().record(action);
    }
    world.actor_data_mut::<Player>().area = Area::OldRelay;
    world.trigger(UseDungeonTarget(DungeonTarget::Switch));
    step_until(app, 100, |world| {
        world.resource::<DungeonProgress>().gate_open()
    });
    let world = app.world_mut();
    world
        .resource_mut::<QuestJournal>()
        .record(QuestAction::ReadSurveyLog);
    world.resource_mut::<QuestJournal>().offer_cavern_report();
    world
        .resource_mut::<QuestJournal>()
        .record(QuestAction::ReportedArchiveHushcap);
    world.resource_mut::<QuestJournal>().take_completion_xp();
    world.actor_data_mut::<Player>().area = Area::Surface;
    for id in [
        EncounterId::Added(crate::content::ContentId::new("relay_roach")),
        EncounterId::Added(crate::content::ContentId::new("relay_archive_hushcap")),
    ] {
        let entity = encounter(world, id);
        world
            .get_mut::<Health>(entity)
            .expect("encounter has health")
            .set_current(0);
        world
            .get_mut::<CorpseLoot>(entity)
            .expect("encounter has loot")
            .stack = None;
        world.entity_mut(entity).insert(Dead).remove::<Collider>();
    }
    world.set_player_data((
        AttributeRanks::default().with_rank(AttributeId::Vitality, 7),
        CharacterProgress {
            level: Level::new(3).expect("authored level is valid"),
            unspent: 6,
        },
        Experience(70),
    ));
    assert!(
        world
            .actor_data_mut::<Inventory>()
            .add(ItemKind::RoachMeat, 1)
    );
    assert!(
        world
            .actor_data_mut::<Inventory>()
            .add(ItemKind::Mycelium, 1)
    );
    let chest = world
        .resource_mut::<ChestStorage>()
        .get_mut(ChestId::Lamp)
        .count(ItemKind::Battery);
    if chest == 0 {
        assert!(
            world
                .resource_mut::<ChestStorage>()
                .get_mut(ChestId::Lamp)
                .add(ItemKind::Battery, 1)
        );
    }
    if chest > 1 {
        assert!(
            world
                .resource_mut::<ChestStorage>()
                .get_mut(ChestId::Lamp)
                .remove(ItemKind::Battery, chest - 1)
        );
    }
    app.update();
}

fn assert_restored_state(world: &mut World, old_player: Entity) {
    assert_ne!(world.player_entity(), old_player);
    assert!(
        world
            .actor_data::<Position>()
            .0
            .distance(WorldLayout::Outpost.initial_spawn())
            < 0.04
    );
    assert_eq!(
        world
            .actor_data::<AttributeRanks>()
            .rank(AttributeId::Vitality),
        7
    );
    assert_eq!(*world.actor_data::<Experience>(), Experience(70));
    assert_eq!(
        *world.actor_data::<CharacterProgress>(),
        CharacterProgress {
            level: Level::new(3).expect("authored level is valid"),
            unspent: 6
        }
    );
    assert_eq!(
        (
            world.actor_data::<Health>().current(),
            world.actor_data::<Health>().maximum()
        ),
        (108, 108)
    );
    assert_eq!(
        world.actor_data::<Inventory>().count(ItemKind::RoachMeat),
        1
    );
    assert_eq!(world.actor_data::<Inventory>().count(ItemKind::Mycelium), 1);
    assert_eq!(
        world
            .resource::<ChestStorage>()
            .get(ChestId::Lamp)
            .count(ItemKind::Battery),
        1
    );
    assert!(!world.resource::<OutpostSecurity>().hostile);
    assert!(world.resource::<ExperienceNotice>().text.is_none());
    let quest = world.resource::<QuestJournal>();
    assert!(quest.is_complete(OLD_RELAY));
    assert_eq!(
        quest.status(CONTAIN_THE_HUSHCAPS),
        Some(QuestStatus::Active)
    );
    assert!(quest.has_occurred(QuestAction::ReportedArchiveHushcap));
    assert!(!quest.has_occurred(QuestAction::AttackedNemi));
    assert!(world.resource::<DungeonProgress>().gate_open());
    for id in [
        EncounterId::Added(crate::content::ContentId::new("relay_roach")),
        EncounterId::Added(crate::content::ContentId::new("relay_archive_hushcap")),
    ] {
        let entity = encounter(world, id);
        assert_eq!(
            world
                .get::<Health>(entity)
                .expect("encounter has health")
                .current(),
            0
        );
        assert!(world.get::<Dead>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_none());
        assert!(
            world
                .get::<CorpseLoot>(entity)
                .expect("encounter has loot")
                .stack
                .is_none()
        );
    }
    let guards: Vec<_> = world
        .query::<(Entity, &GuardPost, &Transform, &Health)>()
        .iter(world)
        .map(|(entity, post, pose, health)| (entity, *post, *pose, health.current()))
        .collect();
    assert_eq!(guards.len(), 2);
    for (_, post, pose, health) in &guards {
        assert_eq!(*health, 140);
        assert!(pose.translation.xz().distance(post.home().xz()) < 0.05);
        assert!(
            pose.translation
                .xz()
                .distance(WorldLayout::Outpost.keeper().xz())
                >= 7.0
        );
        assert!(pose.rotation.angle_between(post.facing()).abs() < 0.01);
    }
    use bevy::ecs::system::RunSystemOnce;
    world
        .run_system_once(
            move |spatial: SpatialQuery,
                  player: Single<(Entity, &Collider), With<Player>>,
                  children: Query<&Children>,
                  worn: Query<&crate::equipment::WornEquipment>| {
                let (entity, capsule) = *player;
                assert!(
                    spatial
                        .cast_shape(
                            capsule,
                            crate::dungeon::ORIGIN + Vec3::new(0.0, 0.91, -16.1),
                            Quat::IDENTITY,
                            Dir3::NEG_Z,
                            &ShapeCastConfig::from_max_distance(3.4),
                            &SpatialQueryFilter::from_excluded_entities([entity])
                        )
                        .is_none(),
                    "restored gate must be clear"
                );
                for (entity, _, pose, _) in &guards {
                    let floor = spatial
                        .cast_ray(
                            pose.translation,
                            Dir3::NEG_Y,
                            2.0,
                            true,
                            &SpatialQueryFilter::from_excluded_entities([*entity]),
                        )
                        .expect("guard post has ice below it");
                    assert!((floor.distance - crate::player::PLAYER_GROUND_OFFSET).abs() < 0.05);
                    let items: Vec<_> = children
                        .iter_descendants(*entity)
                        .filter_map(|child| worn.get(child).ok().map(|piece| piece.item))
                        .collect();
                    assert_eq!(items.len(), 3);
                    assert!(items.contains(&ItemKind::ReinforcedArmor));
                    assert!(items.contains(&ItemKind::ReinforcedHelmet));
                    assert!(items.iter().any(|id| id.as_str() == "guard_polearm"));
                }
            },
        )
        .expect("gate probe system must run");
}

fn step_until(app: &mut App, budget: usize, condition: impl Fn(&mut World) -> bool) {
    for _ in 0..budget {
        app.update();
        if condition(app.world_mut()) {
            return;
        }
        std::thread::yield_now();
    }
    panic!("condition did not hold within {budget} steps");
}

#[test]
fn south_basin_guide_mushroom_is_visible_from_village_and_leads_to_cave_mouth() {
    use bevy::ecs::system::RunSystemOnce;

    let mut app = game();
    step_until(&mut app, 10_000, |world| {
        *world.resource::<State<GameState>>().get() == GameState::Title
    });
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Loading);
    step_until(&mut app, 20_000, |world| {
        *world.resource::<State<GameState>>().get() == GameState::Playing
    });
    app.world_mut()
        .run_system_once(
            |spatial: SpatialQuery,
             actors: Query<Entity, Or<(With<crate::enemy::Enemy>, With<Player>, With<crate::nemi::Nemi>)>>,
             names: Query<&Name>,
             parents: Query<&ChildOf>| {
                let filter = SpatialQueryFilter::from_excluded_entities(actors.iter());
                let cap = crate::cave::GUIDE_MUSHROOM_CAP_POS;
                let cave_mouth = Vec3::new(14.0, 0.0, 84.0);
                let village_views = [
                    ("Old Relay hatch", crate::dungeon::HATCH + Vec3::Y * 1.2),
                    ("Nemi", WorldLayout::Outpost.keeper() + Vec3::Y * 2.0),
                    ("Outpost start", WorldLayout::Outpost.start() + Vec3::Y * 0.8),
                    ("South courtyard", Vec3::new(0.0, 1.0, 16.0)),
                ];
                for (label, origin) in village_views {
                    let to_cap = cap - origin;
                    assert!(
                        spatial
                            .cast_ray(
                                origin,
                                Dir3::new(to_cap).expect("mushroom is away from village"),
                                to_cap.length(),
                                true,
                                &filter,
                            )
                            .is_none(),
                        "guide mushroom must be visible from {label} ({origin})"
                    );
                    let to_mouth = cave_mouth - origin;
                    assert!(
                        spatial
                            .cast_ray(
                                origin,
                                Dir3::new(to_mouth).expect("cave mouth is away from village"),
                                to_mouth.length(),
                                true,
                                &filter,
                            )
                            .is_some(),
                        "cave mouth must stay concealed behind the ridge from {label} ({origin})"
                    );
                }

                // Confirm the mushroom stalk has a matching static collider at its base.
                let stalk_probe = Vec3::new(-13.2, -1.0, 72.5);
                let hit = spatial
                    .cast_ray(stalk_probe, Dir3::X, 2.5, true, &filter)
                    .expect("guide mushroom stalk must have a collider");
                assert!(
                    std::iter::once(hit.entity)
                        .chain(parents.iter_ancestors(hit.entity))
                        .any(|entity| {
                            names
                                .get(entity)
                                .is_ok_and(|name| name.as_str() == crate::cave::GUIDE_MUSHROOM_STALK_NAME)
                        }),
                    "probe into mushroom trunk must hit the guide mushroom stalk collider"
                );

                // Rounding the bend past the guide mushroom reveals the cave mouth.
                let bend_view = Vec3::new(-13.0, 0.0, 76.0);
                let to_mouth = cave_mouth - bend_view;
                assert!(
                    spatial
                        .cast_ray(
                            bend_view,
                            Dir3::new(to_mouth).expect("cave mouth is across the bend"),
                            to_mouth.length(),
                            true,
                            &filter,
                        )
                        .is_none(),
                    "cave mouth must be visible once the player rounds the bend past the guide mushroom"
                );
            },
        )
        .expect("guide mushroom sightline check must run");
}

#[test]
fn shipped_collision_scene_builds_colliders_without_a_renderer() {
    let mut app = app();
    app.add_plugins(PhysicsPlugins::default());
    app.finish();
    app.cleanup();
    let scene: Handle<WorldAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("models/collision.glb#Scene0");
    app.world_mut().spawn((
        WorldAssetRoot(scene.clone()),
        RigidBody::Static,
        ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMesh),
    ));
    step_until(&mut app, 10_000, |world| {
        if let bevy::asset::LoadState::Failed(error) =
            world.resource::<AssetServer>().load_state(&scene)
        {
            panic!("scene load failed: {error}");
        }
        world.query::<&Collider>().iter(world).next().is_some()
    });
}
