#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![deny(clippy::perf)]
#![deny(clippy::too_many_lines)]
#![deny(clippy::unwrap_used)]
/// Headless validation rules for checked-in models.
pub mod asset_check;
mod attributes;
mod camera;
mod cave;
mod combat;
mod combat_audio;
mod combat_feedback;
mod config;
mod content;
pub use content::validate_data_root;
mod controls;
mod crossing;
mod dialogue;
mod dungeon;
mod enemy;
mod equipment;
mod experience;
mod guard;
#[cfg(test)]
mod headless;
mod hushcap;
mod interaction;
mod inventory;
mod level;
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
    /// Skip screenshot captures during the selected smoke scenario.
    pub no_screenshots: bool,
    pub windowed: bool,
    /// Optional absolute save directory supplied at launch.
    pub save_dir: Option<std::path::PathBuf>,
    /// Optional absolute config directory supplied at launch.
    pub config_dir: Option<std::path::PathBuf>,
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
        config_dir,
        ..
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
        .insert_state(GameState::Boot);
    add_game_plugins(
        &mut app,
        save::SavePlugin {
            smoke: arena || world || nemi || cave || crossing,
            directory: save_dir,
        },
        config_dir,
        CombatAudio::Enabled,
    );
    #[cfg(feature = "smoke")]
    if arena || world || nemi || cave || crossing {
        app.add_plugins(smoke::SmokePlugin {
            arena,
            nemi,
            cave,
            crossing,
            no_screenshots: options.no_screenshots,
        });
    }
    app.run();
}

/// Register the game features in the same order for desktop and headless apps.
fn add_game_plugins(
    app: &mut App,
    save_plugin: save::SavePlugin,
    config_dir: Option<std::path::PathBuf>,
    combat_audio: CombatAudio,
) {
    let settings_smoke = save_plugin.smoke;
    app.add_plugins(content::ContentPlugin)
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
            save_plugin,
        ))
        .add_plugins((
            combat::CombatPlugin,
            combat_feedback::CombatFeedbackPlugin,
            move |app: &mut App| {
                if matches!(combat_audio, CombatAudio::Enabled) {
                    app.add_plugins(combat_audio::CombatAudioPlugin);
                }
            },
            ui::settings::SettingsPlugin {
                smoke: settings_smoke,
                directory: config_dir,
            },
            enemy::EnemyPlugin,
            attributes::AttributesPlugin,
            experience::ExperiencePlugin,
            nemi::NemiPlugin,
            guard::GuardPlugin,
            ui::loot_ui::LootUiPlugin,
            cave::CavePlugin,
            crossing::CrossingPlugin,
        ));
}

#[derive(Clone, Copy)]
enum CombatAudio {
    Enabled,
    #[cfg(test)]
    Disabled,
}
