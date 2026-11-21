use crate::{
    GameState,
    quest::{QuestJournal, QuestOutcome, QuestStatus},
    ui::text_style,
};
use bevy::{
    input::mouse::AccumulatedMouseScroll,
    prelude::*,
    ui::{FocusPolicy, RelativeCursorPosition},
};

pub(crate) struct QuestUiPlugin;

#[derive(Resource, Default)]
pub(crate) struct QuestLog {
    pub(crate) open: bool,
    pub(crate) close_requested: bool,
    selected: Option<String>,
}

impl QuestLog {
    pub(crate) fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Component)]
struct Screen;
#[derive(Component)]
struct QuestList;
#[derive(Component)]
struct QuestDetails;
#[derive(Component)]
pub(crate) struct QuestRow {
    pub(crate) id: String,
    pub(crate) status: QuestStatus,
}
#[derive(Component)]
struct Close;

const PANEL: Color = Color::srgb(0.025, 0.055, 0.075);
const BUTTON: Color = Color::srgb(0.055, 0.105, 0.14);
const SELECTED: Color = Color::srgb(0.11, 0.26, 0.31);

impl Plugin for QuestUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QuestLog>()
            .add_systems(Startup, setup)
            .add_systems(Update, (sync, scroll).chain())
            .add_systems(Last, close)
            .add_systems(OnExit(GameState::Playing), |mut log: ResMut<QuestLog>| {
                log.open = false;
                log.close_requested = false;
            });
    }
}

fn column() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(8),
        ..default()
    }
}

fn scroll_pane(marker: impl Component, width: Val) -> impl Bundle {
    (
        marker,
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
        BackgroundColor(Color::srgb(0.018, 0.038, 0.052)),
    )
}

fn button() -> impl Bundle {
    (
        Node {
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
            Screen,
            GlobalZIndex(45),
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
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
                            flex_shrink: 0.0,
                            ..default()
                        })
                        .with_children(|header| {
                            header.spawn((Text::new("Quest log"), text_style(26.0)));
                            header
                                .spawn((Close, button()))
                                .observe(close_button)
                                .with_child((
                                    Text::new("Close  [Tab / Esc]"),
                                    text_style(16.0),
                                    Pickable::IGNORE,
                                ));
                        });
                    panel
                        .spawn(Node {
                            flex_grow: 1.0,
                            min_height: px(0),
                            column_gap: px(12),
                            ..default()
                        })
                        .with_children(|body| {
                            body.spawn(scroll_pane(QuestList, percent(32)));
                            body.spawn(scroll_pane(QuestDetails, percent(68)));
                        });
                });
        });
}

fn sync(
    mut commands: Commands,
    journal: Res<QuestJournal>,
    mut log: ResMut<QuestLog>,
    mut screen: Query<&mut Visibility, With<Screen>>,
    list: Query<(Entity, Option<&Children>), With<QuestList>>,
    mut details: Query<(Entity, Option<&Children>, &mut ScrollPosition), With<QuestDetails>>,
    mut rows: Query<(&QuestRow, &mut BackgroundColor)>,
    mut previous: Local<Option<String>>,
) {
    let Ok(mut visibility) = screen.single_mut() else {
        return;
    };
    let opening = *visibility == Visibility::Hidden;
    visibility.set_if_neq(if log.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
    if !log.open || !(opening || log.is_changed() || journal.is_changed()) {
        return;
    }
    let mut entries: Vec<_> = journal.entries().collect();
    entries.sort_by_key(|(_, progress)| progress.status());
    if !entries
        .iter()
        .any(|(quest, _)| Some(&quest.id) == log.selected.as_ref())
    {
        log.selected = entries.first().map(|(quest, _)| quest.id.clone());
    }
    if (opening || journal.is_changed())
        && let Ok((entity, children)) = list.single()
    {
        clear(&mut commands, children);
        commands.entity(entity).with_children(|list| {
            for (heading, status) in [
                ("Active", QuestStatus::Active),
                ("Completed", QuestStatus::Completed),
                ("Failed", QuestStatus::Failed),
            ] {
                list.spawn((Text::new(heading), text_style(17.0), Pickable::IGNORE));
                let mut any = false;
                for (quest, progress) in &entries {
                    if progress.status() != status {
                        continue;
                    }
                    any = true;
                    let background = row_background(log.selected.as_ref() == Some(&quest.id));
                    list.spawn((
                        button(),
                        QuestRow {
                            id: quest.id.clone(),
                            status,
                        },
                    ))
                    .insert(background)
                    .observe(select)
                    .with_child((
                        Text::new(&quest.title),
                        text_style(17.0),
                        Pickable::IGNORE,
                    ));
                }
                if !any {
                    list.spawn((Text::new("None"), text_style(15.0), Pickable::IGNORE));
                }
            }
        });
    }
    let selection_changed = *previous != log.selected;
    if selection_changed {
        for (row, mut background) in &mut rows {
            background.set_if_neq(row_background(log.selected.as_ref() == Some(&row.id)));
        }
        previous.clone_from(&log.selected);
    }
    if !(opening || selection_changed || journal.is_changed()) {
        return;
    }
    if let Ok((entity, children, mut scroll)) = details.single_mut() {
        clear(&mut commands, children);
        if selection_changed {
            scroll.0 = Vec2::ZERO;
        }
        commands.entity(entity).with_children(|details| {
            if let Some((quest, progress)) = entries
                .iter()
                .find(|(quest, _)| Some(&quest.id) == log.selected.as_ref())
            {
                details.spawn((Text::new(&quest.title), text_style(23.0)));
                details.spawn((Text::new(&quest.summary), text_style(17.0)));
                match &progress.outcome {
                    QuestOutcome::Active(index) => {
                        let objective = &quest.objectives[*index];
                        details.spawn((
                            Text::new(format!("Current objective\n{}", objective.title)),
                            text_style(19.0),
                        ));
                        details.spawn((Text::new(&objective.directions), text_style(17.0)));
                    }
                    QuestOutcome::Completed => {
                        details.spawn((Text::new(&quest.completion), text_style(17.0)));
                    }
                    QuestOutcome::Failed(reason) => {
                        details.spawn((Text::new("Quest failed"), text_style(19.0)));
                        details.spawn((Text::new(reason), text_style(17.0)));
                    }
                }
                if !progress.completed.is_empty() {
                    details.spawn((Text::new("Completed steps"), text_style(19.0)));
                    for &index in &progress.completed {
                        let objective = &quest.objectives[index];
                        details.spawn((
                            Text::new(format!("• {}", objective.title)),
                            text_style(16.0),
                        ));
                    }
                }
            } else {
                details.spawn((
                    Text::new("No quests in your journal yet."),
                    text_style(17.0),
                ));
            }
        });
    }
}

fn clear(commands: &mut Commands, children: Option<&Children>) {
    for child in children.into_iter().flatten() {
        commands.entity(*child).despawn();
    }
}

fn row_background(selected: bool) -> BackgroundColor {
    BackgroundColor(if selected { SELECTED } else { BUTTON })
}

fn select(click: On<PointerClick>, rows: Query<&QuestRow>, mut log: ResMut<QuestLog>) {
    if log.open
        && click.button == PointerButton::Primary
        && let Ok(row) = rows.get(click.entity)
    {
        log.selected = Some(row.id.clone());
    }
}

fn close_button(click: On<PointerClick>, mut log: ResMut<QuestLog>) {
    if log.open && click.button == PointerButton::Primary {
        log.close_requested = true;
    }
}

fn close(mut log: ResMut<QuestLog>) {
    if log.close_requested {
        log.open = false;
        log.close_requested = false;
    }
}

fn scroll(
    log: Res<QuestLog>,
    wheel: Res<AccumulatedMouseScroll>,
    mut panes: Query<
        (&RelativeCursorPosition, &mut ScrollPosition),
        Or<(With<QuestList>, With<QuestDetails>)>,
    >,
) {
    if log.open && wheel.delta.y != 0.0 {
        for (cursor, mut position) in &mut panes {
            if cursor.cursor_over() {
                position.0.y = (position.0.y - wheel.delta.y * 32.0).max(0.0);
            }
        }
    }
}
