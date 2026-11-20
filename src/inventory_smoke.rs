//! Drives the inventory through normal key handling and UI pointer observers before the quest run.
use crate::{
    GameState,
    camera::{FollowCamera, Orbit},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind, Pickup},
    inventory_ui::{InventoryAction, InventoryUiState},
    player::Player,
};
use bevy::{
    camera::NormalizedRenderTarget,
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    picking::{
        backend::HitData,
        pointer::{Location, PointerButton, PointerId},
    },
    prelude::*,
    window::WindowRef,
};
use std::time::{Duration, Instant};

pub struct InventorySmokePlugin;
#[derive(Resource)]
pub struct InventorySmoke {
    pub done: bool,
    pub equipment_ready: bool,
    pub equipment_busy: bool,
    pub equipment_done: bool,
    pub context: Option<(Entity, Entity)>,
    phase: u8,
    collected: usize,
    timer: f32,
    position: Vec3,
    yaw: f32,
    start: Instant,
}
impl Plugin for InventorySmokePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(InventorySmoke {
            done: false,
            equipment_ready: false,
            equipment_busy: true,
            equipment_done: false,
            context: None,
            phase: 0,
            collected: 0,
            timer: 0.0,
            position: Vec3::ZERO,
            yaw: 0.0,
            start: Instant::now(),
        })
        .add_systems(
            PreUpdate,
            drive
                .after(InputSystems)
                .before(crate::ui::pause)
                .before(crate::smoke::drive),
        );
    }
}
pub(crate) fn click(
    commands: &mut Commands,
    action: InventoryAction,
    actions: &Query<(Entity, &InventoryAction)>,
    window: Entity,
    camera: Entity,
) {
    let entity = actions
        .iter()
        .find(|(_, kind)| **kind == action)
        .expect("missing inventory UI action")
        .0;
    commands.trigger(PointerClick {
        entity,
        pointer: Pointer::new(
            PointerId::Mouse,
            Location {
                target: NormalizedRenderTarget::Window(
                    WindowRef::Entity(window)
                        .normalize(Some(window))
                        .expect("an explicit smoke-test window reference must always normalize"),
                ),
                position: Vec2::ZERO,
            },
        ),
        button: PointerButton::Primary,
        hit: HitData::new(camera, 0.0, None, None),
        duration: Duration::from_millis(50),
        count: 1,
    });
}
pub(crate) fn walk(
    keys: &mut ButtonInput<KeyCode>,
    orbit: &mut Orbit,
    position: Vec3,
    destination: Vec3,
) {
    let delta = (destination - position).with_y(0.0);
    orbit.yaw = (-delta.x).atan2(-delta.z);
    keys.press(KeyCode::KeyW);
    keys.press(KeyCode::ShiftLeft);
}
pub(crate) fn drive(
    mut smoke: ResMut<InventorySmoke>,
    state: Res<State<GameState>>,
    time: Res<Time>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    inventory: Res<Inventory>,
    ui: Res<InventoryUiState>,
    nearby: Res<Nearby>,
    player: Query<&Transform, With<Player>>,
    pickups: Query<(&Pickup, &Transform)>,
    actions: Query<(Entity, &InventoryAction)>,
    windows: Query<Entity, With<Window>>,
    cameras: Query<Entity, With<FollowCamera>>,
    mut keyboard: MessageWriter<KeyboardInput>,
    mut commands: Commands,
) {
    if smoke.done {
        return;
    }
    keys.reset_all();
    assert!(
        smoke.start.elapsed().as_secs() < 120,
        "inventory smoke timed out at phase {}",
        smoke.phase
    );
    assert_ne!(
        *state.get(),
        GameState::Failed,
        "inventory assets failed to load"
    );
    if *state.get() == GameState::Loading {
        return;
    }
    let Ok(transform) = player.single() else {
        return;
    };
    let window = windows
        .single()
        .expect("the inventory smoke test requires exactly one game window");
    let camera = cameras
        .single()
        .expect("the inventory smoke test requires exactly one follow camera");
    smoke.context = Some((window, camera));
    let now = time.elapsed_secs();
    match smoke.phase {
        0 => {
            if transform.translation.z > -14.0 {
                walk(
                    &mut keys,
                    &mut orbit,
                    transform.translation,
                    Vec3::new(4.0, 0.0, -15.0),
                );
            } else {
                smoke.phase = 1;
            }
        }
        1 => {
            let order = [
                ItemKind::MethanePouch,
                ItemKind::Scrap,
                ItemKind::Battery,
                ItemKind::Battery,
            ];
            let item = order[smoke.collected];
            let required = if smoke.collected == 3 { 2 } else { 1 };
            if inventory.count(item) >= required {
                smoke.collected += 1;
                if smoke.collected == order.len() {
                    keys.press(KeyCode::KeyI);
                    smoke.phase = 2;
                }
                return;
            }
            if let Some((_, InteractionTarget::Pickup(pickup))) = nearby.0 {
                if pickup.item == item {
                    keys.press(KeyCode::KeyE);
                    return;
                }
            }
            let destination = pickups
                .iter()
                .filter(|(p, _)| p.item == item)
                .min_by(|(_, a), (_, b)| {
                    a.translation
                        .distance_squared(transform.translation)
                        .total_cmp(&b.translation.distance_squared(transform.translation))
                })
                .expect("material missing from world")
                .1
                .translation;
            walk(&mut keys, &mut orbit, transform.translation, destination);
        }
        2 if *state.get() == GameState::Inventory => {
            smoke.position = transform.translation;
            smoke.yaw = orbit.yaw;
            smoke.timer = now;
            smoke.phase = 3;
        }
        3 => {
            assert_eq!(*state.get(), GameState::Inventory);
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::Space);
            keys.press(KeyCode::KeyE);
            assert!(
                smoke.position.distance(transform.translation) < 0.01,
                "inventory did not freeze movement/gravity"
            );
            assert_eq!(smoke.yaw, orbit.yaw);
            if now - smoke.timer > 0.5 {
                click(
                    &mut commands,
                    InventoryAction::Select(ItemKind::Battery),
                    &actions,
                    window,
                    camera,
                );
                smoke.phase = 4;
            }
        }
        4 => {
            click(
                &mut commands,
                InventoryAction::Quantity,
                &actions,
                window,
                camera,
            );
            smoke.phase = 5;
        }
        5 => {
            keyboard.write(KeyboardInput {
                key_code: KeyCode::Digit2,
                logical_key: Key::Character("2".into()),
                state: ButtonState::Pressed,
                text: Some("2".into()),
                repeat: false,
                window,
            });
            smoke.phase = 6;
        }
        6 => {
            assert_eq!(ui.quantity, "2", "quantity field did not accept typing");
            click(
                &mut commands,
                InventoryAction::Minus,
                &actions,
                window,
                camera,
            );
            smoke.phase = 7;
        }
        7 => {
            assert_eq!(ui.amount(&inventory), Some(1));
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/plutonian-inventory.png"));
            click(
                &mut commands,
                InventoryAction::Drop,
                &actions,
                window,
                camera,
            );
            smoke.timer = now;
            smoke.phase = 8;
        }
        8 => {
            if now - smoke.timer > 0.3 {
                assert_eq!(
                    inventory.count(ItemKind::Battery),
                    1,
                    "drop did not remove exactly one battery"
                );
                keys.press(KeyCode::KeyI);
                smoke.phase = 9;
            }
        }
        9 if *state.get() == GameState::Playing => {
            if inventory.count(ItemKind::Battery) == 2 {
                info!(
                    "INVENTORY SMOKE PASS: collected all materials, typed quantity, dropped one battery through UI, recollected it, verified pause"
                );
                smoke.done = true;
                return;
            }
            if let Some((_, InteractionTarget::Pickup(pickup))) = nearby.0 {
                if pickup.item == ItemKind::Battery {
                    keys.press(KeyCode::KeyE);
                    return;
                }
            }
            let destination = pickups
                .iter()
                .filter(|(p, _)| p.item == ItemKind::Battery)
                .min_by(|(_, a), (_, b)| {
                    a.translation
                        .distance_squared(transform.translation)
                        .total_cmp(&b.translation.distance_squared(transform.translation))
                })
                .expect("dropped stack missing")
                .1
                .translation;
            walk(&mut keys, &mut orbit, transform.translation, destination);
        }
        _ => (),
    }
}
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
