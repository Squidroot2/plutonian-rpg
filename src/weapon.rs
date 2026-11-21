use avian3d::prelude::*;
use bevy::{prelude::*, window::PrimaryWindow};

use crate::{
    GameState,
    combat::{CombatSystems, Contact, Hit},
    controls::{Control, Keybinds},
    equipment::{Equipment, EquipmentSlot, WeaponKind},
    interaction::InteractionSystems,
    inventory::{CARRY_CAPACITY, Inventory},
    menu_ui::PlayerMenu,
    player::{Dodge, MovementInput, Player},
    quest::Dialogue,
};

const GRIP_HEIGHT: f32 = -0.05;
pub(crate) const GRIP_SIDE: f32 = -0.40;

impl WeaponKind {
    pub(crate) fn reach(self) -> f32 {
        match self {
            Self::Spear => 2.5,
            Self::Shortsword => 1.4,
        }
    }

    fn timing(self) -> (f32, f32) {
        match self {
            Self::Spear => (6.0 / 24.0, 16.0 / 24.0),
            Self::Shortsword => (5.0 / 24.0, 12.0 / 24.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hand {
    Main,
    Off,
}

impl Hand {
    pub(crate) const ALL: [Self; 2] = [Self::Main, Self::Off];

    fn slot(self) -> EquipmentSlot {
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
    pub(crate) elapsed: f32,
}

pub struct WeaponPlugin;
impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponAttack>()
            .add_systems(
                OnExit(GameState::Playing),
                |mut attack: ResMut<WeaponAttack>| {
                    attack.cancel();
                },
            )
            .add_systems(
                Update,
                attack
                    .after(InteractionSystems)
                    .in_set(CombatSystems::Detect)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Default, Resource)]
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

    fn advance(&mut self, hand: Hand, delta: f32) -> Option<WeaponKind> {
        let slot = &mut self.swings[hand as usize];
        let swing = slot.as_mut()?;
        let weapon = swing.weapon;
        let (impact_at, recovery_at) = weapon.timing();
        let previous = swing.elapsed;
        swing.elapsed += delta;
        let impact = previous < impact_at && swing.elapsed >= impact_at;
        if swing.elapsed >= recovery_at {
            *slot = None;
        }
        impact.then_some(weapon)
    }
}

fn attack(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    windows: Query<&Window, With<PrimaryWindow>>,
    equipment: Res<Equipment>,
    inventory: Res<Inventory>,
    dialogue: Res<Dialogue>,
    time: Res<Time>,
    spatial: SpatialQuery,
    movement: Res<MovementInput>,
    mut swing: ResMut<WeaponAttack>,
    player: Query<(Entity, &Player, &Dodge, &Transform)>,
    mut hits: MessageWriter<Hit>,
    log: Option<Res<PlayerMenu>>,
) {
    let Ok((entity, controller, dodge, transform)) = player.single() else {
        swing.cancel();
        return;
    };
    if dialogue.text.is_some() || !windows.iter().any(|window| window.focused) {
        swing.cancel();
        return;
    }
    let can_start = controller.grounded
        && !log.is_some_and(|log| log.is_open())
        && !dodge.active()
        && !(inventory.carried_weight(&equipment) <= CARRY_CAPACITY
            && movement.dodge_pending(controller.grounded, dodge));

    for hand in Hand::ALL {
        let equipped = equipment.weapon(hand.slot());
        if let Some(current) = swing.current(hand) {
            if equipped != Some(current.weapon) {
                swing.swings[hand as usize] = None;
            } else if let Some(weapon) = swing.advance(hand, time.delta_secs())
                && let Some(hit) = weapon_contact(
                    weapon,
                    hand,
                    &spatial,
                    entity,
                    transform.translation,
                    transform.rotation,
                )
            {
                hits.write(hit);
            }
        } else if can_start
            && let Some(weapon) = equipped
            && bindings.get(hand.control()).just_pressed(&keys, &buttons)
        {
            swing.swings[hand as usize] = Some(Swing {
                weapon,
                elapsed: 0.0,
            });
        }
    }
}

fn weapon_contact(
    weapon: WeaponKind,
    hand: Hand,
    spatial: &SpatialQuery,
    player: Entity,
    position: Vec3,
    facing: Quat,
) -> Option<Hit> {
    let (side, local_direction, radius, damage) = match weapon {
        WeaponKind::Spear => (GRIP_SIDE, Vec3::Z, 0.12, 10),
        WeaponKind::Shortsword => {
            // Sweep inward from the striking hand at close range.
            let sign = if hand == Hand::Main { -1.0 } else { 1.0 };
            (0.36 * sign, Vec3::new(-0.40 * sign, 0.0, 1.0), 0.18, 7)
        }
    };
    let direction = Dir3::new(facing * local_direction).ok()?;
    let origin = position + facing * Vec3::new(side, GRIP_HEIGHT, 0.0);
    spatial
        .cast_shape(
            &Collider::sphere(radius),
            origin,
            Quat::IDENTITY,
            direction,
            &ShapeCastConfig {
                compute_contact_on_penetration: true,
                ..ShapeCastConfig::from_max_distance(weapon.reach())
            },
            &SpatialQueryFilter::from_excluded_entities([player]),
        )
        .map(|hit| Hit {
            source: player,
            target: hit.entity,
            damage,
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
    use crate::equipment::{EquipmentPlugin, UnequipRequest};
    use crate::practice_dummy::PracticeDummy;
    use crate::{
        camera::Orbit,
        inventory::{InventoryNotice, ItemKind},
        player::face_camera,
    };
    use bevy::{
        asset::AssetPlugin, ecs::system::RunSystemOnce, mesh::MeshPlugin, state::app::StatesPlugin,
        time::TimeUpdateStrategy, transform::TransformPlugin,
    };
    use std::time::Duration;

    fn thrust_contact(
        spatial: &SpatialQuery,
        player: Entity,
        position: Vec3,
        facing: Quat,
    ) -> Option<Hit> {
        weapon_contact(
            WeaponKind::Spear,
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
            elapsed: 0.0,
        });
        assert_eq!(thrust.advance(Hand::Main, 0.2), None);
        assert_eq!(
            thrust.advance(Hand::Main, 0.06),
            Some(WeaponKind::Spear),
            "crossing full extension must report an impact"
        );
        assert_eq!(thrust.advance(Hand::Main, 0.2), None);
        assert_eq!(thrust.advance(Hand::Main, 0.3), None);
        assert!(!thrust.active());
    }

    fn test_app() -> App {
        let mut app = App::new();
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
        .init_resource::<Inventory>()
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
        world.spawn((player, Transform::default()));
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
            world
                .resource_mut::<Equipment>()
                .equip(&mut bag, ItemKind::FieldSpear),
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
                    assert!(app.world().resource::<WeaponAttack>().active());
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
            swing(&mut app, 0.0, Some(forward_yaw)).as_slice(),
            [crate::combat::ConfirmedHit {
                contact: Some(_),
                ..
            }]
        ));
        assert!(swing(&mut app, 0.0, None).is_empty());

        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(2.0, 2.0, 0.2),
            Transform::from_xyz(0.0, 0.0, 1.4),
        ));
        for _ in 0..3 {
            app.update();
        }
        assert!(
            swing(&mut app, forward_yaw, None).is_empty(),
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
        world.spawn((player, Transform::default()));
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(!app.world().resource::<WeaponAttack>().active());

        let world = app.world_mut();
        let mut bag = crate::inventory::Inventory::default();
        assert!(bag.add(ItemKind::FieldSpear, 1));
        assert_eq!(
            world
                .resource_mut::<Equipment>()
                .equip(&mut bag, ItemKind::FieldSpear),
            Ok(true)
        );
        world.resource_mut::<Dialogue>().text = Some("Talking");
        app.update();
        assert!(!app.world().resource::<WeaponAttack>().active());
        app.world_mut().resource_mut::<Dialogue>().text = None;
        app.update();
        assert!(app.world().resource::<WeaponAttack>().active());
        app.world_mut().resource_mut::<Dialogue>().text = Some("Talking");
        app.update();
        assert!(!app.world().resource::<WeaponAttack>().active());

        app.world_mut().resource_mut::<Dialogue>().text = None;
        app.update();
        assert!(app.world().resource::<WeaponAttack>().active());
        let mut menu = PlayerMenu::default();
        menu.show(crate::menu_ui::MenuTab::Inventory);
        app.insert_resource(menu);
        app.update();
        assert!(app.world().resource::<WeaponAttack>().active());
        for _ in 0..60 {
            app.update();
        }
        assert!(!app.world().resource::<WeaponAttack>().active());
    }

    #[test]
    fn hands_hit_together_and_recover_independently() {
        use crate::combat::Health;
        use EquipmentSlot::{MainHand, OffHand};
        use ItemKind::{FieldSpear, Shortsword};
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
        world.spawn((player, Transform::default()));
        let target = world
            .spawn((
                Health::new(60),
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
        assert!(bag.add(Shortsword, 2));
        assert!(bag.add(FieldSpear, 1));
        let mut equipment = Equipment::default();
        assert_eq!(equipment.equip(&mut bag, FieldSpear), Ok(true));
        app.insert_resource(equipment.clone());
        for _ in 0..3 {
            app.update();
        }
        click(&mut app, &[Right]);
        assert!(!app.world().resource::<WeaponAttack>().active());

        assert_eq!(equipment.unequip(&mut bag, MainHand), Ok(true));
        assert_eq!(equipment.equip_in(&mut bag, OffHand, Shortsword), Ok(true));
        app.insert_resource(equipment.clone());
        click(&mut app, &[Left]);
        assert!(!app.world().resource::<WeaponAttack>().active());
        click(&mut app, &[Right]);
        assert!(
            app.world()
                .resource::<WeaponAttack>()
                .current(Hand::Off)
                .is_some()
        );
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 53);

        assert_eq!(equipment.equip_in(&mut bag, MainHand, Shortsword), Ok(true));
        app.insert_resource(equipment);
        click(&mut app, &[Right, Left]);
        let attack = app.world().resource::<WeaponAttack>();
        assert!(
            Hand::ALL
                .into_iter()
                .all(|hand| attack.current(hand).is_some())
        );
        for _ in 0..12 {
            app.update();
        }
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 39);
        click(&mut app, &[Left, Right]);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 39);
        assert!(!app.world().resource::<WeaponAttack>().active());

        click(&mut app, &[Left]);
        for _ in 0..4 {
            app.update();
        }
        click(&mut app, &[Right]);
        let attack = app.world().resource::<WeaponAttack>();
        assert!(attack.current(Hand::Main).unwrap().elapsed > 0.0);
        assert_eq!(attack.current(Hand::Off).unwrap().elapsed, 0.0);
        for _ in 0..7 {
            app.update();
        }
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 32);
        for _ in 0..14 {
            app.update();
        }
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 25);
        let attack = app.world().resource::<WeaponAttack>();
        assert!(attack.current(Hand::Main).is_none());
        assert!(attack.current(Hand::Off).is_some());
        click(&mut app, &[Left]);
        assert!(
            app.world()
                .resource::<WeaponAttack>()
                .current(Hand::Main)
                .is_some()
        );

        let mut menu = PlayerMenu::default();
        menu.show(crate::menu_ui::MenuTab::Equipment);
        app.insert_resource(menu);
        app.world_mut().trigger(UnequipRequest(MainHand));
        assert!(!app.world().resource::<WeaponAttack>().active());
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(app.world().get::<Health>(target).unwrap().current(), 25);
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
