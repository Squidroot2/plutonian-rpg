use super::TitleInput;
use crate::{
    GameState,
    content::{Items, Placements},
    save::{RunSave, SaveSlot},
    ui::menu_ui::{modal_overlay, scroll_hovered},
};
use bevy::{
    input::{InputSystems, mouse::AccumulatedMouseScroll},
    prelude::*,
    ui::RelativeCursorPosition,
};
use std::time::{SystemTime, UNIX_EPOCH};

const SAVE_ROW_HEIGHT_PX: f32 = 40.0;

#[derive(Resource, Default)]
pub(crate) struct SavePicker {
    pub(super) open: bool,
    selected: usize,
    confirming: bool,
    pending: Option<PickerAction>,
    error: Option<String>,
}

impl SavePicker {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.selected = 0;
        self.confirming = false;
        self.pending = None;
        self.error = None;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Clone, Copy, Component)]
enum PickerAction {
    Select(usize),
    Load(usize),
    Delete(usize),
    Confirm,
    Cancel,
    Close,
}

#[derive(Component)]
struct PickerScreen;
#[derive(Component)]
struct PickerPane;

pub(super) struct SavePickerPlugin;

impl Plugin for SavePickerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SavePicker>()
            .add_systems(
                PreUpdate,
                keyboard
                    .after(InputSystems)
                    .before(TitleInput)
                    .run_if(in_picker_state),
            )
            .add_systems(
                Update,
                (process, render, scroll).chain().run_if(in_picker_state),
            )
            .add_systems(OnExit(GameState::Paused), despawn_picker_screen);
    }
}

fn in_picker_state(state: Res<State<GameState>>) -> bool {
    matches!(state.get(), GameState::Title | GameState::Paused)
}

fn despawn_picker_screen(mut commands: Commands, screens: Query<Entity, With<PickerScreen>>) {
    for screen in &screens {
        commands.entity(screen).despawn();
    }
}

fn keyboard(keys: Res<ButtonInput<KeyCode>>, slot: Res<SaveSlot>, mut picker: ResMut<SavePicker>) {
    if !picker.open {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        picker.pending = Some(if picker.confirming {
            PickerAction::Cancel
        } else {
            PickerAction::Close
        });
    } else if picker.confirming {
        if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
            picker.pending = Some(PickerAction::Confirm);
        }
    } else if keys.just_pressed(KeyCode::ArrowUp) && !slot.entries.is_empty() {
        picker.pending = Some(PickerAction::Select(picker.selected.saturating_sub(1)));
    } else if keys.just_pressed(KeyCode::ArrowDown) && !slot.entries.is_empty() {
        picker.pending = Some(PickerAction::Select(
            (picker.selected + 1).min(slot.entries.len() - 1),
        ));
    } else if keys.just_pressed(KeyCode::Delete) && !slot.entries.is_empty() {
        picker.pending = Some(PickerAction::Delete(picker.selected));
    } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space])
        && !slot.entries.is_empty()
    {
        picker.pending = Some(PickerAction::Load(picker.selected));
    }
}

fn process(
    mut picker: ResMut<SavePicker>,
    mut slot: ResMut<SaveSlot>,
    mut run: ResMut<RunSave>,
    items: Option<Res<Items>>,
    placements: Option<Res<Placements>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if picker.pending.is_none() {
        return;
    }
    let Some(action) = picker.pending.take() else {
        return;
    };
    match action {
        PickerAction::Select(index) => picker.selected = index,
        PickerAction::Load(index) if !picker.confirming => {
            if let (Some(items), Some(placements)) = (items, placements) {
                if slot.load_into(index, &mut run, &items, &placements) {
                    picker.open = false;
                    next.set(GameState::Loading);
                } else if let Some(error) = slot.error() {
                    picker.error = Some(error.to_owned());
                }
            }
        }
        PickerAction::Delete(index) if !picker.confirming => {
            picker.selected = index;
            picker.confirming = true;
        }
        PickerAction::Confirm if picker.confirming => {
            if let Err(error) = slot.delete(picker.selected) {
                picker.error = Some(format!("Could not delete save: {error}"));
            }
            picker.confirming = false;
            picker.selected = picker.selected.min(slot.entries.len().saturating_sub(1));
        }
        PickerAction::Cancel => picker.confirming = false,
        PickerAction::Close => picker.open = false,
        _ => {}
    }
}

fn render(
    mut commands: Commands,
    picker: Res<SavePicker>,
    slot: Res<SaveSlot>,
    screens: Query<Entity, With<PickerScreen>>,
) {
    if !picker.is_changed() && !slot.is_changed() {
        return;
    }
    for entity in &screens {
        commands.entity(entity).despawn();
    }
    if !picker.open {
        return;
    }
    commands
        .spawn((
            PickerScreen,
            DespawnOnExit(GameState::Title),
            modal_overlay(),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: px(680),
                        max_width: percent(90),
                        max_height: percent(85),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(24)),
                        row_gap: px(10),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.04, 0.08, 0.13)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Load Game"),
                        TextFont {
                            font_size: 28.0.into(),
                            ..default()
                        },
                    ));
                    panel
                        .spawn((
                            PickerPane,
                            ScrollPosition(Vec2::new(
                                0.0,
                                picker.selected as f32 * SAVE_ROW_HEIGHT_PX,
                            )),
                            RelativeCursorPosition::default(),
                            Node {
                                width: percent(100),
                                min_height: px(0),
                                max_height: vh(56),
                                flex_direction: FlexDirection::Column,
                                overflow: Overflow::scroll_y(),
                                ..default()
                            },
                        ))
                        .with_children(|list| {
                            for (index, entry) in slot.entries.iter().enumerate() {
                                list.spawn((Node {
                                    height: px(SAVE_ROW_HEIGHT_PX),
                                    flex_direction: FlexDirection::Row,
                                    column_gap: px(8),
                                    flex_shrink: 0.0,
                                    ..default()
                                },))
                                    .with_children(|row| {
                                        let name =
                                            entry.name.as_deref().unwrap_or("Unreadable save");
                                        let label = format!(
                                            "{}{}  —  {}",
                                            if index == picker.selected {
                                                "› "
                                            } else {
                                                "  "
                                            },
                                            name,
                                            format_time(entry.modified)
                                        );
                                        row_button(
                                            row,
                                            &label,
                                            PickerAction::Load(index),
                                            entry.name.is_some(),
                                            true,
                                        );
                                        row_button(
                                            row,
                                            "Delete",
                                            PickerAction::Delete(index),
                                            true,
                                            false,
                                        );
                                    });
                            }
                        });
                    if let Some(error) = &picker.error {
                        panel.spawn((
                            Text::new(error.clone()),
                            TextColor(Color::srgb(1.0, 0.6, 0.5)),
                        ));
                    }
                    if picker.confirming {
                        panel.spawn(Text::new("Delete this save permanently?"));
                        panel
                            .spawn(Node {
                                column_gap: px(10),
                                ..default()
                            })
                            .with_children(|row| {
                                row_button(row, "Delete", PickerAction::Confirm, true, false);
                                row_button(row, "Cancel", PickerAction::Cancel, true, false);
                            });
                    } else {
                        panel.spawn(Text::new(
                            "↑/↓ select · Enter load · Delete removes · Esc closes",
                        ));
                        row_button(panel, "Back", PickerAction::Close, true, false);
                    }
                });
        });
}

fn row_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: PickerAction,
    enabled: bool,
    grow: bool,
) {
    parent
        .spawn((
            BackgroundColor(Color::srgb(0.10, 0.18, 0.27)),
            action,
            Node {
                flex_grow: if grow { 1.0 } else { 0.0 },
                min_height: px(36),
                padding: UiRect::all(px(8)),
                ..default()
            },
            if enabled {
                Pickable::default()
            } else {
                Pickable::IGNORE
            },
        ))
        .observe(click)
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextColor(if enabled {
                    Color::WHITE
                } else {
                    Color::srgb(0.5, 0.5, 0.5)
                }),
                Pickable::IGNORE,
            ));
        });
}

fn click(event: On<PointerClick>, actions: Query<&PickerAction>, mut picker: ResMut<SavePicker>) {
    if event.button == PointerButton::Primary
        && let Ok(action) = actions.get(event.entity)
    {
        picker.pending = Some(*action);
    }
}

fn scroll(
    wheel: Option<Res<AccumulatedMouseScroll>>,
    mut panes: Query<(&RelativeCursorPosition, &mut ScrollPosition), With<PickerPane>>,
) {
    if let Some(wheel) = wheel {
        for (cursor, mut position) in &mut panes {
            scroll_hovered(wheel.delta.y, cursor, &mut position);
        }
    }
}

// Convert filesystem timestamps to a stable UTC date without depending on the local timezone.
fn format_time(time: Option<SystemTime>) -> String {
    let Some(seconds) = time
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
    else {
        return "Unknown time".into();
    };
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days / 146_097;
    let day = days - era * 146_097;
    let year_of_era = (day - day / 1_460 + day / 36_524 - day / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let date = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    let day_seconds = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{date:02} {:02}:{:02} UTC",
        day_seconds / 3_600,
        day_seconds % 3_600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn opening_picker_resets_previous_selection_and_errors() {
        let mut picker = SavePicker {
            open: false,
            selected: 3,
            confirming: true,
            pending: Some(PickerAction::Close),
            error: Some("old error".into()),
        };

        picker.open();

        assert!(picker.is_open());
        assert_eq!(picker.selected, 0);
        assert!(!picker.confirming);
        assert!(picker.pending.is_none());
        assert!(picker.error.is_none());
    }

    #[test]
    fn escape_closes_picker_without_resuming_paused_game() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(GameState::Paused)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<SaveSlot>()
            .init_resource::<RunSave>()
            .add_plugins(SavePickerPlugin);
        app.update();

        app.world_mut().resource_mut::<SavePicker>().open();
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();

        assert!(!app.world().resource::<SavePicker>().is_open());
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Paused
        );
    }

    #[test]
    fn timestamps_display_utc_dates_including_leap_days() {
        assert_eq!(format_time(Some(UNIX_EPOCH)), "1970-01-01 00:00 UTC");
        assert_eq!(
            format_time(Some(UNIX_EPOCH + Duration::from_secs(1_709_214_120))),
            "2024-02-29 13:42 UTC"
        );
        assert_eq!(format_time(None), "Unknown time");
    }
}
