use crate::{GameState, world::WorldLayout};
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub struct QuestPlugin;

pub(crate) const LAST_LIGHT: &str = "last_light";
pub(crate) const OLD_RELAY: &str = "old_relay";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum QuestAction {
    TalkedToNemi,
    RecoveredCore,
    ReturnedCore,
    InspectedSealedHatch,
    EnteredRelay,
    OpenedArchiveGate,
    ReadSurveyLog,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
enum Condition {
    SessionStart,
    Action(QuestAction),
    QuestCompleted(String),
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Objective {
    pub(crate) title: String,
    pub(crate) directions: String,
    advance_on: Condition,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestDefinition {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) completion: String,
    #[serde(default)]
    outpost_only: bool,
    starts_when: Vec<Condition>,
    pub(crate) objectives: Vec<Objective>,
}

#[derive(Asset, TypePath, Clone, Default, Deserialize)]
#[serde(transparent)]
pub(crate) struct QuestCatalog(Vec<QuestDefinition>);

impl QuestCatalog {
    fn validate(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for quest in &self.0 {
            if quest.id.trim().is_empty() || !ids.insert(quest.id.as_str()) {
                return Err(format!("Missing or duplicate quest ID: {:?}", quest.id));
            }
            if quest.objectives.is_empty() || quest.starts_when.is_empty() {
                return Err(format!(
                    "Quest {} needs an activation rule and objectives",
                    quest.id
                ));
            }
        }
        for required in [LAST_LIGHT, OLD_RELAY] {
            if !ids.contains(required) {
                return Err(format!("The quest catalog is missing {required}"));
            }
        }
        for quest in &self.0 {
            for objective in &quest.objectives {
                if objective.advance_on == Condition::SessionStart {
                    return Err(format!(
                        "Quest {} uses SessionStart as an objective",
                        quest.id
                    ));
                }
            }
            for condition in quest.starts_when.iter().chain(
                quest
                    .objectives
                    .iter()
                    .map(|objective| &objective.advance_on),
            ) {
                if let Condition::QuestCompleted(id) = condition
                    && (!ids.contains(id.as_str()) || *id == quest.id)
                {
                    return Err(format!(
                        "Quest {} has an invalid quest reference: {id}",
                        quest.id
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Default, TypePath)]
struct QuestCatalogLoader;

impl AssetLoader for QuestCatalogLoader {
    type Asset = QuestCatalog;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<QuestCatalog, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let catalog: QuestCatalog = ron::de::from_bytes(&bytes).map_err(std::io::Error::other)?;
        catalog.validate().map_err(std::io::Error::other)?;
        Ok(catalog)
    }

    fn extensions(&self) -> &[&str] {
        &["quest.ron"]
    }
}

#[derive(Resource, Default)]
pub(crate) struct QuestJournal {
    catalog: QuestCatalog,
    layout: WorldLayout,
    progress: BTreeMap<String, usize>,
}

impl QuestJournal {
    pub(crate) fn new(catalog: QuestCatalog, layout: WorldLayout) -> Self {
        let mut journal = Self {
            catalog,
            layout,
            progress: BTreeMap::new(),
        };
        journal.settle(Some(Condition::SessionStart));
        journal
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&QuestDefinition, usize)> {
        self.catalog
            .0
            .iter()
            .filter_map(|quest| self.progress.get(&quest.id).map(|&step| (quest, step)))
    }

    pub(crate) fn is_complete(&self, id: &str) -> bool {
        self.entries()
            .any(|(quest, step)| quest.id == id && step == quest.objectives.len())
    }

    pub(crate) fn expects(&self, action: QuestAction) -> bool {
        self.entries().any(|(quest, step)| {
            quest
                .objectives
                .get(step)
                .is_some_and(|objective| objective.advance_on == Condition::Action(action))
        })
    }

    pub(crate) fn record(&mut self, action: QuestAction) {
        let condition = Condition::Action(action);
        for quest in &self.catalog.0 {
            if let Some(step) = self.progress.get_mut(&quest.id)
                && quest
                    .objectives
                    .get(*step)
                    .is_some_and(|objective| objective.advance_on == condition)
            {
                *step += 1;
            }
        }
        self.settle(Some(condition));
    }

    // Completion conditions persist. Revisit them until chains have settled,
    // regardless of the order definitions are written in the catalog.
    fn settle(&mut self, trigger: Option<Condition>) {
        loop {
            let mut changes = Vec::new();
            let completed = |condition: &Condition| matches!(condition, Condition::QuestCompleted(id) if self.is_complete(id));
            for quest in &self.catalog.0 {
                if quest.outpost_only && self.layout != WorldLayout::Outpost {
                    continue;
                }
                match self.progress.get(&quest.id) {
                    None if quest
                        .starts_when
                        .iter()
                        .any(|rule| Some(rule) == trigger.as_ref() || completed(rule)) =>
                    {
                        changes.push((quest.id.clone(), 0));
                    }
                    Some(&step)
                        if quest
                            .objectives
                            .get(step)
                            .is_some_and(|objective| completed(&objective.advance_on)) =>
                    {
                        changes.push((quest.id.clone(), step + 1));
                    }
                    _ => {}
                }
            }
            if changes.is_empty() {
                break;
            }
            self.progress.extend(changes);
        }
    }
}

#[cfg(test)]
pub(crate) fn test_journal() -> QuestJournal {
    let catalog = ron::from_str(include_str!("../data/quests/catalog.quest.ron"))
        .expect("shipped quest catalog");
    QuestJournal::new(catalog, WorldLayout::Outpost)
}

#[derive(Resource, Default)]
pub struct Dialogue {
    pub text: Option<&'static str>,
}
#[derive(Component)]
pub struct Beacon;
#[derive(Component)]
struct BeaconLens;
#[derive(Event)]
pub struct SpawnBeaconLens(pub Vec3);
impl Plugin for QuestPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<QuestCatalog>()
            .init_asset_loader::<QuestCatalogLoader>()
            .init_resource::<QuestJournal>()
            .init_resource::<Dialogue>()
            .add_observer(spawn_lens)
            .add_systems(
                Update,
                effects
                    .run_if(in_state(GameState::Playing))
                    .after(crate::interaction::InteractionSystems),
            );
    }
}

fn spawn_lens(
    event: On<SpawnBeaconLens>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        BeaconLens,
        Beacon,
        Mesh3d(meshes.add(Sphere::new(0.65))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.04, 0.1, 0.13),
            ..default()
        })),
        Transform::from_translation(event.0),
    ));
}
fn effects(
    quest: Res<QuestJournal>,
    mut beacons: Query<&mut PointLight, With<Beacon>>,
    lens: Query<&MeshMaterial3d<StandardMaterial>, With<BeaconLens>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !quest.is_changed() || !quest.is_complete(LAST_LIGHT) {
        return;
    }
    for mut light in &mut beacons {
        light.intensity = 450_000.0;
    }
    for handle in &lens {
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.emissive = LinearRgba::rgb(3.0, 15.0, 20.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_validates_the_shipped_definitions_and_rejects_bad_references() {
        let mut catalog = test_journal().catalog;
        catalog.validate().expect("valid shipped catalog");
        catalog.0[1]
            .starts_when
            .push(Condition::QuestCompleted("missing".into()));
        assert!(
            catalog
                .validate()
                .expect_err("unknown ID")
                .contains("missing")
        );
    }

    #[test]
    fn catalog_rejects_duplicate_ids_and_empty_objectives() {
        let mut catalog = test_journal().catalog;
        catalog.0.push(catalog.0[0].clone());
        assert!(
            catalog
                .validate()
                .expect_err("duplicate ID")
                .contains("duplicate")
        );
        catalog.0.pop();
        catalog.0[0].objectives.clear();
        assert!(
            catalog
                .validate()
                .expect_err("empty objectives")
                .contains("objectives")
        );
    }

    #[test]
    fn early_discovery_runs_alongside_last_light_and_progresses_in_order() {
        let mut journal = test_journal();
        journal.record(QuestAction::RecoveredCore);
        assert!(journal.expects(QuestAction::TalkedToNemi));
        journal.record(QuestAction::InspectedSealedHatch);
        journal.record(QuestAction::InspectedSealedHatch);
        journal.record(QuestAction::EnteredRelay);
        assert_eq!(
            journal.entries().map(|(_, step)| step).collect::<Vec<_>>(),
            [0, 0]
        );
        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::TalkedToNemi);
        assert_eq!(
            journal.entries().map(|(_, step)| step).collect::<Vec<_>>(),
            [1, 0]
        );
        journal.record(QuestAction::RecoveredCore);
        journal.record(QuestAction::RecoveredCore);
        assert!(!journal.is_complete(LAST_LIGHT));
        journal.record(QuestAction::ReturnedCore);
        assert!(journal.is_complete(LAST_LIGHT));
        assert!(journal.expects(QuestAction::EnteredRelay));
        journal.record(QuestAction::EnteredRelay);
        journal.record(QuestAction::ReadSurveyLog);
        assert!(!journal.is_complete(OLD_RELAY));
        journal.record(QuestAction::OpenedArchiveGate);
        journal.record(QuestAction::ReadSurveyLog);
        journal.record(QuestAction::ReadSurveyLog);
        assert!(journal.is_complete(OLD_RELAY));
        assert_eq!(journal.entries().count(), 2);
    }

    #[test]
    fn beacon_completion_introduces_an_undiscovered_relay() {
        let mut journal = test_journal();
        assert_eq!(journal.entries().count(), 1);
        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::RecoveredCore);
        journal.record(QuestAction::ReturnedCore);
        assert_eq!(journal.entries().count(), 2);
        assert!(journal.expects(QuestAction::EnteredRelay));
    }
}
