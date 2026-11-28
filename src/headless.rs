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
        None,
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

#[test]
fn worm_nest_is_hidden_accessible_grounded_and_swarming() {
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
    step_until(&mut app, 1_000, |world| {
        world.actor_data::<Player>().grounded
    });

    assert_worm_nest_layout(app.world_mut());
    assert_worm_nest_swarm_and_reward(&mut app);
}

fn assert_worm_nest_layout(world: &mut World) {
    use bevy::ecs::system::RunSystemOnce;

    const NEST_WORM_COUNT: usize = 10;
    const NEST_PICKUP_COUNT: usize = 4;
    const MIN_TRAIL_DISTANCE_M: f32 = 25.0;
    const TRAIL_HALFWAY_X: f32 = -4.0;
    const TRAIL_WEST_WAYSTONE_X: f32 = -7.9;
    const GROUND_TOLERANCE_M: f32 = 0.03;

    world
        .run_system_once(
            |spatial: SpatialQuery,
             actors: Query<
                Entity,
                Or<(
                    With<crate::enemy::Enemy>,
                    With<Player>,
                    With<crate::nemi::Nemi>,
                )>,
            >,
             player: Single<(Entity, &Collider), With<Player>>,
             worms: Query<
                (Entity, &Transform, &Collider, &EncounterId),
                With<crate::worm::Worm>,
            >,
             pickups: Query<(&Transform, &crate::content::PlacedPickup)>| {
                let static_filter = SpatialQueryFilter::from_excluded_entities(actors.iter());
                let nest_worms: Vec<_> = worms
                    .iter()
                    .filter(|(_, _, _, id)| id.key().as_str().starts_with("worm_nest_"))
                    .collect();
                assert_eq!(nest_worms.len(), NEST_WORM_COUNT);
                for &(entity, transform, collider, id) in &nest_worms {
                    assert!(
                        (transform.translation.x - TRAIL_HALFWAY_X).abs() >= MIN_TRAIL_DISTANCE_M,
                        "nest worm at {:?} is too close to the core trail",
                        transform.translation
                    );
                    let origin = transform.translation + Vec3::Y;
                    let support = spatial
                        .cast_ray(origin, Dir3::NEG_Y, 2.0, true, &static_filter)
                        .expect("nest worm must rest on walkable shelf");
                    let offset = support.distance - 1.0;
                    assert!(
                        (offset - crate::worm::GROUND_OFFSET).abs() < GROUND_TOLERANCE_M,
                        "nest worm not grounded at {:?}",
                        transform.translation
                    );
                    let obstacles = spatial.shape_intersections(
                        collider,
                        transform.translation,
                        transform.rotation,
                        &SpatialQueryFilter::from_excluded_entities([entity, support.entity]),
                    );
                    assert!(
                        obstacles.is_empty(),
                        "nest worm overlaps collider at {:?}",
                        transform.translation
                    );
                    // Probe above the head as well as at mouth height: the rear-up and
                    // bloom make even a small gap in the terrain visible from a distance.
                    let mouth = transform.transform_point(Vec3::new(0.0, 0.05, 0.78));
                    assert_nest_hidden_from_nivalis(&spatial, &static_filter, mouth, id.key());
                    assert_nest_hidden_from_nivalis(
                        &spatial,
                        &static_filter,
                        mouth + Vec3::Y * 0.5,
                        id.key(),
                    );
                }

                let nest_pickups: Vec<_> = pickups
                    .iter()
                    .filter(|(_, placed)| placed.id.as_str().starts_with("worm_nest_"))
                    .collect();
                assert_eq!(nest_pickups.len(), NEST_PICKUP_COUNT);
                for (transform, placed) in nest_pickups {
                    assert_eq!(
                        crate::worm::surface_location(transform.translation),
                        Some("WORM NEST"),
                        "pickup {} must sit inside the worm nest",
                        placed.id
                    );
                    let support = spatial
                        .cast_ray(
                            transform.translation + Vec3::Y,
                            Dir3::NEG_Y,
                            3.0,
                            true,
                            &static_filter,
                        )
                        .expect("nest pickup must rest on the shelf");
                    assert!(support.distance < 1.5);
                    assert_nest_hidden_from_nivalis(
                        &spatial,
                        &static_filter,
                        transform.translation + Vec3::Y * 0.6,
                        placed.id,
                    );
                }

                for trail_x in [TRAIL_HALFWAY_X, TRAIL_WEST_WAYSTONE_X] {
                    let origin = Vec3::new(trail_x, 0.5, -62.0);
                    assert_eq!(crate::worm::surface_location(origin), None);
                    for nest_target in [Vec3::new(-48.0, 0.5, -67.0), Vec3::new(-54.0, 0.5, -66.5)]
                    {
                        let delta = nest_target - origin;
                        let direction = Dir3::new(delta).expect("nonzero sightline");
                        assert!(
                            spatial
                                .cast_ray(origin, direction, delta.length(), true, &static_filter)
                                .is_some(),
                            "nest interior must be screened from the core trail at {origin}"
                        );
                    }
                }

                assert_nest_approach(
                    &spatial,
                    &static_filter,
                    player.1,
                    nest_worms.iter().map(|(_, pose, _, _)| pose.translation),
                );
            },
        )
        .expect("worm nest layout probe must run");
}

fn assert_nest_approach(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    capsule: &Collider,
    worms: impl Iterator<Item = Vec3>,
) {
    let bend = Vec3::new(-35.5, 0.9, -55.5);
    let approach = [
        Vec3::new(-4.0, 1.35, -52.0),
        Vec3::new(-14.0, 0.9, -52.0),
        Vec3::new(-28.0, 0.9, -52.5),
        bend,
        Vec3::new(-39.0, 0.9, -61.0),
        Vec3::new(-45.0, 0.9, -64.5),
    ];
    for segment in approach.windows(2) {
        let delta = segment[1] - segment[0];
        let direction = Dir3::new(delta).expect("nonzero approach segment");
        assert!(
            spatial
                .cast_shape(
                    capsule,
                    segment[0],
                    Quat::IDENTITY,
                    direction,
                    &ShapeCastConfig::from_max_distance(delta.length()),
                    filter,
                )
                .is_none(),
            "approach segment {:?} -> {:?} must remain passable",
            segment[0],
            segment[1]
        );
    }

    let bend_view = bend + Vec3::Y * 1.6;
    let visible_worms = worms
        .filter(|&position| {
            let delta = position - bend_view;
            spatial
                .cast_ray(
                    bend_view,
                    Dir3::new(delta).expect("the bend is outside the nest"),
                    delta.length(),
                    true,
                    filter,
                )
                .is_none()
        })
        .count();
    assert!(
        visible_worms >= 3,
        "rounding the bend should reveal the pack"
    );
}

fn assert_nest_hidden_from_nivalis(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    target: Vec3,
    id: crate::content::ContentId,
) {
    // Sample views across the settlement, including its north exit.
    for ground in [
        Vec3::new(-14.0, -1.0, -5.0),
        Vec3::new(0.0, -1.0, -2.0),
        Vec3::new(16.0, -1.0, -8.0),
        Vec3::new(0.0, -0.8, -18.0),
        Vec3::new(20.0, -0.8, -18.0),
        Vec3::new(-16.0, -1.2, 10.0),
    ] {
        let player = ground + Vec3::Y * crate::player::PLAYER_GROUND_OFFSET;
        let behind = (player - target).with_y(0.0).normalize();
        for (distance, pitch) in [(5.5, 0.22_f32), (8.0, 1.15)] {
            let camera =
                player + Vec3::Y * 0.65 + distance * (behind * pitch.cos() + Vec3::Y * pitch.sin());
            let delta = target - camera;
            assert!(
                spatial
                    .cast_ray(
                        camera,
                        Dir3::new(delta).expect("nest is away from Nivalis"),
                        delta.length(),
                        true,
                        filter,
                    )
                    .is_some(),
                "{id} must be screened from the camera at {camera} looking from {ground}"
            );
        }
    }
}

fn assert_worm_nest_swarm_and_reward(app: &mut App) {
    const MIN_SIMULTANEOUS_PURSUERS: usize = 4;

    let world = app.world_mut();
    let nest_center = Vec3::new(-45.5, 0.9, -65.5);
    let player = world.player_entity();
    world
        .get_mut::<Transform>(player)
        .expect("player transform")
        .translation = nest_center;

    for _ in 0..10 {
        app.update();
    }

    let world = app.world_mut();
    let pursuing = world
        .query::<(&crate::enemy::MeleePursuit, &EncounterId)>()
        .iter(world)
        .filter(|(pursuit, id)| {
            id.key().as_str().starts_with("worm_nest_")
                && matches!(
                    pursuit.phase,
                    crate::enemy::MeleePhase::Pursuing | crate::enemy::MeleePhase::Windup(_)
                )
        })
        .count();
    assert!(
        pursuing >= MIN_SIMULTANEOUS_PURSUERS,
        "entering the nest must pull a pack of worms (only {pursuing} pursuing)"
    );

    let pike = ItemKind::parse("survey_pike").expect("authored survey pike");
    let items = world.resource::<crate::content::Items>().clone();
    let mut bag = Inventory::default();
    assert!(bag.add(pike, 1));
    let mut gear = crate::equipment::Equipment::default();
    assert!(gear.equip(&mut bag, pike, 5, &items).is_err());
    assert_eq!(gear.equip(&mut bag, pike, 10, &items), Ok(true));
}
