use crate::{
    GameState,
    combat::{ActorDefeated, CombatSystems},
    experience::ExperienceUpdates,
    hushcap,
    loading::{GameAssets, LoadStatus, spawn_static_scenes},
    quest::QuestJournal,
    save::EncounterId,
    world::{SessionEntity, SpawnWorld},
};
use avian3d::prelude::ColliderConstructorHierarchyReady;
use bevy::{gltf::Gltf, prelude::*};

const BASIN_EDGE_Z: f32 = 28.0;
const MOUTH_Z: f32 = 84.0;
pub(crate) const FAR_WALL_Z: f32 = 161.0;
const BASIN_HALF_WIDTH: f32 = 31.0;
const CAVE_HIGH_VAULT_Y: f32 = 8.0;
const CAVE_LIGHT_LUMENS: f32 = 40_000.0;
/// Solid stalk collider name of the single glowing mushroom beside the South Basin ridge bend.
#[cfg(test)]
pub(crate) const GUIDE_MUSHROOM_STALK_NAME: &str = "South Basin solid Guide mushroom stalk";
/// Glowing cap center of the South Basin guide mushroom, visible from Nivalis.
#[cfg(any(test, feature = "smoke"))]
pub(crate) const GUIDE_MUSHROOM_CAP_POS: Vec3 = Vec3::new(-11.68, 1.30, 72.58);
/// Point-light position slightly beneath the cap so its glow washes the stalk, floor, and ridge.
const GUIDE_MUSHROOM_LIGHT_POS: Vec3 = Vec3::new(-11.68, 0.90, 72.58);
const GUIDE_MUSHROOM_LIGHT_LUMENS: f32 = 340_000.0;
const GUIDE_MUSHROOM_LIGHT_RANGE_M: f32 = 38.0;
const GUIDE_MUSHROOM_LIGHT_COLOR: Color = Color::srgb(0.24, 0.92, 0.66);
/// Brighter fungal spill tucked behind the ridge at the cavern mouth so the entrance beckons once
/// the player rounds the bend past the guide mushroom.
const CAVE_MOUTH_LIGHT_POS: Vec3 = Vec3::new(14.5, 2.0, 82.0);
const CAVE_MOUTH_LIGHT_LUMENS: f32 = 130_000.0;
const CAVE_MOUTH_LIGHT_RANGE_M: f32 = 24.0;
const CAVE_MOUTH_LIGHT_COLOR: Color = Color::srgb(0.28, 0.84, 0.72);
// All three chambers share the same floor elevation in the Blender terrain.
#[cfg(feature = "smoke")]
pub(crate) const CHAMBER_FLOOR_Y: f32 = -5.5;

pub(crate) fn surface_location(position: Vec3) -> Option<&'static str> {
    if position.z >= MOUTH_Z
        && position.z <= FAR_WALL_Z
        && (0.0..=29.0).contains(&position.x)
        && position.y <= CAVE_HIGH_VAULT_Y
    {
        Some("HUSHCAP CAVERNS")
    } else if (BASIN_EDGE_Z..MOUTH_Z).contains(&position.z) && position.x.abs() <= BASIN_HALF_WIDTH
    {
        Some("SOUTH BASIN")
    } else {
        None
    }
}

pub(crate) struct CavePlugin;

impl Plugin for CavePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_basin).add_systems(
            Update,
            record_cavern_defeats
                .after(CombatSystems::Resolve)
                .before(ExperienceUpdates)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

fn record_cavern_defeats(
    mut defeats: MessageReader<ActorDefeated>,
    hushcaps: Query<&EncounterId, With<hushcap::Hushcap>>,
    mut journal: ResMut<QuestJournal>,
) {
    for ActorDefeated(entity) in defeats.read() {
        if let Ok(EncounterId::Added(id)) = hushcaps.get(*entity) {
            journal.credit_cavern_defeat(*id);
        }
    }
}

fn spawn_basin(
    _: On<SpawnWorld>,
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
    mut commands: Commands,
) {
    let Some(scenes) = &assets.south_basin else {
        return;
    };
    let collision = spawn_static_scenes(
        &mut commands,
        &models,
        scenes,
        ("South Basin visuals", "South Basin collision"),
        Vec3::ZERO,
    );
    commands.entity(collision).observe(
        |_: On<ColliderConstructorHierarchyReady>, mut status: ResMut<LoadStatus>| {
            status.south_basin_collision_ready = true;
        },
    );
    spawn_lights(&mut commands);
}

fn spawn_lights(commands: &mut Commands) {
    for (position, intensity, range, color) in [
        (
            GUIDE_MUSHROOM_LIGHT_POS,
            GUIDE_MUSHROOM_LIGHT_LUMENS,
            GUIDE_MUSHROOM_LIGHT_RANGE_M,
            GUIDE_MUSHROOM_LIGHT_COLOR,
        ),
        (
            CAVE_MOUTH_LIGHT_POS,
            CAVE_MOUTH_LIGHT_LUMENS,
            CAVE_MOUTH_LIGHT_RANGE_M,
            CAVE_MOUTH_LIGHT_COLOR,
        ),
        (
            Vec3::new(14.4, 0.9, 89.9),
            CAVE_LIGHT_LUMENS,
            9.0,
            Color::srgb(0.35, 0.58, 0.89),
        ),
        (
            Vec3::new(8.0, -2.5, 104.5),
            CAVE_LIGHT_LUMENS,
            13.0,
            Color::srgb(0.28, 0.74, 0.66),
        ),
        (
            Vec3::new(20.4, -2.2, 110.2),
            CAVE_LIGHT_LUMENS,
            12.0,
            Color::srgb(0.44, 0.64, 0.95),
        ),
        (
            Vec3::new(4.6, -2.0, 124.4),
            CAVE_LIGHT_LUMENS,
            13.0,
            Color::srgb(0.44, 0.75, 0.68),
        ),
        (
            Vec3::new(20.0, -1.9, 129.1),
            CAVE_LIGHT_LUMENS,
            12.0,
            Color::srgb(0.34, 0.62, 0.89),
        ),
        (
            Vec3::new(9.6, -2.5, 137.0),
            CAVE_LIGHT_LUMENS,
            10.0,
            Color::srgb(0.47, 0.75, 0.65),
        ),
        (
            Vec3::new(6.5, -1.5, 149.0),
            CAVE_LIGHT_LUMENS,
            15.0,
            Color::srgb(0.25, 0.72, 0.70),
        ),
        (
            Vec3::new(19.0, 1.5, 155.0),
            CAVE_LIGHT_LUMENS,
            16.0,
            Color::srgb(0.53, 0.55, 0.95),
        ),
    ] {
        commands.spawn((
            SessionEntity,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combat::Dead,
        quest::{QuestAction, test_relay_journal},
    };
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn basin_lights_include_guide_mushroom_and_cave_mouth() {
        let mut world = World::new();
        world
            .run_system_once(|mut commands: Commands| spawn_lights(&mut commands))
            .expect("spawn cave lights");
        let lights: Vec<_> = world
            .query::<(&PointLight, &Transform)>()
            .iter(&world)
            .map(|(light, transform)| (transform.translation, light.intensity, light.range))
            .collect();
        assert!(
            lights.iter().any(|&(pos, intensity, range)| {
                pos.distance(GUIDE_MUSHROOM_LIGHT_POS) < 0.01
                    && intensity >= GUIDE_MUSHROOM_LIGHT_LUMENS
                    && range >= GUIDE_MUSHROOM_LIGHT_RANGE_M
            }),
            "expected a bright point light at the South Basin guide mushroom"
        );
        assert!(
            lights.iter().any(|&(pos, intensity, range)| {
                pos.distance(CAVE_MOUTH_LIGHT_POS) < 0.01
                    && intensity >= CAVE_MOUTH_LIGHT_LUMENS
                    && range >= CAVE_MOUTH_LIGHT_RANGE_M
            }),
            "expected a point light illuminating the hidden cave mouth behind the ridge"
        );
    }

    #[test]
    fn only_new_distinct_cavern_defeats_during_the_active_quest_count() {
        let mut world = World::new();
        world.init_resource::<Messages<ActorDefeated>>();
        world.insert_resource(test_relay_journal());
        let early = world
            .spawn((
                EncounterId::Added(crate::content::ContentId::new("cave_gallery_far_hushcap")),
                hushcap::Hushcap,
                Dead,
            ))
            .id();
        world.write_message(ActorDefeated(early));
        world
            .run_system_once(record_cavern_defeats)
            .expect("read early defeat");
        world.resource_mut::<Messages<ActorDefeated>>().clear();
        world.resource_mut::<QuestJournal>().offer_cavern_report();
        world
            .resource_mut::<QuestJournal>()
            .record(QuestAction::ReportedArchiveHushcap);

        let archive = world
            .spawn((
                EncounterId::Added(crate::content::ContentId::new("relay_archive_hushcap")),
                hushcap::Hushcap,
            ))
            .id();
        world.write_message(ActorDefeated(archive));
        for id in [
            crate::content::ContentId::new("cave_first_west_hushcap"),
            crate::content::ContentId::new("cave_first_east_hushcap"),
            crate::content::ContentId::new("cave_split_west_hushcap"),
            crate::content::ContentId::new("cave_split_east_hushcap"),
            crate::content::ContentId::new("cave_first_west_hushcap"),
        ] {
            let entity = world.spawn((EncounterId::Added(id), hushcap::Hushcap)).id();
            world.write_message(ActorDefeated(entity));
        }
        world
            .run_system_once(record_cavern_defeats)
            .expect("count four eligible Hushcaps");
        assert_eq!(world.resource::<QuestJournal>().cavern_defeat_count(), 4);
        assert!(
            !world
                .resource::<QuestJournal>()
                .has_occurred(QuestAction::DefeatedFiveCavernHushcaps)
        );

        let fifth = world
            .spawn((
                EncounterId::Added(crate::content::ContentId::new("cave_split_far_hushcap")),
                hushcap::Hushcap,
            ))
            .id();
        world.write_message(ActorDefeated(fifth));
        world
            .run_system_once(record_cavern_defeats)
            .expect("settle the fifth defeat");
        assert!(
            world
                .resource::<QuestJournal>()
                .expects(QuestAction::ShowedTovanMycelium)
        );
    }
}
