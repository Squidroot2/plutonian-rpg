use crate::{
    GameState,
    loading::LoadStatus,
    player::Player,
    quest::{Dialogue, Nearby, Quest, Target},
};
use bevy::{app::AppExit, prelude::*, window::PrimaryWindow};

pub struct GameUiPlugin;
#[derive(Component)]
struct Hud;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct DialogueText;
#[derive(Component)]
struct PausePanel;
#[derive(Component, Clone, Copy)]
enum Action {
    Resume,
    Quit,
}
impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (pause, display).chain());
    }
}
fn text_style(size: f32) -> (TextFont, TextColor) {
    (
        TextFont {
            font_size: size.into(),
            ..default()
        },
        TextColor(Color::srgb(0.78, 0.9, 0.94)),
    )
}
fn setup(mut commands: Commands) {
    commands.spawn((
        Hud,
        Text::default(),
        text_style(18.0),
        Node {
            position_type: PositionType::Absolute,
            left: px(30),
            top: px(24),
            ..default()
        },
    ));
    commands.spawn((
        StatusText,
        Text::new("Loading the ice shelf..."),
        text_style(22.0),
        Node {
            position_type: PositionType::Absolute,
            left: px(30),
            bottom: px(65),
            max_width: percent(90),
            ..default()
        },
    ));
    commands.spawn((
        DialogueText,
        Text::default(),
        text_style(20.0),
        BackgroundColor(Color::srgba(0.015, 0.028, 0.045, 0.96)),
        Node {
            position_type: PositionType::Absolute,
            left: percent(15),
            right: percent(15),
            bottom: px(110),
            padding: UiRect::all(px(28)),
            ..default()
        },
        Visibility::Hidden,
    ));
    commands
        .spawn((
            PausePanel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                row_gap: px(20),
                ..default()
            },
            BackgroundColor(Color::srgba(0.005, 0.012, 0.025, 0.94)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((Text::new("THE ICE CAN WAIT\n\nPaused"), text_style(30.0)));
            for (label, action) in [
                ("Resume", Action::Resume),
                ("Quit to desktop", Action::Quit),
            ] {
                parent
                    .spawn((
                        action,
                        Node {
                            width: px(230),
                            height: px(52),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.05, 0.13, 0.17)),
                    ))
                    .observe(buttons)
                    .with_children(|p| {
                        p.spawn((Text::new(label), text_style(20.0), Pickable::IGNORE));
                    });
            }
        });
}
fn pause(
    smoke: Option<Res<crate::smoke::Smoke>>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut dialogue: ResMut<Dialogue>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if smoke.is_none() && *state.get() == GameState::Playing && windows.iter().any(|w| !w.focused) {
        next.set(GameState::Paused);
    }
    if keys.just_pressed(KeyCode::Escape) {
        match state.get() {
            GameState::Playing if dialogue.text.is_some() => dialogue.text = None,
            GameState::Playing => next.set(GameState::Paused),
            GameState::Paused => next.set(GameState::Playing),
            _ => (),
        }
    }
}
fn buttons(
    click: On<PointerClick>,
    query: Query<&Action>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if *state.get() != GameState::Paused {
        return;
    }
    if let Ok(action) = query.get(click.entity) {
        match action {
            Action::Resume => next.set(GameState::Playing),
            Action::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}
fn display(
    state: Res<State<GameState>>,
    status: Res<LoadStatus>,
    quest: Res<Quest>,
    dialogue: Res<Dialogue>,
    nearby: Res<Nearby>,
    player: Query<&Transform, With<Player>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<Hud>>,
        Query<&mut Text, With<StatusText>>,
        Query<(&mut Text, &mut Visibility), With<DialogueText>>,
    )>,
    mut pause: Query<&mut Visibility, (With<PausePanel>, Without<DialogueText>)>,
) {
    let distance = player
        .iter()
        .next()
        .and_then(|p| quest.destination().map(|d| p.translation.distance(d)));
    for mut text in &mut texts.p0() {
        **text = if *state.get() == GameState::Loading {
            String::new()
        } else {
            format!(
                "OUTPOST / NIVALIS\n\n{}{}",
                quest.objective(),
                distance.map_or(String::new(), |d| format!("\n{d:.0} m"))
            )
        };
    }
    for mut text in &mut texts.p1() {
        **text = match state.get() {
            GameState::Loading => "Loading the ice shelf...".into(),
            GameState::Failed => format!(
                "{}\nCheck assets/models and restart the game.",
                status.error.as_deref().unwrap_or("Asset loading failed")
            ),
            GameState::Playing if dialogue.text.is_none() => {
                nearby.0.map_or(String::new(), |(_, target)| match target {
                    Target::Keeper => "[E] Speak to Nemi".into(),
                    Target::Core => "[E] Recover power core".into(),
                })
            }
            _ => String::new(),
        };
    }
    for (mut text, mut visibility) in &mut texts.p2() {
        if let Some(content) = dialogue.text {
            **text = format!("{content}\n\n[E / ENTER] Continue");
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
    for mut visibility in &mut pause {
        *visibility = if *state.get() == GameState::Paused {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
