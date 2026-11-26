//! Drive the crossing against the shipped assets and regular combat and dialogue systems.
use super::{
    SmokeRun, capture,
    inventory::{pointer_click, walk},
};
use crate::{
    camera::{FollowCamera, Orbit},
    cave::CaveEncounter,
    combat::{Dead, Hit},
    combat_feedback::DeathAnimation,
    content::NpcId,
    crossing::{Caravan, CrossingGate, SampleStation, village::Homebound},
    dialogue::{Dialogue, Screen, Speaker},
    guard::GuardPost,
    hushcap::Hushcap,
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind},
    player::PlayerWrite,
    player::{Dodge, NpcGait, PLAYER_GROUND_OFFSET, Player},
    quest::{CONTAIN_THE_HUSHCAPS, QuestAction, QuestJournal},
    save::{Bed, EncounterId, RunSave, SaveAtBed},
    terrain::walkable_height,
    ui::death_screen::ContinueAfterDeath,
};
use avian3d::prelude::*;
use bevy::{
    app::AppExit, ecs::system::SystemParam, prelude::*, render::view::screenshot::Screenshot,
};

#[derive(Debug, Default, Resource)]
pub(super) enum Check {
    #[default]
    EarlyKill,
    Activate,
    FifthKill,
    Opening,
    FirstArrivalPhoto(Entity),
    Parked,
    Settling(ArrivalWatch),
    RoomView(usize),
    RoomPhoto(usize, Entity),
    Bench,
    BenchResult,
    ReloadHomes(Entity),
    Speak,
    Samples,
    CloseReply,
    CloseTopics,
    Walk(usize),
    FlatsPhoto(Entity),
    LookoutView,
    LookoutPhoto(Entity),
    Save,
    Defeat,
    Restored(Entity),
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ArrivalWatch {
    walked: u8,
    staggered: bool,
    courtyard_photo: bool,
}

#[derive(Default)]
pub(super) enum DeathScreenCheck {
    #[default]
    WatchCollapse,
    WaitForBlack,
    WaitForCapture(Entity),
}

pub(super) fn check_death_screen(
    phase: Res<Check>,
    mut check: Local<DeathScreenCheck>,
    player: Single<(Entity, &DeathAnimation), (With<Player>, With<Dead>)>,
    animations: Query<&AnimationPlayer>,
    button: Single<(Entity, &Visibility), With<ContinueAfterDeath>>,
    captures: Query<(), With<Screenshot>>,
    window: Single<Entity, With<Window>>,
    camera: Single<Entity, With<FollowCamera>>,
    mut commands: Commands,
) {
    let defeated = match *phase {
        Check::ReloadHomes(defeated) | Check::Restored(defeated) => defeated,
        _ => panic!("crossing smoke entered GameOver from {phase:?}"),
    };
    let (entity, death) = *player;
    assert_eq!(entity, defeated, "Continue must precede crossing reload");
    let animation = animations
        .get(death.players[0])
        .expect("player animation must remain bound during GameOver")
        .animation(death.clip)
        .expect("the collapse must be the active death clip");
    let (button, visibility) = *button;
    match *check {
        DeathScreenCheck::WatchCollapse => {
            assert_eq!(*visibility, Visibility::Hidden);
            assert!(!animation.is_finished());
            if animation.seek_time() >= 0.5 {
                capture(&mut commands, "crossing-death-fade");
                *check = DeathScreenCheck::WaitForBlack;
            }
        }
        DeathScreenCheck::WaitForBlack if *visibility == Visibility::Inherited => {
            assert!(animation.is_finished());
            *check = DeathScreenCheck::WaitForCapture(capture(&mut commands, "crossing-game-over"));
        }
        DeathScreenCheck::WaitForCapture(screenshot) if !captures.contains(screenshot) => {
            commands.trigger(pointer_click(button, *window, *camera));
            *check = DeathScreenCheck::WatchCollapse;
        }
        _ => {}
    }
}

const VILLAGE_HOMES: [(NpcId, Vec2); 4] = [
    (NpcId::Isera, Vec2::new(-8.15, -1.0)),
    (NpcId::Vey, Vec2::new(-5.8, -2.65)),
    (NpcId::Tovan, Vec2::new(-11.70, 7.22)),
    (NpcId::Orla, Vec2::new(8.55, 9.0)),
];

const ROOM_VIEWS: [(&str, Vec2, f32); 5] = [
    (
        "nivalis-family-hut",
        Vec2::new(-9.25, -0.45),
        std::f32::consts::FRAC_PI_2,
    ),
    (
        "nivalis-orla-hut",
        Vec2::new(10.30, 10.0),
        -std::f32::consts::FRAC_PI_2,
    ),
    (
        "nivalis-keeper-hut",
        Vec2::new(5.5, 16.04),
        std::f32::consts::PI,
    ),
    (
        "nivalis-barracks",
        Vec2::new(-7.30, -10.15),
        std::f32::consts::FRAC_PI_2,
    ),
    (
        "nivalis-workshop",
        Vec2::new(-11.35, 8.17),
        std::f32::consts::FRAC_PI_2,
    ),
];

const ELIGIBLE: [CaveEncounter; 5] = [
    CaveEncounter::FirstWestHushcap,
    CaveEncounter::FirstEastHushcap,
    CaveEncounter::SplitWestHushcap,
    CaveEncounter::SplitEastHushcap,
    CaveEncounter::SplitFarHushcap,
];

const FLATS_ROUTE: &[(f32, f32)] = &[
    (-30.0, 43.0),
    (-34.5, 49.0),
    (-42.0, 55.0),
    (-53.0, 59.0),
    (-61.0, 63.0),
    (-75.0, 67.0),
    (-89.0, 73.0),
    (-103.0, 78.0),
    (-107.5, 89.0),
    (-99.0, 97.0),
    (-82.0, 96.0),
    (-76.5, 94.0),
    (-65.0, 88.0),
    (-62.0, 72.0),
    (-61.0, 63.0),
];

#[derive(SystemParam)]
pub(super) struct Fixtures<'w, 's> {
    enemies: Query<'w, 's, (Entity, &'static EncounterId, Has<Dead>), With<Hushcap>>,
    gate: Query<'w, 's, (Entity, &'static CrossingGate)>,
    caravans: Query<'w, 's, &'static Caravan>,
    speakers: Query<'w, 's, (Entity, &'static Speaker, &'static GlobalTransform)>,
    villagers: Query<
        'w,
        's,
        (
            Entity,
            &'static NpcId,
            &'static crate::combat::Health,
            Has<Dead>,
            Has<Collider>,
        ),
        With<crate::nemi::FleeState>,
    >,
    arrivals: Query<
        'w,
        's,
        (
            &'static NpcId,
            &'static NpcGait,
            &'static GlobalTransform,
            Has<Homebound>,
            Has<Speaker>,
        ),
    >,
    guards: Query<
        'w,
        's,
        (
            &'static GuardPost,
            &'static GlobalTransform,
            &'static NpcGait,
        ),
    >,
    bench: Query<'w, 's, Entity, With<SampleStation>>,
    nearby: Res<'w, Nearby>,
    beds: Query<'w, 's, Entity, With<Bed>>,
    captures: Query<'w, 's, (), With<Screenshot>>,
    spatial: SpatialQuery<'w, 's>,
}

pub(super) fn drive(
    mut check: ResMut<Check>,
    mut player: Query<(Entity, &mut Player, &mut Dodge, &mut Transform, &Collider)>,
    mut journal: ResMut<QuestJournal>,
    mut inventory: PlayerWrite<Inventory>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    dialogue: Res<Dialogue>,
    run: Res<RunSave>,
    fixtures: Fixtures,
    mut hits: MessageWriter<Hit>,
    mut saves: MessageWriter<SaveAtBed>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok((entity, mut controller, mut dodge, mut pose, capsule)) = player.single_mut() else {
        return;
    };
    let Ok((gate_entity, gate)) = fixtures.gate.single() else {
        return;
    };
    keys.reset_all();
    match *check {
        Check::EarlyKill | Check::Activate | Check::FifthKill | Check::Opening => {
            prepare_arrival(
                &mut check,
                &fixtures,
                entity,
                &mut controller,
                &mut dodge,
                &mut pose,
                capsule,
                gate_entity,
                gate,
                &mut journal,
                &mut orbit,
                &mut hits,
                &mut commands,
            );
        }
        Check::FirstArrivalPhoto(photo) if fixtures.captures.get(photo).is_err() => {
            *check = Check::Parked
        }
        Check::Parked
        | Check::Settling(_)
        | Check::RoomView(_)
        | Check::RoomPhoto(_, _)
        | Check::Bench
        | Check::BenchResult
        | Check::ReloadHomes(_) => check_homecoming(
            &mut check,
            &fixtures,
            entity,
            &mut controller,
            &mut dodge,
            &mut pose,
            capsule,
            &journal,
            &mut inventory,
            &mut keys,
            &mut orbit,
            &mut hits,
            &mut saves,
            &mut commands,
        ),
        Check::Speak if controller.grounded => {
            keys.press(KeyCode::KeyE);
            *check = Check::Samples;
        }
        Check::Samples => {
            assert!(matches!(
                dialogue.screen(),
                Some(Screen::Conversation {
                    profile: "tovan",
                    ..
                })
            ));
            keys.press(KeyCode::Enter);
            *check = Check::CloseReply;
        }
        Check::CloseReply => {
            assert!(journal.is_complete(CONTAIN_THE_HUSHCAPS));
            assert_eq!(inventory.count(ItemKind::Mycelium), 5);
            keys.press(KeyCode::Enter);
            *check = Check::CloseTopics;
        }
        Check::CloseTopics => {
            keys.press(KeyCode::Escape);
            at(
                &fixtures,
                entity,
                &mut controller,
                &mut dodge,
                &mut pose,
                Vec2::new(-30.0, 20.0),
            );
            *check = Check::Walk(0);
        }
        Check::Walk(index) => {
            if let Some(next) = walk_flats(
                index,
                pose.translation,
                &mut keys,
                &mut orbit,
                &mut commands,
            ) {
                *check = next;
            }
        }
        Check::FlatsPhoto(_) | Check::LookoutView | Check::LookoutPhoto(_) => {
            capture_lookout(
                &mut check,
                &fixtures,
                entity,
                &mut controller,
                &mut dodge,
                &mut pose,
                &mut orbit,
                &mut commands,
            );
        }
        Check::Save => {
            save_with_casualty(&fixtures, entity, &mut hits, &mut saves);
            *check = Check::Defeat;
        }
        Check::Defeat if run.checkpoint().is_some() => {
            hits.write(Hit {
                source: entity,
                target: entity,
                damage: u16::MAX,
                contact: None,
            });
            *check = Check::Restored(entity);
        }
        Check::Restored(original) if original != entity && !fixtures.caravans.is_empty() => {
            check_restored(&fixtures, gate, &journal, &inventory);
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn check_homecoming(
    check: &mut Check,
    fixtures: &Fixtures,
    player: Entity,
    controller: &mut Player,
    dodge: &mut Dodge,
    pose: &mut Transform,
    capsule: &Collider,
    journal: &QuestJournal,
    inventory: &mut Inventory,
    keys: &mut ButtonInput<KeyCode>,
    orbit: &mut Orbit,
    hits: &mut MessageWriter<Hit>,
    saves: &mut MessageWriter<SaveAtBed>,
    commands: &mut Commands,
) {
    match *check {
        Check::Parked if journal.has_occurred(QuestAction::CaravanArrived) => {
            for (id, _) in VILLAGE_HOMES {
                let (_, _, homebound, speaker) = resident(fixtures, id);
                assert!(
                    homebound && !speaker,
                    "{id:?} must leave the parked wagon before talking"
                );
            }
            assert!(inventory.add(ItemKind::Mycelium, 5));
            // Capture a real checkpoint while the homeward routes are active.
            saves.write(SaveAtBed(
                fixtures
                    .beds
                    .single()
                    .expect("the player’s cot saves the run"),
            ));
            at(
                fixtures,
                player,
                controller,
                dodge,
                pose,
                Vec2::new(-6.6, 15.4),
            );
            orbit.yaw = 0.45;
            orbit.pitch = 0.15;
            *check = Check::Settling(ArrivalWatch::default());
        }
        Check::Settling(mut watch) => {
            observe_arrival(fixtures, &mut watch, commands);
            if check_home_positions(fixtures) {
                assert_eq!(watch.walked, 0b1111);
                assert!(watch.staggered);
                check_village_rooms(fixtures, player, capsule);
                frame_room(fixtures, (player, controller, dodge, pose), orbit, 0);
                *check = Check::RoomView(0);
            } else if let Check::Settling(previous) = check {
                *previous = watch;
            }
        }
        Check::RoomView(index) => {
            *check = Check::RoomPhoto(index, capture(commands, ROOM_VIEWS[index].0));
        }
        Check::RoomPhoto(index, photo) if fixtures.captures.get(photo).is_err() => {
            let next = index + 1;
            if next < ROOM_VIEWS.len() {
                frame_room(fixtures, (player, controller, dodge, pose), orbit, next);
                *check = Check::RoomView(next);
                return;
            }
            orbit.distance = Orbit::default().distance;
            let (tovan, ..) = fixtures
                .villagers
                .iter()
                .find(|(_, id, ..)| **id == NpcId::Tovan)
                .expect("Tovan has reached his workshop");
            hits.write(Hit {
                source: player,
                target: tovan,
                damage: u16::MAX,
                contact: None,
            });
            at(
                fixtures,
                player,
                controller,
                dodge,
                pose,
                Vec2::new(-12.60, 7.48),
            );
            *check = Check::Bench;
        }
        Check::Bench
            if controller.grounded
                && fixtures.bench.single().is_ok_and(|bench| {
                    fixtures.nearby.0 == Some((bench, InteractionTarget::SampleStation))
                }) =>
        {
            keys.press(KeyCode::KeyE);
            *check = Check::BenchResult;
        }
        Check::BenchResult => {
            assert!(journal.is_complete(CONTAIN_THE_HUSHCAPS));
            assert_eq!(inventory.count(ItemKind::Mycelium), 5);
            hits.write(Hit {
                source: player,
                target: player,
                damage: u16::MAX,
                contact: None,
            });
            *check = Check::ReloadHomes(player);
        }
        Check::ReloadHomes(original) if player != original && check_home_positions(fixtures) => {
            assert!(journal.expects(QuestAction::ShowedTovanMycelium));
            assert_eq!(inventory.count(ItemKind::Mycelium), 5);
            at(
                fixtures,
                player,
                controller,
                dodge,
                pose,
                Vec2::new(-10.65, 8.0),
            );
            *check = Check::Speak;
        }
        _ => {}
    }
}

fn frame_room(
    fixtures: &Fixtures,
    (player, controller, dodge, pose): (Entity, &mut Player, &mut Dodge, &mut Transform),
    orbit: &mut Orbit,
    index: usize,
) {
    let (_, point, yaw) = ROOM_VIEWS[index];
    at(fixtures, player, controller, dodge, pose, point);
    orbit.yaw = yaw;
    orbit.pitch = 0.22;
    orbit.distance = 3.0;
}

fn resident(fixtures: &Fixtures, id: NpcId) -> (Vec2, NpcGait, bool, bool) {
    let (_, gait, pose, homebound, speaker) = fixtures
        .arrivals
        .iter()
        .find(|(npc, ..)| **npc == id)
        .expect("each caravan villager is spawned");
    (pose.translation().xz(), *gait, homebound, speaker)
}

fn observe_arrival(fixtures: &Fixtures, watch: &mut ArrivalWatch, commands: &mut Commands) {
    let mut moving = 0;
    let mut waiting = 0;
    for (i, (id, _)) in VILLAGE_HOMES.into_iter().enumerate() {
        let (_, gait, homebound, speaker) = resident(fixtures, id);
        if homebound {
            assert!(!speaker, "{id:?} must reach home before talking");
            if matches!(gait, NpcGait::Walk(_)) {
                moving += 1;
                watch.walked |= 1 << i;
            } else {
                waiting += 1;
            }
        }
    }
    for (post, pose, gait) in &fixtures.guards {
        assert_eq!(*gait, NpcGait::Idle);
        assert!(pose.translation().xz().distance(post.home().xz()) < 0.25);
    }
    watch.staggered |= moving > 0 && waiting > 0;
    if !watch.courtyard_photo
        && resident(fixtures, NpcId::Orla).0.x > -12.0
        && resident(fixtures, NpcId::Orla).0.x < -7.5
    {
        capture(commands, "nivalis-homecoming");
        watch.courtyard_photo = true;
    }
}

fn check_home_positions(fixtures: &Fixtures) -> bool {
    fixtures.caravans.single().is_ok()
        && fixtures.bench.single().is_ok()
        && VILLAGE_HOMES.into_iter().all(|(id, home)| {
            let (position, gait, homebound, speaker) = resident(fixtures, id);
            !homebound && speaker && gait == NpcGait::Idle && position.distance(home) < 0.25
        })
        && fixtures.guards.iter().count() == GuardPost::ALL.len()
        && fixtures.guards.iter().all(|(post, pose, gait)| {
            *gait == NpcGait::Idle && pose.translation().xz().distance(post.home().xz()) < 0.25
        })
}

fn check_village_rooms(fixtures: &Fixtures, player: Entity, capsule: &Collider) {
    let filter = SpatialQueryFilter::from_excluded_entities(
        std::iter::once(player).chain(fixtures.villagers.iter().map(|(entity, ..)| entity)),
    );
    for (name, outside, inside) in [
        ("Family", (-4.9, -1.0), (-11.3, -1.0)),
        ("Workshop", (-7.85, 8.0), (-14.1, 8.0)),
        ("Orla", (7.9, 10.0), (13.2, 10.0)),
        ("Keeper", (5.5, 13.6), (5.5, 17.9)),
        ("Barracks", (-3.6, -10.15), (-10.3, -10.15)),
    ] {
        let start = Vec2::from(outside);
        let end = Vec2::from(inside);
        let steps = (start.distance(end) / 0.20).ceil() as u32;
        let mut previous = None;
        for i in 0..=steps {
            let point = start.lerp(end, i as f32 / steps as f32);
            let probe = Vec3::new(point.x, 0.55, point.y);
            let floor = walkable_height(&fixtures.spatial, &filter, probe, 2.5, 0.0)
                .unwrap_or_else(|| panic!("{name} has no walkable floor at {point}"));
            if let Some(y) = previous {
                assert!(
                    f32::abs(floor - y) < crate::terrain::MAX_GROUND_STEP,
                    "{name} threshold at {point}"
                );
            }
            assert!(
                fixtures
                    .spatial
                    .shape_intersections(
                        capsule,
                        probe.with_y(floor + PLAYER_GROUND_OFFSET + 0.06),
                        Quat::IDENTITY,
                        &filter
                    )
                    .is_empty(),
                "{name} doorway or aisle is blocked at {point}"
            );
            previous = Some(floor);
        }
    }
}

fn save_with_casualty(
    fixtures: &Fixtures,
    player: Entity,
    hits: &mut MessageWriter<Hit>,
    saves: &mut MessageWriter<SaveAtBed>,
) {
    let (villager, _, health, dead, _) = fixtures
        .villagers
        .iter()
        .find(|(_, id, ..)| **id == NpcId::Isera)
        .expect("Isera is a damageable parked speaker");
    assert_eq!(health.current(), 100);
    assert!(!dead);
    hits.write(Hit {
        source: player,
        target: villager,
        damage: u16::MAX,
        contact: None,
    });
    saves.write(SaveAtBed(
        fixtures.beds.single().expect("the cot is available"),
    ));
}

fn walk_flats(
    index: usize,
    position: Vec3,
    keys: &mut ButtonInput<KeyCode>,
    orbit: &mut Orbit,
    commands: &mut Commands,
) -> Option<Check> {
    let goal = Vec2::from(FLATS_ROUTE[index]);
    if position.xz().distance(goal) < 0.55 {
        if index + 1 == FLATS_ROUTE.len() {
            Some(Check::FlatsPhoto(capture(
                commands,
                "crossing-listening-flats",
            )))
        } else {
            Some(Check::Walk(index + 1))
        }
    } else {
        walk(keys, orbit, position, Vec3::new(goal.x, 0.0, goal.y));
        None
    }
}

fn capture_lookout(
    check: &mut Check,
    fixtures: &Fixtures,
    entity: Entity,
    controller: &mut Player,
    dodge: &mut Dodge,
    pose: &mut Transform,
    orbit: &mut Orbit,
    commands: &mut Commands,
) {
    match *check {
        Check::FlatsPhoto(photo) if fixtures.captures.get(photo).is_err() => {
            assert!(pose.translation.x < -55.0);
            let probe = Vec3::new(-117.0, 12.0, 70.0);
            let filter = SpatialQueryFilter::from_excluded_entities([entity]);
            let y = walkable_height(&fixtures.spatial, &filter, probe, 3.0, PLAYER_GROUND_OFFSET)
                .expect("the lookout has solid footing");
            let lookout = probe.with_y(y);
            controller.respawn(dodge, pose, lookout);
            let delta = Vec3::new(-72.0, 65.0, -100.0) - (lookout + Vec3::Y * 0.65);
            let dir = Dir3::new(delta).expect("Charon is across the valley");
            assert!(
                fixtures
                    .spatial
                    .cast_ray(lookout + Vec3::Y * 0.65, dir, delta.length(), true, &filter)
                    .is_none(),
                "a notch in the rim should frame Charon"
            );
            orbit.yaw = (-delta.x).atan2(-delta.z);
            orbit.pitch = -0.15;
            *check = Check::LookoutView;
        }
        Check::LookoutView => {
            *check = Check::LookoutPhoto(capture(commands, "crossing-charon-overlook"));
        }
        Check::LookoutPhoto(photo) if fixtures.captures.get(photo).is_err() => {
            at(
                fixtures,
                entity,
                controller,
                dodge,
                pose,
                Vec2::new(8.0, -3.55),
            );
            *check = Check::Save;
        }
        _ => {}
    }
}

fn check_restored(
    fixtures: &Fixtures,
    gate: &CrossingGate,
    journal: &QuestJournal,
    inventory: &Inventory,
) {
    assert!(gate.open());
    assert_eq!(fixtures.caravans.iter().count(), 1);
    check_villagers(fixtures);
    assert!(journal.is_complete(CONTAIN_THE_HUSHCAPS));
    assert_eq!(journal.cavern_defeat_count(), 5);
    assert_eq!(inventory.count(ItemKind::Mycelium), 5);
    let (_, _, health, dead, collider) = fixtures
        .villagers
        .iter()
        .find(|(_, id, ..)| **id == NpcId::Isera)
        .expect("the parked caravan reconstructs Isera");
    assert_eq!(health.current(), 0);
    assert!(dead && !collider);
    info!(
        "CROSSING SMOKE PASS: caravan homecoming, five walkable huts, Tovan’s two quest routes, flats loop, and two checkpoint restores"
    );
}

fn prepare_arrival(
    check: &mut Check,
    fixtures: &Fixtures,
    entity: Entity,
    controller: &mut Player,
    dodge: &mut Dodge,
    pose: &mut Transform,
    capsule: &Collider,
    gate_entity: Entity,
    gate: &CrossingGate,
    journal: &mut QuestJournal,
    orbit: &mut Orbit,
    hits: &mut MessageWriter<Hit>,
    commands: &mut Commands,
) {
    match *check {
        Check::EarlyKill if controller.grounded => {
            assert!(!gate.open());
            let origin = Vec3::new(-30.0, -0.27, 22.0);
            let filter = SpatialQueryFilter::from_excluded_entities([entity]);
            let hit = fixtures
                .spatial
                .cast_shape(
                    capsule,
                    origin,
                    Quat::IDENTITY,
                    Dir3::Z,
                    &ShapeCastConfig::from_max_distance(8.0),
                    &filter,
                )
                .expect("the gate should block the road");
            assert_eq!(
                hit.entity, gate_entity,
                "the first hit should be the leaf at {hit:?}"
            );
            kill(fixtures, hits, entity, CaveEncounter::GalleryFarHushcap);
            *check = Check::Activate;
        }
        Check::Activate => {
            assert!(fixtures.enemies.iter().any(|(_, id, dead)| {
                *id == EncounterId::Cave(CaveEncounter::GalleryFarHushcap) && dead
            }));
            assert_eq!(journal.cavern_defeat_count(), 0);
            for action in [
                QuestAction::TalkedToNemi,
                QuestAction::RecoveredCore,
                QuestAction::ReturnedCore,
                QuestAction::EnteredRelay,
                QuestAction::OpenedArchiveGate,
                QuestAction::ReadSurveyLog,
            ] {
                journal.record(action);
            }
            journal.offer_cavern_report();
            journal.record(QuestAction::ReportedArchiveHushcap);
            for id in &ELIGIBLE[..4] {
                kill(fixtures, hits, entity, *id);
            }
            at(
                fixtures,
                entity,
                controller,
                dodge,
                pose,
                Vec2::new(-17.0, 19.0),
            );
            orbit.yaw = 2.12;
            orbit.pitch = -0.08;
            *check = Check::FifthKill;
        }
        Check::FifthKill => {
            assert_eq!(journal.cavern_defeat_count(), 4);
            assert!(!gate.open() && fixtures.caravans.is_empty());
            kill(fixtures, hits, entity, ELIGIBLE[4]);
            *check = Check::Opening;
        }
        Check::Opening
            if gate.open()
                && fixtures
                    .caravans
                    .single()
                    .is_ok_and(Caravan::crossed_threshold) =>
        {
            assert_eq!(journal.cavern_defeat_count(), 5);
            assert!(journal.expects(QuestAction::ShowedTovanMycelium));
            assert_eq!(fixtures.caravans.iter().count(), 1);
            let route: Vec<_> = std::iter::once((-30.0, 20.0))
                .chain(FLATS_ROUTE.iter().copied())
                .collect();
            super::cave::check_route(
                &fixtures.spatial,
                &SpatialQueryFilter::from_excluded_entities([entity]),
                capsule,
                &route,
                -1.46,
                0.24,
            );
            *check = Check::FirstArrivalPhoto(capture(commands, "crossing-gate-open"));
        }
        _ => {}
    }
}

fn kill(fixtures: &Fixtures, hits: &mut MessageWriter<Hit>, player: Entity, id: CaveEncounter) {
    let (entity, _, dead) = fixtures
        .enemies
        .iter()
        .find(|(_, key, _)| **key == EncounterId::Cave(id))
        .expect("the cave encounter exists");
    assert!(!dead);
    hits.write(Hit {
        source: player,
        target: entity,
        damage: u16::MAX,
        contact: None,
    });
}

fn at(
    fixtures: &Fixtures,
    entity: Entity,
    controller: &mut Player,
    dodge: &mut Dodge,
    pose: &mut Transform,
    point: Vec2,
) {
    let probe = Vec3::new(point.x, 0.2, point.y);
    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    let y = walkable_height(&fixtures.spatial, &filter, probe, 4.0, PLAYER_GROUND_OFFSET)
        .expect("the smoke waypoint has foot support");
    controller.respawn(dodge, pose, probe.with_y(y));
}

fn check_villagers(fixtures: &Fixtures) {
    for name in ["isera", "tovan", "vey", "orla"] {
        assert_eq!(
            fixtures
                .speakers
                .iter()
                .filter(|(_, speaker, _)| speaker.profile == name)
                .count(),
            1
        );
    }
}

pub(super) fn check_rim(
    spatial: SpatialQuery,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    colliders: Query<Entity, With<Collider>>,
    player: Query<&Collider, With<Player>>,
) {
    // Match the edge of the authored shelf from east of the gate to west of it.
    const RIM: [(f32, f32); 15] = [
        (-25.0, 26.8),
        (-25.0, 45.0),
        (-37.5, 54.0),
        (-37.0, 73.0),
        (-43.0, 98.0),
        (-62.0, 112.0),
        (-111.0, 112.0),
        (-126.0, 97.0),
        (-128.0, 57.0),
        (-114.0, 38.0),
        (-77.0, 36.0),
        (-57.0, 45.0),
        (-43.0, 54.0),
        (-35.0, 48.0),
        (-35.0, 26.8),
    ];
    let capsule = player
        .single()
        .expect("the crossing smoke has a player capsule");
    let filter = SpatialQueryFilter::from_excluded_entities(colliders.iter().filter(|entity| {
        !std::iter::once(*entity)
            .chain(parents.iter_ancestors(*entity))
            .any(|ancestor| {
                names
                    .get(ancestor)
                    .is_ok_and(|name| name.as_str() == "Perimeter solid Listening Flats cliffs")
            })
    }));
    for segment in RIM.array_windows::<2>() {
        let [a, b] = segment.map(Vec2::from);
        let edge = b - a;
        let outward = Dir3::new(Vec3::new(edge.y, 0.0, -edge.x)).expect("distinct rim corners");
        let samples = edge.length().ceil() as u32;
        for sample in 0..=samples {
            let point = a.lerp(b, sample as f32 / samples as f32);
            for y in [-0.45, 2.1, 7.1] {
                super::perimeter::assert_blocked(
                    &spatial,
                    capsule,
                    &filter,
                    Vec3::new(point.x, y, point.y),
                    outward,
                );
            }
        }
    }
    info!("LISTENING FLATS RIM PASS: capsule stopped around the new valley");
}

pub(super) fn log_progress(
    run: Res<SmokeRun>,
    check: Res<Check>,
    player: Query<&Transform, With<Player>>,
    fixtures: Fixtures,
    mut last: Local<f64>,
) {
    let now = run.0.elapsed().as_secs_f64();
    if now - *last >= 2.0
        && let Ok(pose) = player.single()
    {
        info!("CROSSING SMOKE: {:?} at {:?}", *check, pose.translation);
        if matches!(*check, Check::Settling(_)) && fixtures.bench.single().is_ok() {
            let villagers = VILLAGE_HOMES.map(|(id, _)| (id, resident(&fixtures, id)));
            let guards: Vec<_> = fixtures
                .guards
                .iter()
                .map(|(post, at, gait)| (post, at.translation().xz(), gait))
                .collect();
            info!("Residents {villagers:?}, guards {guards:?}");
        }
        *last = now;
    }
}
