use super::{menu_ui, text_style};
use crate::{
    GameState,
    controls::Keybinds,
    save::{Bed, SaveAtBed, SleepWithoutSaving},
};
use bevy::{picking::hover::PickingInteraction, prelude::*};

const FADE_OUT_SECONDS: f32 = 0.8;
const HOLD_SECONDS: f32 = 0.4;
const FADE_IN_SECONDS: f32 = 1.8;
const FADE_TOTAL_SECONDS: f32 = FADE_OUT_SECONDS + HOLD_SECONDS + FADE_IN_SECONDS;
const BUTTON_OUTLINE: Color = Color::srgb(0.86, 0.94, 1.0);
const BUTTON_IDLE_OUTLINE: Color = Color::srgba(0.50, 0.65, 0.78, 0.48);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SleepFadeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SleepAction {
    Sleep,
    #[default]
    SleepAndSave,
    Cancel,
}

impl SleepAction {
    const ALL: [Self; 3] = [Self::Sleep, Self::SleepAndSave, Self::Cancel];

    fn label(self) -> &'static str {
        match self {
            Self::Sleep => "Sleep",
            Self::SleepAndSave => "Sleep and Save",
            Self::Cancel => "Cancel",
        }
    }
}

#[derive(Resource, Default)]
struct Selection(SleepAction);

/// Track hover changes so a stationary pointer does not override keyboard selection.
#[derive(Resource, Default)]
struct Hover(Option<SleepAction>);

#[derive(Component)]
struct SleepButton(SleepAction);

#[derive(Component)]
struct Fade(Timer, bool);

#[derive(Resource, Default)]
struct ChosenAction(SleepAction);

pub(crate) struct SleepPlugin;

impl Plugin for SleepPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .init_resource::<Hover>()
            .init_resource::<ChosenAction>()
            .add_systems(OnEnter(GameState::SleepMenu), (reset_selection, open))
            .add_systems(
                Update,
                (navigate, select_hovered_button, style_buttons)
                    .chain()
                    .run_if(in_state(GameState::SleepMenu)),
            )
            .add_systems(OnEnter(GameState::Sleeping), begin_fade)
            .add_systems(
                Update,
                advance_fade
                    .in_set(SleepFadeSet)
                    .run_if(in_state(GameState::Sleeping)),
            );
    }
}

fn reset_selection(mut selection: ResMut<Selection>, mut hover: ResMut<Hover>) {
    selection.0 = SleepAction::default();
    hover.0 = None;
}

fn open(mut commands: Commands) {
    commands
        .spawn((menu_ui::interaction_modal(GameState::SleepMenu),))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(18),
                        padding: UiRect::all(px(30)),
                        ..default()
                    },
                    BackgroundColor(menu_ui::PANEL),
                ))
                .with_children(|panel| {
                    panel.spawn((Text::new("Rest for the night?"), text_style(26.0)));
                    for action in SleepAction::ALL {
                        panel
                            .spawn((
                                SleepButton(action),
                                Node {
                                    width: px(260),
                                    height: px(48),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border: UiRect::all(px(1)),
                                    ..default()
                                },
                                BorderColor::all(BUTTON_IDLE_OUTLINE),
                                BackgroundColor(menu_ui::BUTTON),
                            ))
                            .observe(click)
                            .with_child((
                                Text::new(action.label()),
                                text_style(20.0),
                                Pickable::IGNORE,
                            ));
                    }
                });
        });
}

fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut selection: ResMut<Selection>,
    mut chosen: ResMut<ChosenAction>,
) {
    if *state.get() != GameState::SleepMenu || !matches!(*next, NextState::Unchanged) {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        next.set(GameState::Playing);
        return;
    }
    let direction = menu_ui::menu_direction(&bindings, &keys);
    if direction != 0 {
        selection.0 = adjacent(selection.0, direction);
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        activate(selection.0, &mut chosen, &mut next);
    }
}

fn adjacent(current: SleepAction, direction: isize) -> SleepAction {
    let index = SleepAction::ALL
        .iter()
        .position(|&action| action == current)
        .expect("sleep action is listed") as isize;
    SleepAction::ALL[(index + direction).rem_euclid(SleepAction::ALL.len() as isize) as usize]
}

fn click(
    event: On<PointerClick>,
    buttons: Query<&SleepButton>,
    state: Res<State<GameState>>,
    mut chosen: ResMut<ChosenAction>,
    mut next: ResMut<NextState<GameState>>,
) {
    if *state.get() != GameState::SleepMenu
        || event.button != PointerButton::Primary
        || !matches!(*next, NextState::Unchanged)
    {
        return;
    }
    if let Ok(button) = buttons.get(event.entity) {
        activate(button.0, &mut chosen, &mut next);
    }
}

fn activate(action: SleepAction, chosen: &mut ChosenAction, next: &mut NextState<GameState>) {
    if action == SleepAction::Cancel {
        next.set(GameState::Playing);
    } else {
        chosen.0 = action;
        next.set(GameState::Sleeping);
    }
}

fn style_buttons(selection: Res<Selection>, mut buttons: Query<(&SleepButton, &mut BorderColor)>) {
    for (button, mut border) in &mut buttons {
        border.set_if_neq(BorderColor::all(if button.0 == selection.0 {
            BUTTON_OUTLINE
        } else {
            BUTTON_IDLE_OUTLINE
        }));
    }
}

fn select_hovered_button(
    mut selection: ResMut<Selection>,
    mut last_hovered: ResMut<Hover>,
    buttons: Query<(&SleepButton, Option<&PickingInteraction>)>,
) {
    let hovered = buttons.iter().find_map(|(button, interaction)| {
        matches!(
            interaction,
            Some(PickingInteraction::Hovered | PickingInteraction::Pressed)
        )
        .then_some(button.0)
    });
    if hovered != last_hovered.0 {
        if let Some(action) = hovered {
            selection.0 = action;
        }
        last_hovered.0 = hovered;
    }
}

fn begin_fade(mut commands: Commands) {
    commands.spawn((
        Fade(
            Timer::from_seconds(FADE_TOTAL_SECONDS, TimerMode::Once),
            false,
        ),
        DespawnOnExit(GameState::Sleeping),
        GlobalZIndex(110),
        BackgroundColor(Color::BLACK.with_alpha(0.0)),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        Pickable::IGNORE,
    ));
}

fn advance_fade(
    time: Res<Time>,
    fade: Single<(&mut Fade, &mut BackgroundColor)>,
    choice: Res<ChosenAction>,
    beds: Query<Entity, With<Bed>>,
    mut saving: MessageWriter<SaveAtBed>,
    mut sleeping: MessageWriter<SleepWithoutSaving>,
    mut next: ResMut<NextState<GameState>>,
) {
    let (mut fade, mut background) = fade.into_inner();
    let elapsed_before = fade.0.elapsed_secs();
    fade.0.tick(time.delta());
    let elapsed = fade.0.elapsed_secs();
    let alpha = if elapsed <= FADE_OUT_SECONDS {
        elapsed / FADE_OUT_SECONDS
    } else if elapsed < FADE_OUT_SECONDS + HOLD_SECONDS {
        1.0
    } else {
        1.0 - (elapsed - FADE_OUT_SECONDS - HOLD_SECONDS) / FADE_IN_SECONDS
    };
    background.0 = Color::BLACK.with_alpha(alpha.clamp(0.0, 1.0));

    let effects_at = FADE_OUT_SECONDS + HOLD_SECONDS * 0.5;
    if !fade.1 && elapsed_before < effects_at && elapsed >= effects_at {
        if let Ok(bed) = beds.single() {
            match choice.0 {
                SleepAction::Sleep => {
                    sleeping.write(SleepWithoutSaving(bed));
                }
                SleepAction::SleepAndSave => {
                    saving.write(SaveAtBed(bed));
                }
                SleepAction::Cancel => {}
            }
        }
        fade.1 = true;
    }
    if fade.0.is_finished() {
        next.set(GameState::Playing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::{Binding, Control, Keybinds};
    use bevy::ecs::system::RunSystemOnce;

    fn menu_world() -> World {
        let mut world = World::new();
        world.insert_resource(State::new(GameState::SleepMenu));
        world.init_resource::<NextState<GameState>>();
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<Keybinds>();
        world.init_resource::<Selection>();
        world.init_resource::<Hover>();
        world.init_resource::<ChosenAction>();
        world
    }

    #[test]
    fn selection_wraps_and_starts_on_sleep_and_save() {
        assert_eq!(Selection::default().0, SleepAction::SleepAndSave);
        assert_eq!(adjacent(SleepAction::Sleep, -1), SleepAction::Cancel);
        assert_eq!(adjacent(SleepAction::Cancel, 1), SleepAction::Sleep);
    }

    #[test]
    fn menu_uses_rebound_direction_and_space_does_not_activate() {
        let mut world = menu_world();
        world.resource_mut::<Keybinds>().apply_config(
            Control::MenuDown,
            Binding::Key(KeyCode::KeyK),
            None,
        );
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyK);
        world
            .run_system_once(navigate)
            .expect("navigate rebound menu");
        assert_eq!(world.resource::<Selection>().0, SleepAction::Cancel);

        world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        world
            .run_system_once(navigate)
            .expect("ignore jump binding for menu activation");
        assert!(matches!(
            *world.resource::<NextState<GameState>>(),
            NextState::Unchanged
        ));
    }

    #[test]
    fn escape_cancels_the_sleep_menu() {
        let mut world = menu_world();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        world
            .run_system_once(navigate)
            .expect("cancel sleep menu with Escape");
        assert!(matches!(
            *world.resource::<NextState<GameState>>(),
            NextState::Pending(GameState::Playing)
        ));
    }

    #[test]
    fn hover_selects_button_without_overriding_later_keyboard_selection() {
        let mut world = menu_world();
        let button = world
            .spawn((SleepButton(SleepAction::Sleep), PickingInteraction::Hovered))
            .id();
        world.run_system_once(select_hovered_button).expect("hover");
        assert_eq!(world.resource::<Selection>().0, SleepAction::Sleep);

        world.resource_mut::<Selection>().0 = SleepAction::Cancel;
        world
            .run_system_once(select_hovered_button)
            .expect("stationary hover");
        assert_eq!(world.resource::<Selection>().0, SleepAction::Cancel);

        *world
            .get_mut::<PickingInteraction>(button)
            .expect("interaction") = PickingInteraction::None;
        world.run_system_once(select_hovered_button).expect("leave");
        *world
            .get_mut::<PickingInteraction>(button)
            .expect("interaction") = PickingInteraction::Hovered;
        world
            .run_system_once(select_hovered_button)
            .expect("hover again");
        assert_eq!(world.resource::<Selection>().0, SleepAction::Sleep);
    }
}
