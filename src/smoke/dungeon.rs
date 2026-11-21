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
    interaction::{InteractionTarget, Nearby, Target},
    inventory::{Inventory, ItemKind},
    loot_ui::LootAction,
    player::{Dodge, Player},
    quest::{Dialogue, LAST_LIGHT, OLD_RELAY, QuestAction, QuestJournal, QuestStatus},
    quest_ui::{QuestLog, QuestRow},
    roach::Roach,
    world::WorldLayout,
    worm::{GROUND_OFFSET, Worm},
};
use avian3d::prelude::*;
use bevy::{app::AppExit, prelude::*, render::view::screenshot::Screenshot};

#[derive(Resource, Debug, Default)]
pub(super) enum WorldCheck {
    #[default]
    Spawn,
    OutpostPhoto(Entity),
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
    (mut quest, mut inventory, dialogue, nearby): (
        ResMut<QuestJournal>,
        ResMut<Inventory>,
        Res<Dialogue>,
        Res<Nearby>,
    ),
    (progress, journal, rows): (
        Res<DungeonProgress>,
        Res<QuestLog>,
        Query<(Entity, &QuestRow)>,
    ),
    gate: Query<Entity, With<ArchiveGate>>,
    worms: Query<(Entity, &Transform, &MeleePursuit), (With<Worm>, Without<Player>)>,
    captures: Query<(), With<Screenshot>>,
    (state, roaches, actions, window, camera): (
        Res<State<GameState>>,
        Query<(Entity, &Transform, Has<Dead>), (With<Roach>, Without<Player>)>,
        Query<(Entity, &LootAction)>,
        Query<Entity, With<Window>>,
        Query<Entity, With<FollowCamera>>,
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
            assert!(position.xz().distance(WorldLayout::Outpost.start().xz()) < 0.01);
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
            *phase = WorldCheck::OutpostPhoto(capture(&mut commands, "outpost"));
        }
        WorldCheck::OutpostPhoto(photo) if captures.get(photo).is_err() => {
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
            assert!(journal.is_open());
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
            assert_eq!(journal.selected(), Some(OLD_RELAY));
            capture(&mut commands, "relay-quest-log");
            keys.press(KeyCode::Tab);
            *phase = WorldCheck::SetUpTurnIn;
        }
        WorldCheck::SetUpTurnIn if dialogue.text.is_none() => {
            // The arena already verifies the quest from its first conversation.
            fixture(
                &mut controller,
                &mut dodge,
                &mut transform,
                WorldLayout::Outpost.keeper() + Vec3::Z * 1.65,
            );
            assert!(inventory.add(ItemKind::BeaconCore, 1));
            quest.record(QuestAction::TalkedToNemi);
            quest.record(QuestAction::RecoveredCore);
            *phase = WorldCheck::TurnIn;
        }
        WorldCheck::TurnIn if quest.is_complete(LAST_LIGHT) => {
            assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
            assert!(dialogue.text.is_some_and(|text| text.contains("Old Relay")));
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::ApproachHatch;
        }
        WorldCheck::TurnIn
            if matches!(
                nearby.0,
                Some((_, InteractionTarget::Quest(Target::Keeper)))
            ) =>
        {
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
            assert!(journal.is_open());
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
            assert!(journal.is_open());
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
            info!(
                "WORLD SMOKE PASS: shipped map, pillars, and Old Relay exploration verified in {:.2}s",
                run.0.elapsed().as_secs_f64()
            );
            exit.write(AppExit::Success);
        }
        _ => {}
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
