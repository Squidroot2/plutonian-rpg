//! Exercise the shipped relay with the real movement controller and interact input.
use super::{
    SmokeRun, capture,
    inventory::{click, pointer_click, walk},
};
use crate::{
    GameState,
    attributes::{AttributeId, AttributeRanks, CharacterProgress},
    camera::{FollowCamera, Orbit},
    combat::{Dead, Health, Hit},
    dialogue::Dialogue,
    dungeon::{
        ARCHIVE_HUSHCAP_HOME, ArchiveGate, Area, DungeonProgress, DungeonTarget, GATE_LOCAL, HATCH,
        LOG_LOCAL, ORIGIN, SWITCH_LOCAL,
    },
    enemy::{MeleePhase, MeleePursuit, ranged::SpitProjectile},
    equipment::Equipment,
    experience::{Experience, ExperienceNotice},
    hushcap::{self, Hushcap},
    interaction::{CorpseLoot, INTERACT_HEIGHT, INTERACT_RANGE, InteractionTarget, Nearby, Target},
    inventory::{Inventory, ItemKind},
    player::{Dodge, Player},
    quest::{CONTAIN_THE_HUSHCAPS, LAST_LIGHT, OLD_RELAY, QuestAction, QuestJournal, QuestStatus},
    roach::Roach,
    save::{EncounterId, RunSave},
    storage::{ChestId, ChestStorage},
    ui::loot_ui::LootAction,
    ui::menu_ui::{MenuTab, PlayerMenu},
    ui::quest_ui::{QuestRow, QuestSelection},
    ui::storage_ui::{StorageAction, StorageSide, TransferAmount},
    weapon::{GRIP_SIDE, WeaponAttack},
    world::WorldLayout,
    worm::{GROUND_OFFSET, Worm},
};
use avian3d::prelude::*;
use bevy::{
    app::AppExit,
    ecs::query::{QueryFilter, QueryItem, ReadOnlyQueryData},
    prelude::*,
    render::view::screenshot::Screenshot,
};

const BEACON_PROGRESS: CharacterProgress = CharacterProgress {
    level: 2,
    unspent: 3,
};
const RELAY_PROGRESS: CharacterProgress = CharacterProgress {
    level: 3,
    unspent: 6,
};
const LOOTED_RELAY_XP: Experience = Experience(25 + hushcap::EXPERIENCE as u64);

type HushcapData = (
    Entity,
    &'static Transform,
    &'static Health,
    &'static CorpseLoot,
    Has<Dead>,
    Has<Collider>,
);
type HushcapQuery<'w, 's> =
    Query<'w, 's, (HushcapData, &'static EncounterId), (With<Hushcap>, Without<Player>)>;
type RoachData = (Entity, &'static Transform, Has<Dead>);
type RoachQuery<'w, 's> =
    Query<'w, 's, (RoachData, &'static EncounterId), (With<Roach>, Without<Player>)>;

fn relay_encounter<'a, 's, D: ReadOnlyQueryData, F: QueryFilter>(
    encounters: &'a Query<'_, 's, (D, &'static EncounterId), F>,
    id: EncounterId,
) -> QueryItem<'a, 's, D> {
    encounters
        .iter()
        .find_map(|(data, encounter)| (*encounter == id).then_some(data))
        .expect("the shipped relay encounter must be present")
}

#[derive(Resource, Debug, Default)]
pub(super) enum WorldCheck {
    #[default]
    Spawn,
    FreshRespawn(Entity),
    TentView(f64),
    TentPhoto(Entity),
    InspectChest(ChestId),
    TakeFromChest(ChestId),
    StoreInChest(ChestId),
    CloseChest(ChestId),
    NextChest(ChestId),
    LeaveTent,
    OutpostPhoto(Entity),
    OutsideChestWall,
    BlockedChest,
    ReturnToTent,
    InspectStoredChest,
    CheckStoredChest,
    LeaveStorage,
    RecoverCore,
    CheckRecoveredCore,
    InspectLocked,
    LockedNotice,
    OpenDiscoveredJournal,
    CheckDiscoveredJournal,
    CheckRelaySelection,
    SetUpTurnIn,
    OpenEarlyDialogue,
    ChooseNemiLore,
    ReadNemiLore,
    ChooseNivalisLore,
    ReadNivalisLore,
    SelectBeaconTopic,
    TurnIn,
    ApproachHatch,
    Enter,
    LureWorm,
    DodgeWorm(u16),
    Junction,
    ApproachRoach,
    TakeRoachMeat,
    CloseRoachLoot,
    Switch,
    OpenGateJournal,
    GateRises,
    ReturnToJunction,
    GateApproach,
    CrossGate,
    ApproachLog,
    ReadLog,
    ReadNotice,
    OpenCompletedJournal,
    CheckCompletedJournal,
    ApproachHushcap,
    FightHushcap {
        saw_spit: bool,
    },
    TakeMycelium,
    CloseMycelium,
    ClearArchiveCover,
    LeaveArchive,
    LeaveJunction,
    Exit,
    ApproachNemi,
    SelectCavernTopic,
    CavernBriefing,
    OpenCavernJournal,
    SelectCavernQuest,
    CheckCavernJournal,
    CavernJournalPhoto(Entity),
    Reenter,
    Revisited,
    Recover,
    SaveAtBed,
    ChangeAfterSave,
    SavedRespawn(Entity),
}

pub(super) fn drive(
    run: Res<SmokeRun>,
    phase: ResMut<WorldCheck>,
    mut last_log: Local<f64>,
    mut player: Query<
        (
            Entity,
            &mut Player,
            &mut Dodge,
            &mut Transform,
            &Collider,
            &Health,
            &AttributeRanks,
            &CharacterProgress,
            &Experience,
            &WeaponAttack,
        ),
        With<Player>,
    >,
    (quest, inventory, dialogue, nearby, checkpoint): (
        Res<QuestJournal>,
        Res<Inventory>,
        Res<Dialogue>,
        Res<Nearby>,
        Res<RunSave>,
    ),
    (progress, journal, selection, rows, texts, xp_notice): (
        Res<DungeonProgress>,
        Res<PlayerMenu>,
        Res<QuestSelection>,
        Query<(Entity, &'static QuestRow)>,
        Query<&'static Text>,
        Res<ExperienceNotice>,
    ),
    (gate, names, parents, chests): (
        Query<Entity, With<ArchiveGate>>,
        Query<&'static Name>,
        Query<&'static ChildOf>,
        Query<(&'static ChestId, &'static Transform), Without<Player>>,
    ),
    worms: Query<
        (Entity, &'static Transform, &'static MeleePursuit),
        (With<Worm>, Without<Player>),
    >,
    captures: Query<(), With<Screenshot>>,
    (state, roaches, hushcaps, spit, actions, window, camera, storage, storage_actions): (
        Res<State<GameState>>,
        RoachQuery,
        HushcapQuery,
        Query<(), With<SpitProjectile>>,
        Query<(Entity, &'static LootAction)>,
        Query<Entity, With<Window>>,
        Query<Entity, With<FollowCamera>>,
        Res<ChestStorage>,
        Query<(Entity, &'static StorageAction)>,
    ),
    hits: MessageWriter<Hit>,
    spatial: SpatialQuery,
    orbit: ResMut<Orbit>,
    (mut keys, mut mouse): (
        ResMut<ButtonInput<KeyCode>>,
        ResMut<ButtonInput<MouseButton>>,
    ),
    commands: Commands,
    exit: MessageWriter<AppExit>,
) {
    let Ok((
        entity,
        controller,
        dodge,
        transform,
        capsule,
        player_health,
        ranks,
        attributes,
        xp,
        thrust,
    )) = player.single_mut()
    else {
        return;
    };
    keys.reset_all();
    mouse.reset_all();
    let position = transform.translation;
    let now = run.0.elapsed().as_secs_f64();
    if now - *last_log >= 2.0 {
        info!("WORLD SMOKE: {:?} at {position:?}", *phase);
        *last_log = now;
    }
    let frame = WorldFrame {
        phase,
        quest,
        inventory,
        checkpoint,
        names,
        parents,
        worms,
        captures,
        hits,
        spatial,
        orbit,
        commands,
        entity,
        controller,
        dodge,
        transform,
        player_health,
        ranks,
        attributes,
        xp,
        thrust,
        position,
        now,
        storage,
        keys,
        mouse,
        nearby,
        storage_actions,
        window,
        camera,
        state,
        chests,
        dialogue,
        progress,
        journal,
        selection,
        rows,
        texts,
        xp_notice,
        gate,
        capsule,
        roaches,
        hushcaps,
        spit,
        actions,
        run,
        exit,
    };
    match *frame.phase {
        WorldCheck::Spawn
        | WorldCheck::FreshRespawn(..)
        | WorldCheck::TentView(..)
        | WorldCheck::TentPhoto(..) => check_fresh_run(frame),
        WorldCheck::InspectChest(..)
        | WorldCheck::TakeFromChest(..)
        | WorldCheck::StoreInChest(..)
        | WorldCheck::CloseChest(..)
        | WorldCheck::NextChest(..) => check_tent_chests(frame),
        WorldCheck::LeaveTent
        | WorldCheck::OutpostPhoto(..)
        | WorldCheck::OutsideChestWall
        | WorldCheck::BlockedChest
        | WorldCheck::ReturnToTent
        | WorldCheck::InspectStoredChest
        | WorldCheck::CheckStoredChest
        | WorldCheck::LeaveStorage => check_outpost_storage(frame),
        WorldCheck::RecoverCore
        | WorldCheck::CheckRecoveredCore
        | WorldCheck::InspectLocked
        | WorldCheck::LockedNotice
        | WorldCheck::OpenDiscoveredJournal
        | WorldCheck::CheckDiscoveredJournal
        | WorldCheck::CheckRelaySelection => discover_relay(frame),
        WorldCheck::SetUpTurnIn
        | WorldCheck::OpenEarlyDialogue
        | WorldCheck::ChooseNemiLore
        | WorldCheck::ReadNemiLore
        | WorldCheck::ChooseNivalisLore
        | WorldCheck::ReadNivalisLore
        | WorldCheck::SelectBeaconTopic
        | WorldCheck::TurnIn
        | WorldCheck::ApproachHatch
        | WorldCheck::Enter => unlock_relay(frame),
        WorldCheck::LureWorm
        | WorldCheck::DodgeWorm(..)
        | WorldCheck::Junction
        | WorldCheck::ApproachRoach
        | WorldCheck::TakeRoachMeat
        | WorldCheck::CloseRoachLoot
        | WorldCheck::Switch
        | WorldCheck::OpenGateJournal
        | WorldCheck::GateRises => check_relay_encounters(frame),
        WorldCheck::ReturnToJunction | WorldCheck::GateApproach | WorldCheck::CrossGate => {
            enter_archive(frame)
        }
        WorldCheck::ApproachLog
        | WorldCheck::ReadLog
        | WorldCheck::ReadNotice
        | WorldCheck::OpenCompletedJournal
        | WorldCheck::CheckCompletedJournal
        | WorldCheck::LeaveArchive
        | WorldCheck::LeaveJunction
        | WorldCheck::Exit => finish_relay_quest(frame),
        WorldCheck::ApproachNemi
        | WorldCheck::SelectCavernTopic
        | WorldCheck::CavernBriefing
        | WorldCheck::OpenCavernJournal
        | WorldCheck::SelectCavernQuest
        | WorldCheck::CheckCavernJournal
        | WorldCheck::CavernJournalPhoto(..) => check_cavern_briefing(frame),
        WorldCheck::ApproachHushcap
        | WorldCheck::FightHushcap { .. }
        | WorldCheck::TakeMycelium
        | WorldCheck::CloseMycelium
        | WorldCheck::ClearArchiveCover => fight_hushcap(frame),
        WorldCheck::Reenter | WorldCheck::Revisited => check_relay_revisit(frame),
        WorldCheck::Recover
        | WorldCheck::SaveAtBed
        | WorldCheck::ChangeAfterSave
        | WorldCheck::SavedRespawn(..) => check_bed_rollback(frame),
    }
}

struct WorldFrame<'w, 's, 'a> {
    phase: ResMut<'w, WorldCheck>,
    quest: Res<'w, QuestJournal>,
    inventory: Res<'w, Inventory>,
    checkpoint: Res<'w, RunSave>,
    names: Query<'w, 's, &'static Name>,
    parents: Query<'w, 's, &'static ChildOf>,
    worms: Query<
        'w,
        's,
        (Entity, &'static Transform, &'static MeleePursuit),
        (With<Worm>, Without<Player>),
    >,
    captures: Query<'w, 's, (), With<Screenshot>>,
    hits: MessageWriter<'w, Hit>,
    spatial: SpatialQuery<'w, 's>,
    orbit: ResMut<'w, Orbit>,
    commands: Commands<'w, 's>,
    entity: Entity,
    controller: Mut<'a, Player>,
    dodge: Mut<'a, Dodge>,
    transform: Mut<'a, Transform>,
    player_health: &'a Health,
    ranks: &'a AttributeRanks,
    attributes: &'a CharacterProgress,
    xp: &'a Experience,
    thrust: &'a WeaponAttack,
    position: Vec3,
    now: f64,
    storage: Res<'w, ChestStorage>,
    keys: ResMut<'w, ButtonInput<KeyCode>>,
    mouse: ResMut<'w, ButtonInput<MouseButton>>,
    nearby: Res<'w, Nearby>,
    storage_actions: Query<'w, 's, (Entity, &'static StorageAction)>,
    window: Query<'w, 's, Entity, With<Window>>,
    camera: Query<'w, 's, Entity, With<FollowCamera>>,
    state: Res<'w, State<GameState>>,
    chests: Query<'w, 's, (&'static ChestId, &'static Transform), Without<Player>>,
    dialogue: Res<'w, Dialogue>,
    progress: Res<'w, DungeonProgress>,
    journal: Res<'w, PlayerMenu>,
    selection: Res<'w, QuestSelection>,
    rows: Query<'w, 's, (Entity, &'static QuestRow)>,
    texts: Query<'w, 's, &'static Text>,
    xp_notice: Res<'w, ExperienceNotice>,
    gate: Query<'w, 's, Entity, With<ArchiveGate>>,
    capsule: &'a Collider,
    roaches: RoachQuery<'w, 's>,
    hushcaps: HushcapQuery<'w, 's>,
    spit: Query<'w, 's, (), With<SpitProjectile>>,
    actions: Query<'w, 's, (Entity, &'static LootAction)>,
    run: Res<'w, SmokeRun>,
    exit: MessageWriter<'w, AppExit>,
}

fn check_fresh_run(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        quest,
        inventory,
        checkpoint,
        names,
        parents,
        worms,
        captures,
        mut hits,
        spatial,
        orbit,
        mut commands,
        entity,
        mut controller,
        mut dodge,
        mut transform,
        player_health,
        position,
        now,
        ..
    } = frame;

    match *phase {
        WorldCheck::Spawn if controller.grounded => {
            let spawn = WorldLayout::Outpost.initial_spawn();
            assert!(position.distance(spawn) < 0.04);
            assert!((orbit.yaw - std::f32::consts::FRAC_PI_2).abs() < 0.01);
            assert!(quest.expects(QuestAction::TalkedToNemi));
            let filter = SpatialQueryFilter::from_excluded_entities([entity]);
            for (origin, direction, part) in [
                (position, Dir3::NEG_Y, "deck"),
                (position, Dir3::Y, "roof"),
                (position, Dir3::X, "rear wall"),
                (position, Dir3::Z, "sidewall"),
                (position - Vec3::Y * 0.55, Dir3::NEG_Z, "cot base"),
                (
                    position - Vec3::X * 2.05 - Vec3::Y * 0.55,
                    Dir3::Z,
                    "supply crate",
                ),
            ] {
                let hit = spatial
                    .cast_ray(origin, direction, 5.0, true, &filter)
                    .unwrap_or_else(|| panic!("starting tent {part} should have collision"));
                let node = parents.get(hit.entity).expect("tent collider mesh parent");
                let name = names.get(node.parent()).expect("named tent node");
                assert!(
                    name.as_str()
                        .starts_with(&format!("Starting tent solid {part}")),
                    "expected to hit tent {part}, got {name}"
                );
            }
            let mut relay_worms = worms
                .iter()
                .filter(|(_, transform, _)| transform.translation.y < -7.0);
            let (worm, transform, _) = relay_worms.next().expect("the junction ice worm");
            assert!(relay_worms.next().is_none(), "the archive uses a Hushcap");
            let probe = transform.translation + Vec3::Y;
            let support = spatial
                .cast_ray(
                    probe,
                    Dir3::NEG_Y,
                    2.0,
                    true,
                    &SpatialQueryFilter::from_excluded_entities([worm]),
                )
                .expect("the junction worm must have a supporting floor");
            assert!((support.distance - 1.0 - GROUND_OFFSET).abs() < 0.03);
            commands.queue(|world: &mut World| {
                assert!(world.resource_mut::<Inventory>().add(ItemKind::Scrap, 1));
                world
                    .resource_mut::<QuestJournal>()
                    .record(QuestAction::TalkedToNemi);
            });
            hits.write(Hit {
                source: entity,
                target: entity,
                damage: u16::MAX,
                contact: None,
            });
            *phase = WorldCheck::FreshRespawn(entity);
        }
        WorldCheck::FreshRespawn(old) if entity != old && controller.grounded => {
            assert!(position.distance(WorldLayout::Outpost.initial_spawn()) < 0.04);
            assert!(quest.expects(QuestAction::TalkedToNemi));
            assert_eq!(inventory.count(ItemKind::Scrap), 0);
            assert!(checkpoint.checkpoint().is_none());
            assert_eq!(player_health.current(), player_health.maximum());
            // Allow the new scene's render pipelines to compile before the first photo.
            *phase = WorldCheck::TentView(now + 0.75);
        }
        WorldCheck::TentView(ready_at) if now >= ready_at => {
            *phase = WorldCheck::TentPhoto(capture(&mut commands, "starting-tent"));
        }
        WorldCheck::TentPhoto(photo) if captures.get(photo).is_err() => {
            fixture(
                &spatial,
                entity,
                &mut controller,
                &mut dodge,
                &mut transform,
                Vec3::new(9.7, 0.0, -2.40),
            );
            *phase = WorldCheck::InspectChest(ChestId::Gear);
        }
        _ => {}
    }
}

fn check_tent_chests(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        inventory,
        storage,
        mut keys,
        mut commands,
        mut controller,
        mut dodge,
        mut transform,
        nearby,
        spatial,
        entity,
        storage_actions,
        window,
        camera,
        state,
        ..
    } = frame;
    let click_storage = |commands: &mut Commands, action| {
        click_storage(commands, action, &storage_actions, &window, &camera);
    };
    let storage_open = *state.get() == GameState::Storage;
    match *phase {
        WorldCheck::InspectChest(chest)
            if target_ready(&nearby, InteractionTarget::Chest(chest)) =>
        {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::TakeFromChest(chest);
        }
        WorldCheck::TakeFromChest(chest) if storage_open => {
            let (item, initial_count, _) = chest_stack(chest);
            assert_eq!(storage.get(chest).count(item), initial_count);
            let amount = if initial_count > 1 {
                TransferAmount::Stack
            } else {
                TransferAmount::One
            };
            click_storage(
                &mut commands,
                StorageAction::Transfer(StorageSide::Chest, item, amount),
            );
            *phase = WorldCheck::StoreInChest(chest);
        }
        WorldCheck::StoreInChest(chest) => {
            let (item, initial_count, amount) = chest_stack(chest);
            assert_eq!(storage.get(chest).count(item), 0);
            assert_eq!(inventory.count(item), initial_count);
            click_storage(
                &mut commands,
                StorageAction::Transfer(StorageSide::Backpack, item, amount),
            );
            *phase = WorldCheck::CloseChest(chest);
        }
        WorldCheck::CloseChest(chest) => {
            let (item, initial_count, amount) = chest_stack(chest);
            let stored_count = if amount == TransferAmount::Stack {
                initial_count
            } else {
                1
            };
            assert_eq!(storage.get(chest).count(item), stored_count);
            assert_eq!(inventory.count(item), initial_count - stored_count);
            if chest == ChestId::Lamp {
                keys.press(KeyCode::Escape);
            } else {
                click_storage(&mut commands, StorageAction::Close);
            }
            *phase = WorldCheck::NextChest(chest);
        }
        WorldCheck::NextChest(chest) if !storage_open => match chest {
            ChestId::Gear => {
                fixture(
                    &spatial,
                    entity,
                    &mut controller,
                    &mut dodge,
                    &mut transform,
                    Vec3::new(8.3, 0.0, -2.55),
                );
                *phase = WorldCheck::InspectChest(ChestId::Supply);
            }
            ChestId::Supply => {
                fixture(
                    &spatial,
                    entity,
                    &mut controller,
                    &mut dodge,
                    &mut transform,
                    Vec3::new(8.0, 0.0, -3.55),
                );
                *phase = WorldCheck::InspectChest(ChestId::Lamp);
            }
            ChestId::Lamp => {
                controller.respawn(
                    &mut dodge,
                    &mut transform,
                    WorldLayout::Outpost.initial_spawn(),
                );
                *phase = WorldCheck::LeaveTent;
            }
        },
        _ => {}
    }
}

fn check_outpost_storage(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        inventory,
        nearby,
        chests,
        captures,
        storage,
        mut orbit,
        mut keys,
        mut commands,
        mut controller,
        mut dodge,
        mut transform,
        position,
        spatial,
        entity,
        storage_actions,
        window,
        camera,
        state,
        ..
    } = frame;
    let storage_open = *state.get() == GameState::Storage;
    match *phase {
        WorldCheck::LeaveTent if position.x < 5.55 && controller.grounded => {
            orbit.yaw = -std::f32::consts::FRAC_PI_2;
            *phase = WorldCheck::OutpostPhoto(capture(&mut commands, "outpost"));
        }
        WorldCheck::LeaveTent => keys.press(KeyCode::KeyW),
        WorldCheck::OutpostPhoto(photo) if captures.get(photo).is_err() => {
            fixture(
                &spatial,
                entity,
                &mut controller,
                &mut dodge,
                &mut transform,
                Vec3::new(9.43, 0.0, 1.0),
            );
            *phase = WorldCheck::OutsideChestWall;
        }
        WorldCheck::OutsideChestWall if controller.grounded => {
            let (_, chest) = chests
                .iter()
                .find(|(id, _)| **id == ChestId::Gear)
                .expect("gear chest anchor");
            assert!(
                (position + Vec3::Y * INTERACT_HEIGHT).distance(chest.translation) < INTERACT_RANGE
            );
            assert!(
                !matches!(nearby.0, Some((_, InteractionTarget::Chest(_)))),
                "the tent wall must block storage interaction"
            );
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::BlockedChest;
        }
        WorldCheck::BlockedChest => {
            assert!(!storage_open);
            fixture(
                &spatial,
                entity,
                &mut controller,
                &mut dodge,
                &mut transform,
                WorldLayout::Outpost.initial_spawn().with_x(5.35),
            );
            orbit.yaw = std::f32::consts::FRAC_PI_2;
            *phase = WorldCheck::ReturnToTent;
        }
        WorldCheck::ReturnToTent
            if position.x >= WorldLayout::Outpost.initial_spawn().x && controller.grounded =>
        {
            assert!((position.z - WorldLayout::Outpost.initial_spawn().z).abs() < 0.08);
            fixture(
                &spatial,
                entity,
                &mut controller,
                &mut dodge,
                &mut transform,
                Vec3::new(8.0, 0.0, -3.55),
            );
            *phase = WorldCheck::InspectStoredChest;
        }
        WorldCheck::ReturnToTent => keys.press(KeyCode::KeyS),
        WorldCheck::InspectStoredChest
            if target_ready(&nearby, InteractionTarget::Chest(ChestId::Lamp)) =>
        {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::CheckStoredChest;
        }
        WorldCheck::CheckStoredChest if storage_open => {
            assert_eq!(storage.get(ChestId::Lamp).count(ItemKind::Battery), 1);
            assert_eq!(inventory.count(ItemKind::Battery), 1);
            capture(&mut commands, "tent-storage");
            *phase = WorldCheck::LeaveStorage;
        }
        WorldCheck::LeaveStorage if storage_open && captures.is_empty() => {
            click_storage(
                &mut commands,
                StorageAction::Close,
                &storage_actions,
                &window,
                &camera,
            );
        }
        WorldCheck::LeaveStorage if !storage_open => {
            fixture(
                &spatial,
                entity,
                &mut controller,
                &mut dodge,
                &mut transform,
                WorldLayout::Outpost.core() + Vec3::Z * 1.65,
            );
            *phase = WorldCheck::RecoverCore;
        }
        _ => {}
    }
}

fn discover_relay(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        quest,
        inventory,
        dialogue,
        journal,
        selection,
        rows,
        texts,
        xp,
        window,
        camera,
        mut keys,
        mut commands,
        mut controller,
        mut dodge,
        mut transform,
        nearby,
        spatial,
        entity,
        ..
    } = frame;

    match *phase {
        WorldCheck::RecoverCore
            if target_ready(&nearby, InteractionTarget::Quest(Target::Core)) =>
        {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::CheckRecoveredCore;
        }
        WorldCheck::CheckRecoveredCore => {
            assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
            assert!(quest.expects(QuestAction::TalkedToNemi));
            assert!(!quest.beacon_powered());
            assert_eq!(*xp, Experience(0));
            assert!(
                dialogue
                    .text()
                    .is_some_and(|text| text.contains("Speak to Nemi"))
            );
            keys.press(KeyCode::KeyE);
            fixture(
                &spatial,
                entity,
                &mut controller,
                &mut dodge,
                &mut transform,
                HATCH + Vec3::Z * 1.35,
            );
            *phase = WorldCheck::InspectLocked;
        }
        WorldCheck::InspectLocked
            if target_ready(&nearby, InteractionTarget::Dungeon(DungeonTarget::Hatch)) =>
        {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::LockedNotice;
        }
        WorldCheck::LockedNotice => {
            assert_eq!(
                controller.area,
                Area::Surface,
                "an unfinished quest must keep the hatch locked"
            );
            assert!(quest.expects(QuestAction::TalkedToNemi));
            assert_eq!(quest.entries().count(), 2);
            assert!(
                dialogue
                    .text()
                    .is_some_and(|text| text.contains("Restore the light"))
            );
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::OpenDiscoveredJournal;
        }
        WorldCheck::OpenDiscoveredJournal if !dialogue.is_open() => {
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::CheckDiscoveredJournal;
        }
        WorldCheck::CheckDiscoveredJournal => {
            assert!(journal.is_on(MenuTab::Quest));
            assert!(
                rows.iter()
                    .any(|(_, row)| row.id == LAST_LIGHT && row.status == QuestStatus::Active)
            );
            let (relay, _) = rows
                .iter()
                .find(|(_, row)| row.id == OLD_RELAY && row.status == QuestStatus::Active)
                .expect("Old Relay must have an active journal row");
            commands.trigger(pointer_click(
                relay,
                window.single().expect("smoke window"),
                camera.single().expect("player camera"),
            ));
            *phase = WorldCheck::CheckRelaySelection;
        }
        WorldCheck::CheckRelaySelection => {
            assert_eq!(selection.selected(), Some(OLD_RELAY));
            assert!(
                texts
                    .iter()
                    .any(|text| text.as_str() == "Completion reward | 120 XP")
            );
            capture(&mut commands, "relay-quest-log");
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::SetUpTurnIn;
        }
        _ => {}
    }
}

fn assert_uncommitted_beacon(quest: &QuestJournal, inventory: &Inventory) {
    assert!(quest.expects(QuestAction::TalkedToNemi));
    assert!(!quest.beacon_powered());
    assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
}

fn unlock_relay(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        quest,
        inventory,
        dialogue,
        progress,
        journal,
        xp_notice,
        attributes,
        xp,
        gate,
        mut orbit,
        mut keys,
        mut commands,
        mut controller,
        mut dodge,
        mut transform,
        position,
        nearby,
        spatial,
        entity,
        capsule,
        ..
    } = frame;

    match *phase {
        WorldCheck::SetUpTurnIn if !journal.is_open() => {
            fixture(
                &spatial,
                entity,
                &mut controller,
                &mut dodge,
                &mut transform,
                WorldLayout::Outpost.keeper() + Vec3::Z * 1.65,
            );
            *phase = WorldCheck::OpenEarlyDialogue;
        }
        WorldCheck::OpenEarlyDialogue if target_ready(&nearby, InteractionTarget::Talk) => {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::ChooseNemiLore;
        }
        WorldCheck::ChooseNemiLore => {
            assert_uncommitted_beacon(&quest, &inventory);
            assert!(dialogue.is_open() && dialogue.text().is_none());
            if dialogue.selected() == 0 {
                capture(&mut commands, "nemi-topics");
                keys.press(KeyCode::KeyS);
            } else {
                keys.press(KeyCode::Enter);
                *phase = WorldCheck::ReadNemiLore;
            }
        }
        WorldCheck::ReadNemiLore => {
            assert!(
                dialogue
                    .text()
                    .is_some_and(|text| text.contains("I keep the crossing light"))
            );
            assert_uncommitted_beacon(&quest, &inventory);
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::ChooseNivalisLore;
        }
        WorldCheck::ChooseNivalisLore => {
            assert!(dialogue.is_open() && dialogue.text().is_none());
            if dialogue.selected() == 1 {
                keys.press(KeyCode::KeyS);
            } else {
                keys.press(KeyCode::Enter);
                *phase = WorldCheck::ReadNivalisLore;
            }
        }
        WorldCheck::ReadNivalisLore => {
            assert!(
                dialogue
                    .text()
                    .is_some_and(|text| text.contains("paired blue waystones"))
            );
            assert_uncommitted_beacon(&quest, &inventory);
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::SelectBeaconTopic;
        }
        WorldCheck::SelectBeaconTopic => {
            if dialogue.selected() == 0 {
                keys.press(KeyCode::Enter);
                *phase = WorldCheck::TurnIn;
            } else {
                keys.press(KeyCode::KeyW);
            }
        }
        WorldCheck::TurnIn => {
            assert!(quest.is_complete(LAST_LIGHT));
            assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
            assert_eq!((*attributes, *xp), (BEACON_PROGRESS, Experience(0)));
            assert_eq!(
                xp_notice.text.as_deref(),
                Some("+70 XP | Level 2 (+3 attribute points)")
            );
            let text = dialogue
                .text()
                .expect("first meeting completes The Last Light");
            assert!(text.contains("You found the core") && text.contains("Old Relay"));
            keys.press(KeyCode::Escape);
            *phase = WorldCheck::ApproachHatch;
        }
        WorldCheck::ApproachHatch => {
            if target_ready(&nearby, InteractionTarget::Dungeon(DungeonTarget::Hatch)) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::Enter;
            } else {
                walk_to(&mut keys, &mut orbit, position, HATCH);
            }
        }
        WorldCheck::Enter if controller.grounded => {
            assert_eq!(controller.area, Area::OldRelay);
            assert!((position.y - (ORIGIN.y + 0.9)).abs() < 0.1);
            assert!(!progress.gate_open());
            assert_eq!(
                passage_hit(&spatial, capsule, entity),
                Some(
                    gate.single()
                        .expect("the relay should have one archive gate")
                ),
                "the closed gate must block the player capsule"
            );
            capture(&mut commands, "relay-vestibule");
            *phase = WorldCheck::LureWorm;
        }
        _ => {}
    }
}

fn check_relay_encounters(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        quest,
        inventory,
        nearby,
        progress,
        journal,
        worms,
        state,
        roaches,
        actions,
        window,
        camera,
        mut hits,
        mut orbit,
        mut keys,
        mut commands,
        entity,
        player_health,
        xp,
        position,
        spatial,
        capsule,
        ..
    } = frame;
    let junction = ORIGIN + Vec3::new(3.8, 0.0, -10.75);
    match *phase {
        WorldCheck::LureWorm => {
            if junction_worm(&worms).is_some_and(
                |worm| matches!(worm.phase, MeleePhase::Windup(remaining) if remaining <= 0.36),
            ) {
                orbit.yaw = 0.68;
                orbit.pitch = 0.32;
                capture(&mut commands, "relay-worm");
                *phase = WorldCheck::DodgeWorm(player_health.current());
            } else {
                walk_to(
                    &mut keys,
                    &mut orbit,
                    position,
                    ORIGIN + Vec3::new(0.1, 0.0, -8.7),
                );
            }
        }
        WorldCheck::DodgeWorm(health) => {
            walk_to(&mut keys, &mut orbit, position, junction);
            if junction_worm(&worms)
                .is_some_and(|worm| matches!(worm.phase, MeleePhase::Recovery(_)))
            {
                assert_eq!(
                    player_health.current(),
                    health,
                    "the relay must leave room to dodge"
                );
                *phase = WorldCheck::Junction;
            }
        }
        WorldCheck::Junction => {
            if walk_to(&mut keys, &mut orbit, position, junction) {
                let (roach, _, _) = relay_encounter(&roaches, EncounterId::RelayRoach);
                hits.write(Hit {
                    source: entity,
                    target: roach,
                    damage: 20,
                    contact: None,
                });
                *phase = WorldCheck::ApproachRoach;
            }
        }
        WorldCheck::ApproachRoach => {
            let (roach, corpse, dead) = relay_encounter(&roaches, EncounterId::RelayRoach);
            assert!(dead);
            assert_eq!(*xp, Experience(25));
            if nearby.0 == Some((roach, InteractionTarget::Corpse)) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::TakeRoachMeat;
            } else {
                walk_to(&mut keys, &mut orbit, position, corpse.translation);
            }
        }
        WorldCheck::TakeRoachMeat if *state.get() == GameState::Looting => {
            assert_eq!(inventory.count(ItemKind::RoachMeat), 0);
            click(
                &mut commands,
                LootAction::Take,
                &actions,
                window.single().expect("smoke window"),
                camera.single().expect("player camera"),
            );
            *phase = WorldCheck::CloseRoachLoot;
        }
        WorldCheck::CloseRoachLoot => {
            assert_eq!(inventory.count(ItemKind::RoachMeat), 1);
            keys.press(KeyCode::Escape);
            *phase = WorldCheck::Switch;
        }
        WorldCheck::Switch if *state.get() == GameState::Playing => {
            if target_ready(&nearby, InteractionTarget::Dungeon(DungeonTarget::Switch)) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::OpenGateJournal;
            } else {
                walk_to(&mut keys, &mut orbit, position, ORIGIN + SWITCH_LOCAL);
            }
        }
        WorldCheck::OpenGateJournal => {
            assert!(!progress.gate_open());
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::GateRises;
        }
        WorldCheck::GateRises
            if progress.gate_open() && passage_hit(&spatial, capsule, entity).is_none() =>
        {
            assert!(journal.is_on(MenuTab::Quest));
            assert!(quest.expects(QuestAction::ReadSurveyLog));
            keys.press(KeyCode::Tab);
            capture(&mut commands, "relay-maintenance");
            *phase = WorldCheck::ReturnToJunction;
        }
        _ => {}
    }
}

fn enter_archive(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        mut orbit,
        mut keys,
        position,
        capsule,
        entity,
        spatial,
        names,
        parents,
        hushcaps,
        ..
    } = frame;
    let (waypoint, next) = match *phase {
        WorldCheck::ReturnToJunction => (Vec3::new(3.8, 0.0, -10.75), WorldCheck::GateApproach),
        WorldCheck::GateApproach => (Vec3::new(0.0, 0.0, -14.2), WorldCheck::CrossGate),
        WorldCheck::CrossGate => (Vec3::new(0.0, 0.0, -21.0), WorldCheck::ApproachLog),
        _ => return,
    };
    if !walk_to(&mut keys, &mut orbit, position, ORIGIN + waypoint) {
        return;
    }
    if matches!(*phase, WorldCheck::CrossGate) {
        assert!(position.z < ORIGIN.z + GATE_LOCAL.z);
        let (hushcap, root, health, _, dead, _) =
            relay_encounter(&hushcaps, EncounterId::RelayArchiveHushcap);
        assert_eq!(root.translation, ARCHIVE_HUSHCAP_HOME);
        assert_eq!(health.current(), hushcap::HEALTH);
        assert!(!dead);
        check_archive_cover(
            &spatial, &names, &parents, capsule, entity, hushcap, position,
        );
    }
    *phase = next;
}

fn finish_relay_quest(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        quest,
        dialogue,
        journal,
        rows,
        xp_notice,
        attributes,
        xp,
        mut orbit,
        mut keys,
        mut commands,
        controller,
        position,
        nearby,
        hushcaps,
        ..
    } = frame;
    match *phase {
        WorldCheck::ApproachLog => {
            if target_ready(
                &nearby,
                InteractionTarget::Dungeon(DungeonTarget::SurveyLog),
            ) {
                orbit.yaw = 0.30;
                orbit.pitch = 0.23;
                *phase = WorldCheck::ReadLog;
                capture(&mut commands, "relay-archive");
            } else {
                walk_to(&mut keys, &mut orbit, position, ORIGIN + LOG_LOCAL);
            }
        }
        WorldCheck::ReadLog => {
            assert!(target_ready(
                &nearby,
                InteractionTarget::Dungeon(DungeonTarget::SurveyLog)
            ));
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::ReadNotice;
        }
        WorldCheck::ReadNotice => {
            assert!(quest.is_complete(OLD_RELAY));
            assert_eq!(quest.status(CONTAIN_THE_HUSHCAPS), None);
            assert!(!relay_encounter(&hushcaps, EncounterId::RelayArchiveHushcap).4);
            assert_eq!((*attributes, *xp), (RELAY_PROGRESS, Experience(25)));
            assert_eq!(
                xp_notice.text.as_deref(),
                Some("+120 XP | Level 3 (+3 attribute points)")
            );
            assert!(
                dialogue
                    .text()
                    .is_some_and(|text| text.starts_with("SURVEY LOG"))
            );
            capture(&mut commands, "relay-discovery");
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::OpenCompletedJournal;
        }
        WorldCheck::OpenCompletedJournal if !dialogue.is_open() => {
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::CheckCompletedJournal;
        }
        WorldCheck::CheckCompletedJournal => {
            assert!(journal.is_on(MenuTab::Quest));
            assert!(
                rows.iter()
                    .any(|(_, row)| row.id == OLD_RELAY && row.status == QuestStatus::Completed)
            );
            keys.press(KeyCode::Tab);
            commands.queue(|world: &mut World| {
                world.resource_scope(|world, mut equipment: Mut<Equipment>| {
                    let mut inventory = world.resource_mut::<Inventory>();
                    assert!(inventory.add(ItemKind::FieldSpear, 1));
                    assert_eq!(
                        equipment.equip(&mut inventory, ItemKind::FieldSpear, 5),
                        Ok(true)
                    );
                });
            });
            *phase = WorldCheck::ApproachHushcap;
        }
        WorldCheck::LeaveArchive => {
            if dialogue.is_open() {
                keys.press(KeyCode::KeyE);
            } else if walk_to(
                &mut keys,
                &mut orbit,
                position,
                ORIGIN + Vec3::new(0.0, 0.0, -14.1),
            ) {
                *phase = WorldCheck::LeaveJunction;
            }
        }
        WorldCheck::LeaveJunction => {
            if target_ready(&nearby, InteractionTarget::Dungeon(DungeonTarget::Exit)) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::Exit;
            } else {
                walk_to(
                    &mut keys,
                    &mut orbit,
                    position,
                    ORIGIN + Vec3::new(0.0, 0.0, -2.35),
                );
            }
        }
        WorldCheck::Exit if controller.grounded => {
            assert_eq!(controller.area, Area::Surface);
            assert!((position.xz().distance(HATCH.xz()) - 2.1).abs() < 0.1);
            assert!(quest.is_complete(OLD_RELAY));
            assert!(quest.expects(QuestAction::ReportedArchiveHushcap));
            *phase = WorldCheck::ApproachNemi;
        }
        _ => {}
    }
}

fn check_cavern_briefing(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        quest,
        nearby,
        dialogue,
        position,
        mut keys,
        mut orbit,
        mut commands,
        rows,
        texts,
        journal,
        selection,
        window,
        camera,
        captures,
        ..
    } = frame;
    match *phase {
        WorldCheck::ApproachNemi if target_ready(&nearby, InteractionTarget::Talk) => {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::SelectCavernTopic;
        }
        WorldCheck::ApproachNemi => {
            walk_to(
                &mut keys,
                &mut orbit,
                position,
                WorldLayout::Outpost.keeper(),
            );
        }
        WorldCheck::SelectCavernTopic if dialogue.is_open() => {
            assert!(quest.expects(QuestAction::ReportedArchiveHushcap));
            if dialogue.selected() == 0 {
                keys.press(KeyCode::KeyS);
            } else {
                keys.press(KeyCode::Enter);
                *phase = WorldCheck::CavernBriefing;
            }
        }
        WorldCheck::CavernBriefing => {
            assert!(quest.has_occurred(QuestAction::ReportedArchiveHushcap));
            assert!(dialogue.text().is_some_and(|text| {
                text.contains("mushroom creature")
                    && text.contains("Hushcap Caverns")
                    && text.contains("five Mycelium")
            }));
            keys.press(KeyCode::Escape);
            *phase = WorldCheck::OpenCavernJournal;
        }
        WorldCheck::OpenCavernJournal if !dialogue.is_open() => {
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::SelectCavernQuest;
        }
        WorldCheck::SelectCavernQuest if journal.is_on(MenuTab::Quest) => {
            let (row, _) = rows
                .iter()
                .find(|(_, row)| {
                    row.id == CONTAIN_THE_HUSHCAPS && row.status == QuestStatus::Active
                })
                .expect("the cave quest should appear in the Active log");
            commands.trigger(pointer_click(
                row,
                window.single().expect("smoke window"),
                camera.single().expect("player camera"),
            ));
            *phase = WorldCheck::CheckCavernJournal;
        }
        WorldCheck::CheckCavernJournal if selection.selected() == Some(CONTAIN_THE_HUSHCAPS) => {
            assert!(texts.iter().any(|text| {
                text.as_str()
                    .contains("Defeat five Hushcaps in the caverns")
            }));
            assert!(texts.iter().any(|text| {
                text.as_str()
                    .contains("Hushcaps defeated  0/5\nMycelium in backpack  1/5")
            }));
            *phase = WorldCheck::CavernJournalPhoto(capture(&mut commands, "contain-the-hushcaps"));
        }
        WorldCheck::CavernJournalPhoto(photo) if captures.get(photo).is_err() => {
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::Reenter;
        }
        _ => {}
    }
}

fn check_archive_cover(
    spatial: &SpatialQuery,
    names: &Query<&Name>,
    parents: &Query<&ChildOf>,
    capsule: &Collider,
    player: Entity,
    hushcap: Entity,
    position: Vec3,
) {
    let muzzle = ARCHIVE_HUSHCAP_HOME + Vec3::Y * hushcap::SPIT.muzzle.y;
    let delta = position - muzzle;
    let filter = SpatialQueryFilter::from_excluded_entities([player, hushcap]);
    let cover = spatial
        .cast_shape(
            &Collider::sphere(hushcap::SPIT.radius_m),
            muzzle,
            Quat::IDENTITY,
            Dir3::new(delta).expect("the archive entrance must be away from the Hushcap"),
            &ShapeCastConfig::from_max_distance(delta.length()),
            &filter,
        )
        .expect("the archive console must intercept spit aimed at the entrance");
    assert!(
        std::iter::once(cover.entity)
            .chain(parents.iter_ancestors(cover.entity))
            .any(|node| names
                .get(node)
                .is_ok_and(|name| name.as_str() == "Archive service console")),
        "the archive service console must be the first obstruction"
    );
    assert!(
        spatial
            .cast_shape(
                capsule,
                position,
                Quat::IDENTITY,
                Dir3::NEG_Z,
                &ShapeCastConfig::from_max_distance(4.0),
                &filter,
            )
            .is_none(),
        "the console must leave the central approach walkable"
    );
}

fn fight_hushcap(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        hushcaps,
        spit,
        state,
        nearby,
        inventory,
        xp,
        thrust,
        mut commands,
        mut keys,
        mut mouse,
        mut orbit,
        position,
        controller,
        actions,
        window,
        camera,
        ..
    } = frame;
    let (entity, root, _, loot, dead, solid) =
        relay_encounter(&hushcaps, EncounterId::RelayArchiveHushcap);
    assert_eq!(
        root.translation, ARCHIVE_HUSHCAP_HOME,
        "the Hushcap must stay rooted"
    );
    match *phase {
        WorldCheck::ApproachHushcap => {
            let spear_range = ARCHIVE_HUSHCAP_HOME - Vec3::X * 1.8;
            if walk_to(&mut keys, &mut orbit, position, spear_range) {
                orbit.yaw = -0.94;
                capture(&mut commands, "relay-hushcap");
                *phase = WorldCheck::FightHushcap { saw_spit: false };
            }
        }
        WorldCheck::FightHushcap { saw_spit } if dead => {
            assert!(saw_spit, "the Hushcap should spit during the fight");
            assert!(!solid, "the Hushcap must release its collider on death");
            assert_eq!(*xp, LOOTED_RELAY_XP);
            assert_eq!(
                loot.stack.expect("Hushcap corpse loot").item,
                ItemKind::Mycelium
            );
            if nearby.0 == Some((entity, InteractionTarget::Corpse)) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::TakeMycelium;
            } else {
                walk_to(&mut keys, &mut orbit, position, ARCHIVE_HUSHCAP_HOME);
            }
        }
        WorldCheck::FightHushcap { saw_spit } => {
            if !saw_spit && !spit.is_empty() {
                capture(&mut commands, "relay-hushcap-spit");
                *phase = WorldCheck::FightHushcap { saw_spit: true };
            }
            let delta = (root.translation - position).with_y(0.0);
            let offset = (-GRIP_SIDE / delta.length()).clamp(0.0, 1.0).asin();
            orbit.yaw = delta.x.atan2(delta.z) + offset - std::f32::consts::PI;
            if !thrust.active() && controller.grounded {
                mouse.press(MouseButton::Left);
            }
        }
        WorldCheck::TakeMycelium if *state.get() == GameState::Looting => {
            assert_eq!(inventory.count(ItemKind::Mycelium), 0);
            click(
                &mut commands,
                LootAction::Take,
                &actions,
                window.single().expect("smoke window"),
                camera.single().expect("player camera"),
            );
            *phase = WorldCheck::CloseMycelium;
        }
        WorldCheck::CloseMycelium => {
            assert_eq!(inventory.count(ItemKind::Mycelium), 1);
            assert!(loot.stack.is_none());
            keys.press(KeyCode::Escape);
            *phase = WorldCheck::ClearArchiveCover;
        }
        WorldCheck::ClearArchiveCover
            if *state.get() == GameState::Playing
                && walk_to(
                    &mut keys,
                    &mut orbit,
                    position,
                    ORIGIN + Vec3::new(0.0, 0.0, -25.3),
                ) =>
        {
            *phase = WorldCheck::LeaveArchive;
        }
        _ => {}
    }
}

fn check_relay_revisit(frame: WorldFrame) {
    let WorldFrame {
        mut phase,
        quest,
        inventory,
        progress,
        roaches,
        hushcaps,
        mut keys,
        mut orbit,
        controller,
        position,
        mut transform,
        nearby,
        spatial,
        capsule,
        entity,
        ..
    } = frame;
    match *phase {
        WorldCheck::Reenter
            if target_ready(&nearby, InteractionTarget::Dungeon(DungeonTarget::Hatch)) =>
        {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::Revisited;
        }
        WorldCheck::Reenter => {
            walk_to(&mut keys, &mut orbit, position, HATCH + Vec3::Z);
        }
        WorldCheck::Revisited if controller.grounded => {
            assert_eq!(controller.area, Area::OldRelay);
            assert!(quest.is_complete(OLD_RELAY) && progress.gate_open());
            assert!(quest.has_occurred(QuestAction::ReportedArchiveHushcap));
            assert!(
                relay_encounter(&roaches, EncounterId::RelayRoach).2,
                "the roach corpse persists across area changes"
            );
            assert_eq!(inventory.count(ItemKind::RoachMeat), 1);
            let (_, _, _, loot, dead, solid) =
                relay_encounter(&hushcaps, EncounterId::RelayArchiveHushcap);
            assert!(dead && !solid && loot.stack.is_none());
            assert_eq!(inventory.count(ItemKind::Mycelium), 1);
            assert_eq!(
                passage_hit(&spatial, capsule, entity),
                None,
                "the gate must stay clear after returning"
            );
            transform.translation.y = -20.0;
            *phase = WorldCheck::Recover;
        }
        _ => {}
    }
}

fn check_bed_rollback(frame: WorldFrame) {
    let WorldFrame {
        run,
        mut phase,
        quest,
        inventory,
        nearby,
        checkpoint,
        progress,
        roaches,
        hushcaps,
        storage,
        mut hits,
        mut keys,
        mut commands,
        mut exit,
        entity,
        mut controller,
        mut dodge,
        mut transform,
        player_health,
        ranks,
        attributes,
        xp,
        xp_notice,
        position,
        spatial,
        capsule,
        ..
    } = frame;

    const SAVED_VITALITY: u8 = 7;

    match *phase {
        WorldCheck::Recover if controller.area == Area::Surface && controller.grounded => {
            assert!(position.xz().distance(WorldLayout::Outpost.start().xz()) < 0.1);
            assert!(quest.is_complete(OLD_RELAY) && progress.gate_open());
            assert_eq!(
                quest.status(CONTAIN_THE_HUSHCAPS),
                Some(QuestStatus::Active)
            );
            assert_eq!(storage.get(ChestId::Lamp).count(ItemKind::Battery), 1);
            fixture(
                &spatial,
                entity,
                &mut controller,
                &mut dodge,
                &mut transform,
                Vec3::new(9.7, 0.0, -3.35),
            );
            commands
                .entity(entity)
                .insert(AttributeRanks::default().with_rank(AttributeId::Vitality, SAVED_VITALITY));
            *phase = WorldCheck::SaveAtBed;
        }
        WorldCheck::SaveAtBed
            if controller.grounded && matches!(nearby.0, Some((_, InteractionTarget::Bed))) =>
        {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::ChangeAfterSave;
        }
        WorldCheck::ChangeAfterSave => {
            assert!(
                checkpoint.checkpoint().is_some(),
                "the cot must write the checkpoint"
            );
            assert_eq!((*attributes, *xp), (RELAY_PROGRESS, LOOTED_RELAY_XP));
            commands.entity(entity).insert((
                AttributeRanks::default().with_rank(AttributeId::Vitality, 8),
                CharacterProgress {
                    level: 3,
                    unspent: 0,
                },
                Experience(90),
            ));
            commands.queue(|world: &mut World| {
                assert!(
                    world
                        .resource_mut::<Inventory>()
                        .remove(ItemKind::RoachMeat, 1)
                );
                assert!(
                    world
                        .resource_mut::<Inventory>()
                        .remove(ItemKind::Mycelium, 1)
                );
                assert!(
                    world
                        .resource_mut::<ChestStorage>()
                        .get_mut(ChestId::Lamp)
                        .add(ItemKind::Battery, 1)
                );
            });
            hits.write(Hit {
                source: entity,
                target: entity,
                damage: u16::MAX,
                contact: None,
            });
            *phase = WorldCheck::SavedRespawn(entity);
        }
        WorldCheck::SavedRespawn(old) if entity != old && controller.grounded => {
            assert!(position.distance(WorldLayout::Outpost.initial_spawn()) < 0.04);
            assert_eq!(ranks.rank(AttributeId::Vitality), SAVED_VITALITY);
            assert_eq!((*attributes, *xp), (RELAY_PROGRESS, LOOTED_RELAY_XP));
            assert!(
                xp_notice.text.is_none(),
                "restored completions must not award XP again"
            );
            assert_eq!(
                (player_health.current(), player_health.maximum()),
                (108, 108)
            );
            assert_eq!(inventory.count(ItemKind::RoachMeat), 1);
            assert_eq!(inventory.count(ItemKind::Mycelium), 1);
            assert_eq!(storage.get(ChestId::Lamp).count(ItemKind::Battery), 1);
            assert!(quest.is_complete(OLD_RELAY) && progress.gate_open());
            assert_eq!(
                quest.status(CONTAIN_THE_HUSHCAPS),
                Some(QuestStatus::Active)
            );
            assert!(quest.has_occurred(QuestAction::ReportedArchiveHushcap));
            assert!(relay_encounter(&roaches, EncounterId::RelayRoach).2);
            let (_, _, health, loot, dead, solid) =
                relay_encounter(&hushcaps, EncounterId::RelayArchiveHushcap);
            assert_eq!(health.current(), 0);
            assert!(dead && !solid && loot.stack.is_none());
            assert_eq!(
                passage_hit(&spatial, capsule, entity),
                None,
                "restored gate must be clear"
            );
            info!(
                "WORLD SMOKE PASS: tent storage, Old Relay, Nemi's cavern briefing, and bed save rollback verified in {:.2}s",
                run.0.elapsed().as_secs_f64()
            );
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn walk_to(
    keys: &mut ButtonInput<KeyCode>,
    orbit: &mut Orbit,
    position: Vec3,
    destination: Vec3,
) -> bool {
    if position.xz().distance(destination.xz()) < 0.28 {
        true
    } else {
        walk(keys, orbit, position, destination);
        false
    }
}

fn passage_hit(spatial: &SpatialQuery, capsule: &Collider, entity: Entity) -> Option<Entity> {
    spatial
        .cast_shape(
            capsule,
            ORIGIN + Vec3::new(0.0, 0.91, -16.1),
            Quat::IDENTITY,
            Dir3::NEG_Z,
            &ShapeCastConfig::from_max_distance(3.4),
            &SpatialQueryFilter::from_excluded_entities([entity]),
        )
        .map(|hit| hit.entity)
}

fn fixture(
    spatial: &SpatialQuery,
    entity: Entity,
    player: &mut Player,
    dodge: &mut Dodge,
    transform: &mut Transform,
    point: Vec3,
) {
    let probe = point.with_y(2.0);
    let support = spatial
        .cast_ray(
            probe,
            Dir3::NEG_Y,
            6.0,
            true,
            &SpatialQueryFilter::from_excluded_entities([entity]),
        )
        .expect("the exterior smoke fixture must stand over the ice shelf");
    player.respawn(
        dodge,
        transform,
        probe - Vec3::Y * (support.distance - 0.905),
    );
}

fn click_storage(
    commands: &mut Commands,
    action: StorageAction,
    storage_actions: &Query<(Entity, &StorageAction)>,
    window: &Query<Entity, With<Window>>,
    camera: &Query<Entity, With<FollowCamera>>,
) {
    click(
        commands,
        action,
        storage_actions,
        window.single().expect("smoke window"),
        camera.single().expect("player camera"),
    );
}

fn target_ready(nearby: &Nearby, target: InteractionTarget) -> bool {
    nearby.0.is_some_and(|(_, current)| current == target)
}

fn chest_stack(chest: ChestId) -> (ItemKind, u32, TransferAmount) {
    match chest {
        ChestId::Gear => (ItemKind::FieldHelmet, 1, TransferAmount::One),
        ChestId::Supply => (ItemKind::Scrap, 2, TransferAmount::Stack),
        ChestId::Lamp => (ItemKind::Battery, 2, TransferAmount::One),
    }
}

fn junction_worm<'a>(
    worms: &'a Query<(Entity, &Transform, &MeleePursuit), (With<Worm>, Without<Player>)>,
) -> Option<&'a MeleePursuit> {
    worms
        .iter()
        .find(|(_, transform, _)| {
            transform.translation.y < -7.0 && transform.translation.z > ORIGIN.z - 15.0
        })
        .map(|(_, _, pursuit)| pursuit)
}
