//! Exercises pickup, quantities, and dropping through the live inventory UI.
use crate::{
    GameState,
    camera::{FollowCamera, Orbit},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind, Pickup},
    player::Player,
    player::PlayerRead,
    ui::inventory_ui::{InventoryAction, InventoryRow, InventoryUiState},
    ui::menu_ui::{MenuTab, PlayerMenu},
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
    render::view::screenshot::Screenshot,
    window::WindowRef,
};
use std::time::Duration;

pub struct InventorySmokePlugin;
#[derive(Resource)]
pub struct InventorySmoke {
    pub done: bool,
    pub context: Option<(Entity, Entity)>,
    phase: u8,
    collected: usize,
    opened_at: Duration,
    screenshot: Option<Entity>,
    position: Vec3,
    yaw: f32,
}
impl InventorySmoke {
    pub fn phase(&self) -> u8 {
        self.phase
    }
}

impl Plugin for InventorySmokePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(InventorySmoke {
            done: false,
            context: None,
            phase: 1,
            collected: 0,
            opened_at: Duration::ZERO,
            screenshot: None,
            position: Vec3::ZERO,
            yaw: 0.0,
        })
        .add_systems(
            PreUpdate,
            drive.after(InputSystems).before(crate::ui::pause),
        );
    }
}
pub(crate) fn click<T: Component + PartialEq>(
    commands: &mut Commands,
    action: T,
    actions: &Query<(Entity, &T)>,
    window: Entity,
    camera: Entity,
) {
    let entity = actions
        .iter()
        .find(|(_, kind)| **kind == action)
        .expect("missing smoke test UI action")
        .0;
    commands.trigger(pointer_click(entity, window, camera));
}

pub(crate) fn pointer_click(entity: Entity, window: Entity, camera: Entity) -> PointerClick {
    PointerClick {
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
    }
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
    (state, menu): (Res<State<GameState>>, Res<PlayerMenu>),
    fixed: Res<Time<Fixed>>,
    captures: Query<(), With<Screenshot>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    orbit: ResMut<Orbit>,
    inventory: PlayerRead<Inventory>,
    ui: Res<InventoryUiState>,
    nearby: Res<Nearby>,
    player: Query<(&Transform, &Player)>,
    pickups: Query<(&Pickup, &Transform)>,
    actions: Query<(Entity, &InventoryAction)>,
    windows: Query<Entity, With<Window>>,
    cameras: Query<Entity, With<FollowCamera>>,
    keyboard: MessageWriter<KeyboardInput>,
    commands: Commands,
) {
    if smoke.done || matches!(state.get(), GameState::Title | GameState::Loading) {
        return;
    }
    keys.reset_all();
    let Ok((transform, controller)) = player.single() else {
        return;
    };
    let window = windows
        .single()
        .expect("the inventory smoke test requires exactly one game window");
    let camera = cameras
        .single()
        .expect("the inventory smoke test requires exactly one follow camera");
    smoke.context = Some((window, camera));
    match smoke.phase {
        1 | 10 | 11 => collect_materials(
            smoke, keys, orbit, inventory, nearby, pickups, transform, controller,
        ),
        2 | 3 | 4 | 43 | 44 | 41 | 45 | 42 => drop_battery(
            smoke, state, menu, fixed, keys, orbit, inventory, ui, actions, commands, transform,
            controller, window, camera,
        ),
        5..=9 => recollect_battery(
            smoke, state, menu, captures, keys, orbit, inventory, ui, nearby, pickups, actions,
            keyboard, commands, transform, window, camera,
        ),
        _ => {}
    }
}

fn collect_materials(
    mut smoke: ResMut<InventorySmoke>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    inventory: PlayerRead<Inventory>,
    nearby: Res<Nearby>,
    pickups: Query<(&Pickup, &Transform)>,
    transform: &Transform,
    controller: &Player,
) {
    match smoke.phase {
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
                    smoke.phase = 10;
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
        10 if controller.grounded => {
            smoke.position = transform.translation;
            keys.press(KeyCode::Space);
            smoke.phase = 11;
        }
        11 if !controller.grounded && transform.translation.y > smoke.position.y + 0.12 => {
            keys.press(KeyCode::KeyI);
            smoke.phase = 2;
        }
        _ => {}
    }
}

fn drop_battery(
    mut smoke: ResMut<InventorySmoke>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    fixed: Res<Time<Fixed>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    orbit: ResMut<Orbit>,
    inventory: PlayerRead<Inventory>,
    ui: Res<InventoryUiState>,
    actions: Query<(Entity, &InventoryAction)>,
    mut commands: Commands,
    transform: &Transform,
    controller: &Player,
    window: Entity,
    camera: Entity,
) {
    match smoke.phase {
        2 if menu.is_on(MenuTab::Inventory) => {
            smoke.position = transform.translation;
            smoke.yaw = orbit.yaw;
            smoke.opened_at = fixed.elapsed();
            smoke.phase = 3;
        }
        3 => {
            assert_eq!(*state.get(), GameState::Playing);
            assert!(menu.is_on(MenuTab::Inventory));
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::Space);
            keys.press(KeyCode::KeyE);
            assert!(
                smoke.position.xz().distance(transform.translation.xz()) < 0.01,
                "inventory leaked walking input"
            );
            assert_eq!(smoke.yaw, orbit.yaw);
            if controller.grounded && fixed.elapsed() - smoke.opened_at >= fixed.timestep() * 3 {
                assert!(
                    transform.translation.y < smoke.position.y - 0.02,
                    "gravity must land the player while inventory is open"
                );
                click(
                    &mut commands,
                    InventoryAction::Select(InventoryRow::Stack(ItemKind::Battery)),
                    &actions,
                    window,
                    camera,
                );
                smoke.phase = 4;
            }
        }
        4 => {
            assert!(!ui.drop_is_open(), "the quantity prompt must start hidden");
            assert_eq!(ui.selected_item(), Some(ItemKind::Battery));
            keys.press(KeyCode::KeyS);
            smoke.phase = 43;
        }
        43 => {
            assert_eq!(ui.selected_item(), Some(ItemKind::Scrap));
            keys.press(KeyCode::KeyW);
            smoke.phase = 44;
        }
        44 => {
            assert_eq!(ui.selected_item(), Some(ItemKind::Battery));
            keys.press(KeyCode::KeyQ);
            smoke.phase = 41;
        }
        41 => {
            assert!(ui.drop_is_open(), "Q must open the quantity prompt");
            assert_eq!(inventory.count(ItemKind::Battery), 2);
            keys.press(KeyCode::KeyS);
            smoke.phase = 45;
        }
        45 => {
            assert_eq!(ui.selected_item(), Some(ItemKind::Battery));
            assert!(ui.drop_is_open());
            keys.press(KeyCode::Escape);
            smoke.phase = 42;
        }
        42 => {
            assert!(menu.is_on(MenuTab::Inventory));
            assert!(!ui.drop_is_open());
            click(
                &mut commands,
                InventoryAction::Drop,
                &actions,
                window,
                camera,
            );
            smoke.phase = 5;
        }
        _ => {}
    }
}

fn recollect_battery(
    mut smoke: ResMut<InventorySmoke>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    captures: Query<(), With<Screenshot>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    inventory: PlayerRead<Inventory>,
    ui: Res<InventoryUiState>,
    nearby: Res<Nearby>,
    pickups: Query<(&Pickup, &Transform)>,
    actions: Query<(Entity, &InventoryAction)>,
    mut keyboard: MessageWriter<KeyboardInput>,
    mut commands: Commands,
    transform: &Transform,
    window: Entity,
    camera: Entity,
) {
    match smoke.phase {
        5 => {
            assert!(ui.drop_is_open(), "the footer button must open the prompt");
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
            smoke.screenshot = Some(super::capture(&mut commands, "inventory"));
            click(
                &mut commands,
                InventoryAction::ConfirmDrop,
                &actions,
                window,
                camera,
            );
            smoke.phase = 8;
        }
        8 => {
            if smoke
                .screenshot
                .is_some_and(|entity| captures.get(entity).is_err())
            {
                assert_eq!(
                    inventory.count(ItemKind::Battery),
                    1,
                    "drop did not remove exactly one battery"
                );
                keys.press(KeyCode::KeyI);
                smoke.phase = 9;
            }
        }
        9 if *state.get() == GameState::Playing && !menu.is_open() => {
            if inventory.count(ItemKind::Battery) == 2 {
                info!(
                    "INVENTORY SMOKE PASS: collected all materials, navigated with W/S, typed quantity, dropped one battery through UI, recollected it, verified live physics and captured input"
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
        _ => {}
    }
}
