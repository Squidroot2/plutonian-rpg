#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![deny(clippy::perf)]
#![deny(clippy::unwrap_used)]
mod camera;
mod combat;
mod combat_audio;
mod combat_feedback;
mod controls;
mod dungeon;
mod enemy;
mod equipment;
mod interaction;
mod inventory;
mod inventory_ui;
mod loading;
mod loot_ui;
mod menu_ui;
mod nemi;
mod player;
mod practice_dummy;
mod quest;
mod quest_ui;
mod roach;
mod settings;
mod smoke;
mod spear;
mod storage;
mod storage_ui;
mod title;
mod ui;
mod world;
mod worm;
use bevy::window::WindowMode;
use bevy::{asset::io::AssetSourceBuilder, prelude::*};

#[derive(States, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    Title,
    Loading,
    Playing,
    Paused,
    Looting,
    Storage,
    Failed,
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let arena = args.iter().any(|arg| arg == "--smoke-test");
    let world = args.iter().any(|arg| arg == "--smoke-world");
    let nemi = args.iter().any(|arg| arg == "--smoke-nemi");
    assert!(
        [arena, world, nemi]
            .into_iter()
            .filter(|mode| *mode)
            .count()
            <= 1,
        "choose one smoke mode"
    );
    let mode = if args.iter().any(|arg| arg == "--windowed") {
        WindowMode::Windowed
    } else {
        settings::FULLSCREEN_MODE
    };

    let mut app = App::new();
    app.register_asset_source("data", AssetSourceBuilder::platform_default("data", None))
        .insert_resource(ClearColor(Color::srgb(0.008, 0.014, 0.027)))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.42, 0.65, 0.85),
            brightness: 220.0,
            ..default()
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "The Plutonian".into(),
                resolution: (1280, 800).into(),
                mode,
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .add_plugins((
            avian3d::prelude::PhysicsPlugins::default(),
            loading::LoadingPlugin,
            world::WorldPlugin,
            dungeon::DungeonPlugin,
            player::PlayerPlugin,
            camera::FollowCameraPlugin,
            quest::QuestPlugin,
            inventory::InventoryPlugin,
            equipment::EquipmentPlugin,
            practice_dummy::PracticeDummyPlugin,
            spear::SpearPlugin,
            worm::WormPlugin,
            interaction::InteractionPlugin,
        ))
        .add_plugins((
            inventory_ui::InventoryUiPlugin,
            title::TitlePlugin,
            ui::GameUiPlugin,
            quest_ui::QuestUiPlugin,
            menu_ui::PlayerMenuPlugin,
            storage::StoragePlugin,
            storage_ui::StorageUiPlugin,
        ))
        .add_plugins((
            combat::CombatPlugin,
            combat_feedback::CombatFeedbackPlugin,
            combat_audio::CombatAudioPlugin,
            settings::SettingsPlugin,
            enemy::EnemyPlugin,
            nemi::NemiPlugin,
            loot_ui::LootUiPlugin,
        ));
    if arena || world || nemi {
        app.add_plugins(smoke::SmokePlugin { arena, nemi });
    }
    app.run();
}
