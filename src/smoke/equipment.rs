//! Opt-in equipment playthrough using the live inventory UI and animated player rig.
use super::inventory::{InventorySmoke, click, walk};
use crate::{
    GameState,
    camera::Orbit,
    equipment::{Equipment, EquipmentSlot, WornEquipment},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind, Pickup},
    inventory_ui::InventoryAction,
    loading::PendingScene,
    menu_ui::PlayerMenuScreen,
    menu_ui::{MenuTab, PlayerMenu},
    player::Player,
};
use bevy::{
    ecs::system::SystemParam, input::InputSystems, prelude::*, render::view::screenshot::Screenshot,
};

pub struct EquipmentSmokePlugin;
#[derive(Resource)]
pub(crate) struct EquipmentSmoke {
    stage: u16,
    heavy: bool,
    timer: f32,
    screenshot: Option<Entity>,
    screen_size: Option<(Val, Val)>,
}
#[derive(SystemParam)]
pub(crate) struct View<'w, 's> {
    screen: Query<'w, 's, (&'static mut Node, &'static ComputedNode), With<PlayerMenuScreen>>,
    captures: Query<'w, 's, (), With<Screenshot>>,
    player: Query<'w, 's, (Entity, &'static Transform, &'static Player), With<Player>>,
    pickups: Query<'w, 's, (&'static Pickup, &'static Transform)>,
    actions: Query<'w, 's, (Entity, &'static InventoryAction)>,
    tabs: Query<'w, 's, (Entity, &'static MenuTab)>,
    inventory_rows:
        Query<'w, 's, (&'static InventoryAction, &'static Children), Without<PlayerMenuScreen>>,
    text: Query<'w, 's, &'static Text>,
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
impl EquipmentSmoke {
    pub fn stage(&self) -> u16 {
        self.stage
    }

    pub fn done(&self) -> bool {
        self.stage == 60
    }

    fn captured(&self, view: &View) -> bool {
        self.screenshot
            .is_some_and(|entity| view.captures.get(entity).is_err())
    }
}

impl Plugin for EquipmentSmokePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EquipmentSmoke {
            stage: 0,
            heavy: false,
            timer: 0.0,
            screenshot: None,
            screen_size: None,
        })
        .add_systems(
            PreUpdate,
            drive
                .after(InputSystems)
                .after(super::inventory::drive)
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
pub(crate) fn drive(
    spear_smoke: Res<super::spear::SpearSmoke>,
    mut smoke: ResMut<EquipmentSmoke>,
    inventory_smoke: Res<InventorySmoke>,
    (state, menu): (Res<State<GameState>>, Res<PlayerMenu>),
    time: Res<Time>,
    bag: Res<Inventory>,
    equipment: Res<Equipment>,
    nearby: Res<Nearby>,
    mut view: View,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    mut last_log: Local<f32>,
) {
    if !spear_smoke.done() || !inventory_smoke.done || smoke.done() {
        return;
    }
    let Ok((player, transform, controller)) = view.player.single() else {
        return;
    };
    keys.reset_all();
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
            smoke.screenshot = Some(super::capture(&mut commands, $name));
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
        2 if menu.is_on(MenuTab::Inventory) => {
            click(
                &mut commands,
                MenuTab::Equipment,
                &view.tabs,
                window,
                camera,
            );
        }
        2 if menu.is_on(MenuTab::Equipment) => {
            click!(InventoryAction::SelectGear(ItemKind::FieldArmor));
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
            click!(InventoryAction::SelectGear(ItemKind::FieldHelmet));
            smoke.stage = 5;
        }
        5 => {
            click!(InventoryAction::Equip);
            smoke.stage = 6;
        }
        6 if check_worn(&view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 3);
            assert_eq!(bag.count(ItemKind::FieldArmor), 0);
            let (_, cells) = view
                .inventory_rows
                .iter()
                .find(|(action, _)| **action == InventoryAction::Select(ItemKind::FieldArmor))
                .expect("the inventory table must have a row for worn armor");
            let values: Vec<_> = cells
                .iter()
                .filter_map(|cell| view.text.get(cell).ok())
                .map(|text| text.as_str())
                .collect();
            assert_eq!(values, ["Field armor", "1", "5 kg", "Equipped"]);
            photo!("field-equipment-inventory");
            smoke.stage = 61;
        }
        61 if smoke.captured(&view) => {
            let (mut screen, _) = view
                .screen
                .single_mut()
                .expect("equipment layout checks require one inventory screen");
            smoke.screen_size = Some((screen.width, screen.height));
            screen.width = px(640);
            screen.height = px(704);
            smoke.stage = 62;
        }
        62 => {
            let (_, computed) = view
                .screen
                .single_mut()
                .expect("the narrow layout check requires one inventory screen");
            let size = computed.size() * computed.inverse_scale_factor();
            assert!(
                (size - Vec2::new(640.0, 704.0)).abs().max_element() <= 1.0,
                "the inventory must lay out at 640x704, got {size}"
            );

            photo!("field-equipment-inventory-narrow");
            smoke.stage = 63;
        }
        63 if smoke.captured(&view) => {
            let (width, height) = smoke
                .screen_size
                .take()
                .expect("the narrow layout check must preserve the original inventory dimensions");
            let (mut screen, _) = view
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
            click(
                &mut commands,
                MenuTab::Inventory,
                &view.tabs,
                window,
                camera,
            );
            smoke.stage = 71;
        }
        71 if menu.is_on(MenuTab::Inventory) => {
            click!(InventoryAction::Select(ItemKind::FieldArmor));
            smoke.stage = 8;
        }
        8 => {
            click!(InventoryAction::Drop);
            smoke.stage = 81;
        }
        81 => {
            click!(InventoryAction::ConfirmDrop);
            smoke.stage = 9;
        }
        9 => {
            assert_eq!(
                bag.count(ItemKind::FieldArmor),
                0,
                "unequipped armor was not dropped"
            );
            assert_eq!(equipment.total_defense(), 1);
            keys.press(KeyCode::KeyI);
            smoke.stage = 10;
        }
        10 if *state.get() == GameState::Playing && !menu.is_open() => {
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
        11 if menu.is_on(MenuTab::Inventory) => {
            click(
                &mut commands,
                MenuTab::Equipment,
                &view.tabs,
                window,
                camera,
            );
        }
        11 if menu.is_on(MenuTab::Equipment) => {
            click!(InventoryAction::SelectGear(ItemKind::FieldArmor));
            smoke.stage = 12;
        }
        12 => {
            click!(InventoryAction::Equip);
            smoke.stage = 13;
        }
        13 if check_worn(&view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 3);
            keys.press(KeyCode::Tab);
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
        102 if menu.is_on(MenuTab::Inventory) => {
            click(
                &mut commands,
                MenuTab::Equipment,
                &view.tabs,
                window,
                camera,
            );
        }
        102 if menu.is_on(MenuTab::Equipment) => {
            click!(InventoryAction::SelectGear(ItemKind::ReinforcedArmor));
            smoke.stage = 103;
        }
        103 => {
            click!(InventoryAction::Equip);
            smoke.stage = 104;
        }
        104 => {
            assert_eq!(bag.count(ItemKind::FieldArmor), 1);
            assert_eq!(equipment.total_defense(), 6);
            click!(InventoryAction::SelectGear(ItemKind::ReinforcedHelmet));
            smoke.stage = 105;
        }
        105 => {
            click!(InventoryAction::Equip);
            smoke.stage = 106;
        }
        106 if check_worn(&view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 8);
            assert_eq!(bag.count(ItemKind::FieldHelmet), 1);
            photo!("reinforced-equipment-inventory");
            smoke.stage = 161;
        }
        161 if smoke.captured(&view) => {
            click!(InventoryAction::Unequip(EquipmentSlot::Helmet));
            smoke.stage = 107;
        }
        107 => {
            assert_eq!(equipment.get(EquipmentSlot::Helmet), None);
            assert_eq!(bag.count(ItemKind::ReinforcedHelmet), 1);
            click!(InventoryAction::SelectGear(ItemKind::ReinforcedHelmet));
            smoke.stage = 108;
        }
        108 => {
            click!(InventoryAction::Equip);
            smoke.stage = 109;
        }
        109 if check_worn(&view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 8);
            keys.press(KeyCode::Tab);
            smoke.stage = 49;
        }
        49 if *state.get() == GameState::Playing && !menu.is_open() => {
            // Both equipment rows face a clear strip of floor to the north.
            orbit.yaw = 0.0;
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
                assert!(
                    controller.grounded
                        && !controller.running
                        && controller.velocity.xz().length() > 0.1,
                    "the walk capture requires grounded walking"
                );
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
                assert!(
                    controller.grounded
                        && controller.running
                        && controller.velocity.xz().length() > 0.1,
                    "the sprint capture requires grounded sprinting"
                );
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
                info!(
                    "EQUIPMENT SMOKE PASS: picked up both sets, equipped, swapped, unequipped, dropped/recollected, and checked all animation states"
                );
                smoke.stage = 60;
            } else {
                smoke.heavy = true;
                smoke.stage = 100;
            }
        }
        _ => (),
    }
}
