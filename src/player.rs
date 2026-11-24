use crate::{
    GameState,
    attributes::{
        AttributeRanks, CarryLimits, CharacterProgress, MeleeDamageMultiplier, MovementCosts,
    },
    combat::{CombatSystems, Defense, Health, HealthRecovery},
    content::Items,
    dungeon::Area,
    equipment::Equipment,
    interaction::InteractionSystems,
    inventory::Inventory,
    weapon::{WeaponAttack, WeaponStance},
};
use bevy::prelude::*;
mod animation;
mod movement;

#[cfg(test)]
use animation::Gait;
pub(crate) use animation::{
    ActorVisual, AnimationLibrary, CLIPS, NpcGait, add_sword_animation_masks, bind_actor_animation,
};
use animation::{animate, freeze, resume};
use movement::PLAYER_HIT_PROTECTION_SECONDS;
#[cfg(test)]
use movement::SPRINT_SPEED;
use movement::{
    DODGE_ANIMATION_BLEND, FootstepCadence, LOCOMOTION_ANIMATION_BLEND, RUN_SPEED, WALK_SPEED,
    footsteps,
};
pub(crate) use movement::{
    DODGE_KEY, Dodge, MovementInput, PLAYER_COLLIDER_LENGTH, PLAYER_COLLIDER_RADIUS,
    PLAYER_GROUND_OFFSET, Pace, Stamina, face_camera, input, movement,
};

pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementInput>()
            .add_systems(OnEnter(GameState::Paused), freeze)
            .add_systems(OnEnter(GameState::Looting), freeze)
            .add_systems(OnEnter(GameState::Storage), freeze)
            .add_systems(OnExit(GameState::Paused), resume)
            .add_systems(OnExit(GameState::Looting), resume)
            .add_systems(OnExit(GameState::Storage), resume)
            .add_systems(
                PreUpdate,
                input
                    .after(crate::ui::pause)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(FixedUpdate, movement.run_if(in_state(GameState::Playing)))
            .add_systems(Update, footsteps.run_if(in_state(GameState::Playing)))
            .add_systems(
                Update,
                face_camera
                    .after(InteractionSystems)
                    .before(CombatSystems::Detect)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                sync_defense
                    .before(CombatSystems::Resolve)
                    .run_if(resource_exists::<Items>),
            )
            .add_systems(
                Update,
                animate
                    .after(CombatSystems::Resolve)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
#[derive(Component, Default)]
#[require(Dodge, Health = Health::new(AttributeRanks::default().effects().health), Defense, crate::combat::HitProtection = crate::combat::HitProtection::new(PLAYER_HIT_PROTECTION_SECONDS))]
#[require(HealthRecovery = HealthRecovery::character(AttributeRanks::default().effects().health_per_second))]
#[require(
    AttributeRanks,
    CharacterProgress,
    crate::experience::Experience,
    CarryLimits,
    MovementCosts,
    MeleeDamageMultiplier
)]
#[require(
    Stamina,
    WeaponStance,
    WeaponAttack,
    FootstepCadence,
    Inventory,
    Equipment
)]
pub struct Player {
    pub(crate) area: Area,
    pub velocity: Vec3,
    pub grounded: bool,
    pub(crate) pace: Pace,
    run_enabled: bool,
    sprint_exhausted: bool,
    jumping: bool,
    movement_direction: Vec2,
}
/// Access to the current player’s actor-owned data in systems.
pub(crate) type PlayerRead<'w, 's, T> = Single<'w, 's, Ref<'static, T>, With<Player>>;
pub(crate) type PlayerWrite<'w, 's, T> = Single<'w, 's, &'static mut T, With<Player>>;

#[cfg(any(test, feature = "smoke"))]
pub(crate) trait PlayerWorldExt {
    fn player_entity(&self) -> Entity;
    #[cfg(test)]
    fn actor_data<T: Component>(&self) -> &T;
    fn actor_data_mut<T: Component<Mutability = bevy::ecs::component::Mutable>>(
        &mut self,
    ) -> Mut<'_, T>;
    #[cfg(test)]
    fn set_player_data(&mut self, data: impl Bundle);
}

#[cfg(any(test, feature = "smoke"))]
impl PlayerWorldExt for World {
    fn player_entity(&self) -> Entity {
        self.try_query_filtered::<Entity, With<Player>>()
            .expect("a player must have been spawned")
            .single(self)
            .expect("this operation requires one player")
    }

    #[cfg(test)]
    fn actor_data<T: Component>(&self) -> &T {
        self.get::<T>(self.player_entity())
            .expect("the component belongs to the player")
    }

    fn actor_data_mut<T: Component<Mutability = bevy::ecs::component::Mutable>>(
        &mut self,
    ) -> Mut<'_, T> {
        let player = self.player_entity();
        self.get_mut::<T>(player)
            .expect("the mutable component belongs to the player")
    }

    #[cfg(test)]
    fn set_player_data(&mut self, data: impl Bundle) {
        let player = self.player_entity();
        self.entity_mut(player).insert(data);
    }
}

impl Player {
    pub(crate) fn respawn(&mut self, dodge: &mut Dodge, transform: &mut Transform, start: Vec3) {
        self.area = Area::Surface;
        transform.translation = start;
        self.velocity = Vec3::ZERO;
        self.jumping = false;
        self.grounded = false;
        self.pace = Pace::Walk;
        self.movement_direction = Vec2::ZERO;
        *dodge = Dodge::default();
    }
}

fn sync_defense(items: Res<Items>, mut actors: Query<(Ref<Equipment>, &mut Defense)>) {
    for (equipment, mut armor) in &mut actors {
        if equipment.is_changed() || armor.is_added() {
            armor.set_if_neq(Defense(equipment.total_defense(&items)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_items;
    use crate::{
        camera::Orbit,
        controls::{Control, Keybinds},
        dialogue::Dialogue,
        world::WorldLayout,
    };
    use crate::{
        combat::{CombatPlugin, Hit},
        inventory::ItemKind,
        weapon::WeaponPlugin,
    };
    use avian3d::prelude::*;
    use bevy::{
        asset::{AssetApp, AssetPlugin},
        mesh::MeshPlugin,
        state::app::StatesPlugin,
        time::TimeUpdateStrategy,
        transform::TransformPlugin,
        window::PrimaryWindow,
    };
    use std::time::Duration;

    const STEP: Duration = Duration::from_micros(15_625);

    fn player_attack(app: &App, player: Entity) -> &WeaponAttack {
        app.world()
            .get::<WeaponAttack>(player)
            .expect("the test player must have its required attack component")
    }

    fn test_app() -> (App, Entity) {
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
        .init_asset::<AudioSource>()
        .insert_state(GameState::Playing)
        .insert_resource(TimeUpdateStrategy::ManualDuration(STEP))
        .insert_resource(WorldLayout::SmokeArena)
        .init_resource::<Dialogue>()
        .init_resource::<Orbit>()
        .init_resource::<MovementInput>()
        .init_resource::<Keybinds>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins((CombatPlugin, WeaponPlugin))
        .add_systems(PreUpdate, input.run_if(in_state(GameState::Playing)))
        .add_systems(FixedUpdate, movement.run_if(in_state(GameState::Playing)))
        .add_systems(Update, footsteps.run_if(in_state(GameState::Playing)))
        .add_systems(Update, face_camera.before(CombatSystems::Detect))
        .add_systems(OnEnter(GameState::Paused), freeze);
        let world = app.world_mut();
        world.spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        world.spawn((
            RigidBody::Static,
            Collider::cuboid(20.0, 0.2, 20.0),
            Transform::from_xyz(0.0, -0.1, 0.0),
        ));
        let player = world
            .spawn((
                Player::default(),
                RigidBody::Kinematic,
                CustomPositionIntegration,
                Collider::capsule(PLAYER_COLLIDER_RADIUS, PLAYER_COLLIDER_LENGTH),
                Transform::from_xyz(0.0, 0.905, 0.0),
            ))
            .id();
        app.finish();
        app.cleanup();
        for _ in 0..3 {
            app.update();
        }
        assert!(
            app.world()
                .get::<Player>(player)
                .is_some_and(|player| player.grounded)
        );
        (app, player)
    }

    fn tick(app: &mut App, count: usize) {
        for _ in 0..count {
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .clear();
        }
    }

    fn press(app: &mut App, keys: &[KeyCode]) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        for &key in keys {
            input.press(key);
        }
    }

    fn release(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
    }

    fn transform(app: &App, player: Entity) -> Transform {
        *app.world()
            .get::<Transform>(player)
            .expect("the movement fixture must have a transform")
    }

    fn active(app: &App, player: Entity) -> bool {
        app.world()
            .get::<Dodge>(player)
            .expect("Player must supply its Dodge component")
            .active()
    }

    fn stamina(app: &App, player: Entity) -> f32 {
        app.world()
            .get::<Stamina>(player)
            .expect("Player must supply its Stamina component")
            .current
    }

    fn equip_spear(app: &mut App) {
        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::FieldSpear, 1));
        assert_eq!(
            app.world_mut().actor_data_mut::<Equipment>().equip(
                &mut bag,
                ItemKind::FieldSpear,
                10,
                test_items()
            ),
            Ok(true)
        );
    }

    #[test]
    fn only_rank_100_can_dodge_or_jump_at_empty_stamina() {
        use crate::attributes::{AttributeId, SavedProgress};

        for rank in [99, 100] {
            for key in [DODGE_KEY, KeyCode::Space] {
                let (mut app, player) = test_app();
                let ranks = AttributeRanks::default().with_rank(AttributeId::Agility, rank);
                let profile = SavedProgress::capture(&ranks, CharacterProgress::default());
                app.world_mut()
                    .entity_mut(player)
                    .insert(profile.player_components());
                app.world_mut()
                    .get_mut::<Stamina>(player)
                    .expect("player must have stamina")
                    .current = 0.0;
                press(&mut app, &[key]);
                tick(&mut app, 1);
                let controller = app
                    .world()
                    .get::<Player>(player)
                    .expect("player controller");
                let moved = if key == DODGE_KEY {
                    active(&app, player)
                } else {
                    controller.jumping
                };
                assert_eq!(moved, rank == 100);
                if moved {
                    assert_eq!(stamina(&app, player), 0.0);
                }
            }
        }
    }

    #[test]
    fn jump_costs_stamina_once_and_recovery_begins_on_landing() {
        let (mut app, player) = test_app();
        press(&mut app, &[KeyCode::Space]);
        tick(&mut app, 1);
        assert_eq!(stamina(&app, player), 80.0);
        release(&mut app, KeyCode::Space);
        press(&mut app, &[KeyCode::Space]);
        tick(&mut app, 32);
        assert_eq!(stamina(&app, player), 80.0);

        for _ in 0..128 {
            tick(&mut app, 1);
            if app
                .world()
                .get::<Player>(player)
                .is_some_and(|p| p.grounded)
            {
                assert_eq!(stamina(&app, player), 80.0 + 25.0 * STEP.as_secs_f32());
                return;
            }
            assert_eq!(stamina(&app, player), 80.0);
        }
        panic!("the player must land within two seconds of jumping");
    }

    #[test]
    fn walking_up_a_ramp_does_not_build_up_jump_velocity() {
        let (mut app, player) = test_app();
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(3.0, 0.2, 2.4),
            Transform::from_xyz(0.0, 0.5, -1.5)
                .with_rotation(Quat::from_rotation_x(30_f32.to_radians())),
        ));
        app.world_mut().resource_mut::<Orbit>().yaw = 0.25;
        tick(&mut app, 2);
        let start = transform(&app, player).translation;

        press(&mut app, &[KeyCode::KeyW]);
        for _ in 0..48 {
            tick(&mut app, 1);
            let velocity = app
                .world()
                .get::<Player>(player)
                .expect("the ramp fixture must have a player")
                .velocity;
            assert!(velocity.y <= 0.0, "ramp contact added upward velocity");
        }
        assert!(transform(&app, player).translation.y > start.y + 0.5);
    }

    #[test]
    fn exhausted_sprint_waits_for_a_new_press_while_walking_recovers_stamina() {
        let (mut app, player) = test_app();
        press(&mut app, &[KeyCode::KeyW, KeyCode::ShiftLeft]);
        tick(&mut app, 64);
        assert_eq!(stamina(&app, player), 86.0);
        app.world_mut()
            .get_mut::<Stamina>(player)
            .expect("Player must supply its Stamina component")
            .current = 0.25;
        tick(&mut app, 2);
        assert_eq!(stamina(&app, player), 0.0);

        press(&mut app, &[KeyCode::Space, DODGE_KEY]);
        tick(&mut app, 16);
        assert_eq!(stamina(&app, player), 3.125);
        let controller = app
            .world()
            .get::<Player>(player)
            .expect("the movement fixture must have a controller");
        assert_eq!(controller.pace, Pace::Walk);
        assert_eq!(controller.velocity.z, -WALK_SPEED);
        assert!(controller.grounded);
        assert!(!active(&app, player));

        release(&mut app, KeyCode::ShiftLeft);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        tick(&mut app, 1);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
        press(&mut app, &[KeyCode::ShiftLeft]);
        tick(&mut app, 1);
        assert_eq!(stamina(&app, player), 3.125 - 14.0 * STEP.as_secs_f32());
        assert!(
            app.world()
                .get::<Player>(player)
                .is_some_and(|p| p.pace == Pace::Sprint)
        );
    }

    #[test]
    fn caps_lock_toggles_the_movement_pace_and_running_holds_stamina_steady() {
        let (mut app, player) = test_app();
        assert!(
            app.world_mut()
                .get_mut::<Stamina>(player)
                .expect("Player must supply its Stamina component")
                .try_spend(50.0)
        );
        let velocity = |app: &App| {
            app.world()
                .get::<Player>(player)
                .expect("the movement fixture must have a controller")
                .velocity
                .xz()
        };
        press(&mut app, &[KeyCode::KeyD]);
        tick(&mut app, 16);
        assert_eq!(velocity(&app), Vec2::X * WALK_SPEED);
        assert_eq!(stamina(&app, player), 53.125);

        press(&mut app, &[KeyCode::CapsLock]);
        tick(&mut app, 16);
        assert_eq!(velocity(&app), Vec2::X * RUN_SPEED);
        assert_eq!(stamina(&app, player), 53.125);
        release(&mut app, KeyCode::KeyD);
        tick(&mut app, 32);
        assert_eq!(stamina(&app, player), 65.625);

        release(&mut app, KeyCode::CapsLock);
        press(&mut app, &[KeyCode::CapsLock, KeyCode::KeyW]);
        tick(&mut app, 16);
        assert_eq!(velocity(&app), Vec2::NEG_Y * WALK_SPEED);
        assert_eq!(stamina(&app, player), 68.75);
    }

    #[test]
    fn movement_uses_the_rebound_forward_key() {
        let (mut app, player) = test_app();
        app.world_mut()
            .resource_mut::<Keybinds>()
            .assign(
                Control::Forward,
                crate::controls::Binding::Key(KeyCode::ArrowUp),
            )
            .expect("the up arrow must be available for forward movement");

        let start = transform(&app, player).translation.z;
        press(&mut app, &[KeyCode::KeyW]);
        tick(&mut app, 3);
        assert_eq!(transform(&app, player).translation.z, start);
        press(&mut app, &[KeyCode::ArrowUp]);
        tick(&mut app, 3);
        assert!(transform(&app, player).translation.z < start - 0.1);
    }

    #[test]
    fn idle_player_faces_camera_yaw_even_without_a_fixed_step() {
        let (mut app, player) = test_app();
        let start = transform(&app, player).translation;
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        let mut orbit = app.world_mut().resource_mut::<Orbit>();
        orbit.yaw = std::f32::consts::FRAC_PI_2;
        orbit.pitch = 0.8;
        tick(&mut app, 1);
        let facing = transform(&app, player);
        assert_eq!(facing.translation, start);
        assert!((facing.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_X, 0.001));
        assert!((facing.rotation * Vec3::Y).abs_diff_eq(Vec3::Y, 0.001));
    }

    #[test]
    fn camera_relative_walking_and_sprint_choose_the_expected_gaits() {
        let (mut app, player) = test_app();
        app.world_mut().resource_mut::<Orbit>().yaw = std::f32::consts::FRAC_PI_2;
        let cases: &[(&[KeyCode], Vec3, Gait, Pace)] = &[
            (&[KeyCode::KeyW], Vec3::NEG_X, Gait::Run, Pace::Sprint),
            (&[KeyCode::KeyA], Vec3::Z, Gait::StrafeLeft, Pace::Walk),
            (&[KeyCode::KeyD], Vec3::NEG_Z, Gait::StrafeRight, Pace::Walk),
            (&[KeyCode::KeyS], Vec3::X, Gait::WalkBack, Pace::Walk),
            (
                &[KeyCode::KeyW, KeyCode::KeyA],
                Vec3::NEG_X + Vec3::Z,
                Gait::WalkForwardLeft,
                Pace::Walk,
            ),
            (
                &[KeyCode::KeyW, KeyCode::KeyD],
                Vec3::NEG_X + Vec3::NEG_Z,
                Gait::WalkForwardRight,
                Pace::Walk,
            ),
            (
                &[KeyCode::KeyS, KeyCode::KeyA],
                Vec3::X + Vec3::Z,
                Gait::WalkBackLeft,
                Pace::Walk,
            ),
            (
                &[KeyCode::KeyS, KeyCode::KeyD],
                Vec3::X + Vec3::NEG_Z,
                Gait::WalkBackRight,
                Pace::Walk,
            ),
        ];
        for &(keys, heading, gait, pace) in cases {
            press(&mut app, &[KeyCode::ShiftLeft]);
            press(&mut app, keys);
            let start = transform(&app, player).translation;
            tick(&mut app, 3);
            let end = transform(&app, player);
            let controller = app
                .world()
                .get::<Player>(player)
                .expect("the movement fixture must have a controller");
            let meters = pace.speed() * STEP.as_secs_f32() * 3.0;
            assert!(
                (end.translation - start)
                    .xz()
                    .abs_diff_eq((heading.normalize() * meters).xz(), 0.003)
            );
            assert!((end.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_X, 0.001));
            assert_eq!(Gait::for_player(controller), gait);
            assert_eq!(controller.pace, pace);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
        }
        assert_eq!(
            Gait::WalkForwardRight.playback_speed(Pace::Sprint),
            SPRINT_SPEED / WALK_SPEED
        );
        assert_eq!(
            Gait::Run.playback_speed(Pace::Sprint),
            SPRINT_SPEED / RUN_SPEED
        );
        assert_eq!(
            Gait::StrafeRight.playback_speed(Pace::Run),
            RUN_SPEED / WALK_SPEED
        );
        assert_eq!(Gait::WalkForwardRight.playback_speed(Pace::Walk), 1.0);
    }

    #[test]
    fn neutral_hop_is_backward_and_held_input_does_not_repeat() {
        let (mut app, player) = test_app();
        app.world_mut().resource_mut::<Orbit>().yaw = std::f32::consts::FRAC_PI_2;
        let start = transform(&app, player).translation;
        press(&mut app, &[DODGE_KEY, KeyCode::Space]);
        tick(&mut app, 10);
        assert_eq!(stamina(&app, player), 75.0);
        let rise = transform(&app, player).translation.y - start.y;
        assert!(
            (0.10..0.18).contains(&rise),
            "dodge must take priority over the higher jump, rise {rise}"
        );
        tick(&mut app, 15);
        let landed = transform(&app, player);
        assert!((1.6..2.05).contains(&(landed.translation.x - start.x)));
        assert!((landed.translation.z - start.z).abs() < 0.02);
        assert!((landed.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_X, 0.001));
        assert!(!active(&app, player));
        tick(&mut app, 45);
        assert!(
            transform(&app, player)
                .translation
                .distance(landed.translation)
                < 0.02
        );
    }

    #[test]
    fn diagonal_hop_locks_camera_relative_heading_and_stops_at_walls() {
        let (mut app, player) = test_app();
        let world = app.world_mut();
        world.resource_mut::<Orbit>().yaw = std::f32::consts::FRAC_PI_2;
        world.spawn((
            RigidBody::Static,
            Collider::cuboid(0.2, 3.0, 8.0),
            Transform::from_xyz(-1.0, 1.0, 0.0),
        ));
        tick(&mut app, 2);
        press(&mut app, &[DODGE_KEY, KeyCode::KeyW, KeyCode::KeyD]);
        tick(&mut app, 2);
        let first = transform(&app, player).translation;
        assert!((first.x - first.z).abs() < 0.01 && first.x < -0.1);
        app.world_mut().resource_mut::<Orbit>().yaw = 0.0;
        release(&mut app, KeyCode::KeyW);
        release(&mut app, KeyCode::KeyD);
        tick(&mut app, 1);
        assert!(active(&app, player));
        assert!((transform(&app, player).rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 0.001));
        tick(&mut app, 23);
        let end = transform(&app, player);
        assert!(
            end.translation.z < -1.0,
            "the hop must keep its initial heading and slide along the wall"
        );
        assert!(
            end.translation.x > -0.63 && end.translation.x < -0.53,
            "wall must stop the player capsule at its face, got {:?}",
            end.translation
        );
    }

    #[test]
    fn airborne_dodge_is_discarded_and_cooldown_blocks_an_early_second_hop() {
        let (mut app, player) = test_app();
        press(&mut app, &[KeyCode::Space]);
        tick(&mut app, 3);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 130);
        assert!(!active(&app, player));
        assert!(transform(&app, player).translation.z.abs() < 0.02);

        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 23);
        assert!(!active(&app, player));
        let landed = transform(&app, player).translation;
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 3);
        assert!(!active(&app, player));
        assert!(transform(&app, player).translation.distance(landed) < 0.02);
        tick(&mut app, 12);
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 1);
        assert!(active(&app, player));
    }

    #[test]
    fn pending_dodge_beats_a_new_thrust_and_an_active_thrust_blocks_dodge() {
        let (mut app, player) = test_app();
        equip_spear(&mut app);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        press(&mut app, &[DODGE_KEY]);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        tick(&mut app, 1);
        assert!(!active(&app, player), "no fixed step has run yet");
        assert!(
            !player_attack(&app, player).active(),
            "a pending dodge must reserve the player's action"
        );
        app.insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
        tick(&mut app, 1);
        assert!(active(&app, player));
        tick(&mut app, 40);
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.release(MouseButton::Left);
        mouse.press(MouseButton::Left);
        tick(&mut app, 1);
        assert!(player_attack(&app, player).active());
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 1);
        assert!(!active(&app, player));
    }

    #[test]
    fn unaffordable_dodge_allows_a_swing_that_holds_stamina_steady() {
        let (mut app, player) = test_app();
        equip_spear(&mut app);
        assert!(
            app.world_mut()
                .get_mut::<Stamina>(player)
                .expect("Player must supply its Stamina component")
                .try_spend(90.0)
        );
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        press(&mut app, &[DODGE_KEY]);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        tick(&mut app, 1);
        assert!(player_attack(&app, player).active());
        assert_eq!(stamina(&app, player), 10.0);

        app.insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
        tick(&mut app, 20);
        assert!(!active(&app, player));
        assert!(player_attack(&app, player).active());
        assert_eq!(stamina(&app, player), 10.0);
        tick(&mut app, 30);
        assert!(!player_attack(&app, player).active());
        assert!(stamina(&app, player) > 10.0);
    }

    #[test]
    fn a_hit_that_connects_during_a_dodge_still_damages_the_player() {
        let (mut app, player) = test_app();
        let attacker = app.world_mut().spawn_empty().id();
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 1);
        app.world_mut().write_message(Hit {
            source: attacker,
            target: player,
            damage: 20,
            contact: None,
        });
        tick(&mut app, 1);
        assert!(active(&app, player));
        let health = app
            .world()
            .get::<Health>(player)
            .expect("Player must supply its Health component");
        assert_eq!(health.current(), health.maximum() - 20);
    }

    #[test]
    fn pause_preserves_a_hop_while_dialogue_and_respawn_cancel_it() {
        let (mut app, player) = test_app();
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 4);
        let spent = stamina(&app, player);
        let position = transform(&app, player).translation;
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
        tick(&mut app, 40);
        assert!(active(&app, player));
        assert_eq!(stamina(&app, player), spent);
        assert_eq!(transform(&app, player).translation, position);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.world_mut()
            .resource_mut::<Dialogue>()
            .show_notice_text("Talking to Nemi");
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 1);
        assert!(!active(&app, player));
        assert_eq!(stamina(&app, player), spent);
        assert_eq!(transform(&app, player).translation.z, position.z);
        app.world_mut().resource_mut::<Dialogue>().clear();
        tick(&mut app, 40);
        release(&mut app, DODGE_KEY);
        press(&mut app, &[DODGE_KEY]);
        tick(&mut app, 2);
        assert!(active(&app, player));
        let spent = stamina(&app, player);

        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("the fixture must have a transform to fall below the map")
            .translation
            .y = -16.0;
        tick(&mut app, 1);
        assert_eq!(stamina(&app, player), spent);
        assert_eq!(
            transform(&app, player).translation,
            WorldLayout::SmokeArena.start()
        );
        let dodge = app
            .world()
            .get::<Dodge>(player)
            .expect("respawn must preserve the Dodge component");
        assert!(!dodge.active() && dodge.ready());
    }
}
