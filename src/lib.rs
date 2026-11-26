#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![deny(clippy::perf)]
#![deny(clippy::too_many_lines)]
#![deny(clippy::unwrap_used)]
mod attributes;
mod camera;
mod cave;
mod combat;
mod combat_audio;
mod combat_feedback;
mod content;
mod controls;
mod crossing;
mod dialogue;
mod dungeon;
mod enemy;
mod equipment;
mod experience;
mod guard;
mod hushcap;
mod interaction;
mod inventory;
mod loading;
mod nemi;
mod player;
mod practice_dummy;
mod quest;
mod roach;
mod save;
/// Inspect and create current-version saves.
pub mod save_inspector;
#[cfg(feature = "smoke")]
mod smoke;
mod storage;
mod terrain;
mod ui;
mod weapon;
mod world;
mod worm;
use bevy::window::WindowMode;
use bevy::{asset::io::AssetSourceBuilder, prelude::*};

#[derive(States, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GameState {
    #[default]
    Title,
    Boot,
    Loading,
    Playing,
    GameOver,
    Paused,
    Looting,
    Storage,
    Failed,
}

/// Process options needed to construct the game application.
#[derive(Default)]
pub struct Options {
    pub arena: bool,
    pub world: bool,
    pub nemi: bool,
    pub cave: bool,
    pub crossing: bool,
    pub windowed: bool,
    /// Optional absolute save directory supplied at launch.
    pub save_dir: Option<std::path::PathBuf>,
}

/// Construct and run the game with the selected process options.
pub fn run(options: Options) {
    let Options {
        arena,
        world,
        nemi,
        cave,
        crossing,
        windowed,
        save_dir,
    } = options;
    let mode = if windowed {
        WindowMode::Windowed
    } else {
        ui::settings::FULLSCREEN_MODE
    };

    let mut app = App::new();
    app.register_asset_source("data", AssetSourceBuilder::platform_default("data", None))
        .insert_resource(ClearColor(Color::srgb(0.008, 0.014, 0.027)))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.28, 0.38, 0.55),
            brightness: 80.0,
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
        .insert_state(GameState::Boot)
        .add_plugins(content::ContentPlugin)
        .add_plugins((
            avian3d::prelude::PhysicsPlugins::default(),
            loading::LoadingPlugin,
            world::WorldPlugin,
            dungeon::DungeonPlugin,
            player::PlayerPlugin,
            camera::FollowCameraPlugin,
            quest::QuestPlugin,
            dialogue::DialoguePlugin,
            inventory::InventoryPlugin,
            equipment::EquipmentPlugin,
            practice_dummy::PracticeDummyPlugin,
            weapon::WeaponPlugin,
            worm::WormPlugin,
            interaction::InteractionPlugin,
        ))
        .add_plugins((
            ui::inventory_ui::InventoryUiPlugin,
            ui::title::TitlePlugin,
            ui::GameUiPlugin,
            ui::quest_ui::QuestUiPlugin,
            ui::menu_ui::PlayerMenuPlugin,
            storage::StoragePlugin,
            ui::storage_ui::StorageUiPlugin,
            save::SavePlugin {
                smoke: arena || world || nemi || cave || crossing,
                directory: save_dir,
            },
        ))
        .add_plugins((
            combat::CombatPlugin,
            combat_feedback::CombatFeedbackPlugin,
            combat_audio::CombatAudioPlugin,
            ui::settings::SettingsPlugin,
            enemy::EnemyPlugin,
            attributes::AttributesPlugin,
            experience::ExperiencePlugin,
            nemi::NemiPlugin,
            guard::GuardPlugin,
            ui::loot_ui::LootUiPlugin,
            cave::CavePlugin,
            crossing::CrossingPlugin,
        ));
    #[cfg(feature = "smoke")]
    if arena || world || nemi || cave || crossing {
        app.add_plugins(smoke::SmokePlugin {
            arena,
            nemi,
            cave,
            crossing,
        });
    }
    app.run();
}
