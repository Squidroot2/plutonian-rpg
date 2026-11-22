use crate::{
    GameState,
    enemy::{EnemyAnimations, EnemyModel, EnemyVisual},
    inventory::ItemKind,
    player::{AnimationLibrary, CLIPS, add_sword_animation_masks},
    quest::{QuestCatalog, QuestJournal},
    roach,
    save::{self, RunSave, SaveSlot},
    world::WorldLayout,
    worm,
};
use avian3d::prelude::{Collider, SpatialQuery, SpatialQueryFilter};
use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    gltf::Gltf,
    prelude::*,
    world_serialization::WorldInstanceReady,
};
use std::{collections::BTreeMap, time::Instant};

const LOADING_TIMEOUT_SECONDS: u64 = 90;
const ARENA_FLOOR_PROBE_ORIGIN: Vec3 = Vec3::new(0.0, 5.0, -12.0);
const ARENA_FLOOR_PROBE_RANGE: f32 = 6.0;
const MIN_NON_PLAYER_COLLIDERS: usize = 2;

const ENEMY_MODELS: &[EnemyModel] = &[worm::MODEL, roach::MODEL];

#[derive(Resource)]
pub(crate) struct EnemyVisuals(BTreeMap<&'static str, EnemyVisual>);

impl EnemyVisuals {
    pub(crate) fn get(&self, model: &EnemyModel) -> &EnemyVisual {
        self.0
            .get(model.path)
            .expect("enemy visuals must be prepared during loading")
    }
}

pub struct LoadingPlugin;
impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadStatus>()
            .add_systems(OnEnter(GameState::Loading), load.after(save::reset_session))
            .add_systems(Update, check.run_if(in_state(GameState::Loading)));
    }
}
#[derive(Resource)]
pub struct GameAssets {
    quests: Handle<QuestCatalog>,
    pub character: Handle<Gltf>,
    pub outpost: Option<(Handle<Gltf>, Handle<Gltf>)>,
    pub relay: Option<(Handle<Gltf>, Handle<Gltf>)>,
    pub core: Handle<Gltf>,
    pub dummy: Handle<Gltf>,
    enemies: BTreeMap<&'static str, Handle<Gltf>>,
    pub pickups: BTreeMap<ItemKind, Handle<Gltf>>,
    pub worn: BTreeMap<ItemKind, Handle<Gltf>>,
}
impl GameAssets {
    fn model_handles(&self) -> impl Iterator<Item = &Handle<Gltf>> + Clone {
        [&self.character, &self.core, &self.dummy]
            .into_iter()
            .chain(
                self.outpost
                    .iter()
                    .chain(self.relay.iter())
                    .flat_map(|(visuals, collision)| [visuals, collision]),
            )
            .chain(self.enemies.values())
            .chain(self.pickups.values())
            .chain(self.worn.values())
    }
}

/// A scene root whose instantiated entities are still needed before play can begin.
#[derive(Component)]
pub struct PendingScene;

pub(crate) fn scene_ready(event: On<WorldInstanceReady>, mut commands: Commands) {
    commands.entity(event.entity).remove::<PendingScene>();
}

#[derive(Resource, Default)]
pub struct LoadStatus {
    pub error: Option<String>,
    pub spawning: bool,
    pub collision_ready: bool,
    pub relay_collision_ready: bool,
    pub started: Option<Instant>,
}
fn load(
    mut commands: Commands,
    server: Res<AssetServer>,
    layout: Res<WorldLayout>,
    mut status: ResMut<LoadStatus>,
) {
    *status = LoadStatus::default();
    commands.insert_resource(GameAssets {
        quests: server.load("data://quests/catalog.quest.ron"),
        character: server.load("models/plutonian.glb"),
        outpost: (*layout == WorldLayout::Outpost).then(|| {
            (
                server.load("models/outpost.glb"),
                server.load("models/collision.glb"),
            )
        }),
        relay: (*layout == WorldLayout::Outpost).then(|| {
            (
                server.load("models/old_relay.glb"),
                server.load("models/old_relay_collision.glb"),
            )
        }),
        core: server.load("models/core.glb"),
        dummy: server.load("models/practice_dummy.glb"),
        enemies: ENEMY_MODELS
            .iter()
            .map(|model| (model.path, server.load(model.path)))
            .collect(),
        pickups: ItemKind::ALL
            .into_iter()
            .filter(|&item| item != ItemKind::BeaconCore)
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
    catalogs: Res<Assets<QuestCatalog>>,
    layout: Res<WorldLayout>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    clips: Res<Assets<AnimationClip>>,
    mut status: ResMut<LoadStatus>,
    mut next: ResMut<NextState<GameState>>,
    pending: Query<(), With<PendingScene>>,
    spatial: SpatialQuery,
    arena_floor: Query<Entity, With<crate::smoke::ArenaFloor>>,
    colliders: Query<&Collider, Without<crate::player::Player>>,
    mut run: ResMut<RunSave>,
    mut slot: ResMut<SaveSlot>,
) {
    let started = *status.started.get_or_insert_with(Instant::now);
    if started.elapsed().as_secs() > LOADING_TIMEOUT_SECONDS {
        status.error = Some("Timed out while loading or preparing the world".into());
    }
    let mut handles = assets.model_handles();
    for handle in handles.clone() {
        if let Some(LoadState::Failed(error)) = server.get_load_state(handle.id()) {
            status.error = Some(format!("Could not load game assets: {error}"));
        }
        if let Some(RecursiveDependencyLoadState::Failed(error)) =
            server.get_recursive_dependency_load_state(handle.id())
        {
            status.error = Some(format!("Could not load an asset dependency: {error}"));
        }
    }
    if let Some(LoadState::Failed(error)) = server.get_load_state(assets.quests.id()) {
        status.error = Some(format!("Could not load the quest catalog: {error}"));
    }
    if status.error.is_some() {
        next.set(GameState::Failed);
        return;
    }
    if status.spawning {
        if pending.is_empty()
            && (status.collision_ready || arena_floor_ready(&spatial, arena_floor.single().ok()))
            && (assets.relay.is_none() || status.relay_collision_ready)
            && colliders.iter().nth(MIN_NON_PLAYER_COLLIDERS - 1).is_some()
        {
            next.set(GameState::Playing);
        }
        return;
    }
    if !handles
        .clone()
        .all(|h| server.is_loaded_with_dependencies(h.id()))
    {
        return;
    }
    if handles.any(|handle| {
        gltfs
            .get(handle)
            .is_none_or(|model| model.scenes.is_empty())
    }) {
        status.error = Some("A game model is missing its scene".into());
        next.set(GameState::Failed);
        return;
    }
    let Some(catalog) = catalogs.get(&assets.quests) else {
        return;
    };
    let journal = if let Some(save) = run.checkpoint() {
        match QuestJournal::from_snapshot(catalog.clone(), *layout, &save.journal) {
            Ok(journal) => journal,
            Err(error) => {
                slot.reject(error);
                run.start_fresh();
                commands.remove_resource::<GameAssets>();
                next.set(GameState::Title);
                return;
            }
        }
    } else {
        QuestJournal::new(catalog.clone(), *layout)
    };
    let (player_animations, enemy_visuals) =
        match prepare_animations(&assets, &gltfs, &clips, &mut graphs) {
            Ok(result) => result,
            Err(error) => {
                status.error = Some(error);
                next.set(GameState::Failed);
                return;
            }
        };
    commands.insert_resource(player_animations);
    commands.insert_resource(enemy_visuals);
    commands.insert_resource(journal);
    status.spawning = true;
    commands.trigger(crate::world::SpawnWorld);
}

/// The arena uses a ray probe because it has no imported collider hierarchy.
fn arena_floor_ready(spatial: &SpatialQuery, floor: Option<Entity>) -> bool {
    floor.is_some_and(|floor| {
        spatial
            .cast_ray(
                ARENA_FLOOR_PROBE_ORIGIN,
                Dir3::NEG_Y,
                ARENA_FLOOR_PROBE_RANGE,
                true,
                &SpatialQueryFilter::default(),
            )
            .is_some_and(|hit| hit.entity == floor)
    })
}

fn prepare_animations(
    assets: &GameAssets,
    gltfs: &Assets<Gltf>,
    clips: &Assets<AnimationClip>,
    graphs: &mut Assets<AnimationGraph>,
) -> Result<(AnimationLibrary, EnemyVisuals), String> {
    let model = gltfs.get(&assets.character).expect(
        "the character glTF must be present after all game models passed loading validation",
    );
    for bone in crate::equipment::EQUIPMENT_SOCKETS
        .into_iter()
        .map(|(bone, _, _)| bone)
        .chain([crate::equipment::HOLSTER_ROOT_BONE])
    {
        if !model.named_nodes.contains_key(bone) {
            return Err(format!(
                "The player rig is missing equipment attachment bone {bone}"
            ));
        }
    }
    let (mut graph, nodes) = animation_graph(model, CLIPS, "plutonian.glb")?;
    let guards = add_sword_animation_masks(
        &mut graph,
        &nodes,
        model.named_animations["SwordIdle"].clone(),
        clips,
    )?;
    let enemies = ENEMY_MODELS
        .iter()
        .map(|definition| {
            let model = gltfs
                .get(&assets.enemies[definition.path])
                .expect("enemy models must be present after loading validation");
            let (graph, nodes) = animation_graph(model, definition.clips, definition.path)?;
            Ok((definition.path, (model.scenes[0].clone(), graph, nodes)))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;

    let player = AnimationLibrary {
        graph: graphs.add(graph),
        nodes,
        sword_guards: guards,
    };
    let enemies = EnemyVisuals(
        enemies
            .into_iter()
            .map(|(path, (scene, graph, nodes))| {
                let animations = EnemyAnimations {
                    graph: graphs.add(graph),
                    nodes,
                };
                (path, EnemyVisual { scene, animations })
            })
            .collect(),
    );
    Ok((player, enemies))
}

fn animation_graph<const N: usize>(
    model: &Gltf,
    names: [&str; N],
    file: &str,
) -> Result<(AnimationGraph, [AnimationNodeIndex; N]), String> {
    let mut graph = AnimationGraph::new();
    let mut nodes = Vec::with_capacity(N);
    for name in names {
        let clip = model
            .named_animations
            .get(name)
            .ok_or_else(|| format!("{file} is missing animation {name}"))?;
        nodes.push(graph.add_clip(clip.clone(), 1.0, graph.root));
    }
    let nodes = nodes
        .try_into()
        .expect("each registered clip must produce one animation graph node");
    Ok((graph, nodes))
}
