use crate::{GameState, inventory::ItemKind, player::AnimationLibrary};
use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    gltf::Gltf,
    prelude::*,
};
use std::collections::BTreeMap;

pub struct LoadingPlugin;
impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadStatus>()
            .add_systems(Startup, load)
            .add_systems(Update, check.run_if(in_state(GameState::Loading)));
    }
}
#[derive(Resource)]
pub struct GameAssets {
    pub character: Handle<Gltf>,
    pub outpost: Handle<Gltf>,
    pub collision: Handle<Gltf>,
    pub core: Handle<Gltf>,
    pub pickups: BTreeMap<ItemKind, Handle<Gltf>>,
    pub worn: BTreeMap<ItemKind, Handle<Gltf>>,
}
#[derive(Component)]
pub struct PendingScene;

#[derive(Resource, Default)]
pub struct LoadStatus {
    pub error: Option<String>,
    pub spawning: bool,
    pub collision_ready: bool,
    pub started: Option<std::time::Instant>,
}
fn load(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(GameAssets {
        character: server.load("models/plutonian.glb"),
        outpost: server.load("models/outpost.glb"),
        collision: server.load("models/collision.glb"),
        core: server.load("models/core.glb"),
        pickups: ItemKind::MATERIALS
            .into_iter()
            .chain(ItemKind::GEAR)
            .map(|item| (item, server.load(format!("models/{}.glb", item.asset()))))
            .collect(),
        worn: ItemKind::GEAR
            .into_iter()
            .map(|item| {
                (
                    item,
                    server.load(format!("models/{}_worn.glb", item.asset())),
                )
            })
            .collect(),
    });
}
fn check(
    mut commands: Commands,
    server: Res<AssetServer>,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut status: ResMut<LoadStatus>,
    mut next: ResMut<NextState<GameState>>,
    pending: Query<(), With<PendingScene>>,
    colliders: Query<&avian3d::prelude::Collider, Without<crate::player::Player>>,
) {
    let started = *status.started.get_or_insert_with(std::time::Instant::now);
    if started.elapsed().as_secs() > 90 {
        status.error = Some("Timed out while loading or preparing the world".into());
    }
    let handles = [
        &assets.character,
        &assets.outpost,
        &assets.collision,
        &assets.core,
    ]
    .into_iter()
    .chain(assets.pickups.values())
    .chain(assets.worn.values())
    .collect::<Vec<_>>();
    for handle in &handles {
        let handle = *handle;

        if let Some(LoadState::Failed(error)) = server.get_load_state(handle.id()) {
            status.error = Some(format!("Could not load game assets: {error}"));
        }
        if let Some(RecursiveDependencyLoadState::Failed(error)) =
            server.get_recursive_dependency_load_state(handle.id())
        {
            status.error = Some(format!("Could not load an asset dependency: {error}"));
        }
    }
    if status.error.is_some() {
        next.set(GameState::Failed);
        return;
    }
    if status.spawning {
        // Scene-ready events alone do not guarantee physics constructors have finished.
        if pending.is_empty() && status.collision_ready && colliders.iter().count() > 1 {
            next.set(GameState::Playing);
        }
        return;
    }
    if !handles
        .iter()
        .all(|h| server.is_loaded_with_dependencies(h.id()))
    {
        return;
    }
    if handles.iter().any(|handle| {
        gltfs
            .get(*handle)
            .is_none_or(|model| model.scenes.is_empty())
    }) {
        status.error = Some("A game model is missing its scene".into());
        next.set(GameState::Failed);
        return;
    }
    let model = gltfs.get(&assets.character).expect(
        "the character glTF must be present after all game models passed loading validation",
    );
    for bone in ["spine", "head"] {
        if !model.named_nodes.contains_key(bone) {
            status.error = Some(format!(
                "The player rig is missing equipment attachment bone {bone}"
            ));
            next.set(GameState::Failed);
            return;
        }
    }
    let mut graph = AnimationGraph::new();
    let mut nodes = Vec::new();
    for name in ["Idle", "Walk", "Run", "Jump"] {
        let Some(clip) = model.named_animations.get(name) else {
            status.error = Some(format!("plutonian.glb is missing animation {name}"));
            next.set(GameState::Failed);
            return;
        };
        nodes.push(graph.add_clip(clip.clone(), 1.0, graph.root));
    }
    commands.insert_resource(AnimationLibrary {
        graph: graphs.add(graph),
        nodes: nodes
            .try_into()
            .expect("Idle, Walk, Run, and Jump must produce exactly four animation graph nodes"),
    });
    status.spawning = true;
    commands.trigger(crate::world::SpawnWorld);
}
