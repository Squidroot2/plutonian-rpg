//! Opt-in end-to-end desktop check using the real input, controller, and quest systems.
use crate::{
    GameState,
    camera::Orbit,
    inventory::{Inventory, ItemKind},
    inventory_smoke::InventorySmoke,
    inventory_ui::InventoryAction,
    player::Player,
    quest::{Dialogue, Nearby, Quest},
    world::{CORE, KEEPER},
};
use bevy::{
    app::AppExit,
    input::InputSystems,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use std::time::{Duration, Instant};

pub struct SmokePlugin;
#[derive(Resource)]
pub(crate) struct Smoke {
    start: Instant,
    action_at: f32,
    quest: Quest,
    waypoint: usize,
    captured: bool,
    completed_at: Option<f32>,
    jumped: bool,
    airborne: bool,
    landed: bool,
    fall_at: Option<f32>,
    paused: bool,
    pause_position: Option<Vec3>,
    core_ui: u8,
}
impl Plugin for SmokePlugin {
    fn build(&self, app: &mut App) {
        if std::env::args().any(|arg| arg == "--smoke-fullscreen") {
            app.add_systems(Startup, |mut windows: Query<&mut Window>| {
                for mut window in &mut windows {
                    window.mode = bevy::window::WindowMode::BorderlessFullscreen(
                        bevy::window::MonitorSelection::Current,
                    );
                }
            });
        }
        app.add_systems(
            OnEnter(GameState::Playing),
            check_pillar_collision.run_if(run_once),
        );
        app.insert_resource(Smoke {
            start: Instant::now(),
            action_at: 0.0,
            quest: Quest::MeetKeeper,
            waypoint: 0,
            captured: false,
            completed_at: None,
            jumped: false,
            airborne: false,
            landed: false,
            fall_at: None,
            paused: false,
            pause_position: None,
            core_ui: 0,
        })
        .add_systems(
            PreUpdate,
            drive
                .after(InputSystems)
                .before(crate::player::input)
                .before(crate::ui::pause),
        );
    }
}
pub(crate) fn drive(
    inventory_smoke: Res<InventorySmoke>,
    inventory: Res<Inventory>,
    actions: Query<(Entity, &InventoryAction)>,
    mut last_debug: Local<f32>,
    mut smoke: ResMut<Smoke>,
    time: Res<Time>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    quest: Res<Quest>,
    dialogue: Res<Dialogue>,
    nearby: Res<Nearby>,
    mut player: Query<(&mut Transform, &Player)>,
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if !inventory_smoke.done {
        return;
    }
    keys.reset_all();
    assert!(
        smoke.start.elapsed().as_secs() < 180,
        "smoke test timed out at {:?}, waypoint {}",
        *quest,
        smoke.waypoint
    );
    assert!(*state.get() != GameState::Failed, "asset loading failed");
    let Ok((mut transform, controller)) = player.single_mut() else {
        return;
    };
    let now = time.elapsed_secs();
    if now - *last_debug > 3.0 {
        info!(
            "SMOKE: state {:?}, position {:?}, grounded {}, velocity {:?}, waypoint {}",
            state.get(),
            transform.translation,
            controller.grounded,
            controller.velocity,
            smoke.waypoint
        );
        *last_debug = now;
    }
    if smoke.jumped && !controller.grounded {
        smoke.airborne = true;
    }
    if smoke.airborne && controller.grounded {
        smoke.landed = true;
    }
    if *state.get() == GameState::Inventory && smoke.core_ui > 0 {
        assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
        if smoke.core_ui == 1 {
            let (window, camera) = inventory_smoke.context.expect("the completed inventory smoke phase must have recorded the window and camera before the quest UI check");
            crate::inventory_smoke::click(
                &mut commands,
                InventoryAction::Select(ItemKind::BeaconCore),
                &actions,
                window,
                camera,
            );
            smoke.core_ui = 2;
            smoke.action_at = now;
        } else if smoke.core_ui == 2 && now - smoke.action_at > 0.4 {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/plutonian-quest-inventory.png"));
            smoke.core_ui = 3;
            smoke.action_at = now;
        } else if smoke.core_ui == 3 && now - smoke.action_at > 0.3 {
            keys.press(KeyCode::KeyI);
            smoke.core_ui = 4;
        }
        return;
    }
    if *state.get() == GameState::Paused {
        let position = smoke.pause_position.expect(
            "the smoke test must record the player position before entering its pause check",
        );
        assert!(
            position.distance(transform.translation) < 0.01,
            "player moved while paused"
        );
        if now - smoke.action_at > 0.5 {
            next.set(GameState::Playing);
            smoke.paused = true;
        }
        return;
    }
    if *state.get() != GameState::Playing {
        return;
    }
    if !smoke.paused && controller.grounded {
        smoke.pause_position = Some(transform.translation);
        smoke.action_at = now;
        next.set(GameState::Paused);
        return;
    }
    if !smoke.paused {
        return;
    }
    if !smoke.captured {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/plutonian-outpost.png"));
        smoke.captured = true;
        info!("SMOKE: loaded scenes, grounded player, pause/resume verified");
    }
    if smoke.quest != *quest {
        smoke.quest = *quest;
        smoke.waypoint = 0;
        smoke.action_at = now;
        info!("SMOKE: quest stage {:?}", *quest);
        if *quest == Quest::ReturnCore {
            assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/plutonian-cavern.png"));
        }
    }
    if dialogue.text.is_some() {
        if now - smoke.action_at > 1.0 {
            keys.press(KeyCode::KeyE);
            smoke.action_at = now;
        }
        return;
    }
    if *quest == Quest::ReturnCore && smoke.core_ui == 0 {
        keys.press(KeyCode::KeyI);
        smoke.core_ui = 1;
        return;
    }
    if *quest == Quest::Complete {
        assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
        if smoke.completed_at.is_none() {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/plutonian-beacon.png"));
            smoke.completed_at = Some(now);
        }
        if now
            - smoke.completed_at.expect(
                "the completion timestamp must be initialized before checking the post-quest delay",
            )
            > 2.0
        {
            assert!(
                smoke.airborne && smoke.landed,
                "jump did not leave the ground and land"
            );
            if let Some(fell_at) = smoke.fall_at {
                if now - fell_at > 0.5 {
                    assert!(
                        transform.translation.distance(crate::world::START) < 2.0,
                        "fall recovery failed"
                    );
                    assert_eq!(*quest, Quest::Complete);
                    info!(
                        "SMOKE PASS: walked to cavern and back, jumped and landed, collected core, completed quest, recovered from fall"
                    );
                    exit.write(AppExit::Success);
                }
            } else {
                transform.translation.y = -20.0;
                smoke.fall_at = Some(now);
            }
        }
        return;
    }
    let destination = match *quest {
        Quest::MeetKeeper => KEEPER,
        Quest::FindCore => {
            let points = [
                Vec3::new(-3.2, 0.0, -20.0),
                Vec3::new(-3.2, 0.0, -65.0),
                Vec3::new(-3.2, 0.0, -100.0),
                CORE + Vec3::Z * 2.3,
            ];
            points[smoke.waypoint.min(3)]
        }
        Quest::ReturnCore => {
            let points = [
                Vec3::new(-3.2, 0.0, -100.0),
                Vec3::new(-3.2, 0.0, -65.0),
                Vec3::new(-3.2, 0.0, -20.0),
                KEEPER,
            ];
            points[smoke.waypoint.min(3)]
        }
        Quest::Complete => unreachable!(),
    };
    let delta = (destination - transform.translation).with_y(0.0);
    if nearby.0.is_some() && (*quest == Quest::MeetKeeper || smoke.waypoint >= 3) {
        if now - smoke.action_at > 0.4 {
            keys.press(KeyCode::KeyE);
            smoke.action_at = now;
        }
        return;
    }
    if delta.length() < 0.5 && smoke.waypoint < 3 && *quest != Quest::MeetKeeper {
        smoke.waypoint += 1;
        return;
    }
    orbit.yaw = (-delta.x).atan2(-delta.z);
    keys.press(KeyCode::KeyW);
    keys.press(KeyCode::ShiftLeft);
    if *quest == Quest::FindCore
        && transform.translation.z < -28.0
        && !smoke.jumped
        && controller.grounded
    {
        keys.press(KeyCode::Space);
        smoke.jumped = true;
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
