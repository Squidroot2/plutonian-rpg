use crate::{
    GameState,
    combat::Health,
    controls::{Control, Keybinds},
    dungeon::{Area, DungeonProgress, DungeonTarget},
    interaction::{InteractionSystems, InteractionTarget},
    loading::LoadStatus,
    player::{Player, respawn_defeated},
    quest::{Dialogue, Nearby, Quest, Target},
    settings::SettingsMenu,
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
pub(crate) struct HealthBar;
#[derive(Component)]
struct HealthBarTrack;
#[derive(Component)]
pub(crate) struct HealthBarFill;
const HEALTH_BAR_FADE_SECONDS: f32 = 0.65;
const HEALTH_TRACK_COLOR: Color = Color::srgb(0.045, 0.065, 0.09);
const HEALTH_BORDER_COLOR: Color = Color::srgb(0.30, 0.38, 0.44);
const HEALTH_FILL_COLOR: Color = Color::srgb(0.94, 0.32, 0.35);
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct DialogueText;
#[derive(Component)]
struct PausePanel;
#[derive(Component, Clone, Copy)]
enum Action {
    Resume,
    Settings,
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
pub(crate) fn text_style(size: f32) -> (TextFont, TextColor) {
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
                HudText::Objective,
                Text::default(),
                text_style(18.0),
                Pickable::IGNORE,
            ));
        });
    commands
        .spawn((
            HealthBar,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(24),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            bar.spawn((
                HealthBarTrack,
                Node {
                    width: px(368),
                    height: px(10),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(2)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(HEALTH_TRACK_COLOR),
                BorderColor::all(HEALTH_BORDER_COLOR),
                Pickable::IGNORE,
            ))
            .with_child((
                HealthBarFill,
                Node {
                    width: percent(0),
                    height: percent(100),
                    ..default()
                },
                BackgroundColor(HEALTH_FILL_COLOR),
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
            parent.spawn((Text::new("Paused"), text_style(30.0)));
            for (label, action) in [
                ("Resume", Action::Resume),
                ("Settings", Action::Settings),
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
    bindings: Res<Keybinds>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut dialogue: ResMut<Dialogue>,
    windows: Query<&Window, With<PrimaryWindow>>,
    settings: Option<Res<SettingsMenu>>,
) {
    if settings.is_some_and(|menu| menu.is_open()) {
        return;
    }
    if smoke.is_none() && *state.get() == GameState::Playing && windows.iter().any(|w| !w.focused) {
        next.set(GameState::Paused);
    }
    let inventory_pressed = bindings.just_pressed(Control::Inventory, &keys);
    if inventory_pressed && *state.get() == GameState::Playing && dialogue.text.is_none() {
        next.set(GameState::Inventory);
    }
    if *state.get() == GameState::Inventory
        && (inventory_pressed || keys.just_pressed(KeyCode::Escape))
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
    mut settings: ResMut<SettingsMenu>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if *state.get() != GameState::Paused
        || settings.is_open()
        || click.button != PointerButton::Primary
    {
        return;
    }
    if let Ok(action) = query.get(click.entity) {
        match action {
            Action::Resume => next.set(GameState::Playing),
            Action::Settings => settings.open(),
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
    time: Res<Time>,
    player: Query<&Health, With<Player>>,
    mut bar: Query<&mut Visibility, With<HealthBar>>,
    mut track: Query<(&mut BackgroundColor, &mut BorderColor), With<HealthBarTrack>>,
    mut fill: Query<
        (&mut Node, &mut BackgroundColor),
        (With<HealthBarFill>, Without<HealthBarTrack>),
    >,
    mut opacity: Local<f32>,
) {
    let Ok(mut bar) = bar.single_mut() else {
        return;
    };
    let Some(health) = player.single().ok().filter(|health| health.maximum() > 0) else {
        *opacity = 0.0;
        bar.set_if_neq(Visibility::Hidden);
        return;
    };
    match state.get() {
        GameState::Playing | GameState::Inventory | GameState::Paused => {
            if health.current() < health.maximum() {
                *opacity = 1.0;
            } else if *state.get() == GameState::Playing {
                *opacity = (*opacity - time.delta_secs() / HEALTH_BAR_FADE_SECONDS).max(0.0);
            }
        }
        _ => *opacity = 0.0,
    }
    if *opacity == 0.0 {
        bar.set_if_neq(Visibility::Hidden);
        return;
    }
    if let (Ok((mut track_color, mut border)), Ok((mut fill, mut fill_color))) =
        (track.single_mut(), fill.single_mut())
    {
        bar.set_if_neq(Visibility::Inherited);
        let width = percent(100.0 * f32::from(health.current()) / f32::from(health.maximum()));
        if fill.width != width {
            fill.width = width;
        }
        track_color.set_if_neq(BackgroundColor(HEALTH_TRACK_COLOR.with_alpha(*opacity)));
        border.set_if_neq(BorderColor::all(HEALTH_BORDER_COLOR.with_alpha(*opacity)));
        fill_color.set_if_neq(BackgroundColor(HEALTH_FILL_COLOR.with_alpha(*opacity)));
    }
}

fn refresh_prompt(
    state: Res<State<GameState>>,
    status: Res<LoadStatus>,
    quest: Res<Quest>,
    dungeon: Res<DungeonProgress>,
    dialogue: Res<Dialogue>,
    nearby: Res<Nearby>,
    bindings: Res<Keybinds>,
    mut prompts: Query<&mut Text, With<StatusText>>,
) {
    if !(state.is_changed()
        || status.is_changed()
        || quest.is_changed()
        || dungeon.is_changed()
        || dialogue.is_changed()
        || nearby.is_changed()
        || bindings.is_changed())
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
            let key = bindings.get(Control::Interact).label();
            nearby.0.map_or(String::new(), |(_, target)| {
                let action: String = match target {
                    InteractionTarget::Quest(Target::Keeper) => "Speak to Nemi".into(),
                    InteractionTarget::Quest(Target::Core) => "Recover power core".into(),
                    InteractionTarget::Pickup(pickup) => {
                        format!("Pick up {} x{}", pickup.item.name(), pickup.quantity)
                    }
                    InteractionTarget::Dungeon(target) => match target {
                        DungeonTarget::Hatch if !DungeonProgress::hatch_unlocked(*quest) => {
                            "Inspect the sealed hatch".into()
                        }
                        DungeonTarget::Hatch => "Enter the Old Relay".into(),
                        DungeonTarget::Exit => "Return to Nivalis".into(),
                        DungeonTarget::Switch if dungeon.gate_open() => {
                            "Check maintenance switch".into()
                        }
                        DungeonTarget::Switch => "Operate maintenance switch".into(),
                        DungeonTarget::SurveyLog => "Read the survey log".into(),
                    },
                };
                format!("[{key}] {action}")
            })
        }
        _ => String::new(),
    };
}

fn refresh_dialogue(
    dialogue: Res<Dialogue>,
    bindings: Res<Keybinds>,
    mut panel: Query<(&mut Text, &mut Visibility), With<DialogueText>>,
) {
    if !dialogue.is_changed() && !bindings.is_changed() {
        return;
    }
    let Ok((mut text, mut visibility)) = panel.single_mut() else {
        return;
    };
    if let Some(content) = dialogue.text {
        let key = bindings.get(Control::Interact).label();
        **text = format!("{content}\n\n[{key} / Enter] Continue");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{CombatPlugin, Hit};
    use bevy::{state::app::StatesPlugin, time::TimeUpdateStrategy};
    use std::time::Duration;

    #[test]
    fn the_bar_stays_hidden_at_full_health_then_shows_damage_recovery_and_fade() {
        let step = Duration::from_millis(125);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CombatPlugin))
            .insert_state(GameState::Playing)
            .init_resource::<Dialogue>()
            .insert_resource(Time::<Fixed>::from_duration(step))
            .insert_resource(TimeUpdateStrategy::ManualDuration(step))
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                refresh_health.after(crate::combat::CombatSystems::Resolve),
            );
        let target = app.world_mut().spawn(Player::default()).id();
        let source = app.world_mut().spawn_empty().id();
        let hit = Hit {
            source,
            target,
            damage: 20,
            contact: None,
        };
        let bar = |app: &mut App| {
            let world = app.world_mut();
            *world
                .query_filtered::<&Visibility, With<HealthBar>>()
                .single(world)
                .expect("HUD setup must spawn one health bar")
        };
        let fill = |app: &mut App| {
            let world = app.world_mut();
            let (node, color) = world
                .query_filtered::<(&Node, &BackgroundColor), With<HealthBarFill>>()
                .single(world)
                .expect("HUD setup must spawn one health fill");
            (node.width, color.0.alpha())
        };
        let health = |app: &App| {
            app.world()
                .get::<Health>(target)
                .expect("the player must have health")
                .current()
        };

        app.update();
        assert_eq!(bar(&mut app), Visibility::Hidden);
        app.world_mut().write_message(hit);
        app.update();
        assert_eq!(bar(&mut app), Visibility::Inherited);
        assert_eq!(fill(&mut app), (percent(80), 1.0));
        for _ in 0..16 {
            app.update();
        }
        app.world_mut().write_message(hit);
        app.update();
        assert_eq!(health(&app), 60);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
        for _ in 0..45 {
            app.update();
        }
        assert_eq!(health(&app), 60, "recovery must wait while paused");
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        for _ in 0..40 {
            app.update();
        }
        assert_eq!(
            health(&app),
            60,
            "the second hit must restart the five-second delay"
        );
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(health(&app), 70, "one active second must recover 10 health");
        assert_eq!(fill(&mut app), (percent(70), 1.0));
        for _ in 0..24 {
            app.update();
        }
        assert_eq!(health(&app), 100);
        assert_eq!(bar(&mut app), Visibility::Inherited);
        let (width, opacity) = fill(&mut app);
        assert_eq!(width, percent(100));
        assert!(opacity > 0.0 && opacity < 1.0);
        for _ in 0..6 {
            app.update();
        }
        assert_eq!(bar(&mut app), Visibility::Hidden);
        app.world_mut().write_message(hit);
        app.update();
        assert_eq!(bar(&mut app), Visibility::Inherited);
        assert_eq!(fill(&mut app), (percent(80), 1.0));
    }
}
