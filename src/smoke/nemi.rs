//! Exercise the hostile quest branch with the real spear and interaction input.
use super::{capture, inventory::pointer_click};
use crate::{
    camera::{FollowCamera, Orbit},
    combat::{Dead, Health},
    combat_feedback::DeathAnimation,
    dungeon::{Area, DungeonTarget, HATCH},
    equipment::Equipment,
    interaction::{InteractionTarget, Nearby, Target},
    inventory::{Inventory, ItemKind},
    menu_ui::{MenuTab, PlayerMenu},
    nemi::Nemi,
    player::{Dodge, Player},
    quest::{Beacon, Dialogue, LAST_LIGHT, OLD_RELAY, QuestJournal, QuestStatus, RELIGHT_BEACON},
    quest_ui::{QuestRow, QuestSelection},
    spear::{GRIP_SIDE, Thrust},
};
use avian3d::prelude::*;
use bevy::{app::AppExit, prelude::*};

#[derive(Resource, Debug, Default)]
pub(super) enum Check {
    #[default]
    Start,
    Attack(u16),
    Hit(u16),
    Flee,
    Return,
    Talk,
    CheckRefusal,
    OpenLog,
    SelectFailure,
    CloseLog,
    FinishRecovering,
    Collapse,
    RecoverCore,
    CoreNotice,
    InstallCore,
    BeaconNotice,
    EnterRelay,
    Done,
}

pub(super) fn drive(
    mut stage: ResMut<Check>,
    mut home: Local<Vec3>,
    mut avatar: Query<(Entity, &mut Player, &mut Dodge, &mut Transform), Without<Nemi>>,
    nemi: Query<
        (
            Entity,
            &Nemi,
            &Health,
            &Transform,
            Has<Dead>,
            Has<Collider>,
            &DeathAnimation,
        ),
        Without<Player>,
    >,
    targets: Query<(&Target, &Transform), (Without<Player>, Without<Nemi>)>,
    (quest, dialogue, nearby, log, selection): (
        Res<QuestJournal>,
        Res<Dialogue>,
        Res<Nearby>,
        Res<PlayerMenu>,
        Res<QuestSelection>,
    ),
    (mut bag, mut equipment, thrust): (ResMut<Inventory>, ResMut<Equipment>, Res<Thrust>),
    (lights, animations, rows): (
        Query<&PointLight, With<Beacon>>,
        Query<&AnimationPlayer>,
        Query<(Entity, &QuestRow)>,
    ),
    (window, camera): (
        Query<Entity, With<Window>>,
        Query<Entity, With<FollowCamera>>,
    ),
    (mut keys, mut mouse, mut orbit): (
        ResMut<ButtonInput<KeyCode>>,
        ResMut<ButtonInput<MouseButton>>,
        ResMut<Orbit>,
    ),
    spatial: SpatialQuery,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok((entity, mut player, mut dodge, mut transform)) = avatar.single_mut() else {
        return;
    };
    let (keeper, nemi, health, keeper_transform, dead, collider, death) = nemi
        .single()
        .expect("the shipped outpost must spawn a skinned Nemi");
    let position = keeper_transform.translation;
    keys.reset_all();
    mouse.reset_all();

    let fixture =
        |point: Vec3, player: &mut Player, dodge: &mut Dodge, transform: &mut Transform| {
            let probe = point + Vec3::Y * 1.4;
            let floor = spatial
                .cast_ray(
                    probe,
                    Dir3::NEG_Y,
                    6.0,
                    true,
                    &SpatialQueryFilter::from_excluded_entities([entity, keeper]),
                )
                .expect("smoke fixture needs ground");
            player.respawn(dodge, transform, probe - Vec3::Y * (floor.distance - 0.905));
        };
    let aim = |orbit: &mut Orbit, from: Vec3, target: Vec3| {
        let delta = (target - from).with_y(0.0);
        let offset = (-GRIP_SIDE / delta.length()).clamp(0.0, 1.0).asin();
        orbit.yaw = delta.x.atan2(delta.z) + offset - std::f32::consts::PI;
    };
    let near = |target| nearby.0.is_some_and(|(_, actual)| actual == target);
    let place =
        |target, offset, player: &mut Player, dodge: &mut Dodge, transform: &mut Transform| {
            let (_, anchor) = targets
                .iter()
                .find(|(kind, _)| **kind == target)
                .expect("quest target");
            fixture(anchor.translation + offset, player, dodge, transform);
        };

    match *stage {
        Check::Start if player.grounded => {
            assert!(bag.add(ItemKind::FieldSpear, 1));
            equipment
                .equip(&mut bag, ItemKind::FieldSpear)
                .expect("equip smoke spear");
            *home = position;
            fixture(
                position + Vec3::Z * 1.5,
                &mut player,
                &mut dodge,
                &mut transform,
            );
            *stage = Check::Attack(30);
        }
        Check::Attack(before) if player.grounded && !thrust.active() => {
            aim(&mut orbit, transform.translation, position);
            mouse.press(MouseButton::Left);
            *stage = Check::Hit(before);
        }
        Check::Hit(before) if health.current() < before => {
            assert_eq!(health.current(), before - 10);
            assert_eq!(quest.status(LAST_LIGHT), Some(QuestStatus::Failed));
            assert!(!quest.beacon_powered());
            assert!(!nemi.can_talk());
            *stage = match health.current() {
                20 => Check::Flee,
                10 => Check::FinishRecovering,
                _ => Check::Collapse,
            };
        }
        Check::Hit(_) => aim(&mut orbit, transform.translation, position),
        Check::Flee if position.xz().distance(home.xz()) >= 2.0 => {
            assert!(!dead && collider);
            capture(&mut commands, "nemi-flee");
            fixture(
                Vec3::new(0.0, 1.0, 17.0),
                &mut player,
                &mut dodge,
                &mut transform,
            );
            *stage = Check::Return;
        }
        Check::Return if nemi.refuses() => {
            assert!(position.xz().distance(home.xz()) < 0.2);
            fixture(
                position + Vec3::Z * 1.5,
                &mut player,
                &mut dodge,
                &mut transform,
            );
            *stage = Check::Talk;
        }
        Check::Talk if near(InteractionTarget::Quest(Target::Keeper)) => {
            keys.press(KeyCode::KeyE);
            *stage = Check::CheckRefusal;
        }
        Check::CheckRefusal => {
            assert!(dialogue.text.is_some_and(|text| text.contains("Stay away")));
            keys.press(KeyCode::KeyE);
            *stage = Check::OpenLog;
        }
        Check::OpenLog if dialogue.text.is_none() => {
            keys.press(KeyCode::Tab);
            *stage = Check::SelectFailure;
        }
        Check::SelectFailure => {
            assert!(log.is_on(MenuTab::Quest));
            assert!(
                rows.iter()
                    .any(|(_, row)| row.id == RELIGHT_BEACON && row.status == QuestStatus::Active)
            );
            let (row, _) = rows
                .iter()
                .find(|(_, row)| row.id == LAST_LIGHT && row.status == QuestStatus::Failed)
                .expect("The Last Light should appear under Failed");
            commands.trigger(pointer_click(
                row,
                window.single().expect("window"),
                camera.single().expect("camera"),
            ));
            *stage = Check::CloseLog;
        }
        Check::CloseLog => {
            assert_eq!(selection.selected(), Some(LAST_LIGHT));
            capture(&mut commands, "nemi-quest-failed");
            keys.press(KeyCode::Tab);
            *stage = Check::Attack(20);
        }
        Check::FinishRecovering if !thrust.active() => {
            fixture(
                position + Vec3::Z * 1.0,
                &mut player,
                &mut dodge,
                &mut transform,
            );
            *stage = Check::Attack(10);
        }
        Check::Collapse => {
            assert!(dead && !collider);
            let animation = animations
                .get(death.players[0])
                .expect("Nemi's animation player");
            if animation
                .animation(death.clip)
                .is_some_and(|clip| clip.is_finished())
            {
                orbit.yaw = -0.85;
                capture(&mut commands, "nemi-collapse");
                *stage = Check::RecoverCore;
            }
        }
        Check::RecoverCore if near(InteractionTarget::Quest(Target::Core)) => {
            keys.press(KeyCode::KeyE);
            *stage = Check::CoreNotice;
        }
        Check::RecoverCore => {
            place(
                Target::Core,
                Vec3::Z * 1.65,
                &mut player,
                &mut dodge,
                &mut transform,
            );
        }
        Check::CoreNotice => {
            assert_eq!(bag.count(ItemKind::BeaconCore), 1);
            assert!(
                dialogue
                    .text
                    .is_some_and(|text| text.contains("south side"))
            );
            keys.press(KeyCode::KeyE);
            place(
                Target::BeaconSocket,
                Vec3::Z * 1.25,
                &mut player,
                &mut dodge,
                &mut transform,
            );
            *stage = Check::InstallCore;
        }
        Check::InstallCore
            if dialogue.text.is_none() && near(InteractionTarget::Quest(Target::BeaconSocket)) =>
        {
            keys.press(KeyCode::KeyE);
            *stage = Check::BeaconNotice;
        }
        Check::BeaconNotice => {
            assert_eq!(bag.count(ItemKind::BeaconCore), 0);
            assert!(quest.is_complete(RELIGHT_BEACON));
            assert_eq!(quest.status(LAST_LIGHT), Some(QuestStatus::Failed));
            assert_eq!(quest.status(OLD_RELAY), Some(QuestStatus::Active));
            assert!(lights.single().expect("the beacon light").intensity > 0.0);
            keys.press(KeyCode::KeyE);
            fixture(
                HATCH + Vec3::Z * 1.35,
                &mut player,
                &mut dodge,
                &mut transform,
            );
            *stage = Check::EnterRelay;
        }
        Check::EnterRelay
            if dialogue.text.is_none()
                && near(InteractionTarget::Dungeon(DungeonTarget::Hatch)) =>
        {
            keys.press(KeyCode::KeyE);
            *stage = Check::Done;
        }
        Check::Done => {
            assert_eq!(player.area, Area::OldRelay);
            info!("NEMI SMOKE PASS: fled, returned, died, and the solo route opened the relay");
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}
