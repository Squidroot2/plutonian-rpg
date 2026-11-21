//! Play the spear and trident tutorials before the equipment and quest checks.
use bevy::{
    input::InputSystems,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

use crate::{
    GameState,
    camera::Orbit,
    equipment::{Equipment, EquipmentSlot, WornEquipment},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind, Pickup},
    inventory_smoke::{InventorySmoke, click, walk},
    inventory_ui::InventoryAction,
    loading::PendingScene,
    player::Player,
    spear::{DUMMY_POSITION, GRIP_SIDE, PracticeDummy, Thrust},
};

pub struct SpearSmokePlugin;
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub enum SpearSmoke {
    #[default]
    Collect,
    Select,
    Equip,
    Close,
    CaptureInventory,
    Approach,
    Chamber,
    Impact,
    Recovery,
    CaptureCarry,
    Miss,
    Unequip,
    SelectDrop,
    Drop,
    CloseBag,
    Recollect,
    Done,
}

impl SpearSmoke {
    pub fn done(&self) -> bool {
        *self == Self::Done
    }
}
impl Plugin for SpearSmokePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpearSmoke>().add_systems(
            PreUpdate,
            drive
                .after(InputSystems)
                .after(crate::inventory_smoke::drive)
                .before(crate::equipment_smoke::drive)
                .before(crate::ui::pause)
                .before(crate::player::input),
        );
    }
}

fn drive(
    materials: Res<InventorySmoke>,
    mut smoke: ResMut<SpearSmoke>,
    state: Res<State<GameState>>,
    time: Res<Time>,
    (mut deadline, mut captured_at, mut testing_trident): (
        Local<Option<f32>>,
        Local<f32>,
        Local<bool>,
    ),
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    nearby: Res<Nearby>,
    thrust: Res<Thrust>,
    player: Query<(&Transform, &Player)>,
    dummy: Query<&PracticeDummy>,
    pickups: Query<(&Pickup, &Transform)>,
    worn: Query<&WornEquipment, Without<PendingScene>>,
    actions: Query<(Entity, &InventoryAction)>,
    (mut keys, mut buttons, mut orbit): (
        ResMut<ButtonInput<KeyCode>>,
        ResMut<ButtonInput<MouseButton>>,
        ResMut<Orbit>,
    ),
    mut commands: Commands,
) {
    if !materials.done || smoke.done() {
        return;
    }
    let (weapon, screenshot_name) = if *testing_trident {
        (ItemKind::Trident, "trident")
    } else {
        (ItemKind::FieldSpear, "spear")
    };
    let capture = |commands: &mut Commands, stage| {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!(
                "/tmp/plutonian-{screenshot_name}-{stage}.png"
            )));
    };
    let now = time.elapsed_secs();
    assert!(
        now < *deadline.get_or_insert(now + 25.0),
        "{} smoke timed out in {:?}",
        weapon.name(),
        *smoke
    );
    let (transform, controller) = player
        .single()
        .expect("spear smoke requires one loaded player");
    let (window, camera) = materials
        .context
        .expect("inventory smoke must provide a window and camera");
    let dummy = dummy
        .single()
        .expect("spear smoke requires one loaded practice dummy");
    keys.reset_all();
    buttons.reset_all();

    match *smoke {
        SpearSmoke::Collect | SpearSmoke::Recollect if *state.get() == GameState::Playing => {
            if inventory.count(weapon) == 1 {
                if *smoke == SpearSmoke::Recollect {
                    assert_eq!(equipment.get(EquipmentSlot::Weapon), None);
                    info!(
                        "SPEAR SMOKE PASS: collected, equipped, hit, missed, unequipped, dropped and recollected {}",
                        weapon.name()
                    );
                    if *testing_trident {
                        *smoke = SpearSmoke::Done;
                    } else {
                        *testing_trident = true;
                        *deadline = None;
                        *smoke = SpearSmoke::Collect;
                    }
                } else {
                    keys.press(KeyCode::KeyI);
                    *smoke = SpearSmoke::Select;
                }
            } else if matches!(nearby.0, Some((_, InteractionTarget::Pickup(p))) if p.item == weapon)
            {
                keys.press(KeyCode::KeyE);
            } else {
                let (_, target) = pickups
                    .iter()
                    .find(|(p, _)| p.item == weapon)
                    .expect("the uncollected weapon must have a world pickup");
                walk(
                    &mut keys,
                    &mut orbit,
                    transform.translation,
                    target.translation,
                );
            }
        }
        SpearSmoke::Select if *state.get() == GameState::Inventory => {
            click(
                &mut commands,
                InventoryAction::Select(weapon),
                &actions,
                window,
                camera,
            );
            *smoke = SpearSmoke::Equip;
        }
        SpearSmoke::Equip => {
            click(
                &mut commands,
                InventoryAction::Equip,
                &actions,
                window,
                camera,
            );
            *smoke = SpearSmoke::Close;
        }
        SpearSmoke::Close if worn.iter().any(|piece| piece.item == weapon) => {
            assert_eq!(equipment.get(EquipmentSlot::Weapon), Some(weapon));
            assert_eq!(inventory.count(weapon), 0);
            capture(&mut commands, "inventory");
            *captured_at = now;
            *smoke = SpearSmoke::CaptureInventory;
        }
        SpearSmoke::CaptureInventory if now - *captured_at > 0.2 => {
            keys.press(KeyCode::KeyI);
            *smoke = SpearSmoke::Approach;
        }
        SpearSmoke::Approach if *state.get() == GameState::Playing => {
            let position = DUMMY_POSITION + Vec3::new(-GRIP_SIDE, 0.0, -2.0);
            if transform
                .translation
                .with_y(0.0)
                .distance(position.with_y(0.0))
                > 0.22
            {
                walk(&mut keys, &mut orbit, transform.translation, position);
            } else if controller.grounded {
                orbit.yaw = std::f32::consts::PI;
                orbit.distance = 4.5;
                buttons.press(MouseButton::Left);
                *captured_at = now;
                *smoke = SpearSmoke::Chamber;
            }
        }
        SpearSmoke::Chamber | SpearSmoke::Impact if dummy.remaining > 0.0 => {
            assert!(thrust.active());
            capture(&mut commands, "hit");
            *smoke = SpearSmoke::Recovery;
        }
        SpearSmoke::Chamber | SpearSmoke::Impact if thrust.active() => {
            orbit.yaw = std::f32::consts::PI + 0.95;
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::Space);
            assert!(controller.grounded);
            if now - *captured_at > 0.07 {
                assert_eq!(controller.velocity.xz(), Vec2::ZERO);
            }
            assert!(
                transform.forward().z < -0.99,
                "the horizontal aim must stay locked after the camera moves"
            );
            if *smoke == SpearSmoke::Chamber && now - *captured_at >= 3.0 / 24.0 {
                capture(&mut commands, "chamber");
                *smoke = SpearSmoke::Impact;
            }
        }
        SpearSmoke::Recovery if !thrust.active() && dummy.remaining == 0.0 => {
            orbit.yaw = std::f32::consts::PI;
            capture(&mut commands, "carry");
            *captured_at = now;
            *smoke = SpearSmoke::CaptureCarry;
        }
        SpearSmoke::CaptureCarry if now - *captured_at > 0.2 => {
            orbit.yaw = 0.0;
            buttons.press(MouseButton::Left);
            *smoke = SpearSmoke::Miss;
        }
        SpearSmoke::Miss => {
            assert_eq!(dummy.remaining, 0.0, "aiming away must miss the dummy");
            if !thrust.active() {
                keys.press(KeyCode::KeyI);
                *smoke = SpearSmoke::Unequip;
            }
        }
        SpearSmoke::Unequip if *state.get() == GameState::Inventory => {
            click(
                &mut commands,
                InventoryAction::Unequip(EquipmentSlot::Weapon),
                &actions,
                window,
                camera,
            );
            *smoke = SpearSmoke::SelectDrop;
        }
        SpearSmoke::SelectDrop => {
            assert_eq!(inventory.count(weapon), 1);
            click(
                &mut commands,
                InventoryAction::Select(weapon),
                &actions,
                window,
                camera,
            );
            *smoke = SpearSmoke::Drop;
        }
        SpearSmoke::Drop => {
            click(
                &mut commands,
                InventoryAction::Drop,
                &actions,
                window,
                camera,
            );
            *smoke = SpearSmoke::CloseBag;
        }
        SpearSmoke::CloseBag => {
            assert_eq!(
                inventory.count(weapon),
                0,
                "unequipped weapon must fit the drop clearance"
            );
            assert!(!worn.iter().any(|piece| piece.item == weapon));
            keys.press(KeyCode::KeyI);
            orbit.distance = 5.5;
            *smoke = SpearSmoke::Recollect;
        }
        _ => (),
    }
}
