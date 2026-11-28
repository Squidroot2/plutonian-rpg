use crate::{
    GameState,
    combat::Dead,
    combat_feedback::DeathAnimation,
    content::EnemyDefinition,
    dungeon::Area,
    enemy::{self, EnemyAnimations, EnemyClip, EnemyModel},
    loading::EnemyVisuals,
    world::{SessionEntity, SpawnWorld, WorldLayout},
};
use avian3d::prelude::Collider;
use bevy::{
    animation::graph::AnimationNodeType, app::AnimationSystems, asset::AssetEventSystems,
    color::Mix, gltf::GltfMaterialName, prelude::*, world_serialization::WorldInstanceReady,
};

const NEST_MIN_X: f32 = -63.5;
const NEST_MAX_X: f32 = -35.0;
const NEST_MIN_Z: f32 = -79.5;
const NEST_MAX_Z: f32 = -55.5;
const NEST_FILL_LIGHT_POS: Vec3 = Vec3::new(-47.0, 4.5, -66.5);
const NEST_FILL_LIGHT_COLOR: Color = Color::srgb(0.28, 0.48, 0.74);
const NEST_FILL_LIGHT_LUMENS: f32 = 35_000.0;
const NEST_FILL_LIGHT_RANGE_M: f32 = 24.0;
const NEST_LANTERN_POS: Vec3 = Vec3::new(-54.85, 0.85, -67.25);
const NEST_LANTERN_COLOR: Color = Color::srgb(1.0, 0.52, 0.20);
const NEST_LANTERN_LUMENS: f32 = 8_000.0;
const NEST_LANTERN_RANGE_M: f32 = 7.0;

pub(crate) fn surface_location(position: Vec3) -> Option<&'static str> {
    ((NEST_MIN_X..=NEST_MAX_X).contains(&position.x)
        && (NEST_MIN_Z..=NEST_MAX_Z).contains(&position.z))
    .then_some("WORM NEST")
}

pub(crate) const MODEL: EnemyModel = EnemyModel {
    path: "models/ice_worm.glb",
    clips: [
        EnemyClip::new("WormIdle", 2.0),
        EnemyClip::new("WormBite", 1.4),
        EnemyClip::new("WormDeath", 0.7),
    ],
    visual_offset: Vec3::ZERO,
    locomotion: Some(EnemyClip::new("WormCrawl", 0.9)),
};
pub(crate) const GROUND_OFFSET: f32 = 0.56;
// Material names from the ice worm model.
pub(crate) const MOUTH_MATERIAL: &str = "Warm throat";
const CAVITY_MATERIAL: &str = "Mouth cavity";
#[derive(Component)]
pub(crate) struct Worm;

/// The jaws and inner mouth share a material copy local to this worm.
#[derive(Component)]
struct WormMouth {
    material: Handle<StandardMaterial>,
    lit_color: LinearRgba,
    lit_emission: LinearRgba,
    dark_color: LinearRgba,
}

impl WormMouth {
    fn new(
        source: &Handle<StandardMaterial>,
        dark_color: LinearRgba,
        materials: &mut Assets<StandardMaterial>,
    ) -> Option<Self> {
        let material = materials.get(source)?.clone();
        Some(Self {
            lit_color: material.base_color.to_linear(),
            lit_emission: material.emissive,
            dark_color,
            material: materials.add(material),
        })
    }
}

pub(crate) struct WormPlugin;

impl Plugin for WormPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(bind_mouth)
            .add_observer(spawn_nest_lights)
            .add_systems(
                PostUpdate,
                fade_mouths
                    .after(AnimationSystems)
                    .before(AssetEventSystems)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn spawn_nest_lights(_: On<SpawnWorld>, layout: Res<WorldLayout>, mut commands: Commands) {
    if *layout != WorldLayout::Outpost {
        return;
    }
    for (name, position, color, intensity, range) in [
        (
            "Worm nest fill light",
            NEST_FILL_LIGHT_POS,
            NEST_FILL_LIGHT_COLOR,
            NEST_FILL_LIGHT_LUMENS,
            NEST_FILL_LIGHT_RANGE_M,
        ),
        (
            "Worm nest scout lantern",
            NEST_LANTERN_POS,
            NEST_LANTERN_COLOR,
            NEST_LANTERN_LUMENS,
            NEST_LANTERN_RANGE_M,
        ),
    ] {
        commands.spawn((
            SessionEntity,
            Name::new(name),
            PointLight {
                color,
                intensity,
                range,
                ..default()
            },
            Transform::from_translation(position),
        ));
    }
}

fn bind_mouth(
    ready: On<WorldInstanceReady>,
    parents: Query<&ChildOf>,
    worms: Query<(), With<Worm>>,
    children: Query<&Children>,
    mut parts: Query<(&GltfMaterialName, &mut MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Ok(parent) = parents.get(ready.entity) else {
        return;
    };
    let worm = parent.parent();
    if !worms.contains(worm) {
        return;
    }

    let mut mouth_parts = Vec::new();
    let mut source = None;
    let mut dark_color = None;
    for child in children.iter_descendants(ready.entity) {
        let Ok((name, material)) = parts.get(child) else {
            continue;
        };
        match name.0.as_str() {
            MOUTH_MATERIAL => {
                source.get_or_insert_with(|| material.0.clone());
                mouth_parts.push(child);
            }
            CAVITY_MATERIAL => {
                dark_color = materials
                    .get(&material.0)
                    .map(|mat| mat.base_color.to_linear());
            }
            _ => {}
        }
    }
    let Some(mouth) = source
        .zip(dark_color)
        .and_then(|(source, dark_color)| WormMouth::new(&source, dark_color, &mut materials))
    else {
        warn!("Ice worm scene is missing its mouth materials");
        return;
    };
    for child in mouth_parts {
        let (_, mut material) = parts
            .get_mut(child)
            .expect("a mouth mesh collected from this query must still have its material");
        material.0 = mouth.material.clone();
    }
    commands.entity(worm).insert(mouth);
}

fn fade_mouths(
    worms: Query<(Entity, &WormMouth, &DeathAnimation, &EnemyAnimations), With<Dead>>,
    players: Query<&AnimationPlayer>,
    graphs: Res<Assets<AnimationGraph>>,
    clips: Res<Assets<AnimationClip>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (worm, mouth, death, animations) in &worms {
        let Some(animation) = death
            .players
            .iter()
            .find_map(|&player| players.get(player).ok()?.animation(death.clip))
        else {
            continue;
        };
        let Some(node) = graphs
            .get(&animations.graph)
            .and_then(|graph| graph.get(death.clip))
        else {
            continue;
        };
        let AnimationNodeType::Clip(clip) = &node.node_type else {
            continue;
        };
        let Some(clip) = clips.get(clip) else {
            continue;
        };
        // Follow the collapse even when animation playback pauses.
        let progress = if animation.is_finished() || clip.duration() <= 0.0 {
            1.0
        } else {
            (animation.seek_time() / clip.duration()).clamp(0.0, 1.0)
        };
        let base_color = Color::from(mouth.lit_color.mix(&mouth.dark_color, progress));
        let emissive = mouth.lit_emission.mix(&LinearRgba::BLACK, progress);
        let Some(mut material) = materials.get_mut(&mouth.material) else {
            continue;
        };
        if material.base_color != base_color || material.emissive != emissive {
            material.base_color = base_color;
            material.emissive = emissive;
        }
        if progress == 1.0 {
            commands.entity(worm).remove::<WormMouth>();
        }
    }
}

pub(crate) fn spawn(
    commands: &mut Commands,
    visuals: &EnemyVisuals,
    definition: &EnemyDefinition,
    home: Transform,
    area: Area,
) -> Entity {
    let body = Collider::compound(vec![
        (
            Vec3::new(0.0, 0.0, 0.30),
            Quat::IDENTITY,
            Collider::sphere(0.47),
        ),
        (
            Vec3::new(0.0, -0.14, -0.33),
            Quat::IDENTITY,
            Collider::sphere(0.39),
        ),
        (
            Vec3::new(0.0, -0.32, -0.75),
            Quat::IDENTITY,
            Collider::sphere(0.23),
        ),
    ]);
    enemy::spawn_ground_melee(
        commands,
        Worm,
        visuals.get(&MODEL),
        home,
        area,
        definition.melee(GROUND_OFFSET),
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn surface_location_identifies_only_the_worm_nest_hollow() {
        assert_eq!(
            surface_location(Vec3::new(-47.0, 0.0, -66.5)),
            Some("WORM NEST")
        );
        assert_eq!(
            surface_location(Vec3::new(-54.0, 0.0, -67.0)),
            Some("WORM NEST")
        );
        assert_eq!(surface_location(Vec3::new(-4.0, 0.0, -62.0)), None);
        assert_eq!(surface_location(Vec3::new(0.0, 0.0, 0.0)), None);
    }

    #[test]
    fn nest_lights_illuminate_the_hollow_and_scout_lantern_only_in_outpost() {
        for (layout, expected) in [(WorldLayout::Outpost, 2), (WorldLayout::SmokeArena, 0)] {
            let mut world = World::new();
            world.insert_resource(layout);
            world.add_observer(spawn_nest_lights);
            world.trigger(SpawnWorld);
            world.flush();
            assert_eq!(
                world
                    .query::<(&PointLight, &Transform)>()
                    .iter(&world)
                    .count(),
                expected
            );
        }
    }

    #[test]
    fn the_mouth_fades_with_death_playback_only_for_the_defeated_worm() {
        let mut world = World::new();
        let mut materials = Assets::<StandardMaterial>::default();
        let lit_emission = LinearRgba::rgb(4.0, 1.0, 0.2);
        let lit_color = LinearRgba::rgb(1.0, 0.25, 0.05);
        let dark_color = LinearRgba::rgb(0.03, 0.01, 0.02);
        let source = materials.add(StandardMaterial {
            base_color: lit_color.into(),
            emissive: lit_emission,
            ..default()
        });
        let dead_mouth = WormMouth::new(&source, dark_color, &mut materials).expect("dead mouth");
        let live_mouth = WormMouth::new(&source, dark_color, &mut materials).expect("live mouth");
        let dead_material = dead_mouth.material.clone();
        let live_material = live_mouth.material.clone();
        world.insert_resource(materials);

        let mut clips = Assets::<AnimationClip>::default();
        let mut clip = AnimationClip::default();
        clip.set_duration(0.7);
        let (graph, death) = AnimationGraph::from_clip(clips.add(clip));
        let mut graphs = Assets::<AnimationGraph>::default();
        let animations = EnemyAnimations {
            graph: graphs.add(graph),
            nodes: [death; 3],
            locomotion: None,
        };
        world.insert_resource(clips);
        world.insert_resource(graphs);

        let mut player = AnimationPlayer::default();
        player.play(death).set_seek_time(0.35);
        let player = world.spawn(player).id();
        let worm = world
            .spawn((
                Dead,
                dead_mouth,
                animations.clone(),
                DeathAnimation {
                    players: vec![player],
                    clip: death,
                },
            ))
            .id();
        world.spawn((
            live_mouth,
            animations,
            DeathAnimation {
                players: vec![player],
                clip: death,
            },
        ));

        world.run_system_once(fade_mouths).expect("apply midpoint");
        let materials = world.resource::<Assets<StandardMaterial>>();
        let midpoint = materials.get(&dead_material).expect("midpoint material");
        assert_eq!(midpoint.emissive, lit_emission.mix(&LinearRgba::BLACK, 0.5));
        assert_eq!(midpoint.base_color, lit_color.mix(&dark_color, 0.5).into());

        world
            .get_mut::<AnimationPlayer>(player)
            .expect("death animation player")
            .animation_mut(death)
            .expect("active death clip")
            .set_seek_time(0.75);
        world.run_system_once(fade_mouths).expect("complete fade");
        let materials = world.resource::<Assets<StandardMaterial>>();
        let dark = materials.get(&dead_material).expect("corpse material");
        assert_eq!(dark.emissive, LinearRgba::BLACK);
        assert_eq!(dark.base_color, dark_color.into());
        assert!(world.get::<WormMouth>(worm).is_none());
        for handle in [source, live_material] {
            let lit = materials.get(&handle).expect("living mouth material");
            assert_eq!(lit.emissive, lit_emission);
            assert_eq!(lit.base_color, lit_color.into());
        }
    }
}
