//! Opt-in equipment playthrough using the live inventory UI and animated player rig.
use super::inventory::{InventorySmoke, click, pointer_click, walk};
use crate::{
    GameState,
    camera::Orbit,
    combat::HitReaction,
    combat_feedback::AttackImpact,
    equipment::{Equipment, EquipmentSlot, Handedness, WeaponKind, WornEquipment},
    interaction::{InteractionTarget, Nearby},
    inventory::{Inventory, ItemKind, Pickup},
    loading::PendingScene,
    player::{Pace, Player},
    practice_dummy::PracticeDummy,
    ui::inventory_ui::{InventoryAction, InventoryRow},
    ui::menu_ui::PlayerMenuScreen,
    ui::menu_ui::{MenuTab, PlayerMenu},
    weapon::{Hand, WeaponAttack, WeaponStance},
};
use bevy::{
    ecs::system::SystemParam, input::InputSystems, prelude::*, render::view::screenshot::Screenshot,
};

pub struct EquipmentSmokePlugin;

#[derive(Resource, Default, Debug, PartialEq)]
pub(super) enum ShortswordStep {
    #[default]
    Collect,
    EquipMain,
    EquipOffHand(Entity),
    CheckWorn,
    Holster,
    HolsteredIdle,
    HolsteredWalk(f32),
    HolsteredRun(f32),
    HolsteredJump(f32),
    Redraw,
    Approach,
    Swing {
        hand: Hand,
        landed: bool,
    },
    DualSwing,
    DualRecovery,
    Unequip,
    SelectDrop,
    OpenDrop,
    ConfirmDrop,
    CheckDrop(u32),
    Done,
}

#[derive(Resource)]
pub(crate) struct EquipmentSmoke {
    stage: u16,
    heavy: bool,
    timer: f32,
    screenshot: Option<Entity>,
    screen_size: Option<(Val, Val)>,
}
#[derive(SystemParam)]
pub(crate) struct View<'w, 's> {
    screen: Query<'w, 's, (&'static mut Node, &'static ComputedNode), With<PlayerMenuScreen>>,
    captures: Query<'w, 's, (), With<Screenshot>>,
    player: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            &'static Player,
            &'static WeaponStance,
        ),
        With<Player>,
    >,
    pickups: Query<'w, 's, (&'static Pickup, &'static Transform)>,
    actions: Query<'w, 's, (Entity, &'static InventoryAction)>,
    tabs: Query<'w, 's, (Entity, &'static MenuTab)>,
    inventory_rows:
        Query<'w, 's, (&'static InventoryAction, &'static Children), Without<PlayerMenuScreen>>,
    text: Query<'w, 's, &'static Text>,
    worn: Query<
        'w,
        's,
        (
            Entity,
            &'static WornEquipment,
            &'static ChildOf,
            &'static GlobalTransform,
            Option<&'static PendingScene>,
        ),
    >,
    nodes: Query<'w, 's, (&'static Name, &'static GlobalTransform)>,
    parents: Query<'w, 's, &'static ChildOf>,
}
impl View<'_, '_> {
    fn bag_rows(&self, item: ItemKind) -> impl Iterator<Item = (Entity, InventoryAction)> {
        self.actions.iter().filter_map(move |(entity, &action)| {
            matches!(action, InventoryAction::Select(InventoryRow::BagGear(kind, _)) if kind == item)
                .then_some((entity, action))
        })
    }
}

impl EquipmentSmoke {
    pub fn stage(&self) -> u16 {
        self.stage
    }

    pub fn done(&self) -> bool {
        self.stage == 60
    }

    fn captured(&self, view: &View) -> bool {
        self.screenshot
            .is_some_and(|entity| view.captures.get(entity).is_err())
    }
}

impl Plugin for EquipmentSmokePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EquipmentSmoke {
            stage: 0,
            heavy: false,
            timer: 0.0,
            screenshot: None,
            screen_size: None,
        })
        .init_resource::<ShortswordStep>()
        .add_systems(
            PreUpdate,
            (drive_shortswords, drive)
                .chain()
                .after(InputSystems)
                .after(super::inventory::drive)
                .after(super::spear::drive)
                .before(crate::player::input)
                .before(crate::ui::pause),
        );
    }
}
fn collect(
    item: ItemKind,
    bag: &Inventory,
    nearby: &Nearby,
    view: &View,
    keys: &mut ButtonInput<KeyCode>,
    orbit: &mut Orbit,
    position: Vec3,
) -> bool {
    if bag.count(item) > 0 {
        return true;
    }
    approach_pickup(item, nearby, view, keys, orbit, position);
    false
}

fn approach_pickup(
    item: ItemKind,
    nearby: &Nearby,
    view: &View,
    keys: &mut ButtonInput<KeyCode>,
    orbit: &mut Orbit,
    position: Vec3,
) {
    if matches!(nearby.0, Some((_,InteractionTarget::Pickup(pickup))) if pickup.item == item) {
        keys.press(KeyCode::KeyE);
        return;
    }
    let target = view
        .pickups
        .iter()
        .filter(|(p, _)| p.item == item)
        .min_by(|(_, a), (_, b)| {
            a.translation
                .distance_squared(position)
                .total_cmp(&b.translation.distance_squared(position))
        })
        .expect("each equipment smoke pickup must remain in the world until collected")
        .1
        .translation;
    walk(keys, orbit, position, target);
}
fn check_worn(view: &View, equipment: &Equipment, player: Entity) -> bool {
    let (_, _, _, stance) = view
        .player
        .get(player)
        .expect("equipment smoke requires the player");
    for slot in EquipmentSlot::ALL {
        let roots = view
            .worn
            .iter()
            .filter(|(_, piece, _, _, _)| piece.slot == slot)
            .collect::<Vec<_>>();
        if let Some(item) = equipment.get(slot) {
            if roots.len() != 1 || roots[0].4.is_some() {
                return false;
            }
            let (_, piece, parent, transform, _) = roots[0];
            assert_eq!(piece.item, item, "worn model does not match its slot");
            let (name, bone_transform) = view
                .nodes
                .get(parent.parent())
                .expect("worn equipment must be parented to a named rig bone");
            let handedness = item.weapon().map(WeaponKind::handedness);
            if *stance == WeaponStance::Holstered
                && let Some(handedness) = handedness
            {
                let mount = match (slot, handedness) {
                    (_, Handedness::TwoHanded) => "polearm_holster",
                    (EquipmentSlot::OffHand, _) => "left_hip_holster",
                    _ => "right_hip_holster",
                };
                assert_eq!(name.as_str(), mount);
            } else {
                assert!(crate::equipment::EQUIPMENT_SOCKETS.contains(&(
                    name.as_str(),
                    slot,
                    handedness
                )));
            }
            assert!(
                view.parents
                    .iter_ancestors(parent.parent())
                    .any(|entity| entity == player),
                "equipment was attached to an NPC instead of the player"
            );
            assert!(
                transform
                    .translation()
                    .distance(bone_transform.translation())
                    < 0.001,
                "worn model origin must follow its animated bone"
            );
        } else if !roots.is_empty() {
            return false;
        }
    }
    true
}

fn drive_shortswords(
    spear_smoke: Res<super::spear::SpearSmoke>,
    step: ResMut<ShortswordStep>,
    inventory_smoke: Res<InventorySmoke>,
    (menu, bag, equipment, nearby): (Res<PlayerMenu>, Res<Inventory>, Res<Equipment>, Res<Nearby>),
    attacks: Query<&WeaponAttack, With<Player>>,
    dummy: Query<(&HitReaction, &Transform), With<PracticeDummy>>,
    impacts: Query<(), With<AttackImpact>>,
    view: View,
    (time, mut sword_entities): (Res<Time>, Local<[Option<Entity>; 2]>),
    (mut keys, mut buttons, orbit): (
        ResMut<ButtonInput<KeyCode>>,
        ResMut<ButtonInput<MouseButton>>,
        ResMut<Orbit>,
    ),
    commands: Commands,
) {
    use ItemKind::Shortsword;

    if !spear_smoke.done() || *step == ShortswordStep::Done {
        return;
    }
    let (player, transform, controller, stance) = view.player.single().expect("arena player");
    let attack = attacks.get(player).expect("Player requires WeaponAttack");
    let (dummy, dummy_transform) = dummy.single().expect("arena practice dummy");
    let (window, camera) = inventory_smoke.context.expect("arena UI context");
    keys.reset_all();
    buttons.reset_all();
    let now = time.elapsed_secs();
    let worn_swords = || {
        EquipmentSlot::HANDS.map(|slot| {
            view.worn.iter().find_map(|(entity, piece, _, _, pending)| {
                (piece.slot == slot && piece.item == Shortsword && pending.is_none())
                    .then_some(entity)
            })
        })
    };
    let check_holstered = |expected| {
        assert_eq!(*stance, WeaponStance::Holstered);
        assert!(check_worn(&view, &equipment, player));
        assert_eq!(
            worn_swords(),
            expected,
            "holstering keeps both sword scenes"
        );
        assert!(!attack.active(), "holstered weapons cannot attack");
    };

    match *step {
        ShortswordStep::Collect
        | ShortswordStep::EquipMain
        | ShortswordStep::EquipOffHand(_)
        | ShortswordStep::CheckWorn
        | ShortswordStep::Holster
        | ShortswordStep::HolsteredIdle
        | ShortswordStep::HolsteredWalk(..)
        | ShortswordStep::HolsteredRun(..)
        | ShortswordStep::HolsteredJump(..)
        | ShortswordStep::Redraw => equip_and_holster_swords(
            step,
            menu,
            bag,
            &equipment,
            nearby,
            &view,
            &mut sword_entities,
            keys,
            buttons,
            orbit,
            commands,
            player,
            transform,
            controller,
            window,
            camera,
            now,
            worn_swords,
            check_holstered,
        ),
        ShortswordStep::Approach
        | ShortswordStep::Swing { .. }
        | ShortswordStep::DualSwing
        | ShortswordStep::DualRecovery => check_sword_combat(
            step,
            menu,
            equipment,
            dummy,
            impacts,
            &view,
            sword_entities,
            keys,
            buttons,
            orbit,
            commands,
            player,
            transform,
            controller,
            stance,
            attack,
            dummy_transform,
            worn_swords,
        ),
        ShortswordStep::Unequip
        | ShortswordStep::SelectDrop
        | ShortswordStep::OpenDrop
        | ShortswordStep::ConfirmDrop
        | ShortswordStep::CheckDrop(_) => drop_swords(
            step, menu, bag, equipment, &view, keys, commands, player, window, camera,
        ),
        ShortswordStep::Done => {}
    }
}

fn equip_and_holster_swords(
    mut step: ResMut<ShortswordStep>,
    menu: Res<PlayerMenu>,
    bag: Res<Inventory>,
    equipment: &Equipment,
    nearby: Res<Nearby>,
    view: &View,
    sword_entities: &mut [Option<Entity>; 2],
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    player: Entity,
    transform: &Transform,
    controller: &Player,
    window: Entity,
    camera: Entity,
    now: f32,
    worn_swords: impl Fn() -> [Option<Entity>; 2],
    check_holstered: impl Fn([Option<Entity>; 2]),
) {
    use EquipmentSlot::{MainHand, OffHand};
    use ItemKind::Shortsword;

    match *step {
        ShortswordStep::Collect if bag.count(Shortsword) < 2 => {
            approach_pickup(
                Shortsword,
                &nearby,
                view,
                &mut keys,
                &mut orbit,
                transform.translation,
            );
        }
        ShortswordStep::Collect => {
            keys.press(KeyCode::KeyI);
            *step = ShortswordStep::EquipMain;
        }
        ShortswordStep::EquipMain if menu.accepts(MenuTab::Inventory) => {
            let mut rows = view.bag_rows(Shortsword);
            let (row, _) = rows.next().expect("first Shortsword has a bag row");
            let (other, _) = rows.next().expect("second Shortsword has its own bag row");
            assert!(rows.next().is_none());
            commands.trigger(PointerClick {
                count: 2,
                ..pointer_click(row, window, camera)
            });
            *step = ShortswordStep::EquipOffHand(other);
        }
        ShortswordStep::EquipOffHand(other) => {
            assert_eq!(equipment.get(MainHand), Some(Shortsword));
            assert_eq!(equipment.get(OffHand), None);
            assert_eq!(bag.count(Shortsword), 1);
            assert!(view.bag_rows(Shortsword).map(|(row, _)| row).eq([other]));
            commands.trigger(PointerClick {
                count: 2,
                ..pointer_click(other, window, camera)
            });
            *step = ShortswordStep::CheckWorn;
        }
        ShortswordStep::CheckWorn => {
            assert_eq!(equipment.get(MainHand), Some(Shortsword));
            assert_eq!(equipment.get(OffHand), Some(Shortsword));
            assert_eq!(bag.count(Shortsword), 0);
            if check_worn(view, equipment, player) {
                keys.press(KeyCode::Tab);
                *step = ShortswordStep::Holster;
            }
        }
        ShortswordStep::Holster if !menu.is_open() => {
            *sword_entities = worn_swords();
            assert!(sword_entities.iter().all(Option::is_some));
            orbit.yaw = 0.0;
            orbit.pitch = 0.18;
            orbit.distance = 3.7;
            keys.press(KeyCode::KeyX);
            *step = ShortswordStep::HolsteredIdle;
        }
        ShortswordStep::HolsteredIdle => {
            check_holstered(*sword_entities);
            buttons.press(MouseButton::Left);
            buttons.press(MouseButton::Right);
            super::capture(&mut commands, "shortswords-holstered-idle");
            *step = ShortswordStep::HolsteredWalk(now);
        }
        ShortswordStep::HolsteredWalk(started) => {
            check_holstered(*sword_entities);
            keys.press(KeyCode::KeyW);
            if now - started > 0.3 {
                assert!(
                    controller.grounded
                        && controller.pace == Pace::Walk
                        && controller.velocity.xz().length() > 0.1
                );
                super::capture(&mut commands, "shortswords-holstered-walk");
                keys.press(KeyCode::CapsLock);
                *step = ShortswordStep::HolsteredRun(now);
            }
        }
        ShortswordStep::HolsteredRun(started) => {
            check_holstered(*sword_entities);
            keys.press(KeyCode::KeyW);
            if now - started > 0.3 {
                assert!(controller.grounded && controller.pace == Pace::Run);
                super::capture(&mut commands, "shortswords-holstered-run");
                keys.press(KeyCode::CapsLock);
                keys.press(KeyCode::Space);
                *step = ShortswordStep::HolsteredJump(now);
            }
        }
        ShortswordStep::HolsteredJump(started) if now - started > 0.22 => {
            check_holstered(*sword_entities);
            assert!(!controller.grounded);
            super::capture(&mut commands, "shortswords-holstered-jump");
            *step = ShortswordStep::Redraw;
        }
        ShortswordStep::Redraw if controller.grounded => {
            check_holstered(*sword_entities);
            keys.press(KeyCode::KeyX);
            *step = ShortswordStep::Approach;
        }
        _ => {}
    }
}

fn check_sword_combat(
    mut step: ResMut<ShortswordStep>,
    menu: Res<PlayerMenu>,
    equipment: Res<Equipment>,
    dummy: &HitReaction,
    impacts: Query<(), With<AttackImpact>>,
    view: &View,
    sword_entities: Local<[Option<Entity>; 2]>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    player: Entity,
    transform: &Transform,
    controller: &Player,
    stance: &WeaponStance,
    attack: &WeaponAttack,
    dummy_transform: &Transform,
    worn_swords: impl Fn() -> [Option<Entity>; 2],
) {
    use ItemKind::Shortsword;
    match *step {
        ShortswordStep::Approach if !menu.is_open() => {
            assert_eq!(*stance, WeaponStance::Drawn);
            assert!(check_worn(view, &equipment, player));
            assert_eq!(
                worn_swords(),
                *sword_entities,
                "drawing keeps both sword scenes"
            );
            let position = dummy_transform.translation - Vec3::Z * 1.2;
            if transform.translation.xz().distance(position.xz()) > 0.17 {
                walk(&mut keys, &mut orbit, transform.translation, position);
            } else if controller.grounded {
                orbit.yaw = std::f32::consts::PI;
                orbit.distance = 3.5;
                buttons.press(MouseButton::Left);
                *step = ShortswordStep::Swing {
                    hand: Hand::Main,
                    landed: false,
                };
            }
        }
        ShortswordStep::Swing {
            hand,
            landed: false,
        } => {
            assert!(
                attack.current(hand).is_some(),
                "mouse click must start a swing"
            );
            if dummy.remaining > 0.0 {
                let side = if hand == Hand::Main { "main" } else { "off" };
                super::capture(&mut commands, &format!("shortsword-{side}-hit"));
                *step = ShortswordStep::Swing { hand, landed: true };
            }
        }
        ShortswordStep::Swing { hand, landed: true }
            if !attack.active() && dummy.remaining == 0.0 =>
        {
            if hand == Hand::Main {
                buttons.press(MouseButton::Right);
                *step = ShortswordStep::Swing {
                    hand: Hand::Off,
                    landed: false,
                };
            } else {
                buttons.press(MouseButton::Left);
                buttons.press(MouseButton::Right);
                *step = ShortswordStep::DualSwing;
            }
        }
        ShortswordStep::DualSwing => {
            assert!(
                Hand::ALL
                    .into_iter()
                    .all(|hand| attack.current(hand).is_some())
            );
            if dummy.remaining > 0.0 {
                assert_eq!(impacts.iter().count(), 2, "both swords must hit the dummy");
                for (_, piece, _, worn, _) in &view.worn {
                    assert_eq!(piece.item, Shortsword);
                    assert!(
                        worn.translation().z - transform.translation.z > 0.4,
                        "{:?} sword must extend forward during the dual strike",
                        piece.slot,
                    );
                }
                super::capture(&mut commands, "shortsword-dual-hit");
                *step = ShortswordStep::DualRecovery;
            }
        }
        ShortswordStep::DualRecovery if !attack.active() && dummy.remaining == 0.0 => {
            keys.press(KeyCode::Tab);
            *step = ShortswordStep::Unequip;
        }
        _ => {}
    }
}

fn drop_swords(
    mut step: ResMut<ShortswordStep>,
    menu: Res<PlayerMenu>,
    bag: Res<Inventory>,
    equipment: Res<Equipment>,
    view: &View,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut commands: Commands,
    player: Entity,
    window: Entity,
    camera: Entity,
) {
    use ItemKind::Shortsword;

    macro_rules! click {
        ($action:expr) => {
            click(&mut commands, $action, &view.actions, window, camera)
        };
    }
    match *step {
        ShortswordStep::Unequip if menu.accepts(MenuTab::Inventory) => {
            click(
                &mut commands,
                MenuTab::Equipment,
                &view.tabs,
                window,
                camera,
            );
        }
        ShortswordStep::Unequip if menu.accepts(MenuTab::Equipment) => {
            if let Some(slot) = EquipmentSlot::HANDS
                .into_iter()
                .find(|&slot| equipment.get(slot).is_some())
            {
                click!(InventoryAction::ToggleSlot(slot));
            } else if check_worn(view, &equipment, player) {
                assert_eq!(bag.count(Shortsword), 2);
                click(
                    &mut commands,
                    MenuTab::Inventory,
                    &view.tabs,
                    window,
                    camera,
                );
                *step = ShortswordStep::SelectDrop;
            }
        }
        ShortswordStep::SelectDrop if menu.accepts(MenuTab::Inventory) => {
            let (_, action) = view
                .bag_rows(Shortsword)
                .next()
                .expect("a Shortsword remains in the bag");
            click!(action);
            *step = ShortswordStep::OpenDrop;
        }
        ShortswordStep::OpenDrop => {
            click!(InventoryAction::Drop);
            *step = ShortswordStep::ConfirmDrop;
        }
        ShortswordStep::ConfirmDrop => {
            click!(InventoryAction::ConfirmDrop);
            *step = ShortswordStep::CheckDrop(bag.count(Shortsword) - 1);
        }
        ShortswordStep::CheckDrop(remaining) => {
            assert_eq!(
                bag.count(Shortsword),
                remaining,
                "drop one sword per confirmation"
            );
            if remaining > 0 {
                *step = ShortswordStep::SelectDrop;
                return;
            }
            keys.press(KeyCode::Tab);
            info!(
                "SHORTSWORD SMOKE PASS: collected, equipped and dropped both swords, and landed solo and simultaneous strikes"
            );
            *step = ShortswordStep::Done;
        }
        _ => {}
    }
}

pub(super) fn drive(
    swords: Res<ShortswordStep>,
    smoke: ResMut<EquipmentSmoke>,
    inventory_smoke: Res<InventorySmoke>,
    (state, menu): (Res<State<GameState>>, Res<PlayerMenu>),
    time: Res<Time>,
    bag: Res<Inventory>,
    equipment: Res<Equipment>,
    nearby: Res<Nearby>,
    view: View,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    orbit: ResMut<Orbit>,
    commands: Commands,
    mut last_log: Local<f32>,
) {
    if *swords != ShortswordStep::Done
        || !inventory_smoke.done
        || smoke.done()
        || keys.just_pressed(KeyCode::Tab)
    {
        return;
    }
    let Ok((player, transform, _, _)) = view.player.single() else {
        return;
    };
    keys.reset_all();
    let now = time.elapsed_secs();
    if now - *last_log > 3.0 {
        info!(
            "EQUIPMENT SMOKE: stage {}, position {:?}, defense {}",
            smoke.stage,
            transform.translation,
            equipment.total_defense()
        );
        *last_log = now;
    }
    let (window, camera) = inventory_smoke
        .context
        .expect("completed inventory smoke must provide the test window and camera");
    let transform = *transform;
    match smoke.stage {
        0 | 1 | 2 | 3 | 4 | 41 | 5 | 6 => equip_field_armor(
            smoke, menu, bag, equipment, nearby, &view, keys, orbit, commands, player, transform,
            window, camera,
        ),
        61 | 62 | 63 | 7 | 700 | 701 | 702 => {
            check_inventory_layout(smoke, bag, equipment, view, commands, window, camera)
        }
        71 | 8 | 81 | 9 | 10 | 11 | 12 | 13 => check_field_armor_changes(
            smoke, state, menu, bag, equipment, nearby, &view, keys, orbit, commands, player,
            transform, window, camera,
        ),
        100 | 101 | 102 | 103 | 104 | 105 | 106 | 161 | 107 | 108 | 109 | 110 | 111 | 112 => {
            equip_reinforced_armor(
                smoke, menu, bag, equipment, nearby, &view, keys, orbit, commands, player,
                transform, window, camera,
            )
        }
        49..=59 => check_worn_armor_in_motion(
            smoke, state, menu, equipment, &view, keys, orbit, commands, player, window, camera,
            now,
        ),
        _ => {}
    }
}

fn equip_field_armor(
    mut smoke: ResMut<EquipmentSmoke>,
    menu: Res<PlayerMenu>,
    bag: Res<Inventory>,
    equipment: Res<Equipment>,
    nearby: Res<Nearby>,
    view: &View,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    player: Entity,
    transform: Transform,
    window: Entity,
    camera: Entity,
) {
    macro_rules! click {
        ($action:expr) => {
            click(&mut commands, $action, &view.actions, window, camera)
        };
    }
    macro_rules! photo {
        ($name:expr) => {
            smoke.screenshot = Some(super::capture(&mut commands, $name));
        };
    }
    match smoke.stage {
        0 => {
            if collect(
                ItemKind::FieldArmor,
                &bag,
                &nearby,
                view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                smoke.stage = 1;
            }
        }
        1 => {
            if collect(
                ItemKind::FieldHelmet,
                &bag,
                &nearby,
                view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                keys.press(KeyCode::KeyI);
                smoke.stage = 2;
            }
        }
        2 if menu.is_on(MenuTab::Inventory) => {
            let (_, action) = view
                .bag_rows(ItemKind::FieldArmor)
                .next()
                .expect("field armor bag row");
            click!(action);
            smoke.stage = 3;
        }
        3 => {
            assert_eq!(equipment.get(EquipmentSlot::Body), None);
            let (row, _) = view
                .bag_rows(ItemKind::FieldArmor)
                .next()
                .expect("field armor inventory row");
            commands.trigger(PointerClick {
                count: 2,
                ..pointer_click(row, window, camera)
            });
            smoke.stage = 4;
        }
        4 => {
            assert_eq!(
                equipment.get(EquipmentSlot::Body),
                Some(ItemKind::FieldArmor)
            );
            click(
                &mut commands,
                MenuTab::Equipment,
                &view.tabs,
                window,
                camera,
            );
            smoke.stage = 41;
        }
        41 if menu.is_on(MenuTab::Equipment) => {
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Helmet));
            smoke.stage = 5;
        }
        5 => {
            click!(InventoryAction::EquipFromSlot(
                EquipmentSlot::Helmet,
                ItemKind::FieldHelmet
            ));
            smoke.stage = 6;
        }
        6 if check_worn(view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 3);
            assert_eq!(bag.count(ItemKind::FieldArmor), 0);
            let (_, cells) = view
                .inventory_rows
                .iter()
                .find(|(action, _)| {
                    **action
                        == InventoryAction::Select(InventoryRow::Equipped(
                            ItemKind::FieldArmor,
                            EquipmentSlot::Body,
                        ))
                })
                .expect("the inventory table must have a row for worn armor");
            let values: Vec<_> = cells
                .iter()
                .filter_map(|cell| view.text.get(cell).ok())
                .map(|text| text.as_str())
                .collect();
            assert_eq!(
                values,
                ["Field armor", "1", "5 kg", "Equipped (Body armor)"]
            );
            photo!("field-equipment-inventory");
            smoke.stage = 61;
        }
        _ => {}
    }
}

fn check_inventory_layout(
    mut smoke: ResMut<EquipmentSmoke>,
    bag: Res<Inventory>,
    equipment: Res<Equipment>,
    mut view: View,
    mut commands: Commands,
    window: Entity,
    camera: Entity,
) {
    macro_rules! click {
        ($action:expr) => {
            click(&mut commands, $action, &view.actions, window, camera)
        };
    }
    macro_rules! photo {
        ($name:expr) => {
            smoke.screenshot = Some(super::capture(&mut commands, $name));
        };
    }
    match smoke.stage {
        61 if smoke.captured(&view) => {
            let (mut screen, _) = view
                .screen
                .single_mut()
                .expect("equipment layout checks require one inventory screen");
            smoke.screen_size = Some((screen.width, screen.height));
            screen.width = px(640);
            screen.height = px(704);
            smoke.stage = 62;
        }
        62 => {
            let (_, computed) = view
                .screen
                .single_mut()
                .expect("the narrow layout check requires one inventory screen");
            let size = computed.size() * computed.inverse_scale_factor();
            assert!(
                (size - Vec2::new(640.0, 704.0)).abs().max_element() <= 1.0,
                "the inventory must lay out at 640x704, got {size}"
            );

            photo!("field-equipment-inventory-narrow");
            smoke.stage = 63;
        }
        63 if smoke.captured(&view) => {
            let (width, height) = smoke
                .screen_size
                .take()
                .expect("the narrow layout check must preserve the original inventory dimensions");
            let (mut screen, _) = view
                .screen
                .single_mut()
                .expect("equipment layout checks require one inventory screen");
            screen.width = width;
            screen.height = height;
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Body));
            smoke.stage = 7;
        }
        7 => {
            assert_eq!(equipment.get(EquipmentSlot::Body), None);
            assert_eq!(bag.count(ItemKind::FieldArmor), 1);
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Body));
            smoke.stage = 700;
        }
        700 => {
            click!(InventoryAction::EquipFromSlot(
                EquipmentSlot::Body,
                ItemKind::FieldArmor
            ));
            smoke.stage = 701;
        }
        701 if equipment.get(EquipmentSlot::Body) == Some(ItemKind::FieldArmor) => {
            assert_eq!(bag.count(ItemKind::FieldArmor), 0);
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Body));
            smoke.stage = 702;
        }
        702 if equipment.get(EquipmentSlot::Body).is_none() => {
            assert_eq!(bag.count(ItemKind::FieldArmor), 1);
            click(
                &mut commands,
                MenuTab::Inventory,
                &view.tabs,
                window,
                camera,
            );
            smoke.stage = 71;
        }
        _ => {}
    }
}

fn check_field_armor_changes(
    mut smoke: ResMut<EquipmentSmoke>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    bag: Res<Inventory>,
    equipment: Res<Equipment>,
    nearby: Res<Nearby>,
    view: &View,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    player: Entity,
    transform: Transform,
    window: Entity,
    camera: Entity,
) {
    macro_rules! click {
        ($action:expr) => {
            click(&mut commands, $action, &view.actions, window, camera)
        };
    }
    match smoke.stage {
        71 if menu.is_on(MenuTab::Inventory) => {
            let (_, action) = view
                .bag_rows(ItemKind::FieldArmor)
                .next()
                .expect("unequipped field armor bag row");
            click!(action);
            smoke.stage = 8;
        }
        8 => {
            click!(InventoryAction::Drop);
            smoke.stage = 81;
        }
        81 => {
            click!(InventoryAction::ConfirmDrop);
            smoke.stage = 9;
        }
        9 => {
            assert_eq!(
                bag.count(ItemKind::FieldArmor),
                0,
                "unequipped armor was not dropped"
            );
            assert_eq!(equipment.total_defense(), 1);
            keys.press(KeyCode::KeyI);
            smoke.stage = 10;
        }
        10 if *state.get() == GameState::Playing && !menu.is_open() => {
            if collect(
                ItemKind::FieldArmor,
                &bag,
                &nearby,
                view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                keys.press(KeyCode::KeyI);
                smoke.stage = 11;
            }
        }
        11 if menu.is_on(MenuTab::Inventory) => {
            click(
                &mut commands,
                MenuTab::Equipment,
                &view.tabs,
                window,
                camera,
            );
        }
        11 if menu.is_on(MenuTab::Equipment) => {
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Body));
            smoke.stage = 12;
        }
        12 => {
            click!(InventoryAction::EquipFromSlot(
                EquipmentSlot::Body,
                ItemKind::FieldArmor
            ));
            smoke.stage = 13;
        }
        13 if check_worn(view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 3);
            keys.press(KeyCode::Tab);
            smoke.stage = 49;
        }
        _ => {}
    }
}

fn equip_reinforced_armor(
    mut smoke: ResMut<EquipmentSmoke>,
    menu: Res<PlayerMenu>,
    bag: Res<Inventory>,
    equipment: Res<Equipment>,
    nearby: Res<Nearby>,
    view: &View,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    player: Entity,
    transform: Transform,
    window: Entity,
    camera: Entity,
) {
    macro_rules! click {
        ($action:expr) => {
            click(&mut commands, $action, &view.actions, window, camera)
        };
    }
    macro_rules! photo {
        ($name:expr) => {
            smoke.screenshot = Some(super::capture(&mut commands, $name));
        };
    }
    match smoke.stage {
        100 => {
            if collect(
                ItemKind::ReinforcedArmor,
                &bag,
                &nearby,
                view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                smoke.stage = 101;
            }
        }
        101 => {
            if collect(
                ItemKind::ReinforcedHelmet,
                &bag,
                &nearby,
                view,
                &mut keys,
                &mut orbit,
                transform.translation,
            ) {
                keys.press(KeyCode::KeyI);
                smoke.stage = 102;
            }
        }
        102 if menu.is_on(MenuTab::Inventory) => {
            click(
                &mut commands,
                MenuTab::Equipment,
                &view.tabs,
                window,
                camera,
            );
        }
        102 if menu.is_on(MenuTab::Equipment) => {
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Body));
            smoke.stage = 103;
        }
        103 => {
            click!(InventoryAction::EquipFromSlot(
                EquipmentSlot::Body,
                ItemKind::ReinforcedArmor
            ));
            smoke.stage = 104;
        }
        104 => {
            assert_eq!(bag.count(ItemKind::FieldArmor), 1);
            assert_eq!(equipment.total_defense(), 6);
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Helmet));
            smoke.stage = 105;
        }
        105 => {
            click!(InventoryAction::EquipFromSlot(
                EquipmentSlot::Helmet,
                ItemKind::ReinforcedHelmet
            ));
            smoke.stage = 106;
        }
        106 if check_worn(view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 8);
            assert_eq!(bag.count(ItemKind::FieldHelmet), 1);
            photo!("reinforced-equipment-inventory");
            smoke.stage = 161;
        }
        161 if smoke.captured(view) => {
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Helmet));
            smoke.stage = 107;
        }
        107 => {
            assert_eq!(equipment.get(EquipmentSlot::Helmet), None);
            assert_eq!(bag.count(ItemKind::ReinforcedHelmet), 1);
            click!(InventoryAction::ToggleSlot(EquipmentSlot::Helmet));
            smoke.stage = 108;
        }
        108 => {
            click!(InventoryAction::EquipFromSlot(
                EquipmentSlot::Helmet,
                ItemKind::ReinforcedHelmet
            ));
            smoke.stage = 109;
        }
        109 if check_worn(view, &equipment, player) => {
            assert_eq!(equipment.total_defense(), 8);
            click!(InventoryAction::ToggleSlot(EquipmentSlot::MainHand));
            smoke.stage = 110;
        }
        110 => {
            click!(InventoryAction::EquipFromSlot(
                EquipmentSlot::MainHand,
                ItemKind::Trident
            ));
            smoke.stage = 111;
        }
        111 if check_worn(view, &equipment, player) => {
            assert_eq!(
                equipment.get(EquipmentSlot::MainHand),
                Some(ItemKind::Trident)
            );
            keys.press(KeyCode::Tab);
            smoke.stage = 112;
        }
        112 if !menu.is_open() => {
            keys.press(KeyCode::KeyX);
            smoke.stage = 49;
        }
        _ => {}
    }
}

fn check_worn_armor_in_motion(
    mut smoke: ResMut<EquipmentSmoke>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    equipment: Res<Equipment>,
    view: &View,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    mut commands: Commands,
    player: Entity,
    window: Entity,
    camera: Entity,
    now: f32,
) {
    let (_, _, controller, stance) = view.player.get(player).expect("arena player");
    macro_rules! click {
        ($action:expr) => {
            click(&mut commands, $action, &view.actions, window, camera)
        };
    }
    macro_rules! photo {
        ($name:expr) => {
            smoke.screenshot = Some(super::capture(&mut commands, $name));
        };
    }
    match smoke.stage {
        49 if *state.get() == GameState::Playing && !menu.is_open() => {
            if smoke.heavy {
                assert_eq!(*stance, WeaponStance::Holstered);
            }
            // Both equipment rows face a clear strip of floor to the north.
            orbit.yaw = 0.0;
            orbit.pitch = 0.18;
            orbit.distance = 3.7;
            smoke.timer = now;
            smoke.stage = 50;
        }
        50 if now - smoke.timer > 0.3 => {
            assert!(check_worn(view, &equipment, player));
            photo!(if smoke.heavy {
                "reinforced-gear-idle"
            } else {
                "field-gear-idle"
            });
            smoke.timer = now;
            smoke.stage = 51;
        }
        51 => {
            keys.press(KeyCode::KeyW);
            if now - smoke.timer > 0.5 {
                assert!(
                    controller.grounded
                        && controller.pace == Pace::Walk
                        && controller.velocity.xz().length() > 0.1,
                    "the walk capture requires grounded walking"
                );
                assert!(check_worn(view, &equipment, player));
                photo!(if smoke.heavy {
                    "reinforced-gear-walk"
                } else {
                    "field-gear-walk"
                });
                smoke.timer = now;
                smoke.stage = 52;
            }
        }
        52 => {
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
            if now - smoke.timer > 0.5 {
                assert!(
                    controller.grounded
                        && controller.pace == Pace::Sprint
                        && controller.velocity.xz().length() > 0.1,
                    "the sprint capture requires grounded sprinting"
                );
                assert!(check_worn(view, &equipment, player));
                photo!(if smoke.heavy {
                    "reinforced-gear-run"
                } else {
                    "field-gear-run"
                });
                smoke.stage = 53;
            }
        }
        53 if controller.grounded => {
            keys.press(KeyCode::Space);
            smoke.timer = now;
            smoke.stage = 54;
        }
        54 if now - smoke.timer > 0.3 => {
            assert!(
                !controller.grounded,
                "equipment jump check did not leave the ground"
            );
            assert!(check_worn(view, &equipment, player));
            photo!(if smoke.heavy {
                "reinforced-gear-jump"
            } else {
                "field-gear-jump"
            });
            smoke.stage = 55;
        }
        55 if controller.grounded => {
            assert!(check_worn(view, &equipment, player));
            orbit.yaw = 0.0;
            orbit.pitch = 0.22;
            orbit.distance = 5.5;
            if smoke.heavy {
                keys.press(KeyCode::KeyX);
                smoke.stage = 56;
            } else {
                smoke.heavy = true;
                smoke.stage = 100;
            }
        }
        56 => {
            assert_eq!(*stance, WeaponStance::Drawn);
            assert!(check_worn(view, &equipment, player));
            keys.press(KeyCode::Tab);
            smoke.stage = 57;
        }
        57 if menu.accepts(MenuTab::Equipment) => {
            click!(InventoryAction::ToggleSlot(EquipmentSlot::MainHand));
            smoke.stage = 58;
        }
        58 if check_worn(view, &equipment, player) => {
            assert_eq!(equipment.get(EquipmentSlot::MainHand), None);
            keys.press(KeyCode::Tab);
            smoke.stage = 59;
        }
        59 if !menu.is_open() => {
            info!(
                "EQUIPMENT SMOKE PASS: checked armor and holstered trident during idle, walk, sprint, and jump"
            );
            smoke.stage = 60;
        }
        _ => {}
    }
}
