//! Exercise the shipped relay with the real movement controller and interact input.
use super::{
    SmokeRun, capture,
    inventory::{click, pointer_click, walk},
};
use crate::{
    GameState,
    camera::{FollowCamera, Orbit},
    combat::{Dead, Health, Hit},
    dungeon::{
        ArchiveGate, Area, DungeonProgress, DungeonTarget, GATE_LOCAL, HATCH, LOG_LOCAL, ORIGIN,
        SWITCH_LOCAL,
    },
    enemy::{MeleePhase, MeleePursuit},
    interaction::{INTERACT_HEIGHT, INTERACT_RANGE, InteractionTarget, Nearby, Target},
    inventory::{Inventory, ItemKind},
    loot_ui::LootAction,
    menu_ui::{MenuTab, PlayerMenu},
    player::{Dodge, MAX_HEALTH, Player},
    quest::{Dialogue, LAST_LIGHT, OLD_RELAY, QuestAction, QuestJournal, QuestStatus},
    quest_ui::{QuestRow, QuestSelection},
    roach::Roach,
    save::RunSave,
    storage::{ChestId, ChestStorage},
    storage_ui::{StorageAction, StorageSide, TransferAmount},
    world::WorldLayout,
    worm::{GROUND_OFFSET, Worm},
};
use avian3d::prelude::*;
use bevy::{app::AppExit, prelude::*, render::view::screenshot::Screenshot};

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
    LeaveArchive,
    LeaveJunction,
    Exit,
    Reenter,
    Revisited,
    Recover,
    SaveAtBed,
    ChangeAfterSave,
    SavedRespawn(Entity),
}

pub(super) fn drive(
    run: Res<SmokeRun>,
    mut phase: ResMut<WorldCheck>,
    mut last_log: Local<f64>,
    mut player: Query<
        (
            Entity,
            &mut Player,
            &mut Dodge,
            &mut Transform,
            &Collider,
            &Health,
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
    (progress, journal, selection, rows): (
        Res<DungeonProgress>,
        Res<PlayerMenu>,
        Res<QuestSelection>,
        Query<(Entity, &QuestRow)>,
    ),
    (gate, names, parents, chests): (
        Query<Entity, With<ArchiveGate>>,
        Query<&Name>,
        Query<&ChildOf>,
        Query<(&ChestId, &Transform), Without<Player>>,
    ),
    worms: Query<(Entity, &Transform, &MeleePursuit), (With<Worm>, Without<Player>)>,
    captures: Query<(), With<Screenshot>>,
    (state, roaches, actions, window, camera, storage, storage_actions): (
        Res<State<GameState>>,
        Query<(Entity, &Transform, Has<Dead>), (With<Roach>, Without<Player>)>,
        Query<(Entity, &LootAction)>,
        Query<Entity, With<Window>>,
        Query<Entity, With<FollowCamera>>,
        Res<ChestStorage>,
        Query<(Entity, &StorageAction)>,
    ),
    mut hits: MessageWriter<Hit>,
    spatial: SpatialQuery,
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok((entity, mut controller, mut dodge, mut transform, capsule, player_health)) =
        player.single_mut()
    else {
        return;
    };
    keys.reset_all();
    let position = transform.translation;
    let now = run.0.elapsed().as_secs_f64();
    if now - *last_log >= 2.0 {
        info!("WORLD SMOKE: {:?} at {position:?}", *phase);
        *last_log = now;
    }
    let ready = |target| matches!(nearby.0, Some((_, InteractionTarget::Dungeon(current))) if current == target);
    let quest_ready = |target| matches!(nearby.0, Some((_, InteractionTarget::Quest(current))) if current == target);
    let chest_ready = |chest| matches!(nearby.0, Some((_, InteractionTarget::Chest(current))) if current == chest);
    let storage_open = *state.get() == GameState::Storage;
    let click_storage = |commands: &mut Commands, action| {
        click(
            commands,
            action,
            &storage_actions,
            window.single().expect("smoke window"),
            camera.single().expect("player camera"),
        );
    };
    let walk_to = |keys: &mut ButtonInput<KeyCode>, orbit: &mut Orbit, destination: Vec3| {
        if position.xz().distance(destination.xz()) < 0.28 {
            true
        } else {
            walk(keys, orbit, position, destination);
            false
        }
    };
    let passage_hit = || {
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
    };
    let fixture =
        |player: &mut Player, dodge: &mut Dodge, transform: &mut Transform, point: Vec3| {
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
        };
    let junction = ORIGIN + Vec3::new(3.8, 0.0, -10.75);

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
            let local_worms: Vec<_> = worms
                .iter()
                .filter(|(_, transform, _)| transform.translation.y < -7.0)
                .collect();
            assert_eq!(local_worms.len(), 2);
            for (worm, transform, _) in local_worms {
                let probe = transform.translation + Vec3::Y;
                let support = spatial
                    .cast_ray(
                        probe,
                        Dir3::NEG_Y,
                        2.0,
                        true,
                        &SpatialQueryFilter::from_excluded_entities([worm]),
                    )
                    .expect("each relay worm must have a supporting floor");
                assert!((support.distance - 1.0 - GROUND_OFFSET).abs() < 0.03);
            }
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
            assert_eq!(player_health.current(), MAX_HEALTH);
            // Allow the new scene's render pipelines to compile before the first photo.
            *phase = WorldCheck::TentView(now + 0.75);
        }
        WorldCheck::TentView(ready_at) if now >= ready_at => {
            *phase = WorldCheck::TentPhoto(capture(&mut commands, "starting-tent"));
        }
        WorldCheck::TentPhoto(photo) if captures.get(photo).is_err() => {
            fixture(
                &mut controller,
                &mut dodge,
                &mut transform,
                Vec3::new(9.7, 0.0, -2.40),
            );
            *phase = WorldCheck::InspectChest(ChestId::Gear);
        }
        WorldCheck::InspectChest(chest) if chest_ready(chest) => {
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
                    &mut controller,
                    &mut dodge,
                    &mut transform,
                    Vec3::new(8.3, 0.0, -2.55),
                );
                *phase = WorldCheck::InspectChest(ChestId::Supply);
            }
            ChestId::Supply => {
                fixture(
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
        WorldCheck::LeaveTent if position.x < 5.55 && controller.grounded => {
            orbit.yaw = -std::f32::consts::FRAC_PI_2;
            *phase = WorldCheck::OutpostPhoto(capture(&mut commands, "outpost"));
        }
        WorldCheck::LeaveTent => keys.press(KeyCode::KeyW),
        WorldCheck::OutpostPhoto(photo) if captures.get(photo).is_err() => {
            fixture(
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
                &mut controller,
                &mut dodge,
                &mut transform,
                Vec3::new(8.0, 0.0, -3.55),
            );
            *phase = WorldCheck::InspectStoredChest;
        }
        WorldCheck::ReturnToTent => keys.press(KeyCode::KeyS),
        WorldCheck::InspectStoredChest if chest_ready(ChestId::Lamp) => {
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
            click_storage(&mut commands, StorageAction::Close);
        }
        WorldCheck::LeaveStorage if !storage_open => {
            fixture(
                &mut controller,
                &mut dodge,
                &mut transform,
                WorldLayout::Outpost.core() + Vec3::Z * 1.65,
            );
            *phase = WorldCheck::RecoverCore;
        }
        WorldCheck::RecoverCore if quest_ready(Target::Core) => {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::CheckRecoveredCore;
        }
        WorldCheck::CheckRecoveredCore => {
            assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
            assert!(quest.expects(QuestAction::TalkedToNemi));
            assert!(!quest.beacon_powered());
            assert!(
                dialogue
                    .text
                    .is_some_and(|text| text.contains("Speak to Nemi"))
            );
            keys.press(KeyCode::KeyE);
            fixture(
                &mut controller,
                &mut dodge,
                &mut transform,
                HATCH + Vec3::Z * 1.35,
            );
            *phase = WorldCheck::InspectLocked;
        }
        WorldCheck::InspectLocked if ready(DungeonTarget::Hatch) => {
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
                    .text
                    .is_some_and(|text| text.contains("Restore the light"))
            );
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::OpenDiscoveredJournal;
        }
        WorldCheck::OpenDiscoveredJournal if dialogue.text.is_none() => {
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
            capture(&mut commands, "relay-quest-log");
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::SetUpTurnIn;
        }
        WorldCheck::SetUpTurnIn if !journal.is_open() => {
            fixture(
                &mut controller,
                &mut dodge,
                &mut transform,
                WorldLayout::Outpost.keeper() + Vec3::Z * 1.65,
            );
            *phase = WorldCheck::TurnIn;
        }
        WorldCheck::TurnIn if quest.is_complete(LAST_LIGHT) => {
            assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
            let text = dialogue
                .text
                .expect("first meeting completes The Last Light");
            assert!(text.contains("You found the core") && text.contains("Old Relay"));
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::ApproachHatch;
        }
        WorldCheck::TurnIn if quest_ready(Target::Keeper) => {
            keys.press(KeyCode::KeyE);
        }
        WorldCheck::ApproachHatch => {
            if ready(DungeonTarget::Hatch) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::Enter;
            } else {
                walk_to(&mut keys, &mut orbit, HATCH);
            }
        }
        WorldCheck::Enter if controller.grounded => {
            assert_eq!(controller.area, Area::OldRelay);
            assert!((position.y - (ORIGIN.y + 0.9)).abs() < 0.1);
            assert!(!progress.gate_open());
            assert_eq!(
                passage_hit(),
                Some(
                    gate.single()
                        .expect("the relay should have one archive gate")
                ),
                "the closed gate must block the player capsule"
            );
            capture(&mut commands, "relay-vestibule");
            *phase = WorldCheck::LureWorm;
        }
        WorldCheck::LureWorm => {
            if junction_worm(&worms).is_some_and(
                |worm| matches!(worm.phase, MeleePhase::Windup(remaining) if remaining <= 0.36),
            ) {
                orbit.yaw = 0.68;
                orbit.pitch = 0.32;
                capture(&mut commands, "relay-worm");
                *phase = WorldCheck::DodgeWorm(player_health.current());
            } else {
                walk_to(&mut keys, &mut orbit, ORIGIN + Vec3::new(0.1, 0.0, -8.7));
            }
        }
        WorldCheck::DodgeWorm(health) => {
            walk_to(&mut keys, &mut orbit, junction);
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
            if walk_to(&mut keys, &mut orbit, junction) {
                let (roach, _, _) = roaches.single().expect("Old Relay roach");
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
            let (roach, corpse, dead) = roaches.single().expect("persistent Old Relay roach");
            assert!(dead);
            if nearby.0 == Some((roach, InteractionTarget::Corpse)) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::TakeRoachMeat;
            } else {
                walk_to(&mut keys, &mut orbit, corpse.translation);
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
            if ready(DungeonTarget::Switch) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::OpenGateJournal;
            } else {
                walk_to(&mut keys, &mut orbit, ORIGIN + SWITCH_LOCAL);
            }
        }
        WorldCheck::OpenGateJournal => {
            assert!(!progress.gate_open());
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::GateRises;
        }
        WorldCheck::GateRises if progress.gate_open() && passage_hit().is_none() => {
            assert!(journal.is_on(MenuTab::Quest));
            assert!(quest.expects(QuestAction::ReadSurveyLog));
            keys.press(KeyCode::Tab);
            capture(&mut commands, "relay-maintenance");
            *phase = WorldCheck::ReturnToJunction;
        }
        WorldCheck::ReturnToJunction => {
            if walk_to(&mut keys, &mut orbit, junction) {
                *phase = WorldCheck::GateApproach;
            }
        }
        WorldCheck::GateApproach => {
            if walk_to(&mut keys, &mut orbit, ORIGIN + Vec3::new(0.0, 0.0, -14.2)) {
                *phase = WorldCheck::CrossGate;
            }
        }
        WorldCheck::CrossGate => {
            if walk_to(&mut keys, &mut orbit, ORIGIN + Vec3::new(0.0, 0.0, -21.0)) {
                assert!(position.z < ORIGIN.z + GATE_LOCAL.z);
                *phase = WorldCheck::ApproachLog;
            }
        }
        WorldCheck::ApproachLog => {
            if ready(DungeonTarget::SurveyLog) {
                orbit.yaw = 0.30;
                orbit.pitch = 0.23;
                *phase = WorldCheck::ReadLog;
                capture(&mut commands, "relay-archive");
            } else {
                walk_to(&mut keys, &mut orbit, ORIGIN + LOG_LOCAL);
            }
        }
        WorldCheck::ReadLog => {
            assert!(ready(DungeonTarget::SurveyLog));
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::ReadNotice;
        }
        WorldCheck::ReadNotice => {
            assert!(quest.is_complete(OLD_RELAY));
            assert!(
                dialogue
                    .text
                    .is_some_and(|text| text.starts_with("SURVEY LOG"))
            );
            capture(&mut commands, "relay-discovery");
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::OpenCompletedJournal;
        }
        WorldCheck::OpenCompletedJournal if dialogue.text.is_none() => {
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
            *phase = WorldCheck::LeaveArchive;
        }
        WorldCheck::LeaveArchive => {
            if dialogue.text.is_some() {
                keys.press(KeyCode::KeyE);
            } else if walk_to(&mut keys, &mut orbit, ORIGIN + Vec3::new(0.0, 0.0, -14.1)) {
                *phase = WorldCheck::LeaveJunction;
            }
        }
        WorldCheck::LeaveJunction => {
            if ready(DungeonTarget::Exit) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::Exit;
            } else {
                walk_to(&mut keys, &mut orbit, ORIGIN + Vec3::new(0.0, 0.0, -2.35));
            }
        }
        WorldCheck::Exit if controller.grounded => {
            assert_eq!(controller.area, Area::Surface);
            assert!((position.xz().distance(HATCH.xz()) - 2.1).abs() < 0.1);
            assert!(quest.is_complete(OLD_RELAY));
            *phase = WorldCheck::Reenter;
        }
        WorldCheck::Reenter if ready(DungeonTarget::Hatch) => {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::Revisited;
        }
        WorldCheck::Revisited if controller.grounded => {
            assert_eq!(controller.area, Area::OldRelay);
            assert!(quest.is_complete(OLD_RELAY) && progress.gate_open());
            assert!(
                roaches
                    .single()
                    .expect("the roach corpse persists across area changes")
                    .2
            );
            assert_eq!(inventory.count(ItemKind::RoachMeat), 1);
            assert_eq!(
                passage_hit(),
                None,
                "the gate must stay clear after returning"
            );
            transform.translation.y = -20.0;
            *phase = WorldCheck::Recover;
        }
        WorldCheck::Recover if controller.area == Area::Surface && controller.grounded => {
            assert!(position.xz().distance(WorldLayout::Outpost.start().xz()) < 0.1);
            assert!(quest.is_complete(OLD_RELAY) && progress.gate_open());
            assert_eq!(storage.get(ChestId::Lamp).count(ItemKind::Battery), 1);
            fixture(
                &mut controller,
                &mut dodge,
                &mut transform,
                Vec3::new(9.7, 0.0, -3.35),
            );
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
            commands.queue(|world: &mut World| {
                assert!(
                    world
                        .resource_mut::<Inventory>()
                        .remove(ItemKind::RoachMeat, 1)
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
            assert_eq!(player_health.current(), MAX_HEALTH);
            assert_eq!(inventory.count(ItemKind::RoachMeat), 1);
            assert_eq!(storage.get(ChestId::Lamp).count(ItemKind::Battery), 1);
            assert!(quest.is_complete(OLD_RELAY) && progress.gate_open());
            assert!(roaches.single().expect("saved roach").2);
            assert_eq!(passage_hit(), None, "restored gate must be clear");
            info!(
                "WORLD SMOKE PASS: tent storage, shipped map, Old Relay, and bed save rollback verified in {:.2}s",
                run.0.elapsed().as_secs_f64()
            );
            exit.write(AppExit::Success);
        }
        _ => {}
    }
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
