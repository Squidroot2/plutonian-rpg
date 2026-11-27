use avian3d::prelude::SpatialQuery;
use bevy::{input::mouse::AccumulatedMouseScroll, prelude::*, ui::RelativeCursorPosition};

use crate::{
    GameState,
    content::Items,
    dungeon::Area,
    interaction::{ActiveChest, ActiveRack, INTERACT_HEIGHT, INTERACT_RANGE, unobstructed},
    inventory::{Inventory, ItemKind},
    player::Player,
    player::{PlayerRead, PlayerWrite},
    storage::{ChestId, ChestStorage, RACK_NAME, WeaponRack, WeaponRackStorage},
    ui::menu_ui::{
        button, interaction_modal, scroll_hovered, scrollable_pane, set_display, wide_panel,
    },
    ui::text_style,
};

pub(crate) struct StorageUiPlugin;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageSide {
    Backpack,
    Chest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferAmount {
    One,
    Stack,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageAction {
    Transfer(StorageSide, ItemKind, TransferAmount),
    Close,
}

#[derive(Component)]
struct StoragePane;

#[derive(Component)]
struct StackRow(StorageSide, ItemKind);

#[derive(Component)]
struct StackLabel(StorageSide, ItemKind);

#[derive(Component)]
struct EmptyPane(StorageSide);

#[derive(Component)]
struct StorageNotice;

impl Plugin for StorageUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Storage), open)
            .add_systems(
                OnExit(GameState::Storage),
                |mut chest: ResMut<ActiveChest>, mut rack: ResMut<ActiveRack>| {
                    chest.0 = None;
                    rack.0 = None;
                },
            )
            .add_systems(
                Update,
                (refresh, scroll).run_if(in_state(GameState::Storage)),
            );
    }
}

/// Which container the storage screen is transferring with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Destination {
    Chest(ChestId),
    Rack,
}

impl Destination {
    fn title(self) -> &'static str {
        match self {
            Self::Chest(chest) => chest.name(),
            Self::Rack => RACK_NAME,
        }
    }
}

fn open(
    items: Res<Items>,
    active: Res<ActiveChest>,
    active_rack: Res<ActiveRack>,
    players: Query<(Entity, &Player, &Transform)>,
    chests: Query<(&ChestId, &Transform)>,
    racks: Query<&Transform, With<WeaponRack>>,
    spatial: SpatialQuery,
    mut next: ResMut<NextState<GameState>>,
    mut commands: Commands,
) {
    let in_reach = |entity: Entity, position: &Transform| {
        let (player_entity, player, transform) = players.single().ok()?;
        let origin = transform.translation + Vec3::Y * INTERACT_HEIGHT;
        let delta = position.translation - origin;
        let distance = delta.length();
        (player.area == Area::Surface
            && distance < INTERACT_RANGE
            && unobstructed(&spatial, origin, delta, distance, player_entity, entity))
        .then_some(())
    };
    let destination = active
        .0
        .and_then(|entity| {
            let (id, position) = chests.get(entity).ok()?;
            in_reach(entity, position).map(|()| Destination::Chest(*id))
        })
        .or_else(|| {
            active_rack.0.and_then(|entity| {
                in_reach(entity, racks.get(entity).ok()?).map(|()| Destination::Rack)
            })
        });
    let Some(destination) = destination else {
        next.set(GameState::Playing);
        return;
    };
    // The rack moves one weapon at a time, so it offers no stack action.
    let rack = destination == Destination::Rack;
    let stack = (!rack).then_some("Store stack");
    let take_stack = (!rack).then_some("Take stack");

    commands
        .spawn(interaction_modal(GameState::Storage))
        .with_children(|screen| {
            screen.spawn(wide_panel()).with_children(|panel| {
                panel
                    .spawn(Node {
                        justify_content: JustifyContent::FlexEnd,
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_children(|header| {
                        action_button(header, StorageAction::Close, "Close [Esc]");
                    });
                panel
                    .spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        flex_grow: 1.0,
                        min_height: px(0),
                        ..default()
                    })
                    .with_children(|lists| {
                        spawn_pane(
                            &items,
                            lists,
                            StorageSide::Backpack,
                            "Backpack",
                            "Store 1",
                            stack,
                        );
                        spawn_pane(
                            &items,
                            lists,
                            StorageSide::Chest,
                            destination.title(),
                            "Take 1",
                            take_stack,
                        );
                    });
                panel.spawn((
                    StorageNotice,
                    Text::default(),
                    text_style(14.0),
                    Pickable::IGNORE,
                ));
            });
        });
}

fn spawn_pane(
    items: &Items,
    parent: &mut ChildSpawnerCommands,
    side: StorageSide,
    title: &'static str,
    one: &'static str,
    stack: Option<&'static str>,
) {
    parent
        .spawn((StoragePane, scrollable_pane(percent(49))))
        .with_children(|pane| {
            pane.spawn((Text::new(title), text_style(21.0), Pickable::IGNORE));
            pane.spawn((
                EmptyPane(side),
                Text::new("Empty"),
                text_style(16.0),
                Pickable::IGNORE,
            ));
            for item in items.ids() {
                pane.spawn((
                    StackRow(side, item),
                    Node {
                        display: Display::None,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: px(8),
                        row_gap: px(4),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .with_children(|row| {
                    row.spawn((
                        StackLabel(side, item),
                        Text::default(),
                        text_style(16.0),
                        Pickable::IGNORE,
                    ));
                    row.spawn(Node {
                        column_gap: px(4),
                        ..default()
                    })
                    .with_children(|actions| {
                        action_button(
                            actions,
                            StorageAction::Transfer(side, item, TransferAmount::One),
                            one,
                        );
                        if let Some(stack) = stack {
                            action_button(
                                actions,
                                StorageAction::Transfer(side, item, TransferAmount::Stack),
                                stack,
                            );
                        }
                    });
                });
            }
        });
}

fn action_button(parent: &mut ChildSpawnerCommands, action: StorageAction, label: &'static str) {
    parent.spawn((action, button())).observe(click).with_child((
        Text::new(label),
        text_style(14.0),
        Pickable::IGNORE,
    ));
}

fn click(
    event: On<PointerClick>,
    actions: Query<&StorageAction>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    active: Res<ActiveChest>,
    active_rack: Res<ActiveRack>,
    chests: Query<&ChestId>,
    racks: Query<(), With<WeaponRack>>,
    items: Res<Items>,
    mut storage: ResMut<ChestStorage>,
    mut rack_storage: ResMut<WeaponRackStorage>,
    mut backpack: PlayerWrite<Inventory>,
    mut notice: Query<&mut Text, With<StorageNotice>>,
) {
    if *state.get() != GameState::Storage
        || !matches!(*next, NextState::Unchanged)
        || event.button != PointerButton::Primary
    {
        return;
    }
    let Ok(&action) = actions.get(event.entity) else {
        return;
    };
    let destination = active
        .0
        .and_then(|entity| chests.get(entity).ok().copied().map(Destination::Chest))
        .or_else(|| {
            active_rack
                .0
                .and_then(|entity| racks.get(entity).ok().map(|()| Destination::Rack))
        });
    let Some(destination) = destination else {
        next.set(GameState::Playing);
        return;
    };
    let StorageAction::Transfer(side, item, amount) = action else {
        next.set(GameState::Playing);
        return;
    };
    let message = match destination {
        Destination::Chest(chest) => {
            transfer_chest(&mut storage, chest, &mut backpack, side, item, amount)
        }
        Destination::Rack => transfer_rack(&items, &mut rack_storage, &mut backpack, side, item),
    };
    if let Ok(mut notice) = notice.single_mut() {
        **notice = message;
    }
}

fn transfer_chest(
    storage: &mut ChestStorage,
    chest: ChestId,
    backpack: &mut Inventory,
    side: StorageSide,
    item: ItemKind,
    amount: TransferAmount,
) -> String {
    let chest = storage.get_mut(chest);
    let (source, destination, destination_name) = match side {
        StorageSide::Backpack => (backpack, chest, "chest"),
        StorageSide::Chest => (chest, backpack, "backpack"),
    };
    let available = source.count(item);
    let quantity = match amount {
        TransferAmount::One => 1,
        TransferAmount::Stack => available,
    };
    if available == 0 {
        "That stack is empty.".into()
    } else if source.transfer_to(destination, item, quantity) {
        String::new()
    } else {
        format!("The {destination_name} cannot hold this stack.")
    }
}

/// Move one weapon between the backpack and the rack, refusing
/// non-weapons and deposits into a full rack without touching either side.
fn transfer_rack(
    items: &Items,
    rack: &mut WeaponRackStorage,
    backpack: &mut Inventory,
    side: StorageSide,
    item: ItemKind,
) -> String {
    match side {
        StorageSide::Backpack => {
            if items[item].weapon().is_none() {
                "The weapon rack only holds weapons.".into()
            } else if backpack.count(item) == 0 {
                "That stack is empty.".into()
            } else if rack.is_full() {
                "The weapon rack is full.".into()
            } else {
                debug_assert!(backpack.remove(item, 1));
                debug_assert!(rack.deposit(item));
                String::new()
            }
        }
        StorageSide::Chest => {
            if rack.count(item) == 0 {
                "That stack is empty.".into()
            } else if !backpack.add(item, 1) {
                "The backpack cannot hold this stack.".into()
            } else {
                debug_assert!(rack.withdraw(item));
                String::new()
            }
        }
    }
}

fn refresh(
    items: Res<Items>,
    active: Res<ActiveChest>,
    active_rack: Res<ActiveRack>,
    chests: Query<&ChestId>,
    racks: Query<(), With<WeaponRack>>,
    storage: Res<ChestStorage>,
    rack_storage: Res<WeaponRackStorage>,
    backpack: PlayerRead<Inventory>,
    mut rows: Query<(&StackRow, &mut Node), Without<EmptyPane>>,
    mut labels: Query<(&StackLabel, &mut Text)>,
    mut empty: Query<(&EmptyPane, &mut Node)>,
    mut next: ResMut<NextState<GameState>>,
) {
    let destination = active
        .0
        .and_then(|entity| chests.get(entity).ok().copied().map(Destination::Chest))
        .or_else(|| {
            active_rack
                .0
                .and_then(|entity| racks.get(entity).ok().map(|()| Destination::Rack))
        });
    let Some(destination) = destination else {
        next.set(GameState::Playing);
        return;
    };
    if !(active.is_changed()
        || active_rack.is_changed()
        || storage.is_changed()
        || rack_storage.is_changed()
        || backpack.is_changed())
    {
        return;
    }
    let rack_inventory = rack_storage.as_inventory();
    let inventory = |side| match (side, destination) {
        (StorageSide::Backpack, _) => &*backpack,
        (StorageSide::Chest, Destination::Chest(chest)) => storage.get(chest),
        (StorageSide::Chest, Destination::Rack) => &rack_inventory,
    };
    for (&StackRow(side, item), mut row) in &mut rows {
        set_display(&mut row, inventory(side).count(item) > 0);
    }
    for (&StackLabel(side, item), mut label) in &mut labels {
        let quantity = inventory(side).count(item);
        if quantity > 0 {
            label.set_if_neq(Text::new(format!("{} x{quantity}", items[item].name())));
        }
    }
    for (&EmptyPane(side), mut node) in &mut empty {
        let empty = items
            .ids()
            .into_iter()
            .all(|item| inventory(side).count(item) == 0);
        set_display(&mut node, empty);
    }
}

fn scroll(
    wheel: Res<AccumulatedMouseScroll>,
    mut panes: Query<(&RelativeCursorPosition, &mut ScrollPosition), With<StoragePane>>,
) {
    if wheel.delta.y != 0.0 {
        for (cursor, mut position) in &mut panes {
            scroll_hovered(wheel.delta.y, cursor, &mut position);
        }
    }
}
