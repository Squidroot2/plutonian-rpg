pub(crate) mod picker;

use crate::{
    GameState,
    controls::{Control, Keybinds},
    save::{RunSave, SaveSlot},
    ui::settings::SettingsMenu,
};
use az::Az;
use bevy::{
    app::AppExit,
    asset::uuid_handle,
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    picking::hover::PickingInteraction,
    prelude::*,
};

pub struct TitlePlugin;

pub(super) const TITLE_FONT: Handle<Font> = uuid_handle!("1716292a-80cc-4215-b9ab-d067074e9e67");
// Michroma's wider glyphs must fit between the title screen's horizontal insets.
const TITLE_WORD_SIZE: FontSize = FontSize::VMin(9.5);

#[derive(Component)]
pub(crate) struct TitleBackdrop;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TitleInput;

#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
enum MenuAction {
    #[default]
    Start,
    Load,
    Settings,
    Quit,
}

impl MenuAction {
    const ALL: [Self; 4] = [Self::Start, Self::Load, Self::Settings, Self::Quit];

    fn enabled(self, slot: &SaveSlot) -> bool {
        self != Self::Load || slot.available()
    }
}

#[derive(Resource, Default)]
pub(super) struct NameEntry {
    open: bool,
    text: String,
    error: bool,
}

impl NameEntry {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Component)]
struct NamePrompt;

#[derive(Component)]
struct NameScreen;

#[derive(Component)]
struct NameError;

#[derive(Resource, Default)]
struct MenuSelection(MenuAction);

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<KeyboardInput>()
            .add_plugins(picker::SavePickerPlugin)
            .init_resource::<MenuSelection>()
            .init_resource::<NameEntry>()
            .init_resource::<crate::player::NewPlayerName>()
            .add_systems(OnEnter(GameState::Title), setup)
            .add_systems(
                PreUpdate,
                navigate
                    .after(InputSystems)
                    .in_set(TitleInput)
                    .run_if(in_state(GameState::Title)),
            )
            .add_systems(
                Update,
                (style_buttons, refresh_name_prompt).run_if(in_state(GameState::Title)),
            );
    }
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut slot: ResMut<SaveSlot>,
    mut picker: ResMut<picker::SavePicker>,
    mut selection: ResMut<MenuSelection>,
    mut entry: ResMut<NameEntry>,
) {
    *entry = NameEntry::default();
    *picker = picker::SavePicker::default();
    slot.refresh();
    *selection = MenuSelection::default();
    commands
        .spawn((
            DespawnOnExit(GameState::Title),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(vw(8)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb(0.03, 0.06, 0.11)),
        ))
        .with_children(|screen| {
            spawn_backdrop(screen, &assets);
            screen
                .spawn(Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    ..default()
                })
                .with_children(|content| {
                    content.spawn((
                        Text::new("THE"),
                        TextFont::from_font_size(FontSize::VMin(4.6)).with_font(TITLE_FONT),
                        TextColor(Color::srgb(0.66, 0.83, 0.95)),
                    ));
                    content.spawn((
                        Text::new("PLUTONIAN"),
                        TextFont::from_font_size(TITLE_WORD_SIZE).with_font(TITLE_FONT),
                        TextColor(Color::srgb(0.89, 0.95, 1.0)),
                    ));
                    content
                        .spawn(Node {
                            width: px(274),
                            max_width: percent(100),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(12),
                            margin: UiRect::top(vh(5)),
                            ..default()
                        })
                        .with_children(|buttons| {
                            for (action, label, color) in [
                                (
                                    MenuAction::Start,
                                    "Start Game",
                                    Color::srgb(0.10, 0.06, 0.03),
                                ),
                                (MenuAction::Load, "Load Game", Color::srgb(0.81, 0.90, 0.96)),
                                (
                                    MenuAction::Settings,
                                    "Settings",
                                    Color::srgb(0.81, 0.90, 0.96),
                                ),
                                (MenuAction::Quit, "Quit", Color::srgb(0.81, 0.90, 0.96)),
                            ] {
                                let enabled = action.enabled(&slot);
                                buttons
                                    .spawn((
                                        action,
                                        if enabled {
                                            Pickable::default()
                                        } else {
                                            Pickable::IGNORE
                                        },
                                        Node {
                                            width: percent(100),
                                            min_height: px(50),
                                            align_items: AlignItems::Center,
                                            padding: UiRect::horizontal(px(20)),
                                            border: UiRect::all(px(1)),
                                            border_radius: BorderRadius::all(px(3)),
                                            ..default()
                                        },
                                        BackgroundColor::default(),
                                        BorderColor::default(),
                                    ))
                                    .observe(click)
                                    .with_children(|button| {
                                        button.spawn((
                                            Text::new(label),
                                            TextFont {
                                                font_size: 19.0.into(),
                                                ..default()
                                            },
                                            TextColor(if enabled {
                                                color
                                            } else {
                                                Color::srgb(0.45, 0.52, 0.57)
                                            }),
                                            Pickable::IGNORE,
                                        ));
                                    });
                            }
                            if let Some(error) = slot.error() {
                                buttons.spawn((
                                    Text::new(error),
                                    TextFont {
                                        font_size: 15.0.into(),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.97, 0.67, 0.50)),
                                    Pickable::IGNORE,
                                ));
                            }
                        });
                });
        });
    spawn_name_screen(&mut commands);
}

fn spawn_name_screen(commands: &mut Commands) {
    commands
        .spawn((
            NameScreen,
            DespawnOnExit(GameState::Title),
            GlobalZIndex(1),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(24),
                ..default()
            },
            BackgroundColor(Color::BLACK),
        ))
        .with_children(|screen| {
            screen.spawn((
                Text::new("What is your name?"),
                TextFont {
                    font_size: 32.0.into(),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            screen.spawn((
                NamePrompt,
                Text::new("_"),
                TextFont {
                    font_size: 26.0.into(),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            screen.spawn((
                NameError,
                Text::default(),
                TextFont {
                    font_size: 16.0.into(),
                    ..default()
                },
                TextColor(Color::srgb(0.97, 0.67, 0.50)),
            ));
        });
}

fn spawn_backdrop(screen: &mut ChildSpawnerCommands, assets: &AssetServer) {
    screen.spawn((
        TitleBackdrop,
        ImageNode::new(assets.load("ui/title.png")).with_mode(NodeImageMode::Stretch),
        Node {
            position_type: PositionType::Absolute,
            right: px(0),
            top: px(0),
            min_width: percent(100),
            min_height: percent(100),
            width: vh(1600.0 / 9.0),
            height: vw(56.25),
            ..default()
        },
        Pickable::IGNORE,
    ));
    screen.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundGradient::from(LinearGradient::new(
            LinearGradient::TO_RIGHT,
            vec![
                ColorStop::percent(Color::srgba(0.02, 0.04, 0.08, 0.98), 0),
                ColorStop::percent(Color::srgba(0.02, 0.04, 0.08, 0.87), 35),
                ColorStop::percent(Color::srgba(0.02, 0.04, 0.08, 0.15), 70),
                ColorStop::percent(Color::srgba(0.02, 0.04, 0.08, 0.0), 100),
            ],
        )),
        Pickable::IGNORE,
    ));
}

fn navigate(
    bindings: Res<Keybinds>,
    keys: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<MenuSelection>,
    mut settings: ResMut<SettingsMenu>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    mut slot: ResMut<SaveSlot>,
    mut run: ResMut<RunSave>,
    mut entry: ResMut<NameEntry>,
    mut name: ResMut<crate::player::NewPlayerName>,
    mut input: MessageReader<KeyboardInput>,
    mut picker: ResMut<picker::SavePicker>,
) {
    if picker.open {
        input.clear();
        return;
    }
    if entry.open {
        for event in input.read() {
            if event.state != ButtonState::Pressed {
                continue;
            }
            match &event.logical_key {
                Key::Escape => {
                    *entry = NameEntry::default();
                    break;
                }
                Key::Backspace => {
                    entry.text.pop();
                    entry.error = false;
                }
                Key::Enter if !event.repeat => {
                    let trimmed = entry.text.trim();
                    if crate::player::PlayerName::valid(trimmed) {
                        name.0 = trimmed.to_owned();
                        run.start_fresh();
                        slot.start_fresh();
                        entry.open = false;
                        next.set(GameState::Loading);
                    } else {
                        entry.error = true;
                    }
                    break;
                }
                Key::Character(_) | Key::Space => {
                    if let Some(text) = &event.text {
                        let valid = text.chars().all(|c| c.is_alphanumeric() || c == ' ');
                        if valid && entry.text.chars().count() + text.chars().count() <= 16 {
                            entry.text.push_str(text);
                            entry.error = false;
                        } else {
                            entry.error = true;
                        }
                    }
                }
                _ => {}
            }
        }
        return;
    }
    input.clear();
    if settings.is_open() || !matches!(*next, NextState::Unchanged) {
        return;
    }
    let direction = if bindings.just_pressed(Control::MenuUp, &keys) {
        -1
    } else if bindings.just_pressed(Control::MenuDown, &keys) || keys.just_pressed(KeyCode::Tab) {
        1
    } else {
        0
    };
    if direction != 0 {
        selection.0 = next_action(selection.0, direction, &slot);
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        activate(
            selection.0,
            &slot,
            &mut settings,
            &mut exit,
            &mut entry,
            &mut picker,
        );
    }
}

fn next_action(current: MenuAction, direction: isize, slot: &SaveSlot) -> MenuAction {
    let actions = MenuAction::ALL;
    let index = actions
        .iter()
        .position(|&action| action == current)
        .expect("menu action is listed");
    let index = index.az::<isize>();
    let len = actions.len().az::<isize>();
    (1..=len)
        .map(|step| {
            let index = (index + direction * step).rem_euclid(len);
            actions[index.az::<usize>()]
        })
        .find(|&action| action.enabled(slot))
        .expect("there is at least one enabled menu action")
}

fn click(
    event: On<PointerClick>,
    actions: Query<&MenuAction>,
    state: Res<State<GameState>>,
    mut settings: ResMut<SettingsMenu>,
    next: Res<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    slot: Res<SaveSlot>,
    mut entry: ResMut<NameEntry>,
    mut picker: ResMut<picker::SavePicker>,
) {
    if !picker.open
        && !entry.open
        && *state.get() == GameState::Title
        && !settings.is_open()
        && matches!(*next, NextState::Unchanged)
        && event.button == PointerButton::Primary
        && let Ok(&action) = actions.get(event.entity)
    {
        activate(
            action,
            &slot,
            &mut settings,
            &mut exit,
            &mut entry,
            &mut picker,
        );
    }
}

fn activate(
    action: MenuAction,
    slot: &SaveSlot,
    settings: &mut SettingsMenu,
    exit: &mut MessageWriter<AppExit>,
    entry: &mut NameEntry,
    picker: &mut picker::SavePicker,
) {
    match action {
        MenuAction::Start => {
            *entry = NameEntry {
                open: true,
                ..default()
            };
        }
        MenuAction::Load if slot.available() => picker.open(),
        MenuAction::Load => {}
        MenuAction::Settings => settings.open(),
        MenuAction::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

fn style_buttons(
    selection: Res<MenuSelection>,
    slot: Res<SaveSlot>,
    mut buttons: Query<(
        &MenuAction,
        Option<&PickingInteraction>,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
) {
    for (action, hover, mut background, mut border) in &mut buttons {
        if !action.enabled(&slot) {
            background.set_if_neq(BackgroundColor(Color::srgba(0.05, 0.08, 0.12, 0.9)));
            border.set_if_neq(BorderColor::all(Color::NONE));
            continue;
        }
        let pointer_over = matches!(
            hover,
            Some(PickingInteraction::Hovered | PickingInteraction::Pressed)
        );
        let fill = match (action, pointer_over) {
            (MenuAction::Start, true) => Color::srgb(1.0, 0.78, 0.50),
            (MenuAction::Start, false) => Color::srgb(0.94, 0.65, 0.37),
            (_, true) => Color::srgba(0.18, 0.30, 0.43, 0.95),
            (_, false) => Color::srgba(0.07, 0.12, 0.19, 0.9),
        };
        let outline = if *action == selection.0 || pointer_over {
            Color::srgb(0.86, 0.94, 1.0)
        } else {
            Color::srgba(0.50, 0.65, 0.78, 0.48)
        };
        background.set_if_neq(BackgroundColor(fill));
        border.set_if_neq(BorderColor::all(outline));
    }
}

fn refresh_name_prompt(
    entry: Res<NameEntry>,
    mut screens: Query<&mut Node, With<NameScreen>>,
    mut prompts: Query<&mut Text, (With<NamePrompt>, Without<NameError>)>,
    mut errors: Query<&mut Text, (With<NameError>, Without<NamePrompt>)>,
) {
    if !entry.is_changed() {
        return;
    }
    for mut screen in &mut screens {
        screen.display = if entry.open {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut text in &mut prompts {
        **text = format!("{}_", entry.text);
    }
    for mut text in &mut errors {
        **text = if entry.error {
            "Use 1–16 letters, numbers, or spaces.".into()
        } else {
            String::new()
        };
    }
}
