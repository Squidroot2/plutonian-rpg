//! Opt-in desktop checks for live gameplay and shipped-map collisions.
mod equipment;
mod inventory;
mod spear;

use self::{
    equipment::{EquipmentSmoke, EquipmentSmokePlugin},
    inventory::{InventorySmoke, InventorySmokePlugin, click, walk},
    spear::{SpearSmoke, SpearSmokePlugin},
};
use crate::{
    GameState,
    camera::Orbit,
    interaction::{InteractionTarget, Nearby, Target},
    inventory::{Inventory, ItemKind},
    inventory_ui::{InventoryAction, InventoryUiState},
    loading::LoadStatus,
    player::Player,
    quest::{Beacon, Dialogue, Quest},
    title::{TitleBackdrop, TitleInput},
    world::WorldLayout,
};
use avian3d::prelude::*;
use bevy::{
    app::AppExit,
    input::InputSystems,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use std::time::{Duration, Instant};

pub const ARENA_LOOT: [(ItemKind, f32, f32); 10] = [
    (ItemKind::MethanePouch, -2.0, 0.0),
    (ItemKind::Scrap, -3.0, 1.3),
    (ItemKind::Battery, -4.0, 0.0),
    (ItemKind::Battery, -5.0, 1.3),
    (ItemKind::FieldSpear, 4.0, 0.0),
    (ItemKind::Trident, 7.5, 1.0),
    (ItemKind::FieldArmor, -3.0, -4.0),
    (ItemKind::FieldHelmet, 0.0, -4.0),
    (ItemKind::ReinforcedArmor, 3.0, -4.0),
    (ItemKind::ReinforcedHelmet, 6.0, -4.0),
];

const WALL: Vec3 = Vec3::new(0.0, 1.0, 9.0);
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Component)]
pub struct ArenaFloor;

pub fn spawn_arena(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let floor = Vec3::new(28.0, 1.0, 28.0);
    commands.spawn((
        ArenaFloor,
        Name::new("Smoke arena floor"),
        Mesh3d(meshes.add(Cuboid::from_size(floor))),
        MeshMaterial3d(materials.add(Color::srgb(0.16, 0.23, 0.32))),
        RigidBody::Static,
        Collider::cuboid(floor.x, floor.y, floor.z),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    let wall = Vec3::new(4.0, 2.0, 0.4);
    commands.spawn((
        Name::new("Smoke arena wall"),
        Mesh3d(meshes.add(Cuboid::from_size(wall))),
        MeshMaterial3d(materials.add(Color::srgb(0.4, 0.24, 0.12))),
        RigidBody::Static,
        Collider::cuboid(wall.x, wall.y, wall.z),
        Transform::from_translation(WALL),
    ));
}

pub struct SmokePlugin {
    pub arena: bool,
    pub fullscreen: bool,
}

#[derive(Resource)]
pub(crate) struct SmokeRun(Instant);

#[derive(Resource, Debug, Default)]
enum ArenaStep {
    #[default]
    CaptureArena,
    Pause(Entity),
    Paused {
        position: Vec3,
        started: Duration,
    },
    Quest,
    SelectCore,
    CheckCore,
    TryCoreDrop(Entity),
    CloseCore,
    Fall(Entity),
    Recover,
}

impl Plugin for SmokePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SmokeRun(Instant::now()))
            .add_systems(Update, watchdog)
            .add_systems(
                PreUpdate,
                start_from_title
                    .after(InputSystems)
                    .before(TitleInput)
                    .run_if(in_state(GameState::Title)),
            );
        if self.fullscreen {
            app.add_systems(Startup, |mut windows: Query<&mut Window>| {
                for mut window in &mut windows {
                    window.mode = bevy::window::WindowMode::BorderlessFullscreen(
                        bevy::window::MonitorSelection::Current,
                    );
                }
            });
        }
        if self.arena {
            app.insert_resource(WorldLayout::SmokeArena)
                .init_resource::<ArenaStep>()
                .add_systems(
                    OnEnter(GameState::Playing),
                    check_arena_wall.run_if(run_once),
                )
                .add_systems(
                    PreUpdate,
                    drive
                        .after(InputSystems)
                        .after(equipment::drive)
                        .before(crate::player::input)
                        .before(crate::ui::pause),
                )
                .add_plugins((InventorySmokePlugin, EquipmentSmokePlugin, SpearSmokePlugin));
        } else {
            app.add_systems(
                OnEnter(GameState::Playing),
                check_pillar_collision.run_if(run_once),
            )
            .add_systems(
                Update,
                finish_world_check.run_if(in_state(GameState::Playing)),
            );
        }
    }
}

fn start_from_title(
    backdrop: Query<&ImageNode, With<TitleBackdrop>>,
    captures: Query<(), With<Screenshot>>,
    assets: Res<Assets<Image>>,
    game_assets: Option<Res<crate::loading::GameAssets>>,
    time: Res<Time>,
    mut ready_at: Local<Option<Duration>>,
    mut screenshot: Local<Option<Entity>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    assert!(
        game_assets.is_none(),
        "the title screen must defer world loading"
    );
    if let Some(entity) = *screenshot {
        if captures.get(entity).is_err() {
            keys.press(KeyCode::Enter);
        }
    } else if backdrop
        .single()
        .is_ok_and(|node| assets.contains(&node.image))
    {
        // Give the renderer time to prepare the first UI pipelines after loading the texture.
        let ready_at = *ready_at.get_or_insert(time.elapsed());
        if time.elapsed() - ready_at >= Duration::from_millis(250) {
            *screenshot = Some(capture(&mut commands, "title"));
        }
    }
}

fn watchdog(
    run: Res<SmokeRun>,
    state: Res<State<GameState>>,
    status: Res<LoadStatus>,
    inventory: Option<Res<InventorySmoke>>,
    weapons: Option<Res<SpearSmoke>>,
    armor: Option<Res<EquipmentSmoke>>,
    arena: Option<Res<ArenaStep>>,
) {
    assert_ne!(
        *state.get(),
        GameState::Failed,
        "smoke test asset loading failed: {:?}",
        status.error
    );
    assert!(
        run.0.elapsed() < TIMEOUT,
        "smoke test timed out in {:?}; inventory {:?}, weapons {:?}, armor {:?}, arena {:?}",
        state.get(),
        inventory.as_deref().map(InventorySmoke::phase),
        weapons.as_deref(),
        armor.as_deref().map(EquipmentSmoke::stage),
        arena.as_deref(),
    );
}

pub(crate) fn capture(commands: &mut Commands, name: &str) -> Entity {
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("/tmp/plutonian-{name}.png")))
        .id()
}

fn drive(
    (inventory_smoke, armor, run): (Res<InventorySmoke>, Res<EquipmentSmoke>, Res<SmokeRun>),
    (inventory, ui, actions): (
        Res<Inventory>,
        Res<InventoryUiState>,
        Query<(Entity, &InventoryAction)>,
    ),
    (mut step, mut core_checked, fixed): (ResMut<ArenaStep>, Local<bool>, Res<Time<Fixed>>),
    (state, quest, dialogue, nearby): (
        Res<State<GameState>>,
        Res<Quest>,
        Res<Dialogue>,
        Res<Nearby>,
    ),
    mut player: Query<(&mut Transform, &Player), With<Player>>,
    targets: Query<(&Target, &Transform), Without<Player>>,
    beacon: Query<&PointLight, With<Beacon>>,
    layout: Res<WorldLayout>,
    captures: Query<(), With<Screenshot>>,
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if !armor.done() {
        return;
    }
    let (mut transform, controller) = player.single_mut().expect("the arena must have one player");
    keys.reset_all();
    let click_action = |commands: &mut Commands, action| {
        let (window, camera) = inventory_smoke
            .context
            .expect("the inventory smoke phase must supply the window and camera for the quest UI");
        click(commands, action, &actions, window, camera);
    };
    match *step {
        ArenaStep::CaptureArena if *state.get() == GameState::Playing && controller.grounded => {
            *step = ArenaStep::Pause(capture(&mut commands, "smoke-arena"));
        }
        ArenaStep::Pause(capture) if captures.get(capture).is_err() => {
            keys.press(KeyCode::Escape);
            *step = ArenaStep::Paused {
                position: transform.translation,
                started: fixed.elapsed(),
            };
        }
        ArenaStep::Paused { position, started } if *state.get() == GameState::Paused => {
            keys.press(KeyCode::KeyW);
            assert!(
                position.distance(transform.translation) < 0.01,
                "player moved while paused"
            );
            if fixed.elapsed() - started >= fixed.timestep() * 3 {
                keys.press(KeyCode::Escape);
                *step = ArenaStep::Quest;
            }
        }
        ArenaStep::Quest if *state.get() == GameState::Playing => {
            if dialogue.text.is_some() {
                keys.press(KeyCode::KeyE);
                return;
            }
            if *quest == Quest::Complete {
                assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
                assert!(
                    beacon.single().is_ok_and(|light| light.intensity > 0.0),
                    "turning in the core must light the beacon"
                );
                *step = ArenaStep::Fall(capture(&mut commands, "beacon"));
                return;
            }
            if *quest == Quest::ReturnCore && !*core_checked {
                assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
                keys.press(KeyCode::KeyI);
                *step = ArenaStep::SelectCore;
                return;
            }
            let target = match *quest {
                Quest::MeetKeeper | Quest::ReturnCore => Target::Keeper,
                Quest::FindCore => Target::Core,
                Quest::Complete => unreachable!(),
            };
            if matches!(nearby.0, Some((_, InteractionTarget::Quest(current))) if current == target)
            {
                keys.press(KeyCode::KeyE);
            } else {
                let (_, destination) = targets
                    .iter()
                    .find(|(current, _)| **current == target)
                    .expect("the active quest target must have a world entity");
                walk(
                    &mut keys,
                    &mut orbit,
                    transform.translation,
                    destination.translation,
                );
            }
        }
        ArenaStep::SelectCore if *state.get() == GameState::Inventory => {
            click_action(&mut commands, InventoryAction::Select(ItemKind::BeaconCore));
            *step = ArenaStep::CheckCore;
        }
        ArenaStep::CheckCore => {
            assert_eq!(ui.selected, Some(ItemKind::BeaconCore));
            assert_eq!(
                ui.amount(&inventory),
                None,
                "the core must be protected in inventory"
            );
            *step = ArenaStep::TryCoreDrop(capture(&mut commands, "quest-inventory"));
        }
        ArenaStep::TryCoreDrop(capture) if captures.get(capture).is_err() => {
            click_action(&mut commands, InventoryAction::Drop);
            *step = ArenaStep::CloseCore;
        }
        ArenaStep::CloseCore => {
            assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
            *core_checked = true;
            keys.press(KeyCode::KeyI);
            *step = ArenaStep::Quest;
        }
        ArenaStep::Fall(capture) if captures.get(capture).is_err() => {
            transform.translation.y = -20.0;
            *step = ArenaStep::Recover;
        }
        ArenaStep::Recover
            if controller.grounded && transform.translation.distance(layout.start()) < 0.1 =>
        {
            assert_eq!(*quest, Quest::Complete);
            assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
            info!(
                "SMOKE PASS: arena gameplay checks completed in {:.2}s",
                run.0.elapsed().as_secs_f64()
            );
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn check_arena_wall(players: Query<&Collider, With<Player>>, mover: MoveAndSlide) {
    let capsule = players
        .single()
        .expect("arena collision check requires one player capsule");
    let position = mover
        .move_and_slide(
            capsule,
            WALL + Vec3::new(0.0, -0.085, -2.0),
            Quat::IDENTITY,
            Vec3::Z * 5.0,
            Duration::from_secs(1),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::default(),
            |_| MoveAndSlideHitResponse::Accept,
        )
        .position;
    assert!(
        (0.45..0.65).contains(&(WALL.z - position.z)),
        "the movement capsule must approach and stop at the arena wall, position {position:?}"
    );
    info!("ARENA COLLISION PASS: wall stops the character capsule");
}

fn finish_world_check(
    run: Res<SmokeRun>,
    player: Query<(&Player, &Transform)>,
    captures: Query<(), With<Screenshot>>,
    mut screenshot: Local<Option<Entity>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok((controller, transform)) = player.single() else {
        return;
    };
    if let Some(entity) = *screenshot {
        if captures.get(entity).is_err() {
            info!(
                "WORLD SMOKE PASS: shipped map, player grounding, and authored pillars verified in {:.2}s",
                run.0.elapsed().as_secs_f64()
            );
            exit.write(AppExit::Success);
        }
    } else if controller.grounded {
        let start = WorldLayout::Outpost.start();
        assert!(transform.translation.xz().distance(start.xz()) < 0.01);
        *screenshot = Some(capture(&mut commands, "outpost"));
    }
}

// Exercise the same capsule and move-and-slide path used by character movement
// against every authored pillar, without changing the player's live position.
fn check_pillar_collision(
    nodes: Query<(&Name, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    colliders: Query<Entity, With<avian3d::prelude::Collider>>,
    players: Query<&avian3d::prelude::Collider, With<Player>>,
    mover: avian3d::prelude::MoveAndSlide,
) {
    use avian3d::prelude::*;
    let capsule = players
        .single()
        .expect("pillar collision checks require exactly one player capsule");
    let mut pillars = std::collections::BTreeMap::new();
    for (name, transform) in &nodes {
        if name.as_str().starts_with("Waystone") && !name.as_str().starts_with("Waystone lamp") {
            pillars.insert(name.as_str().to_owned(), transform.translation());
        }
    }
    assert!(
        !pillars.is_empty(),
        "pillar collision checks require trail pillars in the loaded scene"
    );
    for (name, center) in &pillars {
        // A few cave pillars sit against walls. Isolate each pillar's actual
        // collider descendants so a wall cannot mask a missing pillar collider.
        let excluded = colliders.iter().filter(|entity| {
            !std::iter::once(*entity)
                .chain(parents.iter_ancestors(*entity))
                .any(|ancestor| {
                    nodes
                        .get(ancestor)
                        .is_ok_and(|(ancestor_name, _)| ancestor_name.as_str() == name)
                })
        });
        let filter = SpatialQueryFilter::from_excluded_entities(excluded);
        for side in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            // Pillar meshes are 2.8 m tall with their origin at the center;
            // put the player's 0.895 m half-height capsule just above the base.
            let start = *center + side * 2.0 - Vec3::Y * 0.455;
            let output = mover.move_and_slide(
                capsule,
                start,
                Quat::IDENTITY,
                -side * 5.0,
                Duration::from_secs(1),
                &MoveAndSlideConfig::default(),
                &filter,
                |_| MoveAndSlideHitResponse::Accept,
            );
            let gap = (output.position - *center).dot(side);
            assert!(
                (0.5..1.0).contains(&gap),
                "{name}: character capsule passed through or failed to approach the pillar from {side:?}; final gap {gap}"
            );
        }
    }
    info!(
        "PILLAR COLLISION PASS: {} pillars block the character capsule from four directions",
        pillars.len()
    );
}
