use crate::{
    GameState,
    save::{RunSave, SaveSlot},
    settings::SettingsMenu,
};
use bevy::{app::AppExit, input::InputSystems, picking::hover::PickingInteraction, prelude::*};

pub struct TitlePlugin;

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
struct MenuSelection(MenuAction);

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuSelection>()
            .add_systems(OnEnter(GameState::Title), setup)
            .add_systems(
                PreUpdate,
                navigate
                    .after(InputSystems)
                    .in_set(TitleInput)
                    .run_if(in_state(GameState::Title)),
            )
            .add_systems(Update, style_buttons.run_if(in_state(GameState::Title)));
    }
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    slot: Res<SaveSlot>,
    mut selection: ResMut<MenuSelection>,
) {
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
                        TextFont {
                            font_size: FontSize::VMin(4.6),
                            ..default()
                        },
                        TextColor(Color::srgb(0.66, 0.83, 0.95)),
                    ));
                    content.spawn((
                        Text::new("PLUTONIAN"),
                        TextFont {
                            font_size: FontSize::VMin(11.5),
                            ..default()
                        },
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
    keys: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<MenuSelection>,
    mut settings: ResMut<SettingsMenu>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    slot: Res<SaveSlot>,
    mut run: ResMut<RunSave>,
) {
    if settings.is_open() || !matches!(*next, NextState::Unchanged) {
        return;
    }
    let direction = if keys.just_pressed(KeyCode::ArrowUp) {
        -1
    } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::Tab]) {
        1
    } else {
        0
    };
    if direction != 0 {
        selection.0 = next_action(selection.0, direction, &slot);
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
        activate(
            selection.0,
            &slot,
            &mut run,
            &mut settings,
            &mut next,
            &mut exit,
        );
    }
}

fn next_action(current: MenuAction, direction: isize, slot: &SaveSlot) -> MenuAction {
    let actions = MenuAction::ALL;
    let index = actions
        .iter()
        .position(|&action| action == current)
        .expect("menu action is listed") as isize;
    (1..=actions.len() as isize)
        .map(|step| actions[(index + direction * step).rem_euclid(actions.len() as isize) as usize])
        .find(|&action| action.enabled(slot))
        .expect("there is at least one enabled menu action")
}

fn click(
    event: On<PointerClick>,
    actions: Query<&MenuAction>,
    state: Res<State<GameState>>,
    mut settings: ResMut<SettingsMenu>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    slot: Res<SaveSlot>,
    mut run: ResMut<RunSave>,
) {
    if *state.get() == GameState::Title
        && !settings.is_open()
        && matches!(*next, NextState::Unchanged)
        && event.button == PointerButton::Primary
        && let Ok(&action) = actions.get(event.entity)
    {
        activate(action, &slot, &mut run, &mut settings, &mut next, &mut exit);
    }
}

fn activate(
    action: MenuAction,
    slot: &SaveSlot,
    run: &mut RunSave,
    settings: &mut SettingsMenu,
    next: &mut NextState<GameState>,
    exit: &mut MessageWriter<AppExit>,
) {
    match action {
        MenuAction::Start => {
            run.start_fresh();
            next.set(GameState::Loading);
        }
        MenuAction::Load => {
            if slot.load_into(run) {
                next.set(GameState::Loading);
            }
        }
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
