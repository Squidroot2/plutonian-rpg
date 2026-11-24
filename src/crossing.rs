//! The moving gate, the Listening Flats, and the crossing families.
mod caravan;
#[cfg(feature = "smoke")]
pub(crate) use caravan::Caravan;
#[cfg(test)]
pub(crate) use caravan::register_topics;
pub(crate) use caravan::{STATION_NEEDS_SAMPLES, STATION_VERIFIED, SampleStation, verify_samples};

use crate::{
    GameState,
    dialogue::DialogueRegistry,
    loading::{GameAssets, LoadStatus, PendingScene, scene_ready, spawn_static_scenes},
    quest::{QuestAction, QuestJournal},
    world::{SessionEntity, SpawnWorld},
};
use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*};

pub(crate) const WEST_RECOVERY_X: f32 = -135.0;
pub(crate) const GATE_CENTER: Vec3 = Vec3::new(-30.0, 2.30, 27.2);
pub(crate) const GATE_SIZE: Vec3 = Vec3::new(10.05, 8.7, 1.1);
const GATE_LIFT_M: f32 = 8.7;
const GATE_TRAVEL_SECONDS: f32 = 3.0;

pub(crate) struct CrossingAssets {
    terrain: (Handle<Gltf>, Handle<Gltf>),
    gate: Handle<Gltf>,
    pub(crate) vellik: Handle<Gltf>,
    pub(crate) wagon: Handle<Gltf>,
}

impl CrossingAssets {
    pub(crate) fn load(server: &AssetServer) -> Self {
        Self {
            terrain: (
                server.load("models/listening_flats.glb"),
                server.load("models/listening_flats_collision.glb"),
            ),
            gate: server.load("models/nivalis_gate.glb"),
            vellik: server.load("models/vellik.glb"),
            wagon: server.load("models/crossing_wagon.glb"),
        }
    }

    pub(crate) fn handles(&self) -> impl Iterator<Item = &Handle<Gltf>> + Clone {
        [
            &self.terrain.0,
            &self.terrain.1,
            &self.gate,
            &self.vellik,
            &self.wagon,
        ]
        .into_iter()
    }
}

pub(crate) fn surface_location(position: Vec3) -> Option<&'static str> {
    let east = -25.0 - ((position.z - 45.0) * (12.0 / 9.0)).clamp(0.0, 12.0);
    (position.z > GATE_CENTER.z && position.x < east).then_some("THE LISTENING FLATS")
}

#[derive(Component)]
pub(crate) struct CrossingGate {
    elapsed: f32,
}

impl CrossingGate {
    pub(crate) fn open(&self) -> bool {
        self.elapsed >= GATE_TRAVEL_SECONDS
    }

    fn height(&self) -> f32 {
        let t = self.elapsed / GATE_TRAVEL_SECONDS;
        GATE_CENTER.y + GATE_LIFT_M * t * t * (3.0 - 2.0 * t)
    }
}

pub(crate) struct CrossingPlugin;

impl Plugin for CrossingPlugin {
    fn build(&self, app: &mut App) {
        caravan::register_topics(&mut app.world_mut().resource_mut::<DialogueRegistry>());
        app.add_observer(spawn_crossing)
            .add_systems(
                FixedUpdate,
                (
                    lift_gate,
                    caravan::stage_caravan,
                    caravan::move_caravan,
                    caravan::settle_villagers,
                )
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                caravan::animate.run_if(in_state(GameState::Playing)),
            );
    }
}

fn spawn_crossing(
    _: On<SpawnWorld>,
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
    journal: Res<QuestJournal>,
    mut commands: Commands,
) {
    let Some(crossing) = &assets.crossing else {
        return;
    };
    let terrain = spawn_static_scenes(
        &mut commands,
        &models,
        &crossing.terrain,
        ("Listening Flats", "Listening Flats collision"),
        Vec3::ZERO,
    );
    commands.entity(terrain).observe(
        |_: On<ColliderConstructorHierarchyReady>, mut status: ResMut<LoadStatus>| {
            status.crossing_collision_ready = true;
        },
    );

    let model = models
        .get(&crossing.gate)
        .expect("the validated gate model is loaded");
    let gate = CrossingGate {
        elapsed: if journal.has_occurred(QuestAction::DefeatedFiveCavernHushcaps) {
            GATE_TRAVEL_SECONDS
        } else {
            0.0
        },
    };
    commands
        .spawn((
            SessionEntity,
            Name::new("Perimeter solid Gate leaf"),
            Transform::from_translation(GATE_CENTER.with_y(gate.height())),
            Visibility::default(),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            Collider::cuboid(GATE_SIZE.x, GATE_SIZE.y, GATE_SIZE.z),
            gate,
        ))
        .with_children(|gate| {
            gate.spawn((PendingScene, WorldAssetRoot(model.scenes[0].clone())))
                .observe(scene_ready);
        });

    for (at, tint) in [
        (Vec3::new(-30.0, 4.2, 34.0), Color::srgb(0.65, 0.44, 0.28)),
        (Vec3::new(-58.0, 4.3, 58.0), Color::srgb(0.29, 0.61, 0.85)),
        (Vec3::new(-101.0, 5.2, 84.0), Color::srgb(0.35, 0.57, 0.82)),
        (Vec3::new(-116.0, 6.0, 76.0), Color::srgb(0.50, 0.57, 0.82)),
    ] {
        commands.spawn((
            SessionEntity,
            Name::new("Crossing waylight"),
            Transform::from_translation(at),
            PointLight {
                color: tint,
                range: 29.0,
                intensity: 62_000.0,
                ..default()
            },
        ));
    }
}

fn lift_gate(
    time: Res<Time<Fixed>>,
    journal: Res<QuestJournal>,
    mut gates: Query<(&mut CrossingGate, &mut Transform)>,
) {
    if let Ok((mut gate, mut pose)) = gates.single_mut()
        && !gate.open()
        && journal.has_occurred(QuestAction::DefeatedFiveCavernHushcaps)
    {
        gate.elapsed = (gate.elapsed + time.delta_secs()).min(GATE_TRAVEL_SECONDS);
        pose.translation.y = gate.height();
    }
}
