use crate::{
    equipment::{EquipRequest, Equipment, EquipmentSlot, UnequipRequest},
    inventory::{
        CARRY_CAPACITY, DropRequest, IMMOBILE_WEIGHT, Inventory, InventoryNotice, ItemKind,
    },
    menu_ui::{self, BUTTON, DISABLED, MenuTab, PlayerMenu, SELECTED, column, scroll_pane},
    ui::text_style as font,
};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
};

pub struct InventoryUiPlugin;
#[derive(Resource, Default)]
pub struct InventoryUiState {
    pub selected: Option<ItemKind>,
    pub quantity: String,
    editing: bool,
    replace: bool,
}
impl InventoryUiState {
    pub fn amount(&self, inventory: &Inventory) -> Option<u32> {
        let item = self.selected?;
        if item == ItemKind::BeaconCore {
            return None;
        }
        self.quantity
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0 && *n <= inventory.count(item))
    }
    fn step(&mut self, inventory: &Inventory, direction: i32) {
        let Some(item) = self.selected else {
            return;
        };
        let max = inventory.count(item);
        if max == 0 {
            return;
        }
        let amount = self.quantity.parse::<u32>().unwrap_or(1);
        self.quantity = (amount as i64 + direction as i64)
            .clamp(1, max as i64)
            .to_string();
        self.editing = false;
    }
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryAction {
    Select(ItemKind),
    Equip,
    Unequip(EquipmentSlot),
    Minus,
    Plus,
    Quantity,
    Drop,
}
#[derive(Component)]
struct SlotText(EquipmentSlot);
#[derive(Component)]
struct DefenseText;
#[derive(Component)]
struct EquipLabel;
#[derive(Component)]
struct ItemLabel(ItemKind);
#[derive(Component)]
struct ItemTitle;
#[derive(Component)]
struct DetailText;
#[derive(Component)]
struct AmountText;
#[derive(Component)]
struct NoticeText;
#[derive(Component)]
struct EmptyText;
#[derive(Component)]
struct DropControls;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryUiState>()
            .add_systems(
                Update,
                (edit_quantity, sync)
                    .chain()
                    .run_if(|menu: Res<PlayerMenu>| menu.is_on(MenuTab::Inventory)),
            )
            .add_systems(Last, blur_quantity.after(menu_ui::close));
    }
}

fn action_button(parent: &mut ChildSpawnerCommands, kind: InventoryAction, label: &str) {
    parent
        .spawn((kind, menu_ui::button()))
        .observe(action)
        .with_child((Text::new(label), font(16.0), Pickable::IGNORE));
}

pub(crate) fn spawn(page: &mut ChildSpawnerCommands) {
    page.spawn(scroll_pane(MenuTab::Inventory, percent(32)))
        .with_children(|bag| {
            bag.spawn((Text::new("Bag"), font(19.0)));
            bag.spawn((EmptyText, Text::new("Your bag is empty."), font(16.0)));
            for item in ItemKind::ALL {
                bag.spawn((InventoryAction::Select(item), menu_ui::button()))
                    .observe(action)
                    .with_child((
                        ItemLabel(item),
                        Text::default(),
                        font(16.0),
                        Pickable::IGNORE,
                    ));
            }
        });
    page.spawn(scroll_pane(MenuTab::Inventory, percent(68)))
        .with_children(|details| {
            details.spawn((ItemTitle, Text::default(), font(23.0)));
            details.spawn((DetailText, Text::default(), font(17.0)));
            details
                .spawn((InventoryAction::Equip, menu_ui::button()))
                .observe(action)
                .with_child((EquipLabel, Text::new("Equip"), font(16.0), Pickable::IGNORE));
            details
                .spawn((
                    DropControls,
                    Node {
                        flex_shrink: 0.0,
                        ..column()
                    },
                ))
                .with_children(|controls| {
                    controls.spawn((Text::new("Quantity to drop"), font(16.0)));
                    controls
                        .spawn(Node {
                            column_gap: px(8),
                            ..default()
                        })
                        .with_children(|quantity| {
                            action_button(quantity, InventoryAction::Minus, "-");
                            quantity
                                .spawn((InventoryAction::Quantity, menu_ui::button()))
                                .insert(Node {
                                    min_width: px(96),
                                    min_height: px(34),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::axes(px(8), px(6)),
                                    ..default()
                                })
                                .observe(action)
                                .with_child((
                                    AmountText,
                                    Text::new("1"),
                                    font(16.0),
                                    Pickable::IGNORE,
                                ));
                            action_button(quantity, InventoryAction::Plus, "+");
                        });
                    action_button(controls, InventoryAction::Drop, "Drop selected quantity");
                });
            details.spawn((NoticeText, Text::default(), font(14.0)));
            details.spawn((Text::new("Equipped"), font(19.0)));
            details.spawn((DefenseText, Text::default(), font(16.0)));
            for slot in EquipmentSlot::ALL {
                details
                    .spawn(Node {
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: px(8),
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            SlotText(slot),
                            Text::default(),
                            font(16.0),
                            Node {
                                min_width: px(0),
                                flex_grow: 1.0,
                                ..default()
                            },
                        ));
                        action_button(row, InventoryAction::Unequip(slot), "Unequip");
                    });
            }
        });
}

fn blur_quantity(
    menu: Res<PlayerMenu>,
    mut ui: ResMut<InventoryUiState>,
    mut notice: ResMut<InventoryNotice>,
) {
    if menu.is_changed() && !menu.is_on(MenuTab::Inventory) {
        if ui.editing {
            ui.editing = false;
        }
        if !notice.0.is_empty() {
            notice.0.clear();
        }
    }
}

fn action(
    click: On<PointerClick>,
    actions: Query<&InventoryAction>,
    menu: Res<PlayerMenu>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    mut ui: ResMut<InventoryUiState>,
    mut notice: ResMut<InventoryNotice>,
    mut commands: Commands,
) {
    if !menu.accepts(MenuTab::Inventory) || click.button != PointerButton::Primary {
        return;
    }
    let Ok(action) = actions.get(click.entity) else {
        return;
    };
    match *action {
        InventoryAction::Select(item) if inventory.count(item) > 0 => {
            ui.selected = Some(item);
            ui.quantity = "1".into();
            ui.editing = false;
            notice.0.clear();
        }
        InventoryAction::Equip => {
            if let Some(item) = ui.selected {
                commands.trigger(EquipRequest(item));
                ui.editing = false;
            }
        }
        InventoryAction::Unequip(slot) if equipment.get(slot).is_some() => {
            commands.trigger(UnequipRequest(slot));
            ui.editing = false;
        }
        InventoryAction::Minus => ui.step(&inventory, -1),
        InventoryAction::Plus => ui.step(&inventory, 1),
        InventoryAction::Quantity => {
            ui.editing = true;
            ui.replace = true;
        }
        InventoryAction::Drop => {
            if let (Some(item), Some(quantity)) = (ui.selected, ui.amount(&inventory)) {
                ui.editing = false;
                commands.trigger(DropRequest { item, quantity });
            } else {
                notice.0 = "Choose a quantity from 1 to the available count.".into();
            }
        }
        _ => (),
    }
}
fn edit_quantity(
    mut events: MessageReader<KeyboardInput>,
    mut ui: ResMut<InventoryUiState>,
    inventory: Res<Inventory>,
    menu: Res<PlayerMenu>,
) {
    if !menu.accepts(MenuTab::Inventory) {
        events.clear();
        return;
    }
    for event in events.read() {
        if event.state != ButtonState::Pressed
            || !ui.editing
            || ui.selected == Some(ItemKind::BeaconCore)
        {
            continue;
        }
        match &event.logical_key {
            Key::Backspace => {
                if ui.replace {
                    ui.quantity.clear();
                } else {
                    ui.quantity.pop();
                }
                ui.replace = false;
            }
            Key::Delete => {
                ui.quantity.clear();
                ui.replace = false;
            }
            Key::Enter => ui.editing = false,
            Key::ArrowUp => ui.step(&inventory, 1),
            Key::ArrowDown => ui.step(&inventory, -1),
            _ => {
                if let Some(text) = &event.text {
                    if text.chars().all(|c| c.is_ascii_digit()) {
                        if ui.replace {
                            ui.quantity.clear();
                            ui.replace = false;
                        }
                        if ui.quantity.len() + text.len() <= 10 {
                            ui.quantity.push_str(text);
                        }
                    }
                }
            }
        }
    }
}
fn sync(
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    mut ui: ResMut<InventoryUiState>,
    notice: Res<InventoryNotice>,
    mut controls: Query<
        &mut Node,
        (
            With<DropControls>,
            Without<InventoryAction>,
            Without<EmptyText>,
        ),
    >,
    mut buttons: Query<(&InventoryAction, &mut Node, &mut BackgroundColor), Without<EmptyText>>,
    mut empty: Query<
        &mut Node,
        (
            With<EmptyText>,
            Without<InventoryAction>,
            Without<DropControls>,
        ),
    >,
    mut texts: ParamSet<(
        Query<(&ItemLabel, &mut Text)>,
        Query<&mut Text, With<DetailText>>,
        Query<&mut Text, With<AmountText>>,
        Query<&mut Text, With<NoticeText>>,
        Query<(&SlotText, &mut Text)>,
        Query<&mut Text, With<DefenseText>>,
        Query<&mut Text, With<EquipLabel>>,
        Query<&mut Text, With<ItemTitle>>,
    )>,
) {
    if !(inventory.is_changed() || equipment.is_changed() || ui.is_changed() || notice.is_changed())
    {
        return;
    }
    if ui.selected.is_none_or(|item| inventory.count(item) == 0) {
        ui.selected = ItemKind::ALL
            .into_iter()
            .find(|item| inventory.count(*item) > 0);
        ui.quantity = "1".into();
        ui.editing = false;
    }
    if inventory.is_changed()
        && let Some(item) = ui.selected
    {
        if ui
            .quantity
            .parse::<u32>()
            .is_ok_and(|n| n > inventory.count(item))
        {
            ui.quantity = inventory.count(item).to_string();
        }
    }
    let droppable = ui.selected.is_some_and(|item| item != ItemKind::BeaconCore);
    for mut node in &mut controls {
        node.display = if droppable {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut node in &mut empty {
        node.display = if ui.selected.is_none() {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (action, mut node, mut color) in &mut buttons {
        if let InventoryAction::Select(item) = action {
            node.display = if inventory.count(*item) > 0 {
                Display::Flex
            } else {
                Display::None
            };
        }
        if *action == InventoryAction::Equip {
            node.display = if ui.selected.is_some_and(|item| item.gear().is_some()) {
                Display::Flex
            } else {
                Display::None
            };
        }
        let selected = matches!(action, InventoryAction::Select(item) if Some(*item) == ui.selected)
            || (*action == InventoryAction::Quantity && ui.editing);
        let invalid = match *action {
            InventoryAction::Drop => ui.amount(&inventory).is_none(),
            InventoryAction::Equip => ui.selected.is_none_or(|item| {
                item.gear()
                    .is_none_or(|gear| equipment.get(gear.slot) == Some(item))
            }),
            InventoryAction::Unequip(slot) => equipment.get(slot).is_none(),
            _ => false,
        };
        color.0 = if invalid {
            DISABLED
        } else if selected {
            SELECTED
        } else {
            BUTTON
        };
    }
    for (label, mut text) in &mut texts.p0() {
        **text = format!(
            "{}  x{}{}",
            label.0.name(),
            inventory.count(label.0),
            if label.0 == ItemKind::BeaconCore {
                "  / Quest item"
            } else {
                ""
            }
        );
    }
    for mut text in &mut texts.p7() {
        **text = ui.selected.map_or("", ItemKind::name).into();
    }
    for mut text in &mut texts.p1() {
        **text = ui.selected.map_or(String::new(), |item| {
            let description = item.gear().map_or_else(
                || item.description().into(),
                |gear| {
                    if gear.slot == EquipmentSlot::Weapon {
                        format!(
                            "Two-handed weapon / Reach {:.1} m\n{}",
                            crate::spear::REACH,
                            item.description()
                        )
                    } else {
                        format!(
                            "{} / Defense {}\n{}",
                            gear.slot.name(),
                            gear.defense,
                            item.description()
                        )
                    }
                },
            );
            format!("{} kg each\n{description}", item.weight())
        });
    }
    for mut text in &mut texts.p2() {
        **text = format!("{}{}", ui.quantity, if ui.editing { "_" } else { "" });
    }
    for mut text in &mut texts.p3() {
        **text = if droppable && ui.amount(&inventory).is_none() {
            "Enter a quantity from 1 to the available count.".into()
        } else {
            notice.0.clone()
        };
    }
    for (slot, mut text) in &mut texts.p4() {
        **text = equipment.get(slot.0).map_or_else(
            || format!("{}: Empty", slot.0.name()),
            |item| {
                if slot.0 == EquipmentSlot::Weapon {
                    return format!("{}: {}", slot.0.name(), item.name());
                }
                format!(
                    "{}: {} (+{})",
                    slot.0.name(),
                    item.name(),
                    item.gear()
                        .expect("only equipment items can occupy equipment slots")
                        .defense
                )
            },
        );
    }
    for mut text in &mut texts.p5() {
        let weight = inventory.carried_weight(&equipment);
        let status = if weight > IMMOBILE_WEIGHT {
            " / Cannot move"
        } else if weight > CARRY_CAPACITY {
            " / Walking only"
        } else {
            ""
        };
        **text = format!(
            "Defense {}  /  Load {weight}/{CARRY_CAPACITY} kg{status}",
            equipment.total_defense()
        );
    }
    for mut text in &mut texts.p6() {
        **text = if ui.selected.is_some_and(|item| {
            item.gear()
                .is_some_and(|gear| equipment.get(gear.slot) == Some(item))
        }) {
            "Equipped"
        } else {
            "Equip"
        }
        .into();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantity_field_requires_a_positive_available_integer() {
        let mut inventory = Inventory::default();
        inventory.add(ItemKind::Battery, 3);
        let mut ui = InventoryUiState {
            selected: Some(ItemKind::Battery),
            ..default()
        };
        for text in ["", "0", "4", "-1", "1.5", "4294967296"] {
            ui.quantity = text.into();
            assert_eq!(ui.amount(&inventory), None);
        }
        ui.quantity = "2".into();
        assert_eq!(ui.amount(&inventory), Some(2));
        ui.step(&inventory, 1);
        assert_eq!(ui.amount(&inventory), Some(3));
        ui.step(&inventory, 1);
        assert_eq!(ui.amount(&inventory), Some(3));
        ui.selected = Some(ItemKind::BeaconCore);
        assert_eq!(ui.amount(&inventory), None);
    }
}
