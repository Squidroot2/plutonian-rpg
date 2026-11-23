use crate::{
    GameState,
    content::Items,
    crossing::CrossingAssets,
    dialogue::{DialogueCatalog, DialogueRegistry},
    enemy::{EnemyAnimations, EnemyModel, EnemyVisual},
    guard, hushcap,
    inventory::ItemKind,
    player::{AnimationLibrary, CLIPS, add_sword_animation_masks},
    quest::{QuestCatalog, QuestJournal},
    roach,
    save::{self, RunSave, SaveSlot},
    world::{SessionEntity, WorldLayout},
    worm,
};
use avian3d::prelude::{
    Collider, ColliderConstructor, ColliderConstructorHierarchy, RigidBody, SpatialQuery,
    SpatialQueryFilter,
};
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

const ENEMY_MODELS: &[EnemyModel] = &[worm::MODEL, roach::MODEL, hushcap::MODEL, guard::MODEL];

#[derive(Resource)]
pub(crate) struct EnemyVisuals(BTreeMap<&'static str, EnemyVisual>);

impl EnemyVisuals {
    pub(crate) fn get(&self, model: &EnemyModel) -> &EnemyVisual {
        self.0
            .get(model.path)
            .expect("enemy visuals must be prepared during loading")
    }
    #[cfg(test)]
    pub(crate) fn test_stub(model: &EnemyModel) -> Self {
        let graph = AnimationGraph::new();
        Self(BTreeMap::from([(
            model.path,
            EnemyVisual {
                scene: Handle::default(),
                animations: EnemyAnimations {
                    graph: Handle::default(),
                    nodes: [graph.root; 3],
                    locomotion: None,
                },
                offset: model.visual_offset,
            },
        )]))
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
    dialogue: Handle<DialogueCatalog>,
    pub character: Handle<Gltf>,
    pub outpost: Option<(Handle<Gltf>, Handle<Gltf>)>,
    pub relay: Option<(Handle<Gltf>, Handle<Gltf>)>,
    pub south_basin: Option<(Handle<Gltf>, Handle<Gltf>)>,
    pub(crate) crossing: Option<CrossingAssets>,
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
                    .chain(self.south_basin.iter())
                    .flat_map(|(visuals, collision)| [visuals, collision]),
            )
            .chain(self.crossing.iter().flat_map(CrossingAssets::handles))
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

/// Instantiate aligned terrain scenes and return the collision root for its readiness observer.
pub(crate) fn spawn_static_scenes(
    commands: &mut Commands,
    models: &Assets<Gltf>,
    handles: &(Handle<Gltf>, Handle<Gltf>),
    names: (&'static str, &'static str),
    origin: Vec3,
) -> Entity {
    let scene = |handle| {
        WorldAssetRoot(
            models
                .get(handle)
                .expect("validated terrain models must remain loaded")
                .scenes[0]
                .clone(),
        )
    };
    commands
        .spawn((
            SessionEntity,
            Name::new(names.0),
            PendingScene,
            scene(&handles.0),
            Transform::from_translation(origin),
        ))
        .observe(scene_ready);
    commands
        .spawn((
            SessionEntity,
            Name::new(names.1),
            PendingScene,
            scene(&handles.1),
            Transform::from_translation(origin),
            RigidBody::Static,
            Visibility::Hidden,
            ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMesh),
        ))
        .observe(scene_ready)
        .id()
}

#[derive(Resource, Default)]
pub struct LoadStatus {
    pub error: Option<String>,
    pub spawning: bool,
    pub collision_ready: bool,
    pub relay_collision_ready: bool,
    pub south_basin_collision_ready: bool,
    pub(crate) crossing_collision_ready: bool,
    pub started: Option<Instant>,
}
fn load(
    items: Res<Items>,
    mut commands: Commands,
    server: Res<AssetServer>,
    layout: Res<WorldLayout>,
    mut status: ResMut<LoadStatus>,
) {
    *status = LoadStatus::default();
    commands.insert_resource(GameAssets {
        quests: server.load("data://quests/catalog.quest.ron"),
        dialogue: server.load("data://dialogue/catalog.dialogue.ron"),
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
        south_basin: (*layout == WorldLayout::Outpost).then(|| {
            (
                server.load("models/south_basin.glb"),
                server.load("models/south_basin_collision.glb"),
            )
        }),
        crossing: (*layout == WorldLayout::Outpost).then(|| CrossingAssets::load(&server)),
        core: server.load(items[ItemKind::BeaconCore].pickup_model.clone()),
        dummy: server.load("models/practice_dummy.glb"),
        enemies: ENEMY_MODELS
            .iter()
            .map(|model| (model.path, server.load(model.path)))
            .collect(),
        pickups: items
            .ids()
            .filter(|&item| item != ItemKind::BeaconCore)
            .map(|item| (item, server.load(items[item].pickup_model.clone())))
            .collect(),
        worn: items
            .gear_ids()
            .map(|item| {
                (
                    item,
                    server.load(
                        items[item]
                            .worn_model()
                            .expect("gear must have a worn model")
                            .to_owned(),
                    ),
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
    (catalogs, dialogue_catalogs, dialogue_handlers): (
        Res<Assets<QuestCatalog>>,
        Res<Assets<DialogueCatalog>>,
        Res<DialogueRegistry>,
    ),
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
    if let Some(LoadState::Failed(error)) = server.get_load_state(assets.dialogue.id()) {
        status.error = Some(format!("Could not load the dialogue catalogue: {error}"));
    }
    if status.error.is_some() {
        next.set(GameState::Failed);
        return;
    }
    if status.spawning {
        if pending.is_empty()
            && (status.collision_ready || arena_floor_ready(&spatial, arena_floor.single().ok()))
            && (assets.relay.is_none() || status.relay_collision_ready)
            && (assets.south_basin.is_none() || status.south_basin_collision_ready)
            && (assets.crossing.is_none() || status.crossing_collision_ready)
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
    let Some(dialogue_catalog) = dialogue_catalogs.get(&assets.dialogue) else {
        return;
    };
    if let Err(error) = dialogue_catalog.validate_registry(&dialogue_handlers) {
        status.error = Some(format!("Invalid dialogue catalogue: {error}"));
        next.set(GameState::Failed);
        return;
    }
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
    commands.insert_resource(dialogue_catalog.clone());
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
            let (mut graph, nodes) = animation_graph(model, definition.clips, definition.path)?;
            let locomotion = definition
                .locomotion
                .map(|name| animation_node(&mut graph, model, name, definition.path))
                .transpose()?;
            Ok((
                definition.path,
                (
                    model.scenes[0].clone(),
                    graph,
                    nodes,
                    locomotion,
                    definition.visual_offset,
                ),
            ))
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
            .map(|(path, (scene, graph, nodes, locomotion, offset))| {
                let animations = EnemyAnimations {
                    graph: graphs.add(graph),
                    nodes,
                    locomotion,
                };
                (
                    path,
                    EnemyVisual {
                        scene,
                        animations,
                        offset,
                    },
                )
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
        nodes.push(animation_node(&mut graph, model, name, file)?);
    }
    let nodes = nodes
        .try_into()
        .expect("each registered clip must produce one animation graph node");
    Ok((graph, nodes))
}

fn animation_node(
    graph: &mut AnimationGraph,
    model: &Gltf,
    name: &str,
    file: &str,
) -> Result<AnimationNodeIndex, String> {
    let clip = model
        .named_animations
        .get(name)
        .ok_or_else(|| format!("{file} is missing animation {name}"))?;
    Ok(graph.add_clip(clip.clone(), 1.0, graph.root))
}
