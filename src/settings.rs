use crate::{
    controls::{Binding, Control, Keybinds},
    title::TitleInput,
    ui,
};
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::InputSystems,
    picking::hover::PickingInteraction,
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, WindowMode},
};

pub(crate) const FULLSCREEN_MODE: WindowMode =
    WindowMode::BorderlessFullscreen(MonitorSelection::Primary);

pub(crate) struct SettingsPlugin;

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum FpsPosition {
    #[default]
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    Disabled,
}

impl FpsPosition {
    fn next(self) -> Self {
        match self {
            Self::TopLeft => Self::TopRight,
            Self::TopRight => Self::BottomRight,
            Self::BottomRight => Self::BottomLeft,
            Self::BottomLeft => Self::Disabled,
            Self::Disabled => Self::TopLeft,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomRight => "Bottom right",
            Self::BottomLeft => "Bottom left",
            Self::Disabled => "Off",
        }
    }
}

#[derive(Component)]
struct FpsCounter;

#[derive(Resource, Default)]
pub(crate) struct SettingsMenu {
    open: bool,
    keybinds: bool,
    capture: Option<Control>,
    notice: String,
}

impl SettingsMenu {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open(&mut self) {
        *self = Self {
            open: true,
            ..default()
        };
    }
}

#[derive(Component)]
struct SettingsPanel;

#[derive(Component, PartialEq, Eq)]
enum Section {
    General,
    Keybinds,
    Capture,
}

#[derive(Component)]
struct ActionLabel(Action);

#[derive(Component)]
struct CapturePrompt;

#[derive(Component, Clone, Copy)]
enum Action {
    ToggleDisplay,
    CycleFps,
    Keybinds,
    Rebind(Control),
    ResetBindings,
    Back,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .init_resource::<FpsPosition>()
            .init_resource::<SettingsMenu>()
            .init_resource::<Keybinds>()
            .add_systems(Startup, setup)
            .add_systems(
                PreUpdate,
                navigate
                    .after(InputSystems)
                    .after(TitleInput)
                    .after(ui::pause),
            )
            .add_systems(Update, (refresh, refresh_fps));
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((
        FpsCounter,
        Text::new("FPS: --"),
        ui::text_style(15.0),
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(4),
            ..default()
        },
        GlobalZIndex(100),
        Pickable::IGNORE,
    ));
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
            panel
                .spawn((Section::General, section_layout()))
                .with_children(|menu| {
                    for action in [
                        Action::ToggleDisplay,
                        Action::CycleFps,
                        Action::Keybinds,
                        Action::Back,
                    ] {
                        spawn_button(menu, action, percent(100));
                    }
                });
            panel
                .spawn((Section::Keybinds, section_layout()))
                .with_children(|menu| {
                    menu.spawn(Node {
                        width: percent(100),
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::SpaceBetween,
                        row_gap: px(6),
                        ..default()
                    })
                    .with_children(|grid| {
                        for control in Control::ALL {
                            spawn_button(grid, Action::Rebind(control), percent(49));
                        }
                    });
                    spawn_button(menu, Action::ResetBindings, percent(100));
                    spawn_button(menu, Action::Back, percent(100));
                });
            panel
                .spawn((Section::Capture, section_layout()))
                .with_children(|menu| {
                    menu.spawn((CapturePrompt, Text::default(), ui::text_style(18.0)));
                    menu.spawn((Text::new("Esc to cancel"), ui::text_style(16.0)));
                });
        });
}

fn section_layout() -> Node {
    Node {
        display: Display::None,
        width: px(480),
        max_width: percent(94),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(8),
        ..default()
    }
}

fn spawn_button(parent: &mut ChildSpawnerCommands, action: Action, width: Val) {
    parent
        .spawn((
            action,
            BackgroundColor(Color::srgb(0.05, 0.13, 0.17)),
            Node {
                width,
                min_height: px(36),
                padding: UiRect::horizontal(px(4)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(px(3)),
                ..default()
            },
        ))
        .observe(click)
        .with_children(|button| {
            button.spawn((
                ActionLabel(action),
                Text::default(),
                ui::text_style(16.0),
                Pickable::IGNORE,
            ));
        });
}

fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut settings: ResMut<SettingsMenu>,
    mut bindings: ResMut<Keybinds>,
    mut fps: ResMut<FpsPosition>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !settings.is_open() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        activate(
            Action::Back,
            &mut settings,
            &mut bindings,
            &mut fps,
            &mut windows,
        );
    } else if let Some(control) = settings.capture {
        if settings.is_changed() {
            return;
        }
        let binding = keys
            .get_just_pressed()
            .next()
            .copied()
            .map(Binding::Key)
            .or_else(|| {
                if control == Control::Attack {
                    mouse.get_just_pressed().next().copied().map(Binding::Mouse)
                } else {
                    None
                }
            });
        if let Some(binding) = binding {
            match bindings.assign(control, binding) {
                Ok(()) => {
                    settings.capture = None;
                    settings.notice.clear();
                }
                Err(message) => settings.notice = message,
            }
        }
    } else if !settings.keybinds
        && !settings.is_changed()
        && keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space])
    {
        activate(
            Action::ToggleDisplay,
            &mut settings,
            &mut bindings,
            &mut fps,
            &mut windows,
        );
    }
}

fn click(
    event: On<PointerClick>,
    actions: Query<&Action>,
    mut settings: ResMut<SettingsMenu>,
    mut bindings: ResMut<Keybinds>,
    mut fps: ResMut<FpsPosition>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if settings.is_open()
        && event.button == PointerButton::Primary
        && let Ok(&action) = actions.get(event.entity)
    {
        activate(action, &mut settings, &mut bindings, &mut fps, &mut windows);
    }
}

fn activate(
    action: Action,
    settings: &mut SettingsMenu,
    bindings: &mut Keybinds,
    fps: &mut FpsPosition,
    windows: &mut Query<&mut Window, With<PrimaryWindow>>,
) {
    match action {
        Action::Back if settings.capture.take().is_some() => settings.notice.clear(),
        Action::Back if settings.keybinds => settings.keybinds = false,
        Action::Back => settings.open = false,
        Action::CycleFps => *fps = fps.next(),
        Action::Keybinds => settings.keybinds = true,
        Action::Rebind(control) => {
            settings.capture = Some(control);
            settings.notice.clear();
        }
        Action::ResetBindings => *bindings = Keybinds::default(),
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
    bindings: Res<Keybinds>,
    fps: Res<FpsPosition>,
    mut panel: Query<&mut Visibility, With<SettingsPanel>>,
    mut sections: Query<(&Section, &mut Node)>,
    windows: Query<Ref<Window>, With<PrimaryWindow>>,
    mut labels: Query<(&ActionLabel, &mut Text)>,
    mut prompt: Query<&mut Text, (With<CapturePrompt>, Without<ActionLabel>)>,
    mut buttons: Query<
        (&PickingInteraction, &mut BackgroundColor),
        (With<Action>, Changed<PickingInteraction>),
    >,
) {
    if settings.is_changed() {
        if let Ok(mut visibility) = panel.single_mut() {
            *visibility = if settings.is_open() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        let visible = if settings.capture.is_some() {
            Section::Capture
        } else if settings.keybinds {
            Section::Keybinds
        } else {
            Section::General
        };
        for (section, mut node) in &mut sections {
            node.display = if *section == visible {
                Display::Flex
            } else {
                Display::None
            };
        }
        if let Some(control) = settings.capture
            && let Ok(mut text) = prompt.single_mut()
        {
            let input = if control == Control::Attack {
                "a key or mouse button"
            } else {
                "a key"
            };
            **text = format!(
                "Press {input} for {}.\n{}",
                control.label(),
                settings.notice
            );
        }
    }
    let window = windows.single().ok();
    if settings.is_open()
        && (settings.is_changed()
            || bindings.is_changed()
            || fps.is_changed()
            || window.as_ref().is_some_and(|window| window.is_changed()))
    {
        for (label, mut text) in &mut labels {
            let title = match label.0 {
                Action::ToggleDisplay
                    if window
                        .as_ref()
                        .is_some_and(|window| window.mode == WindowMode::Windowed) =>
                {
                    "Display mode: Windowed".into()
                }
                Action::ToggleDisplay => "Display mode: Fullscreen".into(),
                Action::CycleFps => format!("FPS counter: {}", fps.label()),
                Action::Keybinds => "Keybinds".into(),
                Action::Rebind(control) => {
                    format!("{}  ·  {}", control.label(), bindings.get(control).label())
                }
                Action::ResetBindings => "Restore defaults".into(),
                Action::Back => "Back (Esc)".into(),
            };
            text.set_if_neq(Text::new(title));
        }
    }
    for (interaction, mut color) in &mut buttons {
        *color = BackgroundColor(match interaction {
            PickingInteraction::None => Color::srgb(0.05, 0.13, 0.17),
            PickingInteraction::Hovered => Color::srgb(0.09, 0.22, 0.28),
            PickingInteraction::Pressed => Color::srgb(0.13, 0.30, 0.38),
        });
    }
}

fn refresh_fps(
    diagnostics: Res<DiagnosticsStore>,
    position: Res<FpsPosition>,
    mut counter: Query<(&mut Text, &mut Node, &mut Visibility), With<FpsCounter>>,
    mut last_fps: Local<Option<u32>>,
) {
    let Ok((mut text, mut node, mut visibility)) = counter.single_mut() else {
        return;
    };
    if position.is_changed() {
        let left = matches!(*position, FpsPosition::TopLeft | FpsPosition::BottomLeft);
        let top = matches!(*position, FpsPosition::TopLeft | FpsPosition::TopRight);
        node.left = if left { px(12) } else { Val::Auto };
        node.right = if left { Val::Auto } else { px(12) };
        node.top = if top { px(4) } else { Val::Auto };
        node.bottom = if top { Val::Auto } else { px(4) };
        *visibility = if *position == FpsPosition::Disabled {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    if *position == FpsPosition::Disabled {
        return;
    }
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|diagnostic| diagnostic.smoothed())
        .filter(|fps| fps.is_finite())
        .map(|fps| fps.round() as u32);
    if *last_fps != fps {
        **text = fps.map_or_else(|| "FPS: --".into(), |fps| format!("FPS: {fps}"));
        *last_fps = fps;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameState, title::TitlePlugin};
    use bevy::{asset::AssetPlugin, state::app::StatesPlugin};

    #[test]
    fn fps_position_cycles_through_corners_and_disabled() {
        let mut position = FpsPosition::default();
        let labels = [
            "Top left",
            "Top right",
            "Bottom right",
            "Bottom left",
            "Off",
            "Top left",
        ];
        for label in labels {
            assert_eq!(position.label(), label);
            position = position.next();
        }
    }

    #[test]
    fn title_settings_open_without_toggling_and_return_to_title() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .init_asset::<Image>()
            .init_state::<GameState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
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
            .init_resource::<ButtonInput<MouseButton>>()
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

    #[test]
    fn capture_rejects_a_duplicate_and_applies_an_available_key() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SettingsPlugin))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>();
        let world = app.world_mut();
        let mut menu = world.resource_mut::<SettingsMenu>();
        menu.open();
        menu.keybinds = true;
        menu.capture = Some(Control::Jump);
        app.update();

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.update();
        let menu = app.world().resource::<SettingsMenu>();
        assert_eq!(menu.capture, Some(Control::Jump));
        assert!(menu.notice.contains("Move forward"));

        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(KeyCode::KeyR);
        app.update();
        assert!(matches!(
            app.world().resource::<Keybinds>().get(Control::Jump),
            Binding::Key(KeyCode::KeyR)
        ));
        assert_eq!(app.world().resource::<SettingsMenu>().capture, None);
    }

    #[test]
    fn menu_navigation_rebinds_independently_of_gameplay_controls() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SettingsPlugin))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>();
        let bindings = app.world().resource::<Keybinds>();
        assert_eq!(bindings.get(Control::MenuUp).label(), "W");
        assert_eq!(bindings.get(Control::MenuDown).label(), "S");

        let mut menu = app.world_mut().resource_mut::<SettingsMenu>();
        menu.open();
        menu.keybinds = true;
        menu.capture = Some(Control::MenuUp);
        app.update();

        let press = |app: &mut App, key| {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(key);
            app.update();
        };
        press(&mut app, KeyCode::KeyD);
        let bindings = app.world().resource::<Keybinds>();
        assert_eq!(bindings.get(Control::MenuUp).label(), "D");
        assert_eq!(bindings.get(Control::Right).label(), "D");

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut().resource_mut::<SettingsMenu>().capture = Some(Control::MenuDown);
        app.update();
        press(&mut app, KeyCode::KeyD);
        assert!(
            app.world()
                .resource::<SettingsMenu>()
                .notice
                .contains("Menu Up")
        );
        press(&mut app, KeyCode::KeyQ);
        assert!(
            app.world()
                .resource::<SettingsMenu>()
                .notice
                .contains("Drop item")
        );
        press(&mut app, KeyCode::ArrowDown);
        assert_eq!(
            app.world()
                .resource::<Keybinds>()
                .get(Control::MenuDown)
                .label(),
            "ArrowDown"
        );
        assert_eq!(app.world().resource::<SettingsMenu>().capture, None);
    }
}
