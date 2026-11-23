use avian3d::prelude::SpatialQuery;
use bevy::{input::mouse::AccumulatedMouseScroll, prelude::*, ui::RelativeCursorPosition};

use crate::{
    GameState,
    content::Items,
    dungeon::Area,
    interaction::{ActiveChest, INTERACT_HEIGHT, INTERACT_RANGE, unobstructed},
    inventory::{Inventory, ItemKind},
    player::Player,
    storage::{ChestId, ChestStorage},
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
                |mut active: ResMut<ActiveChest>| {
                    active.0 = None;
                },
            )
            .add_systems(
                Update,
                (refresh, scroll).run_if(in_state(GameState::Storage)),
            );
    }
}

fn open(
    items: Res<Items>,
    active: Res<ActiveChest>,
    players: Query<(Entity, &Player, &Transform)>,
    chests: Query<(&ChestId, &Transform)>,
    spatial: SpatialQuery,
    mut next: ResMut<NextState<GameState>>,
    mut commands: Commands,
) {
    let chest = active.0.and_then(|entity| {
        let (id, position) = chests.get(entity).ok()?;
        let (player_entity, player, transform) = players.single().ok()?;
        let origin = transform.translation + Vec3::Y * INTERACT_HEIGHT;
        let delta = position.translation - origin;
        let distance = delta.length();
        (player.area == Area::Surface
            && distance < INTERACT_RANGE
            && unobstructed(&spatial, origin, delta, distance, player_entity, entity))
        .then_some(*id)
    });
    let Some(chest) = chest else {
        next.set(GameState::Playing);
        return;
    };

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
                            "Store stack",
                        );
                        spawn_pane(
                            &items,
                            lists,
                            StorageSide::Chest,
                            chest.name(),
                            "Take 1",
                            "Take stack",
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
    stack: &'static str,
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
                        action_button(
                            actions,
                            StorageAction::Transfer(side, item, TransferAmount::Stack),
                            stack,
                        );
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
    chests: Query<&ChestId>,
    mut storage: ResMut<ChestStorage>,
    mut backpack: ResMut<Inventory>,
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
    let Some(chest) = active.0.and_then(|entity| chests.get(entity).ok().copied()) else {
        next.set(GameState::Playing);
        return;
    };
    let StorageAction::Transfer(side, item, amount) = action else {
        next.set(GameState::Playing);
        return;
    };

    let chest = storage.get_mut(chest);
    let (source, destination, destination_name) = match side {
        StorageSide::Backpack => (&mut *backpack, chest, "chest"),
        StorageSide::Chest => (chest, &mut *backpack, "backpack"),
    };
    let available = source.count(item);
    let quantity = match amount {
        TransferAmount::One => 1,
        TransferAmount::Stack => available,
    };
    let message = if available == 0 {
        "That stack is empty.".into()
    } else if source.transfer_to(destination, item, quantity) {
        String::new()
    } else {
        format!("The {destination_name} cannot hold this stack.")
    };
    if let Ok(mut notice) = notice.single_mut() {
        **notice = message;
    }
}

fn refresh(
    items: Res<Items>,
    active: Res<ActiveChest>,
    chests: Query<&ChestId>,
    storage: Res<ChestStorage>,
    backpack: Res<Inventory>,
    mut rows: Query<(&StackRow, &mut Node), Without<EmptyPane>>,
    mut labels: Query<(&StackLabel, &mut Text)>,
    mut empty: Query<(&EmptyPane, &mut Node)>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Some(chest) = active.0.and_then(|entity| chests.get(entity).ok().copied()) else {
        next.set(GameState::Playing);
        return;
    };
    if !(active.is_changed() || storage.is_changed() || backpack.is_changed()) {
        return;
    }
    let inventory = |side| match side {
        StorageSide::Backpack => &*backpack,
        StorageSide::Chest => storage.get(chest),
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
