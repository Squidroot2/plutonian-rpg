mod recipe;
mod scene;

use crate::{
    add_desktop_rendering,
    camera::world_camera,
    loading::PendingScene,
    world::{WorldLayout, configure_sky_meshes, spawn_lights, spawn_outpost_lights},
};
use bevy::{
    asset::io::AssetSourceBuilder,
    prelude::*,
    render::{
        RenderPlugin,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
};
use recipe::{Mount, View};
use scene::{LoadQueue, spawn_loaded_scenes};
use std::{ffi::OsString, fmt::Display, path::PathBuf};

const CAPTURE_TIMEOUT_SECS: f32 = 90.0;
// Let the sampled poses and mounted meshes go through animation, propagation, and rendering.
const SETTLE_FRAMES: u8 = 3;

#[derive(Resource)]
struct Output(PathBuf);

/// Render and save one equipment shot using the desktop renderer without starting a game session.
pub fn run(args: impl IntoIterator<Item = OsString>) -> Result<AppExit, String> {
    let mut args = args.into_iter().peekable();
    if args
        .peek()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!("{}", recipe::USAGE);
        return Ok(AppExit::Success);
    }
    let (model, shot, output) = recipe::select(args)?;
    let view = shot.view.unwrap_or(match shot.mount {
        Mount::Rack => View::Rack,
        Mount::Socket(_) => View::Hand,
    });

    let mut app = App::new();
    app.register_asset_source(
        recipe::LOCAL_FILES,
        AssetSourceBuilder::platform_default("/", None),
    );
    add_desktop_rendering(
        &mut app,
        Window {
            title: "The Plutonian — Art preview".into(),
            resolution: (1280, 736).into(),
            ..default()
        },
        RenderPlugin {
            // The first captured frame must contain materials that were just seen by the renderer.
            synchronous_pipeline_compilation: true,
            ..default()
        },
    );
    let queue = LoadQueue::new(app.world().resource::<AssetServer>(), &model, shot)?;
    app.insert_resource(queue)
        .insert_resource(Output(output))
        .add_systems(Startup, move |mut commands: Commands| {
            let (transform, projection) = view.camera();
            commands.spawn((world_camera(), transform, projection));
            spawn_outpost_lights(&mut commands);
            spawn_lights(&mut commands, WorldLayout::Outpost);
        })
        .add_systems(
            Update,
            (spawn_loaded_scenes, capture_when_ready, capture_timeout).chain(),
        )
        .add_systems(Update, configure_sky_meshes);
    Ok(app.run())
}

fn capture_when_ready(
    queue: Res<LoadQueue>,
    pending: Query<(), With<PendingScene>>,
    mut settled: Local<u8>,
    mut commands: Commands,
) {
    if !queue.is_empty() || !pending.is_empty() || *settled >= SETTLE_FRAMES {
        return;
    }
    *settled += 1;
    if *settled == SETTLE_FRAMES {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_capture);
    }
}

fn save_capture(
    capture: On<ScreenshotCaptured>,
    output: Res<Output>,
    mut exit: MessageWriter<AppExit>,
) {
    let path = &output.0;
    let result = capture
        .image
        .clone()
        .try_into_dynamic()
        .map_err(|error| error.to_string())
        .and_then(|image| {
            image
                .into_rgb8()
                .save(path)
                .map_err(|error| error.to_string())
        });
    match result {
        Ok(()) => {
            info!("Screenshot saved to {}", path.display());
            exit.write(AppExit::Success);
        }
        Err(error) => fail(
            &mut exit,
            format_args!("Could not save {}: {error}", path.display()),
        ),
    }
}

fn capture_timeout(time: Res<Time<Real>>, mut exit: MessageWriter<AppExit>) {
    if time.elapsed_secs() >= CAPTURE_TIMEOUT_SECS {
        fail(
            &mut exit,
            "Art preview timed out waiting for the screenshot",
        );
    }
}

fn fail(exit: &mut MessageWriter<AppExit>, error: impl Display) {
    error!("{error}");
    exit.write(AppExit::error());
}
