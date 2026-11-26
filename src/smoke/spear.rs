//! Play the spear and trident tutorials before the equipment and quest checks.
use bevy::{input::InputSystems, prelude::*, render::view::screenshot::Screenshot};

use super::inventory::{InventorySmoke, click, pointer_click, walk};
use crate::{
    GameState,
    camera::Orbit,
    combat::HitReaction,
    combat_feedback::AttackImpact,
    equipment::{Equipment, EquipmentSlot, WornEquipment},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind, Pickup},
    loading::PendingScene,
    player::Player,
    player::PlayerRead,
    practice_dummy::PracticeDummy,
    ui::inventory_ui::{InventoryAction, InventoryRow},
    ui::menu_ui::{MenuTab, PlayerMenu},
    weapon::{GRIP_SIDE, WeaponAttack, WeaponStance},
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
    Holster,
    CaptureHolstered,
    Draw,
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

pub(super) fn drive(
    materials: Res<InventorySmoke>,
    smoke: ResMut<SpearSmoke>,
    (state, menu): (Res<State<GameState>>, Res<PlayerMenu>),
    time: Res<Time>,
    (screenshot, captured_at, testing_trident, holstered_weapon, turn_armed): (
        Local<Option<Entity>>,
        Local<f32>,
        Local<bool>,
        Local<Option<Entity>>,
        Local<bool>,
    ),
    inventory: PlayerRead<Inventory>,
    equipment: PlayerRead<Equipment>,
    nearby: Res<Nearby>,
    player: Query<(&Transform, &Player, &WeaponStance, &WeaponAttack)>,
    (dummy, captures, impacts): (
        Query<(&HitReaction, &Transform), With<PracticeDummy>>,
        Query<(), With<Screenshot>>,
        Query<&Transform, With<AttackImpact>>,
    ),
    pickups: Query<(&Pickup, &Transform)>,
    (worn, names): (
        Query<(Entity, &WornEquipment, &ChildOf), Without<PendingScene>>,
        Query<&Name>,
    ),
    (actions, tabs): (Query<(Entity, &InventoryAction)>, Query<(Entity, &MenuTab)>),
    (mut keys, mut buttons, orbit): (
        ResMut<ButtonInput<KeyCode>>,
        ResMut<ButtonInput<MouseButton>>,
        ResMut<Orbit>,
    ),
    commands: Commands,
) {
    if !materials.done || smoke.done() {
        return;
    }
    let (weapon, screenshot_name) = if *testing_trident {
        (ItemKind::Trident, "trident")
    } else {
        (ItemKind::FieldSpear, "spear")
    };
    let capture = |commands: &mut Commands, stage: &str| {
        super::capture(commands, &format!("{screenshot_name}-{stage}"))
    };
    let now = time.elapsed_secs();
    let (transform, controller, stance, thrust) = player
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
        SpearSmoke::Collect
        | SpearSmoke::Recollect
        | SpearSmoke::Select
        | SpearSmoke::Equip
        | SpearSmoke::Close
        | SpearSmoke::CaptureInventory
        | SpearSmoke::Holster
        | SpearSmoke::CaptureHolstered
        | SpearSmoke::Draw => check_spear_equipment(
            smoke,
            state,
            menu,
            screenshot,
            testing_trident,
            holstered_weapon,
            inventory,
            equipment,
            nearby,
            captures,
            pickups,
            worn,
            names,
            actions,
            tabs,
            keys,
            orbit,
            commands,
            weapon,
            capture,
            transform,
            stance,
            window,
            camera,
        ),
        SpearSmoke::Approach
        | SpearSmoke::Chamber
        | SpearSmoke::Impact
        | SpearSmoke::Recovery
        | SpearSmoke::CaptureCarry
        | SpearSmoke::StartMiss
        | SpearSmoke::Miss => check_spear_combat(
            smoke,
            state,
            menu,
            screenshot,
            captured_at,
            turn_armed,
            testing_trident,
            holstered_weapon,
            equipment,
            dummy,
            captures,
            impacts,
            worn,
            names,
            keys,
            buttons,
            orbit,
            commands,
            capture,
            now,
            transform,
            controller,
            stance,
            thrust,
            dummy_transform,
        ),
        SpearSmoke::Unequip
        | SpearSmoke::SwitchToDrop
        | SpearSmoke::SelectDrop
        | SpearSmoke::Drop
        | SpearSmoke::ConfirmDrop
        | SpearSmoke::CloseBag => check_spear_drop(
            smoke, menu, inventory, worn, actions, tabs, keys, orbit, commands, weapon, window,
            camera,
        ),
        SpearSmoke::WormEncounter | SpearSmoke::Done => {}
    }
}

fn check_spear_equipment(
    mut smoke: ResMut<SpearSmoke>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    mut screenshot: Local<Option<Entity>>,
    mut testing_trident: Local<bool>,
    mut holstered_weapon: Local<Option<Entity>>,
    inventory: PlayerRead<Inventory>,
    equipment: PlayerRead<Equipment>,
    nearby: Res<Nearby>,
    captures: Query<(), With<Screenshot>>,
    pickups: Query<(&Pickup, &Transform)>,
    worn: Query<(Entity, &WornEquipment, &ChildOf), Without<PendingScene>>,
    names: Query<&Name>,
    actions: Query<(Entity, &InventoryAction)>,
    tabs: Query<(Entity, &MenuTab)>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    weapon: ItemKind,
    capture: impl Fn(&mut Commands, &str) -> Entity,
    transform: &Transform,
    stance: &WeaponStance,
    window: Entity,
    camera: Entity,
) {
    match *smoke {
        SpearSmoke::Collect | SpearSmoke::Recollect
            if *state.get() == GameState::Playing && !menu.is_open() =>
        {
            if inventory.count(weapon) == 1 {
                if *smoke == SpearSmoke::Recollect {
                    assert_eq!(equipment.get(EquipmentSlot::MainHand), None);
                    info!(
                        "SPEAR SMOKE PASS: collected, equipped, hit, missed, unequipped, dropped and recollected {}",
                        weapon
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
                InventoryAction::ToggleSlot(EquipmentSlot::MainHand),
                &actions,
                window,
                camera,
            );
            *smoke = SpearSmoke::Equip;
        }
        SpearSmoke::Equip => {
            click(
                &mut commands,
                InventoryAction::EquipFromSlot(EquipmentSlot::MainHand, weapon),
                &actions,
                window,
                camera,
            );
            *smoke = SpearSmoke::Close;
        }
        SpearSmoke::Close if worn.iter().any(|(_, piece, _)| piece.item == weapon) => {
            assert_eq!(equipment.get(EquipmentSlot::MainHand), Some(weapon));
            assert_eq!(inventory.count(weapon), 0);
            *screenshot = Some(capture(&mut commands, "inventory"));
            *smoke = SpearSmoke::CaptureInventory;
        }
        SpearSmoke::CaptureInventory
            if screenshot.is_some_and(|entity| captures.get(entity).is_err()) =>
        {
            keys.press(KeyCode::Tab);
            *smoke = SpearSmoke::Holster;
        }
        SpearSmoke::Holster if !menu.is_open() => {
            *holstered_weapon = worn
                .iter()
                .find_map(|(entity, piece, _)| (piece.item == weapon).then_some(entity));
            assert!(holstered_weapon.is_some());
            keys.press(KeyCode::KeyX);
            *smoke = SpearSmoke::CaptureHolstered;
        }
        SpearSmoke::CaptureHolstered => {
            assert_eq!(*stance, WeaponStance::Holstered);
            let (_, _, parent) = worn
                .get(holstered_weapon.unwrap())
                .expect("holstering keeps the worn polearm scene");
            assert_eq!(
                names
                    .get(parent.parent())
                    .expect("the holstered polearm must be parented to a named socket")
                    .as_str(),
                "polearm_holster"
            );
            *screenshot = Some(capture(&mut commands, "holstered"));
            *smoke = SpearSmoke::Draw;
        }
        SpearSmoke::Draw if screenshot.is_some_and(|entity| captures.get(entity).is_err()) => {
            keys.press(KeyCode::KeyX);
            *smoke = SpearSmoke::Approach;
        }
        _ => {}
    }
}

fn check_spear_combat(
    mut smoke: ResMut<SpearSmoke>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    mut screenshot: Local<Option<Entity>>,
    mut captured_at: Local<f32>,
    mut turn_armed: Local<bool>,
    testing_trident: Local<bool>,
    holstered_weapon: Local<Option<Entity>>,
    equipment: PlayerRead<Equipment>,
    dummy: &HitReaction,
    captures: Query<(), With<Screenshot>>,
    impacts: Query<&Transform, With<AttackImpact>>,
    worn: Query<(Entity, &WornEquipment, &ChildOf), Without<PendingScene>>,
    names: Query<&Name>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    capture: impl Fn(&mut Commands, &str) -> Entity,
    now: f32,
    transform: &Transform,
    controller: &Player,
    stance: &WeaponStance,
    thrust: &WeaponAttack,
    dummy_transform: &Transform,
) {
    let mut turn_checked = false;
    if matches!(*smoke, SpearSmoke::Chamber | SpearSmoke::Impact)
        && (thrust.active() || dummy.remaining > 0.0)
    {
        orbit.yaw = std::f32::consts::PI;
        if *turn_armed {
            assert!(
                (transform.rotation * Vec3::Z).z > 0.99,
                "the player must turn toward the dummy during the thrust"
            );
            turn_checked = true;
        } else {
            *turn_armed = true;
        }
    }
    match *smoke {
        SpearSmoke::Approach if *state.get() == GameState::Playing && !menu.is_open() => {
            assert_eq!(*stance, WeaponStance::Drawn);
            let (_, _, parent) = worn
                .get(holstered_weapon.unwrap())
                .expect("drawing keeps the worn polearm scene");
            assert_eq!(
                names
                    .get(parent.parent())
                    .expect("the drawn polearm must be parented to a named socket")
                    .as_str(),
                "spear_grip"
            );
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
                *turn_armed = false;
                *smoke = SpearSmoke::Chamber;
            }
        }
        SpearSmoke::Chamber | SpearSmoke::Impact if dummy.remaining > 0.0 && turn_checked => {
            assert!(thrust.active());
            let impact = impacts
                .single()
                .expect("each landed dummy hit must spawn one impact");
            assert!(impact.translation.distance(dummy_transform.translation) < 0.85);
            capture(&mut commands, "hit");
            *smoke = SpearSmoke::Recovery;
        }
        SpearSmoke::Chamber | SpearSmoke::Impact if thrust.active() => {
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::Space);
            assert!(controller.grounded);
            if now - *captured_at > 0.07 {
                assert_eq!(controller.velocity.xz(), Vec2::ZERO);
            }
            if *smoke == SpearSmoke::Chamber && now - *captured_at >= 3.0 / 24.0 {
                capture(&mut commands, "chamber");
                *smoke = SpearSmoke::Impact;
            }
        }
        SpearSmoke::Chamber | SpearSmoke::Impact if now - *captured_at > 0.3 => {
            panic!(
                "the smoke thrust did not reach the dummy: grounded {}, weapon {:?}",
                controller.grounded,
                equipment.get(EquipmentSlot::MainHand)
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
        _ => {}
    }
}

fn check_spear_drop(
    mut smoke: ResMut<SpearSmoke>,
    menu: Res<PlayerMenu>,
    inventory: PlayerRead<Inventory>,
    worn: Query<(Entity, &WornEquipment, &ChildOf), Without<PendingScene>>,
    actions: Query<(Entity, &InventoryAction)>,
    tabs: Query<(Entity, &MenuTab)>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    weapon: ItemKind,
    window: Entity,
    camera: Entity,
) {
    match *smoke {
        SpearSmoke::Unequip if menu.is_on(MenuTab::Inventory) => {
            click(&mut commands, MenuTab::Equipment, &tabs, window, camera);
        }
        SpearSmoke::Unequip if menu.is_on(MenuTab::Equipment) => {
            click(
                &mut commands,
                InventoryAction::ToggleSlot(EquipmentSlot::MainHand),
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
            let (row, _) = actions.iter().find(|(_, action)| {
                matches!(action, InventoryAction::Select(InventoryRow::BagGear(item, _)) if *item == weapon)
            }).expect("unequipped spear has a bag row");
            commands.trigger(pointer_click(row, window, camera));
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
            assert!(!worn.iter().any(|(_, piece, _)| piece.item == weapon));
            keys.press(KeyCode::KeyI);
            orbit.distance = 5.5;
            *smoke = SpearSmoke::Recollect;
        }
        _ => {}
    }
}
