//! Opt-in equipment playthrough using the live inventory UI and animated player rig.
use crate::{
    GameState,
    camera::Orbit,
    equipment::{Equipment, EquipmentSlot, WornEquipment},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind, Pickup},
    inventory_smoke::{InventorySmoke, click, walk},
    inventory_ui::{InventoryAction, InventoryScreen},
    loading::PendingScene,
    player::Player,
    quest::Quest,
};
use bevy::{
    ecs::system::SystemParam,
    input::InputSystems,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use std::time::Instant;

pub struct EquipmentSmokePlugin;
#[derive(Resource)]
struct EquipmentSmoke {
    stage: u16,
    heavy: bool,
    timer: f32,
    started: Instant,
    screen_size: Option<(Val, Val)>,
}
#[derive(SystemParam)]
struct View<'w, 's> {
    screen: Query<'w, 's, &'static mut Node, With<InventoryScreen>>,
    player: Query<'w, 's, (Entity, &'static Transform, &'static Player), With<Player>>,
    pickups: Query<'w, 's, (&'static Pickup, &'static Transform)>,
    actions: Query<'w, 's, (Entity, &'static InventoryAction)>,
    worn: Query<
        'w,
        's,
        (
            &'static WornEquipment,
            &'static ChildOf,
            &'static GlobalTransform,
            Option<&'static PendingScene>,
        ),
    >,
    nodes: Query<'w, 's, (&'static Name, &'static GlobalTransform)>,
    parents: Query<'w, 's, &'static ChildOf>,
}
impl Plugin for EquipmentSmokePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EquipmentSmoke {
            stage: 0,
            heavy: false,
            timer: 0.0,
            started: Instant::now(),
            screen_size: None,
        })
        .add_systems(
            PreUpdate,
            drive
                .after(InputSystems)
                .after(crate::inventory_smoke::drive)
                .before(crate::smoke::drive)
                .before(crate::ui::pause),
        );
    }
}
fn collect(
    item: ItemKind,
    bag: &Inventory,
    nearby: &Nearby,
    view: &View,
    keys: &mut ButtonInput<KeyCode>,
    orbit: &mut Orbit,
    position: Vec3,
) -> bool {
    if bag.count(item) > 0 {
        return true;
    }
    if matches!(nearby.0, Some((_,InteractionTarget::Pickup(pickup))) if pickup.item == item) {
        keys.press(KeyCode::KeyE);
        return false;
    }
    let target = view
        .pickups
        .iter()
        .filter(|(p, _)| p.item == item)
        .min_by(|(_, a), (_, b)| {
            a.translation
                .distance_squared(position)
                .total_cmp(&b.translation.distance_squared(position))
        })
        .expect("each equipment smoke pickup must remain in the world until collected")
        .1
        .translation;
    walk(keys, orbit, position, target);
    false
}
fn check_worn(view: &View, equipment: &Equipment, player: Entity) -> bool {
    for slot in EquipmentSlot::ALL {
        let roots = view
            .worn
            .iter()
            .filter(|(piece, _, _, _)| piece.slot == slot)
            .collect::<Vec<_>>();
        if let Some(item) = equipment.get(slot) {
            if roots.len() != 1 || roots[0].3.is_some() {
                return false;
            }
            let (piece, parent, transform, _) = roots[0];
            assert_eq!(piece.item, item, "worn model does not match its slot");
            let (name, bone_transform) = view
                .nodes
                .get(parent.parent())
                .expect("worn equipment must be parented to a named rig bone");
            assert_eq!(name.as_str(), slot.bone());
            assert!(
                view.parents
                    .iter_ancestors(parent.parent())
                    .any(|entity| entity == player),
                "equipment was attached to an NPC instead of the player"
            );
            assert!(
                transform
                    .translation()
                    .distance(bone_transform.translation())
                    < 0.001,
                "worn model origin must follow its animated bone"
            );
        } else if !roots.is_empty() {
            return false;
        }
    }
    true
}
fn drive(
    mut smoke: ResMut<EquipmentSmoke>,
    mut inventory_smoke: ResMut<InventorySmoke>,
    state: Res<State<GameState>>,
    time: Res<Time>,
    quest: Res<Quest>,
    bag: Res<Inventory>,
    equipment: Res<Equipment>,
    nearby: Res<Nearby>,
    mut view: View,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    mut last_log: Local<f32>,
) {
    if !inventory_smoke.done || inventory_smoke.equipment_done {
        return;
    }
    let Ok((player, transform, controller)) = view.player.single() else {
        return;
    };
    if inventory_smoke.equipment_ready && !inventory_smoke.equipment_busy {
        if *quest == Quest::FindCore && transform.translation.z < -109.0 {
            inventory_smoke.equipment_busy = true;
            smoke.heavy = true;
            smoke.stage = 100;
        } else {
            return;
        }
    }
    keys.reset_all();
    assert!(
        smoke.started.elapsed().as_secs() < 180,
        "equipment smoke timed out at stage {}",
        smoke.stage
    );
    let now = time.elapsed_secs();
    if now - *last_log > 3.0 {
        info!(
            "EQUIPMENT SMOKE: stage {}, position {:?}, defense {}",
            smoke.stage,
            transform.translation,
            equipment.total_defense()
        );
        *last_log = now;
    }
    let (window, camera) = inventory_smoke
        .context
        .expect("completed inventory smoke must provide the test window and camera");
    macro_rules! click {
        ($action:expr) => {
            click(&mut commands, $action, &view.actions, window, camera)
        };
    }
    macro_rules! photo {
        ($name:expr) => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("/tmp/plutonian-{}.png", $name)));
        };
    }
    match smoke.stage {
        0 => {
            if collect(
                ItemKind::FieldArmor,
                &bag,
                &nearby,
                &view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                smoke.stage = 1;
            }
        }
        1 => {
            if collect(
                ItemKind::FieldHelmet,
                &bag,
                &nearby,
                &view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                keys.press(KeyCode::KeyI);
                smoke.stage = 2;
            }
        }
        2 if *state.get() == GameState::Inventory => {
            click!(InventoryAction::Select(ItemKind::FieldArmor));
            smoke.stage = 3;
        }
        3 => {
            click!(InventoryAction::Equip);
            smoke.stage = 4;
        }
        4 => {
            assert_eq!(
                equipment.get(EquipmentSlot::Body),
                Some(ItemKind::FieldArmor)
            );
            click!(InventoryAction::Select(ItemKind::FieldHelmet));
            smoke.stage = 5;
        }
        5 => {
            click!(InventoryAction::Equip);
            smoke.timer = now;
            smoke.stage = 6;
        }
        6 if now - smoke.timer > 0.3 && check_worn(&view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 3);
            photo!("field-equipment-inventory");
            smoke.timer = now;
            smoke.stage = 61;
        }
        61 if now - smoke.timer > 0.2 => {
            let mut screen = view
                .screen
                .single_mut()
                .expect("equipment layout checks require one inventory screen");
            smoke.screen_size = Some((screen.width, screen.height));
            screen.width = px(640);
            screen.height = px(704);
            smoke.timer = now;
            smoke.stage = 62;
        }
        62 if now - smoke.timer > 0.3 => {
            photo!("field-equipment-inventory-narrow");
            smoke.timer = now;
            smoke.stage = 63;
        }
        63 if now - smoke.timer > 0.2 => {
            let (width, height) = smoke
                .screen_size
                .take()
                .expect("the narrow layout check must preserve the original inventory dimensions");
            let mut screen = view
                .screen
                .single_mut()
                .expect("equipment layout checks require one inventory screen");
            screen.width = width;
            screen.height = height;
            click!(InventoryAction::Unequip(EquipmentSlot::Body));
            smoke.stage = 7;
        }
        7 => {
            assert_eq!(equipment.get(EquipmentSlot::Body), None);
            assert_eq!(bag.count(ItemKind::FieldArmor), 1);
            click!(InventoryAction::Select(ItemKind::FieldArmor));
            smoke.stage = 8;
        }
        8 => {
            click!(InventoryAction::Drop);
            smoke.timer = now;
            smoke.stage = 9;
        }
        9 if now - smoke.timer > 0.3 => {
            assert_eq!(
                bag.count(ItemKind::FieldArmor),
                0,
                "unequipped armor was not dropped"
            );
            assert_eq!(equipment.total_defense(), 1);
            keys.press(KeyCode::KeyI);
            smoke.stage = 10;
        }
        10 if *state.get() == GameState::Playing => {
            if collect(
                ItemKind::FieldArmor,
                &bag,
                &nearby,
                &view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                keys.press(KeyCode::KeyI);
                smoke.stage = 11;
            }
        }
        11 if *state.get() == GameState::Inventory => {
            click!(InventoryAction::Select(ItemKind::FieldArmor));
            smoke.stage = 12;
        }
        12 => {
            click!(InventoryAction::Equip);
            smoke.timer = now;
            smoke.stage = 13;
        }
        13 if now - smoke.timer > 0.3 && check_worn(&view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 3);
            keys.press(KeyCode::KeyI);
            smoke.stage = 49;
        }
        100 => {
            if collect(
                ItemKind::ReinforcedArmor,
                &bag,
                &nearby,
                &view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                smoke.stage = 101;
            }
        }
        101 => {
            if collect(
                ItemKind::ReinforcedHelmet,
                &bag,
                &nearby,
                &view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                keys.press(KeyCode::KeyI);
                smoke.stage = 102;
            }
        }
        102 if *state.get() == GameState::Inventory => {
            click!(InventoryAction::Select(ItemKind::ReinforcedArmor));
            smoke.stage = 103;
        }
        103 => {
            click!(InventoryAction::Equip);
            smoke.stage = 104;
        }
        104 => {
            assert_eq!(bag.count(ItemKind::FieldArmor), 1);
            assert_eq!(equipment.total_defense(), 6);
            click!(InventoryAction::Select(ItemKind::ReinforcedHelmet));
            smoke.stage = 105;
        }
        105 => {
            click!(InventoryAction::Equip);
            smoke.timer = now;
            smoke.stage = 106;
        }
        106 if now - smoke.timer > 0.3 && check_worn(&view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 8);
            assert_eq!(bag.count(ItemKind::FieldHelmet), 1);
            photo!("reinforced-equipment-inventory");
            smoke.timer = now;
            smoke.stage = 161;
        }
        161 if now - smoke.timer > 0.2 => {
            click!(InventoryAction::Unequip(EquipmentSlot::Helmet));
            smoke.stage = 107;
        }
        107 => {
            assert_eq!(equipment.get(EquipmentSlot::Helmet), None);
            assert_eq!(bag.count(ItemKind::ReinforcedHelmet), 1);
            click!(InventoryAction::Select(ItemKind::ReinforcedHelmet));
            smoke.stage = 108;
        }
        108 => {
            click!(InventoryAction::Equip);
            smoke.timer = now;
            smoke.stage = 109;
        }
        109 if now - smoke.timer > 0.3 && check_worn(&view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 8);
            keys.press(KeyCode::KeyI);
            smoke.stage = 49;
        }
        49 if *state.get() == GameState::Playing => {
            let forward = transform.rotation * Vec3::Z;
            orbit.yaw = forward.x.atan2(forward.z);
            orbit.pitch = 0.18;
            orbit.distance = 3.7;
            smoke.timer = now;
            smoke.stage = 50;
        }
        50 if now - smoke.timer > 0.3 => {
            assert!(check_worn(&view, &equipment, player));
            photo!(if smoke.heavy {
                "reinforced-gear-idle"
            } else {
                "field-gear-idle"
            });
            smoke.timer = now;
            smoke.stage = 51;
        }
        51 => {
            keys.press(KeyCode::KeyW);
            if now - smoke.timer > 0.5 {
                assert!(check_worn(&view, &equipment, player));
                photo!(if smoke.heavy {
                    "reinforced-gear-walk"
                } else {
                    "field-gear-walk"
                });
                smoke.timer = now;
                smoke.stage = 52;
            }
        }
        52 => {
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
            if now - smoke.timer > 0.5 {
                assert!(check_worn(&view, &equipment, player));
                photo!(if smoke.heavy {
                    "reinforced-gear-run"
                } else {
                    "field-gear-run"
                });
                smoke.stage = 53;
            }
        }
        53 if controller.grounded => {
            keys.press(KeyCode::Space);
            smoke.timer = now;
            smoke.stage = 54;
        }
        54 if now - smoke.timer > 0.3 => {
            assert!(
                !controller.grounded,
                "equipment jump check did not leave the ground"
            );
            assert!(check_worn(&view, &equipment, player));
            photo!(if smoke.heavy {
                "reinforced-gear-jump"
            } else {
                "field-gear-jump"
            });
            smoke.stage = 55;
        }
        55 if controller.grounded => {
            assert!(check_worn(&view, &equipment, player));
            orbit.yaw = 0.0;
            orbit.pitch = 0.22;
            orbit.distance = 5.5;
            if smoke.heavy {
                inventory_smoke.equipment_done = true;
                info!(
                    "EQUIPMENT SMOKE PASS: picked up both sets, equipped, swapped, unequipped, dropped/recollected, and checked all animation states"
                );
            }
            inventory_smoke.equipment_ready = true;
            inventory_smoke.equipment_busy = false;
            smoke.stage = 60;
        }
        _ => (),
    }
}
