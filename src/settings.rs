use crate::{title::TitleInput, ui};
use bevy::{
    input::InputSystems,
    picking::hover::PickingInteraction,
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, WindowMode},
};

pub(crate) const FULLSCREEN_MODE: WindowMode =
    WindowMode::BorderlessFullscreen(MonitorSelection::Primary);

pub(crate) struct SettingsPlugin;

#[derive(Resource, Default)]
pub(crate) struct SettingsMenu {
    open: bool,
}

impl SettingsMenu {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open(&mut self) {
        self.open = true;
    }
}

#[derive(Component)]
struct SettingsPanel;

#[derive(Component)]
struct DisplayLabel;

#[derive(Component, Clone, Copy)]
enum Action {
    ToggleDisplay,
    Back,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SettingsMenu>()
            .add_systems(Startup, setup)
            .add_systems(
                PreUpdate,
                navigate
                    .after(InputSystems)
                    .after(TitleInput)
                    .after(ui::pause),
            )
            .add_systems(Update, refresh);
    }
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            SettingsPanel,
            Visibility::Hidden,
            GlobalZIndex(60),
            BackgroundColor(Color::srgb(0.018, 0.035, 0.055)),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(16),
                ..default()
            },
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Settings"),
                ui::text_style(30.0),
                Pickable::IGNORE,
            ));
            for action in [Action::ToggleDisplay, Action::Back] {
                panel
                    .spawn((
                        action,
                        BackgroundColor(Color::srgb(0.05, 0.13, 0.17)),
                        Node {
                            width: px(300),
                            max_width: percent(90),
                            min_height: px(52),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            border_radius: BorderRadius::all(px(3)),
                            ..default()
                        },
                    ))
                    .observe(click)
                    .with_children(|button| {
                        let text = (ui::text_style(18.0), Pickable::IGNORE);
                        match action {
                            Action::ToggleDisplay => {
                                button.spawn((DisplayLabel, Text::default(), text));
                            }
                            Action::Back => {
                                button.spawn((Text::new("Back (Esc)"), text));
                            }
                        }
                    });
            }
        });
}

fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<SettingsMenu>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !settings.is_open() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        settings.open = false;
    } else if !settings.is_changed()
        && keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space])
    {
        activate(Action::ToggleDisplay, &mut settings, &mut windows);
    }
}

fn click(
    event: On<PointerClick>,
    actions: Query<&Action>,
    mut settings: ResMut<SettingsMenu>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if settings.is_open()
        && event.button == PointerButton::Primary
        && let Ok(&action) = actions.get(event.entity)
    {
        activate(action, &mut settings, &mut windows);
    }
}

fn activate(
    action: Action,
    settings: &mut SettingsMenu,
    windows: &mut Query<&mut Window, With<PrimaryWindow>>,
) {
    match action {
        Action::Back => settings.open = false,
        Action::ToggleDisplay => {
            if let Ok(mut window) = windows.single_mut() {
                window.mode = if window.mode == WindowMode::Windowed {
                    FULLSCREEN_MODE
                } else {
                    WindowMode::Windowed
                };
            }
        }
    }
}

fn refresh(
    settings: Res<SettingsMenu>,
    mut panel: Query<&mut Visibility, With<SettingsPanel>>,
    windows: Query<Ref<Window>, With<PrimaryWindow>>,
    mut label: Query<&mut Text, With<DisplayLabel>>,
    mut buttons: Query<
        (&PickingInteraction, &mut BackgroundColor),
        (With<Action>, Changed<PickingInteraction>),
    >,
) {
    if settings.is_changed()
        && let Ok(mut visibility) = panel.single_mut()
    {
        *visibility = if settings.is_open() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if settings.is_open()
        && let Ok(window) = windows.single()
        && (window.is_changed() || settings.is_changed())
        && let Ok(mut text) = label.single_mut()
    {
        **text = if window.mode == WindowMode::Windowed {
            "Display mode: Windowed"
        } else {
            "Display mode: Fullscreen"
        }
        .into();
    }
    for (interaction, mut color) in &mut buttons {
        *color = BackgroundColor(match interaction {
            PickingInteraction::None => Color::srgb(0.05, 0.13, 0.17),
            PickingInteraction::Hovered => Color::srgb(0.09, 0.22, 0.28),
            PickingInteraction::Pressed => Color::srgb(0.13, 0.30, 0.38),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameState, title::TitlePlugin};
    use bevy::{asset::AssetPlugin, state::app::StatesPlugin};

    #[test]
    fn title_settings_open_without_toggling_and_return_to_title() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .init_asset::<Image>()
            .init_state::<GameState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins((TitlePlugin, SettingsPlugin));
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let mode = |app: &App| {
            app.world()
                .get::<Window>(window)
                .expect("the primary window must exist")
                .mode
        };
        let open = |app: &App| app.world().resource::<SettingsMenu>().is_open();
        let press = |app: &mut App, keys: &[KeyCode]| {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            input.reset_all();
            for &key in keys {
                input.press(key);
            }
            app.update();
        };

        app.update();
        press(&mut app, &[KeyCode::ArrowDown, KeyCode::Enter]);
        assert!(open(&app));
        assert_eq!(mode(&app), WindowMode::Windowed);
        press(&mut app, &[KeyCode::Enter]);
        assert_eq!(mode(&app), FULLSCREEN_MODE);
        press(&mut app, &[KeyCode::Enter]);
        assert_eq!(mode(&app), WindowMode::Windowed);
        press(&mut app, &[KeyCode::Escape]);
        assert!(!open(&app));
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Title
        );
    }

    #[test]
    fn escape_from_paused_settings_returns_to_pause() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, SettingsPlugin))
            .insert_state(GameState::Paused)
            .init_resource::<crate::quest::Dialogue>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(PreUpdate, ui::pause);
        app.world_mut().resource_mut::<SettingsMenu>().open();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<SettingsMenu>().is_open());
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Paused
        );
    }
}
