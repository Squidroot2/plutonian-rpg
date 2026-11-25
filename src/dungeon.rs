use crate::{
    GameState,
    camera::Orbit,
    content::{Enemies, Placements},
    dialogue::{Dialogue, DialogueCatalog, DialogueRegistry, NoticeId},
    enemy,
    loading::{EnemyVisuals, GameAssets, LoadStatus, spawn_static_scenes},
    player::{Dodge, MovementInput, PLAYER_GROUND_OFFSET, Player},
    quest::{QuestAction, QuestJournal},
    save::EncounterId,
    weapon::WeaponAttack,
    world::{SessionEntity, SpawnWorld},
};
use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*};

pub(crate) const HATCH: Vec3 = Vec3::new(-3.0, 0.0, 1.0);
pub(crate) const ORIGIN: Vec3 = Vec3::new(0.0, -10.0, -8.0);
pub(crate) const ARCHIVE_HUSHCAP_HOME: Vec3 = Vec3::new(ORIGIN.x + 3.2, ORIGIN.y, ORIGIN.z - 26.0);
pub(crate) const GATE_LOCAL: Vec3 = Vec3::new(0.0, 1.85, -17.4);
pub(crate) const SWITCH_LOCAL: Vec3 = Vec3::new(9.9, 0.0, -10.75);
pub(crate) const LOG_LOCAL: Vec3 = Vec3::new(0.0, 0.0, -27.3);
const EXIT_LOCAL: Vec3 = Vec3::new(-1.25, 0.0, -1.15);
const GATE_OPEN_SECONDS: f32 = 0.6;
const HATCH_GROUND_PROBE_HEIGHT: f32 = 3.0;
const HATCH_GROUND_PROBE_RANGE: f32 = 6.0;
const RELAY_LANDING_OFFSET: Vec3 = Vec3::new(0.0, 2.6, -2.65);
const SURFACE_LANDING_OFFSET: Vec3 = Vec3::new(0.0, 3.0, 2.1);
const LANDING_CAMERA_PITCH: f32 = 0.22;
const GATE_LIFT: f32 = 3.9;

// Match the Old Relay notice IDs in data/dialogue/catalog.dialogue.ron.
pub(crate) const SEALED_HATCH: NoticeId = NoticeId("old_relay.sealed_hatch");
pub(crate) const ARCHIVE_GATE_OPEN: NoticeId = NoticeId("old_relay.archive_gate_open");
pub(crate) const HATCH_OBSTRUCTED: NoticeId = NoticeId("old_relay.hatch_obstructed");
pub(crate) const ARCHIVE_LOG: NoticeId = NoticeId("old_relay.survey_log");
pub(crate) const RELAY_NOTICES: &[NoticeId] = &[
    SEALED_HATCH,
    ARCHIVE_GATE_OPEN,
    HATCH_OBSTRUCTED,
    ARCHIVE_LOG,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub(crate) enum Area {
    #[default]
    Surface,
    OldRelay,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DungeonTarget {
    Hatch,
    Exit,
    Switch,
    SurveyLog,
}
impl DungeonTarget {
    pub(crate) fn area(self) -> Area {
        match self {
            Self::Hatch => Area::Surface,
            _ => Area::OldRelay,
        }
    }
}

#[derive(Event)]
pub(crate) struct UseDungeonTarget(pub DungeonTarget);

#[derive(
    Clone, Copy, Debug, Default, PartialEq, bitcode::Encode, bitcode::Decode, serde::Serialize,
)]
enum GateState {
    #[default]
    Closed,
    Opening(f32),
    Open,
}

impl GateState {
    fn height(self) -> f32 {
        let t = match self {
            Self::Closed => 0.0,
            Self::Opening(elapsed) => elapsed / GATE_OPEN_SECONDS,
            Self::Open => 1.0,
        };
        ORIGIN.y + GATE_LOCAL.y + GATE_LIFT * t * t * (3.0 - 2.0 * t)
    }
}

#[derive(Resource, Clone, Copy, Default, bitcode::Encode, bitcode::Decode, serde::Serialize)]
pub(crate) struct DungeonProgress {
    gate: GateState,
}

impl DungeonProgress {
    pub(crate) fn is_valid(&self) -> bool {
        match self.gate {
            GateState::Opening(elapsed) => (0.0..GATE_OPEN_SECONDS).contains(&elapsed),
            _ => true,
        }
    }

    pub(crate) fn gate_open(&self) -> bool {
        self.gate == GateState::Open
    }

    fn activate_switch(&mut self) {
        if self.gate == GateState::Closed {
            self.gate = GateState::Opening(0.0);
        }
    }

    fn read_log(&self) -> Option<NoticeId> {
        self.gate_open().then_some(ARCHIVE_LOG)
    }
}

#[derive(Component)]
pub(crate) struct ArchiveGate;

pub(crate) struct DungeonPlugin;
impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DialogueRegistry>()
            .world_mut()
            .resource_mut::<DialogueRegistry>()
            .require_notices(RELAY_NOTICES);
        app.init_resource::<DungeonProgress>()
            .add_observer(spawn_relay)
            .add_observer(use_target)
            .add_systems(OnExit(GameState::Loading), ground_surface_hatch)
            .add_systems(
                OnTransition {
                    exited: GameState::Loading,
                    entered: GameState::Playing,
                },
                restore_gate,
            )
            .add_systems(
                FixedUpdate,
                raise_gate
                    .before(crate::player::movement)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn spawn_relay(
    _: On<SpawnWorld>,
    assets: Res<GameAssets>,
    models: Res<Assets<Gltf>>,
    visuals: Res<EnemyVisuals>,
    enemies: Res<Enemies>,
    placements: Res<Placements>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Some(relay) = &assets.relay else {
        return;
    };
    let collision = spawn_static_scenes(
        &mut commands,
        &models,
        relay,
        ("Old Relay visuals", "Old Relay collision"),
        ORIGIN,
    );
    commands.entity(collision).observe(
        |_: On<ColliderConstructorHierarchyReady>, mut status: ResMut<LoadStatus>| {
            status.relay_collision_ready = true;
        },
    );
    spawn_relay_fixtures(&mut commands, &mut meshes, &mut materials);
    spawn_relay_lights(&mut commands);
    spawn_relay_encounters(&mut commands, &visuals, &enemies, &placements);
}

fn spawn_relay_fixtures(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let alloy = materials.add(StandardMaterial {
        base_color: Color::srgb(0.14, 0.23, 0.28),
        metallic: 0.65,
        perceptual_roughness: 0.38,
        ..default()
    });
    let cyan = materials.add(StandardMaterial {
        base_color: Color::srgb(0.17, 0.74, 0.86),
        emissive: LinearRgba::rgb(0.13, 1.2, 1.65),
        ..default()
    });
    let amber = materials.add(StandardMaterial {
        base_color: Color::srgb(0.99, 0.49, 0.19),
        emissive: LinearRgba::rgb(1.6, 0.48, 0.10),
        ..default()
    });

    for (target, name, anchor) in [
        (DungeonTarget::Hatch, "Old Relay hatch", HATCH),
        (
            DungeonTarget::Exit,
            "Nivalis return hatch",
            ORIGIN + EXIT_LOCAL,
        ),
    ] {
        commands
            .spawn((
                SessionEntity,
                target,
                Name::new(name),
                Transform::from_translation(anchor),
                Visibility::default(),
            ))
            .with_children(|hatch| {
                let lid = meshes.add(Cylinder::new(1.02, 0.14));
                hatch.spawn((
                    Mesh3d(lid),
                    MeshMaterial3d(alloy.clone()),
                    Transform::from_xyz(0.0, 0.07, 0.0),
                ));
                hatch.spawn((
                    Mesh3d(meshes.add(Torus::new(0.82, 0.9))),
                    MeshMaterial3d(cyan.clone()),
                    Transform::from_xyz(0.0, 0.14, 0.0),
                ));
                hatch.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.22, 0.12, 0.78))),
                    MeshMaterial3d(alloy.clone()),
                    Transform::from_xyz(0.0, 0.15, 0.0),
                ));
            });
    }

    let gate_size = Vec3::new(3.77, 3.7, 0.33);
    commands
        .spawn((
            SessionEntity,
            ArchiveGate,
            Name::new("Archive gate"),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            Collider::cuboid(gate_size.x, gate_size.y, gate_size.z),
            Mesh3d(meshes.add(Cuboid::from_size(gate_size))),
            MeshMaterial3d(alloy.clone()),
            Transform::from_translation(ORIGIN + GATE_LOCAL),
        ))
        .with_children(|gate| {
            let inset = meshes.add(Cuboid::new(0.10, 2.75, 0.06));
            for x in [-1.25, -0.63, 0.0, 0.63, 1.25] {
                gate.spawn((
                    Mesh3d(inset.clone()),
                    MeshMaterial3d(cyan.clone()),
                    Transform::from_xyz(x, 0.0, 0.18),
                ));
            }
        });

    commands.spawn((
        SessionEntity,
        DungeonTarget::Switch,
        Name::new("Maintenance switch"),
        Mesh3d(meshes.add(Cuboid::new(0.29, 0.23, 0.28))),
        MeshMaterial3d(amber),
        Transform::from_translation(ORIGIN + SWITCH_LOCAL + Vec3::Y * 0.98),
    ));
    commands.spawn((
        SessionEntity,
        DungeonTarget::SurveyLog,
        Name::new("Survey log"),
        Mesh3d(meshes.add(Cuboid::new(0.74, 0.10, 0.46))),
        MeshMaterial3d(cyan),
        Transform::from_translation(ORIGIN + LOG_LOCAL + Vec3::Y * 1.14)
            .with_rotation(Quat::from_rotation_x(0.16)),
    ));
}

fn spawn_relay_lights(commands: &mut Commands) {
    let blue = Color::srgb(0.42, 0.80, 1.0);
    let warm = Color::srgb(1.0, 0.57, 0.27);
    for (offset, tint, intensity, range) in [
        (Vec3::new(0.0, 3.35, -3.0), blue, 27_000.0, 8.0),
        (Vec3::new(-3.9, 3.5, -9.0), blue, 48_000.0, 10.0),
        (Vec3::new(4.3, 3.3, -11.0), warm, 36_000.0, 9.0),
        (Vec3::new(9.4, 3.3, -10.5), warm, 49_000.0, 8.0),
        (Vec3::new(0.0, 3.6, -18.0), blue, 22_000.0, 7.0),
        (Vec3::new(2.2, 3.4, -23.0), blue, 41_000.0, 9.0),
        (Vec3::new(-2.5, 3.5, -26.5), blue, 46_000.0, 8.0),
    ] {
        commands.spawn((
            SessionEntity,
            PointLight {
                color: tint,
                intensity,
                range,
                radius: 0.45,
                ..default()
            },
            Transform::from_translation(ORIGIN + offset),
        ));
    }
}

fn spawn_relay_encounters(
    commands: &mut Commands,
    visuals: &EnemyVisuals,
    enemies: &Enemies,
    placements: &Placements,
) {
    for (id, home) in [
        (EncounterId::RelayRoach, ORIGIN + Vec3::new(7.0, 1.2, -12.5)),
        (
            EncounterId::RelayJunctionWorm,
            ORIGIN + Vec3::new(-3.5, 1.2, -10.5),
        ),
        (EncounterId::RelayArchiveHushcap, ARCHIVE_HUSHCAP_HOME),
    ] {
        enemy::spawn_variant(
            commands,
            visuals,
            placements.enemy(enemies, id),
            Transform::from_translation(home),
            Area::OldRelay,
            id,
        );
    }
}

fn ground_surface_hatch(
    spatial: SpatialQuery,
    mut targets: Query<(&DungeonTarget, &mut Transform)>,
) {
    for (&target, mut transform) in &mut targets {
        if target != DungeonTarget::Hatch {
            continue;
        }
        let origin = transform.translation + Vec3::Y * HATCH_GROUND_PROBE_HEIGHT;
        if let Some(hit) = spatial.cast_ray(
            origin,
            Dir3::NEG_Y,
            HATCH_GROUND_PROBE_RANGE,
            true,
            &SpatialQueryFilter::default(),
        ) {
            transform.translation.y = origin.y - hit.distance;
        }
    }
}

fn use_target(
    event: On<UseDungeonTarget>,
    mut quest: ResMut<QuestJournal>,
    mut progress: ResMut<DungeonProgress>,
    mut dialogue: ResMut<Dialogue>,
    catalog: Res<DialogueCatalog>,
    mut player: Query<(
        Entity,
        &mut Player,
        &mut Dodge,
        &mut Transform,
        &mut WeaponAttack,
    )>,
    mut orbit: ResMut<Orbit>,
    mut input: ResMut<MovementInput>,
    spatial: SpatialQuery,
) {
    let Ok((entity, mut player, mut dodge, mut transform, mut attack)) = player.single_mut() else {
        return;
    };
    if player.area != event.0.area() {
        return;
    }
    let (area, probe, yaw) = match event.0 {
        DungeonTarget::Hatch if !quest.beacon_powered() => {
            quest.record(QuestAction::InspectedSealedHatch);
            dialogue.show_notice(&catalog, SEALED_HATCH);
            return;
        }
        DungeonTarget::Hatch => (Area::OldRelay, ORIGIN + RELAY_LANDING_OFFSET, 0.0),
        DungeonTarget::Exit => (
            Area::Surface,
            HATCH + SURFACE_LANDING_OFFSET,
            std::f32::consts::PI,
        ),
        DungeonTarget::Switch => {
            if progress.gate_open() {
                dialogue.show_notice(&catalog, ARCHIVE_GATE_OPEN);
            } else {
                progress.activate_switch();
            }
            return;
        }
        DungeonTarget::SurveyLog => {
            if let Some(notice) = progress.read_log() {
                dialogue.show_notice(&catalog, notice);
                quest.record(QuestAction::ReadSurveyLog);
            }
            return;
        }
    };
    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    let Some(hit) = spatial.cast_ray(probe, Dir3::NEG_Y, HATCH_GROUND_PROBE_RANGE, true, &filter)
    else {
        dialogue.show_notice(&catalog, HATCH_OBSTRUCTED);
        return;
    };
    player.respawn(
        &mut dodge,
        &mut transform,
        probe - Vec3::Y * (hit.distance - PLAYER_GROUND_OFFSET),
    );
    player.area = area;
    if area == Area::OldRelay {
        quest.record(QuestAction::EnteredRelay);
    } else {
        quest.offer_cavern_report();
    }
    orbit.yaw = yaw;
    orbit.pitch = LANDING_CAMERA_PITCH;
    attack.cancel();
    *input = MovementInput::default();
}

fn raise_gate(
    time: Res<Time<Fixed>>,
    mut progress: ResMut<DungeonProgress>,
    mut gate: Query<&mut Transform, With<ArchiveGate>>,
    mut quest: ResMut<QuestJournal>,
) {
    let GateState::Opening(elapsed) = progress.gate else {
        return;
    };
    let Ok(mut transform) = gate.single_mut() else {
        return;
    };
    let elapsed = (elapsed + time.delta_secs()).min(GATE_OPEN_SECONDS);
    progress.gate = if elapsed >= GATE_OPEN_SECONDS {
        quest.record(QuestAction::OpenedArchiveGate);
        GateState::Open
    } else {
        GateState::Opening(elapsed)
    };
    transform.translation.y = progress.gate.height();
}

fn restore_gate(
    progress: Res<DungeonProgress>,
    mut gate: Query<&mut Transform, With<ArchiveGate>>,
) {
    if let Ok(mut transform) = gate.single_mut() {
        transform.translation.y = progress.gate.height();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    #[test]
    fn gate_latches_and_the_discovery_can_be_reread() {
        let mut world = World::new();
        world.init_resource::<Time<Fixed>>();
        world.init_resource::<DungeonProgress>();
        world.insert_resource(crate::quest::test_journal());
        let closed = ORIGIN + GATE_LOCAL;
        let gate = world
            .spawn((ArchiveGate, Transform::from_translation(closed)))
            .id();
        let mut progress = world.resource_mut::<DungeonProgress>();
        assert!(progress.read_log().is_none());
        progress.activate_switch();

        for _ in 0..2 {
            world
                .resource_mut::<Time<Fixed>>()
                .advance_by(Duration::from_millis(300));
            world
                .run_system_once(raise_gate)
                .expect("gate test must advance the fixed gate system");
            world.resource_mut::<DungeonProgress>().activate_switch();
        }
        let opened = world
            .get::<Transform>(gate)
            .expect("the gate fixture must have a transform")
            .translation;
        assert!((opened.y - closed.y - GATE_LIFT).abs() < 0.001);
        let progress = world.resource::<DungeonProgress>();
        assert!(progress.gate_open());
        assert_eq!(progress.read_log(), Some(ARCHIVE_LOG));
        assert_eq!(progress.read_log(), Some(ARCHIVE_LOG));
    }
}
