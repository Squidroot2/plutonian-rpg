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
    for id in [EncounterId::RelayRoach, EncounterId::RelayArchiveHushcap] {
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
    for id in [EncounterId::RelayRoach, EncounterId::RelayArchiveHushcap] {
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
