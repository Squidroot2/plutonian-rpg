use crate::{
    GameState,
    attributes::{AttributeId, AttributeRanks, CharacterProgress, SavedProgress},
    camera::Orbit,
    cave::{self, CaveEncounter},
    combat::{ActorDefeated, CombatSystems, ConfirmedHit, Dead, Health, Hit},
    dialogue::{Dialogue, DialogueClick},
    dungeon::{ARCHIVE_HUSHCAP_HOME, Area, DungeonProgress},
    equipment::{Equipment, EquipmentSlot},
    experience::{Experience, ExperienceNotice, ExperienceReward, ExperienceUpdates},
    hushcap,
    interaction::{ActiveChest, CorpseLoot, InteractionSystems, LootTarget, Nearby, Target},
    inventory::{Inventory, InventoryNotice, ItemKind, Pickup},
    nemi::{self, Nemi, NemiState},
    player::{MovementInput, Player},
    quest::{QuestAction, QuestJournal, SavedJournal},
    roach,
    storage::ChestStorage,
    ui::inventory_ui::InventoryUiState,
    ui::menu_ui::PlayerMenu,
    ui::quest_ui::QuestSelection,
    world::SessionEntity,
    worm,
};
use avian3d::prelude::Collider;
use bevy::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

// Four magic bytes followed by the little-endian save schema version.
const HEADER: &[u8; 6] = b"PLUT\x07\x00";
const V6_HEADER: &[u8; 6] = b"PLUT\x06\x00";
const V5_HEADER: &[u8; 6] = b"PLUT\x05\x00";
const V4_HEADER: &[u8; 6] = b"PLUT\x04\x00";
const V3_HEADER: &[u8; 6] = b"PLUT\x03\x00";
const V2_HEADER: &[u8; 6] = b"PLUT\x02\x00";
const LEGACY_HEADER: &[u8; 6] = b"PLUT\x01\x00";
const MAX_SAVE_BYTES: usize = 1024 * 1024;

#[derive(Component)]
pub(crate) struct Bed;

#[derive(Message)]
pub(crate) struct SaveAtBed(pub(crate) Entity);

#[derive(
    Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum EncounterId {
    NearTrailWorm,
    FarTrailWorm,
    CavernWorm,
    RelayJunctionWorm,
    // Versions 1–4 used this discriminant for the archive worm.
    RelayArchiveHushcap,
    RelayRoach,
    Cave(CaveEncounter),
}

impl EncounterId {
    const BASE: [Self; 6] = [
        Self::NearTrailWorm,
        Self::FarTrailWorm,
        Self::CavernWorm,
        Self::RelayJunctionWorm,
        Self::RelayArchiveHushcap,
        Self::RelayRoach,
    ];

    fn base(archive_health: u16) -> impl ExactSizeIterator<Item = (Self, u16)> {
        Self::BASE.into_iter().map(move |id| {
            let health = match id {
                Self::RelayRoach => roach::CONFIG.body.health,
                Self::RelayArchiveHushcap => archive_health,
                _ => worm::CONFIG.body.health,
            };
            (id, health)
        })
    }
}

// Bitcode derives its enum encoding from the variant count. Keep the original
// six-variant type for both encounter poses and contribution credit in v1–v5.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, bitcode::Decode)]
#[cfg_attr(test, derive(bitcode::Encode))]
enum LegacyEncounterId {
    NearTrailWorm,
    FarTrailWorm,
    CavernWorm,
    RelayJunctionWorm,
    RelayArchiveHushcap,
    RelayRoach,
}

impl From<LegacyEncounterId> for EncounterId {
    fn from(id: LegacyEncounterId) -> Self {
        match id {
            LegacyEncounterId::NearTrailWorm => Self::NearTrailWorm,
            LegacyEncounterId::FarTrailWorm => Self::FarTrailWorm,
            LegacyEncounterId::CavernWorm => Self::CavernWorm,
            LegacyEncounterId::RelayJunctionWorm => Self::RelayJunctionWorm,
            LegacyEncounterId::RelayArchiveHushcap => Self::RelayArchiveHushcap,
            LegacyEncounterId::RelayRoach => Self::RelayRoach,
        }
    }
}

#[derive(bitcode::Encode, bitcode::Decode)]
pub(crate) struct SavedPickup {
    pub(crate) stack: Pickup,
    pub(crate) position: [f32; 3],
}

#[derive(bitcode::Encode, bitcode::Decode)]
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

#[derive(bitcode::Encode, bitcode::Decode)]
struct SavedEncounter {
    actor: SavedActor,
    has_loot: bool,
}

impl SavedEncounter {
    fn fresh(home: Vec3, health: u16) -> Self {
        Self {
            actor: SavedActor {
                position: home.to_array(),
                rotation: Quat::IDENTITY.to_array(),
                health,
            },
            has_loot: true,
        }
    }

    fn is_valid(&self, max_health: u16) -> bool {
        self.actor.is_valid(max_health) && (self.actor.health == 0 || self.has_loot)
    }
}

#[derive(bitcode::Encode, bitcode::Decode)]
pub(crate) struct Snapshot<
    E = Equipment,
    P = SavedProgress,
    X = SavedExperience,
    I: Ord = EncounterId,
> {
    pub(crate) journal: SavedJournal,
    inventory: Inventory,
    equipment: E,
    chests: ChestStorage,
    dungeon: DungeonProgress,
    pub(crate) pickups: Vec<SavedPickup>,
    nemi: SavedActor,
    nemi_state: NemiState,
    encounters: BTreeMap<I, SavedEncounter>,
    pub(crate) progression: P,
    experience: X,
    #[bitcode(skip)]
    pub(crate) add_shortswords_on_spawn: bool,
}

type LegacySnapshot<E = Equipment, P = SavedProgress, X = SavedExperience<LegacyEncounterId>> =
    Snapshot<E, P, X, LegacyEncounterId>;

#[derive(bitcode::Encode, bitcode::Decode)]
pub(crate) struct SavedExperience<I: Ord = EncounterId> {
    player: Experience,
    nemi_contributed: bool,
    // Use stable encounter IDs because checkpoint loading recreates the actors.
    encounter_contributions: BTreeSet<I>,
}

impl<I: Ord> Default for SavedExperience<I> {
    fn default() -> Self {
        Self {
            player: Experience::default(),
            nemi_contributed: false,
            encounter_contributions: BTreeSet::new(),
        }
    }
}

impl From<SavedExperience<LegacyEncounterId>> for SavedExperience {
    fn from(old: SavedExperience<LegacyEncounterId>) -> Self {
        Self {
            player: old.player,
            nemi_contributed: old.nemi_contributed,
            encounter_contributions: old
                .encounter_contributions
                .into_iter()
                .map(EncounterId::from)
                .collect(),
        }
    }
}

// Bitcode packs three slot variants differently from four. ItemKind stays within
// the same packing width as v1, including after appending Mycelium in v5.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, bitcode::Decode)]
#[cfg_attr(test, derive(bitcode::Encode))]
enum LegacyEquipmentSlot {
    Body,
    Helmet,
    Weapon,
}

fn migrate<E>(
    old: LegacySnapshot<E, (), ()>,
    equipment: Equipment,
    add_shortswords_on_spawn: bool,
) -> io::Result<Snapshot> {
    let mut snapshot = upgrade_snapshot(
        old,
        equipment,
        SavedProgress::default(),
        SavedExperience::default(),
        add_shortswords_on_spawn,
    );
    if !snapshot.equipment.valid_slots() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid legacy equipment slots",
        ));
    }
    let strength = snapshot.progression.ranks().rank(AttributeId::Strength);
    for slot in EquipmentSlot::ALL {
        if let Some(item) = snapshot.equipment.get(slot)
            && !item.meets_strength(strength)
        {
            snapshot
                .equipment
                .unequip(&mut snapshot.inventory, slot)
                .map_err(|reason| io::Error::new(io::ErrorKind::InvalidData, reason))?;
        }
    }
    Ok(snapshot)
}

// Unit fields consume no bitcode bytes in schemas that predate the corresponding data.
fn upgrade_snapshot<E, P, X>(
    old: LegacySnapshot<E, P, X>,
    equipment: Equipment,
    progression: SavedProgress,
    experience: SavedExperience,
    add_shortswords_on_spawn: bool,
) -> Snapshot {
    Snapshot {
        journal: old.journal,
        inventory: old.inventory,
        equipment,
        chests: old.chests,
        dungeon: old.dungeon,
        pickups: old.pickups,
        nemi: old.nemi,
        nemi_state: old.nemi_state,
        encounters: old
            .encounters
            .into_iter()
            .map(|(id, encounter)| (id.into(), encounter))
            .collect(),
        progression,
        experience,
        add_shortswords_on_spawn,
    }
}

impl Snapshot {
    pub(crate) fn player_experience(&self) -> Experience {
        self.experience.player
    }

    fn is_valid(&self) -> bool {
        self.is_valid_with_encounters(hushcap::HEALTH, true)
    }

    fn is_valid_with_encounters(&self, archive_health: u16, include_cave: bool) -> bool {
        self.inventory.valid_stacks()
            && self.progression.is_valid()
            && self.experience.player.is_valid(self.progression.level())
            && (!self.experience.nemi_contributed || self.nemi.health > 0)
            && self.experience.encounter_contributions.iter().all(|id| {
                self.encounters
                    .get(id)
                    .is_some_and(|encounter| encounter.actor.health > 0)
            })
            && self.equipment.valid_slots()
            && self.equipment.0.values().all(|item| {
                item.meets_strength(self.progression.ranks().rank(AttributeId::Strength))
            })
            && self.chests.valid_stacks()
            && self.dungeon.is_valid()
            && self.nemi.is_valid(Nemi::MAX_HEALTH)
            && self.nemi_state.is_valid()
            && self.has_expected_encounters(archive_health, include_cave)
            && self.pickups.iter().all(|pickup| {
                pickup.stack.quantity > 0
                    && pickup.stack.item != ItemKind::BeaconCore
                    && Vec3::from_array(pickup.position).is_finite()
            })
    }

    fn has_expected_encounters(&self, archive_health: u16, include_cave: bool) -> bool {
        let valid = |(id, health)| {
            self.encounters
                .get(&id)
                .is_some_and(|encounter| encounter.is_valid(health))
        };
        let mut base = EncounterId::base(archive_health);
        let mut cave =
            cave::encounter_homes().map(|(id, _, health)| (EncounterId::Cave(id), health));
        let count = base.len() + if include_cave { cave.len() } else { 0 };
        self.encounters.len() == count && base.all(valid) && (!include_cave || cave.all(valid))
    }

    fn replace_archive_worm(&mut self) {
        let archive = EncounterId::RelayArchiveHushcap;
        self.experience.encounter_contributions.remove(&archive);
        self.encounters.insert(
            archive,
            SavedEncounter::fresh(ARCHIVE_HUSHCAP_HOME, hushcap::HEALTH),
        );
    }

    fn seed_cave(&mut self) {
        self.encounters
            .extend(cave::encounter_homes().map(|(id, home, health)| {
                (EncounterId::Cave(id), SavedEncounter::fresh(home, health))
            }));
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

#[derive(Resource, Default)]
pub(crate) struct SaveSlot {
    path: Option<PathBuf>,
    saved: Option<Arc<Snapshot>>,
    error: Option<&'static str>,
}

impl SaveSlot {
    fn open(path: Option<PathBuf>) -> Self {
        let mut slot = Self {
            path,
            saved: None,
            error: None,
        };
        if let Some(path) = &slot.path {
            match read_save(path) {
                Ok(save) => slot.saved = Some(Arc::new(save)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => slot.reject(&error),
            }
        }
        slot
    }

    pub(crate) fn available(&self) -> bool {
        self.saved.is_some()
    }

    pub(crate) fn error(&self) -> Option<&'static str> {
        self.error
    }

    pub(crate) fn load_into(&self, run: &mut RunSave) -> bool {
        if let Some(save) = &self.saved {
            run.0 = Some(save.clone());
            return true;
        }
        false
    }

    pub(crate) fn reject(&mut self, error: impl std::fmt::Display) {
        warn!("Could not load save: {error}");
        self.saved = None;
        self.error = Some("Could not load the save file.");
    }

    fn save(&mut self, snapshot: Snapshot, run: &mut RunSave) -> io::Result<()> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| io::Error::other("No save directory"))?;
        write_save(path, &snapshot)?;
        let snapshot = Arc::new(snapshot);
        self.saved = Some(snapshot.clone());
        self.error = None;
        run.0 = Some(snapshot);
        Ok(())
    }
}

fn data_path() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/share"))
        })
        .map(|data| data.join("plutonian-rpg/save.bin"))
}

fn unique_suffix() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}-{}", std::process::id(), now.as_nanos())
}

fn read_save(path: &Path) -> io::Result<Snapshot> {
    let mut file = File::open(path)?.take(MAX_SAVE_BYTES as u64 + 1);
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    if data.len() > MAX_SAVE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Save file is too large",
        ));
    }
    let invalid_data = |error| io::Error::new(io::ErrorKind::InvalidData, error);
    if let Some(payload) = data
        .strip_prefix(HEADER)
        .or_else(|| data.strip_prefix(V6_HEADER))
    {
        let snapshot: Snapshot = bitcode::decode(payload).map_err(invalid_data)?;
        let mut snapshot = validate_contents(snapshot, hushcap::HEALTH, true)?;
        if data.starts_with(V6_HEADER) {
            snapshot.journal.credit_legacy_relay_return();
        }
        return Ok(snapshot);
    }
    let v5 = data.starts_with(V5_HEADER);
    let snapshot = decode_legacy(&data)?;
    let archive_health = if v5 {
        hushcap::HEALTH
    } else {
        worm::CONFIG.body.health
    };
    let mut snapshot = validate_contents(snapshot, archive_health, false)?;
    if !v5 {
        snapshot.replace_archive_worm();
    }
    snapshot.seed_cave();
    snapshot.journal.credit_legacy_relay_return();
    Ok(snapshot)
}

fn decode_legacy(data: &[u8]) -> io::Result<Snapshot> {
    let invalid_data = |error| io::Error::new(io::ErrorKind::InvalidData, error);
    let snapshot = if let Some(payload) = data
        .strip_prefix(V5_HEADER)
        .or_else(|| data.strip_prefix(V4_HEADER))
    {
        let mut old: LegacySnapshot = bitcode::decode(payload).map_err(invalid_data)?;
        let equipment = std::mem::take(&mut old.equipment);
        let progression = std::mem::take(&mut old.progression);
        let experience = std::mem::take(&mut old.experience).into();
        upgrade_snapshot(old, equipment, progression, experience, false)
    } else if let Some(payload) = data.strip_prefix(V3_HEADER) {
        let mut old: LegacySnapshot<Equipment, SavedProgress, ()> =
            bitcode::decode(payload).map_err(invalid_data)?;
        let equipment = std::mem::take(&mut old.equipment);
        let progression = std::mem::take(&mut old.progression);
        upgrade_snapshot(
            old,
            equipment,
            progression,
            SavedExperience::default(),
            false,
        )
    } else if let Some(payload) = data.strip_prefix(V2_HEADER) {
        let mut old: LegacySnapshot<Equipment, (), ()> =
            bitcode::decode(payload).map_err(invalid_data)?;
        let equipment = std::mem::take(&mut old.equipment);
        migrate(old, equipment, false)?
    } else if let Some(payload) = data.strip_prefix(LEGACY_HEADER) {
        let old: LegacySnapshot<BTreeMap<LegacyEquipmentSlot, ItemKind>, (), ()> =
            bitcode::decode(payload).map_err(invalid_data)?;
        let equipment = Equipment(
            old.equipment
                .iter()
                .map(|(&slot, &item)| {
                    let slot = match slot {
                        LegacyEquipmentSlot::Body => EquipmentSlot::Body,
                        LegacyEquipmentSlot::Helmet => EquipmentSlot::Helmet,
                        LegacyEquipmentSlot::Weapon => EquipmentSlot::MainHand,
                    };
                    (slot, item)
                })
                .collect(),
        );
        migrate(old, equipment, true)?
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid or unsupported save header",
        ));
    };
    Ok(snapshot)
}

fn validate_contents(
    snapshot: Snapshot,
    archive_health: u16,
    include_cave: bool,
) -> io::Result<Snapshot> {
    if !snapshot.is_valid_with_encounters(archive_health, include_cave) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid save contents",
        ));
    }
    Ok(snapshot)
}

fn write_save(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    if !snapshot.is_valid() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid save contents",
        ));
    }
    let bytes = bitcode::encode(snapshot);
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
}

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        let path = if self.smoke {
            Some(
                std::env::temp_dir()
                    .join(format!("plutonian-smoke-{}", unique_suffix()))
                    .join("save.bin"),
            )
        } else {
            data_path()
        };
        app.insert_resource(SaveSlot::open(path))
            .init_resource::<RunSave>()
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
                (age_notice, recover_defeated, save_at_bed)
                    .chain()
                    .after(InteractionSystems)
                    .after(CombatSystems::Resolve)
                    .after(ExperienceUpdates)
                    .after(nemi::react_to_player_hit)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

pub(crate) fn recover_defeated(
    players: Query<(), (With<Player>, Added<Dead>)>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !players.is_empty() {
        next.set(GameState::Loading);
    }
}

pub(crate) fn save_at_bed(
    mut requests: MessageReader<SaveAtBed>,
    beds: Query<(), With<Bed>>,
    mut player: Query<
        (
            &Player,
            &mut Health,
            &AttributeRanks,
            &CharacterProgress,
            &Experience,
        ),
        (Without<Dead>, Without<Nemi>, Without<EncounterId>),
    >,
    nemi: Query<(&Transform, &Health, &Nemi, &ExperienceReward)>,
    encounters: Query<(
        &EncounterId,
        &Transform,
        &Health,
        &CorpseLoot,
        &ExperienceReward,
    )>,
    pickups: Query<(&Pickup, &Transform)>,
    next: Res<NextState<GameState>>,
    journal: Res<QuestJournal>,
    inventory: Res<Inventory>,
    equipment: Res<Equipment>,
    chests: Res<ChestStorage>,
    dungeon: Res<DungeonProgress>,
    mut slot: ResMut<SaveSlot>,
    mut run: ResMut<RunSave>,
    mut notice: ResMut<SaveNotice>,
) {
    if !requests.read().any(|request| beds.contains(request.0))
        || !matches!(*next, NextState::Unchanged)
    {
        return;
    }
    let Ok((player, mut player_health, ranks, progress, experience)) = player.single_mut() else {
        return;
    };
    let Ok((pose, health, nemi, nemi_reward)) = nemi.single() else {
        return;
    };
    if player.area != Area::Surface {
        return;
    }
    let snapshot = Snapshot {
        journal: journal.snapshot(),
        inventory: inventory.clone(),
        equipment: equipment.clone(),
        chests: chests.clone(),
        dungeon: *dungeon,
        pickups: pickups
            .iter()
            .map(|(&stack, transform)| SavedPickup {
                stack,
                position: transform.translation.to_array(),
            })
            .collect(),
        nemi: SavedActor::capture(pose, health),
        nemi_state: nemi.snapshot(),
        encounters: encounters
            .iter()
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
            nemi_contributed: health.current() > 0 && nemi_reward.player_contributed,
            encounter_contributions: encounters
                .iter()
                .filter(|(_, _, health, _, reward)| {
                    health.current() > 0 && reward.player_contributed
                })
                .map(|(&id, ..)| id)
                .collect(),
        },
        add_shortswords_on_spawn: false,
    };
    let text = match slot.save(snapshot, &mut run) {
        Ok(()) => {
            player_health.restore();
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

pub(crate) fn reset_session(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<SessionEntity>>();
    let roots: Vec<_> = query.iter(world).collect();
    for root in roots {
        world.despawn(root);
    }
    let checkpoint = world.resource::<RunSave>().0.clone();
    if let Some(save) = checkpoint {
        world.insert_resource(save.inventory.clone());
        world.insert_resource(save.equipment.clone());
        world.insert_resource(save.chests.clone());
        world.insert_resource(save.dungeon);
    } else {
        world.insert_resource(Inventory::default());
        world.insert_resource(Equipment::default());
        world.insert_resource(ChestStorage::default());
        world.insert_resource(DungeonProgress::default());
    }
    world.insert_resource(Dialogue::default());
    world.insert_resource(DialogueClick::default());
    world.insert_resource(Nearby::default());
    world.insert_resource(LootTarget::default());
    world.insert_resource(ActiveChest::default());
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
    journal: Res<QuestJournal>,
    mut nemi: Query<
        (
            Entity,
            &mut Transform,
            &mut Health,
            &mut Nemi,
            &mut ExperienceReward,
        ),
        Without<EncounterId>,
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
    if let Ok((entity, mut transform, mut health, mut nemi, mut reward)) = nemi.single_mut() {
        save.nemi
            .restore(&mut transform, &mut health, &mut commands.entity(entity));
        nemi.restore(&save.nemi_state);
        reward.player_contributed = save.experience.nemi_contributed;
    }
    for (entity, id, mut transform, mut health, mut loot, mut reward) in &mut encounters {
        if let Some(encounter) = save.encounters.get(id) {
            encounter
                .actor
                .restore(&mut transform, &mut health, &mut commands.entity(entity));
            reward.player_contributed = save.experience.encounter_contributions.contains(id);
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
            Self(std::env::temp_dir().join(format!("plutonian-save-test-{}", unique_suffix())))
        }

        fn path(&self) -> PathBuf {
            self.0.join("save.bin")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn bed_world(path: PathBuf) -> (World, Entity, Entity, Entity) {
        let mut world = World::new();
        world.insert_resource(SaveSlot::open(Some(path)));
        world.insert_resource(test_journal());
        world.init_resource::<RunSave>();
        world.init_resource::<SaveNotice>();
        world.init_resource::<NextState<GameState>>();
        world.init_resource::<Messages<SaveAtBed>>();
        world.init_resource::<Inventory>();
        world.init_resource::<Equipment>();
        world.init_resource::<ChestStorage>();
        world.init_resource::<DungeonProgress>();

        let bed = world.spawn(Bed).id();
        let player = world.spawn(Player::default()).id();
        world
            .get_mut::<Health>(player)
            .expect("player health")
            .set_current(21);
        world.spawn((Nemi::default(), Transform::default(), Collider::sphere(0.5)));
        let mut corpse = Entity::PLACEHOLDER;
        use EncounterId::*;
        let base = EncounterId::base(hushcap::HEALTH).map(|(id, health)| (id, Vec3::ZERO, health));
        let cave = cave::encounter_homes().map(|(id, home, health)| (Cave(id), home, health));
        for (id, home, health) in base.chain(cave) {
            let item = match id {
                RelayRoach | Cave(CaveEncounter::SplitRoach | CaveEncounter::GalleryRoach) => {
                    ItemKind::RoachMeat
                }
                RelayArchiveHushcap
                | Cave(
                    CaveEncounter::FirstWestHushcap
                    | CaveEncounter::FirstEastHushcap
                    | CaveEncounter::SplitWestHushcap
                    | CaveEncounter::SplitEastHushcap
                    | CaveEncounter::SplitFarHushcap
                    | CaveEncounter::GalleryWestHushcap
                    | CaveEncounter::GalleryEastHushcap
                    | CaveEncounter::GalleryFarHushcap,
                ) => ItemKind::Mycelium,
                _ => ItemKind::WormMeat,
            };
            let mut entity = world.spawn((
                id,
                Transform::from_translation(home),
                Health::new(health),
                CorpseLoot::new(item, Area::Surface),
                ExperienceReward::new(10),
                Collider::sphere(0.5),
            ));
            if id == RelayRoach {
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

    fn interact_with(world: &mut World, entity: Entity) {
        world.resource_mut::<Messages<SaveAtBed>>().clear();
        world.write_message(SaveAtBed(entity));
        world.run_system_once(save_at_bed).expect("bed save system");
    }

    fn legacy_id(id: EncounterId) -> Option<LegacyEncounterId> {
        Some(match id {
            EncounterId::NearTrailWorm => LegacyEncounterId::NearTrailWorm,
            EncounterId::FarTrailWorm => LegacyEncounterId::FarTrailWorm,
            EncounterId::CavernWorm => LegacyEncounterId::CavernWorm,
            EncounterId::RelayJunctionWorm => LegacyEncounterId::RelayJunctionWorm,
            EncounterId::RelayArchiveHushcap => LegacyEncounterId::RelayArchiveHushcap,
            EncounterId::RelayRoach => LegacyEncounterId::RelayRoach,
            EncounterId::Cave(_) => return None,
        })
    }

    fn base_encounters(
        encounters: BTreeMap<EncounterId, SavedEncounter>,
    ) -> BTreeMap<LegacyEncounterId, SavedEncounter> {
        encounters
            .into_iter()
            .filter_map(|(id, encounter)| Some((legacy_id(id)?, encounter)))
            .collect()
    }

    fn legacy_encounters(
        encounters: BTreeMap<EncounterId, SavedEncounter>,
    ) -> BTreeMap<LegacyEncounterId, SavedEncounter> {
        let mut encounters = base_encounters(encounters);
        let archive = encounters
            .get_mut(&LegacyEncounterId::RelayArchiveHushcap)
            .expect("the archive worm occupied the same save slot");
        archive.actor.health = worm::CONFIG.body.health;
        archive.actor.position = [3.65, -9.44, -32.0];
        encounters
    }

    fn legacy_snapshot(base: Snapshot, archive_worm: bool) -> LegacySnapshot {
        let encounters = if archive_worm {
            legacy_encounters(base.encounters)
        } else {
            base_encounters(base.encounters)
        };
        Snapshot {
            journal: base.journal,
            inventory: base.inventory,
            equipment: base.equipment,
            chests: base.chests,
            dungeon: base.dungeon,
            pickups: base.pickups,
            nemi: base.nemi,
            nemi_state: base.nemi_state,
            encounters,
            progression: base.progression,
            experience: SavedExperience {
                player: base.experience.player,
                nemi_contributed: base.experience.nemi_contributed,
                encounter_contributions: base
                    .experience
                    .encounter_contributions
                    .into_iter()
                    .filter_map(legacy_id)
                    .collect(),
            },
            add_shortswords_on_spawn: false,
        }
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
            .resource_mut::<Equipment>()
            .equip(&mut inventory, ItemKind::FieldArmor, 10)
            .expect("equip armor");
        world.insert_resource(inventory);
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

        let slot = SaveSlot::open(Some(dir.path()));
        let mut restarted = RunSave::default();
        assert!(slot.load_into(&mut restarted));
        let save = restarted.checkpoint().expect("saved run");
        assert_eq!(save.inventory.count(ItemKind::Scrap), 3);
        assert_eq!(
            save.equipment.get(EquipmentSlot::Body),
            Some(ItemKind::FieldArmor)
        );
        assert_eq!(save.chests.get(ChestId::Lamp).count(ItemKind::Battery), 1);
        assert_eq!(save.pickups[0].position, dropped_at.to_array());
        assert_eq!(save.pickups[0].stack.quantity, 1);
        assert!(save.encounters.contains_key(&EncounterId::RelayRoach));

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

        let bad_destination = dir.0.join("directory");
        fs::create_dir(&bad_destination).expect("directory blocks atomic replacement");
        world.resource_mut::<SaveSlot>().path = Some(bad_destination);
        world
            .get_mut::<Health>(player)
            .expect("player health")
            .set_current(19);
        assert!(world.resource_mut::<Inventory>().add(ItemKind::Scrap, 2));
        interact_with(&mut world, bed);
        let active = world
            .resource::<RunSave>()
            .0
            .as_ref()
            .expect("retained checkpoint");
        assert!(Arc::ptr_eq(active, &previous_checkpoint));
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
        assert!(world.resource_mut::<Inventory>().add(ItemKind::Battery, 2));
        world.entity_mut(player).insert(Dead);
        world
            .run_system_once(recover_defeated)
            .expect("death recovery");
        interact_with(&mut world, bed);
        assert!(matches!(
            *world.resource::<NextState<GameState>>(),
            NextState::Pending(GameState::Loading)
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
        let (mut world, bed, _, _) = bed_world(dir.path());
        interact_with(&mut world, bed);
        let mut bytes = fs::read(dir.path()).expect("first save");
        bytes[4] += 1;
        fs::write(dir.path(), bytes).expect("write unsupported version");
        let slot = SaveSlot::open(Some(dir.path()));
        assert!(!slot.available());
        assert!(slot.error().is_some());

        fs::write(dir.path(), HEADER).expect("write header only");
        assert!(!SaveSlot::open(Some(dir.path())).available());
    }

    #[test]
    fn v1_equipment_loads_into_main_hand_and_new_saves_persist_both_swords() {
        use EquipmentSlot::{MainHand, OffHand};
        use ItemKind::{FieldArmor, FieldHelmet, FieldSpear, Shortsword};

        let dir = TestDirectory::new();
        let file = dir.path();
        let (mut world, bed, _, _) = bed_world(file.clone());
        interact_with(&mut world, bed);
        let base = read_save(&file).expect("read initial save");
        let legacy = Snapshot {
            journal: base.journal,
            inventory: base.inventory,
            equipment: BTreeMap::from([
                (LegacyEquipmentSlot::Body, FieldArmor),
                (LegacyEquipmentSlot::Helmet, FieldHelmet),
                (LegacyEquipmentSlot::Weapon, FieldSpear),
            ]),
            chests: base.chests,
            dungeon: base.dungeon,
            pickups: base.pickups,
            nemi: base.nemi,
            nemi_state: base.nemi_state,
            encounters: legacy_encounters(base.encounters),
            progression: (),
            experience: (),
            add_shortswords_on_spawn: false,
        };
        let mut bytes = LEGACY_HEADER.to_vec();
        bytes.extend(bitcode::encode(&legacy));
        fs::write(&file, bytes).expect("write legacy save");

        let mut migrated = read_save(&file).expect("migrate legacy save");
        assert_eq!(migrated.equipment.get(MainHand), Some(FieldSpear));
        assert_eq!(migrated.equipment.get(OffHand), None);
        assert_eq!(
            migrated.equipment.get(EquipmentSlot::Body),
            Some(FieldArmor)
        );
        assert_eq!(
            migrated.equipment.get(EquipmentSlot::Helmet),
            Some(FieldHelmet)
        );
        assert!(migrated.add_shortswords_on_spawn);

        assert!(migrated.inventory.add(Shortsword, 2));
        for _ in 0..2 {
            assert_eq!(
                migrated
                    .equipment
                    .equip(&mut migrated.inventory, Shortsword, 10),
                Ok(true)
            );
        }
        write_save(&file, &migrated).expect("save dual-wield loadout");
        let saved = read_save(&file).expect("read the current loadout");
        assert!(fs::read(&file).expect("new save bytes").starts_with(HEADER));
        assert_eq!(saved.equipment.get(MainHand), Some(Shortsword));
        assert_eq!(saved.equipment.get(OffHand), Some(Shortsword));
        assert_eq!(saved.inventory.count(FieldSpear), 1);
        assert_eq!(saved.inventory.count(Shortsword), 0);
        assert!(!saved.add_shortswords_on_spawn);
    }

    #[test]
    fn v2_migration_returns_ineligible_gear_and_preserves_eligible_items() {
        use ItemKind::{FieldHelmet, ReinforcedArmor, Trident};

        let dir = TestDirectory::new();
        let file = dir.path();
        let (mut world, bed, _, _) = bed_world(file.clone());
        interact_with(&mut world, bed);
        let base = read_save(&file).expect("read the fixture save");
        let legacy = Snapshot {
            journal: base.journal,
            inventory: base.inventory,
            equipment: Equipment(BTreeMap::from([
                (EquipmentSlot::Body, ReinforcedArmor),
                (EquipmentSlot::Helmet, FieldHelmet),
                (EquipmentSlot::MainHand, Trident),
            ])),
            chests: base.chests,
            dungeon: base.dungeon,
            pickups: base.pickups,
            nemi: base.nemi,
            nemi_state: base.nemi_state,
            encounters: legacy_encounters(base.encounters),
            progression: (),
            experience: (),
            add_shortswords_on_spawn: false,
        };
        let mut bytes = V2_HEADER.to_vec();
        bytes.extend(bitcode::encode(&legacy));
        fs::write(&file, &bytes).expect("write legacy v2 save");

        let saved = read_save(&file).expect("migrate v2 save");
        assert_eq!(
            saved.equipment.get(EquipmentSlot::Helmet),
            Some(FieldHelmet)
        );
        assert_eq!(saved.equipment.get(EquipmentSlot::MainHand), None);
        assert_eq!(saved.equipment.get(EquipmentSlot::Body), None);
        assert_eq!(saved.inventory.count(Trident), 1);
        assert_eq!(saved.inventory.count(ReinforcedArmor), 1);
        assert_eq!(saved.progression.ranks(), AttributeRanks::default());
        assert_eq!(fs::read(&file).expect("read original bytes"), bytes);
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
            level: 8,
            unspent: 9,
        };
        let xp = Experience(41);
        world
            .entity_mut(player)
            .insert((ranks.clone(), progress, xp));
        interact_with(&mut world, bed);
        let saved = read_save(&file).expect("read the saved progression");
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
    fn current_save_restores_damage_credit_for_living_encounters_and_nemi() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, corpse) = bed_world(dir.path());
        let nemi = world
            .query_filtered::<Entity, With<Nemi>>()
            .single(&world)
            .expect("Nemi");
        let worm = world
            .query::<(Entity, &EncounterId)>()
            .iter(&world)
            .find_map(|(entity, id)| (*id == EncounterId::NearTrailWorm).then_some(entity))
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
        for (actor, credited) in [(nemi, true), (worm, true), (corpse, false)] {
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
    fn v3_migration_keeps_levels_and_attributes_and_starts_at_zero_xp() {
        let dir = TestDirectory::new();
        let path = dir.path();
        let (mut world, bed, player, _) = bed_world(path.clone());
        let ranks = AttributeRanks::default().with_rank(AttributeId::Vitality, 7);
        let progress = CharacterProgress {
            level: 4,
            unspent: 7,
        };
        world.entity_mut(player).insert((ranks.clone(), progress));
        interact_with(&mut world, bed);
        let base = read_save(&path).expect("read the fixture save");
        let legacy = Snapshot {
            journal: base.journal,
            inventory: base.inventory,
            equipment: base.equipment,
            chests: base.chests,
            dungeon: base.dungeon,
            pickups: base.pickups,
            nemi: base.nemi,
            nemi_state: base.nemi_state,
            encounters: legacy_encounters(base.encounters),
            progression: base.progression,
            experience: (),
            add_shortswords_on_spawn: false,
        };
        let mut bytes = V3_HEADER.to_vec();
        bytes.extend(bitcode::encode(&legacy));
        fs::write(&path, &bytes).expect("write v3 save");
        let migrated = read_save(&path).expect("migrate v3 save");
        assert_eq!(migrated.player_experience(), Experience(0));
        assert!(!migrated.experience.nemi_contributed);
        assert!(migrated.experience.encounter_contributions.is_empty());
        let player = world.spawn(migrated.progression.player_components());
        assert_eq!(player.get::<AttributeRanks>(), Some(&ranks));
        assert_eq!(player.get::<CharacterProgress>(), Some(&progress));
        assert_eq!(fs::read(path).expect("unaltered v3 save"), bytes);
    }

    #[test]
    fn v4_migration_replaces_the_archive_worm_and_keeps_other_progress() {
        let dir = TestDirectory::new();
        let path = dir.path();
        let (mut world, bed, _, _) = bed_world(path.clone());
        assert!(world.resource_mut::<Inventory>().add(ItemKind::WormMeat, 2));
        interact_with(&mut world, bed);
        let base = read_save(&path).expect("read the fixture save");
        let mut legacy = legacy_snapshot(base, true);
        let archive = EncounterId::RelayArchiveHushcap;
        let junction = EncounterId::RelayJunctionWorm;
        legacy.experience.player = Experience(21);
        legacy.experience.encounter_contributions.extend([
            LegacyEncounterId::RelayArchiveHushcap,
            LegacyEncounterId::RelayJunctionWorm,
        ]);
        let mut bytes = V4_HEADER.to_vec();
        bytes.extend(bitcode::encode(&legacy));
        fs::write(&path, &bytes).expect("write v4 save");

        let fresh = read_save(&path).expect("migrate the v4 archive encounter");
        assert_eq!(fresh.experience.player, Experience(21));
        assert_eq!(fresh.inventory.count(ItemKind::WormMeat), 2);
        assert!(fresh.experience.encounter_contributions == BTreeSet::from([junction]));
        let hushcap = &fresh.encounters[&archive];
        assert_eq!(hushcap.actor.position, ARCHIVE_HUSHCAP_HOME.to_array());
        assert_eq!(hushcap.actor.health, crate::hushcap::HEALTH);
        assert!(hushcap.has_loot);
        assert_eq!(fs::read(&path).expect("unaltered v4 save"), bytes);

        let expired = legacy
            .encounters
            .get_mut(&LegacyEncounterId::RelayArchiveHushcap)
            .expect("the saved worm");
        expired.actor.health = 0;
        expired.has_loot = false;
        legacy
            .experience
            .encounter_contributions
            .remove(&LegacyEncounterId::RelayArchiveHushcap);
        let mut bytes = V4_HEADER.to_vec();
        bytes.extend(bitcode::encode(&legacy));
        fs::write(&path, bytes).expect("write v4 save with a looted worm");
        let fresh = read_save(&path).expect("a looted worm still becomes a living Hushcap");
        assert!(fresh.encounters[&archive].has_loot);
        assert_eq!(
            fresh.encounters[&archive].actor.health,
            crate::hushcap::HEALTH
        );
    }

    #[test]
    fn v5_migration_seeds_the_cave_and_preserves_the_existing_archive_hushcap() {
        let dir = TestDirectory::new();
        let path = dir.path();
        let (mut world, bed, _, _) = bed_world(path.clone());
        interact_with(&mut world, bed);
        let mut legacy = legacy_snapshot(read_save(&path).expect("fixture save"), false);
        let archive = LegacyEncounterId::RelayArchiveHushcap;
        let saved = legacy
            .encounters
            .get_mut(&archive)
            .expect("v5 archive encounter");
        saved.actor.health = hushcap::HEALTH - 1;
        saved.actor.position = ARCHIVE_HUSHCAP_HOME.to_array();
        legacy.experience.encounter_contributions.insert(archive);
        let mut bytes = V5_HEADER.to_vec();
        bytes.extend(bitcode::encode(&legacy));
        fs::write(&path, &bytes).expect("write the v5 fixture");

        let migrated = read_save(&path).expect("read v5 with the cave expansion installed");
        let archive = EncounterId::RelayArchiveHushcap;
        assert_eq!(
            migrated.encounters[&archive].actor.health,
            hushcap::HEALTH - 1
        );
        assert!(
            migrated
                .experience
                .encounter_contributions
                .contains(&archive)
        );
        assert_eq!(migrated.encounters.len(), 18);
        for (id, home, health) in cave::encounter_homes() {
            let encounter = &migrated.encounters[&EncounterId::Cave(id)];
            assert_eq!(encounter.actor.position, home.to_array());
            assert_eq!(encounter.actor.health, health);
            assert!(encounter.has_loot);
        }
        assert_eq!(fs::read(path).expect("v5 file remains on disk"), bytes);
    }

    #[test]
    fn only_legacy_checkpoints_infer_the_relay_return() {
        let dir = TestDirectory::new();
        let path = dir.path();
        let (mut world, bed, _, _) = bed_world(path.clone());
        for attacked in [false, true] {
            let mut journal = test_relay_journal();
            if attacked {
                journal.record(QuestAction::AttackedNemi);
            }
            world.insert_resource(journal);
            interact_with(&mut world, bed);
            let current = read_save(&path).expect("read v7 with no hatch return");
            assert_eq!(
                restore_test_journal(&current.journal).status(CONTAIN_THE_HUSHCAPS),
                None
            );

            let mut bytes = V6_HEADER.to_vec();
            bytes.extend(bitcode::encode(&current));
            fs::write(&path, &bytes).expect("write the v6 fixture");
            let migrated = read_save(&path).expect("read v6 without exit history");
            let mut journal = restore_test_journal(&migrated.journal);
            assert_eq!(
                journal.expects(QuestAction::ReportedArchiveHushcap),
                !attacked
            );
            assert_eq!(journal.take_completion_xp(), 0);
            assert_eq!(
                fs::read(&path).expect("migration leaves disk untouched"),
                bytes
            );
        }
    }

    #[test]
    fn the_cavern_quest_restores_its_current_step_and_completion_without_repeating_xp() {
        let dir = TestDirectory::new();
        let path = dir.path();
        let (mut world, bed, _, _) = bed_world(path.clone());
        let mut journal = test_relay_journal();
        journal.offer_cavern_report();
        journal.record(QuestAction::ReportedArchiveHushcap);
        journal.record(QuestAction::DefeatedFiveCavernHushcaps);
        world.insert_resource(journal);
        interact_with(&mut world, bed);

        let checkpoint = read_save(&path).expect("read the cave quest checkpoint");
        let mut restored = restore_test_journal(&checkpoint.journal);
        assert!(restored.expects(QuestAction::ShowedNemiMycelium));
        assert_eq!(restored.take_completion_xp(), 0);
        restored.record(QuestAction::ShowedNemiMycelium);
        assert!(restored.is_complete(CONTAIN_THE_HUSHCAPS));
        assert_eq!(restored.take_completion_xp(), 150);
        world.insert_resource(restored);
        interact_with(&mut world, bed);

        let checkpoint = read_save(&path).expect("read the completed cave quest checkpoint");
        let mut restored = restore_test_journal(&checkpoint.journal);
        assert!(restored.is_complete(CONTAIN_THE_HUSHCAPS));
        assert_eq!(restored.take_completion_xp(), 0);
    }

    fn encounter_entity(world: &mut World, id: CaveEncounter) -> Entity {
        world
            .query::<(Entity, &EncounterId)>()
            .iter(world)
            .find_map(|(entity, found)| (*found == EncounterId::Cave(id)).then_some(entity))
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
            rebuilt
                .get::<ExperienceReward>(hushcap)
                .expect("restored XP credit")
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
            CaveEncounter::FirstWestHushcap,
            CaveEncounter::SplitWorm,
            CaveEncounter::GalleryRoach,
        ]
        .map(|id| encounter_entity(world, id))
    }

    #[test]
    fn title_mouse_and_keyboard_load_a_saved_run_and_start_leaves_the_slot_intact() {
        let dir = TestDirectory::new();
        let (mut world, bed, _, _) = bed_world(dir.path());
        assert!(world.resource_mut::<Inventory>().add(ItemKind::Battery, 3));
        interact_with(&mut world, bed);

        for control in [Some(KeyCode::ArrowDown), Some(KeyCode::Tab), None] {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
                .init_asset::<Image>()
                .init_state::<GameState>()
                .init_resource::<ButtonInput<KeyCode>>()
                .init_resource::<SettingsMenu>()
                .init_resource::<RunSave>()
                .insert_resource(SaveSlot::open(Some(dir.path())))
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
                world.trigger(crate::smoke::pointer_click(load, window, camera));
            }
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
            assert!(app.world().resource::<RunSave>().checkpoint().is_none());
            assert!(app.world().resource::<SaveSlot>().available());
        }
    }
}
