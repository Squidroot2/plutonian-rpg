use std::time::Duration;

use avian3d::prelude::*;
use bevy::{
    asset::{AssetPlugin, io::AssetSourceBuilder},
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
        PhysicsPlugins::default(),
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
    app
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
