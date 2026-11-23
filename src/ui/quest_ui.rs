use crate::{
    experience::ExperienceUpdates,
    inventory::{Inventory, ItemKind},
    quest::{
        CAVERN_HUSHCAP_GOAL, CONTAIN_THE_HUSHCAPS, MYCELIUM_GOAL, QuestJournal, QuestOutcome,
        QuestStatus,
    },
    ui::menu_ui::{BUTTON, MenuTab, PlayerMenu, SELECTED, button, scroll_pane},
    ui::text_style,
};
use bevy::prelude::*;

pub(crate) struct QuestUiPlugin;

#[derive(Resource, Default)]
pub(crate) struct QuestSelection(Option<String>);

impl QuestSelection {
    pub(crate) fn selected(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

#[derive(Component)]
struct QuestList;
#[derive(Component)]
struct QuestDetails;
#[derive(Component)]
struct CavernQuestCounts;
#[derive(Component)]
pub(crate) struct QuestRow {
    pub(crate) id: String,
    pub(crate) status: QuestStatus,
}
impl Plugin for QuestUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QuestSelection>().add_systems(
            Update,
            (sync, refresh_cavern_counts)
                .chain()
                .after(ExperienceUpdates)
                .run_if(|menu: Res<PlayerMenu>| menu.is_on(MenuTab::Quest)),
        );
    }
}

pub(crate) fn spawn(page: &mut ChildSpawnerCommands) {
    page.spawn((QuestList, scroll_pane(MenuTab::Quest, percent(32))));
    page.spawn((QuestDetails, scroll_pane(MenuTab::Quest, percent(68))));
}

fn sync(
    mut commands: Commands,
    journal: Res<QuestJournal>,
    mut selection: ResMut<QuestSelection>,
    list: Query<(Entity, Option<&Children>), With<QuestList>>,
    mut details: Query<(Entity, Option<&Children>, &mut ScrollPosition), With<QuestDetails>>,
    mut rows: Query<(&QuestRow, &mut BackgroundColor)>,
    mut previous: Local<Option<String>>,
) {
    if !(selection.is_changed() || journal.is_changed()) {
        return;
    }
    let mut entries: Vec<_> = journal.entries().collect();
    entries.sort_by_key(|(_, progress)| progress.status());
    if !entries
        .iter()
        .any(|(quest, _)| Some(&quest.id) == selection.0.as_ref())
    {
        selection.0 = entries.first().map(|(quest, _)| quest.id.clone());
    }
    if journal.is_changed()
        && let Ok((entity, children)) = list.single()
    {
        clear(&mut commands, children);
        commands.entity(entity).with_children(|list| {
            for (heading, status) in [
                ("Active", QuestStatus::Active),
                ("Completed", QuestStatus::Completed),
                ("Failed", QuestStatus::Failed),
            ] {
                list.spawn((Text::new(heading), text_style(17.0), Pickable::IGNORE));
                let mut any = false;
                for (quest, progress) in &entries {
                    if progress.status() != status {
                        continue;
                    }
                    any = true;
                    let background = row_background(selection.0.as_ref() == Some(&quest.id));
                    list.spawn((
                        button(),
                        QuestRow {
                            id: quest.id.clone(),
                            status,
                        },
                    ))
                    .insert(background)
                    .observe(select)
                    .with_child((
                        Text::new(&quest.title),
                        text_style(17.0),
                        Pickable::IGNORE,
                    ));
                }
                if !any {
                    list.spawn((Text::new("None"), text_style(15.0), Pickable::IGNORE));
                }
            }
        });
    }
    let selection_changed = *previous != selection.0;
    if selection_changed {
        for (row, mut background) in &mut rows {
            background.set_if_neq(row_background(selection.0.as_ref() == Some(&row.id)));
        }
        previous.clone_from(&selection.0);
    }
    if !(selection_changed || journal.is_changed()) {
        return;
    }
    if let Ok((entity, children, mut scroll)) = details.single_mut() {
        clear(&mut commands, children);
        if selection_changed {
            scroll.0 = Vec2::ZERO;
        }
        commands.entity(entity).with_children(|details| {
            if let Some((quest, progress)) = entries
                .iter()
                .find(|(quest, _)| Some(&quest.id) == selection.0.as_ref())
            {
                details.spawn((Text::new(&quest.title), text_style(23.0)));
                details.spawn((Text::new(&quest.summary), text_style(17.0)));
                if !matches!(progress.outcome, QuestOutcome::Failed(_)) {
                    details.spawn((
                        Text::new(format!("Completion reward | {} XP", quest.completion_xp)),
                        text_style(17.0),
                    ));
                }
                match &progress.outcome {
                    QuestOutcome::Active(index) => {
                        let objective = &quest.objectives[*index];
                        details.spawn((
                            Text::new(format!("Current objective\n{}", objective.title)),
                            text_style(19.0),
                        ));
                        if quest.id == CONTAIN_THE_HUSHCAPS {
                            details.spawn((CavernQuestCounts, Text::default(), text_style(17.0)));
                        }
                        details.spawn((Text::new(&objective.directions), text_style(17.0)));
                    }
                    QuestOutcome::Completed => {
                        details.spawn((Text::new(&quest.completion), text_style(17.0)));
                    }
                    QuestOutcome::Failed(reason) => {
                        details.spawn((Text::new("Quest failed"), text_style(19.0)));
                        details.spawn((Text::new(reason), text_style(17.0)));
                    }
                }
                if !progress.completed.is_empty() {
                    details.spawn((Text::new("Completed steps"), text_style(19.0)));
                    for &index in &progress.completed {
                        let objective = &quest.objectives[index];
                        details.spawn((
                            Text::new(format!("• {}", objective.title)),
                            text_style(16.0),
                        ));
                    }
                }
            } else {
                details.spawn((
                    Text::new("No quests in your journal yet."),
                    text_style(17.0),
                ));
            }
        });
    }
}

fn refresh_cavern_counts(
    mut text: Query<&mut Text, With<CavernQuestCounts>>,
    journal: Res<QuestJournal>,
    inventory: Res<Inventory>,
    mut previous: Local<Option<(usize, u32)>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let defeated = journal.cavern_defeat_count();
    let carried = inventory.count(ItemKind::Mycelium).min(MYCELIUM_GOAL);
    let counts = Some((defeated, carried));
    if *previous != counts || text.is_added() {
        text.0 = format!(
            "Hushcaps defeated  {defeated}/{CAVERN_HUSHCAP_GOAL}\nMycelium in backpack  {carried}/{MYCELIUM_GOAL}"
        );
        *previous = counts;
    }
}

fn clear(commands: &mut Commands, children: Option<&Children>) {
    for child in children.into_iter().flatten() {
        commands.entity(*child).despawn();
    }
}

fn row_background(selected: bool) -> BackgroundColor {
    BackgroundColor(if selected { SELECTED } else { BUTTON })
}

fn select(
    click: On<PointerClick>,
    rows: Query<&QuestRow>,
    menu: Res<PlayerMenu>,
    mut selection: ResMut<QuestSelection>,
) {
    if menu.accepts(MenuTab::Quest)
        && click.button == PointerButton::Primary
        && let Ok(row) = rows.get(click.entity)
    {
        selection.0 = Some(row.id.clone());
    }
}
