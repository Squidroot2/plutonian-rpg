use std::collections::BTreeMap;

use crate::{
    GameState,
    attributes::{AttributeId, AttributeRanks, AttributeUpdates, CarryLimits, DEFAULT_RANK},
    controls::{Control, Keybinds},
    equipment::{EquipRequest, Equipment, EquipmentSlot, Handedness, UnequipRequest},
    inventory::{DropRequest, Inventory, InventoryNotice, ItemKind},
    menu_ui::{
        self, BUTTON, DISABLED, MenuTab, PlayerMenu, SELECTED, column, scroll_pane, set_display,
    },
    player::Player,
    quest::Dialogue,
    ui::text_style as font,
};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
    ui::UiGlobalTransform,
};

pub struct InventoryUiPlugin;

#[derive(Component)]
struct StrengthWarning;

#[derive(Component)]
struct StrengthWarningClose;

#[derive(Resource, Default)]
pub struct InventoryUiState {
    selected: Option<InventoryRow>,
    pub(crate) quantity: String,
    equip_slot: Option<EquipmentSlot>,
    drop_open: bool,
    editing: bool,
    replace: bool,
}

impl InventoryUiState {
    pub(crate) fn selected_item(&self) -> Option<ItemKind> {
        self.selected.map(InventoryRow::item)
    }

    pub fn amount(&self, inventory: &Inventory) -> Option<u32> {
        let row = self.selected?;
        let available = row.available_to_drop(inventory);
        if matches!(row, InventoryRow::BagGear(..)) {
            return (available > 0).then_some(1);
        }
        self.quantity
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0 && *n <= available)
    }

    pub(crate) fn cancel_drop(&mut self) -> bool {
        if !self.drop_open {
            return false;
        }
        self.drop_open = false;
        self.editing = false;
        self.replace = false;
        true
    }

    pub(crate) fn drop_is_open(&self) -> bool {
        self.drop_open
    }

    fn open_drop(&mut self, inventory: &Inventory) {
        if !self.drop_open
            && self
                .selected
                .is_some_and(|row| row.available_to_drop(inventory) > 0)
        {
            self.drop_open = true;
            self.quantity = "1".into();
            self.editing = self.uses_quantity_picker();
            self.replace = self.editing;
        }
    }

    fn uses_quantity_picker(&self) -> bool {
        matches!(self.selected, Some(InventoryRow::Stack(_)))
    }

    fn step(&mut self, inventory: &Inventory, direction: i32) {
        let Some(row @ InventoryRow::Stack(_)) = self.selected else {
            return;
        };
        let max = row.available_to_drop(inventory);
        if max == 0 {
            return;
        }
        let amount = self.quantity.parse::<u32>().unwrap_or(1);
        self.quantity = (i64::from(amount) + i64::from(direction))
            .clamp(1, i64::from(max))
            .to_string();
        self.editing = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InventoryRow {
    Stack(ItemKind),
    BagGear(ItemKind, u64),
    Equipped(ItemKind, EquipmentSlot),
}

impl InventoryRow {
    pub(crate) fn item(self) -> ItemKind {
        match self {
            Self::Stack(item) | Self::BagGear(item, _) | Self::Equipped(item, _) => item,
        }
    }

    fn quantity(self, inventory: &Inventory) -> u32 {
        match self {
            Self::Stack(item) => inventory.count(item),
            _ => 1,
        }
    }

    fn available_to_drop(self, inventory: &Inventory) -> u32 {
        match self {
            Self::Stack(ItemKind::BeaconCore) | Self::Equipped(..) => 0,
            Self::Stack(item) => inventory.count(item),
            Self::BagGear(item, _) => inventory.count(item).min(1),
        }
    }
}

fn can_equip_from_bag(
    item: ItemKind,
    slot: EquipmentSlot,
    inventory: &Inventory,
    equipment: &Equipment,
) -> bool {
    let reserved = slot == EquipmentSlot::OffHand && equipment.off_hand_reserved();
    !reserved && item.fits_slot(slot) && inventory.count(item) > 0
}

fn has_gear_for_slot(slot: EquipmentSlot, inventory: &Inventory, equipment: &Equipment) -> bool {
    ItemKind::GEAR
        .into_iter()
        .any(|item| can_equip_from_bag(item, slot, inventory, equipment))
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryAction {
    Select(InventoryRow),
    ToggleSlot(EquipmentSlot),
    EquipFromSlot(EquipmentSlot, ItemKind),
    CloseSlotPicker(EquipmentSlot),
    Minus,
    Plus,
    Quantity,
    Drop,
    ConfirmDrop,
    CancelDrop,
}

impl InventoryAction {
    fn tab(self) -> MenuTab {
        match self {
            Self::ToggleSlot(_) | Self::EquipFromSlot(_, _) | Self::CloseSlotPicker(_) => {
                MenuTab::Equipment
            }
            _ => MenuTab::Inventory,
        }
    }
}

#[derive(Component)]
enum PanelText {
    ItemQuantity(InventoryRow),
    ItemWeight(InventoryRow),
    Slot(EquipmentSlot),
    SlotAction(EquipmentSlot),
    Defense,
    Load,
    Detail,
    Amount,
    DropLabel,
    DropTitle,
    Notice(MenuTab),
}

#[derive(Component)]
struct EmptyList;
#[derive(Component)]
struct DropControls;
#[derive(Component, Default)]
struct ItemList {
    rows: Vec<(InventoryRow, Entity)>,
    next_copy: u64,
}
#[derive(Component)]
struct SlotPicker(EquipmentSlot);

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryUiState>()
            .add_systems(
                Update,
                (
                    sync_inventory_rows,
                    navigate_items,
                    edit_quantity,
                    (reconcile_selection, (refresh_controls, refresh_labels))
                        .chain()
                        .run_if(inventory_panel_changed),
                )
                    .chain()
                    .after(AttributeUpdates),
            )
            .add_systems(Last, dismiss_prompt.after(menu_ui::close))
            .add_systems(
                Update,
                close_strength_warning.run_if(in_state(GameState::Playing)),
            );
    }
}

fn action_button(parent: &mut ChildSpawnerCommands, kind: InventoryAction, label: impl Bundle) {
    parent
        .spawn((kind, menu_ui::button()))
        .observe(action)
        .with_child((label, font(16.0), Pickable::IGNORE));
}

fn table_cells(row: &mut ChildSpawnerCommands, cells: [(Text, Option<PanelText>); 4]) {
    for ((cell, kind), width) in cells.into_iter().zip([43.0, 15.0, 21.0, 21.0]) {
        let mut entity = row.spawn((
            cell,
            font(16.0),
            Pickable::IGNORE,
            Node {
                width: percent(width),
                min_width: px(0),
                ..default()
            },
        ));
        if let Some(kind) = kind {
            entity.insert(kind);
        }
    }
}

impl ItemList {
    fn desired_rows(
        &self,
        inventory: &Inventory,
        equipment: &Equipment,
        selected: Option<InventoryRow>,
        next_copy: &mut u64,
    ) -> Vec<InventoryRow> {
        let mut desired = Vec::new();
        for item in ItemKind::ALL {
            let count = inventory.count(item);
            if item.gear().is_none() {
                if count > 0 {
                    desired.push(InventoryRow::Stack(item));
                }
                continue;
            }
            desired.extend(
                EquipmentSlot::ALL
                    .into_iter()
                    .filter(|slot| equipment.get(*slot) == Some(item))
                    .map(|slot| InventoryRow::Equipped(item, slot)),
            );
            let mut bag: Vec<_> = self
                .rows
                .iter()
                .map(|&(row, _)| row)
                .filter(|row| matches!(row, InventoryRow::BagGear(kind, _) if *kind == item))
                .collect();
            let count = count as usize;
            // Copies are interchangeable in the bag. Remove the selected row first so the
            // other copies keep their entities and their own click histories.
            if bag.len() > count
                && let Some(index) = bag.iter().position(|&row| Some(row) == selected)
            {
                bag.remove(index);
            }
            bag.truncate(count);
            desired.extend(bag.iter().copied());
            for _ in bag.len()..count {
                desired.push(InventoryRow::BagGear(item, *next_copy));
                *next_copy += 1;
            }
        }
        desired
    }
}

fn sync_inventory_rows(
    menu: Res<PlayerMenu>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    ui: Res<InventoryUiState>,
    mut lists: Query<(Entity, &mut ItemList)>,
    empty_label: Query<Entity, With<EmptyList>>,
    mut commands: Commands,
) {
    if !menu.is_open() {
        return;
    }
    let (Ok((list_entity, mut list)), Ok(empty_label)) = (lists.single_mut(), empty_label.single())
    else {
        return;
    };
    if !list.is_added() && !menu.is_changed() && !inventory.is_changed() && !equipment.is_changed()
    {
        return;
    }
    let mut next_copy = list.next_copy;
    let desired = list.desired_rows(&inventory, &equipment, ui.selected, &mut next_copy);
    if list
        .rows
        .iter()
        .map(|&(row, _)| row)
        .eq(desired.iter().copied())
    {
        return;
    }

    let mut old: BTreeMap<_, _> = list.rows.iter().copied().collect();
    list.rows = desired
        .into_iter()
        .map(|row| {
            let entity = old
                .remove(&row)
                .unwrap_or_else(|| spawn_item_row(&mut commands, row));
            (row, entity)
        })
        .collect();
    list.next_copy = next_copy;
    let children: Vec<_> = std::iter::once(empty_label)
        .chain(list.rows.iter().map(|&(_, entity)| entity))
        .collect();
    commands.entity(list_entity).replace_children(&children);
    for entity in old.into_values() {
        commands.entity(entity).despawn();
    }
}

fn spawn_item_row(commands: &mut Commands, row: InventoryRow) -> Entity {
    let item = row.item();
    let status = match row {
        InventoryRow::Stack(ItemKind::BeaconCore) => "Quest item".into(),
        InventoryRow::Equipped(_, slot) => format!("Equipped ({})", slot.name()),
        _ => String::new(),
    };
    commands
        .spawn((InventoryAction::Select(row), menu_ui::button()))
        .observe(action)
        .with_children(|cells| {
            table_cells(
                cells,
                [
                    (Text::new(item.name()), None),
                    (Text::default(), Some(PanelText::ItemQuantity(row))),
                    (Text::default(), Some(PanelText::ItemWeight(row))),
                    (Text::new(status), None),
                ],
            );
        })
        .id()
}

pub(crate) fn spawn(page: &mut ChildSpawnerCommands) {
    page.spawn(Node {
        width: percent(100),
        ..column()
    })
    .with_children(|panel| {
        panel
            .spawn(Node {
                padding: UiRect::horizontal(px(24)),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|row| {
                table_cells(
                    row,
                    ["Item", "Qty", "Weight", "Status"].map(|label| (Text::new(label), None)),
                )
            });
        panel
            .spawn((
                ItemList::default(),
                scroll_pane(MenuTab::Inventory, percent(100)),
            ))
            .insert(Node {
                flex_grow: 1.0,
                min_height: px(0),
                padding: UiRect::all(px(12)),
                overflow: Overflow::scroll_y(),
                ..column()
            })
            .with_children(|bag| {
                bag.spawn((EmptyList, Text::new("Your inventory is empty."), font(16.0)));
            });
        panel.spawn((
            PanelText::Detail,
            Text::default(),
            font(15.0),
            Node {
                flex_shrink: 0.0,
                ..default()
            },
        ));
        panel
            .spawn((
                DropControls,
                Node {
                    display: Display::None,
                    flex_shrink: 0.0,
                    ..column()
                },
            ))
            .with_children(|prompt| {
                prompt.spawn((PanelText::DropTitle, Text::default(), font(17.0)));
                prompt
                    .spawn(Node {
                        column_gap: px(8),
                        row_gap: px(6),
                        flex_wrap: FlexWrap::Wrap,
                        ..default()
                    })
                    .with_children(|quantity| {
                        action_button(quantity, InventoryAction::Minus, Text::new("-"));
                        quantity
                            .spawn((InventoryAction::Quantity, menu_ui::button()))
                            .insert(Node {
                                min_width: px(96),
                                min_height: px(34),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            })
                            .observe(action)
                            .with_child((
                                PanelText::Amount,
                                Text::default(),
                                font(16.0),
                                Pickable::IGNORE,
                            ));
                        action_button(quantity, InventoryAction::Plus, Text::new("+"));
                        action_button(
                            quantity,
                            InventoryAction::ConfirmDrop,
                            Text::new("Confirm drop"),
                        );
                        action_button(quantity, InventoryAction::CancelDrop, Text::new("Cancel"));
                    });
            });
        panel
            .spawn((
                StrengthWarning,
                menu_ui::interaction_modal(GameState::Playing),
                Visibility::Hidden,
            ))
            .insert(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(14),
                ..default()
            })
            .with_children(|modal| {
                modal.spawn((
                    Text::new("You don't have enough Strength to use this item."),
                    font(18.0),
                ));
                modal
                    .spawn((StrengthWarningClose, menu_ui::button()))
                    .observe(close_strength_warning_click)
                    .with_child((Text::new("Close"), font(16.0), Pickable::IGNORE));
            });
        panel
            .spawn(Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|footer| {
                footer.spawn((PanelText::Load, Text::default(), font(16.0)));
                action_button(
                    footer,
                    InventoryAction::Drop,
                    (PanelText::DropLabel, Text::default()),
                );
            });
    });
}

pub(crate) fn spawn_equipment(page: &mut ChildSpawnerCommands) {
    page.spawn(scroll_pane(MenuTab::Equipment, percent(100)))
        .with_children(|details| {
            details.spawn((Text::new("Equipped"), font(19.0)));
            details.spawn((PanelText::Defense, Text::default(), font(16.0)));
            for slot in EquipmentSlot::ALL {
                details
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_children(|slot_group| {
                        slot_group
                            .spawn(Node {
                                align_items: AlignItems::Center,
                                column_gap: px(8),
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((
                                    PanelText::Slot(slot),
                                    Text::default(),
                                    font(16.0),
                                    Node {
                                        min_width: px(0),
                                        flex_grow: 1.0,
                                        ..default()
                                    },
                                ));
                                action_button(
                                    row,
                                    InventoryAction::ToggleSlot(slot),
                                    (PanelText::SlotAction(slot), Text::default()),
                                );
                            });
                        slot_group
                            .spawn((
                                SlotPicker(slot),
                                Node {
                                    display: Display::None,
                                    flex_wrap: FlexWrap::Wrap,
                                    column_gap: px(6),
                                    row_gap: px(6),
                                    padding: UiRect::all(px(8)),
                                    ..default()
                                },
                                BackgroundColor(menu_ui::BUTTON),
                            ))
                            .with_children(|picker| {
                                picker.spawn((Text::new("Choose equipment"), font(15.0)));
                                for item in ItemKind::GEAR
                                    .into_iter()
                                    .filter(|&item| item.fits_slot(slot))
                                {
                                    action_button(
                                        picker,
                                        InventoryAction::EquipFromSlot(slot, item),
                                        Text::new(format!(
                                            "{} / Strength {}",
                                            item.name(),
                                            item.gear()
                                                .expect("picker items are gear")
                                                .minimum_strength
                                        )),
                                    );
                                }
                                action_button(
                                    picker,
                                    InventoryAction::CloseSlotPicker(slot),
                                    Text::new("Cancel"),
                                );
                            });
                    });
            }
            details.spawn((
                PanelText::Notice(MenuTab::Equipment),
                Text::default(),
                font(14.0),
            ));
        });
}

fn show_strength_warning(
    menu: Res<PlayerMenu>,
    dialogue: Res<Dialogue>,
    mut warnings: Query<&mut Visibility, With<StrengthWarning>>,
) {
    if menu.accepts(MenuTab::Inventory)
        && dialogue.text.is_none()
        && let Ok(mut visibility) = warnings.single_mut()
    {
        visibility.set_if_neq(Visibility::Inherited);
    }
}

fn close_strength_warning_click(
    click: On<PointerClick>,
    buttons: Query<(), With<StrengthWarningClose>>,
    menu: Res<PlayerMenu>,
    mut warnings: Query<&mut Visibility, With<StrengthWarning>>,
) {
    if click.button == PointerButton::Primary
        && menu.is_open()
        && buttons.contains(click.entity)
        && let Ok(mut visibility) = warnings.single_mut()
    {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

fn close_strength_warning(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Res<PlayerMenu>,
    mut warnings: Query<&mut Visibility, With<StrengthWarning>>,
) {
    if menu.is_open()
        && keys.just_pressed(KeyCode::Escape)
        && let Ok(mut visibility) = warnings.single_mut()
    {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

fn dismiss_prompt(
    menu: Res<PlayerMenu>,
    mut ui: ResMut<InventoryUiState>,
    mut notice: ResMut<InventoryNotice>,
) {
    if !menu.is_on(MenuTab::Inventory) && ui.drop_is_open() {
        ui.cancel_drop();
    }
    if !menu.is_open() && !notice.0.is_empty() {
        notice.0.clear();
    }
}

fn action(
    click: On<PointerClick>,
    actions: Query<&InventoryAction>,
    ranks: Query<&AttributeRanks, With<Player>>,
    menu: Res<PlayerMenu>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    mut ui: ResMut<InventoryUiState>,
    mut notice: ResMut<InventoryNotice>,
    mut commands: Commands,
) {
    let Ok(&action) = actions.get(click.entity) else {
        return;
    };
    if !menu.accepts(action.tab()) || click.button != PointerButton::Primary {
        return;
    }
    let strength = ranks
        .single()
        .map_or(DEFAULT_RANK, |ranks| ranks.rank(AttributeId::Strength));
    match action {
        InventoryAction::Select(row) => {
            let item = row.item();
            let present = match row {
                InventoryRow::Equipped(_, slot) => equipment.get(slot) == Some(item),
                _ => inventory.count(item) > 0,
            };
            if !present {
                return;
            }
            ui.selected = Some(row);
            ui.cancel_drop();
            notice.0.clear();
            if click.count == 2 && matches!(row, InventoryRow::BagGear(..)) {
                if !item.meets_strength(strength) {
                    commands.run_system_cached(show_strength_warning);
                } else if equipment.count(item) == 0 || item.fits_slot(EquipmentSlot::OffHand) {
                    commands.trigger(EquipRequest::Quick(item));
                }
            }
        }
        InventoryAction::ToggleSlot(slot) if equipment.get(slot).is_some() => {
            commands.trigger(UnequipRequest(slot));
        }
        InventoryAction::ToggleSlot(slot) => {
            if has_gear_for_slot(slot, &inventory, &equipment) {
                ui.equip_slot = (ui.equip_slot != Some(slot)).then_some(slot);
            }
        }
        InventoryAction::EquipFromSlot(slot, item)
            if can_equip_from_bag(item, slot, &inventory, &equipment)
                && item.meets_strength(strength) =>
        {
            commands.trigger(EquipRequest::InSlot(slot, item));
            ui.equip_slot = None;
        }
        InventoryAction::CloseSlotPicker(slot) if ui.equip_slot == Some(slot) => {
            ui.equip_slot = None;
        }
        InventoryAction::Drop => ui.open_drop(&inventory),
        InventoryAction::CancelDrop => {
            ui.cancel_drop();
        }
        InventoryAction::ConfirmDrop => confirm_drop(&mut ui, &inventory, &mut commands),
        InventoryAction::Minus if ui.drop_is_open() => ui.step(&inventory, -1),
        InventoryAction::Plus if ui.drop_is_open() => ui.step(&inventory, 1),
        InventoryAction::Quantity if ui.drop_is_open() && ui.uses_quantity_picker() => {
            ui.editing = true;
            ui.replace = true;
        }
        _ => (),
    }
}

fn confirm_drop(ui: &mut InventoryUiState, inventory: &Inventory, commands: &mut Commands) {
    if ui.drop_is_open()
        && let (Some(item), Some(quantity)) = (ui.selected_item(), ui.amount(inventory))
    {
        ui.cancel_drop();
        commands.trigger(DropRequest { item, quantity });
    }
}

fn navigate_items(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    menu: Res<PlayerMenu>,
    mut ui: ResMut<InventoryUiState>,
    mut notice: ResMut<InventoryNotice>,
    mut lists: Query<(
        &ItemList,
        &ComputedNode,
        &UiGlobalTransform,
        &mut ScrollPosition,
    )>,
    rows: Query<(&ComputedNode, &UiGlobalTransform)>,
) {
    if !menu.accepts(MenuTab::Inventory)
        || ui.drop_is_open()
        || keys.just_pressed(KeyCode::Escape)
        || bindings.just_pressed(Control::Drop, &keys)
    {
        return;
    }
    let up = bindings.just_pressed(Control::MenuUp, &keys);
    let down = bindings.just_pressed(Control::MenuDown, &keys);
    if up == down {
        return;
    }

    let Ok((items, list, list_transform, mut scroll)) = lists.single_mut() else {
        return;
    };
    let Some(index) = items
        .rows
        .iter()
        .position(|&(row, _)| Some(row) == ui.selected)
    else {
        return;
    };
    let next = if down {
        Some(index + 1)
    } else {
        index.checked_sub(1)
    };
    let Some(&(key, entity)) = next.and_then(|index| items.rows.get(index)) else {
        return;
    };
    ui.selected = Some(key);
    notice.0.clear();

    if let Ok((row, row_transform)) = rows.get(entity) {
        let offset = row_transform.translation.y - list_transform.translation.y;
        let top_overflow = offset - row.size().y * 0.5 + list.size().y * 0.5;
        let bottom_overflow = offset + row.size().y * 0.5 - list.size().y * 0.5;
        let adjustment = if top_overflow < 0.0 {
            top_overflow
        } else {
            bottom_overflow.max(0.0)
        };
        if adjustment != 0.0 {
            scroll.0.y =
                ((list.scroll_position.y + adjustment) * list.inverse_scale_factor()).max(0.0);
        }
    }
}

fn edit_quantity(
    mut events: MessageReader<KeyboardInput>,
    mut ui: ResMut<InventoryUiState>,
    inventory: Res<Inventory>,
    menu: Res<PlayerMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    mut commands: Commands,
) {
    if !menu.accepts(MenuTab::Inventory) || keys.just_pressed(KeyCode::Escape) {
        events.clear();
        return;
    }
    if bindings.just_pressed(Control::Drop, &keys) {
        ui.open_drop(&inventory);
        events.clear();
        return;
    }
    if ui.drop_is_open() && ui.uses_quantity_picker() {
        if bindings.just_pressed(Control::MenuLeft, &keys) {
            ui.step(&inventory, -1);
        }
        if bindings.just_pressed(Control::MenuRight, &keys) {
            ui.step(&inventory, 1);
        }
    }
    for event in events.read() {
        if event.state != ButtonState::Pressed || !ui.drop_is_open() {
            continue;
        }
        match &event.logical_key {
            Key::Enter if !event.repeat => confirm_drop(&mut ui, &inventory, &mut commands),
            Key::ArrowUp => ui.step(&inventory, 1),
            Key::ArrowDown => ui.step(&inventory, -1),
            Key::Backspace if ui.editing => {
                if ui.replace {
                    ui.quantity.clear();
                } else {
                    ui.quantity.pop();
                }
                ui.replace = false;
            }
            Key::Delete if ui.editing => {
                ui.quantity.clear();
                ui.replace = false;
            }
            _ if ui.editing => {
                if let Some(text) = &event.text
                    && text.chars().all(|c| c.is_ascii_digit())
                {
                    if ui.replace {
                        ui.quantity.clear();
                    }
                    ui.replace = false;
                    if ui.quantity.len() + text.len() <= 10 {
                        ui.quantity.push_str(text);
                    }
                }
            }
            _ => (),
        }
    }
}

/// Refresh the views when their shared inputs change or the menu is opened.
fn inventory_panel_changed(
    attributes: Query<
        (),
        (
            With<Player>,
            Or<(Changed<AttributeRanks>, Changed<CarryLimits>)>,
        ),
    >,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    ui: Res<InventoryUiState>,
    notice: Res<InventoryNotice>,
    bindings: Res<Keybinds>,
    menu: Res<PlayerMenu>,
) -> bool {
    menu.is_open()
        && (menu.is_changed()
            || !attributes.is_empty()
            || inventory.is_changed()
            || equipment.is_changed()
            || ui.is_changed()
            || notice.is_changed()
            || bindings.is_changed())
}

fn reconcile_selection(
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    lists: Query<&ItemList>,
    mut ui: ResMut<InventoryUiState>,
) {
    if let Ok(list) = lists.single()
        && !list.rows.iter().any(|&(row, _)| Some(row) == ui.selected)
    {
        let item = ui.selected_item();
        let rows = || list.rows.iter().map(|&(row, _)| row);
        let selected = rows()
            .find(|row| matches!(row, InventoryRow::BagGear(kind, _) if Some(*kind) == item))
            .or_else(|| rows().find(|row| Some(row.item()) == item))
            .or_else(|| rows().next());
        if ui.selected != selected {
            ui.selected = selected;
        }
        if ui.drop_is_open() {
            ui.cancel_drop();
        }
    }
    if ui.equip_slot.is_some_and(|slot| {
        equipment.get(slot).is_some() || !has_gear_for_slot(slot, &inventory, &equipment)
    }) {
        ui.equip_slot = None;
    }
    if ui.drop_is_open() && !selection_is_droppable(&ui, &inventory) {
        ui.cancel_drop();
    }
}

fn selection_is_droppable(ui: &InventoryUiState, inventory: &Inventory) -> bool {
    ui.selected
        .is_some_and(|row| row.available_to_drop(inventory) > 0)
}

fn refresh_controls(
    ranks: Query<&AttributeRanks, With<Player>>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    ui: Res<InventoryUiState>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<DropControls>>,
        Query<(&InventoryAction, &mut Node, &mut BackgroundColor)>,
        Query<&mut Node, With<EmptyList>>,
        Query<(&SlotPicker, &mut Node)>,
    )>,
) {
    let strength = ranks
        .single()
        .map_or(DEFAULT_RANK, |ranks| ranks.rank(AttributeId::Strength));
    let droppable = selection_is_droppable(&ui, &inventory);
    for mut node in &mut nodes.p0() {
        set_display(&mut node, ui.drop_is_open());
    }
    for (action, mut node, mut color) in &mut nodes.p1() {
        let (shown, selected, disabled) = match *action {
            InventoryAction::Select(row) => (true, Some(row) == ui.selected, false),
            InventoryAction::ToggleSlot(slot) => (
                slot != EquipmentSlot::OffHand || !equipment.off_hand_reserved(),
                false,
                equipment.get(slot).is_none() && !has_gear_for_slot(slot, &inventory, &equipment),
            ),
            InventoryAction::EquipFromSlot(slot, item) => (
                can_equip_from_bag(item, slot, &inventory, &equipment),
                false,
                !item.meets_strength(strength),
            ),
            InventoryAction::Drop => (true, false, !droppable),
            InventoryAction::ConfirmDrop => (true, false, ui.amount(&inventory).is_none()),
            InventoryAction::Quantity => (ui.uses_quantity_picker(), ui.editing, false),
            InventoryAction::Minus | InventoryAction::Plus => {
                (ui.uses_quantity_picker(), false, false)
            }
            _ => (true, false, false),
        };
        set_display(&mut node, shown);
        color.set_if_neq(BackgroundColor(if disabled {
            DISABLED
        } else if selected {
            SELECTED
        } else {
            BUTTON
        }));
    }
    for mut node in &mut nodes.p2() {
        set_display(&mut node, ui.selected.is_none());
    }
    for (picker, mut node) in &mut nodes.p3() {
        set_display(&mut node, ui.equip_slot == Some(picker.0));
    }
}

fn refresh_labels(
    limits: Query<&CarryLimits, With<Player>>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    ui: Res<InventoryUiState>,
    notice: Res<InventoryNotice>,
    bindings: Res<Keybinds>,
    menu: Res<PlayerMenu>,
    mut texts: Query<(&PanelText, &mut Text)>,
) {
    let limits = limits.single().copied().unwrap_or_default();
    let capacity = limits.capacity_kg;
    let weight = inventory.carried_weight(&equipment);
    let load_status = if weight > limits.movement_limit_kg {
        " / Cannot move"
    } else if weight > capacity {
        " / Walking only"
    } else {
        ""
    };
    for (kind, mut text) in &mut texts {
        let label = match *kind {
            PanelText::ItemQuantity(row) => row.quantity(&inventory).to_string(),
            PanelText::ItemWeight(row) => {
                format!(
                    "{} kg",
                    row.item().weight() * u64::from(row.quantity(&inventory))
                )
            }
            PanelText::Slot(slot) => slot_label(slot, &equipment),
            PanelText::SlotAction(slot) => {
                if equipment.get(slot).is_some() {
                    "Unequip".into()
                } else {
                    "Equip".into()
                }
            }
            PanelText::Defense => format!("Defense {}", equipment.total_defense()),
            PanelText::Load => format!("Load {weight}/{capacity} kg{load_status}"),
            PanelText::Detail => ui.selected_item().map_or_else(String::new, item_detail),
            PanelText::Amount => format!("{}{}", ui.quantity, if ui.editing { "_" } else { "" }),
            PanelText::DropLabel => format!("Drop [{}]", bindings.get(Control::Drop).label()),
            PanelText::DropTitle => match ui.selected {
                Some(row @ InventoryRow::Stack(item)) => format!(
                    "Quantity to drop for {} (up to {})",
                    item.name(),
                    row.available_to_drop(&inventory)
                ),
                Some(InventoryRow::BagGear(item, _)) => format!("Drop one {}?", item.name()),
                _ => String::new(),
            },
            PanelText::Notice(tab) if menu.is_on(tab) => {
                if ui.drop_is_open() && ui.amount(&inventory).is_none() {
                    "Enter a quantity from 1 to the available count.".into()
                } else {
                    notice.0.clone()
                }
            }
            PanelText::Notice(_) => String::new(),
        };
        text.set_if_neq(Text::new(label));
    }
}

fn item_detail(item: ItemKind) -> String {
    let weight = item.weight();
    let description = item.description();
    if let Some(weapon) = item.weapon() {
        let minimum = item
            .gear()
            .expect("weapons have an equipment definition")
            .minimum_strength;
        let hands = match weapon.handedness() {
            Handedness::OneHanded => "One-handed",
            Handedness::TwoHanded => "Two-handed",
        };
        return format!(
            "{weight} kg each / {hands} / Reach {:.1} m / Strength {minimum} / {description}",
            weapon.reach()
        );
    }
    match item.gear() {
        Some(gear) => format!(
            "{weight} kg each / Defense {} / Strength {} / {description}",
            gear.defense, gear.minimum_strength
        ),
        None => format!("{weight} kg each / {description}"),
    }
}

fn slot_label(slot: EquipmentSlot, equipment: &Equipment) -> String {
    if slot == EquipmentSlot::OffHand && equipment.off_hand_reserved() {
        return "Off hand: Used by main-hand weapon".into();
    }
    equipment.get(slot).map_or_else(
        || format!("{}: Empty", slot.name()),
        |item| {
            let gear = item.gear().expect("only gear can occupy equipment slots");
            if item.weapon().is_some() {
                format!("{}: {}", slot.name(), item.name())
            } else {
                format!("{}: {} (+{})", slot.name(), item.name(), gear.defense)
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_double_click_equips_available_gear_and_swaps_the_worn_item() {
        use crate::{GameState, equipment::EquipmentPlugin, smoke::pointer_click};
        use bevy::state::app::StatesPlugin;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, EquipmentPlugin))
            .insert_state(GameState::Playing)
            .init_resource::<InventoryNotice>()
            .init_resource::<InventoryUiState>();
        let world = app.world_mut();
        world.spawn((
            Player::default(),
            AttributeRanks::default().with_rank(AttributeId::Strength, 10),
        ));
        let mut menu = PlayerMenu::default();
        menu.show(MenuTab::Inventory);
        world.insert_resource(menu);
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::FieldArmor, 1));
        assert!(bag.add(ItemKind::ReinforcedArmor, 1));
        assert!(bag.add(ItemKind::Battery, 1));
        assert_eq!(
            world
                .resource_mut::<Equipment>()
                .equip(&mut bag, ItemKind::FieldArmor, 10),
            Ok(true)
        );
        world.insert_resource(bag);

        let armor = world
            .spawn(InventoryAction::Select(InventoryRow::BagGear(
                ItemKind::ReinforcedArmor,
                0,
            )))
            .observe(action)
            .id();
        let battery = world
            .spawn(InventoryAction::Select(InventoryRow::Stack(
                ItemKind::Battery,
            )))
            .observe(action)
            .id();
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();
        world.trigger(pointer_click(armor, window, camera));
        assert_eq!(
            world.resource::<InventoryUiState>().selected_item(),
            Some(ItemKind::ReinforcedArmor)
        );
        world.trigger(PointerClick {
            count: 2,
            ..pointer_click(battery, window, camera)
        });
        assert_eq!(
            world.resource::<Equipment>().get(EquipmentSlot::Body),
            Some(ItemKind::FieldArmor)
        );
        assert!(world.resource::<InventoryNotice>().0.is_empty());

        for _ in 0..2 {
            world.trigger(PointerClick {
                count: 2,
                ..pointer_click(armor, window, camera)
            });
            world.flush();
        }
        assert_eq!(
            world.resource::<Equipment>().get(EquipmentSlot::Body),
            Some(ItemKind::ReinforcedArmor)
        );
        assert_eq!(
            world
                .resource::<Inventory>()
                .count(ItemKind::ReinforcedArmor),
            0
        );
        assert_eq!(world.resource::<Inventory>().count(ItemKind::FieldArmor), 1);
    }

    #[test]
    fn double_clicking_strength_gated_gear_shows_notice_without_equipping() {
        use crate::{GameState, equipment::EquipmentPlugin, smoke::pointer_click};
        use bevy::state::app::StatesPlugin;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            EquipmentPlugin,
            InventoryUiPlugin,
        ))
        .insert_state(GameState::Playing)
        .init_resource::<InventoryNotice>()
        .init_resource::<InventoryUiState>();
        let world = app.world_mut();
        world.spawn((Player::default(), AttributeRanks::default()));
        let warning = world.spawn((StrengthWarning, Visibility::Hidden)).id();
        let mut menu = PlayerMenu::default();
        menu.show(MenuTab::Inventory);
        world.insert_resource(menu);
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::ReinforcedArmor, 1));
        world.insert_resource(bag);
        let armor = world
            .spawn(InventoryAction::Select(InventoryRow::BagGear(
                ItemKind::ReinforcedArmor,
                0,
            )))
            .observe(action)
            .id();
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();

        world.trigger(PointerClick {
            count: 2,
            ..pointer_click(armor, window, camera)
        });
        world.flush();

        assert_eq!(world.resource::<Equipment>().get(EquipmentSlot::Body), None);
        assert_eq!(
            world
                .resource::<Inventory>()
                .count(ItemKind::ReinforcedArmor),
            1
        );
        assert_eq!(
            world.get::<Visibility>(warning),
            Some(&Visibility::Inherited)
        );
    }

    #[test]
    fn each_shortsword_row_can_be_selected_and_equipped_separately() {
        use crate::{GameState, equipment::EquipmentPlugin, smoke::pointer_click};
        use bevy::state::app::StatesPlugin;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            EquipmentPlugin,
            InventoryUiPlugin,
        ))
        .insert_state(GameState::Playing)
        .init_resource::<InventoryNotice>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<Keybinds>()
        .add_message::<KeyboardInput>();
        let sword = ItemKind::Shortsword;
        let mut bag = Inventory::default();
        assert!(bag.add(sword, 2));
        let mut menu = PlayerMenu::default();
        menu.show(MenuTab::Inventory);
        let world = app.world_mut();
        world.spawn(Player::default());
        world.insert_resource(bag);
        world.insert_resource(menu);
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();
        world.commands().spawn_empty().with_children(spawn);
        world.flush();
        app.update();

        let world = app.world_mut();
        let mut lists = world.query::<&ItemList>();
        let [(first_key, first), (second_key, second)] =
            lists.single(world).expect("inventory list").rows.as_slice()
        else {
            panic!("two swords must have two rows");
        };
        let (first_key, first, second_key, second) = (*first_key, *first, *second_key, *second);
        assert_ne!(first_key, second_key);
        assert_eq!(
            world.resource::<InventoryUiState>().selected,
            Some(first_key)
        );
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyS);
        app.update();

        let world = app.world_mut();
        assert_eq!(
            world.resource::<InventoryUiState>().selected,
            Some(second_key)
        );
        world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
        world.trigger(PointerClick {
            count: 2,
            ..pointer_click(second, window, camera)
        });
        world.flush();
        app.update();

        let world = app.world_mut();
        assert_eq!(
            world.resource::<Equipment>().get(EquipmentSlot::MainHand),
            Some(sword)
        );
        assert_eq!(
            world.resource::<Equipment>().get(EquipmentSlot::OffHand),
            None
        );
        assert_eq!(world.resource::<Inventory>().count(sword), 1);
        let rows = &lists.single(world).expect("inventory list").rows;
        assert_eq!(
            rows[0].0,
            InventoryRow::Equipped(sword, EquipmentSlot::MainHand)
        );
        assert_eq!(rows[1], (first_key, first));
        assert_eq!(
            world.resource::<InventoryUiState>().selected,
            Some(first_key)
        );
        let equipped = rows[0].1;
        world.trigger(PointerClick {
            count: 2,
            ..pointer_click(equipped, window, camera)
        });
        world.flush();
        assert_eq!(
            world.resource::<Equipment>().get(EquipmentSlot::OffHand),
            None
        );
        world.trigger(PointerClick {
            count: 2,
            ..pointer_click(first, window, camera)
        });
        world.flush();
        app.update();

        let world = app.world();
        let equipment = world.resource::<Equipment>();
        assert_eq!(equipment.get(EquipmentSlot::MainHand), Some(sword));
        assert_eq!(equipment.get(EquipmentSlot::OffHand), Some(sword));
        assert_eq!(world.resource::<Inventory>().count(sword), 0);
        assert!(
            lists
                .single(world)
                .expect("inventory list")
                .rows
                .iter()
                .map(|&(row, _)| row)
                .eq([
                    InventoryRow::Equipped(sword, EquipmentSlot::MainHand),
                    InventoryRow::Equipped(sword, EquipmentSlot::OffHand),
                ])
        );
    }

    #[test]
    fn empty_equipment_slot_opens_a_compatible_gear_picker() {
        use crate::{GameState, equipment::EquipmentPlugin, smoke::pointer_click};
        use bevy::state::app::StatesPlugin;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, EquipmentPlugin))
            .insert_state(GameState::Playing)
            .init_resource::<InventoryNotice>()
            .init_resource::<InventoryUiState>();
        let world = app.world_mut();
        world.spawn((
            Player::default(),
            AttributeRanks::default().with_rank(AttributeId::Strength, 10),
        ));
        let mut menu = PlayerMenu::default();
        menu.show(MenuTab::Equipment);
        world.insert_resource(menu);
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::FieldHelmet, 1));
        assert!(bag.add(ItemKind::FieldArmor, 1));
        world.insert_resource(bag);

        let open_picker = world
            .spawn(InventoryAction::ToggleSlot(EquipmentSlot::Helmet))
            .observe(action)
            .id();
        let equip_helmet = world
            .spawn(InventoryAction::EquipFromSlot(
                EquipmentSlot::Helmet,
                ItemKind::FieldHelmet,
            ))
            .observe(action)
            .id();
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();

        world.trigger(pointer_click(open_picker, window, camera));
        world.flush();
        assert_eq!(
            world.resource::<InventoryUiState>().equip_slot,
            Some(EquipmentSlot::Helmet)
        );

        world.trigger(pointer_click(equip_helmet, window, camera));
        world.flush();
        assert_eq!(
            world.resource::<Equipment>().get(EquipmentSlot::Helmet),
            Some(ItemKind::FieldHelmet)
        );
        assert_eq!(
            world.resource::<Inventory>().count(ItemKind::FieldHelmet),
            0
        );
        assert_eq!(world.resource::<InventoryUiState>().equip_slot, None);
        assert_eq!(world.resource::<Inventory>().count(ItemKind::FieldArmor), 1);
    }

    #[test]
    fn drop_prompt_limits_gear_to_one_and_validates_stack_quantities() {
        let mut inventory = Inventory::default();
        assert!(inventory.add(ItemKind::FieldArmor, 2));
        assert!(inventory.add(ItemKind::Battery, 2));
        assert!(inventory.add(ItemKind::BeaconCore, 1));
        let mut ui = InventoryUiState {
            selected: Some(InventoryRow::BagGear(ItemKind::FieldArmor, 0)),
            ..default()
        };
        ui.open_drop(&inventory);
        assert!(ui.drop_is_open());
        assert!(!ui.uses_quantity_picker());
        assert!(!ui.editing);
        ui.step(&inventory, 1);
        assert_eq!(ui.amount(&inventory), Some(1));
        assert!(ui.cancel_drop());

        ui.selected = Some(InventoryRow::Stack(ItemKind::Battery));
        ui.open_drop(&inventory);
        assert!(ui.uses_quantity_picker());
        for text in ["", "0", "3", "-1", "1.5", "4294967296"] {
            ui.quantity = text.into();
            assert_eq!(ui.amount(&inventory), None);
        }
        ui.quantity = "1".into();
        ui.step(&inventory, 1);
        assert_eq!(ui.amount(&inventory), Some(2));
        assert!(ui.cancel_drop());

        for row in [
            InventoryRow::Equipped(ItemKind::FieldArmor, EquipmentSlot::Body),
            InventoryRow::Stack(ItemKind::BeaconCore),
        ] {
            ui.selected = Some(row);
            ui.open_drop(&inventory);
            assert!(!ui.drop_is_open());
            assert_eq!(ui.amount(&inventory), None);
        }
    }
}
