//! Exercise the shipped relay with the real movement controller and interact input.
use super::{SmokeRun, capture, inventory::walk};
use crate::{
    camera::Orbit,
    dungeon::{
        ArchiveGate, Area, DungeonProgress, DungeonTarget, GATE_LOCAL, HATCH, LOG_LOCAL, ORIGIN,
        SWITCH_LOCAL,
    },
    interaction::{InteractionTarget, Nearby, Target},
    inventory::{Inventory, ItemKind},
    player::Player,
    quest::{Dialogue, Quest},
    world::WorldLayout,
    worm::{GROUND_OFFSET, Worm, WormPhase},
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
    SetUpTurnIn,
    TurnIn,
    ApproachHatch,
    Enter,
    LureWorm,
    DodgeWorm(u16),
    Junction,
    Switch,
    GateRises,
    ReturnToJunction,
    GateApproach,
    CrossGate,
    ApproachLog,
    ReadLog,
    ReadNotice,
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
    mut player: Query<(Entity, &mut Player, &mut Transform, &Collider)>,
    (mut quest, mut inventory, dialogue, nearby): (
        ResMut<Quest>,
        ResMut<Inventory>,
        Res<Dialogue>,
        Res<Nearby>,
    ),
    progress: Res<DungeonProgress>,
    gate: Query<Entity, With<ArchiveGate>>,
    worms: Query<(Entity, &Transform, &Worm), Without<Player>>,
    captures: Query<(), With<Screenshot>>,
    spatial: SpatialQuery,
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok((entity, mut controller, mut transform, capsule)) = player.single_mut() else {
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
    let fixture = |player: &mut Player, transform: &mut Transform, point: Vec3| {
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
        player.respawn(transform, probe - Vec3::Y * (support.distance - 0.905));
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
            fixture(&mut controller, &mut transform, HATCH + Vec3::Z * 1.35);
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
            assert_eq!(*quest, Quest::MeetKeeper);
            assert!(
                dialogue
                    .text
                    .is_some_and(|text| text.contains("Restore the light"))
            );
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::SetUpTurnIn;
        }
        WorldCheck::SetUpTurnIn if dialogue.text.is_none() => {
            // The arena already verifies the quest from its first conversation.
            fixture(
                &mut controller,
                &mut transform,
                WorldLayout::Outpost.keeper() + Vec3::Z * 1.65,
            );
            assert!(inventory.add(ItemKind::BeaconCore, 1));
            *quest = Quest::ReturnCore;
            *phase = WorldCheck::TurnIn;
        }
        WorldCheck::TurnIn if *quest == Quest::Complete => {
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
                |worm| matches!(worm.phase, WormPhase::Windup(remaining) if remaining <= 0.36),
            ) {
                orbit.yaw = 0.68;
                orbit.pitch = 0.32;
                capture(&mut commands, "relay-worm");
                *phase = WorldCheck::DodgeWorm(controller.vitality.health);
            } else {
                walk_to(&mut keys, &mut orbit, ORIGIN + Vec3::new(0.1, 0.0, -8.7));
            }
        }
        WorldCheck::DodgeWorm(health) => {
            walk_to(&mut keys, &mut orbit, junction);
            if junction_worm(&worms)
                .is_some_and(|worm| matches!(worm.phase, WormPhase::Recovery(_)))
            {
                assert_eq!(
                    controller.vitality.health, health,
                    "the relay must leave room to dodge"
                );
                *phase = WorldCheck::Junction;
            }
        }
        WorldCheck::Junction => {
            if walk_to(&mut keys, &mut orbit, junction) {
                *phase = WorldCheck::Switch;
            }
        }
        WorldCheck::Switch => {
            if ready(DungeonTarget::Switch) {
                keys.press(KeyCode::KeyE);
                *phase = WorldCheck::GateRises;
            } else {
                walk_to(&mut keys, &mut orbit, ORIGIN + SWITCH_LOCAL);
            }
        }
        WorldCheck::GateRises if progress.gate_open() && passage_hit().is_none() => {
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
            assert!(progress.explored);
            assert!(
                dialogue
                    .text
                    .is_some_and(|text| text.starts_with("SURVEY LOG"))
            );
            capture(&mut commands, "relay-discovery");
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
            assert!(progress.explored);
            *phase = WorldCheck::Reenter;
        }
        WorldCheck::Reenter if ready(DungeonTarget::Hatch) => {
            keys.press(KeyCode::KeyE);
            *phase = WorldCheck::Revisited;
        }
        WorldCheck::Revisited if controller.grounded => {
            assert_eq!(controller.area, Area::OldRelay);
            assert!(progress.explored && progress.gate_open());
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
            assert!(progress.explored && progress.gate_open());
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
    worms: &'a Query<(Entity, &Transform, &Worm), Without<Player>>,
) -> Option<&'a Worm> {
    worms
        .iter()
        .find(|(_, transform, _)| {
            transform.translation.y < -7.0 && transform.translation.z > ORIGIN.z - 15.0
        })
        .map(|(_, _, worm)| worm)
}
