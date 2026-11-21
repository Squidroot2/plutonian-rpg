//! Opt-in desktop checks for live gameplay and shipped-map collisions.
mod dungeon;
mod equipment;
mod inventory;
mod nemi;
#[cfg(test)]
pub(crate) use inventory::pointer_click;
mod spear;
mod worm;

use self::{
    equipment::{EquipmentSmoke, EquipmentSmokePlugin},
    inventory::{InventorySmoke, InventorySmokePlugin, click, walk},
    spear::{SpearSmoke, SpearSmokePlugin},
};
use crate::{
    GameState,
    camera::Orbit,
    equipment::Equipment,
    interaction::{InteractionTarget, Nearby, Target},
    inventory::{CARRY_CAPACITY, DropRequest, IMMOBILE_WEIGHT, Inventory, ItemKind},
    inventory_ui::{InventoryAction, InventoryUiState},
    loading::LoadStatus,
    menu_ui::{MenuTab, PlayerMenu},
    player::{DODGE_KEY, Dodge, Player},
    quest::{Beacon, Dialogue, LAST_LIGHT, QuestAction, QuestJournal, QuestStatus},
    quest_ui::{QuestRow, QuestSelection},
    title::{TitleBackdrop, TitleInput},
    world::{SessionEntity, WorldLayout},
};
use avian3d::prelude::*;
use bevy::{
    app::AppExit,
    input::InputSystems,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use std::time::{Duration, Instant};

const BOULDER_SITES: [Vec3; 2] = [Vec3::new(2.5, 0.0, -2.0), Vec3::new(2.5, 0.0, 2.0)];

pub const ARENA_LOOT: [(ItemKind, f32, f32); 14] = [
    (ItemKind::MethanePouch, -2.0, 0.0),
    (ItemKind::Scrap, -3.0, 1.3),
    (ItemKind::Battery, -4.0, 0.0),
    (ItemKind::Battery, -5.0, 1.3),
    (ItemKind::FieldSpear, 4.0, 0.0),
    (ItemKind::Trident, 7.5, 1.0),
    (ItemKind::Shortsword, 4.0, 2.5),
    (ItemKind::Shortsword, 7.5, 2.7),
    (ItemKind::FieldArmor, -3.0, -4.0),
    (ItemKind::FieldHelmet, 0.0, -4.0),
    (ItemKind::ReinforcedArmor, 3.0, -4.0),
    (ItemKind::ReinforcedHelmet, 6.0, -4.0),
    (
        ItemKind::LargeBoulder,
        BOULDER_SITES[0].x,
        BOULDER_SITES[0].z,
    ),
    (
        ItemKind::LargeBoulder,
        BOULDER_SITES[1].x,
        BOULDER_SITES[1].z,
    ),
];

const WALL: Vec3 = Vec3::new(0.0, 1.0, 9.0);
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Component)]
pub struct ArenaFloor;

pub fn spawn_arena(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let floor = Vec3::new(28.0, 1.0, 28.0);
    commands.spawn((
        SessionEntity,
        ArenaFloor,
        Name::new("Smoke arena floor"),
        Mesh3d(meshes.add(Cuboid::from_size(floor))),
        MeshMaterial3d(materials.add(Color::srgb(0.16, 0.23, 0.32))),
        RigidBody::Static,
        Collider::cuboid(floor.x, floor.y, floor.z),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    let wall = Vec3::new(4.0, 2.0, 0.4);
    commands.spawn((
        SessionEntity,
        Name::new("Smoke arena wall"),
        Mesh3d(meshes.add(Cuboid::from_size(wall))),
        MeshMaterial3d(materials.add(Color::srgb(0.4, 0.24, 0.12))),
        RigidBody::Static,
        Collider::cuboid(wall.x, wall.y, wall.z),
        Transform::from_translation(WALL),
    ));
}

pub struct SmokePlugin {
    pub arena: bool,
    pub nemi: bool,
}

#[derive(Resource)]
pub(crate) struct SmokeRun(Instant);

#[derive(Resource, Debug, Default)]
enum ArenaStep {
    #[default]
    CaptureArena,
    Pause(Entity),
    Paused {
        position: Vec3,
        started: Duration,
    },
    Strafe {
        position: Vec3,
        started: Duration,
    },
    Backpedal {
        position: Vec3,
        started: Duration,
    },
    SwitchToCharacter,
    CloseCharacter(u32),
    ReopenCharacter,
    SelectQuestTab,
    OpenJournal(Vec3),
    SelectJournal(Vec3),
    CloseJournal,
    Quest,
    SelectCore,
    CheckCore,
    TryCoreDrop(Entity),
    CloseCore,
    Fall(Entity),
    Recover,
    CollectBoulder(u32),
    CheckBurden {
        count: u32,
        after_drop: bool,
        position: Vec3,
        started: Duration,
    },
    DropBoulder(u32),
    ResumeAfterDrop(u32),
}

impl Plugin for SmokePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SmokeRun(Instant::now()))
            .add_systems(Update, watchdog)
            .add_systems(
                PreUpdate,
                start_from_title
                    .after(InputSystems)
                    .before(TitleInput)
                    .run_if(in_state(GameState::Title)),
            );
        if self.arena {
            app.insert_resource(WorldLayout::SmokeArena)
                .init_resource::<ArenaStep>()
                .add_systems(
                    OnEnter(GameState::Playing),
                    check_arena_wall.run_if(run_once),
                )
                .add_systems(
                    PreUpdate,
                    drive
                        .after(InputSystems)
                        .after(equipment::drive)
                        .before(crate::player::input)
                        .before(crate::ui::pause),
                )
                .add_plugins((InventorySmokePlugin, EquipmentSmokePlugin, SpearSmokePlugin));
        } else if self.nemi {
            app.init_resource::<nemi::Check>().add_systems(
                PreUpdate,
                nemi::drive
                    .after(InputSystems)
                    .before(crate::ui::pause)
                    .run_if(in_state(GameState::Playing)),
            );
        } else {
            app.add_systems(
                OnEnter(GameState::Playing),
                (check_pillar_collision, worm::check_outpost_sites).run_if(run_once),
            )
            .init_resource::<dungeon::WorldCheck>()
            .add_systems(
                PreUpdate,
                dungeon::drive
                    .after(InputSystems)
                    .before(crate::player::input)
                    .before(crate::ui::pause)
                    .run_if(|state: Res<State<GameState>>| {
                        matches!(
                            state.get(),
                            GameState::Playing | GameState::Looting | GameState::Storage
                        )
                    }),
            );
        }
    }
}

fn start_from_title(
    backdrop: Query<&ImageNode, With<TitleBackdrop>>,
    captures: Query<(), With<Screenshot>>,
    assets: Res<Assets<Image>>,
    game_assets: Option<Res<crate::loading::GameAssets>>,
    time: Res<Time>,
    mut ready_at: Local<Option<Duration>>,
    mut screenshot: Local<Option<Entity>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    assert!(
        game_assets.is_none(),
        "the title screen must defer world loading"
    );
    if let Some(entity) = *screenshot {
        if captures.get(entity).is_err() {
            keys.press(KeyCode::Enter);
        }
    } else if backdrop
        .single()
        .is_ok_and(|node| assets.contains(&node.image))
    {
        // Give the renderer time to prepare the first UI pipelines after loading the texture.
        let ready_at = *ready_at.get_or_insert(time.elapsed());
        if time.elapsed() - ready_at >= Duration::from_millis(250) {
            *screenshot = Some(capture(&mut commands, "title"));
        }
    }
}

fn watchdog(
    run: Res<SmokeRun>,
    state: Res<State<GameState>>,
    status: Res<LoadStatus>,
    inventory: Option<Res<InventorySmoke>>,
    weapons: Option<Res<SpearSmoke>>,
    swords: Option<Res<equipment::ShortswordStep>>,
    armor: Option<Res<EquipmentSmoke>>,
    arena: Option<Res<ArenaStep>>,
    world: Option<Res<dungeon::WorldCheck>>,
    nemi: Option<Res<nemi::Check>>,
) {
    assert_ne!(
        *state.get(),
        GameState::Failed,
        "smoke test asset loading failed: {:?}",
        status.error
    );
    assert!(
        run.0.elapsed() < TIMEOUT,
        "smoke test timed out in {:?}; inventory {:?}, spears {:?}, shortswords {:?}, armor {:?}, arena {:?}, world {:?}, Nemi {:?}",
        state.get(),
        inventory.as_deref().map(InventorySmoke::phase),
        weapons.as_deref(),
        swords.as_deref(),
        armor.as_deref().map(EquipmentSmoke::stage),
        arena.as_deref(),
        world.as_deref(),
        nemi.as_deref(),
    );
}

pub(crate) fn capture(commands: &mut Commands, name: &str) -> Entity {
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("/tmp/plutonian-{name}.png")))
        .id()
}

fn drive(
    (inventory_smoke, armor, run): (Res<InventorySmoke>, Res<EquipmentSmoke>, Res<SmokeRun>),
    (inventory, ui, actions): (
        Res<Inventory>,
        Res<InventoryUiState>,
        Query<(Entity, &InventoryAction)>,
    ),
    (mut step, mut core_checked, fixed): (ResMut<ArenaStep>, Local<bool>, Res<Time<Fixed>>),
    (state, quest, dialogue, nearby): (
        Res<State<GameState>>,
        Res<QuestJournal>,
        Res<Dialogue>,
        Res<Nearby>,
    ),
    equipment: Res<Equipment>,
    (menu, selection, rows, tabs): (
        Res<PlayerMenu>,
        Res<QuestSelection>,
        Query<(Entity, &QuestRow)>,
        Query<(Entity, &MenuTab)>,
    ),
    mut player: Query<(&mut Transform, &Player, &Dodge)>,
    targets: Query<(&Target, &Transform), Without<Player>>,
    beacon: Query<&PointLight, With<Beacon>>,
    layout: Res<WorldLayout>,
    captures: Query<(), With<Screenshot>>,
    mut orbit: ResMut<Orbit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if !armor.done() {
        return;
    }
    let (mut transform, controller, dodge) =
        player.single_mut().expect("the arena must have one player");
    keys.reset_all();
    let click_action = |commands: &mut Commands, action| {
        let (window, camera) = inventory_smoke
            .context
            .expect("the inventory smoke phase must supply the window and camera for the quest UI");
        click(commands, action, &actions, window, camera);
    };
    match *step {
        ArenaStep::CaptureArena if *state.get() == GameState::Playing && controller.grounded => {
            *step = ArenaStep::Pause(capture(&mut commands, "smoke-arena"));
        }
        ArenaStep::Pause(capture) if captures.get(capture).is_err() => {
            keys.press(KeyCode::Escape);
            *step = ArenaStep::Paused {
                position: transform.translation,
                started: fixed.elapsed(),
            };
        }
        ArenaStep::Paused { position, started } if *state.get() == GameState::Paused => {
            keys.press(KeyCode::KeyW);
            assert!(
                position.distance(transform.translation) < 0.01,
                "player moved while paused"
            );
            if fixed.elapsed() - started >= fixed.timestep() * 3 {
                keys.press(KeyCode::Escape);
                *step = ArenaStep::Strafe {
                    position: transform.translation,
                    started: fixed.elapsed(),
                };
            }
        }
        ArenaStep::Strafe { position, started } if *state.get() == GameState::Playing => {
            orbit.yaw = 0.0;
            keys.press(KeyCode::KeyD);
            keys.press(KeyCode::ShiftLeft);
            if fixed.elapsed() - started >= fixed.timestep() * 12 {
                let travel = transform.translation - position;
                assert!(
                    travel.x > 0.3 && travel.z.abs() < 0.03,
                    "D must strafe right: {travel:?}"
                );
                assert!(
                    !controller.running,
                    "shift must keep pure strafing at walking speed"
                );
                assert!((transform.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 0.001));
                capture(&mut commands, "strafe");
                // Switch inputs before the next fixed step.
                *step = ArenaStep::Backpedal {
                    position: transform.translation,
                    started: fixed.elapsed(),
                };
                keys.release(KeyCode::KeyD);
                keys.press(KeyCode::KeyS);
            }
        }
        ArenaStep::Backpedal { position, started } if *state.get() == GameState::Playing => {
            keys.press(KeyCode::KeyS);
            keys.press(KeyCode::ShiftLeft);
            if fixed.elapsed() - started >= fixed.timestep() * 12 {
                let travel = transform.translation - position;
                assert!(
                    travel.z > 0.3 && travel.x.abs() < 0.03,
                    "S must backpedal: {travel:?}"
                );
                assert!(
                    !controller.running,
                    "shift must keep backpedaling at walking speed"
                );
                assert!((transform.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 0.001));
                capture(&mut commands, "backpedal");
                info!(
                    "MOVEMENT SMOKE PASS: strafed and backpedaled at walking speed while facing the camera aim"
                );
                keys.press(KeyCode::Tab);
                keys.press(KeyCode::KeyE);
                *step = ArenaStep::SwitchToCharacter;
            }
        }
        ArenaStep::SwitchToCharacter => {
            assert!(
                menu.is_on(MenuTab::Equipment),
                "Tab must reopen Equipment after the gear checks"
            );
            let (window, camera) = inventory_smoke.context.expect("smoke UI context");
            click(&mut commands, MenuTab::Character, &tabs, window, camera);
            let batteries = inventory.count(ItemKind::Battery);
            assert!(batteries > 0);
            commands.trigger(DropRequest {
                item: ItemKind::Battery,
                quantity: 1,
            });
            *step = ArenaStep::CloseCharacter(batteries);
        }
        ArenaStep::CloseCharacter(batteries) => {
            assert!(menu.is_on(MenuTab::Character));
            assert_eq!(
                inventory.count(ItemKind::Battery),
                batteries,
                "a hidden Inventory tab must reject drop requests"
            );
            capture(&mut commands, "character-menu");
            keys.press(KeyCode::Tab);
            *step = ArenaStep::ReopenCharacter;
        }
        ArenaStep::ReopenCharacter => {
            assert!(!menu.is_open());
            keys.press(KeyCode::Tab);
            *step = ArenaStep::SelectQuestTab;
        }
        ArenaStep::SelectQuestTab => {
            assert!(menu.is_on(MenuTab::Character));
            let (window, camera) = inventory_smoke.context.expect("smoke UI context");
            click(&mut commands, MenuTab::Quest, &tabs, window, camera);
            *step = ArenaStep::OpenJournal(transform.translation);
        }
        ArenaStep::OpenJournal(position) => {
            assert!(menu.is_on(MenuTab::Quest));
            assert!(dialogue.text.is_none());
            assert_eq!(*state.get(), GameState::Playing);
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::KeyE);
            let (row, _) = rows
                .iter()
                .find(|(_, row)| row.id == LAST_LIGHT && row.status == QuestStatus::Active)
                .expect("The Last Light must have an active journal row");
            let (window, camera) = inventory_smoke.context.expect("smoke UI context");
            commands.trigger(inventory::pointer_click(row, window, camera));
            *step = ArenaStep::SelectJournal(position);
        }
        ArenaStep::SelectJournal(position) => {
            assert_eq!(selection.selected(), Some(LAST_LIGHT));
            assert!(transform.translation.xz().distance(position.xz()) < 0.025);
            assert!(dialogue.text.is_none());
            capture(&mut commands, "quest-log");
            keys.press(KeyCode::Escape);
            keys.press(KeyCode::KeyE);
            *step = ArenaStep::CloseJournal;
        }
        ArenaStep::CloseJournal => {
            assert!(!menu.is_open());
            assert_eq!(*state.get(), GameState::Playing);
            assert!(dialogue.text.is_none());
            *step = ArenaStep::Quest;
        }
        ArenaStep::Quest if *state.get() == GameState::Playing => {
            if dialogue.text.is_some() {
                keys.press(KeyCode::KeyE);
                return;
            }
            if quest.is_complete(LAST_LIGHT) {
                assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
                assert!(
                    beacon.single().is_ok_and(|light| light.intensity > 0.0),
                    "turning in the core must light the beacon"
                );
                *step = ArenaStep::Fall(capture(&mut commands, "beacon"));
                return;
            }
            if quest.expects(QuestAction::ReturnedCore) && !*core_checked {
                assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
                keys.press(KeyCode::KeyI);
                *step = ArenaStep::SelectCore;
                return;
            }
            let target = if quest.expects(QuestAction::RecoveredCore) {
                Target::Core
            } else {
                Target::Keeper
            };
            if matches!(nearby.0, Some((_, InteractionTarget::Quest(current))) if current == target)
            {
                keys.press(KeyCode::KeyE);
            } else {
                let (_, destination) = targets
                    .iter()
                    .find(|(current, _)| **current == target)
                    .expect("the active quest target must have a world entity");
                walk(
                    &mut keys,
                    &mut orbit,
                    transform.translation,
                    destination.translation,
                );
            }
        }
        ArenaStep::SelectCore if menu.is_on(MenuTab::Inventory) => {
            click_action(&mut commands, InventoryAction::Select(ItemKind::BeaconCore));
            *step = ArenaStep::CheckCore;
        }
        ArenaStep::CheckCore => {
            assert_eq!(ui.selected, Some(ItemKind::BeaconCore));
            assert_eq!(
                ui.amount(&inventory),
                None,
                "the core must be protected in inventory"
            );
            *step = ArenaStep::TryCoreDrop(capture(&mut commands, "quest-inventory"));
        }
        ArenaStep::TryCoreDrop(capture) if captures.get(capture).is_err() => {
            click_action(&mut commands, InventoryAction::Drop);
            *step = ArenaStep::CloseCore;
        }
        ArenaStep::CloseCore => {
            assert!(
                !ui.drop_is_open(),
                "a protected item cannot open the drop prompt"
            );
            assert_eq!(inventory.count(ItemKind::BeaconCore), 1);
            *core_checked = true;
            keys.press(KeyCode::KeyI);
            *step = ArenaStep::Quest;
        }
        ArenaStep::Fall(capture) if captures.get(capture).is_err() => {
            transform.translation.y = -20.0;
            *step = ArenaStep::Recover;
        }
        ArenaStep::Recover
            if controller.grounded && transform.translation.distance(layout.start()) < 0.1 =>
        {
            assert!(quest.is_complete(LAST_LIGHT));
            assert_eq!(inventory.count(ItemKind::BeaconCore), 0);
            *step = ArenaStep::CollectBoulder(1);
        }
        ArenaStep::CollectBoulder(count) => {
            if inventory.count(ItemKind::LargeBoulder) == count {
                *step = ArenaStep::CheckBurden {
                    count,
                    after_drop: false,
                    position: transform.translation,
                    started: fixed.elapsed(),
                };
            } else if matches!(nearby.0, Some((_, InteractionTarget::Pickup(pickup))) if pickup.item == ItemKind::LargeBoulder)
            {
                keys.press(KeyCode::KeyE);
            } else {
                walk(
                    &mut keys,
                    &mut orbit,
                    transform.translation,
                    BOULDER_SITES[count as usize - 1],
                );
            }
        }
        ArenaStep::CheckBurden {
            count,
            after_drop,
            position,
            started,
        } if *state.get() == GameState::Playing => {
            orbit.yaw = 0.0;
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
            if count > 0 {
                keys.press(KeyCode::Space);
                keys.press(DODGE_KEY);
            }
            if fixed.elapsed() - started >= fixed.timestep() * 10 {
                let weight = inventory.carried_weight(&equipment);
                let travel = transform.translation - position;
                match count {
                    2 => {
                        assert!(
                            weight > IMMOBILE_WEIGHT && travel.length() < 0.02,
                            "the severe load must prevent movement, load {weight}, travel {travel:?}"
                        );
                        keys.press(KeyCode::KeyI);
                        *step = ArenaStep::DropBoulder(1);
                    }
                    1 => {
                        assert!(
                            (CARRY_CAPACITY + 1..=IMMOBILE_WEIGHT).contains(&weight)
                                && travel.z < -0.2,
                            "the moderate load must allow walking, load {weight}, travel {travel:?}"
                        );
                        assert!(
                            !controller.running && controller.grounded && !dodge.active(),
                            "a load above capacity must prevent sprinting, jumping and dodging"
                        );
                        if !after_drop {
                            *step = ArenaStep::CollectBoulder(2);
                        } else {
                            keys.press(KeyCode::KeyI);
                            *step = ArenaStep::DropBoulder(0);
                        }
                    }
                    _ => {
                        assert!(
                            controller.running && travel.z < -0.4,
                            "dropping the boulders must restore sprinting"
                        );
                        info!(
                            "SMOKE PASS: arena gameplay and carrying limits verified in {:.2}s",
                            run.0.elapsed().as_secs_f64()
                        );
                        exit.write(AppExit::Success);
                    }
                }
            }
        }
        ArenaStep::DropBoulder(remaining) if menu.is_on(MenuTab::Inventory) => {
            commands.trigger(DropRequest {
                item: ItemKind::LargeBoulder,
                quantity: 1,
            });
            *step = ArenaStep::ResumeAfterDrop(remaining);
        }
        ArenaStep::ResumeAfterDrop(remaining) => {
            assert_eq!(
                inventory.count(ItemKind::LargeBoulder),
                remaining,
                "the overloaded player must still be able to drop a boulder"
            );
            keys.press(KeyCode::KeyI);
            *step = ArenaStep::CheckBurden {
                count: remaining,
                after_drop: true,
                position: transform.translation,
                started: fixed.elapsed(),
            };
        }
        _ => {}
    }
}

fn check_arena_wall(players: Query<&Collider, With<Player>>, mover: MoveAndSlide) {
    let capsule = players
        .single()
        .expect("arena collision check requires one player capsule");
    let position = mover
        .move_and_slide(
            capsule,
            WALL + Vec3::new(0.0, -0.085, -2.0),
            Quat::IDENTITY,
            Vec3::Z * 5.0,
            Duration::from_secs(1),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::default(),
            |_| MoveAndSlideHitResponse::Accept,
        )
        .position;
    assert!(
        (0.45..0.65).contains(&(WALL.z - position.z)),
        "the movement capsule must approach and stop at the arena wall, position {position:?}"
    );
    info!("ARENA COLLISION PASS: wall stops the character capsule");
}

// Exercise the same capsule and move-and-slide path used by character movement
// against every authored pillar, without changing the player's live position.
fn check_pillar_collision(
    nodes: Query<(&Name, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    colliders: Query<Entity, With<avian3d::prelude::Collider>>,
    players: Query<&avian3d::prelude::Collider, With<Player>>,
    mover: avian3d::prelude::MoveAndSlide,
) {
    use avian3d::prelude::*;
    let capsule = players
        .single()
        .expect("pillar collision checks require exactly one player capsule");
    let mut pillars = std::collections::BTreeMap::new();
    for (name, transform) in &nodes {
        if name.as_str().starts_with("Waystone") && !name.as_str().starts_with("Waystone lamp") {
            pillars.insert(name.as_str().to_owned(), transform.translation());
        }
    }
    assert!(
        !pillars.is_empty(),
        "pillar collision checks require trail pillars in the loaded scene"
    );
    for (name, center) in &pillars {
        // A few cave pillars sit against walls. Isolate each pillar's actual
        // collider descendants so a wall cannot mask a missing pillar collider.
        let excluded = colliders.iter().filter(|entity| {
            !std::iter::once(*entity)
                .chain(parents.iter_ancestors(*entity))
                .any(|ancestor| {
                    nodes
                        .get(ancestor)
                        .is_ok_and(|(ancestor_name, _)| ancestor_name.as_str() == name)
                })
        });
        let filter = SpatialQueryFilter::from_excluded_entities(excluded);
        for side in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            // Pillar meshes are 2.8 m tall with their origin at the center;
            // put the player's 0.895 m half-height capsule just above the base.
            let start = *center + side * 2.0 - Vec3::Y * 0.455;
            let output = mover.move_and_slide(
                capsule,
                start,
                Quat::IDENTITY,
                -side * 5.0,
                Duration::from_secs(1),
                &MoveAndSlideConfig::default(),
                &filter,
                |_| MoveAndSlideHitResponse::Accept,
            );
            let gap = (output.position - *center).dot(side);
            assert!(
                (0.5..1.0).contains(&gap),
                "{name}: character capsule passed through or failed to approach the pillar from {side:?}; final gap {gap}"
            );
        }
    }
    info!(
        "PILLAR COLLISION PASS: {} pillars block the character capsule from four directions",
        pillars.len()
    );
}
