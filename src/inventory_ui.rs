use crate::{
    controls::{Control, Keybinds},
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
    ui::UiGlobalTransform,
};

pub struct InventoryUiPlugin;

#[derive(Resource, Default)]
pub struct InventoryUiState {
    pub selected: Option<ItemKind>,
    pub quantity: String,
    gear: Option<ItemKind>,
    drop_open: bool,
    editing: bool,
    replace: bool,
}

impl InventoryUiState {
    pub fn amount(&self, inventory: &Inventory) -> Option<u32> {
        let available = available_to_drop(self.selected?, inventory);
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
                .is_some_and(|item| available_to_drop(item, inventory) > 0)
        {
            self.drop_open = true;
            self.quantity = "1".into();
            self.editing = true;
            self.replace = true;
        }
    }

    fn step(&mut self, inventory: &Inventory, direction: i32) {
        let Some(item) = self.selected else { return };
        let max = available_to_drop(item, inventory);
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

fn available_to_drop(item: ItemKind, inventory: &Inventory) -> u32 {
    if item == ItemKind::BeaconCore {
        0
    } else {
        inventory.count(item)
    }
}

fn equipped(item: ItemKind, equipment: &Equipment) -> bool {
    item.gear()
        .is_some_and(|gear| equipment.get(gear.slot) == Some(item))
}

fn owned(item: ItemKind, inventory: &Inventory, equipment: &Equipment) -> u64 {
    u64::from(inventory.count(item)) + u64::from(equipped(item, equipment))
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryAction {
    Select(ItemKind),
    SelectGear(ItemKind),
    Equip,
    Unequip(EquipmentSlot),
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
            Self::SelectGear(_) | Self::Equip | Self::Unequip(_) => MenuTab::Equipment,
            _ => MenuTab::Inventory,
        }
    }
}

#[derive(Component)]
enum PanelText {
    ItemQuantity(ItemKind),
    ItemWeight(ItemKind),
    ItemStatus(ItemKind),
    Gear(ItemKind),
    Slot(EquipmentSlot),
    Defense,
    Load,
    Detail,
    Amount,
    DropLabel,
    DropTitle,
    Notice(MenuTab),
}

#[derive(Component)]
struct EmptyList(MenuTab);
#[derive(Component)]
struct DropControls;
#[derive(Component)]
struct ItemList;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryUiState>()
            .add_systems(Update, (navigate_items, edit_quantity, sync).chain())
            .add_systems(Last, dismiss_prompt.after(menu_ui::close));
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
            .spawn((ItemList, scroll_pane(MenuTab::Inventory, percent(100))))
            .insert(Node {
                flex_grow: 1.0,
                min_height: px(0),
                padding: UiRect::all(px(12)),
                overflow: Overflow::scroll_y(),
                ..column()
            })
            .with_children(|bag| {
                bag.spawn((
                    EmptyList(MenuTab::Inventory),
                    Text::new("Your inventory is empty."),
                    font(16.0),
                ));
                for item in ItemKind::ALL {
                    bag.spawn((InventoryAction::Select(item), menu_ui::button()))
                        .observe(action)
                        .with_children(|row| {
                            table_cells(
                                row,
                                [
                                    (Text::new(item.name()), None),
                                    (Text::default(), Some(PanelText::ItemQuantity(item))),
                                    (Text::default(), Some(PanelText::ItemWeight(item))),
                                    (Text::default(), Some(PanelText::ItemStatus(item))),
                                ],
                            )
                        });
                }
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
        panel.spawn((
            PanelText::Notice(MenuTab::Inventory),
            Text::default(),
            font(14.0),
        ));
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
    page.spawn(scroll_pane(MenuTab::Equipment, percent(38)))
        .with_children(|list| {
            list.spawn((Text::new("Available gear"), font(19.0)));
            list.spawn((
                EmptyList(MenuTab::Equipment),
                Text::new("No gear in your bag."),
                font(16.0),
            ));
            for item in ItemKind::GEAR {
                action_button(
                    list,
                    InventoryAction::SelectGear(item),
                    (PanelText::Gear(item), Text::default()),
                );
            }
            action_button(list, InventoryAction::Equip, Text::new("Equip selected"));
        });
    page.spawn(scroll_pane(MenuTab::Equipment, percent(62)))
        .with_children(|details| {
            details.spawn((Text::new("Equipped"), font(19.0)));
            details.spawn((PanelText::Defense, Text::default(), font(16.0)));
            for slot in EquipmentSlot::ALL {
                details
                    .spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: px(8),
                        flex_shrink: 0.0,
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
                        action_button(row, InventoryAction::Unequip(slot), Text::new("Unequip"));
                    });
            }
            details.spawn((
                PanelText::Notice(MenuTab::Equipment),
                Text::default(),
                font(14.0),
            ));
        });
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
    match action {
        InventoryAction::Select(item) if owned(item, &inventory, &equipment) > 0 => {
            ui.selected = Some(item);
            ui.cancel_drop();
            notice.0.clear();
            if click.count == 2
                && item.gear().is_some()
                && inventory.count(item) > 0
                && !equipped(item, &equipment)
            {
                commands.trigger(EquipRequest(item));
            }
        }
        InventoryAction::SelectGear(item)
            if inventory.count(item) > 0 && !equipped(item, &equipment) =>
        {
            ui.gear = Some(item);
            notice.0.clear();
        }
        InventoryAction::Equip => {
            if let Some(item) = ui.gear.filter(|item| !equipped(*item, &equipment)) {
                commands.trigger(EquipRequest(item));
            }
        }
        InventoryAction::Unequip(slot) if equipment.get(slot).is_some() => {
            commands.trigger(UnequipRequest(slot));
        }
        InventoryAction::Drop => ui.open_drop(&inventory),
        InventoryAction::CancelDrop => {
            ui.cancel_drop();
        }
        InventoryAction::ConfirmDrop => confirm_drop(&mut ui, &inventory, &mut commands),
        InventoryAction::Minus if ui.drop_is_open() => ui.step(&inventory, -1),
        InventoryAction::Plus if ui.drop_is_open() => ui.step(&inventory, 1),
        InventoryAction::Quantity if ui.drop_is_open() => {
            ui.editing = true;
            ui.replace = true;
        }
        _ => (),
    }
}

fn confirm_drop(ui: &mut InventoryUiState, inventory: &Inventory, commands: &mut Commands) {
    if ui.drop_is_open()
        && let (Some(item), Some(quantity)) = (ui.selected, ui.amount(inventory))
    {
        ui.cancel_drop();
        commands.trigger(DropRequest { item, quantity });
    }
}

fn navigate_items(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    menu: Res<PlayerMenu>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    mut ui: ResMut<InventoryUiState>,
    mut notice: ResMut<InventoryNotice>,
    mut lists: Query<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<ItemList>>,
    rows: Query<(&InventoryAction, &ComputedNode, &UiGlobalTransform)>,
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

    let selected = ui.selected;
    let items = ItemKind::ALL
        .into_iter()
        .filter(|&item| owned(item, &inventory, &equipment) > 0);
    let next = if down {
        items.skip_while(|&item| Some(item) != selected).nth(1)
    } else {
        items
            .rev()
            .skip_while(|&item| Some(item) != selected)
            .nth(1)
    };
    let Some(item) = next else { return };
    ui.selected = Some(item);
    notice.0.clear();

    if let Ok((list, list_transform, mut scroll)) = lists.single_mut()
        && let Some((_, row, row_transform)) = rows
            .iter()
            .find(|(action, _, _)| **action == InventoryAction::Select(item))
    {
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
    if ui.drop_is_open() {
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

fn sync(
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    mut ui: ResMut<InventoryUiState>,
    notice: Res<InventoryNotice>,
    bindings: Res<Keybinds>,
    menu: Res<PlayerMenu>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<DropControls>>,
        Query<(&InventoryAction, &mut Node, &mut BackgroundColor)>,
        Query<(&EmptyList, &mut Node)>,
    )>,
    mut texts: Query<(&PanelText, &mut Text)>,
) {
    if !menu.is_open()
        || !(menu.is_changed()
            || inventory.is_changed()
            || equipment.is_changed()
            || ui.is_changed()
            || notice.is_changed()
            || bindings.is_changed())
    {
        return;
    }
    if ui
        .selected
        .is_none_or(|item| owned(item, &inventory, &equipment) == 0)
    {
        ui.selected = ItemKind::ALL
            .into_iter()
            .find(|&item| owned(item, &inventory, &equipment) > 0);
        ui.cancel_drop();
    }
    if ui
        .gear
        .is_none_or(|item| inventory.count(item) == 0 || equipped(item, &equipment))
    {
        ui.gear = ItemKind::GEAR
            .into_iter()
            .find(|&item| inventory.count(item) > 0 && !equipped(item, &equipment));
    }
    let droppable = ui
        .selected
        .is_some_and(|item| available_to_drop(item, &inventory) > 0);
    if !droppable {
        ui.cancel_drop();
    }
    for mut node in &mut nodes.p0() {
        node.display = display(ui.drop_is_open());
    }
    for (action, mut node, mut color) in &mut nodes.p1() {
        let (shown, selected, disabled) = match *action {
            InventoryAction::Select(item) => (
                owned(item, &inventory, &equipment) > 0,
                Some(item) == ui.selected,
                false,
            ),
            InventoryAction::SelectGear(item) => (
                inventory.count(item) > 0,
                Some(item) == ui.gear,
                equipped(item, &equipment),
            ),
            InventoryAction::Equip => (ui.gear.is_some(), false, false),
            InventoryAction::Unequip(slot) => (true, false, equipment.get(slot).is_none()),
            InventoryAction::Drop => (true, false, !droppable),
            InventoryAction::ConfirmDrop => (true, false, ui.amount(&inventory).is_none()),
            InventoryAction::Quantity => (true, ui.editing, false),
            _ => (true, false, false),
        };
        node.display = display(shown);
        color.0 = if disabled {
            DISABLED
        } else if selected {
            SELECTED
        } else {
            BUTTON
        };
    }
    for (kind, mut node) in &mut nodes.p2() {
        node.display = display(match kind.0 {
            MenuTab::Inventory => ui.selected.is_none(),
            _ => !ItemKind::GEAR
                .into_iter()
                .any(|item| inventory.count(item) > 0),
        });
    }
    let weight = inventory.carried_weight(&equipment);
    let load_status = if weight > IMMOBILE_WEIGHT {
        " / Cannot move"
    } else if weight > CARRY_CAPACITY {
        " / Walking only"
    } else {
        ""
    };
    for (kind, mut text) in &mut texts {
        **text = match *kind {
            PanelText::ItemQuantity(item) => owned(item, &inventory, &equipment).to_string(),
            PanelText::ItemWeight(item) => {
                format!("{} kg", item.weight() * owned(item, &inventory, &equipment))
            }
            PanelText::ItemStatus(item) if equipped(item, &equipment) => "Equipped".into(),
            PanelText::ItemStatus(ItemKind::BeaconCore) => "Quest item".into(),
            PanelText::ItemStatus(_) => String::new(),
            PanelText::Gear(item) => format!("{}  x{}", item.name(), inventory.count(item)),
            PanelText::Slot(slot) => slot_label(slot, &equipment),
            PanelText::Defense => format!("Defense {}", equipment.total_defense()),
            PanelText::Load => format!("Load {weight}/{CARRY_CAPACITY} kg{load_status}"),
            PanelText::Detail => ui.selected.map_or_else(String::new, item_detail),
            PanelText::Amount => format!("{}{}", ui.quantity, if ui.editing { "_" } else { "" }),
            PanelText::DropLabel => format!("Drop [{}]", bindings.get(Control::Drop).label()),
            PanelText::DropTitle => ui.selected.map_or(String::new(), |item| {
                format!(
                    "Quantity to drop for {} (up to {})",
                    item.name(),
                    available_to_drop(item, &inventory)
                )
            }),
            PanelText::Notice(tab) if menu.is_on(tab) => {
                if ui.drop_is_open() && ui.amount(&inventory).is_none() {
                    "Enter a quantity from 1 to the available count.".into()
                } else {
                    notice.0.clone()
                }
            }
            PanelText::Notice(_) => String::new(),
        };
    }
}

fn display(shown: bool) -> Display {
    if shown { Display::Flex } else { Display::None }
}

fn item_detail(item: ItemKind) -> String {
    let weight = item.weight();
    let description = item.description();
    match item.gear() {
        Some(gear) if gear.slot == EquipmentSlot::Weapon => {
            format!(
                "{weight} kg each / Reach {:.1} m / {description}",
                crate::spear::REACH
            )
        }
        Some(gear) => format!(
            "{weight} kg each / Defense {} / {description}",
            gear.defense
        ),
        None => format!("{weight} kg each / {description}"),
    }
}

fn slot_label(slot: EquipmentSlot, equipment: &Equipment) -> String {
    equipment.get(slot).map_or_else(
        || format!("{}: Empty", slot.name()),
        |item| {
            let gear = item.gear().expect("only gear can occupy equipment slots");
            if slot == EquipmentSlot::Weapon {
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
                .equip(&mut bag, ItemKind::FieldArmor),
            Ok(true)
        );
        world.insert_resource(bag);

        let armor = world
            .spawn(InventoryAction::Select(ItemKind::ReinforcedArmor))
            .observe(action)
            .id();
        let battery = world
            .spawn(InventoryAction::Select(ItemKind::Battery))
            .observe(action)
            .id();
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();
        world.trigger(pointer_click(armor, window, camera));
        assert_eq!(
            world.resource::<InventoryUiState>().selected,
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
    fn quantity_prompt_validates_spares_and_protects_equipped_copies_and_the_core() {
        let mut inventory = Inventory::default();
        let mut equipment = Equipment::default();
        inventory.add(ItemKind::FieldArmor, 3);
        equipment
            .equip(&mut inventory, ItemKind::FieldArmor)
            .expect("fixture armor");
        let mut ui = InventoryUiState {
            selected: Some(ItemKind::FieldArmor),
            ..default()
        };
        assert_eq!(owned(ItemKind::FieldArmor, &inventory, &equipment), 3);
        assert!(equipped(ItemKind::FieldArmor, &equipment));
        assert!(!ui.drop_is_open());
        ui.open_drop(&inventory);
        for text in ["", "0", "3", "-1", "1.5", "4294967296"] {
            ui.quantity = text.into();
            assert_eq!(ui.amount(&inventory), None);
        }
        ui.quantity = "1".into();
        ui.step(&inventory, 1);
        assert_eq!(ui.amount(&inventory), Some(2));
        assert!(ui.cancel_drop());
        assert!(inventory.remove(ItemKind::FieldArmor, 2));
        ui.open_drop(&inventory);
        assert!(!ui.drop_is_open());
        assert_eq!(owned(ItemKind::FieldArmor, &inventory, &equipment), 1);
        inventory.add(ItemKind::BeaconCore, 1);
        ui.selected = Some(ItemKind::BeaconCore);
        ui.open_drop(&inventory);
        assert!(!ui.drop_is_open());
        assert_eq!(ui.amount(&inventory), None);
    }
}
