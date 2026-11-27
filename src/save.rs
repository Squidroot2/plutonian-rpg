#[cfg(test)]
use crate::player::PlayerWorldExt as _;
mod content_ids;

use crate::{
    GameState,
    attributes::{AttributeId, AttributeRanks},
    camera::Orbit,
    combat::{ActorDefeated, CombatSystems, ConfirmedHit, Dead, Health, Hit},
    content::{CollectedPickups, ContentId, Items, NpcId, PlacedPickup, Placements},
    dialogue::{Dialogue, DialogueClick},
    dungeon::{Area, DungeonProgress},
    equipment::{Equipment, EquipmentSlot},
    experience::{Experience, ExperienceNotice, ExperienceReward, ExperienceUpdates},
    guard::{self, GuardPost, OutpostSecurity},
    interaction::{
        ActiveChest, ActiveRack, CorpseLoot, InteractionSystems, LootTarget, Nearby, Target,
    },
    inventory::{Inventory, InventoryNotice, ItemKind, Pickup},
    nemi::{self, FleeState, Nemi, SavedFleeState},
    player::{CharacterProgress, MovementInput, Player, SavedProgress},
    quest::{QuestAction, QuestJournal, SavedJournal},
    storage::{ChestStorage, WeaponRackStorage},
    ui::death_screen::request_game_over,
    ui::inventory_ui::InventoryUiState,
    ui::menu_ui::PlayerMenu,
    ui::quest_ui::QuestSelection,
    world::SessionEntity,
};
use avian3d::prelude::Collider;
use bevy::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

// Four magic bytes followed by the little-endian save schema version.
const HEADER: &[u8; 6] = b"PLUT\x0f\x00";
const SAVE_FORMAT_VERSION: u16 = u16::from_le_bytes([HEADER[4], HEADER[5]]);
const MAX_SAVE_BYTES: usize = 1024 * 1024;

#[derive(Component)]
pub(crate) struct Bed;

#[derive(Message)]
/// Request saving the current run at the bed identified by this entity.
///
/// The entity is the bed's runtime Bevy entity, not the player or save-slot
/// entity; the save system checks that it has the [`Bed`] component.
pub(crate) struct SaveAtBed(pub(crate) Entity);

#[derive(Message)]
pub(crate) struct SleepWithoutSaving(pub(crate) Entity);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BedSaveSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SleepEffectsSet;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EncounterId {
    Guard(GuardPost),
    Added(ContentId),
}

impl EncounterId {
    pub(crate) fn key(self) -> ContentId {
        match self {
            Self::Guard(GuardPost::West) => ContentId::new("guard_west"),
            Self::Guard(GuardPost::East) => ContentId::new("guard_east"),
            Self::Added(id) => id,
        }
    }

    fn from_key(id: ContentId) -> Self {
        GuardPost::ALL
            .into_iter()
            .map(Self::Guard)
            .find(|encounter| encounter.key() == id)
            .unwrap_or(Self::Added(id))
    }
}

#[derive(
    Clone,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
pub(crate) struct SavedPickup<K = ItemKind> {
    pub(crate) stack: Pickup<K>,
    pub(crate) position: [f32; 3],
}

#[derive(
    Clone,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
struct SavedActor {
    position: [f32; 3],
    rotation: [f32; 4],
    health: u16,
}

impl SavedActor {
    fn capture(transform: &Transform, health: &Health) -> Self {
        Self {
            position: transform.translation.to_array(),
            rotation: transform.rotation.to_array(),
            health: health.current(),
        }
    }

    fn is_valid(&self, max_health: u16) -> bool {
        Vec3::from_array(self.position).is_finite()
            && Quat::from_array(self.rotation).is_normalized()
            && self.health <= max_health
    }

    fn restore(&self, transform: &mut Transform, health: &mut Health, entity: &mut EntityCommands) {
        transform.translation = self.position.into();
        transform.rotation = Quat::from_array(self.rotation);
        health.set_current(self.health);
        if self.health == 0 {
            entity.insert(Dead).remove::<Collider>();
        }
    }
}

#[derive(
    Clone,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
struct SavedEncounter {
    actor: SavedActor,
    has_loot: bool,
}

impl SavedEncounter {
    fn is_valid(&self, max_health: u16) -> bool {
        self.actor.is_valid(max_health) && (self.actor.health == 0 || self.has_loot)
    }
}

#[derive(
    Clone,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
pub(crate) struct SavedNpc {
    actor: SavedActor,
    flee: SavedFleeState,
}

impl SavedNpc {
    pub(crate) fn restore(
        &self,
        pose: &mut Transform,
        health: &mut Health,
        flee: &mut FleeState,
        entity: &mut EntityCommands,
    ) {
        self.actor.restore(pose, health, entity);
        flee.restore(&self.flee);
    }
}

// The runtime snapshot is never encoded directly; `content_ids` converts
// it to the string-ID wire format before bitcode encoding.
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub(crate) player_name: String,
    pub(crate) journal: SavedJournal,
    inventory: Inventory,
    equipment: Equipment,
    chests: ChestStorage,
    weapon_rack: WeaponRackStorage,
    dungeons: BTreeMap<ContentId, DungeonProgress>,
    pub(crate) pickups: Vec<SavedPickup>,
    encounters: BTreeMap<EncounterId, SavedEncounter>,
    pub(crate) progression: SavedProgress,
    experience: SavedExperience,
    world: SavedWorld,
    pub(crate) collected_pickups: BTreeSet<ContentId>,
    npcs: BTreeMap<NpcId, SavedNpc>,
}

#[derive(
    Clone,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
struct SavedWorld {
    guards_hostile: bool,
    day: u32,
    surface_last_kill: Option<u32>,
    relay_last_kill: Option<u32>,
}

const RESPAWN_DAYS: u32 = 6;

/// The active run's clock, distinct from the checkpoint until the next save.
#[derive(Resource, Clone, Default)]
pub(crate) struct RespawnClock {
    day: u32,
    surface_last_kill: Option<u32>,
    relay_last_kill: Option<u32>,
}

impl RespawnClock {
    #[cfg(test)]
    pub(crate) fn day(&self) -> u32 {
        self.day
    }

    fn last_kill_mut(&mut self, area: Area) -> &mut Option<u32> {
        match area {
            Area::Surface => &mut self.surface_last_kill,
            Area::OldRelay => &mut self.relay_last_kill,
        }
    }

    fn due(&self, area: Area) -> bool {
        let last = match area {
            Area::Surface => self.surface_last_kill,
            Area::OldRelay => self.relay_last_kill,
        };
        last.is_some_and(|day| self.day.saturating_sub(day) >= RESPAWN_DAYS)
    }
}

#[derive(Message)]
pub(crate) struct RenewArea(pub(crate) Area);

const OLD_RELAY: ContentId = ContentId::new("old_relay");

#[derive(
    Clone,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
pub(crate) struct SavedExperience<I: Ord = EncounterId, N: Ord = NpcId> {
    player: Experience,
    npc_contributions: BTreeSet<N>,
    // Use stable encounter IDs because checkpoint loading recreates the actors.
    encounter_contributions: BTreeSet<I>,
}

impl<I: Ord, N: Ord> Default for SavedExperience<I, N> {
    fn default() -> Self {
        Self {
            player: Experience::default(),
            npc_contributions: BTreeSet::new(),
            encounter_contributions: BTreeSet::new(),
        }
    }
}

impl Snapshot {
    pub(crate) fn caravan_speaker(&self, id: NpcId) -> Option<&SavedNpc> {
        self.npcs.get(&id)
    }

    pub(crate) fn player_loadout(&self) -> (Inventory, Equipment) {
        (self.inventory.clone(), self.equipment.clone())
    }

    pub(crate) fn player_experience(&self) -> Experience {
        self.experience.player
    }

    fn is_valid(&self) -> bool {
        self.valid_state()
            && self.npcs.contains_key(&NpcId::Nemi)
            && (self.npcs.keys().all(|id| !NpcId::CARAVAN.contains(id))
                || self.journal.has_occurred(QuestAction::CaravanArrived))
            && self.npcs.iter().all(|(&id, actor)| {
                (id == NpcId::Nemi || NpcId::CARAVAN.contains(&id))
                    && actor.actor.is_valid(u16::MAX)
                    && actor.flee.is_valid()
            })
            && GuardPost::ALL
                .into_iter()
                .all(|post| self.encounters.contains_key(&EncounterId::Guard(post)))
            && self
                .encounters
                .values()
                .all(|encounter| encounter.is_valid(u16::MAX))
    }

    fn valid_state(&self) -> bool {
        crate::player::PlayerName::valid(&self.player_name)
            && self
                .world
                .surface_last_kill
                .is_none_or(|day| day <= self.world.day)
            && self
                .world
                .relay_last_kill
                .is_none_or(|day| day <= self.world.day)
            && self.inventory.valid_stacks()
            && self.progression.is_valid()
            && self.experience.player.is_valid(self.progression.level())
            && self
                .experience
                .npc_contributions
                .iter()
                .all(|id| self.npcs.get(id).is_some_and(|npc| npc.actor.health > 0))
            && self.experience.encounter_contributions.iter().all(|id| {
                matches!(id, EncounterId::Guard(_))
                    && self
                        .encounters
                        .get(id)
                        .is_some_and(|encounter| encounter.actor.health > 0)
            })
            && self.chests.valid_stacks()
            && self.dungeons.contains_key(&OLD_RELAY)
            && self.dungeons.values().all(DungeonProgress::is_valid)
            && self.pickups.iter().all(|pickup| {
                pickup.stack.quantity > 0
                    && pickup.stack.item != ItemKind::BeaconCore
                    && Vec3::from_array(pickup.position).is_finite()
            })
    }

    fn apply_catalogs(&mut self, items: &Items, placements: &Placements) -> Result<(), String> {
        let saved_items = self
            .inventory
            .0
            .keys()
            .copied()
            .chain(self.equipment.0.values().copied())
            .chain(
                self.chests
                    .0
                    .values()
                    .flat_map(|chest| chest.0.keys().copied()),
            )
            .chain(self.pickups.iter().map(|pickup| pickup.stack.item))
            .chain(self.weapon_rack.0.iter().flatten().copied());
        for item in saved_items {
            items.require(item)?;
        }
        if !self.weapon_rack.valid_weapons(items) {
            return Err("The weapon rack holds a non-weapon item".into());
        }
        let strength = self.progression.ranks().rank(AttributeId::Strength);
        for slot in EquipmentSlot::ALL {
            if let Some(item) = self.equipment.get(slot)
                && (!items[item].fits_slot(slot)
                    || !items[item].meets_strength(strength)
                    || slot == EquipmentSlot::OffHand && self.equipment.off_hand_reserved(items))
            {
                self.equipment
                    .unequip(&mut self.inventory, slot)
                    .map_err(|reason| format!("{reason} Could not return {item} to the bag."))?;
            }
        }
        debug_assert!(self.equipment.valid_slots(items));
        let added: BTreeSet<_> = placements.encounter_ids().collect();
        self.encounters
            .retain(|id, _| !matches!(id, EncounterId::Added(_)) || added.contains(id));
        self.experience
            .encounter_contributions
            .retain(|id| self.encounters.contains_key(id));
        Ok(())
    }
}

#[derive(Resource, Default)]
pub(crate) struct RunSave(Option<Arc<Snapshot>>);

impl RunSave {
    pub(crate) fn start_fresh(&mut self) {
        self.0 = None;
    }

    pub(crate) fn checkpoint(&self) -> Option<&Snapshot> {
        self.0.as_deref()
    }
}

#[derive(Clone)]
pub(crate) struct SaveEntry {
    pub(crate) path: PathBuf,
    pub(crate) name: Option<String>,
    pub(crate) modified: Option<SystemTime>,
}

#[derive(Resource, Default)]
pub(crate) struct SaveSlot {
    directory: Option<PathBuf>,
    pub(crate) entries: Vec<SaveEntry>,
    active: Option<PathBuf>,
    error: Option<String>,
}

impl SaveSlot {
    fn new(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            ..default()
        }
    }

    #[cfg(test)]
    pub(crate) fn open(directory: Option<PathBuf>) -> Self {
        let mut slot = Self::new(directory);
        slot.refresh();
        slot
    }

    pub(crate) fn refresh(&mut self) {
        self.entries.clear();
        let Some(directory) = &self.directory else {
            return;
        };
        let files = match fs::read_dir(directory) {
            Ok(files) => files,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => {
                self.error = Some(format!("Could not list saves: {error}"));
                return;
            }
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|ext| ext != "save") || !path.is_file() {
                continue;
            }
            self.entries.push(SaveEntry {
                modified: file
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok()),
                name: read_save(&path).ok().map(|save| save.player_name),
                path,
            });
        }
        self.entries.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| b.path.cmp(&a.path))
        });
    }

    pub(crate) fn available(&self) -> bool {
        !self.entries.is_empty()
    }
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn load_into(
        &mut self,
        index: usize,
        run: &mut RunSave,
        items: &Items,
        placements: &Placements,
    ) -> bool {
        let Some(entry) = self.entries.get(index) else {
            return false;
        };
        if entry.name.is_none() {
            return false;
        }
        let path = entry.path.clone();
        let result = read_save(&path).and_then(|mut save| {
            save.apply_catalogs(items, placements)
                .map_err(invalid_save)?;
            Ok(save)
        });
        match result {
            Ok(save) => {
                self.active = Some(path);
                self.error = None;
                run.0 = Some(Arc::new(save));
                true
            }
            Err(error) => {
                self.reject(error);
                false
            }
        }
    }

    pub(crate) fn delete(&mut self, index: usize) -> io::Result<()> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| io::Error::other("Save no longer exists"))?;
        fs::remove_file(&entry.path)?;
        if self.active.as_ref() == Some(&entry.path) {
            self.active = None;
        }
        self.entries.remove(index);
        Ok(())
    }

    pub(crate) fn start_fresh(&mut self) {
        self.active = None;
        self.error = None;
    }

    pub(crate) fn reject(&mut self, error: impl std::fmt::Display) {
        warn!("Could not load save: {error}");
        self.active = None;
        self.error = Some(format!("Could not load the save file. {error}"));
    }

    fn save(&mut self, snapshot: Snapshot, run: &mut RunSave) -> io::Result<()> {
        let directory = self
            .directory
            .as_deref()
            .ok_or_else(|| io::Error::other("No save directory"))?;
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let path = next_save_path(directory, &snapshot.player_name, seconds);
        write_save(&path, &snapshot)?;
        let snapshot = Arc::new(snapshot);
        self.active = Some(path);
        self.error = None;
        run.0 = Some(snapshot);
        Ok(())
    }
}

fn next_save_path(directory: &Path, name: &str, seconds: u64) -> PathBuf {
    let stem = format!("{name}-{seconds}");
    let mut path = directory.join(format!("{stem}.save"));
    let mut suffix = 2;
    while path.exists() {
        path = directory.join(format!("{stem}-{suffix}.save"));
        suffix += 1;
    }
    path
}

pub(crate) fn data_path() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/share"))
        })
        .map(|data| data.join("plutonian-rpg"))
}

fn unique_suffix() -> String {
    // The timestamp alone is not a uniqueness guarantee: parallel tests (and
    // concurrent saves) can observe the same clock value, and two callers
    // sharing a suffix share the path built from it — one caller's cleanup
    // can then delete the other's directory mid-write. The counter makes
    // every suffix in this process distinct.
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{}", std::process::id(), now.as_nanos(), count)
}

pub(crate) fn read_save(path: &Path) -> io::Result<Snapshot> {
    let max_save_bytes = u64::try_from(MAX_SAVE_BYTES).expect("save limit fits in u64");
    let mut file = File::open(path)?.take(max_save_bytes + 1);
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    if data.len() > MAX_SAVE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Save file is too large",
        ));
    }
    let Some(payload) = data.strip_prefix(HEADER) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid or unsupported save header",
        ));
    };
    let (base, npcs): (Vec<u8>, BTreeMap<String, SavedNpc>) =
        bitcode::decode(payload).map_err(invalid_save)?;
    let mut snapshot = content_ids::decode(&base)?;
    snapshot.npcs = parse_npcs(npcs)?;
    snapshot
        .is_valid()
        .then_some(snapshot)
        .ok_or_else(|| invalid_save("Invalid save contents"))
}

fn parse_npcs(npcs: BTreeMap<String, SavedNpc>) -> io::Result<BTreeMap<NpcId, SavedNpc>> {
    npcs.into_iter()
        .map(|(key, actor)| {
            NpcId::persistent_id(&key)
                .map(|id| (id, actor))
                .ok_or_else(|| invalid_save(format!("Unknown persistent NPC {key}")))
        })
        .collect()
}

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[schemars(deny_unknown_fields)]
struct InspectorDocument {
    format_version: u16,
    #[serde(flatten)]
    snapshot: content_ids::WireSnapshot,
    collected_pickups: BTreeSet<String>,
    cavern_defeats: BTreeSet<String>,
    npcs: BTreeMap<String, SavedNpc>,
}

pub(crate) fn inspection_data(snapshot: &Snapshot) -> io::Result<serde_json::Value> {
    let document = InspectorDocument {
        format_version: SAVE_FORMAT_VERSION,
        snapshot: content_ids::wire_snapshot(snapshot),
        collected_pickups: snapshot
            .collected_pickups
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        cavern_defeats: snapshot
            .journal
            .cavern_defeats
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        npcs: snapshot
            .npcs
            .iter()
            .map(|(id, actor)| (id.key().to_owned(), actor.clone()))
            .collect(),
    };
    serde_json::to_value(document).map_err(io::Error::other)
}

pub(crate) fn inspection_schema() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&schemars::schema_for!(InspectorDocument))
}

pub(crate) fn create_from_json(input: &Path, output: &Path) -> io::Result<()> {
    use crate::{content, quest::QuestCatalog, world::WorldLayout};

    let source = fs::read(input)?;
    let json: serde_json::Value = serde_json::from_slice(&source).map_err(invalid_save)?;
    let version = json
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| invalid_save("Expected an integer format_version"))?;
    if version != u64::from(SAVE_FORMAT_VERSION) {
        return Err(invalid_save(format!(
            "Unsupported save format version {version}, expected {SAVE_FORMAT_VERSION}"
        )));
    }
    let document: InspectorDocument = serde_json::from_slice(&source).map_err(invalid_save)?;
    let mut snapshot = content_ids::into_snapshot(
        document.snapshot,
        document.cavern_defeats,
        document.collected_pickups,
    )?;
    snapshot.npcs = parse_npcs(document.npcs)?;
    if !snapshot.is_valid() {
        return Err(invalid_save("Invalid save contents"));
    }

    let data = bevy::asset::io::file::FileAssetReader::get_base_path().join("data");
    let (items, placements) = content::read_save_catalogs(&data)?;
    validate_fixture_placements(&snapshot, &placements)?;
    // Loading can move gear to the bag. Validate those changes on a copy so the
    // written save still describes the state provided by the fixture.
    snapshot
        .clone()
        .apply_catalogs(&items, &placements)
        .map_err(invalid_save)?;
    let quests: QuestCatalog = content::read_ron(data.join(QuestCatalog::PATH))?;
    quests.validate().map_err(invalid_save)?;
    QuestJournal::from_snapshot(quests, WorldLayout::Outpost, &snapshot.journal)
        .map_err(invalid_save)?;
    if inspection_data(&snapshot)? != json {
        return Err(invalid_save(
            "Fixture differs from the inspect --json representation after decoding",
        ));
    }
    write_save(output, &snapshot)
}

fn validate_fixture_placements(snapshot: &Snapshot, placements: &Placements) -> io::Result<()> {
    let authored_encounters: BTreeSet<_> = placements.encounter_ids().collect();
    for id in snapshot.encounters.keys() {
        if matches!(id, EncounterId::Added(_)) && !authored_encounters.contains(id) {
            return Err(invalid_save(format!("Unknown encounter ID {}", id.key())));
        }
    }
    let authored_pickups: BTreeSet<_> = placements.pickups().map(|pickup| pickup.id).collect();
    if let Some(id) = snapshot
        .collected_pickups
        .difference(&authored_pickups)
        .next()
    {
        return Err(invalid_save(format!("Unknown collected pickup ID {id}")));
    }
    Ok(())
}

fn invalid_save(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn set_cavern_defeats(journal: &mut SavedJournal, defeats: BTreeSet<ContentId>) -> io::Result<()> {
    if defeats.len() > crate::quest::CAVERN_HUSHCAP_GOAL
        || defeats
            .iter()
            .any(|id| !crate::quest::CAVERN_HUSHCAP_IDS.contains(id))
    {
        return Err(invalid_save("Invalid credited cavern defeats"));
    }
    journal.cavern_defeats = defeats;
    Ok(())
}

fn write_save(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    if !snapshot.is_valid() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid save contents",
        ));
    }
    let npcs: BTreeMap<_, _> = snapshot
        .npcs
        .iter()
        .map(|(id, actor)| (id.key(), actor.clone()))
        .collect();
    let bytes = bitcode::encode(&(content_ids::encode(snapshot), npcs));
    if HEADER.len() + bytes.len() > MAX_SAVE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Save file is too large",
        ));
    }
    let parent = path
        .parent()
        .expect("save path must have a parent directory");
    fs::create_dir_all(parent)?;
    let temp = path.with_extension(format!("{}.tmp", unique_suffix()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = file
        .write_all(HEADER)
        .and_then(|()| file.write_all(&bytes))
        .and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[derive(Resource, Default)]
pub(crate) struct SaveNotice {
    pub(crate) text: Option<&'static str>,
    remaining: f32,
}

fn age_notice(time: Res<Time>, mut notice: ResMut<SaveNotice>) {
    if notice.text.is_some() {
        notice.remaining -= time.delta_secs();
        if notice.remaining <= 0.0 {
            notice.text = None;
        }
    }
}

pub(crate) struct SavePlugin {
    pub(crate) smoke: bool,
    pub(crate) directory: Option<PathBuf>,
}

fn select_save_directory(
    smoke: Option<PathBuf>,
    override_directory: Option<PathBuf>,
    default: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    smoke.or(override_directory).or_else(default)
}

fn playing_or_sleeping(state: Res<State<GameState>>) -> bool {
    matches!(state.get(), GameState::Playing | GameState::Sleeping)
}

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        let smoke_directory = self
            .smoke
            .then(|| std::env::temp_dir().join(format!("plutonian-smoke-{}", unique_suffix())));
        let path = select_save_directory(smoke_directory, self.directory.clone(), data_path);
        if !self.smoke
            && self.directory.is_some()
            && let Some(path) = &path
        {
            info!("Save directory: {}", path.display());
        }
        app.insert_resource(SaveSlot::new(path))
            .init_resource::<CollectedPickups>()
            .init_resource::<RunSave>()
            .init_resource::<RespawnClock>()
            .add_message::<SaveAtBed>()
            .add_message::<RenewArea>()
            .add_message::<SleepWithoutSaving>()
            .init_resource::<SaveNotice>()
            .add_systems(OnEnter(GameState::Loading), reset_session)
            .add_systems(
                OnTransition {
                    exited: GameState::Loading,
                    entered: GameState::Playing,
                },
                restore_actors,
            )
            .add_systems(
                Update,
                (
                    crate::content::clear_sleep_mask,
                    record_disposable_deaths,
                    age_notice,
                    request_game_over,
                )
                    .chain()
                    .after(InteractionSystems)
                    .after(CombatSystems::Resolve)
                    .after(ExperienceUpdates)
                    .after(nemi::react_to_player_hit)
                    .after(guard::react_to_player_hit)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                save_at_bed
                    .in_set(BedSaveSet)
                    .after(InteractionSystems)
                    .after(CombatSystems::Resolve)
                    .after(ExperienceUpdates)
                    .after(nemi::react_to_player_hit)
                    .after(guard::react_to_player_hit)
                    .after(request_game_over)
                    .after(crate::ui::sleep::SleepFadeSet)
                    .run_if(playing_or_sleeping),
            )
            .add_systems(
                Update,
                sleep_without_saving
                    .in_set(SleepEffectsSet)
                    .after(crate::ui::sleep::SleepFadeSet)
                    .run_if(in_state(GameState::Sleeping)),
            )
            .add_systems(
                Update,
                crate::content::renew_encounters
                    .after(BedSaveSet)
                    .after(SleepEffectsSet)
                    .run_if(playing_or_sleeping),
            );
    }
}

fn record_disposable_deaths(
    mut defeats: MessageReader<ActorDefeated>,
    encounters: Query<(&EncounterId, &CorpseLoot), With<crate::enemy::Disposable>>,
    mut clock: ResMut<RespawnClock>,
) {
    for defeat in defeats.read() {
        if let Ok((EncounterId::Added(_), loot)) = encounters.get(defeat.0) {
            let day = clock.day;
            *clock.last_kill_mut(loot.area) = Some(day);
        }
    }
}

pub(crate) fn save_at_bed(
    mut requests: MessageReader<SaveAtBed>,
    beds: Query<(), With<Bed>>,
    mut player: Query<
        (
            &Player,
            &crate::player::PlayerName,
            &mut Health,
            &AttributeRanks,
            &CharacterProgress,
            &Experience,
            &Inventory,
            &Equipment,
        ),
        (Without<Dead>, Without<Nemi>, Without<EncounterId>),
    >,
    nemi: Query<(&Transform, &Health, &FleeState, &ExperienceReward), With<Nemi>>,
    encounters: Query<(
        &EncounterId,
        &Transform,
        &Health,
        &CorpseLoot,
        &ExperienceReward,
    )>,
    pickups: Query<(&Pickup, &Transform), Without<PlacedPickup>>,
    caravan: Query<(&NpcId, &Transform, &Health, &FleeState), (Without<Nemi>, Without<Player>)>,
    (next, collected, placements): (
        Res<NextState<GameState>>,
        Res<CollectedPickups>,
        Res<Placements>,
    ),
    (journal, chests, weapon_rack, dungeon, security): (
        Res<QuestJournal>,
        Res<ChestStorage>,
        Res<WeaponRackStorage>,
        Res<DungeonProgress>,
        Res<OutpostSecurity>,
    ),
    mut slot: ResMut<SaveSlot>,
    mut run: ResMut<RunSave>,
    mut notice: ResMut<SaveNotice>,
    (mut clock, mut renewals): (ResMut<RespawnClock>, MessageWriter<RenewArea>),
) {
    if !requests.read().any(|request| beds.contains(request.0))
        || !matches!(*next, NextState::Unchanged)
    {
        return;
    }
    let Ok((player, name, mut player_health, ranks, progress, experience, inventory, equipment)) =
        player.single_mut()
    else {
        return;
    };
    let Ok((pose, health, nemi, nemi_reward)) = nemi.single() else {
        return;
    };
    if player.area != Area::Surface {
        return;
    }
    let (next_clock, due) = next_sleep_clock(&clock);
    let snapshot = Snapshot {
        player_name: name.0.clone(),
        journal: journal.snapshot(),
        inventory: inventory.clone(),
        equipment: equipment.clone(),
        chests: chests.clone(),
        weapon_rack: weapon_rack.clone(),
        dungeons: BTreeMap::from([(OLD_RELAY, *dungeon)]),
        pickups: pickups
            .iter()
            .map(|(&stack, transform)| SavedPickup {
                stack,
                position: transform.translation.to_array(),
            })
            .collect(),
        encounters: encounters
            .iter()
            .filter(|(id, ..)| {
                !placements
                    .encounters()
                    .any(|entry| EncounterId::Added(entry.id) == **id && due.contains(&entry.area))
            })
            .map(|(&id, transform, health, loot, _)| {
                (
                    id,
                    SavedEncounter {
                        actor: SavedActor::capture(transform, health),
                        has_loot: loot.stack.is_some(),
                    },
                )
            })
            .collect(),
        progression: SavedProgress::capture(ranks, *progress),
        experience: SavedExperience {
            player: *experience,
            npc_contributions: (health.current() > 0 && nemi_reward.player_contributed)
                .then_some(NpcId::Nemi)
                .into_iter()
                .collect(),
            encounter_contributions: encounters
                .iter()
                .filter(|(id, _, health, _, reward)| {
                    matches!(id, EncounterId::Guard(_))
                        && health.current() > 0
                        && reward.player_contributed
                })
                .map(|(&id, ..)| id)
                .collect(),
        },
        world: SavedWorld {
            guards_hostile: security.hostile,
            day: next_clock.day,
            surface_last_kill: next_clock.surface_last_kill,
            relay_last_kill: next_clock.relay_last_kill,
        },
        collected_pickups: collected.0.clone(),
        npcs: caravan
            .iter()
            .map(|(&id, pose, health, flee)| {
                (
                    id,
                    SavedNpc {
                        actor: SavedActor::capture(pose, health),
                        flee: flee.snapshot(),
                    },
                )
            })
            .chain(std::iter::once((
                NpcId::Nemi,
                SavedNpc {
                    actor: SavedActor::capture(pose, health),
                    flee: nemi.snapshot(),
                },
            )))
            .collect(),
    };
    let text = match slot.save(snapshot, &mut run) {
        Ok(()) => {
            commit_sleep_effects(
                &mut clock,
                next_clock,
                due,
                &mut renewals,
                &mut player_health,
            );
            "Game saved."
        }
        Err(error) => {
            warn!("Could not save game: {error}");
            "Could not save."
        }
    };
    *notice = SaveNotice {
        text: Some(text),
        remaining: 2.0,
    };
}

fn next_sleep_clock(clock: &RespawnClock) -> (RespawnClock, Vec<Area>) {
    let mut next = clock.clone();
    next.day = next.day.saturating_add(1);
    let due: Vec<_> = [Area::Surface, Area::OldRelay]
        .into_iter()
        .filter(|&area| next.due(area))
        .collect();
    for &area in &due {
        *next.last_kill_mut(area) = None;
    }
    (next, due)
}

fn commit_sleep_effects(
    clock: &mut RespawnClock,
    next: RespawnClock,
    due: Vec<Area>,
    renewals: &mut MessageWriter<RenewArea>,
    health: &mut Health,
) {
    *clock = next;
    for area in due {
        renewals.write(RenewArea(area));
    }
    health.restore();
}

fn sleep_without_saving(
    mut requests: MessageReader<SleepWithoutSaving>,
    beds: Query<(), With<Bed>>,
    mut player: Query<(&Player, &mut Health), (Without<Dead>, Without<Nemi>, Without<EncounterId>)>,
    mut clock: ResMut<RespawnClock>,
    mut renewals: MessageWriter<RenewArea>,
    mut notice: ResMut<SaveNotice>,
) {
    let Some(_request) = requests.read().find(|request| beds.contains(request.0)) else {
        return;
    };
    let Ok((player, mut health)) = player.single_mut() else {
        return;
    };
    if player.area != Area::Surface {
        return;
    }
    let (next, due) = next_sleep_clock(&clock);
    commit_sleep_effects(&mut clock, next, due, &mut renewals, &mut health);
    *notice = SaveNotice {
        text: Some("A new day dawns."),
        remaining: 2.0,
    };
}

pub(crate) fn reset_session(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<SessionEntity>>();
    let roots: Vec<_> = query.iter(world).collect();
    for root in roots {
        world.despawn(root);
    }
    let checkpoint = world.resource::<RunSave>().0.clone();
    let mut clock = checkpoint
        .as_ref()
        .map_or_else(RespawnClock::default, |save| RespawnClock {
            day: save.world.day,
            surface_last_kill: save.world.surface_last_kill,
            relay_last_kill: save.world.relay_last_kill,
        });
    for area in [Area::Surface, Area::OldRelay] {
        if clock.due(area) {
            *clock.last_kill_mut(area) = None;
        }
    }
    world.insert_resource(clock);
    if let Some(save) = checkpoint {
        world.insert_resource(CollectedPickups(save.collected_pickups.clone()));
        world.insert_resource(save.chests.clone());
        world.insert_resource(save.weapon_rack.clone());
        world.insert_resource(
            *save
                .dungeons
                .get(&OLD_RELAY)
                .expect("validated save contains Old Relay progress"),
        );
    } else {
        let stock = world.resource::<Placements>().stock();
        world.insert_resource(stock);
        world.insert_resource(WeaponRackStorage::<ItemKind>::default());
        world.insert_resource(CollectedPickups::default());
        world.insert_resource(DungeonProgress::default());
    }
    world.insert_resource(OutpostSecurity::default());
    world.insert_resource(Dialogue::default());
    world.insert_resource(DialogueClick::default());
    world.insert_resource(Nearby::default());
    world.insert_resource(LootTarget::default());
    world.insert_resource(ActiveChest::default());
    world.insert_resource(ActiveRack::default());
    world.insert_resource(InventoryNotice::default());
    world.insert_resource(SaveNotice::default());
    world.insert_resource(ExperienceNotice::default());
    world.insert_resource(PlayerMenu::default());
    world.insert_resource(InventoryUiState::default());
    world.insert_resource(QuestSelection::default());
    world.insert_resource(MovementInput::default());
    world.insert_resource(Orbit::default());
    world.resource_mut::<Messages<Hit>>().clear();
    world.resource_mut::<Messages<ConfirmedHit>>().clear();
    world.resource_mut::<Messages<ActorDefeated>>().clear();
}

fn restore_actors(
    run: Res<RunSave>,
    placements: Res<Placements>,
    journal: Res<QuestJournal>,
    mut security: ResMut<OutpostSecurity>,
    mut nemi: Query<
        (
            Entity,
            &mut Transform,
            &mut Health,
            &mut FleeState,
            &mut ExperienceReward,
        ),
        (With<Nemi>, Without<EncounterId>),
    >,
    mut encounters: Query<
        (
            Entity,
            &EncounterId,
            &mut Transform,
            &mut Health,
            &mut CorpseLoot,
            &mut ExperienceReward,
        ),
        Without<Nemi>,
    >,
    quest_targets: Query<(Entity, &Target)>,
    mut commands: Commands,
) {
    let Some(save) = run.checkpoint() else {
        return;
    };
    security.hostile = save.world.guards_hostile;
    if let Ok((entity, mut transform, mut health, mut nemi, mut reward)) = nemi.single_mut() {
        if let Some(saved) = save.npcs.get(&NpcId::Nemi) {
            saved.restore(
                &mut transform,
                &mut health,
                &mut nemi,
                &mut commands.entity(entity),
            );
        }
        reward.player_contributed = save.experience.npc_contributions.contains(&NpcId::Nemi);
    }
    let clock = RespawnClock {
        day: save.world.day,
        surface_last_kill: save.world.surface_last_kill,
        relay_last_kill: save.world.relay_last_kill,
    };
    for (entity, id, mut transform, mut health, mut loot, mut reward) in &mut encounters {
        if placements
            .encounters()
            .any(|entry| EncounterId::Added(entry.id) == *id && clock.due(entry.area))
        {
            continue;
        }
        if let Some(encounter) = save.encounters.get(id) {
            // Living guards regroup at their posts so the player can leave the cot.
            if matches!(id, EncounterId::Guard(_)) && encounter.actor.health > 0 {
                health.set_current(encounter.actor.health);
            } else {
                encounter
                    .actor
                    .restore(&mut transform, &mut health, &mut commands.entity(entity));
            }
            reward.player_contributed = matches!(id, EncounterId::Guard(_))
                && save.experience.encounter_contributions.contains(id);
            if !encounter.has_loot {
                loot.stack = None;
                commands.entity(entity).insert(Visibility::Hidden);
            }
        }
    }
    if journal.has_occurred(QuestAction::RecoveredCore) {
        for (entity, &target) in &quest_targets {
            if target == Target::Core {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_items;
    use crate::{
        equipment::EquipmentSlot,
        quest::{CONTAIN_THE_HUSHCAPS, restore_test_journal, test_journal, test_relay_journal},
        storage::ChestId,
        ui::settings::SettingsMenu,
        ui::title::TitlePlugin,
        world::WorldLayout,
    };
    use bevy::{asset::AssetPlugin, ecs::system::RunSystemOnce, state::app::StatesPlugin};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let dir =
                Self(std::env::temp_dir().join(format!("plutonian-save-test-{}", unique_suffix())));
            fs::create_dir_all(&dir.0).expect("create the test save directory");
            dir
        }

        fn path(&self) -> PathBuf {
            fs::read_dir(&self.0)
                .expect("test directory")
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "save"))
                .max_by_key(|path| {
                    (
                        fs::metadata(path).and_then(|m| m.modified()).ok(),
                        path.clone(),
                    )
                })
                .unwrap_or_else(|| self.0.join("fixture.save"))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn bed_world(path: PathBuf) -> (World, Entity, Entity, Entity) {
        let mut world = World::new();
        world.insert_resource(SaveSlot::open(path.parent().map(Path::to_path_buf)));
        world.insert_resource(test_journal());
        world.init_resource::<RunSave>();
        world.init_resource::<OutpostSecurity>();
        world.init_resource::<SaveNotice>();
        world.init_resource::<NextState<GameState>>();
        world.init_resource::<Messages<SaveAtBed>>();
        world.init_resource::<Messages<SleepWithoutSaving>>();
        world.init_resource::<Messages<RenewArea>>();
        world.init_resource::<RespawnClock>();

        world.insert_resource(crate::content::test_placements().stock());
        world.init_resource::<WeaponRackStorage>();
        world.init_resource::<CollectedPickups>();
        world.insert_resource(crate::content::test_placements().clone());
        world.init_resource::<DungeonProgress>();

        let bed = world.spawn(Bed).id();
        let player = world
            .spawn((
                Player::default(),
                crate::player::PlayerName("Test Player".into()),
            ))
            .id();
        world
            .get_mut::<Health>(player)
            .expect("player health")
            .set_current(21);
        world.spawn((
            Nemi,
            Health::new(30),
            Transform::default(),
            Collider::sphere(0.5),
        ));
        let mut corpse = Entity::PLACEHOLDER;
        use EncounterId::*;
        let placements = crate::content::test_placements();
        let enemies = crate::content::test_enemies();
        let authored = placements.encounters().map(|entry| {
            let definition = enemies.get(entry.enemy).expect("fixture enemy exists");
            (
                Added(entry.id),
                Vec3::from_array(entry.position),
                definition.body().health,
                definition.body().loot.item,
            )
        });
        let guards = GuardPost::ALL
            .into_iter()
            .map(|post| (Guard(post), post.home(), 140, ItemKind::Scrap));
        for (id, home, health, item) in authored.chain(guards) {
            let mut entity = world.spawn((
                id,
                Transform::from_translation(home),
                Health::new(health),
                CorpseLoot::new(item, Area::Surface),
                ExperienceReward::new(10),
                Collider::sphere(0.5),
            ));
            if id == Added(ContentId::new("relay_roach")) {
                entity
                    .get_mut::<Health>()
                    .expect("roach health")
                    .set_current(0);
                entity.get_mut::<CorpseLoot>().expect("roach loot").stack = None;
                corpse = entity.id();
            }
        }
        (world, bed, player, corpse)
    }

    #[test]
    fn sleeping_advances_clock_and_renews_only_due_areas() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        world.resource_mut::<RespawnClock>().surface_last_kill = Some(0);
        for expected in 1..=5 {
            interact_with(&mut world, bed);
            let clock = world.resource::<RespawnClock>();
            assert_eq!(clock.day, expected);
            assert_eq!(clock.surface_last_kill, Some(0));
        }
        interact_with(&mut world, bed);
        let clock = world.resource::<RespawnClock>();
        assert_eq!(clock.day, 6);
        assert_eq!(clock.surface_last_kill, None);
        let snapshot = world
            .resource::<RunSave>()
            .checkpoint()
            .expect("sleep saved");
        assert_eq!(snapshot.world.day, 6);
        assert_eq!(snapshot.world.surface_last_kill, None);
    }

    #[test]
    fn sleep_without_saving_advances_clock_heals_and_renews_without_checkpoint() {
        let dir = TestDirectory::new();
        let (mut world, bed, player, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let saved_day = world
            .resource::<RunSave>()
            .checkpoint()
            .expect("save before sleep-only choice")
            .world
            .day;
        let files_before = fs::read_dir(&dir.0).expect("save directory").count();
        {
            let mut clock = world.resource_mut::<RespawnClock>();
            clock.day = 5;
            clock.surface_last_kill = Some(0);
            clock.relay_last_kill = Some(4);
        }
        world
            .get_mut::<Health>(player)
            .expect("player health")
            .set_current(1);
        let mut renewals = world.resource::<Messages<RenewArea>>().get_cursor_current();
        world.write_message(SleepWithoutSaving(bed));
        world
            .run_system_once(sleep_without_saving)
            .expect("sleep without saving system");

        let clock = world.resource::<RespawnClock>();
        assert_eq!(clock.day, 6);
        assert_eq!(clock.surface_last_kill, None);
        assert_eq!(clock.relay_last_kill, Some(4));
        let emitted: Vec<_> = renewals
            .read(world.resource::<Messages<RenewArea>>())
            .map(|renewal| renewal.0)
            .collect();
        assert_eq!(emitted, [Area::Surface]);
        let health = world.get::<Health>(player).expect("restored player health");
        assert_eq!(health.current(), health.maximum());
        assert_eq!(
            world
                .resource::<RunSave>()
                .checkpoint()
                .expect("sleep-only must preserve checkpoint")
                .world
                .day,
            saved_day
        );
        assert_eq!(
            world.resource::<SaveNotice>().text,
            Some("A new day dawns.")
        );
        assert_eq!(
            fs::read_dir(&dir.0).expect("save directory").count(),
            files_before
        );
    }

    fn interact_with(world: &mut World, entity: Entity) {
        world.resource_mut::<Messages<SaveAtBed>>().clear();
        world.write_message(SaveAtBed(entity));
        world.run_system_once(save_at_bed).expect("bed save system");
    }

    #[test]
    fn inspection_reports_complete_current_snapshot_without_changing_the_save() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let file = dir.path();
        let before = fs::read(&file).expect("read game-written save");
        let data = crate::save_inspector::inspect(Some(&file)).expect("inspect game save");
        assert_eq!(data["format_version"], 15);
        assert_eq!(data["player_name"], "Test Player");
        assert!(data["journal"]["progress"].get("last_light").is_some());
        assert_eq!(data["encounters"]["relay_roach"]["actor"]["health"], 0);
        for field in [
            "inventory",
            "equipment",
            "progression",
            "experience",
            "chests",
            "weapon_rack",
            "dungeons",
            "pickups",
            "collected_pickups",
            "cavern_defeats",
            "world",
            "npcs",
        ] {
            assert!(data.get(field).is_some(), "missing {field}");
        }
        let report = crate::save_inspector::text_report(&data);
        for expected in ["Test Player", "last_light", "relay_roach", "health: 0"] {
            assert!(report.contains(expected), "report missing {expected}");
        }
        assert_eq!(fs::read(&file).expect("read save after inspection"), before);
        let mut loaded = read_save(&file).expect("game reads inspected save");
        loaded
            .apply_catalogs(test_items(), crate::content::test_placements())
            .expect("game validates inspected save against catalogs");
    }

    #[test]
    fn inspector_game_fixture_loads_with_current_catalogs_and_quest_journal() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inspector-v15.save");
        let before = fs::read(&file).expect("read fixture");
        crate::save_inspector::inspect(Some(&file)).expect("inspect fixture");
        let mut save = read_save(&file).expect("decode inspected fixture");
        save.apply_catalogs(test_items(), crate::content::test_placements())
            .expect("validate inspected fixture");
        let catalog =
            ron::from_str(include_str!("../data/quests/catalog.quest.ron")).expect("quest catalog");
        QuestJournal::from_snapshot(catalog, WorldLayout::Outpost, &save.journal)
            .expect("restore inspected quest journal");
        assert_eq!(fs::read(&file).expect("read inspected fixture"), before);
    }

    #[test]
    fn inspector_rejects_decodable_but_invalid_saved_contents() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let file = dir.path();
        let mut snapshot = read_save(&file).expect("valid checkpoint");
        snapshot.player_name.clear();
        let npcs: BTreeMap<_, _> = snapshot
            .npcs
            .iter()
            .map(|(id, npc)| (id.key(), npc.clone()))
            .collect();
        let mut bytes = HEADER.to_vec();
        bytes.extend(bitcode::encode(&(content_ids::encode(&snapshot), npcs)));
        fs::write(&file, bytes).expect("write invalid snapshot");
        let error =
            crate::save_inspector::inspect(Some(&file)).expect_err("invalid snapshot rejected");
        assert!(error.to_string().contains("Invalid save contents"));
    }

    #[test]
    fn save_directory_selection_prefers_smoke_then_override_then_default() {
        let smoke = PathBuf::from("/tmp/smoke");
        let override_path = PathBuf::from("/tmp/golden");
        let default_path = PathBuf::from("/tmp/default");
        assert_eq!(
            select_save_directory(Some(smoke.clone()), Some(override_path.clone()), || Some(
                default_path.clone()
            )),
            Some(smoke)
        );
        assert_eq!(
            select_save_directory(None, Some(override_path.clone()), || Some(
                default_path.clone()
            )),
            Some(override_path)
        );
        assert_eq!(
            select_save_directory(None, None, || Some(default_path.clone())),
            Some(default_path)
        );
        assert_eq!(select_save_directory(None, None, || None), None);
    }

    #[test]
    fn override_directory_confines_listing_loading_saving_and_deletion() {
        let default_dir = TestDirectory::new();
        let override_dir = TestDirectory::new();
        let (mut default_world, default_bed, _, _) = bed_world(default_dir.path());
        interact_with(&mut default_world, default_bed);
        let default_file = default_dir.path();
        let default_bytes = fs::read(&default_file).expect("default save");

        let (mut world, bed, _, _) = bed_world(override_dir.path());
        let selected = select_save_directory(None, Some(override_dir.0.clone()), || {
            Some(default_dir.0.clone())
        });
        world.insert_resource(SaveSlot::open(selected));
        interact_with(&mut world, bed);
        interact_with(&mut world, bed);
        let slot = SaveSlot::open(Some(override_dir.0.clone()));
        assert_eq!(slot.entries.len(), 2);
        assert!(
            slot.entries
                .iter()
                .all(|entry| entry.name.as_deref() == Some("Test Player")
                    && entry.modified.is_some())
        );
        let first_entry = slot.entries[0].path.clone();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .init_asset::<Image>()
            .init_state::<GameState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::controls::Keybinds>()
            .init_resource::<SettingsMenu>()
            .init_resource::<RunSave>()
            .insert_resource(test_items().clone())
            .insert_resource(crate::content::test_placements().clone())
            .insert_resource(slot)
            .add_plugins(TitlePlugin);
        app.update();
        let press = |app: &mut App, key| {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();
        };
        press(&mut app, KeyCode::KeyS);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.world().resource::<SaveSlot>().active.as_ref(),
            Some(&first_entry)
        );
        let loaded = app.world().resource::<RunSave>().0.clone();
        assert!(loaded.is_some());
        world.insert_resource(RunSave(loaded));
        interact_with(&mut world, bed);
        let new_save = override_dir.path();
        assert_ne!(new_save, first_entry);
        assert_eq!(
            fs::read_dir(&override_dir.0)
                .expect("override directory")
                .count(),
            3
        );
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Title);
        app.update();
        press(&mut app, KeyCode::KeyS);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Delete);
        assert!(new_save.exists());
        press(&mut app, KeyCode::Enter);
        assert!(!new_save.exists());
        assert!(first_entry.exists());
        assert_eq!(app.world().resource::<SaveSlot>().entries.len(), 2);
        assert_eq!(
            fs::read(default_file).expect("default save remains"),
            default_bytes
        );
        assert_eq!(
            fs::read_dir(&default_dir.0)
                .expect("default directory")
                .count(),
            1
        );
    }

    #[test]
    fn bed_saves_coexist_and_loading_an_older_save_preserves_its_checkpoint() {
        let dir = TestDirectory::new();
        let (mut world, bed, player, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let first = dir.path();
        let first_bytes = fs::read(&first).expect("first save");
        File::options()
            .write(true)
            .open(&first)
            .expect("open first save")
            .set_times(
                fs::FileTimes::new().set_modified(UNIX_EPOCH + std::time::Duration::from_secs(100)),
            )
            .expect("set first save time");
        world
            .entity_mut(player)
            .insert(crate::player::PlayerName("Other Player".into()));
        assert!(
            world
                .actor_data_mut::<Inventory>()
                .add(ItemKind::Battery, 2)
        );
        interact_with(&mut world, bed);
        let second = dir.path();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).expect("first save remains"), first_bytes);
        let mut slot = SaveSlot::open(Some(dir.0.clone()));
        assert_eq!(slot.entries.len(), 2);
        assert_eq!(slot.entries[0].path, second);
        assert_eq!(slot.entries[0].name.as_deref(), Some("Other Player"));
        assert_eq!(slot.entries[1].name.as_deref(), Some("Test Player"));
        let mut run = RunSave::default();
        assert!(slot.load_into(1, &mut run, test_items(), crate::content::test_placements()));
        assert_eq!(slot.active.as_ref(), Some(&first));
        assert_eq!(
            run.checkpoint()
                .expect("loaded save")
                .inventory
                .count(ItemKind::Battery),
            0
        );
        world.insert_resource(run);
        world.entity_mut(player).insert(Dead);
        world
            .run_system_once(request_game_over)
            .expect("recovery system");
        assert_eq!(
            world
                .resource::<RunSave>()
                .checkpoint()
                .expect("recovery checkpoint")
                .inventory
                .count(ItemKind::Battery),
            0
        );
    }

    #[test]
    fn unreadable_saves_can_be_deleted_without_touching_other_files() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let valid = dir.path();
        let original = fs::read(&valid).expect("valid save");
        let garbage = dir.0.join("garbage-1.save");
        fs::write(&garbage, b"garbage").expect("garbage file");
        fs::write(dir.0.join("save.bin"), b"old layout").expect("legacy file");
        let mut slot = SaveSlot::open(Some(dir.0.clone()));
        assert_eq!(slot.entries.len(), 2);
        let bad = slot
            .entries
            .iter()
            .position(|entry| entry.path == garbage)
            .expect("unreadable entry");
        assert!(slot.entries[bad].name.is_none());
        let mut run = RunSave::default();
        assert!(!slot.load_into(
            bad,
            &mut run,
            test_items(),
            crate::content::test_placements()
        ));
        slot.delete(bad).expect("delete unreadable save");
        assert!(!garbage.exists());
        assert_eq!(fs::read(valid).expect("valid save remains"), original);
        assert!(dir.0.join("save.bin").exists());
        assert!(slot.load_into(0, &mut run, test_items(), crate::content::test_placements()));
    }

    #[test]
    fn save_filename_collision_uses_increasing_suffixes() {
        let dir = TestDirectory::new();
        let first = next_save_path(&dir.0, "Test Player", 42);
        assert!(first.ends_with("Test Player-42.save"));
        fs::write(&first, b"first").expect("reserve first name");
        let second = next_save_path(&dir.0, "Test Player", 42);
        assert!(second.ends_with("Test Player-42-2.save"));
        fs::write(&second, b"second").expect("reserve second name");
        assert!(next_save_path(&dir.0, "Test Player", 42).ends_with("Test Player-42-3.save"));
    }

    #[test]
    fn authored_ids_round_trip_and_removed_placements_do_not_revive_loot() {
        use crate::content::tests::{variant_enemies, variant_items, variant_placements};

        let items = variant_items();
        let enemies = variant_enemies(&items);
        let placements = variant_placements(&items, &enemies);
        let pickup = placements.pickups().next().expect("authored pickup");
        let encounter = placements
            .encounter_ids()
            .next()
            .expect("authored encounter");
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let mut added = read_save(&dir.path()).expect("checkpoint predates the placement");
        added
            .apply_catalogs(&items, &placements)
            .expect("new placements permit old saves");
        assert!(
            !added.encounters.contains_key(&encounter),
            "no saved actor overrides its fresh spawn"
        );

        let mut bag = placements
            .stock()
            .get(crate::storage::ChestId::Gear)
            .clone();
        assert_eq!(
            world
                .actor_data_mut::<Equipment>()
                .equip(&mut bag, pickup.stack.item, 10, &items),
            Ok(true)
        );
        world.resource_mut::<CollectedPickups>().0.insert(pickup.id);
        let dropped = world
            .spawn((pickup.stack, Transform::from_xyz(5.0, 1.0, 4.0)))
            .id();
        world.spawn((
            pickup.stack,
            Transform::default(),
            PlacedPickup {
                id: pickup.id,
                area: pickup.area,
            },
        ));
        world.spawn((
            encounter,
            Transform::default(),
            Health::new(0),
            CorpseLoot {
                stack: None,
                area: Area::Surface,
            },
            ExperienceReward::new(82),
        ));
        interact_with(&mut world, bed);

        let mut saved = read_save(&dir.path()).expect("reload authored IDs");
        assert_eq!(saved.collected_pickups, BTreeSet::from([pickup.id]));
        assert_eq!(saved.pickups.len(), 1, "authored pickups are reconstructed");
        assert_eq!(saved.pickups[0].stack, pickup.stack);
        assert_eq!(
            saved.equipment.get(EquipmentSlot::MainHand),
            Some(pickup.stack.item)
        );
        assert_eq!(saved.encounters[&encounter].actor.health, 0);
        assert!(!saved.encounters[&encounter].has_loot);
        assert!(
            saved
                .apply_catalogs(test_items(), &placements)
                .expect_err("dropped variants need their item definition")
                .contains("relay_pike")
        );
        assert!(world.despawn(dropped));
        world.actor_data_mut::<Equipment>().0.clear();
        interact_with(&mut world, bed);

        let mut saved =
            read_save(&dir.path()).expect("checkpoint after removing the dropped pickup");
        saved
            .apply_catalogs(test_items(), crate::content::test_placements())
            .expect("removed placements do not invalidate the checkpoint");
        assert!(!saved.encounters.contains_key(&encounter));
        assert!(saved.collected_pickups.contains(&pickup.id));
    }

    #[test]
    fn loading_after_balance_changes_returns_gear_and_caps_living_actor_health() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        world
            .actor_data_mut::<Equipment>()
            .0
            .insert(EquipmentSlot::Body, ItemKind::FieldArmor);
        interact_with(&mut world, bed);
        let mut saved = read_save(&dir.path()).expect("read pre-edit checkpoint");
        let text = include_str!("../data/items/catalog.item.ron").replace(
            "slot: Body, defense: 2, minimum_strength: 1",
            "slot: Body, defense: 2, minimum_strength: 50",
        );
        let items: Items = ron::from_str(&text).expect("updated RON parses");
        saved
            .apply_catalogs(&items, crate::content::test_placements())
            .expect("gear can go into the bag");
        assert!(saved.equipment.get(EquipmentSlot::Body).is_none());
        assert_eq!(saved.inventory.count(ItemKind::FieldArmor), 1);

        let (mut rebuilt, _, _, dead_roach) = bed_world(dir.path());
        let worm = encounter_entity(
            &mut rebuilt,
            crate::content::ContentId::new("cave_first_worm"),
        );
        rebuilt.entity_mut(worm).insert(Health::new(10));
        rebuilt.insert_resource(RunSave(Some(Arc::new(saved))));
        rebuilt
            .run_system_once(restore_actors)
            .expect("apply saved health");
        assert_eq!(
            rebuilt.get::<Health>(worm).expect("worm health").current(),
            10
        );
        assert!(rebuilt.get::<Dead>(dead_roach).is_some());
        assert!(
            rebuilt
                .get::<CorpseLoot>(dead_roach)
                .expect("saved corpse")
                .stack
                .is_none()
        );
    }

    #[test]
    fn current_save_restores_nemi_health_and_keeps_defeat() {
        let dir = TestDirectory::new();
        let path = dir.path();
        let (mut world, bed, _, _) = bed_world(path.clone());
        interact_with(&mut world, bed);
        let path = dir.path();
        let mut checkpoint = read_save(&path).expect("fixture checkpoint");
        for expected in [30, 15, 0] {
            checkpoint
                .npcs
                .get_mut(&NpcId::Nemi)
                .expect("Nemi")
                .actor
                .health = expected;
            write_save(&path, &checkpoint).expect("write the current format");
            let loaded = read_save(&path).expect("current save still loads");
            let (mut rebuilt, _, _, _) = bed_world(path.clone());
            let keeper = rebuilt
                .query_filtered::<Entity, With<Nemi>>()
                .single(&rebuilt)
                .expect("keeper");
            rebuilt.entity_mut(keeper).insert(Health::new(84));
            rebuilt.insert_resource(RunSave(Some(Arc::new(loaded))));
            rebuilt
                .run_system_once(restore_actors)
                .expect("restore Nemi");
            assert_eq!(
                rebuilt
                    .get::<Health>(keeper)
                    .expect("Nemi health")
                    .current(),
                expected
            );
            assert_eq!(rebuilt.get::<Dead>(keeper).is_some(), expected == 0);
        }
    }

    #[test]
    fn nemi_pose_and_flee_state_survive_save_and_restore() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        let nemi = world
            .query_filtered::<Entity, With<Nemi>>()
            .single(&world)
            .expect("Nemi");
        let position = Vec3::new(4.0, 1.0, 9.0);
        world
            .get_mut::<Transform>(nemi)
            .expect("Nemi pose")
            .translation = position;
        world
            .get_mut::<FleeState>(nemi)
            .expect("Nemi flee state")
            .set_residence(Vec3::new(8.0, 0.0, 3.0), None);
        let expected = serde_json::to_value(
            world
                .get::<FleeState>(nemi)
                .expect("Nemi flee state")
                .snapshot(),
        )
        .expect("serialize flee state");
        interact_with(&mut world, bed);
        let checkpoint = read_save(&dir.path()).expect("read checkpoint");
        let (mut rebuilt, _, _, _) = bed_world(dir.path());
        let restored = rebuilt
            .query_filtered::<Entity, With<Nemi>>()
            .single(&rebuilt)
            .expect("Nemi");
        rebuilt.insert_resource(RunSave(Some(Arc::new(checkpoint))));
        rebuilt
            .run_system_once(restore_actors)
            .expect("restore Nemi");
        assert_eq!(
            rebuilt
                .get::<Transform>(restored)
                .expect("Nemi pose")
                .translation,
            position
        );
        let actual = serde_json::to_value(
            rebuilt
                .get::<FleeState>(restored)
                .expect("Nemi flee state")
                .snapshot(),
        )
        .expect("serialize flee state");
        assert_eq!(actual, expected);
    }

    #[test]
    fn version_thirteen_round_trips_caravan_casualties_and_poses() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        world
            .resource_mut::<QuestJournal>()
            .record(QuestAction::CaravanArrived);
        let position = Vec3::new(-25.0, 0.0, 18.0);
        world.spawn((
            NpcId::Tovan,
            Transform::from_translation(position),
            Health::new(0),
            FleeState::civilian(),
        ));
        world.spawn((
            NpcId::Isera,
            Transform::default(),
            Health::new(64),
            FleeState::civilian(),
        ));
        interact_with(&mut world, bed);
        let checkpoint = read_save(&dir.path()).expect("read v14 checkpoint");
        let inspection =
            crate::save_inspector::inspect(Some(&dir.path())).expect("inspect caravan");
        assert_eq!(inspection["npcs"][NpcId::Tovan.key()]["actor"]["health"], 0);
        assert_eq!(
            inspection["npcs"][NpcId::Isera.key()]["actor"]["health"],
            64
        );
        assert_eq!(checkpoint.player_name, "Test Player");
        let tovan = checkpoint
            .caravan_speaker(NpcId::Tovan)
            .expect("Tovan checkpoint");
        assert_eq!(tovan.actor.position, position.to_array());
        assert_eq!(tovan.actor.health, 0);
        assert_eq!(
            checkpoint
                .caravan_speaker(NpcId::Isera)
                .expect("Isera checkpoint")
                .actor
                .health,
            64
        );
    }

    #[test]
    fn bed_is_the_only_save_target_and_the_latest_save_restores_world_progress() {
        let dir = TestDirectory::new();
        let (mut world, bed, player, corpse) = bed_world(dir.path());
        interact_with(&mut world, player);
        assert!(!dir.path().exists());
        assert_eq!(
            world
                .get::<Health>(player)
                .expect("player health")
                .current(),
            21
        );
        interact_with(&mut world, bed);
        assert_eq!(
            world
                .get::<Health>(player)
                .expect("player health")
                .current(),
            AttributeRanks::default().effects().health
        );

        let journal = &mut *world.resource_mut::<QuestJournal>();
        for action in [
            QuestAction::TalkedToNemi,
            QuestAction::RecoveredCore,
            QuestAction::ReturnedCore,
        ] {
            journal.record(action);
        }
        let mut inventory = Inventory::default();
        assert!(inventory.add(ItemKind::Scrap, 3));
        assert!(inventory.add(ItemKind::FieldArmor, 1));
        world
            .actor_data_mut::<Equipment>()
            .equip(&mut inventory, ItemKind::FieldArmor, 10, test_items())
            .expect("equip armor");
        world.set_player_data(inventory);
        let lamp = world
            .resource_mut::<ChestStorage>()
            .into_inner()
            .get_mut(ChestId::Lamp);
        assert!(lamp.remove(ItemKind::Battery, 1));
        let dropped_at = Vec3::new(5.0, -1.25, -8.0);
        world.spawn((
            Pickup {
                item: ItemKind::Battery,
                quantity: 1,
            },
            Transform::from_translation(dropped_at),
        ));
        interact_with(&mut world, bed);

        let slot = SaveSlot::open(Some(dir.0.clone()));
        let mut restarted = RunSave::default();
        let mut slot = slot;
        assert!(slot.load_into(
            0,
            &mut restarted,
            test_items(),
            crate::content::test_placements()
        ));
        let save = restarted.checkpoint().expect("saved run");
        assert_eq!(save.inventory.count(ItemKind::Scrap), 3);
        assert_eq!(
            save.equipment.get(EquipmentSlot::Body),
            Some(ItemKind::FieldArmor)
        );
        assert_eq!(save.chests.get(ChestId::Lamp).count(ItemKind::Battery), 1);
        assert_eq!(save.pickups[0].position, dropped_at.to_array());
        assert_eq!(save.pickups[0].stack.quantity, 1);
        assert!(
            save.encounters
                .contains_key(&EncounterId::Added(crate::content::ContentId::new(
                    "relay_roach"
                )))
        );

        let catalog =
            ron::from_str(include_str!("../data/quests/catalog.quest.ron")).expect("quest catalog");
        let journal = QuestJournal::from_snapshot(catalog, WorldLayout::Outpost, &save.journal)
            .expect("saved quest progress");
        assert!(journal.beacon_powered());
        assert!(journal.has_occurred(QuestAction::RecoveredCore));
        world.insert_resource(journal);
        world.insert_resource(restarted);
        let core = world.spawn(Target::Core).id();
        world
            .get_mut::<Health>(corpse)
            .expect("roach health")
            .restore();
        world
            .get_mut::<CorpseLoot>(corpse)
            .expect("roach loot")
            .stack = Some(Pickup {
            item: ItemKind::RoachMeat,
            quantity: 1,
        });
        world
            .run_system_once(restore_actors)
            .expect("restore actors");
        assert!(world.get_entity(core).is_err());
        assert!(world.get::<Dead>(corpse).is_some());
        assert!(world.get::<Collider>(corpse).is_none());
        assert!(
            world
                .get::<CorpseLoot>(corpse)
                .expect("roach loot")
                .stack
                .is_none()
        );
    }

    #[test]
    fn failed_write_preserves_slot_checkpoint_and_health() {
        let dir = TestDirectory::new();
        let (mut world, bed, player, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let previous_file = fs::read(dir.path()).expect("first save");
        let previous_checkpoint = world.resource::<RunSave>().0.clone().expect("checkpoint");
        let previous_day = world.resource::<RespawnClock>().day;

        let bad_destination = dir.0.join("directory");
        fs::write(&bad_destination, b"block").expect("file blocks directory creation");
        world.resource_mut::<SaveSlot>().directory = Some(bad_destination);
        world
            .get_mut::<Health>(player)
            .expect("player health")
            .set_current(19);
        assert!(world.actor_data_mut::<Inventory>().add(ItemKind::Scrap, 2));
        interact_with(&mut world, bed);
        let active = world
            .resource::<RunSave>()
            .0
            .as_ref()
            .expect("retained checkpoint");
        assert!(Arc::ptr_eq(active, &previous_checkpoint));
        assert_eq!(world.resource::<RespawnClock>().day, previous_day);
        assert_eq!(fs::read(dir.path()).expect("previous save"), previous_file);
        assert_eq!(
            world
                .get::<Health>(player)
                .expect("player health")
                .current(),
            19
        );
        assert_eq!(world.resource::<SaveNotice>().text, Some("Could not save."));
        assert_eq!(fs::read_dir(&dir.0).expect("test directory").count(), 2);
    }

    #[test]
    fn death_takes_priority_over_a_bed_request() {
        let dir = TestDirectory::new();
        let (mut world, bed, player, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let previous = fs::read(dir.path()).expect("bed save");
        assert!(
            world
                .actor_data_mut::<Inventory>()
                .add(ItemKind::Battery, 2)
        );
        world.entity_mut(player).insert(Dead);
        world
            .run_system_once(request_game_over)
            .expect("death recovery");
        interact_with(&mut world, bed);
        assert!(matches!(
            *world.resource::<NextState<GameState>>(),
            NextState::Pending(GameState::GameOver)
        ));
        assert_eq!(fs::read(dir.path()).expect("retained save"), previous);
        let save = world
            .resource::<RunSave>()
            .checkpoint()
            .expect("retained checkpoint");
        assert_eq!(save.inventory.count(ItemKind::Battery), 0);
    }

    #[test]
    fn save_with_unsupported_version_or_truncated_payload_is_unavailable() {
        let dir = TestDirectory::new();
        fs::write(
            dir.path(),
            include_bytes!("../tests/fixtures/inspector-v14.save"),
        )
        .expect("write previous version fixture");
        assert!(
            read_save(&dir.path()).is_err(),
            "v14 saves must be rejected"
        );
        fs::remove_file(dir.path()).expect("remove previous version test fixture");
        let (mut world, bed, _, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let mut bytes = fs::read(dir.path()).expect("first save");
        bytes[4] = 14;
        fs::write(dir.path(), bytes).expect("write unsupported version");
        assert!(
            read_save(&dir.path())
                .err()
                .expect("previous version rejected")
                .to_string()
                .contains("Invalid or unsupported save header")
        );
        let slot = SaveSlot::open(Some(dir.0.clone()));
        assert!(slot.available());
        assert!(slot.entries[0].name.is_none());

        fs::write(dir.path(), HEADER).expect("write header only");
        assert!(
            SaveSlot::open(Some(dir.0.clone())).entries[0]
                .name
                .is_none()
        );
    }

    #[test]
    fn bed_save_restores_ranks_points_and_full_derived_pools() {
        use crate::player::Stamina;

        let dir = TestDirectory::new();
        let file = dir.path();
        let (mut world, bed, player, _) = bed_world(file.clone());
        let ranks = AttributeRanks::default()
            .with_rank(AttributeId::Vitality, 12)
            .with_rank(AttributeId::Endurance, 10);
        let progress = CharacterProgress {
            level: crate::level::Level::new(8).expect("valid test level"),
            unspent: 9,
        };
        let xp = Experience(41);
        world
            .entity_mut(player)
            .insert((ranks.clone(), progress, xp));
        interact_with(&mut world, bed);
        let saved = read_save(&dir.path()).expect("read the saved progression");
        let restored = world.spawn((
            Player::default(),
            saved.progression.player_components(),
            saved.player_experience(),
        ));
        assert_eq!(restored.get::<AttributeRanks>(), Some(&ranks));
        assert_eq!(restored.get::<CharacterProgress>(), Some(&progress));
        assert_eq!(restored.get::<Experience>(), Some(&xp));
        assert_eq!(
            restored.get::<Health>().expect("derived health").current(),
            128
        );
        assert_eq!(
            restored.get::<Stamina>().expect("derived stamina").tuning(),
            (120.0, 30.0)
        );
        assert_eq!(
            restored
                .get::<Stamina>()
                .expect("derived stamina")
                .fraction(),
            1.0
        );
    }

    #[test]
    fn current_save_restores_nemi_credit_but_not_disposable_credit() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, corpse) = bed_world(dir.path());
        let nemi = world
            .query_filtered::<Entity, With<Nemi>>()
            .single(&world)
            .expect("Nemi");
        let worm = world
            .query::<(Entity, &EncounterId)>()
            .iter(&world)
            .find_map(|(entity, id)| {
                (*id == EncounterId::Added(crate::content::ContentId::new("near_trail_worm")))
                    .then_some(entity)
            })
            .expect("near trail encounter");
        for actor in [nemi, worm, corpse] {
            world
                .get_mut::<ExperienceReward>(actor)
                .expect("actor XP")
                .player_contributed = true;
        }
        interact_with(&mut world, bed);
        let loaded = read_save(&dir.path()).expect("valid XP checkpoint");
        world.insert_resource(RunSave(Some(Arc::new(loaded))));
        for actor in [nemi, worm] {
            world
                .get_mut::<ExperienceReward>(actor)
                .expect("actor XP")
                .player_contributed = false;
        }
        world
            .run_system_once(restore_actors)
            .expect("restore actors");
        for (actor, credited) in [(nemi, true), (worm, false), (corpse, false)] {
            assert_eq!(
                world
                    .get::<ExperienceReward>(actor)
                    .expect("actor XP")
                    .player_contributed,
                credited
            );
        }
    }

    #[test]
    fn guard_checkpoint_restores_hostility_damage_and_looted_defeat() {
        let dir = TestDirectory::new();
        let path = dir.path();
        let (mut world, bed, _, _) = bed_world(path.clone());
        let west = guard_entity(&mut world, GuardPost::West);
        let east = guard_entity(&mut world, GuardPost::East);
        world.resource_mut::<OutpostSecurity>().hostile = true;
        let fight_site = Vec3::new(3.0, 0.0, 1.0);
        world
            .get_mut::<Transform>(west)
            .expect("west pose")
            .translation = fight_site;
        world
            .get_mut::<Transform>(east)
            .expect("east pose")
            .translation = fight_site;
        world
            .get_mut::<Health>(west)
            .expect("west health")
            .set_current(90);
        world
            .get_mut::<Health>(east)
            .expect("east health")
            .set_current(0);
        world.get_mut::<CorpseLoot>(east).expect("east loot").stack = None;
        world
            .get_mut::<ExperienceReward>(west)
            .expect("west XP")
            .player_contributed = true;
        interact_with(&mut world, bed);

        let path = dir.path();
        let saved = read_save(&path).expect("guard checkpoint");
        let (mut rebuilt, _, _, _) = bed_world(path);
        let west = guard_entity(&mut rebuilt, GuardPost::West);
        let east = guard_entity(&mut rebuilt, GuardPost::East);
        rebuilt.insert_resource(RunSave(Some(Arc::new(saved))));
        rebuilt
            .run_system_once(restore_actors)
            .expect("restore guards");
        assert!(rebuilt.resource::<OutpostSecurity>().hostile);
        assert_eq!(
            rebuilt.get::<Health>(west).expect("west health").current(),
            90
        );
        assert_eq!(
            rebuilt
                .get::<Transform>(west)
                .expect("west pose")
                .translation,
            GuardPost::West.home()
        );
        assert!(
            rebuilt
                .get::<ExperienceReward>(west)
                .expect("west XP")
                .player_contributed
        );
        assert_eq!(
            rebuilt
                .get::<Transform>(east)
                .expect("east pose")
                .translation,
            fight_site
        );
        assert!(rebuilt.get::<Dead>(east).is_some());
        assert!(rebuilt.get::<Collider>(east).is_none());
        assert!(
            rebuilt
                .get::<CorpseLoot>(east)
                .expect("east loot")
                .stack
                .is_none()
        );
        assert_eq!(rebuilt.get::<Visibility>(east), Some(&Visibility::Hidden));
    }

    fn guard_entity(world: &mut World, post: GuardPost) -> Entity {
        world
            .query::<(Entity, &EncounterId)>()
            .iter(world)
            .find_map(|(entity, id)| (*id == EncounterId::Guard(post)).then_some(entity))
            .expect("guard fixture")
    }

    #[test]
    fn the_cavern_quest_restores_its_current_step_and_completion_without_repeating_xp() {
        let dir = TestDirectory::new();
        let path = dir.path();
        let (mut world, bed, _, _) = bed_world(path.clone());
        let mut journal = test_relay_journal();
        journal.offer_cavern_report();
        journal.record(QuestAction::ReportedArchiveHushcap);
        for id in [
            crate::content::ContentId::new("cave_first_west_hushcap"),
            crate::content::ContentId::new("cave_first_east_hushcap"),
            crate::content::ContentId::new("cave_split_west_hushcap"),
            crate::content::ContentId::new("cave_split_east_hushcap"),
        ] {
            journal.credit_cavern_defeat(id);
        }
        world.insert_resource(journal);
        interact_with(&mut world, bed);

        let checkpoint = read_save(&dir.path()).expect("read the cave quest checkpoint");
        let inspection =
            crate::save_inspector::inspect(Some(&dir.path())).expect("inspect cavern credits");
        let first_id =
            EncounterId::Added(crate::content::ContentId::new("cave_first_west_hushcap")).key();
        assert!(
            inspection["cavern_defeats"]
                .as_array()
                .expect("credited IDs")
                .iter()
                .any(|id| id == first_id.as_str())
        );
        let mut restored = restore_test_journal(&checkpoint.journal);
        assert_eq!(restored.cavern_defeat_count(), 4);
        restored.credit_cavern_defeat(crate::content::ContentId::new("cave_first_west_hushcap"));
        assert!(!restored.has_occurred(QuestAction::DefeatedFiveCavernHushcaps));
        restored.credit_cavern_defeat(crate::content::ContentId::new("cave_split_far_hushcap"));
        assert_eq!(restored.cavern_defeat_count(), 5);
        assert!(restored.expects(QuestAction::ShowedTovanMycelium));
        assert_eq!(restored.take_completion_xp(), 0);
        restored.record(QuestAction::CaravanArrived);
        restored.record(QuestAction::ShowedTovanMycelium);
        assert!(restored.is_complete(CONTAIN_THE_HUSHCAPS));
        assert_eq!(restored.take_completion_xp(), 150);
        world.insert_resource(restored);
        interact_with(&mut world, bed);

        let checkpoint = read_save(&dir.path()).expect("read the completed cave quest checkpoint");
        let mut restored = restore_test_journal(&checkpoint.journal);
        assert!(restored.is_complete(CONTAIN_THE_HUSHCAPS));
        assert!(restored.has_occurred(QuestAction::CaravanArrived));
        assert_eq!(restored.cavern_defeat_count(), 5);
        assert_eq!(restored.take_completion_xp(), 0);
    }

    fn encounter_entity(world: &mut World, id: ContentId) -> Entity {
        world
            .query::<(Entity, &EncounterId)>()
            .iter(world)
            .find_map(|(entity, found)| (*found == EncounterId::Added(id)).then_some(entity))
            .expect("the cave encounter is in the checkpoint fixture")
    }

    #[test]
    fn bed_checkpoint_restores_damaged_defeated_and_looted_cave_encounters() {
        let path = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(path.path());
        let damaged = encountered_cave_actors(&mut world);
        let [hushcap, worm, roach] = damaged;
        let moved = world
            .get::<Transform>(hushcap)
            .expect("Hushcap pose")
            .translation
            + Vec3::X;
        world
            .get_mut::<Transform>(hushcap)
            .expect("Hushcap pose")
            .translation = moved;
        world
            .get_mut::<Health>(hushcap)
            .expect("Hushcap health")
            .set_current(23);
        world
            .get_mut::<ExperienceReward>(hushcap)
            .expect("Hushcap award")
            .player_contributed = true;
        for defeated in [worm, roach] {
            world
                .get_mut::<Health>(defeated)
                .expect("encounter health")
                .set_current(0);
        }
        world
            .get_mut::<CorpseLoot>(roach)
            .expect("roach corpse loot")
            .stack = None;
        interact_with(&mut world, bed);

        let (mut rebuilt, _, _, _) = bed_world(path.path());
        let [hushcap, worm, roach] = encountered_cave_actors(&mut rebuilt);
        rebuilt.insert_resource(RunSave(Some(Arc::new(
            read_save(&path.path()).expect("load the v6 cave checkpoint"),
        ))));
        rebuilt
            .run_system_once(restore_actors)
            .expect("restore cave encounters");
        assert_eq!(
            rebuilt
                .get::<Transform>(hushcap)
                .expect("restored Hushcap")
                .translation,
            moved
        );
        assert_eq!(
            rebuilt
                .get::<Health>(hushcap)
                .expect("restored health")
                .current(),
            23
        );
        assert!(
            !rebuilt
                .get::<ExperienceReward>(hushcap)
                .expect("restored reward")
                .player_contributed
        );
        for defeated in [worm, roach] {
            assert!(rebuilt.get::<Dead>(defeated).is_some());
            assert!(rebuilt.get::<Collider>(defeated).is_none());
        }
        assert!(
            rebuilt
                .get::<CorpseLoot>(worm)
                .expect("worm corpse")
                .stack
                .is_some()
        );
        assert!(
            rebuilt
                .get::<CorpseLoot>(roach)
                .expect("roach corpse")
                .stack
                .is_none()
        );
        assert_eq!(rebuilt.get::<Visibility>(roach), Some(&Visibility::Hidden));
    }

    fn encountered_cave_actors(world: &mut World) -> [Entity; 3] {
        [
            crate::content::ContentId::new("cave_first_west_hushcap"),
            crate::content::ContentId::new("cave_split_worm"),
            crate::content::ContentId::new("cave_gallery_roach"),
        ]
        .map(|id| encounter_entity(world, id))
    }

    #[test]
    fn title_picker_requires_confirmation_before_deleting_a_save() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let file = dir.path();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .init_asset::<Image>()
            .init_state::<GameState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::controls::Keybinds>()
            .init_resource::<SettingsMenu>()
            .init_resource::<RunSave>()
            .insert_resource(test_items().clone())
            .insert_resource(crate::content::test_placements().clone())
            .insert_resource(SaveSlot::open(Some(dir.0.clone())))
            .add_plugins(TitlePlugin);
        app.update();
        let press = |app: &mut App, key| {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();
        };
        press(&mut app, KeyCode::KeyS);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Delete);
        assert!(file.exists());
        press(&mut app, KeyCode::Escape);
        assert!(file.exists());
        press(&mut app, KeyCode::Delete);
        assert!(file.exists());
        press(&mut app, KeyCode::Enter);
        assert!(!file.exists());
    }

    #[test]
    fn pause_picker_loads_the_selected_save_into_the_loading_state() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        assert!(
            world
                .actor_data_mut::<Inventory>()
                .add(ItemKind::Battery, 3)
        );
        interact_with(&mut world, bed);

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .insert_state(GameState::Paused)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::controls::Keybinds>()
            .init_resource::<SettingsMenu>()
            .init_resource::<RunSave>()
            .insert_resource(test_items().clone())
            .insert_resource(crate::content::test_placements().clone())
            .insert_resource(SaveSlot::open(Some(dir.0.clone())))
            .add_plugins(TitlePlugin);
        app.update();

        app.world_mut()
            .resource_mut::<crate::ui::title::picker::SavePicker>()
            .open();
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        app.update();

        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Loading
        );
        let save = app
            .world()
            .resource::<RunSave>()
            .checkpoint()
            .expect("pause picker loaded the selected checkpoint");
        assert_eq!(save.inventory.count(ItemKind::Battery), 3);
    }

    #[test]
    fn title_mouse_and_keyboard_load_a_saved_run_and_start_leaves_the_slot_intact() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        assert!(
            world
                .actor_data_mut::<Inventory>()
                .add(ItemKind::Battery, 3)
        );
        interact_with(&mut world, bed);

        for control in [Some(KeyCode::KeyS), Some(KeyCode::Tab), None] {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
                .init_asset::<Image>()
                .init_state::<GameState>()
                .init_resource::<ButtonInput<KeyCode>>()
                .init_resource::<crate::controls::Keybinds>()
                .init_resource::<SettingsMenu>()
                .init_resource::<RunSave>()
                .insert_resource(test_items().clone())
                .insert_resource(crate::content::test_placements().clone())
                .insert_resource(SaveSlot::open(Some(dir.0.clone())))
                .add_plugins(TitlePlugin);
            app.update();

            let world = app.world_mut();
            if let Some(key) = control {
                let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
                keys.press(key);
                keys.press(KeyCode::Enter);
            } else {
                let mut query = world.query::<(&Text, &ChildOf)>();
                let load = query
                    .iter(world)
                    .find_map(|(text, parent)| {
                        (text.as_str() == "Load Game").then_some(parent.parent())
                    })
                    .expect("load button exists");
                let window = world.spawn_empty().id();
                let camera = world.spawn_empty().id();
                world.trigger(crate::interaction::pointer_click(load, window, camera));
            }
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Enter);
            app.update();
            app.update();
            assert_eq!(
                *app.world().resource::<State<GameState>>().get(),
                GameState::Loading
            );
            let save = app
                .world()
                .resource::<RunSave>()
                .checkpoint()
                .expect("loaded checkpoint");
            assert_eq!(save.inventory.count(ItemKind::Battery), 3);

            let world = app.world_mut();
            world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
            world
                .resource_mut::<NextState<GameState>>()
                .set(GameState::Title);
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Enter);
            app.update();
            assert!(app.world().resource::<RunSave>().checkpoint().is_some());
            assert!(app.world().resource::<SaveSlot>().available());
            let window = app.world_mut().spawn_empty().id();
            for (logical_key, text) in [
                (bevy::input::keyboard::Key::Enter, None),
                (
                    bevy::input::keyboard::Key::Character("invalid!".into()),
                    Some("invalid!".into()),
                ),
                (
                    bevy::input::keyboard::Key::Character("abcdefghijklmnopq".into()),
                    Some("abcdefghijklmnopq".into()),
                ),
                (bevy::input::keyboard::Key::Enter, None),
            ] {
                app.world_mut()
                    .write_message(bevy::input::keyboard::KeyboardInput {
                        key_code: KeyCode::Enter,
                        logical_key,
                        state: bevy::input::ButtonState::Pressed,
                        text,
                        repeat: false,
                        window,
                    });
                app.update();
                assert_eq!(
                    *app.world().resource::<State<GameState>>().get(),
                    GameState::Title
                );
                assert!(
                    app.world()
                        .resource::<crate::player::NewPlayerName>()
                        .0
                        .is_empty()
                );
            }
            for (logical_key, text) in [
                (
                    bevy::input::keyboard::Key::Character("New Name".into()),
                    Some("New Name".into()),
                ),
                (bevy::input::keyboard::Key::Enter, None),
            ] {
                app.world_mut()
                    .write_message(bevy::input::keyboard::KeyboardInput {
                        key_code: KeyCode::Enter,
                        logical_key,
                        state: bevy::input::ButtonState::Pressed,
                        text,
                        repeat: false,
                        window,
                    });
                app.update();
            }
            assert!(app.world().resource::<RunSave>().checkpoint().is_none());
            assert_eq!(
                app.world().resource::<crate::player::NewPlayerName>().0,
                "New Name"
            );
        }
    }
}
