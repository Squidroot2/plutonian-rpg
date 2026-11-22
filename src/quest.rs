use crate::{
    GameState,
    world::{SessionEntity, WorldLayout},
};
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub struct QuestPlugin;

pub(crate) const LAST_LIGHT: &str = "last_light";
pub(crate) const OLD_RELAY: &str = "old_relay";
pub(crate) const RELIGHT_BEACON: &str = "relight_beacon";

#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    bitcode::Encode,
    bitcode::Decode,
)]
pub(crate) enum QuestAction {
    TalkedToNemi,
    RecoveredCore,
    ReturnedCore,
    AttackedNemi,
    InstalledCore,
    InspectedSealedHatch,
    EnteredRelay,
    OpenedArchiveGate,
    ReadSurveyLog,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
enum Condition {
    SessionStart,
    Action(QuestAction),
    Occurred(QuestAction),
    QuestCompleted(String),
    QuestFailed(String),
}

impl Condition {
    fn is_persistent(&self) -> bool {
        matches!(
            self,
            Self::Occurred(_) | Self::QuestCompleted(_) | Self::QuestFailed(_)
        )
    }
}

#[derive(Clone, Deserialize)]
enum Destination {
    Objective(String),
    Complete,
    Fail(String),
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transition {
    when: Condition,
    to: Destination,
    #[serde(default = "credit_objective")]
    credit: bool,
    #[serde(default)]
    credit_early: bool,
}

fn credit_objective() -> bool {
    true
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Interrupt {
    on: QuestAction,
    reason: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Objective {
    id: String,
    pub(crate) title: String,
    pub(crate) directions: String,
    transitions: Vec<Transition>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuestDefinition {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) completion: String,
    pub(crate) completion_xp: u32,
    #[serde(default)]
    outpost_only: bool,
    starts_when: Vec<Condition>,
    first_objective: String,
    #[serde(default)]
    interrupts: Vec<Interrupt>,
    pub(crate) objectives: Vec<Objective>,
}

impl QuestDefinition {
    fn objective_index(&self, id: &str) -> Option<usize> {
        self.objectives
            .iter()
            .position(|objective| objective.id == id)
    }

    fn validate(&self, ids: &BTreeSet<&str>) -> Result<(), String> {
        if self.completion_xp == 0 {
            return Err(format!("Quest {} needs positive completion_xp", self.id));
        }
        if self.starts_when.is_empty() || self.objectives.is_empty() {
            return Err(format!(
                "Quest {} needs an activation rule and objectives",
                self.id
            ));
        }
        let mut objectives = BTreeSet::new();
        for objective in &self.objectives {
            if objective.id.trim().is_empty() || !objectives.insert(objective.id.as_str()) {
                return Err(format!(
                    "Quest {} has a blank or duplicate objective ID: {}",
                    self.id, objective.id
                ));
            }
            if objective.transitions.is_empty() {
                return Err(format!(
                    "Quest {} objective {} needs a transition",
                    self.id, objective.id
                ));
            }
        }
        let validate_target = |id: &str| {
            if objectives.contains(id) {
                Ok(())
            } else {
                Err(format!("Quest {} has an unknown objective: {id}", self.id))
            }
        };
        validate_target(&self.first_objective)?;
        for transition in self
            .objectives
            .iter()
            .flat_map(|objective| &objective.transitions)
        {
            if let Destination::Objective(id) = &transition.to {
                validate_target(id)?;
            }
            if transition.when == Condition::SessionStart {
                return Err(format!(
                    "Quest {} uses SessionStart as a transition",
                    self.id
                ));
            }
            if transition.credit_early
                && (!matches!(transition.when, Condition::Occurred(_))
                    || !transition.credit
                    || matches!(transition.to, Destination::Fail(_)))
            {
                return Err(format!(
                    "Quest {} uses credit_early on a transition that does not credit an Occurred condition",
                    self.id
                ));
            }
        }
        for condition in self
            .starts_when
            .iter()
            .chain(self.objectives.iter().flat_map(|objective| {
                objective
                    .transitions
                    .iter()
                    .map(|transition| &transition.when)
            }))
        {
            if let Condition::QuestCompleted(id) | Condition::QuestFailed(id) = condition
                && (!ids.contains(id.as_str()) || *id == self.id)
            {
                return Err(format!(
                    "Quest {} has an invalid quest reference: {id}",
                    self.id
                ));
            }
        }

        // Persistent transitions are evaluated repeatedly, so each must eventually
        // stop or reach an objective that requires a fresh action.
        let mut unverified: BTreeSet<_> = self
            .objectives
            .iter()
            .map(|objective| objective.id.as_str())
            .collect();
        while !unverified.is_empty() {
            let before = unverified.len();
            let ready: Vec<_> = self.objectives.iter().filter(|objective| {
                unverified.contains(objective.id.as_str()) &&
                    objective.transitions.iter().filter(|transition| transition.when.is_persistent())
                    .all(|transition| !matches!(&transition.to, Destination::Objective(id) if unverified.contains(id.as_str())))
            }).map(|objective| objective.id.as_str()).collect();
            for id in ready {
                unverified.remove(id);
            }
            if unverified.len() == before {
                return Err(format!(
                    "Quest {} has a cycle of persistent transitions",
                    self.id
                ));
            }
        }
        Ok(())
    }
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
        }
        for required in [LAST_LIGHT, OLD_RELAY, RELIGHT_BEACON] {
            if !ids.contains(required) {
                return Err(format!("The quest catalog is missing {required}"));
            }
        }
        for quest in &self.0 {
            quest.validate(&ids)?;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QuestOutcome {
    Active(usize),
    Completed,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum QuestStatus {
    Active,
    Completed,
    Failed,
}

pub(crate) struct QuestProgress {
    pub(crate) outcome: QuestOutcome,
    pub(crate) completed: Vec<usize>,
}

impl QuestProgress {
    pub(crate) fn status(&self) -> QuestStatus {
        match self.outcome {
            QuestOutcome::Active(_) => QuestStatus::Active,
            QuestOutcome::Completed => QuestStatus::Completed,
            QuestOutcome::Failed(_) => QuestStatus::Failed,
        }
    }

    /// Apply a transition from an active objective and return any completion XP.
    fn advance(&mut self, quest: &QuestDefinition, transition: &Transition) -> u32 {
        if let QuestOutcome::Active(current) = self.outcome
            && transition.credit
            && !matches!(transition.to, Destination::Fail(_))
        {
            self.credit(current);
        }
        self.outcome = match &transition.to {
            Destination::Objective(id) => QuestOutcome::Active(
                quest
                    .objective_index(id)
                    .expect("validated transition target"),
            ),
            Destination::Complete => QuestOutcome::Completed,
            Destination::Fail(reason) => QuestOutcome::Failed(reason.clone()),
        };
        if matches!(self.outcome, QuestOutcome::Completed) {
            quest.completion_xp
        } else {
            0
        }
    }

    fn credit(&mut self, objective: usize) {
        if !self.completed.contains(&objective) {
            self.completed.push(objective);
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct QuestJournal {
    catalog: QuestCatalog,
    layout: WorldLayout,
    progress: BTreeMap<String, QuestProgress>,
    occurred: BTreeSet<QuestAction>,
    pending_completion_xp: u64,
}

#[derive(bitcode::Encode, bitcode::Decode)]
pub(crate) struct SavedJournal {
    progress: BTreeMap<String, SavedQuest>,
    occurred: BTreeSet<QuestAction>,
}

#[derive(bitcode::Encode, bitcode::Decode)]
struct SavedQuest {
    outcome: SavedOutcome,
    completed: Vec<String>,
}

#[derive(bitcode::Encode, bitcode::Decode)]
enum SavedOutcome {
    Active(String),
    Completed,
    Failed(String),
}

impl QuestJournal {
    pub(crate) fn new(catalog: QuestCatalog, layout: WorldLayout) -> Self {
        let mut journal = Self {
            catalog,
            layout,
            ..default()
        };
        journal.settle(Some(Condition::SessionStart));
        journal
    }

    pub(crate) fn snapshot(&self) -> SavedJournal {
        SavedJournal {
            progress: self
                .entries()
                .map(|(quest, progress)| {
                    let objective = |index: usize| quest.objectives[index].id.clone();
                    let outcome = match &progress.outcome {
                        QuestOutcome::Active(index) => SavedOutcome::Active(objective(*index)),
                        QuestOutcome::Completed => SavedOutcome::Completed,
                        QuestOutcome::Failed(reason) => SavedOutcome::Failed(reason.clone()),
                    };
                    let completed = progress.completed.iter().copied().map(objective).collect();
                    (quest.id.clone(), SavedQuest { outcome, completed })
                })
                .collect(),
            occurred: self.occurred.clone(),
        }
    }

    pub(crate) fn from_snapshot(
        catalog: QuestCatalog,
        layout: WorldLayout,
        snapshot: &SavedJournal,
    ) -> Result<Self, String> {
        let mut progress = BTreeMap::new();
        for (id, saved) in &snapshot.progress {
            let quest = catalog
                .0
                .iter()
                .find(|quest| quest.id == *id)
                .ok_or_else(|| format!("Unknown saved quest {id}"))?;
            let objective = |name: &str| {
                quest
                    .objective_index(name)
                    .ok_or_else(|| format!("Unknown saved objective {name} in {id}"))
            };
            let outcome = match &saved.outcome {
                SavedOutcome::Active(name) => QuestOutcome::Active(objective(name)?),
                SavedOutcome::Completed => QuestOutcome::Completed,
                SavedOutcome::Failed(reason) => QuestOutcome::Failed(reason.clone()),
            };
            let completed = saved
                .completed
                .iter()
                .map(|name| objective(name))
                .collect::<Result<_, _>>()?;
            progress.insert(id.clone(), QuestProgress { outcome, completed });
        }
        Ok(Self {
            catalog,
            layout,
            progress,
            occurred: snapshot.occurred.clone(),
            pending_completion_xp: 0,
        })
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&QuestDefinition, &QuestProgress)> {
        self.catalog.0.iter().filter_map(|quest| {
            self.progress
                .get(&quest.id)
                .map(|progress| (quest, progress))
        })
    }

    pub(crate) fn status(&self, id: &str) -> Option<QuestStatus> {
        self.progress.get(id).map(QuestProgress::status)
    }

    pub(crate) fn is_complete(&self, id: &str) -> bool {
        self.status(id) == Some(QuestStatus::Completed)
    }

    pub(crate) fn beacon_powered(&self) -> bool {
        self.is_complete(LAST_LIGHT) || self.is_complete(RELIGHT_BEACON)
    }

    pub(crate) fn expects(&self, action: QuestAction) -> bool {
        self.entries().any(|(quest, progress)| {
            matches!(progress.outcome, QuestOutcome::Active(index) if quest.objectives[index]
                .transitions.iter().any(|transition| transition.when == Condition::Action(action)))
        })
    }

    pub(crate) fn has_occurred(&self, action: QuestAction) -> bool {
        self.occurred.contains(&action)
    }

    /// Drain XP from new completions. Checkpoints store the resulting player XP.
    pub(crate) fn take_completion_xp(&mut self) -> u64 {
        std::mem::take(&mut self.pending_completion_xp)
    }

    pub(crate) fn record(&mut self, action: QuestAction) {
        self.occurred.insert(action);
        let condition = Condition::Action(action);
        for quest in &self.catalog.0 {
            let Some(progress) = self.progress.get_mut(&quest.id) else {
                continue;
            };
            let QuestOutcome::Active(index) = progress.outcome else {
                continue;
            };
            if let Some(interrupt) = quest
                .interrupts
                .iter()
                .find(|interrupt| interrupt.on == action)
            {
                progress.outcome = QuestOutcome::Failed(interrupt.reason.clone());
            } else if let Some(transition) = quest.objectives[index]
                .transitions
                .iter()
                .find(|transition| transition.when == condition)
            {
                self.pending_completion_xp += u64::from(progress.advance(quest, transition));
            }
        }
        self.settle(Some(condition));
    }

    fn persistent(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Occurred(action) => self.has_occurred(*action),
            Condition::QuestCompleted(id) => self.is_complete(id),
            Condition::QuestFailed(id) => self.status(id) == Some(QuestStatus::Failed),
            _ => false,
        }
    }

    // Resolve persistent conditions against a snapshot each pass so a catalog's
    // file order does not change which edge wins.
    fn settle(&mut self, trigger: Option<Condition>) {
        loop {
            let mut starts = Vec::new();
            let mut credits = Vec::new();
            let mut advances = Vec::new();
            for quest in &self.catalog.0 {
                if quest.outpost_only && self.layout != WorldLayout::Outpost {
                    continue;
                }
                match self.progress.get(&quest.id) {
                    None if quest
                        .starts_when
                        .iter()
                        .any(|rule| Some(rule) == trigger.as_ref() || self.persistent(rule)) =>
                    {
                        starts.push(quest)
                    }
                    Some(progress) => {
                        let QuestOutcome::Active(index) = progress.outcome else {
                            continue;
                        };
                        for (objective_index, objective) in quest.objectives.iter().enumerate() {
                            if !progress.completed.contains(&objective_index)
                                && objective.transitions.iter().any(|transition| {
                                    transition.credit_early && self.persistent(&transition.when)
                                })
                            {
                                credits.push((&quest.id, objective_index));
                            }
                        }
                        if let Some(transition) = quest.objectives[index]
                            .transitions
                            .iter()
                            .find(|transition| self.persistent(&transition.when))
                        {
                            advances.push((quest, transition));
                        }
                    }
                    _ => {}
                }
            }
            for (id, objective) in credits {
                self.progress
                    .get_mut(id)
                    .expect("active quest")
                    .credit(objective);
            }
            if starts.is_empty() && advances.is_empty() {
                break;
            }
            for quest in starts {
                self.progress.insert(
                    quest.id.clone(),
                    QuestProgress {
                        outcome: QuestOutcome::Active(
                            quest
                                .objective_index(&quest.first_objective)
                                .expect("validated first objective"),
                        ),
                        completed: Vec::new(),
                    },
                );
            }
            for (quest, transition) in advances {
                let earned = self
                    .progress
                    .get_mut(&quest.id)
                    .expect("active quest")
                    .advance(quest, transition);
                self.pending_completion_xp += u64::from(earned);
            }
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
                OnTransition {
                    exited: GameState::Loading,
                    entered: GameState::Playing,
                },
                effects,
            )
            .add_systems(
                Update,
                effects
                    .run_if(in_state(GameState::Playing))
                    .run_if(resource_changed::<QuestJournal>)
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
        SessionEntity,
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
    let powered = quest.beacon_powered();
    for mut light in &mut beacons {
        light.intensity = if powered { 450_000.0 } else { 0.0 };
    }
    for handle in &lens {
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.emissive = if powered {
                LinearRgba::rgb(3.0, 15.0, 20.0)
            } else {
                LinearRgba::BLACK
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_snapshot_survives_catalog_reordering_and_rejects_missing_objectives() {
        let mut journal = test_journal();
        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::RecoveredCore);
        let mut saved = journal.snapshot();
        let mut catalog = journal.catalog;
        catalog.0.reverse();
        for quest in &mut catalog.0 {
            quest.objectives.reverse();
        }
        let mut restored =
            QuestJournal::from_snapshot(catalog.clone(), WorldLayout::Outpost, &saved)
                .expect("quest IDs and objective IDs are independent of order");
        assert!(restored.expects(QuestAction::ReturnedCore));
        restored.record(QuestAction::ReturnedCore);
        assert!(restored.beacon_powered());

        saved
            .progress
            .get_mut(LAST_LIGHT)
            .expect("Last Light saved")
            .outcome = SavedOutcome::Active("removed_objective".into());
        assert!(QuestJournal::from_snapshot(catalog, WorldLayout::Outpost, &saved).is_err());
    }

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
    fn quest_catalog_rejects_zero_completion_xp() {
        let mut catalog = test_journal().catalog;
        catalog.0[0].completion_xp = 0;
        assert!(
            catalog
                .validate()
                .expect_err("quests must have a positive XP reward")
                .contains("completion_xp")
        );
    }

    #[test]
    fn only_new_completions_award_xp_after_partial_progress_failure_or_loading() {
        let mut journal = test_journal();
        journal.record(QuestAction::ReturnedCore);
        journal.record(QuestAction::RecoveredCore);
        assert_eq!(journal.take_completion_xp(), 0);
        journal.record(QuestAction::AttackedNemi);
        assert_eq!(journal.status(LAST_LIGHT), Some(QuestStatus::Failed));
        assert_eq!(journal.take_completion_xp(), 0);

        let active_checkpoint = journal.snapshot();
        journal.record(QuestAction::InstalledCore);
        journal.record(QuestAction::InstalledCore);
        assert_eq!(journal.take_completion_xp(), 70);
        assert_eq!(journal.take_completion_xp(), 0);

        let completed_checkpoint = journal.snapshot();
        let mut restored = QuestJournal::from_snapshot(
            journal.catalog.clone(),
            WorldLayout::Outpost,
            &completed_checkpoint,
        )
        .expect("completed quest checkpoint");
        assert!(restored.is_complete(RELIGHT_BEACON));
        restored.record(QuestAction::InstalledCore);
        assert_eq!(restored.take_completion_xp(), 0);

        let mut restored =
            QuestJournal::from_snapshot(journal.catalog, WorldLayout::Outpost, &active_checkpoint)
                .expect("active quest checkpoint");
        assert_eq!(restored.take_completion_xp(), 0);
        restored.record(QuestAction::InstalledCore);
        assert_eq!(restored.take_completion_xp(), 70);
    }

    #[test]
    fn a_completion_that_settles_a_dependent_quest_awards_both_rewards() {
        let mut catalog = test_journal().catalog;
        let relay = catalog
            .0
            .iter_mut()
            .find(|quest| quest.id == OLD_RELAY)
            .expect("Old Relay");
        let transition = &mut relay.objectives[0].transitions[0];
        transition.to = Destination::Complete;
        transition.credit = false;
        catalog
            .validate()
            .expect("completion based on a quest dependency is valid");
        let mut journal = QuestJournal::new(catalog, WorldLayout::Outpost);
        for action in [
            QuestAction::TalkedToNemi,
            QuestAction::RecoveredCore,
            QuestAction::ReturnedCore,
            QuestAction::ReturnedCore,
        ] {
            journal.record(action);
        }
        assert!(journal.is_complete(LAST_LIGHT));
        assert!(journal.is_complete(OLD_RELAY));
        assert_eq!(journal.take_completion_xp(), 190);
        assert_eq!(journal.take_completion_xp(), 0);
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
    fn early_core_recovery_credits_the_objective_without_skipping_the_meeting() {
        let mut journal = test_journal();
        journal.record(QuestAction::ReturnedCore);
        journal.record(QuestAction::RecoveredCore);
        journal.record(QuestAction::RecoveredCore);
        assert!(journal.expects(QuestAction::TalkedToNemi));
        assert!(!journal.expects(QuestAction::ReturnedCore));
        assert!(!journal.beacon_powered());
        assert_eq!(journal.progress[LAST_LIGHT].completed, [1]);

        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::TalkedToNemi);
        assert!(journal.expects(QuestAction::ReturnedCore));
        assert!(!journal.beacon_powered());
        assert_eq!(journal.progress[LAST_LIGHT].completed, [1, 0]);

        journal.record(QuestAction::ReturnedCore);
        journal.record(QuestAction::ReturnedCore);
        assert!(journal.beacon_powered());
        assert_eq!(journal.progress[LAST_LIGHT].completed, [1, 0, 2]);
    }

    #[test]
    fn early_hatch_discovery_runs_alongside_last_light_and_keeps_relay_steps_in_order() {
        let mut journal = test_journal();
        journal.record(QuestAction::InspectedSealedHatch);
        journal.record(QuestAction::InspectedSealedHatch);
        journal.record(QuestAction::EnteredRelay);
        assert_eq!(
            journal
                .entries()
                .map(|(_, progress)| progress.completed.len())
                .collect::<Vec<_>>(),
            [0, 0]
        );
        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::TalkedToNemi);
        assert_eq!(
            journal
                .entries()
                .map(|(_, progress)| progress.completed.len())
                .collect::<Vec<_>>(),
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
    #[test]
    fn attack_before_conversation_opens_the_solo_route_and_keeps_relay_waiting() {
        let mut journal = test_journal();
        journal.record(QuestAction::InspectedSealedHatch);
        journal.record(QuestAction::AttackedNemi);
        assert_eq!(journal.status(LAST_LIGHT), Some(QuestStatus::Failed));
        assert_eq!(journal.status(RELIGHT_BEACON), Some(QuestStatus::Active));
        assert!(journal.expects(QuestAction::RecoveredCore));
        assert!(!journal.expects(QuestAction::EnteredRelay));
        assert!(!journal.beacon_powered());

        journal.record(QuestAction::RecoveredCore);
        journal.record(QuestAction::InstalledCore);
        assert!(journal.beacon_powered());
        assert!(journal.expects(QuestAction::EnteredRelay));
        assert!(journal.is_complete(RELIGHT_BEACON));
        assert_eq!(journal.status(LAST_LIGHT), Some(QuestStatus::Failed));
    }

    #[test]
    fn attack_with_a_core_skips_solo_recovery_and_keeps_actual_objective_history() {
        let mut journal = test_journal();
        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::RecoveredCore);
        journal.record(QuestAction::AttackedNemi);
        journal.record(QuestAction::AttackedNemi);
        let last_light = journal.progress.get(LAST_LIGHT).expect("started quest");
        assert_eq!(last_light.completed, [0, 1]);
        assert!(
            matches!(&last_light.outcome, QuestOutcome::Failed(reason) if reason.contains("attacked Nemi"))
        );
        assert!(journal.expects(QuestAction::InstalledCore));
        assert!(!journal.expects(QuestAction::ReturnedCore));
        journal.record(QuestAction::InstalledCore);
        assert_eq!(
            journal
                .progress
                .get(RELIGHT_BEACON)
                .expect("solo quest")
                .completed,
            [1]
        );
        assert!(journal.expects(QuestAction::EnteredRelay));
    }

    #[test]
    fn attack_after_early_recovery_opens_the_solo_route_at_installation() {
        let mut journal = test_journal();
        journal.record(QuestAction::RecoveredCore);
        journal.record(QuestAction::AttackedNemi);

        assert_eq!(journal.status(LAST_LIGHT), Some(QuestStatus::Failed));
        assert_eq!(journal.progress[LAST_LIGHT].completed, [1]);
        assert!(journal.progress[RELIGHT_BEACON].completed.is_empty());
        assert!(journal.expects(QuestAction::InstalledCore));
        assert!(!journal.beacon_powered());

        journal.record(QuestAction::InstalledCore);
        assert!(journal.is_complete(RELIGHT_BEACON));
        assert!(journal.expects(QuestAction::EnteredRelay));
    }

    #[test]
    fn attack_after_turn_in_preserves_the_completed_quest_and_power() {
        let mut journal = test_journal();
        journal.record(QuestAction::TalkedToNemi);
        journal.record(QuestAction::RecoveredCore);
        journal.record(QuestAction::ReturnedCore);
        journal.record(QuestAction::AttackedNemi);
        assert!(journal.is_complete(LAST_LIGHT));
        assert!(journal.beacon_powered());
        assert!(journal.expects(QuestAction::EnteredRelay));
        assert_eq!(journal.status(RELIGHT_BEACON), None);
    }

    #[test]
    fn objective_edges_can_branch_to_different_outcomes_and_validate_targets() {
        let mut catalog = test_journal().catalog;
        let transition = Transition {
            when: Condition::Action(QuestAction::InspectedSealedHatch),
            to: Destination::Objective("missing".into()),
            credit: false,
            credit_early: false,
        };
        catalog.0[0].objectives[0].transitions.push(transition);
        assert!(
            catalog
                .validate()
                .expect_err("missing objective")
                .contains("missing")
        );
        catalog.0[0].objectives[0].transitions[1].to = Destination::Fail("Example outcome".into());
        catalog.validate().expect("valid branch");
        let mut journal = QuestJournal::new(catalog, WorldLayout::Outpost);
        journal.record(QuestAction::InspectedSealedHatch);
        let progress = journal.progress.get(LAST_LIGHT).expect("started quest");
        assert_eq!(
            progress.outcome,
            QuestOutcome::Failed("Example outcome".into())
        );
        assert!(progress.completed.is_empty());
    }

    #[test]
    fn early_credit_requires_a_crediting_occurred_transition() {
        let mut catalog = test_journal().catalog;
        catalog.0[0].objectives[1].transitions[0].credit_early = true;
        assert!(
            catalog
                .validate()
                .expect_err("early credit requires an occurred transition")
                .contains("credit_early")
        );

        catalog.0[0].objectives[1].transitions[0].credit_early = false;
        catalog.0[0].objectives[1].transitions[1].credit = false;
        assert!(
            catalog
                .validate()
                .expect_err("early credit requires a transition that grants credit")
                .contains("credit_early")
        );

        catalog.0[0].objectives[1].transitions[1].credit = true;
        catalog.0[0].objectives[1].transitions[1].to = Destination::Fail("Example outcome".into());
        assert!(
            catalog
                .validate()
                .expect_err("a failure transition cannot grant early credit")
                .contains("credit_early")
        );
    }
}
