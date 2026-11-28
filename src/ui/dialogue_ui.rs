use crate::{
    GameState,
    controls::{Control, Keybinds},
    dialogue::{
        Choice, Dialogue, DialogueCatalog, DialogueClick, DialogueContext, DialogueRegistry, Page,
        Screen,
    },
    interaction::InteractionSystems,
    inventory::Inventory,
    nemi::react_to_player_hit,
    player::PlayerRead,
    quest::QuestJournal,
    ui::{
        menu_ui::{SELECTED, button, modal_overlay, scroll_hovered, scrollable_pane},
        text_style,
    },
};
use bevy::{input::mouse::AccumulatedMouseScroll, prelude::*, ui::RelativeCursorPosition};

pub(crate) struct DialogueUiPlugin;

#[derive(Component)]
struct Panel;

#[derive(Component)]
/// Marks the dialogue content area so mouse-wheel input scrolls it only while hovered.
struct ScrollPane;

#[derive(Component)]
struct ButtonChoice {
    revision: u64,
    choice: Choice,
}

impl Plugin for DialogueUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            render.after(InteractionSystems).after(react_to_player_hit),
        )
        .add_systems(Update, scroll);
    }
}

fn render(
    mut commands: Commands,
    state: Res<State<GameState>>,
    dialogue: Res<Dialogue>,
    catalog: Res<DialogueCatalog>,
    handlers: Res<DialogueRegistry>,
    journal: Res<QuestJournal>,
    inventory: PlayerRead<Inventory>,
    bindings: Res<Keybinds>,
    panels: Query<Entity, With<Panel>>,
) {
    if !(state.is_changed()
        || dialogue.is_changed()
        || journal.is_changed()
        || inventory.is_changed()
        || bindings.is_changed())
    {
        return;
    }
    for panel in &panels {
        commands.entity(panel).despawn();
    }
    if *state.get() != GameState::Playing {
        return;
    }
    let Some(screen) = dialogue.screen() else {
        return;
    };
    commands
        .spawn((Panel, modal_overlay()))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: percent(90),
                        max_width: px(680),
                        max_height: percent(80),
                        padding: UiRect::all(px(16)),
                        ..crate::ui::menu_ui::column()
                    },
                    BackgroundColor(crate::ui::menu_ui::PANEL),
                ))
                .with_children(|panel| {
                    if let Screen::Conversation { profile, .. } = screen
                        && let Some(profile) = catalog.profile(profile)
                    {
                        panel.spawn((Text::new(&profile.name), text_style(25.0)));
                    }
                    panel
                        .spawn((ScrollPane, scrollable_pane(percent(100))))
                        .with_children(|pane| {
                            if let Some(text) = dialogue.text() {
                                pane.spawn((Text::new(text), text_style(20.0)));
                            } else if let Screen::Conversation {
                                profile,
                                page: Page::Topics,
                                ..
                            } = screen
                                && let Some(profile) = catalog.profile(profile)
                            {
                                let context = DialogueContext {
                                    journal: &*journal,
                                    inventory: &**inventory,
                                };
                                for topic in profile.available_topics(&handlers, &context) {
                                    spawn_button(
                                        pane,
                                        &topic.label,
                                        Choice::Topic(topic.id.clone()),
                                        dialogue.revision(),
                                        dialogue.selected_topic() == Some(topic.id.as_str()),
                                    );
                                }
                                spawn_button(
                                    pane,
                                    "Goodbye",
                                    Choice::Goodbye,
                                    dialogue.revision(),
                                    dialogue.selected_topic().is_none(),
                                );
                            }
                        });
                    if dialogue.text().is_some() {
                        let key = bindings
                            .get(Control::Interact)
                            .map_or_else(|| "None".into(), |binding| binding.label());
                        spawn_button(
                            panel,
                            &format!("Continue [{key} / Enter]"),
                            Choice::Continue,
                            dialogue.revision(),
                            false,
                        );
                    } else {
                        let up = bindings
                            .get(Control::MenuUp)
                            .map_or_else(|| "None".into(), |binding| binding.label());
                        let down = bindings
                            .get(Control::MenuDown)
                            .map_or_else(|| "None".into(), |binding| binding.label());
                        panel.spawn((
                            Text::new(format!(
                                "[{up} / {down}] Move   [Enter] Select   [Esc] Leave"
                            )),
                            text_style(15.0),
                        ));
                    }
                });
        });
}

fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    choice: Choice,
    revision: u64,
    selected: bool,
) {
    let mut entity = parent.spawn((ButtonChoice { revision, choice }, button()));
    if selected {
        entity.insert(BackgroundColor(SELECTED));
    }
    entity
        .with_child((Text::new(label), text_style(19.0), Pickable::IGNORE))
        .observe(choose);
}

fn choose(
    event: On<PointerClick>,
    choices: Query<&ButtonChoice>,
    mut click: ResMut<DialogueClick>,
    state: Res<State<GameState>>,
) {
    if *state.get() == GameState::Playing
        && event.button == PointerButton::Primary
        && click.0.is_none()
        && let Ok(button) = choices.get(event.entity)
    {
        click.0 = Some((button.revision, button.choice.clone()));
    }
}

fn scroll(
    wheel: Res<AccumulatedMouseScroll>,
    mut panes: Query<(&RelativeCursorPosition, &mut ScrollPosition), With<ScrollPane>>,
) {
    if wheel.delta.y != 0.0 {
        for (cursor, mut position) in &mut panes {
            scroll_hovered(wheel.delta.y, cursor, &mut position);
        }
    }
}
