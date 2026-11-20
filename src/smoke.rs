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
use std::time::Instant;

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
                Vec3::new(0.0, 0.0, -20.0),
                Vec3::new(0.0, 0.0, -65.0),
                Vec3::new(0.0, 0.0, -100.0),
                CORE + Vec3::Z * 2.3,
            ];
            points[smoke.waypoint.min(3)]
        }
        Quest::ReturnCore => {
            let points = [
                Vec3::new(0.0, 0.0, -100.0),
                Vec3::new(0.0, 0.0, -65.0),
                Vec3::new(0.0, 0.0, -20.0),
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
