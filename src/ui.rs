use crate::{
    GameState,
    combat::Health,
    dungeon::{Area, DungeonProgress, DungeonTarget},
    interaction::{InteractionSystems, InteractionTarget},
    loading::LoadStatus,
    player::{Player, respawn_defeated},
    quest::{Dialogue, Nearby, Quest, Target},
    world::WorldLayout,
};
use bevy::{app::AppExit, prelude::*, window::PrimaryWindow};

pub struct GameUiPlugin;
#[derive(Component)]
enum HudText {
    Location,
    Objective,
}
#[derive(Component)]
struct HealthBar;
#[derive(Component)]
struct HealthLabel;
#[derive(Component)]
pub(crate) struct HealthBarFill;
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
            .add_systems(PreUpdate, pause.after(bevy::input::InputSystems))
            .add_systems(
                Update,
                (
                    (refresh_hud, refresh_health).after(respawn_defeated),
                    refresh_prompt,
                    refresh_dialogue,
                    refresh_pause,
                )
                    .after(InteractionSystems),
            );
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
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(30),
                right: px(30),
                top: px(24),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                row_gap: px(8),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|hud| {
            hud.spawn((
                HudText::Location,
                Text::default(),
                text_style(18.0),
                Pickable::IGNORE,
            ));
            hud.spawn((
                HealthBar,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    ..default()
                },
                Visibility::Hidden,
                Pickable::IGNORE,
            ))
            .with_children(|bar| {
                bar.spawn((
                    HealthLabel,
                    Text::default(),
                    text_style(16.0),
                    Pickable::IGNORE,
                ));
                bar.spawn((
                    Node {
                        width: px(184),
                        height: px(10),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(2)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.045, 0.065, 0.09)),
                    BorderColor::all(Color::srgb(0.30, 0.38, 0.44)),
                    Pickable::IGNORE,
                ))
                .with_child((
                    HealthBarFill,
                    Node {
                        width: percent(0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.94, 0.32, 0.35)),
                    Pickable::IGNORE,
                ));
            });
            hud.spawn((
                HudText::Objective,
                Text::default(),
                text_style(18.0),
                Pickable::IGNORE,
            ));
        });
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
pub(crate) fn pause(
    smoke: Option<Res<crate::smoke::SmokeRun>>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut dialogue: ResMut<Dialogue>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if smoke.is_none() && *state.get() == GameState::Playing && windows.iter().any(|w| !w.focused) {
        next.set(GameState::Paused);
    }
    if keys.just_pressed(KeyCode::KeyI)
        && *state.get() == GameState::Playing
        && dialogue.text.is_none()
    {
        next.set(GameState::Inventory);
    }
    if *state.get() == GameState::Inventory
        && keys.any_just_pressed([KeyCode::KeyI, KeyCode::Escape])
    {
        next.set(GameState::Playing);
        return;
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
#[derive(PartialEq, Eq)]
struct HudSnapshot {
    location: &'static str,
    objective: &'static str,
    distance: Option<i32>,
}

fn refresh_hud(
    state: Res<State<GameState>>,
    quest: Res<Quest>,
    layout: Res<WorldLayout>,
    dungeon: Res<DungeonProgress>,
    targets: Query<(&DungeonTarget, &Transform)>,
    player: Query<(&Player, &Transform)>,
    mut hud: Query<(&HudText, &mut Text)>,
    mut last: Local<Option<HudSnapshot>>,
) {
    if matches!(state.get(), GameState::Title | GameState::Loading) {
        if last.take().is_some() {
            for (_, mut text) in &mut hud {
                text.clear();
            }
        }
        return;
    }
    let avatar = player.single().ok();
    let area = avatar.map_or(Area::Surface, |(player, _)| player.area);
    let show_dungeon = *layout == WorldLayout::Outpost && DungeonProgress::hatch_unlocked(*quest);
    let objective = if show_dungeon {
        dungeon.objective(area)
    } else {
        quest.objective()
    };
    let destination = if show_dungeon && area == Area::Surface && !dungeon.explored {
        targets
            .iter()
            .find(|(target, _)| **target == DungeonTarget::Hatch)
            .map(|(_, transform)| transform.translation)
    } else {
        quest.destination(*layout)
    };
    let snapshot = HudSnapshot {
        location: match (*layout, area) {
            (_, Area::OldRelay) => "THE OLD RELAY",
            (WorldLayout::Outpost, Area::Surface) => "OUTPOST / NIVALIS",
            (WorldLayout::SmokeArena, Area::Surface) => "SMOKE ARENA",
        },
        objective,
        distance: avatar.and_then(|(_, transform)| {
            destination.map(|point| transform.translation.distance(point).round() as i32)
        }),
    };
    if last.as_ref() == Some(&snapshot) {
        return;
    }
    for (kind, mut text) in &mut hud {
        let value = match kind {
            HudText::Location => snapshot.location.to_owned(),
            HudText::Objective => match snapshot.distance {
                Some(distance) => format!("{}\n{distance} m", snapshot.objective),
                None => snapshot.objective.to_owned(),
            },
        };
        text.set_if_neq(Text::new(value));
    }
    *last = Some(snapshot);
}

fn refresh_health(
    state: Res<State<GameState>>,
    player: Query<&Health, With<Player>>,
    mut bar: Query<&mut Visibility, With<HealthBar>>,
    mut label: Query<&mut Text, With<HealthLabel>>,
    mut fill: Query<&mut Node, With<HealthBarFill>>,
    mut last: Local<Option<(u16, u16)>>,
) {
    let health = match state.get() {
        GameState::Playing | GameState::Inventory | GameState::Paused => player
            .single()
            .ok()
            .map(|health| (health.current(), health.maximum())),
        _ => None,
    };
    if *last == health {
        return;
    }
    let (Ok(mut bar), Ok(mut label), Ok(mut fill)) =
        (bar.single_mut(), label.single_mut(), fill.single_mut())
    else {
        return;
    };
    *last = health;
    if let Some((current, maximum)) = health {
        *bar = Visibility::Inherited;
        **label = format!("Health {current}/{maximum}");
        fill.width = percent(100.0 * f32::from(current.min(maximum)) / f32::from(maximum.max(1)));
    } else {
        *bar = Visibility::Hidden;
    }
}

fn refresh_prompt(
    state: Res<State<GameState>>,
    status: Res<LoadStatus>,
    quest: Res<Quest>,
    dungeon: Res<DungeonProgress>,
    dialogue: Res<Dialogue>,
    nearby: Res<Nearby>,
    mut prompts: Query<&mut Text, With<StatusText>>,
) {
    if !(state.is_changed()
        || status.is_changed()
        || quest.is_changed()
        || dungeon.is_changed()
        || dialogue.is_changed()
        || nearby.is_changed())
    {
        return;
    }
    let Ok(mut text) = prompts.single_mut() else {
        return;
    };
    **text = match state.get() {
        GameState::Loading => "Loading the ice shelf...".into(),
        GameState::Failed => format!(
            "{}\nCheck assets/models and restart the game.",
            status.error.as_deref().unwrap_or("Asset loading failed")
        ),
        GameState::Playing if dialogue.text.is_none() => {
            nearby.0.map_or(String::new(), |(_, target)| match target {
                InteractionTarget::Quest(Target::Keeper) => "[E] Speak to Nemi".into(),
                InteractionTarget::Quest(Target::Core) => "[E] Recover power core".into(),
                InteractionTarget::Pickup(pickup) => {
                    format!("[E] Pick up {} x{}", pickup.item.name(), pickup.quantity)
                }
                InteractionTarget::Dungeon(target) => match target {
                    DungeonTarget::Hatch if !DungeonProgress::hatch_unlocked(*quest) => {
                        "[E] Inspect the sealed hatch".into()
                    }
                    DungeonTarget::Hatch => "[E] Enter the Old Relay".into(),
                    DungeonTarget::Exit => "[E] Return to Nivalis".into(),
                    DungeonTarget::Switch if dungeon.gate_open() => {
                        "[E] Check maintenance switch".into()
                    }
                    DungeonTarget::Switch => "[E] Operate maintenance switch".into(),
                    DungeonTarget::SurveyLog => "[E] Read the survey log".into(),
                },
            })
        }
        _ => String::new(),
    };
}

fn refresh_dialogue(
    dialogue: Res<Dialogue>,
    mut panel: Query<(&mut Text, &mut Visibility), With<DialogueText>>,
) {
    if !dialogue.is_changed() {
        return;
    }
    let Ok((mut text, mut visibility)) = panel.single_mut() else {
        return;
    };
    if let Some(content) = dialogue.text {
        **text = format!("{content}\n\n[E / ENTER] Continue");
        *visibility = Visibility::Visible;
    } else {
        *visibility = Visibility::Hidden;
    }
}

fn refresh_pause(
    state: Res<State<GameState>>,
    mut panel: Query<&mut Visibility, With<PausePanel>>,
) {
    if state.is_changed()
        && let Ok(mut visibility) = panel.single_mut()
    {
        *visibility = if *state.get() == GameState::Paused {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
