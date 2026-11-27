#[cfg(test)]
use crate::player::PlayerWorldExt as _;
use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    GameState,
    attributes::{CarryLimits, MeleeDamageMultiplier, MovementCosts},
    combat::{CombatSystems, Contact, Hit},
    content::{Items, WeaponStats},
    controls::{Control, Keybinds},
    dialogue::Dialogue,
    equipment::{Equipment, EquipmentSlot, WeaponKind},
    interaction::InteractionSystems,
    inventory::{Inventory, ItemKind},
    player::PlayerRead,
    player::{Dodge, MovementInput, Player, Stamina},
    ui::menu_ui::PlayerMenu,
};

const GRIP_HEIGHT: f32 = -0.05;
pub(crate) const GRIP_SIDE: f32 = -0.40;
pub(crate) const ATTACK_CLIP_FPS: f32 = 24.0;
const SPEAR_IMPACT_FRAME: f32 = 6.0;
const SPEAR_RECOVERY_FRAME: f32 = 16.0;
const SWORD_IMPACT_FRAME: f32 = 5.0;
const SWORD_RECOVERY_FRAME: f32 = 12.0;
const SWORD_GRIP_OFFSET: f32 = 0.36;
const SWORD_INWARD_SWEEP: f32 = 0.40;

impl WeaponKind {
    pub(crate) const fn timing(self) -> (f32, f32) {
        let (impact, recovery) = match self {
            Self::Spear => (SPEAR_IMPACT_FRAME, SPEAR_RECOVERY_FRAME),
            Self::Shortsword => (SWORD_IMPACT_FRAME, SWORD_RECOVERY_FRAME),
        };
        (impact / ATTACK_CLIP_FPS, recovery / ATTACK_CLIP_FPS)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hand {
    Main,
    Off,
}

impl Hand {
    pub(crate) const ALL: [Self; 2] = [Self::Main, Self::Off];

    pub(crate) fn slot(self) -> EquipmentSlot {
        match self {
            Self::Main => EquipmentSlot::MainHand,
            Self::Off => EquipmentSlot::OffHand,
        }
    }

    fn control(self) -> Control {
        match self {
            Self::Main => Control::Attack,
            Self::Off => Control::OffHandAttack,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Swing {
    pub(crate) weapon: WeaponKind,
    item: ItemKind,
    stats: WeaponStats,
    pub(crate) elapsed: f32,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WeaponStance {
    #[default]
    Drawn,
    Holstered,
}

pub struct WeaponPlugin;
impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(GameState::Playing), cancel_attacks)
            .add_systems(
                Update,
                attack
                    .after(InteractionSystems)
                    .in_set(CombatSystems::Detect)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn cancel_attacks(mut attacks: Query<&mut WeaponAttack, With<Player>>) {
    for mut attack in &mut attacks {
        attack.cancel();
    }
}

/// The in-progress swings for an actor, tracked independently for each hand.
#[derive(Component, Default)]
pub struct WeaponAttack {
    swings: [Option<Swing>; 2],
}
impl WeaponAttack {
    pub fn active(&self) -> bool {
        self.swings.iter().any(Option::is_some)
    }

    pub(crate) fn current(&self, hand: Hand) -> Option<Swing> {
        self.swings[hand as usize]
    }

    pub(crate) fn just_started(&self) -> bool {
        self.swings
            .iter()
            .flatten()
            .any(|swing| swing.elapsed == 0.0)
    }

    pub(crate) fn cancel(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn begin(&mut self, hand: Hand, equipment: &Equipment, items: &Items) -> bool {
        if self.current(hand).is_some() {
            return false;
        }
        let Some(item) = equipment.get(hand.slot()) else {
            return false;
        };
        let Some((weapon, stats)) = items[item].attack() else {
            return false;
        };
        self.swings[hand as usize] = Some(Swing {
            weapon,
            item,
            stats,
            elapsed: 0.0,
        });
        true
    }

    pub(crate) fn strike(
        &mut self,
        hand: Hand,
        equipment: &Equipment,
        delta: f32,
        melee: MeleeDamageMultiplier,
        spatial: &SpatialQuery,
        entity: Entity,
        pose: &Transform,
    ) -> Option<Hit> {
        if equipment.get(hand.slot()) != Some(self.current(hand)?.item) {
            self.swings[hand as usize] = None;
            return None;
        }
        let (weapon, stats) = self.advance(hand, delta)?;
        let mut hit = weapon_contact(
            weapon,
            stats,
            hand,
            spatial,
            entity,
            pose.translation,
            pose.rotation,
        )?;
        hit.damage = melee.scale(hit.damage);
        Some(hit)
    }

    fn advance(&mut self, hand: Hand, delta: f32) -> Option<(WeaponKind, WeaponStats)> {
        let slot = &mut self.swings[hand as usize];
        let swing = slot.as_mut()?;
        let (weapon, stats) = (swing.weapon, swing.stats);
        let (impact_at, recovery_at) = weapon.timing();
        let previous = swing.elapsed;
        swing.elapsed += delta;
        let impact = previous < impact_at && swing.elapsed >= impact_at;
        if swing.elapsed >= recovery_at {
            *slot = None;
        }
        impact.then_some((weapon, stats))
    }
}

fn attack(
    items: Res<Items>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    equipment: PlayerRead<Equipment>,
    inventory: PlayerRead<Inventory>,
    dialogue: Res<Dialogue>,
    time: Res<Time>,
    spatial: SpatialQuery,
    movement: Res<MovementInput>,
    mut player: Query<(
        Entity,
        &Player,
        &Dodge,
        &Stamina,
        &MovementCosts,
        &CarryLimits,
        &MeleeDamageMultiplier,
        &Transform,
        &mut WeaponStance,
        &mut WeaponAttack,
    )>,
    mut hits: MessageWriter<Hit>,
    log: Option<Res<PlayerMenu>>,
    next: Res<NextState<GameState>>,
) {
    let Ok((
        entity,
        controller,
        dodge,
        stamina,
        costs,
        limits,
        melee,
        transform,
        mut stance,
        mut swing,
    )) = player.single_mut()
    else {
        for (.., mut swing) in &mut player {
            swing.cancel();
        }
        return;
    };
    if dialogue.is_open() {
        swing.cancel();
        return;
    }
    let menu_open = log.is_some_and(|log| log.is_open());
    if bindings.just_pressed(Control::Holster, &keys)
        && !menu_open
        && matches!(*next, NextState::Unchanged)
        && !swing.active()
        && EquipmentSlot::HANDS
            .into_iter()
            .any(|hand| equipment.weapon(hand, &items).is_some())
    {
        *stance = match *stance {
            WeaponStance::Drawn => WeaponStance::Holstered,
            WeaponStance::Holstered => WeaponStance::Drawn,
        };
        return;
    }
    if *stance == WeaponStance::Holstered {
        return;
    }

    let can_start = controller.grounded
        && !menu_open
        && !dodge.active()
        && !(inventory.carried_weight(&equipment, &items) <= limits.capacity_kg
            && movement.dodge_pending(controller.grounded, dodge, stamina, costs.dodge));

    for hand in Hand::ALL {
        if swing.current(hand).is_some() {
            if let Some(hit) = swing.strike(
                hand,
                &equipment,
                time.delta_secs(),
                *melee,
                &spatial,
                entity,
                transform,
            ) {
                hits.write(hit);
            }
        } else if can_start && bindings.just_pressed_any(hand.control(), &keys, &buttons) {
            swing.begin(hand, &equipment, &items);
        }
    }
}

/// Aim the hand's offset contact cast toward a horizontal target.
pub(crate) fn weapon_heading(profile: WeaponKind, hand: Hand, delta: Vec3) -> Quat {
    let (side, direction) = contact_geometry(profile, hand);
    let local_origin = Vec2::new(side, 0.0);
    let ray = direction.xz().normalize();
    let along = local_origin.dot(ray);
    let distance_squared = delta
        .xz()
        .length_squared()
        .max(local_origin.length_squared());
    let travel = (along * along + distance_squared - local_origin.length_squared())
        .max(0.0)
        .sqrt()
        - along;
    let contact = local_origin + ray * travel;
    crate::terrain::ground_heading(delta) * Quat::from_rotation_y(-contact.x.atan2(contact.y))
}

fn contact_geometry(weapon: WeaponKind, hand: Hand) -> (f32, Vec3) {
    match weapon {
        WeaponKind::Spear => (GRIP_SIDE, Vec3::Z),
        WeaponKind::Shortsword => {
            let sign = if hand == Hand::Main { -1.0 } else { 1.0 };
            (
                SWORD_GRIP_OFFSET * sign,
                Vec3::new(-SWORD_INWARD_SWEEP * sign, 0.0, 1.0),
            )
        }
    }
}

pub(crate) fn weapon_contact(
    weapon: WeaponKind,
    stats: WeaponStats,
    hand: Hand,
    spatial: &SpatialQuery,
    player: Entity,
    position: Vec3,
    facing: Quat,
) -> Option<Hit> {
    let (side, local_direction) = contact_geometry(weapon, hand);
    let direction = Dir3::new(facing * local_direction).ok()?;
    let origin = position + facing * Vec3::new(side, GRIP_HEIGHT, 0.0);
    spatial
        .cast_shape(
            &Collider::sphere(stats.contact_radius_m),
            origin,
            Quat::IDENTITY,
            direction,
            &ShapeCastConfig {
                compute_contact_on_penetration: true,
                ..ShapeCastConfig::from_max_distance(stats.reach_m)
            },
            &SpatialQueryFilter::from_excluded_entities([player]),
        )
        .map(|hit| Hit {
            source: player,
            target: hit.entity,
            damage: stats.damage,
            contact: Some(Contact {
                position: hit.point1,
                normal: hit.normal1,
            }),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::TrainingTarget;
    use crate::content::test_items;
    use crate::equipment::{EquipmentPlugin, UnequipRequest};
    use crate::practice_dummy::PracticeDummy;
    use crate::{
        camera::Orbit,
        inventory::{InventoryNotice, ItemKind},
        player::face_camera,
    };
    use bevy::window::PrimaryWindow;
    use bevy::{
        asset::AssetPlugin, ecs::system::RunSystemOnce, mesh::MeshPlugin, state::app::StatesPlugin,
        time::TimeUpdateStrategy, transform::TransformPlugin,
    };
    use std::time::Duration;

    fn test_stats(item: ItemKind) -> WeaponStats {
        test_items()[item]
            .attack()
            .expect("test item is a weapon")
            .1
    }

    fn thrust_contact(
        spatial: &SpatialQuery,
        player: Entity,
        position: Vec3,
        facing: Quat,
    ) -> Option<Hit> {
        weapon_contact(
            WeaponKind::Spear,
            test_stats(ItemKind::FieldSpear),
            Hand::Main,
            spatial,
            player,
            position,
            facing,
        )
    }

    #[test]
    fn thrust_has_one_impact_and_ends_after_recovery() {
        let mut thrust = WeaponAttack::default();
        thrust.swings[Hand::Main as usize] = Some(Swing {
            weapon: WeaponKind::Spear,
            item: ItemKind::FieldSpear,
            stats: test_stats(ItemKind::FieldSpear),
            elapsed: 0.0,
        });
        assert_eq!(thrust.advance(Hand::Main, 0.2), None);
        assert_eq!(
            thrust.advance(Hand::Main, 0.06),
            Some((WeaponKind::Spear, test_stats(ItemKind::FieldSpear))),
            "crossing full extension must report an impact"
        );
        assert_eq!(thrust.advance(Hand::Main, 0.2), None);
        assert_eq!(thrust.advance(Hand::Main, 0.3), None);
        assert!(!thrust.active());
    }

    fn player_attack(app: &App, player: Entity) -> &WeaponAttack {
        app.world()
            .get::<WeaponAttack>(player)
            .expect("the test player must have its required attack component")
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(test_items().clone());
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            TransformPlugin,
            AssetPlugin::default(),
            MeshPlugin,
            PhysicsPlugins::default(),
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .init_asset::<AudioSource>()
        .init_state::<GameState>()
        .init_resource::<Dialogue>()
        .init_resource::<InventoryNotice>()
        .init_resource::<Orbit>()
        .init_resource::<MovementInput>()
        .init_resource::<Keybinds>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins((crate::combat::CombatPlugin, EquipmentPlugin, WeaponPlugin))
        .add_systems(Update, face_camera.before(CombatSystems::Detect));
        app.finish();
        app.cleanup();
        app
    }

    #[test]
    fn thrust_respects_aim_reach_and_world_occlusion() {
        let mut app = test_app();
        let player = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::sphere(0.5),
                Transform::default(),
            ))
            .id();
        let dummy = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(0.72, 1.5, 0.55),
                Transform::from_xyz(GRIP_SIDE, 0.0, 2.4),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }
        let hit = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                thrust_contact(&spatial, player, Vec3::ZERO, Quat::IDENTITY)
            })
            .expect("physics resources must be initialized for the thrust cast");
        let hit = hit.expect("the cast must ignore the player collider and reach the dummy");
        assert_eq!(hit.target, dummy);
        let contact = hit.contact.expect("thrust must include a world contact");
        assert!(
            (contact.position.z - 2.125).abs() < 0.02,
            "impact must lie on the dummy's front face"
        );
        assert!(contact.normal.dot(Vec3::NEG_Z) > 0.98);
        let misses = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                [
                    thrust_contact(
                        &spatial,
                        player,
                        Vec3::ZERO,
                        Quat::from_rotation_y(std::f32::consts::PI),
                    ),
                    thrust_contact(&spatial, player, Vec3::NEG_Z, Quat::IDENTITY),
                ]
            })
            .expect("physics resources must be initialized for both miss cases");
        assert_eq!(misses, [None, None]);
        let wall = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(2.0, 2.0, 0.2),
                Transform::from_xyz(0.0, 0.0, 1.4),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }
        let hit = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                thrust_contact(&spatial, player, Vec3::ZERO, Quat::IDENTITY)
            })
            .expect("the thrust must query the updated world collision tree");
        assert_eq!(
            hit.map(|contact| contact.target),
            Some(wall),
            "the caster must stop at the wall before the dummy"
        );
    }

    #[test]
    fn grounded_thrust_reaches_the_roach_hitbox() {
        let mut app = test_app();
        let player_position = Vec3::Y * 0.905;
        let player = app
            .world_mut()
            .spawn(Transform::from_translation(player_position))
            .id();
        let roach = app
            .world_mut()
            .spawn((
                RigidBody::Kinematic,
                crate::roach::collider(),
                Transform::from_xyz(0.0, crate::roach::GROUND_OFFSET, 2.4),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }

        let hit = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                thrust_contact(&spatial, player, player_position, Quat::IDENTITY)
            })
            .expect("physics resources must be initialized for the thrust cast");
        assert_eq!(hit.map(|hit| hit.target), Some(roach));
    }

    #[test]
    fn only_a_landed_thrust_emits_one_impact() {
        let mut app = test_app();
        let world = app.world_mut();
        world.spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        let mut player = Player::default();
        player.grounded = true;
        let player = world.spawn((player, Transform::default())).id();
        world.spawn((
            PracticeDummy,
            TrainingTarget,
            RigidBody::Static,
            Collider::cuboid(0.72, 1.5, 0.55),
            Transform::from_xyz(GRIP_SIDE, 0.0, 2.4),
        ));
        let mut bag = crate::inventory::Inventory::default();
        assert!(bag.add(ItemKind::FieldSpear, 1));
        assert_eq!(
            world.actor_data_mut::<Equipment>().equip(
                &mut bag,
                ItemKind::FieldSpear,
                10,
                test_items()
            ),
            Ok(true)
        );
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        for _ in 0..3 {
            app.update();
        }

        fn swing(
            app: &mut App,
            player: Entity,
            camera_yaw: f32,
            turn_to: Option<f32>,
        ) -> Vec<crate::combat::ConfirmedHit> {
            app.world_mut().resource_mut::<Orbit>().yaw = camera_yaw;
            let mut hits = app
                .world()
                .resource::<Messages<crate::combat::ConfirmedHit>>()
                .get_cursor_current();
            let mut feedback = Vec::new();
            let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            input.reset_all();
            input.press(MouseButton::Left);
            for frame in 0..40 {
                if frame == 5
                    && let Some(yaw) = turn_to
                {
                    assert!(player_attack(app, player).active());
                    app.world_mut().resource_mut::<Orbit>().yaw = yaw;
                }
                app.update();
                app.world_mut()
                    .resource_mut::<ButtonInput<MouseButton>>()
                    .clear_just_pressed(MouseButton::Left);
                feedback.extend(
                    hits.read(
                        app.world()
                            .resource::<Messages<crate::combat::ConfirmedHit>>(),
                    )
                    .copied(),
                );
            }
            feedback
        }

        let forward_yaw = -std::f32::consts::PI;
        assert!(matches!(
            swing(&mut app, player, 0.0, Some(forward_yaw)).as_slice(),
            [crate::combat::ConfirmedHit {
                contact: Some(_),
                ..
            }]
        ));
        assert!(swing(&mut app, player, 0.0, None).is_empty());

        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(2.0, 2.0, 0.2),
            Transform::from_xyz(0.0, 0.0, 1.4),
        ));
        for _ in 0..3 {
            app.update();
        }
        assert!(
            swing(&mut app, player, forward_yaw, None).is_empty(),
            "terrain must stop feedback along with the thrust"
        );
    }

    #[test]
    fn live_menu_preserves_a_thrust_in_progress_and_blocks_new_attacks() {
        let mut app = test_app();
        let world = app.world_mut();
        world.spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        let mut player = Player::default();
        player.grounded = true;
        let player = world.spawn((player, Transform::default())).id();
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(!player_attack(&app, player).active());

        let world = app.world_mut();
        let mut bag = crate::inventory::Inventory::default();
        assert!(bag.add(ItemKind::FieldSpear, 1));
        assert_eq!(
            world.actor_data_mut::<Equipment>().equip(
                &mut bag,
                ItemKind::FieldSpear,
                10,
                test_items()
            ),
            Ok(true)
        );
        world.resource_mut::<Dialogue>().show_notice_text("Talking");
        app.update();
        assert!(!player_attack(&app, player).active());
        app.world_mut().resource_mut::<Dialogue>().clear();
        app.update();
        assert!(player_attack(&app, player).active());
        app.world_mut()
            .resource_mut::<Dialogue>()
            .show_notice_text("Talking");
        app.update();
        assert!(!player_attack(&app, player).active());

        app.world_mut().resource_mut::<Dialogue>().clear();
        app.update();
        assert!(player_attack(&app, player).active());
        let mut menu = PlayerMenu::default();
        menu.show(crate::ui::menu_ui::MenuTab::Inventory);
        app.insert_resource(menu);
        app.update();
        assert!(player_attack(&app, player).active());
        for _ in 0..60 {
            app.update();
        }
        assert!(!player_attack(&app, player).active());
    }

    #[test]
    fn hands_hit_together_and_recover_independently() {
        use crate::combat::Health;
        use EquipmentSlot::{MainHand, OffHand};

        use MouseButton::{Left, Right};

        fn click(app: &mut App, buttons: &[MouseButton]) {
            let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            for &button in buttons {
                input.press(button);
            }
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .reset_all();
        }

        let mut app = test_app();
        let world = app.world_mut();
        world.spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        let mut player = Player::default();
        player.grounded = true;
        let player = world.spawn((player, Transform::default())).id();
        let target = world
            .spawn((
                Health::new(60),
                RigidBody::Static,
                Collider::cuboid(0.7, 1.5, 0.3),
                Transform::from_xyz(0.0, 0.0, 1.1),
            ))
            .id();
        let health = |app: &App| {
            app.world()
                .get::<Health>(target)
                .expect("the melee target must have health to take damage")
                .current()
        };
        world.resource_mut::<Orbit>().yaw = std::f32::consts::PI;
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);

        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::Shortsword, 2));
        assert!(bag.add(ItemKind::FieldSpear, 1));
        let mut equipment = Equipment::default();
        assert_eq!(
            equipment.equip(&mut bag, ItemKind::FieldSpear, 10, test_items()),
            Ok(true)
        );
        app.world_mut().set_player_data(equipment.clone());
        for _ in 0..3 {
            app.update();
        }
        click(&mut app, &[Right]);
        assert!(!player_attack(&app, player).active());

        assert_eq!(equipment.unequip(&mut bag, MainHand), Ok(true));
        assert_eq!(
            equipment.equip_in(&mut bag, OffHand, ItemKind::Shortsword, 10, test_items()),
            Ok(true)
        );
        app.world_mut().set_player_data(equipment.clone());
        click(&mut app, &[Left]);
        assert!(!player_attack(&app, player).active());
        click(&mut app, &[Right]);
        assert!(player_attack(&app, player).current(Hand::Off).is_some());
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(health(&app), 53);

        assert_eq!(
            equipment.equip_in(&mut bag, MainHand, ItemKind::Shortsword, 10, test_items()),
            Ok(true)
        );
        app.world_mut().set_player_data(equipment);
        click(&mut app, &[Right, Left]);
        let attack = player_attack(&app, player);
        assert!(
            Hand::ALL
                .into_iter()
                .all(|hand| attack.current(hand).is_some())
        );
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyX);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(KeyCode::KeyX);
        for _ in 0..11 {
            app.update();
        }
        assert_eq!(health(&app), 39);
        click(&mut app, &[Left, Right]);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(health(&app), 39);
        assert!(!player_attack(&app, player).active());
        assert_eq!(
            app.world().get::<WeaponStance>(player),
            Some(&WeaponStance::Drawn)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();

        click(&mut app, &[Left]);
        for _ in 0..4 {
            app.update();
        }
        click(&mut app, &[Right]);
        let attack = player_attack(&app, player);
        let main = attack
            .current(Hand::Main)
            .expect("the earlier main-hand swing must still be active");
        let off = attack
            .current(Hand::Off)
            .expect("the off-hand click must start a swing");
        assert!(main.elapsed > 0.0);
        assert_eq!(off.elapsed, 0.0);
        for _ in 0..7 {
            app.update();
        }
        assert_eq!(health(&app), 32);
        for _ in 0..14 {
            app.update();
        }
        assert_eq!(health(&app), 25);
        let attack = player_attack(&app, player);
        assert!(attack.current(Hand::Main).is_none());
        assert!(attack.current(Hand::Off).is_some());
        click(&mut app, &[Left]);
        assert!(player_attack(&app, player).current(Hand::Main).is_some());

        let mut menu = PlayerMenu::default();
        menu.show(crate::ui::menu_ui::MenuTab::Equipment);
        app.insert_resource(menu);
        app.world_mut().trigger(UnequipRequest(MainHand));
        assert!(!player_attack(&app, player).active());
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(health(&app), 25);
    }

    #[test]
    fn ron_weapon_uses_its_own_damage_and_reach_with_the_spear_profile() {
        use crate::{combat::Health, content::tests::variant_items};

        let mut app = test_app();
        let items = variant_items();
        let pike = ItemKind::parse("relay_pike").expect("valid authored item ID");
        let mut bag = Inventory::default();
        assert!(bag.add(pike, 1));
        let mut equipment = Equipment::default();
        assert_eq!(equipment.equip(&mut bag, pike, 10, &items), Ok(true));
        let world = app.world_mut();
        world.insert_resource(items);
        world.spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        let mut player = Player::default();
        player.grounded = true;
        let player = world.spawn((player, Transform::default(), equipment)).id();
        let target = world
            .spawn((
                Health::new(60),
                RigidBody::Static,
                Collider::cuboid(0.3, 1.5, 0.2),
                Transform::from_xyz(GRIP_SIDE, 0.0, 3.1),
            ))
            .id();
        world.resource_mut::<Orbit>().yaw = std::f32::consts::PI;
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        for _ in 0..3 {
            app.update();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        let swing = player_attack(&app, player)
            .current(Hand::Main)
            .expect("pike swing starts");
        assert_eq!((swing.weapon, swing.item), (WeaponKind::Spear, pike));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(
            app.world()
                .get::<Health>(target)
                .expect("target health")
                .current(),
            33
        );
    }

    #[test]
    fn strength_scales_both_hands_before_target_defense() {
        use crate::{
            attributes::{AttributeId, AttributeRanks},
            combat::{Defense, Health},
            player::{CharacterProgress, SavedProgress},
        };

        let mut app = test_app();
        let world = app.world_mut();
        world.spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        let mut controller = Player::default();
        controller.grounded = true;
        let ranks = AttributeRanks::default().with_rank(AttributeId::Strength, 100);
        let profile = SavedProgress::capture(&ranks, CharacterProgress::default());
        world.spawn((
            controller,
            Transform::default(),
            profile.player_components(),
        ));
        let target = world
            .spawn((
                Health::new(60),
                Defense(3),
                RigidBody::Static,
                Collider::cuboid(0.7, 1.5, 0.3),
                Transform::from_xyz(0.0, 0.0, 1.1),
            ))
            .id();
        world.resource_mut::<Orbit>().yaw = std::f32::consts::PI;
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::Shortsword, 2));
        let mut equipment = Equipment::default();
        for _ in 0..2 {
            assert_eq!(
                equipment.equip(&mut bag, ItemKind::Shortsword, 100, test_items()),
                Ok(true)
            );
        }
        app.world_mut().set_player_data(equipment);
        for _ in 0..3 {
            app.update();
        }
        let mut buttons = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        buttons.press(MouseButton::Left);
        buttons.press(MouseButton::Right);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(
            app.world()
                .get::<Health>(target)
                .expect("target has health")
                .current(),
            38
        );
    }

    #[test]
    fn holster_uses_the_current_binding_and_blocks_attacks_until_drawn() {
        use crate::controls::Binding;
        use KeyCode::{KeyH, KeyX};
        use MouseButton::{Left, Right};
        use WeaponStance::{Drawn, Holstered};

        fn frame(app: &mut App, key: Option<KeyCode>, clicks: &[MouseButton]) {
            let world = app.world_mut();
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            if let Some(key) = key {
                keys.press(key);
            }
            let mut buttons = world.resource_mut::<ButtonInput<MouseButton>>();
            buttons.reset_all();
            for &button in clicks {
                buttons.press(button);
            }
            app.update();
        }

        let mut app = test_app();
        let world = app.world_mut();
        world.spawn((
            PrimaryWindow,
            Window {
                focused: true,
                ..default()
            },
        ));
        let mut player = Player::default();
        player.grounded = true;
        let player = world.spawn((player, Transform::default())).id();
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.update();

        let stance = |app: &App| {
            *app.world()
                .get::<WeaponStance>(player)
                .expect("the player must have a weapon stance even with empty hands")
        };
        assert!(matches!(
            app.world().resource::<Keybinds>().get(Control::Holster),
            Binding::Key(KeyX)
        ));
        frame(&mut app, Some(KeyX), &[]);
        assert_eq!(stance(&app), Drawn, "empty hands leave stance unchanged");

        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::Shortsword, 1));
        assert_eq!(
            app.world_mut().actor_data_mut::<Equipment>().equip_in(
                &mut bag,
                EquipmentSlot::OffHand,
                ItemKind::Shortsword,
                5,
                test_items(),
            ),
            Ok(true)
        );
        let mut menu = PlayerMenu::default();
        menu.show(crate::ui::menu_ui::MenuTab::Inventory);
        app.insert_resource(menu);
        frame(&mut app, Some(KeyX), &[]);
        assert_eq!(stance(&app), Drawn, "the inventory takes precedence");
        app.world_mut().remove_resource::<PlayerMenu>();
        app.world_mut()
            .resource_mut::<Dialogue>()
            .show_notice_text("Talking");
        frame(&mut app, Some(KeyX), &[]);
        assert_eq!(stance(&app), Drawn, "dialogue takes precedence");
        app.world_mut().resource_mut::<Dialogue>().clear();

        frame(&mut app, Some(KeyX), &[Right]);
        assert_eq!(stance(&app), Holstered);
        assert!(!player_attack(&app, player).active());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(KeyX);
        app.update();
        assert_eq!(stance(&app), Holstered, "a held key toggles only once");
        frame(&mut app, None, &[Left, Right]);
        assert_eq!(stance(&app), Holstered);
        assert!(!player_attack(&app, player).active());

        app.world_mut()
            .resource_mut::<Keybinds>()
            .assign(
                Control::Holster,
                crate::controls::KeybindSlot::Primary,
                Binding::Key(KeyH),
            )
            .expect("H must be available to bind as the holster key");
        frame(&mut app, Some(KeyX), &[]);
        assert_eq!(stance(&app), Holstered);
        frame(&mut app, Some(KeyH), &[Right]);
        assert_eq!(stance(&app), Drawn);
        assert!(
            !player_attack(&app, player).active(),
            "drawing suppresses same-frame attacks"
        );
        frame(&mut app, None, &[Right]);
        assert!(player_attack(&app, player).current(Hand::Off).is_some());
    }

    #[test]
    fn a_sword_uses_the_correct_side_and_stops_at_walls_and_short_reach() {
        let mut app = test_app();
        let player = app.world_mut().spawn(Transform::default()).id();
        let close = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::sphere(0.11),
                Transform::from_xyz(0.4, 0.0, 0.45),
            ))
            .id();
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(1.0, 1.0, 0.2),
            Transform::from_xyz(0.0, 0.0, 1.9),
        ));
        let close_hits = |app: &mut App| {
            for _ in 0..3 {
                app.update();
            }
            app.world_mut()
                .run_system_once(move |spatial: SpatialQuery| {
                    Hand::ALL.map(|hand| {
                        weapon_contact(
                            WeaponKind::Shortsword,
                            test_stats(ItemKind::Shortsword),
                            hand,
                            &spatial,
                            player,
                            Vec3::ZERO,
                            Quat::IDENTITY,
                        )
                        .map(|hit| hit.target)
                    })
                })
                .expect("run the contact queries")
        };
        assert_eq!(close_hits(&mut app), [None, Some(close)]);
        let wall = app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(2.0, 2.0, 0.1),
                Transform::from_xyz(0.0, 0.0, 0.2),
            ))
            .id();
        assert_eq!(close_hits(&mut app), [Some(wall), Some(wall)]);
    }
}
