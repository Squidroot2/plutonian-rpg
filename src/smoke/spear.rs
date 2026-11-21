//! Play the spear and trident tutorials before the equipment and quest checks.
use bevy::{input::InputSystems, prelude::*, render::view::screenshot::Screenshot};

use super::inventory::{InventorySmoke, click, walk};
use crate::{
    GameState,
    camera::Orbit,
    combat::HitReaction,
    combat_feedback::AttackImpact,
    equipment::{Equipment, EquipmentSlot, WornEquipment},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind, Pickup},
    inventory_ui::InventoryAction,
    loading::PendingScene,
    menu_ui::{MenuTab, PlayerMenu},
    player::Player,
    practice_dummy::PracticeDummy,
    spear::{GRIP_SIDE, Thrust},
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
    StartMiss,
    Miss,
    WormEncounter,
    Unequip,
    SwitchToDrop,
    SelectDrop,
    Drop,
    ConfirmDrop,
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
        app.init_resource::<SpearSmoke>()
            .init_resource::<super::worm::Encounter>()
            .add_systems(
                PreUpdate,
                drive
                    .after(InputSystems)
                    .after(super::inventory::drive)
                    .before(super::equipment::drive)
                    .before(crate::ui::pause)
                    .before(crate::player::input),
            )
            .add_systems(
                PreUpdate,
                super::worm::fight
                    .after(drive)
                    .before(crate::player::input)
                    .before(crate::ui::pause),
            );
    }
}

fn drive(
    materials: Res<InventorySmoke>,
    mut smoke: ResMut<SpearSmoke>,
    (state, menu): (Res<State<GameState>>, Res<PlayerMenu>),
    time: Res<Time>,
    (mut screenshot, mut captured_at, mut testing_trident): (
        Local<Option<Entity>>,
        Local<f32>,
        Local<bool>,
    ),
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    nearby: Res<Nearby>,
    thrust: Res<Thrust>,
    player: Query<(&Transform, &Player)>,
    (dummy, captures, impacts, windows): (
        Query<(&HitReaction, &Transform), With<PracticeDummy>>,
        Query<(), With<Screenshot>>,
        Query<&Transform, With<AttackImpact>>,
        Query<&Window>,
    ),
    pickups: Query<(&Pickup, &Transform)>,
    worn: Query<&WornEquipment, Without<PendingScene>>,
    (actions, tabs): (Query<(Entity, &InventoryAction)>, Query<(Entity, &MenuTab)>),
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
        super::capture(commands, &format!("{screenshot_name}-{stage}"))
    };
    let now = time.elapsed_secs();
    let (transform, controller) = player
        .single()
        .expect("spear smoke requires one loaded player");
    let (window, camera) = materials
        .context
        .expect("inventory smoke must provide a window and camera");
    let (dummy, dummy_transform) = dummy
        .single()
        .expect("spear smoke requires one loaded practice dummy");
    keys.reset_all();
    buttons.reset_all();

    match *smoke {
        SpearSmoke::Collect | SpearSmoke::Recollect
            if *state.get() == GameState::Playing && !menu.is_open() =>
        {
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
        SpearSmoke::Select if menu.is_on(MenuTab::Inventory) => {
            click(&mut commands, MenuTab::Equipment, &tabs, window, camera);
        }
        SpearSmoke::Select if menu.is_on(MenuTab::Equipment) => {
            click(
                &mut commands,
                InventoryAction::SelectGear(weapon),
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
            *screenshot = Some(capture(&mut commands, "inventory"));
            *smoke = SpearSmoke::CaptureInventory;
        }
        SpearSmoke::CaptureInventory
            if screenshot.is_some_and(|entity| captures.get(entity).is_err()) =>
        {
            keys.press(KeyCode::Tab);
            *smoke = SpearSmoke::Approach;
        }
        SpearSmoke::Approach if *state.get() == GameState::Playing && !menu.is_open() => {
            let position = dummy_transform.translation + Vec3::new(-GRIP_SIDE, 0.0, -2.0);
            if transform
                .translation
                .with_y(0.0)
                .distance(position.with_y(0.0))
                > 0.22
            {
                walk(&mut keys, &mut orbit, transform.translation, position);
            } else if controller.grounded {
                orbit.yaw = std::f32::consts::PI + 0.95;
                orbit.distance = 4.5;
                buttons.press(MouseButton::Left);
                *captured_at = now;
                *smoke = SpearSmoke::Chamber;
            }
        }
        SpearSmoke::Chamber | SpearSmoke::Impact if dummy.remaining > 0.0 => {
            assert!(thrust.active());
            let impact = impacts
                .single()
                .expect("each landed dummy hit must spawn one impact");
            assert!(impact.translation.distance(dummy_transform.translation) < 0.85);
            capture(&mut commands, "hit");
            *smoke = SpearSmoke::Recovery;
        }
        SpearSmoke::Chamber | SpearSmoke::Impact if thrust.active() => {
            orbit.yaw = std::f32::consts::PI;
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::Space);
            assert!(controller.grounded);
            if now - *captured_at > 0.07 {
                assert_eq!(controller.velocity.xz(), Vec2::ZERO);
                assert!(
                    (transform.rotation * Vec3::Z).z > 0.99,
                    "the player must turn toward the dummy during the thrust"
                );
            }
            if *smoke == SpearSmoke::Chamber && now - *captured_at >= 3.0 / 24.0 {
                capture(&mut commands, "chamber");
                *smoke = SpearSmoke::Impact;
            }
        }
        SpearSmoke::Chamber | SpearSmoke::Impact if now - *captured_at > 0.3 => {
            panic!(
                "the smoke thrust did not reach the dummy: grounded {}, window focused {}, weapon {:?}",
                controller.grounded,
                windows.get(window).is_ok_and(|window| window.focused),
                equipment.get(EquipmentSlot::Weapon)
            );
        }
        SpearSmoke::Recovery if !thrust.active() && dummy.remaining == 0.0 => {
            assert!(
                impacts.is_empty(),
                "the impact must expire after its brief flash"
            );
            orbit.yaw = std::f32::consts::PI;
            *screenshot = Some(capture(&mut commands, "carry"));
            *smoke = SpearSmoke::CaptureCarry;
        }
        SpearSmoke::CaptureCarry
            if screenshot.is_some_and(|entity| captures.get(entity).is_err()) =>
        {
            orbit.yaw = 0.0;
            buttons.press(MouseButton::Left);
            *smoke = SpearSmoke::StartMiss;
        }
        SpearSmoke::StartMiss => {
            assert!(thrust.active(), "aiming away must start a real thrust");
            *smoke = SpearSmoke::Miss;
        }
        SpearSmoke::Miss => {
            assert_eq!(dummy.remaining, 0.0, "aiming away must miss the dummy");
            assert!(
                impacts.is_empty(),
                "a missed thrust must not spawn an impact"
            );
            if !thrust.active() {
                if *testing_trident {
                    *smoke = SpearSmoke::WormEncounter;
                } else {
                    keys.press(KeyCode::KeyI);
                    *smoke = SpearSmoke::Unequip;
                }
            }
        }
        SpearSmoke::Unequip if menu.is_on(MenuTab::Inventory) => {
            click(&mut commands, MenuTab::Equipment, &tabs, window, camera);
        }
        SpearSmoke::Unequip if menu.is_on(MenuTab::Equipment) => {
            click(
                &mut commands,
                InventoryAction::Unequip(EquipmentSlot::Weapon),
                &actions,
                window,
                camera,
            );
            *smoke = SpearSmoke::SwitchToDrop;
        }
        SpearSmoke::SwitchToDrop => {
            click(&mut commands, MenuTab::Inventory, &tabs, window, camera);
            *smoke = SpearSmoke::SelectDrop;
        }
        SpearSmoke::SelectDrop if menu.is_on(MenuTab::Inventory) => {
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
            *smoke = SpearSmoke::ConfirmDrop;
        }
        SpearSmoke::ConfirmDrop => {
            click(
                &mut commands,
                InventoryAction::ConfirmDrop,
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
