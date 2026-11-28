use super::{
    fail,
    recipe::{ACTOR_ORIGIN, Effect, Mount, Pose, Shot, model_path},
};
use crate::{
    loading::{CHARACTER_MODEL, OUTPOST_MODEL, PendingScene, scene_ready},
    storage::{RACK_POSITION, rack_display_transform},
    weapon::ATTACK_CLIP_FPS,
};
use bevy::{
    asset::{AssetPath, LoadState, RecursiveDependencyLoadState},
    gltf::Gltf,
    prelude::*,
    world_serialization::WorldInstanceReady,
};

#[derive(Resource)]
pub(super) struct LoadQueue(Vec<(Scene, Transform)>);

impl LoadQueue {
    pub(super) fn new(server: &AssetServer, model: &str, shot: Shot) -> Result<Self, String> {
        let mut roots = vec![(
            Scene::load(server, OUTPOST_MODEL, None),
            Transform::IDENTITY,
        )];
        let gear = (!shot.hide_model)
            .then(|| Scene::gear(server, model, shot.effects))
            .transpose()?;
        match shot.mount {
            Mount::Rack => {
                if let Some(gear) = gear {
                    let mount =
                        Transform::from_translation(RACK_POSITION) * rack_display_transform(0);
                    roots.push((gear, mount));
                }
            }
            Mount::Socket(socket) => {
                let mut actor = Scene::load(server, CHARACTER_MODEL, shot.pose);
                actor.mounts.extend(gear.map(|scene| Attachment {
                    socket,
                    offset: Vec3::ZERO,
                    scene,
                }));
                roots.push((actor, Transform::from_translation(ACTOR_ORIGIN)));
            }
        }
        Ok(Self(roots))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Component)]
struct Scene {
    model: Handle<Gltf>,
    pose: Option<Pose>,
    mounts: Vec<Attachment>,
}

struct Attachment {
    socket: String,
    offset: Vec3,
    scene: Scene,
}

impl Scene {
    fn load(server: &AssetServer, path: impl Into<AssetPath<'static>>, pose: Option<Pose>) -> Self {
        Self {
            model: server.load(path),
            pose,
            mounts: Vec::new(),
        }
    }

    fn gear(server: &AssetServer, model: &str, effects: Vec<Effect>) -> Result<Self, String> {
        let mut gear = Self::load(server, model_path(model)?, None);
        for effect in effects {
            gear.mounts.push(Attachment {
                socket: effect.socket,
                offset: effect.offset.into(),
                scene: Self::load(server, model_path(&effect.model)?, effect.pose),
            });
        }
        Ok(gear)
    }

    fn ready(&self, server: &AssetServer) -> Result<bool, String> {
        let id = self.model.id();
        if let LoadState::Failed(error) = server.load_state(id) {
            return Err(format!("Could not load {}: {error}", self.label()));
        }
        if let RecursiveDependencyLoadState::Failed(error) =
            server.recursive_dependency_load_state(id)
        {
            return Err(format!(
                "Could not load dependencies of {}: {error}",
                self.label()
            ));
        }
        self.mounts
            .iter()
            .try_fold(server.is_loaded_with_dependencies(id), |ready, mount| {
                Ok(mount.scene.ready(server)? && ready)
            })
    }

    fn label(&self) -> String {
        self.model
            .path()
            .map_or_else(|| "GLB".into(), ToString::to_string)
    }

    fn spawn(
        self,
        commands: &mut Commands,
        models: &Assets<Gltf>,
        transform: Transform,
        parent: Option<Entity>,
    ) -> Result<(), String> {
        let world = models
            .get(&self.model)
            .and_then(|model| {
                model
                    .default_scene
                    .as_ref()
                    .or_else(|| model.scenes.first())
            })
            .ok_or_else(|| format!("{} has no scene", self.label()))?;
        let mut entity =
            commands.spawn((WorldAssetRoot(world.clone()), transform, PendingScene, self));
        if let Some(parent) = parent {
            entity.insert(ChildOf(parent));
        }
        entity.observe(attach_and_pose).observe(scene_ready);
        Ok(())
    }
}

pub(super) fn spawn_loaded_scenes(
    mut queue: ResMut<LoadQueue>,
    server: Res<AssetServer>,
    models: Res<Assets<Gltf>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if queue.is_empty() {
        return;
    }
    let ready = queue.0.iter().try_fold(true, |ready, (scene, _)| {
        Ok::<_, String>(scene.ready(&server)? && ready)
    });
    match ready {
        Ok(true) => {
            for (scene, transform) in queue.0.drain(..) {
                if let Err(error) = scene.spawn(&mut commands, &models, transform, None) {
                    fail(&mut exit, error);
                    return;
                }
            }
        }
        Ok(false) => (),
        Err(error) => fail(&mut exit, error),
    }
}

fn attach_and_pose(
    ready: On<WorldInstanceReady>,
    mut scenes: Query<&mut Scene>,
    children: Query<&Children>,
    names: Query<&Name>,
    models: Res<Assets<Gltf>>,
    clips: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut players: Query<&mut AnimationPlayer>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(mut scene) = scenes.get_mut(ready.entity) else {
        return;
    };
    let descendants =
        || std::iter::once(ready.entity).chain(children.iter_descendants(ready.entity));
    if let Some(pose) = scene.pose.take()
        && let Err(error) = pose_scene(
            &scene,
            &pose,
            descendants(),
            &models,
            &clips,
            &mut graphs,
            &mut players,
            &mut commands,
        )
    {
        fail(&mut exit, error);
        return;
    }
    for attachment in std::mem::take(&mut scene.mounts) {
        let Some(socket) = descendants().find(|entity| {
            names
                .get(*entity)
                .is_ok_and(|name| name.as_str() == attachment.socket)
        }) else {
            fail(
                &mut exit,
                format_args!("{} has no socket {:?}", scene.label(), attachment.socket),
            );
            return;
        };
        if let Err(error) = attachment.scene.spawn(
            &mut commands,
            &models,
            Transform::from_translation(attachment.offset),
            Some(socket),
        ) {
            fail(&mut exit, error);
            return;
        }
    }
}

fn pose_scene(
    scene: &Scene,
    pose: &Pose,
    descendants: impl Iterator<Item = Entity>,
    models: &Assets<Gltf>,
    clips: &Assets<AnimationClip>,
    graphs: &mut Assets<AnimationGraph>,
    players: &mut Query<&mut AnimationPlayer>,
    commands: &mut Commands,
) -> Result<(), String> {
    let clip = models
        .get(&scene.model)
        .and_then(|model| model.named_animations.get(pose.clip.as_str()))
        .ok_or_else(|| format!("{} has no animation named {:?}", scene.label(), pose.clip))?;
    let time = f32::from(pose.frame) / ATTACK_CLIP_FPS;
    let duration = clips
        .get(clip)
        .ok_or("The requested animation was not loaded")?
        .duration();
    if time > duration {
        return Err(format!(
            "Frame {} is after the end of {:?} ({duration:.3} s)",
            pose.frame, pose.clip
        ));
    }
    let (graph, node) = AnimationGraph::from_clip(clip.clone());
    let graph = graphs.add(graph);
    let mut posed = false;
    for entity in descendants {
        if let Ok(mut player) = players.get_mut(entity) {
            player.play(node).pause().set_seek_time(time);
            commands
                .entity(entity)
                .insert(AnimationGraphHandle(graph.clone()));
            posed = true;
        }
    }
    if !posed {
        return Err(format!(
            "The scene in {} has no animation player",
            scene.label()
        ));
    }
    Ok(())
}
