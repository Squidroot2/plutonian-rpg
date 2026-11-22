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
mod controls;
mod dungeon;
mod enemy;
mod equipment;
mod experience;
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
    let cave = args.iter().any(|arg| arg == "--smoke-cave");
    assert!(
        [arena, world, nemi, cave]
            .into_iter()
            .filter(|mode| *mode)
            .count()
            <= 1,
        "choose one smoke mode"
    );
    let mode = if args.iter().any(|arg| arg == "--windowed") {
        WindowMode::Windowed
    } else {
        ui::settings::FULLSCREEN_MODE
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
                smoke: arena || world || nemi || cave,
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
            ui::loot_ui::LootUiPlugin,
            cave::CavePlugin,
        ));
    if arena || world || nemi || cave {
        app.add_plugins(smoke::SmokePlugin { arena, nemi, cave });
    }
    app.run();
}
