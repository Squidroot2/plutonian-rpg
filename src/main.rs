#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![deny(clippy::perf)]
#![deny(clippy::unwrap_used)]
mod camera;
mod equipment;
mod equipment_smoke;
mod interaction;
mod inventory;
mod inventory_smoke;
mod inventory_ui;
mod loading;
mod player;
mod quest;
mod smoke;
mod spear;
mod spear_smoke;
mod ui;
mod world;
use bevy::prelude::*;

#[derive(States, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    Loading,
    Playing,
    Paused,
    Inventory,
    Failed,
}

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.008, 0.014, 0.027)))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.42, 0.65, 0.85),
            brightness: 220.0,
            ..default()
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "The Plutonian".into(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .add_plugins((
            avian3d::prelude::PhysicsPlugins::default(),
            loading::LoadingPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            camera::FollowCameraPlugin,
            quest::QuestPlugin,
            inventory::InventoryPlugin,
            equipment::EquipmentPlugin,
            spear::SpearPlugin,
            interaction::InteractionPlugin,
            inventory_ui::InventoryUiPlugin,
            ui::GameUiPlugin,
        ));
    let args: Vec<_> = std::env::args().skip(1).collect();
    let arena = args.iter().any(|arg| arg == "--smoke-test");
    let world = args.iter().any(|arg| arg == "--smoke-world");
    assert!(
        !(arena && world),
        "choose either --smoke-test or --smoke-world"
    );
    if arena || world {
        app.add_plugins(smoke::SmokePlugin {
            arena,
            fullscreen: args.iter().any(|arg| arg == "--smoke-fullscreen"),
        });
    }
    if arena {
        app.add_plugins((
            inventory_smoke::InventorySmokePlugin,
            equipment_smoke::EquipmentSmokePlugin,
            spear_smoke::SpearSmokePlugin,
        ));
    }
    app.run();
}
