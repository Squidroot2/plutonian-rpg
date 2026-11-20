use crate::{
    GameState,
    equipment::{EquipRequest, Equipment, EquipmentSlot, UnequipRequest},
    inventory::{DropRequest, Inventory, InventoryNotice, ItemKind},
};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::AccumulatedMouseScroll,
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
    Close,
}
#[derive(Component)]
pub(crate) struct InventoryScreen;
#[derive(Component)]
struct InventoryPanel;
#[derive(Component)]
struct BagList;
#[derive(Component)]
struct SlotText(EquipmentSlot);
#[derive(Component)]
struct DefenseText;
#[derive(Component)]
struct EquipLabel;
#[derive(Component)]
struct ItemLabel(ItemKind);
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
            .add_systems(Startup, setup)
            .add_systems(OnEnter(GameState::Inventory), open)
            .add_systems(Update, visibility)
            .add_systems(
                Update,
                (edit_quantity, sync)
                    .chain()
                    .run_if(in_state(GameState::Inventory)),
            );
    }
}
fn font(size: f32) -> (TextFont, TextColor) {
    (
        TextFont {
            font_size: size.into(),
            ..default()
        },
        TextColor(Color::srgb(0.78, 0.9, 0.94)),
    )
}
fn button(action: InventoryAction) -> impl Bundle {
    (
        action,
        Node {
            min_height: px(34),
            padding: UiRect::axes(px(10), px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(Color::srgb(0.05, 0.13, 0.17)),
    )
}
fn setup(mut commands: Commands) {
    commands
        .spawn((
            InventoryScreen,
            Visibility::Hidden,
            GlobalZIndex(50),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.005, 0.012, 0.025, 0.94)),
        ))
        .with_children(|screen| {
            screen
                .spawn((
                    InventoryPanel,
                    Node {
                        width: percent(92),
                        max_width: px(600),
                        max_height: percent(94),
                        padding: UiRect::all(px(16)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.025, 0.055, 0.075)),
                ))
                .with_children(|panel| {
                    panel.spawn((Text::new("INVENTORY"), font(24.0)));
                    panel.spawn((DefenseText, Text::default(), font(16.0)));
                    for slot in EquipmentSlot::ALL {
                        panel
                            .spawn(Node {
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::SpaceBetween,
                                column_gap: px(10),
                                flex_shrink: 0.0,
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((SlotText(slot), Text::default(), font(16.0)));
                                row.spawn(button(InventoryAction::Unequip(slot)))
                                    .observe(action)
                                    .with_children(|b| {
                                        b.spawn((
                                            Text::new("Unequip"),
                                            font(16.0),
                                            Pickable::IGNORE,
                                        ));
                                    });
                            });
                    }
                    panel.spawn((Text::new("BAG"), font(16.0)));
                    panel
                        .spawn((
                            BagList,
                            ScrollPosition::default(),
                            Node {
                                height: px(160),
                                min_height: px(56),
                                overflow: Overflow::scroll_y(),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(6),
                                ..default()
                            },
                        ))
                        .with_children(|bag| {
                            bag.spawn((EmptyText, Text::new("Your bag is empty."), font(16.0)));
                            for item in ItemKind::ALL {
                                bag.spawn(button(InventoryAction::Select(item)))
                                    .observe(action)
                                    .with_children(|row| {
                                        row.spawn((
                                            ItemLabel(item),
                                            Text::default(),
                                            font(16.0),
                                            Pickable::IGNORE,
                                        ));
                                    });
                            }
                        });
                    panel.spawn((DetailText, Text::default(), font(14.0)));
                    panel
                        .spawn(button(InventoryAction::Equip))
                        .observe(action)
                        .with_children(|b| {
                            b.spawn((EquipLabel, Text::new("Equip"), font(16.0), Pickable::IGNORE));
                        });
                    panel
                        .spawn((
                            DropControls,
                            Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8),
                                flex_shrink: 0.0,
                                ..default()
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
                                    for (label, kind) in [
                                        ("-", InventoryAction::Minus),
                                        ("1", InventoryAction::Quantity),
                                        ("+", InventoryAction::Plus),
                                    ] {
                                        let mut node = quantity.spawn(button(kind));
                                        node.insert(Node {
                                            min_width: px(if kind == InventoryAction::Quantity {
                                                110
                                            } else {
                                                42
                                            }),
                                            min_height: px(34),
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::Center,
                                            padding: UiRect::all(px(8)),
                                            ..default()
                                        });
                                        node.observe(action).with_children(|child| {
                                            let mut text = child.spawn((
                                                Text::new(label),
                                                font(16.0),
                                                Pickable::IGNORE,
                                            ));
                                            if kind == InventoryAction::Quantity {
                                                text.insert(AmountText);
                                            }
                                        });
                                    }
                                });
                            controls
                                .spawn(button(InventoryAction::Drop))
                                .observe(action)
                                .with_children(|b| {
                                    b.spawn((
                                        Text::new("Drop selected quantity"),
                                        font(16.0),
                                        Pickable::IGNORE,
                                    ));
                                });
                        });
                    panel.spawn((NoticeText, Text::default(), font(14.0)));
                    panel
                        .spawn(button(InventoryAction::Close))
                        .observe(action)
                        .with_children(|b| {
                            b.spawn((Text::new("Close"), font(16.0), Pickable::IGNORE));
                        });
                });
        });
}
fn open(
    mut ui: ResMut<InventoryUiState>,
    inventory: Res<Inventory>,
    mut notice: ResMut<InventoryNotice>,
    mut panels: Query<&mut ScrollPosition, With<BagList>>,
) {
    if ui.selected.is_none_or(|item| inventory.count(item) == 0) {
        ui.selected = ItemKind::ALL
            .into_iter()
            .find(|item| inventory.count(*item) > 0);
    }
    ui.quantity = "1".into();
    ui.editing = false;
    notice.0.clear();
    for mut scroll in &mut panels {
        scroll.0 = Vec2::ZERO;
    }
}
fn visibility(
    state: Res<State<GameState>>,
    mut screens: Query<&mut Visibility, With<InventoryScreen>>,
) {
    for mut visible in &mut screens {
        *visible = if *state.get() == GameState::Inventory {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
fn action(
    click: On<PointerClick>,
    actions: Query<&InventoryAction>,
    state: Res<State<GameState>>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    mut ui: ResMut<InventoryUiState>,
    mut notice: ResMut<InventoryNotice>,
    mut next: ResMut<NextState<GameState>>,
    mut commands: Commands,
) {
    if *state.get() != GameState::Inventory {
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
        InventoryAction::Close => next.set(GameState::Playing),
        _ => (),
    }
}
fn edit_quantity(
    mut events: MessageReader<KeyboardInput>,
    mut ui: ResMut<InventoryUiState>,
    inventory: Res<Inventory>,
    scroll: Res<AccumulatedMouseScroll>,
    mut panels: Query<&mut ScrollPosition, With<BagList>>,
) {
    for mut panel in &mut panels {
        panel.0.y = (panel.0.y - scroll.delta.y * 30.0).max(0.0);
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
    )>,
) {
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
            Color::srgb(0.065, 0.075, 0.08)
        } else if selected {
            Color::srgb(0.09, 0.25, 0.3)
        } else {
            Color::srgb(0.05, 0.13, 0.17)
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
    for mut text in &mut texts.p1() {
        **text = ui.selected.map_or(String::new(), |item| {
            item.gear().map_or_else(
                || item.description().into(),
                |gear| {
                    format!(
                        "{} / Defense {}\n{}",
                        gear.slot.name(),
                        gear.defense,
                        item.description()
                    )
                },
            )
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
        **text = format!("Total defense: {}", equipment.total_defense());
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
