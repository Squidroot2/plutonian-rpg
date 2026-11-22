use crate::{
    GameState,
    combat::{CombatSystems, Dead},
    dungeon::Area,
    experience::ExperienceUpdates,
    hushcap,
    loading::{EnemyVisuals, GameAssets, LoadStatus, spawn_static_scenes},
    quest::{CAVERN_HUSHCAP_GOAL, QuestAction, QuestJournal},
    roach,
    save::EncounterId,
    world::{SessionEntity, SpawnWorld},
    worm,
};
use avian3d::prelude::ColliderConstructorHierarchyReady;
use bevy::{gltf::Gltf, prelude::*};
use std::collections::BTreeSet;

const BASIN_EDGE_Z: f32 = 28.0;
const MOUTH_Z: f32 = 84.0;
pub(crate) const FAR_WALL_Z: f32 = 161.0;
const BASIN_HALF_WIDTH: f32 = 31.0;
const CAVE_HIGH_VAULT_Y: f32 = 8.0;
const CAVE_LIGHT_LUMENS: f32 = 40_000.0;
// All three chambers share the same floor elevation in the Blender terrain.
pub(crate) const CHAMBER_FLOOR_Y: f32 = -5.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, bitcode::Encode, bitcode::Decode)]
pub(crate) enum CaveEncounter {
    FirstWestHushcap,
    FirstEastHushcap,
    FirstWorm,
    SplitWestHushcap,
    SplitEastHushcap,
    SplitFarHushcap,
    SplitWorm,
    SplitRoach,
    GalleryWestHushcap,
    GalleryEastHushcap,
    GalleryFarHushcap,
    GalleryRoach,
}

#[derive(Clone, Copy)]
enum Species {
    Hushcap,
    Worm,
    Roach,
}

impl Species {
    fn ground_offset(self) -> f32 {
        match self {
            Self::Hushcap => 0.0,
            Self::Worm => worm::GROUND_OFFSET,
            Self::Roach => roach::GROUND_OFFSET,
        }
    }

    fn max_health(self) -> u16 {
        match self {
            Self::Hushcap => hushcap::HEALTH,
            Self::Worm => worm::CONFIG.body.health,
            Self::Roach => roach::CONFIG.body.health,
        }
    }
}

use CaveEncounter::*;
use Species::*;

const ENCOUNTERS: [(CaveEncounter, Species, f32, f32); 12] = [
    (FirstWestHushcap, Hushcap, 8.8, 104.3),
    (FirstEastHushcap, Hushcap, 22.3, 109.6),
    (FirstWorm, Worm, 12.8, 111.2),
    (SplitWestHushcap, Hushcap, 3.5, 123.5),
    (SplitEastHushcap, Hushcap, 19.8, 125.3),
    (SplitFarHushcap, Hushcap, 10.7, 134.0),
    (SplitWorm, Worm, 5.0, 131.6),
    (SplitRoach, Roach, 19.5, 132.0),
    (GalleryWestHushcap, Hushcap, 5.6, 146.4),
    (GalleryEastHushcap, Hushcap, 21.3, 146.0),
    (GalleryFarHushcap, Hushcap, 15.0, 156.5),
    (GalleryRoach, Roach, 8.8, 153.0),
];

fn encounters() -> impl ExactSizeIterator<Item = (CaveEncounter, Species, Vec3)> {
    ENCOUNTERS.into_iter().map(|(id, species, x, z)| {
        (
            id,
            species,
            Vec3::new(x, CHAMBER_FLOOR_Y + species.ground_offset(), z),
        )
    })
}

pub(crate) fn encounter_homes() -> impl ExactSizeIterator<Item = (CaveEncounter, Vec3, u16)> {
    encounters().map(|(id, species, home)| (id, home, species.max_health()))
}

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

pub(crate) type DefeatedHushcaps<'w, 's> =
    Query<'w, 's, &'static EncounterId, (With<Dead>, With<hushcap::Hushcap>)>;

pub(crate) fn defeated_hushcap_count(hushcaps: &DefeatedHushcaps) -> usize {
    hushcaps
        .iter()
        .filter_map(|encounter| match encounter {
            EncounterId::Cave(id) => Some(*id),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .len()
}

fn record_cavern_defeats(hushcaps: DefeatedHushcaps, mut journal: ResMut<QuestJournal>) {
    let milestone = QuestAction::DefeatedFiveCavernHushcaps;
    if !journal.has_occurred(milestone) && defeated_hushcap_count(&hushcaps) >= CAVERN_HUSHCAP_GOAL
    {
        journal.record(milestone);
    }
}

fn spawn_basin(
    _: On<SpawnWorld>,
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
    visuals: Res<EnemyVisuals>,
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
    for (id, species, home) in encounters() {
        let area = Area::Surface;
        let entity = match species {
            Hushcap => hushcap::spawn(&mut commands, &visuals, home, area),
            Worm => worm::spawn(&mut commands, &visuals, home, area),
            Roach => roach::spawn(&mut commands, &visuals, home, area),
        };
        commands.entity(entity).insert(EncounterId::Cave(id));
    }
}

fn spawn_lights(commands: &mut Commands) {
    for (x, y, z, range, color) in [
        (16.3, 2.1, 80.7, 13.0, Color::srgb(0.45, 0.64, 0.91)),
        (14.4, 0.9, 89.9, 9.0, Color::srgb(0.35, 0.58, 0.89)),
        (8.0, -2.5, 104.5, 13.0, Color::srgb(0.28, 0.74, 0.66)),
        (20.4, -2.2, 110.2, 12.0, Color::srgb(0.44, 0.64, 0.95)),
        (4.6, -2.0, 124.4, 13.0, Color::srgb(0.44, 0.75, 0.68)),
        (20.0, -1.9, 129.1, 12.0, Color::srgb(0.34, 0.62, 0.89)),
        (9.6, -2.5, 137.0, 10.0, Color::srgb(0.47, 0.75, 0.65)),
        (6.5, -1.5, 149.0, 15.0, Color::srgb(0.25, 0.72, 0.70)),
        (19.0, 1.5, 155.0, 16.0, Color::srgb(0.53, 0.55, 0.95)),
    ] {
        commands.spawn((
            SessionEntity,
            PointLight {
                color,
                intensity: CAVE_LIGHT_LUMENS,
                range,
                ..default()
            },
            Transform::from_xyz(x, y, z),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quest::test_relay_journal;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn the_archive_enemy_and_duplicate_ids_do_not_count_toward_five_cavern_defeats() {
        let mut world = World::new();
        let mut journal = test_relay_journal();
        journal.offer_cavern_report();
        journal.record(QuestAction::ReportedArchiveHushcap);
        world.insert_resource(journal);
        for id in [
            FirstWestHushcap,
            FirstEastHushcap,
            SplitWestHushcap,
            SplitEastHushcap,
            FirstWestHushcap,
        ] {
            world.spawn((EncounterId::Cave(id), hushcap::Hushcap, Dead));
        }
        world.spawn((EncounterId::RelayArchiveHushcap, hushcap::Hushcap, Dead));
        world.spawn((EncounterId::Cave(FirstWorm), worm::Worm, Dead));
        let fifth = world
            .spawn((EncounterId::Cave(SplitFarHushcap), hushcap::Hushcap))
            .id();
        world
            .run_system_once(record_cavern_defeats)
            .expect("check cavern defeats");
        assert!(
            !world
                .resource::<QuestJournal>()
                .has_occurred(QuestAction::DefeatedFiveCavernHushcaps)
        );

        world.entity_mut(fifth).insert(Dead);
        world
            .run_system_once(record_cavern_defeats)
            .expect("settle the fifth defeat");
        assert!(
            world
                .resource::<QuestJournal>()
                .expects(QuestAction::ShowedNemiMycelium)
        );
    }
}
