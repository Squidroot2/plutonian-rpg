use crate::GameState;
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
    Quit,
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

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
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
                                (MenuAction::Quit, "Quit", Color::srgb(0.81, 0.90, 0.96)),
                            ] {
                                buttons
                                    .spawn((
                                        action,
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
                                            TextColor(color),
                                            Pickable::IGNORE,
                                        ));
                                    });
                            }
                        });
                });
        });
}

fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<MenuSelection>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::ArrowUp) {
        selection.0 = MenuAction::Start;
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        selection.0 = MenuAction::Quit;
    } else if keys.just_pressed(KeyCode::Tab) {
        selection.0 = match selection.0 {
            MenuAction::Start => MenuAction::Quit,
            MenuAction::Quit => MenuAction::Start,
        };
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
        activate(selection.0, &mut next, &mut exit);
    }
}

fn click(
    event: On<PointerClick>,
    actions: Query<&MenuAction>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if *state.get() == GameState::Title
        && event.button == PointerButton::Primary
        && let Ok(&action) = actions.get(event.entity)
    {
        activate(action, &mut next, &mut exit);
    }
}

fn activate(
    action: MenuAction,
    next: &mut NextState<GameState>,
    exit: &mut MessageWriter<AppExit>,
) {
    match action {
        MenuAction::Start => next.set(GameState::Loading),
        MenuAction::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

fn style_buttons(
    selection: Res<MenuSelection>,
    mut buttons: Query<(
        &MenuAction,
        Option<&PickingInteraction>,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
) {
    for (action, hover, mut background, mut border) in &mut buttons {
        let pointer_over = matches!(
            hover,
            Some(PickingInteraction::Hovered | PickingInteraction::Pressed)
        );
        let fill = match (action, pointer_over) {
            (MenuAction::Start, true) => Color::srgb(1.0, 0.78, 0.50),
            (MenuAction::Start, false) => Color::srgb(0.94, 0.65, 0.37),
            (MenuAction::Quit, true) => Color::srgba(0.18, 0.30, 0.43, 0.95),
            (MenuAction::Quit, false) => Color::srgba(0.07, 0.12, 0.19, 0.9),
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
