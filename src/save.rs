use crate::{
    GameState,
    camera::Orbit,
    combat::{CombatSystems, ConfirmedHit, Dead, Health, Hit},
    dungeon::{Area, DungeonProgress},
    equipment::{Equipment, EquipmentSlot},
    interaction::{ActiveChest, CorpseLoot, InteractionSystems, LootTarget, Nearby, Target},
    inventory::{Inventory, InventoryNotice, ItemKind, Pickup},
    inventory_ui::InventoryUiState,
    menu_ui::PlayerMenu,
    nemi::{self, Nemi, NemiState},
    player::{MovementInput, Player},
    quest::{Dialogue, QuestAction, QuestJournal, SavedJournal},
    quest_ui::QuestSelection,
    roach,
    storage::ChestStorage,
    weapon::WeaponAttack,
    world::SessionEntity,
    worm,
};
use avian3d::prelude::Collider;
use bevy::prelude::*;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

// Four magic bytes followed by the little-endian save schema version.
const HEADER: &[u8; 6] = b"PLUT\x02\x00";
const LEGACY_HEADER: &[u8; 6] = b"PLUT\x01\x00";
const MAX_SAVE_BYTES: usize = 1024 * 1024;

#[derive(Component)]
pub(crate) struct Bed;

#[derive(Message)]
pub(crate) struct SaveAtBed(pub(crate) Entity);

#[derive(
    Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum EncounterId {
    NearTrailWorm,
    FarTrailWorm,
    CavernWorm,
    RelayJunctionWorm,
    RelayArchiveWorm,
    RelayRoach,
}

impl EncounterId {
    fn max_health(self) -> u16 {
        match self {
            Self::RelayRoach => roach::CONFIG.health,
            _ => worm::CONFIG.health,
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

#[derive(bitcode::Encode, bitcode::Decode)]
pub(crate) struct Snapshot<E = Equipment> {
    pub(crate) journal: SavedJournal,
    inventory: Inventory,
    equipment: E,
    chests: ChestStorage,
    dungeon: DungeonProgress,
    pub(crate) pickups: Vec<SavedPickup>,
    nemi: SavedActor,
    nemi_state: NemiState,
    encounters: BTreeMap<EncounterId, SavedEncounter>,
    #[bitcode(skip)]
    pub(crate) add_shortswords_on_spawn: bool,
}

// Bitcode packs three slot variants differently from four. ItemKind stays within
// the same packing width as v1, and its original variants keep their order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, bitcode::Decode)]
#[cfg_attr(test, derive(bitcode::Encode))]
enum LegacyEquipmentSlot {
    Body,
    Helmet,
    Weapon,
}

impl From<Snapshot<BTreeMap<LegacyEquipmentSlot, ItemKind>>> for Snapshot {
    fn from(old: Snapshot<BTreeMap<LegacyEquipmentSlot, ItemKind>>) -> Self {
        Self {
            journal: old.journal,
            inventory: old.inventory,
            equipment: Equipment(
                old.equipment
                    .into_iter()
                    .map(|(slot, item)| {
                        let slot = match slot {
                            LegacyEquipmentSlot::Body => EquipmentSlot::Body,
                            LegacyEquipmentSlot::Helmet => EquipmentSlot::Helmet,
                            LegacyEquipmentSlot::Weapon => EquipmentSlot::MainHand,
                        };
                        (slot, item)
                    })
                    .collect(),
            ),
            chests: old.chests,
            dungeon: old.dungeon,
            pickups: old.pickups,
            nemi: old.nemi,
            nemi_state: old.nemi_state,
            encounters: old.encounters,
            add_shortswords_on_spawn: true,
        }
    }
}

impl Snapshot {
    fn is_valid(&self) -> bool {
        self.inventory.valid_stacks()
            && self.equipment.valid_slots()
            && self.chests.valid_stacks()
            && self.dungeon.is_valid()
            && self.nemi.is_valid(Nemi::MAX_HEALTH)
            && self.nemi_state.is_valid()
            && self.encounters.len() == 6
            && self.encounters.iter().all(|(&id, encounter)| {
                encounter.actor.is_valid(id.max_health())
                    && (encounter.actor.health == 0 || encounter.has_loot)
            })
            && self.pickups.iter().all(|pickup| {
                pickup.stack.quantity > 0
                    && pickup.stack.item != ItemKind::BeaconCore
                    && Vec3::from_array(pickup.position).is_finite()
            })
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
    let snapshot: Snapshot = if let Some(payload) = data.strip_prefix(HEADER) {
        bitcode::decode(payload)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    } else if let Some(payload) = data.strip_prefix(LEGACY_HEADER) {
        let legacy: Snapshot<BTreeMap<LegacyEquipmentSlot, ItemKind>> = bitcode::decode(payload)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        legacy.into()
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid or unsupported save header",
        ));
    };
    if !snapshot.is_valid() {
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
    mut player: Query<(&Player, &mut Health), (Without<Dead>, Without<Nemi>, Without<EncounterId>)>,
    nemi: Query<(&Transform, &Health, &Nemi)>,
    encounters: Query<(&EncounterId, &Transform, &Health, &CorpseLoot)>,
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
    let Ok((player, mut player_health)) = player.single_mut() else {
        return;
    };
    let Ok((pose, health, nemi)) = nemi.single() else {
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
            .map(|(&id, transform, health, loot)| {
                (
                    id,
                    SavedEncounter {
                        actor: SavedActor::capture(transform, health),
                        has_loot: loot.stack.is_some(),
                    },
                )
            })
            .collect(),
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
    world.insert_resource(Nearby::default());
    world.insert_resource(LootTarget::default());
    world.insert_resource(ActiveChest::default());
    world.insert_resource(InventoryNotice::default());
    world.insert_resource(SaveNotice::default());
    world.insert_resource(PlayerMenu::default());
    world.insert_resource(InventoryUiState::default());
    world.insert_resource(QuestSelection::default());
    world.insert_resource(MovementInput::default());
    world.insert_resource(Orbit::default());
    world.resource_mut::<WeaponAttack>().cancel();
    world.resource_mut::<Messages<Hit>>().clear();
    world.resource_mut::<Messages<ConfirmedHit>>().clear();
}

fn restore_actors(
    run: Res<RunSave>,
    journal: Res<QuestJournal>,
    mut nemi: Query<(Entity, &mut Transform, &mut Health, &mut Nemi), Without<EncounterId>>,
    mut encounters: Query<
        (
            Entity,
            &EncounterId,
            &mut Transform,
            &mut Health,
            &mut CorpseLoot,
        ),
        Without<Nemi>,
    >,
    quest_targets: Query<(Entity, &Target)>,
    mut commands: Commands,
) {
    let Some(save) = run.checkpoint() else {
        return;
    };
    if let Ok((entity, mut transform, mut health, mut nemi)) = nemi.single_mut() {
        save.nemi
            .restore(&mut transform, &mut health, &mut commands.entity(entity));
        nemi.restore(&save.nemi_state);
    }
    for (entity, id, mut transform, mut health, mut loot) in &mut encounters {
        if let Some(encounter) = save.encounters.get(id) {
            encounter
                .actor
                .restore(&mut transform, &mut health, &mut commands.entity(entity));
            if !encounter.has_loot {
                loot.stack = None;
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
        equipment::EquipmentSlot, player::MAX_HEALTH, quest::test_journal, settings::SettingsMenu,
        storage::ChestId, title::TitlePlugin, world::WorldLayout,
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
        for id in [
            NearTrailWorm,
            FarTrailWorm,
            CavernWorm,
            RelayJunctionWorm,
            RelayArchiveWorm,
            RelayRoach,
        ] {
            let item = if id == RelayRoach {
                ItemKind::RoachMeat
            } else {
                ItemKind::WormMeat
            };
            let mut entity = world.spawn((
                id,
                Transform::default(),
                Health::new(id.max_health()),
                CorpseLoot::new(item, Area::Surface),
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
            MAX_HEALTH
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
            .equip(&mut inventory, ItemKind::FieldArmor)
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
    fn v1_equipment_loads_into_main_hand_and_v2_persists_both_swords() {
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
            encounters: base.encounters,
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
                    .equip(&mut migrated.inventory, Shortsword),
                Ok(true)
            );
        }
        write_save(&file, &migrated).expect("save dual-wield loadout");
        let saved = read_save(&file).expect("read the v2 loadout");
        assert!(fs::read(&file).expect("new save bytes").starts_with(HEADER));
        assert_eq!(saved.equipment.get(MainHand), Some(Shortsword));
        assert_eq!(saved.equipment.get(OffHand), Some(Shortsword));
        assert_eq!(saved.inventory.count(FieldSpear), 1);
        assert_eq!(saved.inventory.count(Shortsword), 0);
        assert!(!saved.add_shortswords_on_spawn);
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
