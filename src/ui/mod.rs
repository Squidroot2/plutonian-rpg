pub(crate) mod death_screen;
pub(crate) mod dialogue_ui;
pub(crate) mod inventory_ui;
pub(crate) mod loot_ui;
pub(crate) mod menu_ui;
pub(crate) mod quest_ui;
pub(crate) mod settings;
pub(crate) mod storage_ui;
pub(crate) mod title;

use crate::{
    GameState,
    camera::Orbit,
    cave,
    combat::Health,
    content::Items,
    controls::{Control, Keybinds},
    dialogue::{Dialogue, DialogueCatalog, Speaker},
    dungeon::{Area, DungeonProgress, DungeonTarget},
    experience::{ExperienceNotice, ExperienceUpdates},
    interaction::{InteractionSystems, InteractionTarget, Nearby, Target},
    loading::LoadStatus,
    player::{Player, Stamina},
    quest::QuestJournal,
    save::{SaveNotice, SaveSlot, save_at_bed},
    ui::menu_ui::PlayerMenu,
    ui::{settings::SettingsMenu, title::picker::SavePicker},
    world::WorldLayout,
};
use bevy::{app::AppExit, asset::AssetId, picking::hover::PickingInteraction, prelude::*};

pub struct GameUiPlugin;
#[derive(Component)]
struct LocationText;
#[derive(Component)]
struct Compass;
#[derive(Component)]
pub(crate) struct HealthBar;
#[derive(Component)]
pub(crate) struct HealthBarFill;
#[derive(Component)]
pub(crate) struct StaminaBar;
#[derive(Component)]
pub(crate) struct StaminaBarFill;

#[derive(Component)]
struct StatusBar {
    kind: StatusBarKind,
    track: Entity,
    fill: Entity,
    opacity: f32,
}

#[derive(Component)]
struct StatusBarTrack;

#[derive(Clone, Copy)]
enum StatusBarKind {
    Health,
    Stamina,
}

impl StatusBarKind {
    fn fill_color(self) -> Color {
        match self {
            Self::Health => Color::srgb(0.94, 0.32, 0.35),
            Self::Stamina => Color::srgb(0.36, 0.78, 0.43),
        }
    }
}

const BAR_FADE_SECONDS: f32 = 0.65;
const BAR_TRACK_COLOR: Color = Color::srgb(0.045, 0.065, 0.09);
const BAR_BORDER_COLOR: Color = Color::srgb(0.30, 0.38, 0.44);
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct ExperienceText;
#[derive(Component)]
struct PausePanel;
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Action {
    #[default]
    Resume,
    Load,
    Settings,
    Quit,
}

impl Action {
    const ALL: [Self; 4] = [Self::Resume, Self::Load, Self::Settings, Self::Quit];
}

#[derive(Resource, Default)]
struct PauseSelection(Action);

const PAUSE_BUTTON_OUTLINE: Color = Color::srgb(0.86, 0.94, 1.0);
const PAUSE_BUTTON_IDLE_OUTLINE: Color = Color::srgba(0.50, 0.65, 0.78, 0.48);
impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        // Font constraint: neither shipped font — Exo 2 (the default font
        // below) nor Michroma (the title font) — contains arrow glyphs or
        // other symbol/dingbat ranges, so UI text must stick to Latin
        // letters, digits, and common punctuation. The punctuation verified
        // present in Exo 2 is the em dash, en dash, middle dot, bullet,
        // › (U+203A), and the curly apostrophe. A future UI string that
        // needs a new non-ASCII character should be checked against the
        // font's character map before it ships, not discovered as a
        // missing glyph in game.
        // Replace Bevy's default face before text layout starts, including for
        // text spawned with an implicit TextFont.
        let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
        fonts
            .insert(
                AssetId::default(),
                Font::from_bytes(include_bytes!("../../assets/fonts/Exo2-Regular.ttf").to_vec()),
            )
            .expect("Bevy's default font asset ID must accept the game UI font");
        fonts
            .insert(
                title::TITLE_FONT.id(),
                Font::from_bytes(
                    include_bytes!("../../assets/fonts/Michroma-Regular.ttf").to_vec(),
                ),
            )
            .expect("the title font asset ID must accept the title font");
        app.add_plugins((
            dialogue_ui::DialogueUiPlugin,
            death_screen::DeathScreenPlugin,
        ))
        .add_systems(Startup, setup)
        .init_resource::<PauseSelection>()
        .add_systems(OnEnter(GameState::Paused), reset_pause_selection)
        .add_systems(PreUpdate, pause.after(bevy::input::InputSystems))
        .add_systems(
            PreUpdate,
            pause_menu_input
                .after(bevy::input::InputSystems)
                .after(pause),
        )
        .add_systems(
            Update,
            (
                (refresh_hud, refresh_status_bars).after(save_at_bed),
                refresh_compass,
                refresh_prompt.after(save_at_bed),
                refresh_experience_notice.after(ExperienceUpdates),
                refresh_pause,
                style_pause_buttons,
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
                left: px(0),
                right: px(0),
                top: px(24),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|hud| {
            hud.spawn((
                LocationText,
                Text::default(),
                text_style(18.0),
                Pickable::IGNORE,
            ));
            hud.spawn((
                Compass,
                Text::default(),
                text_style(15.0),
                Node {
                    padding: UiRect::axes(px(10), px(2)),
                    border_radius: BorderRadius::all(px(3)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.018, 0.035, 0.055, 0.82)),
                Visibility::Hidden,
                Pickable::IGNORE,
            ));
        });
    let (bar, fill) = spawn_status_bar(&mut commands, StatusBarKind::Health, 24);
    commands.entity(bar).insert(HealthBar);
    commands.entity(fill).insert(HealthBarFill);
    let (bar, fill) = spawn_status_bar(&mut commands, StatusBarKind::Stamina, 10);
    commands.entity(bar).insert(StaminaBar);
    commands.entity(fill).insert(StaminaBarFill);
    commands.spawn((
        ExperienceText,
        Text::default(),
        text_style(18.0),
        Node {
            position_type: PositionType::Absolute,
            top: px(24),
            right: px(30),
            padding: UiRect::axes(px(10), px(4)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.018, 0.035, 0.055, 0.82)),
        Visibility::Hidden,
        Pickable::IGNORE,
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
                ("Load Game", Action::Load),
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
                            border: UiRect::all(px(1)),
                            ..default()
                        },
                        BorderColor::all(PAUSE_BUTTON_IDLE_OUTLINE),
                        BackgroundColor(Color::srgb(0.05, 0.13, 0.17)),
                    ))
                    .observe(buttons)
                    .with_children(|p| {
                        p.spawn((Text::new(label), text_style(20.0), Pickable::IGNORE));
                    });
            }
        });
}

fn spawn_status_bar(commands: &mut Commands, kind: StatusBarKind, bottom: u16) -> (Entity, Entity) {
    let bar = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(bottom),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .id();
    let track = commands
        .spawn((
            ChildOf(bar),
            StatusBarTrack,
            Node {
                width: px(368),
                height: px(10),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(2)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(BAR_TRACK_COLOR),
            BorderColor::all(BAR_BORDER_COLOR),
            Pickable::IGNORE,
        ))
        .id();
    let fill = commands
        .spawn((
            ChildOf(track),
            Node {
                width: percent(0),
                height: percent(100),
                ..default()
            },
            BackgroundColor(kind.fill_color()),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(bar).insert(StatusBar {
        kind,
        track,
        fill,
        opacity: 0.0,
    });
    (bar, fill)
}

fn reset_pause_selection(mut selection: ResMut<PauseSelection>) {
    *selection = PauseSelection::default();
}

fn pause_menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    state: Res<State<GameState>>,
    mut settings: ResMut<SettingsMenu>,
    mut picker: ResMut<SavePicker>,
    mut selection: ResMut<PauseSelection>,
    mut slot: ResMut<SaveSlot>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if *state.get() != GameState::Paused
        || settings.is_open()
        || picker.is_open()
        || !matches!(*next, NextState::Unchanged)
    {
        return;
    }

    if bindings.just_pressed(Control::MenuUp, &keys) {
        selection.0 = next_action(selection.0, -1);
    } else if bindings.just_pressed(Control::MenuDown, &keys) {
        selection.0 = next_action(selection.0, 1);
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        activate(
            selection.0,
            &mut settings,
            &mut picker,
            &mut slot,
            &mut next,
            &mut exit,
        );
    }
}

fn next_action(current: Action, direction: isize) -> Action {
    let index = Action::ALL
        .iter()
        .position(|&action| action == current)
        .expect("pause action is listed") as isize;
    Action::ALL[(index + direction).rem_euclid(Action::ALL.len() as isize) as usize]
}

pub(crate) fn pause(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut dialogue: ResMut<Dialogue>,
    settings: Option<Res<SettingsMenu>>,
    picker: Option<Res<SavePicker>>,
    log: Option<ResMut<PlayerMenu>>,
    drop_prompt: Option<ResMut<crate::ui::inventory_ui::InventoryUiState>>,
) {
    if !matches!(*next, NextState::Unchanged)
        || settings.is_some_and(|menu| menu.is_open())
        || picker.is_some_and(|picker| picker.is_open())
    {
        return;
    }
    if *state.get() == GameState::Playing
        && !dialogue.is_open()
        && let Some(mut menu) = log
    {
        let tab = keys.just_pressed(KeyCode::Tab);
        let inventory = bindings.just_pressed(Control::Inventory, &keys);
        let quest_log = bindings.just_pressed(Control::QuestLog, &keys);
        let escape = keys.just_pressed(KeyCode::Escape);
        if escape
            && menu.is_on(crate::ui::menu_ui::MenuTab::Inventory)
            && drop_prompt.is_some_and(|mut ui| ui.cancel_drop())
        {
            return;
        }
        if ((tab || inventory || quest_log || escape)
            && menu.shortcuts(tab, inventory, quest_log, escape))
            || menu.is_open()
        {
            return;
        }
    }
    if keys.just_pressed(KeyCode::Escape)
        || (keys.just_pressed(KeyCode::Tab)
            && matches!(state.get(), GameState::Looting | GameState::Storage))
    {
        match state.get() {
            GameState::Playing if dialogue.is_open() => dialogue.request_close(),
            GameState::Playing => next.set(GameState::Paused),
            GameState::Paused | GameState::Looting | GameState::Storage => {
                next.set(GameState::Playing)
            }
            _ => (),
        }
    }
}
fn buttons(
    click: On<PointerClick>,
    query: Query<&Action>,
    state: Res<State<GameState>>,
    mut settings: ResMut<SettingsMenu>,
    mut picker: ResMut<SavePicker>,
    mut slot: ResMut<SaveSlot>,
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
        activate(
            *action,
            &mut settings,
            &mut picker,
            &mut slot,
            &mut next,
            &mut exit,
        );
    }
}

fn activate(
    action: Action,
    settings: &mut SettingsMenu,
    picker: &mut SavePicker,
    slot: &mut SaveSlot,
    next: &mut NextState<GameState>,
    exit: &mut MessageWriter<AppExit>,
) {
    match action {
        Action::Resume => next.set(GameState::Playing),
        Action::Load => {
            // Refresh before the availability gate so saves created during
            // this run are visible through either input path.
            slot.refresh();
            if slot.available() {
                picker.open();
            }
        }
        Action::Settings => settings.open(),
        Action::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

fn style_pause_buttons(
    selection: Res<PauseSelection>,
    mut buttons: Query<(&Action, Option<&PickingInteraction>, &mut BorderColor)>,
) {
    for (action, hover, mut border) in &mut buttons {
        let highlighted = *action == selection.0
            || matches!(
                hover,
                Some(PickingInteraction::Hovered | PickingInteraction::Pressed)
            );
        border.set_if_neq(BorderColor::all(if highlighted {
            PAUSE_BUTTON_OUTLINE
        } else {
            PAUSE_BUTTON_IDLE_OUTLINE
        }));
    }
}
fn refresh_hud(
    state: Res<State<GameState>>,
    layout: Res<WorldLayout>,
    player: Query<(&Player, &Transform)>,
    mut hud: Query<&mut Text, With<LocationText>>,
) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };
    let player = player.single().ok();
    let area = player.map_or(Area::Surface, |(player, _)| player.area);
    let location = match (state.get(), *layout, area) {
        (GameState::Boot | GameState::Title | GameState::Loading | GameState::GameOver, _, _) => "",
        (_, _, Area::OldRelay) => "THE OLD RELAY",
        (_, WorldLayout::Outpost, Area::Surface) => player
            .and_then(|(_, pose)| {
                crate::crossing::surface_location(pose.translation)
                    .or_else(|| cave::surface_location(pose.translation))
            })
            .unwrap_or("OUTPOST / NIVALIS"),
        (_, WorldLayout::SmokeArena, Area::Surface) => "SMOKE ARENA",
    };
    if text.as_str() != location {
        **text = location.to_owned();
    }
}

fn refresh_compass(
    state: Res<State<GameState>>,
    orbit: Res<Orbit>,
    mut compass: Query<(&mut Text, &mut Visibility), With<Compass>>,
    mut last_bearing: Local<Option<u16>>,
) {
    let Ok((mut text, mut visibility)) = compass.single_mut() else {
        return;
    };
    let visible = matches!(
        state.get(),
        GameState::Playing | GameState::Paused | GameState::Looting | GameState::Storage
    );
    visibility.set_if_neq(if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !visible {
        return;
    }
    let (degrees, direction) = camera_bearing(orbit.yaw);
    if *last_bearing != Some(degrees) {
        **text = format!("{direction}  {degrees:03} deg");
        *last_bearing = Some(degrees);
    }
}

fn camera_bearing(yaw: f32) -> (u16, &'static str) {
    const DIRECTIONS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    // At zero yaw the camera looks toward -Z. Positive orbit yaw turns west.
    let bearing = (-yaw).to_degrees().rem_euclid(360.0);
    let degrees = bearing.round() as u16 % 360;
    let direction = (f32::from(degrees) / 45.0).round() as usize % DIRECTIONS.len();
    (degrees, DIRECTIONS[direction])
}

fn refresh_status_bars(
    state: Res<State<GameState>>,
    time: Res<Time>,
    player: Query<(&Health, &Stamina), With<Player>>,
    mut bars: Query<(&mut StatusBar, &mut Visibility)>,
    mut track: Query<(&mut BackgroundColor, &mut BorderColor), With<StatusBarTrack>>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), Without<StatusBarTrack>>,
) {
    let player = player.single().ok();
    let show_bars = matches!(
        state.get(),
        GameState::Playing | GameState::Paused | GameState::Looting | GameState::Storage
    );
    for (mut bar, mut visibility) in &mut bars {
        let amount = player.and_then(|(health, stamina)| match bar.kind {
            StatusBarKind::Health => (health.maximum() > 0)
                .then(|| 100.0 * f32::from(health.current()) / f32::from(health.maximum())),
            StatusBarKind::Stamina => Some(100.0 * stamina.fraction()),
        });
        let Some(amount) = amount.filter(|_| show_bars) else {
            bar.opacity = 0.0;
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        if amount < 100.0 {
            bar.opacity = 1.0;
        } else if *state.get() == GameState::Playing && bar.opacity > 0.0 {
            bar.opacity = (bar.opacity - time.delta_secs() / BAR_FADE_SECONDS).max(0.0);
        }
        if bar.opacity == 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        if let (Ok((mut track_color, mut border)), Ok((mut fill, mut fill_color))) =
            (track.get_mut(bar.track), fill.get_mut(bar.fill))
        {
            visibility.set_if_neq(Visibility::Inherited);
            let width = percent(amount);
            if fill.width != width {
                fill.width = width;
            }
            track_color.set_if_neq(BackgroundColor(BAR_TRACK_COLOR.with_alpha(bar.opacity)));
            border.set_if_neq(BorderColor::all(BAR_BORDER_COLOR.with_alpha(bar.opacity)));
            fill_color.set_if_neq(BackgroundColor(
                bar.kind.fill_color().with_alpha(bar.opacity),
            ));
        }
    }
}

fn refresh_experience_notice(
    state: Res<State<GameState>>,
    notice: Res<ExperienceNotice>,
    mut label: Single<(&mut Text, &mut Visibility), With<ExperienceText>>,
) {
    let (text, visibility) = &mut *label;
    if *state.get() == GameState::Playing
        && let Some(value) = &notice.text
    {
        if text.0 != *value {
            text.0.clone_from(value);
        }
        visibility.set_if_neq(Visibility::Inherited);
    } else {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

fn refresh_prompt(
    items: Option<Res<Items>>,
    state: Res<State<GameState>>,
    status: Res<LoadStatus>,
    quest: Res<QuestJournal>,
    dungeon: Res<DungeonProgress>,
    dialogue: Res<Dialogue>,
    nearby: Res<Nearby>,
    catalog: Res<DialogueCatalog>,
    speakers: Query<&Speaker>,
    bindings: Res<Keybinds>,
    log: Res<PlayerMenu>,
    notice: Res<SaveNotice>,
    mut prompts: Query<&mut Text, With<StatusText>>,
) {
    if !(state.is_changed()
        || status.is_changed()
        || quest.is_changed()
        || log.is_changed()
        || dungeon.is_changed()
        || dialogue.is_changed()
        || nearby.is_changed()
        || notice.is_changed()
        || bindings.is_changed())
    {
        return;
    }
    let Ok(mut text) = prompts.single_mut() else {
        return;
    };
    **text = match state.get() {
        GameState::Boot => "Loading gameplay catalogs...".into(),
        GameState::Loading => "Loading the ice shelf...".into(),
        GameState::Failed => format!(
            "{}\nCheck the game files and restart the game.",
            status.error.as_deref().unwrap_or("Asset loading failed")
        ),
        GameState::Playing if !dialogue.is_open() && !log.is_open() => {
            if let Some(notice) = notice.text {
                **text = notice.into();
                return;
            }
            let key = bindings.get(Control::Interact).label();
            nearby.0.map_or(String::new(), |(entity, target)| {
                let action: String = match target {
                    InteractionTarget::Talk => {
                        let Some(profile) = speakers
                            .get(entity)
                            .ok()
                            .and_then(|speaker| catalog.profile(speaker.profile))
                        else {
                            return String::new();
                        };
                        format!("Speak to {}", profile.name)
                    }
                    InteractionTarget::Quest(Target::Core) => "Recover power core".into(),
                    InteractionTarget::Quest(Target::BeaconSocket) => {
                        "Install the beacon core".into()
                    }
                    InteractionTarget::Pickup(pickup) => {
                        format!(
                            "Pick up {} x{}",
                            items.as_ref().map_or_else(
                                || pickup.item.as_str(),
                                |items| items[pickup.item].name()
                            ),
                            pickup.quantity
                        )
                    }
                    InteractionTarget::Corpse => "Loot corpse".into(),
                    InteractionTarget::Chest(chest) => format!("Open {}", chest.name()),
                    InteractionTarget::WeaponRack => "Open Weapon rack".into(),
                    InteractionTarget::SampleStation => "Verify Mycelium samples".into(),
                    InteractionTarget::Bed => "Save game".into(),
                    InteractionTarget::Dungeon(target) => match target {
                        DungeonTarget::Hatch if !quest.beacon_powered() => {
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

fn refresh_pause(
    state: Res<State<GameState>>,
    picker: Option<Res<SavePicker>>,
    mut panel: Query<&mut Visibility, With<PausePanel>>,
) {
    if (state.is_changed() || picker.as_ref().is_some_and(|picker| picker.is_changed()))
        && let Ok(mut visibility) = panel.single_mut()
    {
        *visibility = if *state.get() == GameState::Paused
            && picker.as_ref().is_none_or(|picker| !picker.is_open())
        {
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

    /// Owns a unique temporary save directory for one pause-menu test.
    ///
    /// `SaveSlot` discovers saves by listing this directory, so the tests
    /// must use a real on-disk directory to exercise the refresh path
    /// instead of hand-filling entries. Keep the guard alive for the whole
    /// test; dropping it removes the directory.
    struct SaveDirGuard(std::path::PathBuf);

    impl SaveDirGuard {
        /// Create a fresh, uniquely named empty save directory.
        ///
        /// The label only namespaces the path; uniqueness comes from the
        /// process id, current time, and a per-process counter so parallel
        /// tests never share a directory.
        fn new(label: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "plutonian-ui-{label}-{}-{nanos}-{count}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).expect("create test save directory");
            Self(dir)
        }

        /// Copy a valid current-format save into the directory.
        ///
        /// The copy lets `SaveSlot::refresh` read a real player name, so
        /// the test exercises the same disk-to-picker path as a bed save
        /// created during the run. Call it before `SaveSlot::open` to
        /// model a save that exists at launch, or afterwards to model a
        /// save created after the title snapshot.
        fn copy_fixture_save(&self, name: &str) {
            let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/inspector-v15.save");
            std::fs::copy(&source, self.0.join(name)).expect("copy fixture save");
        }
    }

    impl Drop for SaveDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn pause_load_button_opens_the_save_picker_when_a_save_exists() {
        let dir = SaveDirGuard::new("pause-existing");
        dir.copy_fixture_save("existing.save");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(GameState::Paused)
            .init_resource::<SettingsMenu>()
            .init_resource::<SavePicker>()
            .insert_resource(SaveSlot::open(Some(dir.0.clone())))
            .add_message::<AppExit>()
            .add_observer(buttons);
        let button = app.world_mut().spawn(Action::Load).id();
        let window = app.world_mut().spawn_empty().id();
        let camera = app.world_mut().spawn_empty().id();

        app.world_mut()
            .trigger(crate::interaction::pointer_click(button, window, camera));

        assert!(app.world().resource::<SavePicker>().is_open());
        assert_eq!(app.world().resource::<SaveSlot>().entries.len(), 1);
    }

    #[test]
    fn pause_load_button_opens_picker_for_save_created_during_run() {
        let dir = SaveDirGuard::new("pause-regression");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(GameState::Paused)
            .init_resource::<SettingsMenu>()
            .init_resource::<SavePicker>()
            .insert_resource(SaveSlot::open(Some(dir.0.clone())))
            .add_message::<AppExit>()
            .add_observer(buttons);
        assert!(
            !app.world().resource::<SaveSlot>().available(),
            "launch starts with no saves"
        );
        dir.copy_fixture_save("bed-save.save");
        assert!(
            app.world().resource::<SaveSlot>().entries.is_empty(),
            "the title snapshot stays stale until the pause Load click refreshes"
        );
        let button = app.world_mut().spawn(Action::Load).id();
        let window = app.world_mut().spawn_empty().id();
        let camera = app.world_mut().spawn_empty().id();

        app.world_mut()
            .trigger(crate::interaction::pointer_click(button, window, camera));

        assert!(app.world().resource::<SavePicker>().is_open());
        let entries = &app.world().resource::<SaveSlot>().entries;
        assert_eq!(entries.len(), 1, "the bed save must be listed");
        assert_eq!(
            entries[0].path,
            dir.0.join("bed-save.save"),
            "the picker must list the save created during the run"
        );
        assert!(
            entries[0].name.is_some(),
            "the fixture save must be readable through refresh"
        );
    }

    #[test]
    fn pause_selection_resets_to_resume_on_entering_paused() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(GameState::Playing)
            .init_resource::<PauseSelection>()
            .add_systems(OnEnter(GameState::Paused), reset_pause_selection);
        app.world_mut().resource_mut::<PauseSelection>().0 = Action::Quit;
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);

        app.update();

        assert_eq!(app.world().resource::<PauseSelection>().0, Action::Resume);
    }

    #[test]
    fn pause_menu_navigation_uses_bindings_and_wraps_in_both_directions() {
        let dir = SaveDirGuard::new("pause-navigation");
        let mut app = pause_input_app(&dir);

        press_pause_key(&mut app, KeyCode::KeyS);
        app.update();
        assert_eq!(app.world().resource::<PauseSelection>().0, Action::Load);

        press_pause_key(&mut app, KeyCode::KeyW);
        app.update();
        assert_eq!(app.world().resource::<PauseSelection>().0, Action::Resume);

        press_pause_key(&mut app, KeyCode::KeyW);
        app.update();
        assert_eq!(app.world().resource::<PauseSelection>().0, Action::Quit);

        press_pause_key(&mut app, KeyCode::KeyS);
        app.update();
        assert_eq!(app.world().resource::<PauseSelection>().0, Action::Resume);
    }

    #[test]
    fn enter_activates_the_selected_pause_action_through_the_shared_handler() {
        let dir = SaveDirGuard::new("pause-keyboard-load");
        let mut app = pause_input_app(&dir);
        dir.copy_fixture_save("keyboard-save.save");
        app.world_mut().resource_mut::<PauseSelection>().0 = Action::Load;

        press_pause_key(&mut app, KeyCode::NumpadEnter);
        app.update();

        assert!(app.world().resource::<SavePicker>().is_open());
        assert_eq!(app.world().resource::<SaveSlot>().entries.len(), 1);
    }

    fn pause_input_app(dir: &SaveDirGuard) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(GameState::Paused)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Keybinds>()
            .init_resource::<SettingsMenu>()
            .init_resource::<SavePicker>()
            .init_resource::<PauseSelection>()
            .insert_resource(SaveSlot::open(Some(dir.0.clone())))
            .add_message::<AppExit>()
            .add_systems(Update, pause_menu_input);
        app
    }

    fn press_pause_key(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release_all();
        keys.clear();
        keys.press(key);
    }

    #[test]
    fn compass_uses_camera_facing_and_wraps_at_north() {
        assert_eq!(camera_bearing(0.0), (0, "N"));
        assert_eq!(camera_bearing(90_f32.to_radians()), (270, "W"));
        assert_eq!(camera_bearing((-45_f32).to_radians()), (45, "NE"));
        assert_eq!(camera_bearing(180_f32.to_radians()), (180, "S"));
        assert_eq!(camera_bearing(360.2_f32.to_radians()), (0, "N"));
    }

    #[test]
    fn the_stamina_bar_shows_spending_and_recovery_then_fades_independently() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(GameState::Playing)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                125,
            )))
            .add_systems(Startup, setup)
            .add_systems(Update, refresh_status_bars);
        let player = app.world_mut().spawn(Player::default()).id();
        let stamina_bar = |app: &mut App| {
            let world = app.world_mut();
            let (node, visibility) = world
                .query_filtered::<(&Node, &Visibility), With<StaminaBar>>()
                .single(world)
                .expect("HUD setup must spawn one stamina bar");
            assert_eq!(node.bottom, px(10));
            *visibility
        };
        let stamina_fill = |app: &mut App| {
            let world = app.world_mut();
            let (node, color) = world
                .query_filtered::<(&Node, &BackgroundColor), With<StaminaBarFill>>()
                .single(world)
                .expect("HUD setup must spawn one stamina fill");
            (node.width, color.0.alpha())
        };

        app.update();
        assert_eq!(stamina_bar(&mut app), Visibility::Hidden);
        let mut avatar = app.world_mut().entity_mut(player);
        avatar
            .get_mut::<Health>()
            .expect("Player must supply its Health component")
            .set_current(80);
        assert!(
            avatar
                .get_mut::<Stamina>()
                .expect("Player must supply its Stamina component")
                .try_spend(50.0)
        );
        app.update();
        assert_eq!(stamina_bar(&mut app), Visibility::Inherited);
        assert_eq!(stamina_fill(&mut app), (percent(50), 1.0));

        for width in [75, 100] {
            app.world_mut()
                .get_mut::<Stamina>(player)
                .expect("Player must supply its Stamina component")
                .recover(1.0);
            app.update();
            assert_eq!(stamina_fill(&mut app).0, percent(width));
        }
        let opacity = stamina_fill(&mut app).1;
        assert!(opacity > 0.0 && opacity < 1.0);
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(stamina_bar(&mut app), Visibility::Hidden);
        let world = app.world_mut();
        let visibility = world
            .query_filtered::<&Visibility, With<HealthBar>>()
            .single(world)
            .expect("HUD setup must spawn one health bar");
        assert_eq!(*visibility, Visibility::Inherited);
    }

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
                refresh_status_bars.after(crate::combat::CombatSystems::Resolve),
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
