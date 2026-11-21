use crate::{GameState, inventory_ui, quest_ui, ui::text_style};
use bevy::{
    input::mouse::AccumulatedMouseScroll,
    prelude::*,
    ui::{FocusPolicy, RelativeCursorPosition},
};

pub(crate) struct PlayerMenuPlugin;

#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuTab {
    #[default]
    Quest,
    Inventory,
    Equipment,
    Character,
}

impl MenuTab {
    const ALL: [Self; 4] = [
        Self::Quest,
        Self::Inventory,
        Self::Equipment,
        Self::Character,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Quest => "Quest",
            Self::Inventory => "Inventory",
            Self::Equipment => "Equipment",
            Self::Character => "Character",
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct PlayerMenu {
    open: bool,
    close_requested: bool,
    tab: MenuTab,
}

impl PlayerMenu {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn is_on(&self, tab: MenuTab) -> bool {
        self.open && self.tab == tab
    }

    pub(crate) fn accepts(&self, tab: MenuTab) -> bool {
        self.is_on(tab) && !self.close_requested
    }

    pub(crate) fn show(&mut self, tab: MenuTab) {
        self.open = true;
        self.close_requested = false;
        self.tab = tab;
    }

    // Keep capturing raw input until the end of the closing frame.
    pub(crate) fn shortcuts(
        &mut self,
        tab: bool,
        inventory: bool,
        quest_log: bool,
        escape: bool,
    ) -> bool {
        if escape {
            if self.open {
                self.close_requested = true;
                return true;
            }
            return false;
        }
        if tab {
            if self.open {
                self.close_requested = true;
            } else {
                self.show(self.tab);
            }
        } else if inventory {
            if self.is_on(MenuTab::Inventory) {
                self.close_requested = true;
            } else {
                self.show(MenuTab::Inventory);
            }
        } else if quest_log {
            if self.is_on(MenuTab::Quest) {
                self.close_requested = true;
            } else {
                self.show(MenuTab::Quest);
            }
        }
        self.open
    }

    fn hide(&mut self) {
        self.open = false;
        self.close_requested = false;
    }
}

#[derive(Component)]
pub(crate) struct PlayerMenuScreen;

#[derive(Component)]
struct Page(MenuTab);

#[derive(Component)]
struct ScrollPane(MenuTab);

pub(crate) const PANEL: Color = Color::srgb(0.025, 0.055, 0.075);
const PANE: Color = Color::srgb(0.018, 0.038, 0.052);
pub(crate) const BUTTON: Color = Color::srgb(0.055, 0.105, 0.14);
pub(crate) const SELECTED: Color = Color::srgb(0.11, 0.26, 0.31);
pub(crate) const DISABLED: Color = Color::srgb(0.065, 0.075, 0.08);

impl Plugin for PlayerMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerMenu>()
            .add_systems(Startup, setup)
            .add_systems(Update, (visibility, scroll).chain())
            .add_systems(Last, close)
            .add_systems(
                OnExit(GameState::Playing),
                |mut menu: ResMut<PlayerMenu>| menu.hide(),
            );
    }
}

pub(crate) fn column() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(8),
        ..default()
    }
}

pub(crate) fn scroll_pane(tab: MenuTab, width: Val) -> impl Bundle {
    (ScrollPane(tab), scrollable_pane(width))
}

pub(crate) fn scrollable_pane(width: Val) -> impl Bundle {
    (
        ScrollPosition::default(),
        RelativeCursorPosition::default(),
        Node {
            width,
            min_width: px(0),
            min_height: px(0),
            padding: UiRect::all(px(12)),
            overflow: Overflow::scroll_y(),
            ..column()
        },
        FocusPolicy::Pass,
        BackgroundColor(PANE),
    )
}

pub(crate) fn button() -> impl Bundle {
    (
        Node {
            min_height: px(34),
            padding: UiRect::axes(px(12), px(8)),
            flex_shrink: 0.0,
            ..default()
        },
        FocusPolicy::Pass,
        BackgroundColor(BUTTON),
    )
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            PlayerMenuScreen,
            GlobalZIndex(45),
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.005, 0.012, 0.025, 0.32)),
        ))
        .with_children(|screen| {
            screen
                .spawn((
                    Node {
                        width: percent(94),
                        max_width: px(940),
                        height: percent(76),
                        max_height: px(650),
                        padding: UiRect::all(px(16)),
                        ..column()
                    },
                    BackgroundColor(PANEL),
                ))
                .with_children(|panel| {
                    panel
                        .spawn(Node {
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            column_gap: px(8),
                            row_gap: px(6),
                            flex_wrap: FlexWrap::Wrap,
                            flex_shrink: 0.0,
                            ..default()
                        })
                        .with_children(|header| {
                            header
                                .spawn(Node {
                                    column_gap: px(6),
                                    flex_wrap: FlexWrap::Wrap,
                                    ..default()
                                })
                                .with_children(|tabs| {
                                    for tab in MenuTab::ALL {
                                        tabs.spawn((tab, button())).observe(select).with_child((
                                            Text::new(tab.label()),
                                            text_style(17.0),
                                            Pickable::IGNORE,
                                        ));
                                    }
                                });
                            header.spawn(button()).observe(close_button).with_child((
                                Text::new("Close  [Tab / Esc]"),
                                text_style(16.0),
                                Pickable::IGNORE,
                            ));
                        });
                    panel
                        .spawn(Node {
                            flex_grow: 1.0,
                            min_height: px(0),
                            ..default()
                        })
                        .with_children(|body| {
                            for tab in MenuTab::ALL {
                                body.spawn((
                                    Page(tab),
                                    Visibility::Hidden,
                                    Node {
                                        position_type: PositionType::Absolute,
                                        width: percent(100),
                                        height: percent(100),
                                        column_gap: px(12),
                                        ..default()
                                    },
                                ))
                                .with_children(|page| match tab {
                                    MenuTab::Quest => quest_ui::spawn(page),
                                    MenuTab::Inventory => inventory_ui::spawn(page),
                                    MenuTab::Equipment => inventory_ui::spawn_equipment(page),
                                    MenuTab::Character => character(page),
                                });
                            }
                        });
                });
        });
}

fn character(page: &mut ChildSpawnerCommands) {
    page.spawn(scroll_pane(MenuTab::Character, percent(100)))
        .with_children(|pane| {
            pane.spawn((Text::new("Plutonian"), text_style(23.0)));
            pane.spawn((Text::new("Explorer"), text_style(17.0)));
            pane.spawn((Text::new("Level 1"), text_style(17.0)));
            pane.spawn((Text::new("Placeholder character data"), text_style(14.0)));
        });
}

fn select(click: On<PointerClick>, tabs: Query<&MenuTab>, mut menu: ResMut<PlayerMenu>) {
    if menu.is_open()
        && !menu.close_requested
        && click.button == PointerButton::Primary
        && let Ok(&tab) = tabs.get(click.entity)
    {
        menu.show(tab);
    }
}

fn close_button(click: On<PointerClick>, mut menu: ResMut<PlayerMenu>) {
    if menu.is_open() && click.button == PointerButton::Primary {
        menu.close_requested = true;
    }
}

pub(crate) fn close(mut menu: ResMut<PlayerMenu>) {
    if menu.close_requested {
        menu.hide();
    }
}

fn visibility(
    menu: Res<PlayerMenu>,
    mut screen: Query<&mut Visibility, With<PlayerMenuScreen>>,
    mut pages: Query<(&Page, &mut Visibility), Without<PlayerMenuScreen>>,
    mut tabs: Query<(&MenuTab, &mut BackgroundColor)>,
) {
    if !menu.is_changed() {
        return;
    }
    for mut visibility in &mut screen {
        visibility.set_if_neq(visible(menu.open));
    }
    for (page, mut visibility) in &mut pages {
        visibility.set_if_neq(visible(menu.is_on(page.0)));
    }
    for (tab, mut background) in &mut tabs {
        background.set_if_neq(BackgroundColor(if menu.is_on(*tab) {
            SELECTED
        } else {
            BUTTON
        }));
    }
}

fn visible(show: bool) -> Visibility {
    if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn scroll(
    menu: Res<PlayerMenu>,
    wheel: Res<AccumulatedMouseScroll>,
    mut panes: Query<(&ScrollPane, &RelativeCursorPosition, &mut ScrollPosition)>,
) {
    if wheel.delta.y != 0.0 {
        for (pane, cursor, mut position) in &mut panes {
            if menu.accepts(pane.0) {
                scroll_hovered(wheel.delta.y, cursor, &mut position);
            }
        }
    }
}

pub(crate) fn scroll_hovered(
    delta: f32,
    cursor: &RelativeCursorPosition,
    position: &mut ScrollPosition,
) {
    if cursor.cursor_over() {
        position.0.y = (position.0.y - delta * 32.0).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_reopens_the_last_page_and_inventory_toggles_its_own_page() {
        let mut menu = PlayerMenu::default();
        assert!(menu.shortcuts(true, false, false, false));
        assert!(menu.is_on(MenuTab::Quest));
        menu.show(MenuTab::Character);
        assert!(menu.shortcuts(true, false, false, false));
        assert!(menu.is_open());
        menu.hide();
        assert!(menu.shortcuts(true, false, false, false));
        assert!(menu.is_on(MenuTab::Character));

        assert!(menu.shortcuts(false, true, false, false));
        assert!(menu.accepts(MenuTab::Inventory));
        assert!(menu.shortcuts(false, true, false, false));
        assert!(menu.is_open());
        assert!(!menu.accepts(MenuTab::Inventory));
        menu.hide();
        assert!(menu.shortcuts(true, false, false, false));
        assert!(menu.is_on(MenuTab::Inventory));

        assert!(menu.shortcuts(false, false, true, false));
        assert!(menu.is_on(MenuTab::Quest));
        assert!(menu.shortcuts(false, false, true, false));
        assert!(!menu.accepts(MenuTab::Quest));
    }
}
