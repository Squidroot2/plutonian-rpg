use crate::{
    GameState,
    camera::Orbit,
    inventory::scene_ready,
    loading::{EnemyVisuals, GameAssets, LoadStatus, PendingScene},
    player::{Dodge, MovementInput, Player},
    quest::{Dialogue, LAST_LIGHT, QuestAction, QuestJournal},
    roach,
    spear::Thrust,
    world::SpawnWorld,
    worm,
};
use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*};

pub(crate) const HATCH: Vec3 = Vec3::new(-3.0, 0.0, 1.0);
pub(crate) const ORIGIN: Vec3 = Vec3::new(0.0, -10.0, -8.0);
pub(crate) const GATE_LOCAL: Vec3 = Vec3::new(0.0, 1.85, -17.4);
pub(crate) const SWITCH_LOCAL: Vec3 = Vec3::new(9.9, 0.0, -10.75);
pub(crate) const LOG_LOCAL: Vec3 = Vec3::new(0.0, 0.0, -27.3);
const EXIT_LOCAL: Vec3 = Vec3::new(-1.25, 0.0, -1.15);
const DURATION: f32 = 0.6;
const GATE_LIFT: f32 = 3.9;
const ARCHIVE_LOG: &str = "SURVEY LOG / THE FIRST WATCH\n\nBefore there was a Nivalis, there was this room. We put the first lamps on the ice so the survey crews could find their way back in the long dark.\n\nThe beacon above us still sends the same signal. Its first meaning was simple. There is shelter here, and someone is waiting.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum GateState {
    #[default]
    Closed,
    Opening(f32),
    Open,
}

#[derive(Resource, Default)]
pub(crate) struct DungeonProgress {
    gate: GateState,
}

impl DungeonProgress {
    pub(crate) fn gate_open(&self) -> bool {
        self.gate == GateState::Open
    }

    fn activate_switch(&mut self) {
        if self.gate == GateState::Closed {
            self.gate = GateState::Opening(0.0);
        }
    }

    fn read_log(&self) -> Option<&'static str> {
        self.gate_open().then_some(ARCHIVE_LOG)
    }
}

#[derive(Component)]
pub(crate) struct ArchiveGate;

pub(crate) struct DungeonPlugin;
impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DungeonProgress>()
            .add_observer(spawn_relay)
            .add_observer(use_target)
            .add_systems(OnExit(GameState::Loading), ground_surface_hatch)
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
    enemies: Res<EnemyVisuals>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Some((visual, collision)) = &assets.relay else {
        return;
    };
    let scene = |handle| {
        WorldAssetRoot(
            models
                .get(handle)
                .expect("the validated relay model must remain loaded")
                .scenes[0]
                .clone(),
        )
    };
    commands
        .spawn((
            Name::new("Old Relay visuals"),
            PendingScene,
            scene(visual),
            Transform::from_translation(ORIGIN),
        ))
        .observe(scene_ready);
    commands
        .spawn((
            Name::new("Old Relay collision"),
            PendingScene,
            scene(collision),
            Transform::from_translation(ORIGIN),
            RigidBody::Static,
            Visibility::Hidden,
            ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMesh),
        ))
        .observe(scene_ready)
        .observe(
            |_: On<ColliderConstructorHierarchyReady>, mut status: ResMut<LoadStatus>| {
                status.relay_collision_ready = true;
            },
        );

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
        DungeonTarget::Switch,
        Name::new("Maintenance switch"),
        Mesh3d(meshes.add(Cuboid::new(0.29, 0.23, 0.28))),
        MeshMaterial3d(amber),
        Transform::from_translation(ORIGIN + SWITCH_LOCAL + Vec3::Y * 0.98),
    ));
    commands.spawn((
        DungeonTarget::SurveyLog,
        Name::new("Survey log"),
        Mesh3d(meshes.add(Cuboid::new(0.74, 0.10, 0.46))),
        MeshMaterial3d(cyan),
        Transform::from_translation(ORIGIN + LOG_LOCAL + Vec3::Y * 1.14)
            .with_rotation(Quat::from_rotation_x(0.16)),
    ));

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

    roach::spawn(
        &mut commands,
        &enemies,
        ORIGIN + Vec3::new(7.0, 1.2, -12.5),
        Area::OldRelay,
    );
    for offset in [Vec3::new(-3.5, 1.2, -10.5), Vec3::new(3.65, 1.2, -24.0)] {
        worm::spawn(&mut commands, &enemies, ORIGIN + offset, Area::OldRelay);
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
        let origin = transform.translation + Vec3::Y * 3.0;
        if let Some(hit) = spatial.cast_ray(
            origin,
            Dir3::NEG_Y,
            6.0,
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
    mut player: Query<(Entity, &mut Player, &mut Dodge, &mut Transform)>,
    mut orbit: ResMut<Orbit>,
    mut thrust: ResMut<Thrust>,
    mut input: ResMut<MovementInput>,
    spatial: SpatialQuery,
) {
    let Ok((entity, mut player, mut dodge, mut transform)) = player.single_mut() else {
        return;
    };
    if player.area != event.0.area() {
        return;
    }
    let (area, probe, yaw) = match event.0 {
        DungeonTarget::Hatch if !quest.is_complete(LAST_LIGHT) => {
            quest.record(QuestAction::InspectedSealedHatch);
            dialogue.text = Some(
                "OLD RELAY\n\nThe hatch responds to the beacon. Restore the light to release the seal.",
            );
            return;
        }
        DungeonTarget::Hatch => (Area::OldRelay, ORIGIN + Vec3::new(0.0, 2.6, -2.65), 0.0),
        DungeonTarget::Exit => (
            Area::Surface,
            HATCH + Vec3::new(0.0, 3.0, 2.1),
            std::f32::consts::PI,
        ),
        DungeonTarget::Switch => {
            if progress.gate_open() {
                dialogue.text = Some("The archive gate is open.");
            } else {
                progress.activate_switch();
            }
            return;
        }
        DungeonTarget::SurveyLog => {
            dialogue.text = progress.read_log();
            if dialogue.text.is_some() {
                quest.record(QuestAction::ReadSurveyLog);
            }
            return;
        }
    };
    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    let Some(hit) = spatial.cast_ray(probe, Dir3::NEG_Y, 6.0, true, &filter) else {
        dialogue.text = Some("The hatch landing is obstructed.");
        return;
    };
    player.respawn(
        &mut dodge,
        &mut transform,
        probe - Vec3::Y * (hit.distance - 0.905),
    );
    player.area = area;
    if area == Area::OldRelay {
        quest.record(QuestAction::EnteredRelay);
    }
    orbit.yaw = yaw;
    orbit.pitch = 0.22;
    thrust.cancel();
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
    let elapsed = (elapsed + time.delta_secs()).min(DURATION);
    let t = elapsed / DURATION;
    let eased = t * t * (3.0 - 2.0 * t);
    transform.translation.y = ORIGIN.y + GATE_LOCAL.y + GATE_LIFT * eased;
    progress.gate = if elapsed >= DURATION {
        quest.record(QuestAction::OpenedArchiveGate);
        GateState::Open
    } else {
        GateState::Opening(elapsed)
    };
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
