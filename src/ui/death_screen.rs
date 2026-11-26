use super::{menu_ui, text_style, title::TITLE_FONT};
use crate::{GameState, combat::Dead, player::Player};
use bevy::prelude::*;

const FADE_SECONDS: f32 = 1.6;
const HEADING_START_SECONDS: f32 = 0.35;
const HEADING_FULL_SECONDS: f32 = 1.1;

pub(super) struct DeathScreenPlugin;

#[derive(Component)]
struct DeathFade(Timer);

#[derive(Component)]
struct Heading;

#[derive(Component)]
pub(crate) struct ContinueAfterDeath;

impl Plugin for DeathScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::GameOver), spawn)
            .add_systems(Update, fade.run_if(in_state(GameState::GameOver)));
    }
}

pub(crate) fn request_game_over(
    players: Query<(), (With<Player>, Added<Dead>)>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !players.is_empty() {
        next.set(GameState::GameOver);
    }
}

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            DeathFade(Timer::from_seconds(FADE_SECONDS, TimerMode::Once)),
            DespawnOnExit(GameState::GameOver),
            GlobalZIndex(110),
            BackgroundColor(Color::BLACK.with_alpha(0.0)),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                row_gap: px(24),
                ..default()
            },
        ))
        .with_children(|screen| {
            screen.spawn((
                Heading,
                Text::new("Game Over"),
                TextFont {
                    font: TITLE_FONT.into(),
                    font_size: FontSize::VMin(8.0),
                    ..default()
                },
                TextColor(Color::WHITE.with_alpha(0.0)),
                Pickable::IGNORE,
            ));
            screen
                .spawn((ContinueAfterDeath, menu_ui::button(), Visibility::Hidden))
                .observe(continue_run)
                .with_child((Text::new("Continue"), text_style(20.0), Pickable::IGNORE));
        });
}

fn fade(
    time: Res<Time>,
    screen: Single<(&mut DeathFade, &mut BackgroundColor)>,
    mut heading: Single<&mut TextColor, With<Heading>>,
    mut button: Single<&mut Visibility, With<ContinueAfterDeath>>,
) {
    let (mut fade, mut background) = screen.into_inner();
    if fade.0.is_finished() {
        return;
    }
    fade.0.tick(time.delta());
    background.0 = Color::BLACK.with_alpha(fade.0.fraction());
    let heading_alpha = ((fade.0.elapsed_secs() - HEADING_START_SECONDS)
        / (HEADING_FULL_SECONDS - HEADING_START_SECONDS))
        .clamp(0.0, 1.0);
    heading.0 = Color::WHITE.with_alpha(heading_alpha);
    if fade.0.just_finished() {
        **button = Visibility::Inherited;
    }
}

fn continue_run(
    click: On<PointerClick>,
    state: Res<State<GameState>>,
    button: Query<&Visibility, With<ContinueAfterDeath>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if *state.get() == GameState::GameOver
        && click.button == PointerButton::Primary
        && matches!(button.get(click.entity), Ok(Visibility::Inherited))
        && matches!(*next, NextState::Unchanged)
    {
        next.set(GameState::Loading);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::pointer_click;
    use bevy::{state::app::StatesPlugin, time::TimeUpdateStrategy};
    use std::time::Duration;

    #[test]
    fn continue_waits_for_full_black_and_then_loads_on_primary_click() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, DeathScreenPlugin))
            .insert_state(GameState::GameOver)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )));
        app.update();
        let world = app.world_mut();
        let screen = world
            .query_filtered::<Entity, With<DeathFade>>()
            .single(world)
            .expect("game over must have one overlay");
        let heading = world
            .query_filtered::<Entity, With<Heading>>()
            .single(world)
            .expect("game over must have one heading");
        let button = world
            .query_filtered::<Entity, With<ContinueAfterDeath>>()
            .single(world)
            .expect("game over must have one Continue button");
        let window = world.spawn_empty().id();
        let camera = world.spawn_empty().id();

        world.trigger(pointer_click(button, window, camera));
        for _ in 0..7 {
            app.update();
        }
        let world = app.world();
        assert_eq!(
            *world.resource::<State<GameState>>().get(),
            GameState::GameOver
        );
        assert!(matches!(
            world.get::<Visibility>(button),
            Some(Visibility::Hidden)
        ));
        let heading_alpha = world
            .get::<TextColor>(heading)
            .expect("heading color")
            .0
            .alpha();
        assert!(heading_alpha > 0.0 && heading_alpha < 1.0);
        let background_alpha = world
            .get::<BackgroundColor>(screen)
            .expect("overlay color")
            .0
            .alpha();
        assert!(background_alpha > 0.0 && background_alpha < 1.0);

        for _ in 0..20 {
            app.update();
        }
        let world = app.world_mut();
        assert_eq!(
            *world.resource::<State<GameState>>().get(),
            GameState::GameOver
        );
        assert_eq!(
            world
                .get::<BackgroundColor>(screen)
                .expect("opaque overlay")
                .0,
            Color::BLACK
        );
        assert!(matches!(
            world.get::<Visibility>(button),
            Some(Visibility::Inherited)
        ));
        world.trigger(PointerClick {
            button: PointerButton::Secondary,
            ..pointer_click(button, window, camera)
        });
        assert!(matches!(
            *world.resource::<NextState<GameState>>(),
            NextState::Unchanged
        ));
        world.trigger(pointer_click(button, window, camera));
        app.update();
        let world = app.world();
        assert_eq!(
            *world.resource::<State<GameState>>().get(),
            GameState::Loading
        );
        assert!(world.get_entity(screen).is_err());
        assert!(world.get_entity(button).is_err());
    }
}
