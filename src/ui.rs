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
            .add_systems(PreUpdate, pause.after(bevy::input::InputSystems))
            .add_systems(
                Update,
                (
                    refresh_hud.after(respawn_defeated),
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
    health: Option<(u16, u16)>,
}

fn refresh_hud(
    state: Res<State<GameState>>,
    quest: Res<Quest>,
    layout: Res<WorldLayout>,
    dungeon: Res<DungeonProgress>,
    targets: Query<(&DungeonTarget, &Transform)>,
    player: Query<(&Player, &Transform, &Health)>,
    mut hud: Query<&mut Text, With<Hud>>,
    mut last: Local<Option<HudSnapshot>>,
) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };
    if matches!(state.get(), GameState::Title | GameState::Loading) {
        if last.take().is_some() {
            **text = String::new();
        }
        return;
    }
    let avatar = player.single().ok();
    let area = avatar.map_or(Area::Surface, |(player, _, _)| player.area);
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
        distance: avatar.and_then(|(_, transform, _)| {
            destination.map(|point| transform.translation.distance(point).round() as i32)
        }),
        health: avatar.map(|(_, _, health)| (health.current(), health.maximum())),
    };
    if last.as_ref() == Some(&snapshot) {
        return;
    }
    **text = format!(
        "{}{}\n\n{}{}",
        snapshot.location,
        snapshot
            .health
            .map_or(String::new(), |(current, maximum)| format!(
                "\nHealth {current}/{maximum}"
            )),
        snapshot.objective,
        snapshot
            .distance
            .map_or(String::new(), |distance| format!("\n{distance} m"))
    );
    *last = Some(snapshot);
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
