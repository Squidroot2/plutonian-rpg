use super::{Player, PlayerRead};
use crate::{
    attributes::{AttributeRanks, CarryLimits, MovementCosts},
    camera::Orbit,
    content::Items,
    controls::{Control, Keybinds},
    dialogue::Dialogue,
    equipment::Equipment,
    inventory::Inventory,
    ui::menu_ui::PlayerMenu,
    weapon::{WeaponAttack, WeaponStance},
    world::WorldLayout,
};
use avian3d::prelude::*;
use bevy::prelude::*;
use std::time::Duration;

impl Player {
    fn launch(&mut self, speed: f32) {
        self.velocity.y = speed;
        self.jumping = true;
        self.grounded = false;
    }

    /// Update locomotion state and return the intended horizontal velocity and gravity.
    fn travel_velocity(
        &mut self,
        dodge: &Dodge,
        stamina: &mut Stamina,
        input: &MovementInput,
        can_move: bool,
        agile: bool,
        yaw: f32,
        delta: f32,
    ) -> (Vec3, f32) {
        if let Some(hop) = &dodge.hop {
            self.pace = Pace::Walk;
            self.movement_direction = Vec2::ZERO;
            return (hop.heading * DODGE_SPEED, DODGE_GRAVITY);
        }

        let direction = if can_move {
            input.direction
        } else {
            Vec2::ZERO
        };
        self.movement_direction = direction;
        self.pace = if agile && self.run_enabled && direction != Vec2::ZERO {
            Pace::Run
        } else {
            Pace::Walk
        };
        if agile
            && input.sprint
            && direction.y > 0.0
            && direction.x == 0.0
            && !self.sprint_exhausted
        {
            if !stamina.is_empty() {
                self.pace = Pace::Sprint;
                stamina.drain_sprint(delta);
            }
            self.sprint_exhausted = stamina.is_empty();
        }
        (camera_heading(direction, yaw) * self.pace.speed(), GRAVITY)
    }
}

pub(crate) const PLAYER_COLLIDER_RADIUS: f32 = 0.32;
pub(crate) const PLAYER_COLLIDER_LENGTH: f32 = 1.15;
pub(crate) const PLAYER_GROUND_OFFSET: f32 = 0.905;
pub(super) const PLAYER_HIT_PROTECTION_SECONDS: f32 = 0.75;
pub(crate) const DODGE_KEY: KeyCode = KeyCode::ControlLeft;
const DODGE_LAUNCH_SPEED: f32 = 1.7;
const DODGE_GRAVITY: f32 = 10.0;
const DODGE_SPEED: f32 = 6.0;
const DODGE_DURATION: f32 = 0.32;
const DODGE_COOLDOWN: f32 = 0.55;
pub(super) const WALK_SPEED: f32 = 2.5;
const WALK_FOOTSTEP_SECONDS: f32 = 0.5;
const RUN_FOOTSTEP_SECONDS: f32 = 0.4;
const SPRINT_FOOTSTEP_SECONDS: f32 = 0.32;
const FOOTSTEP_VOLUME: f32 = 0.2;
const JUMP_SPEED: f32 = 4.7;
const GRAVITY: f32 = 5.0;
const MAX_FALL_SPEED: f32 = 22.0;
const GROUND_PROBE_RADIUS: f32 = 0.30;
const GROUND_PROBE_DISTANCE: f32 = 0.12;
const DODGE_GROUND_PROBE_DISTANCE: f32 = 0.025;
const GROUND_NORMAL_MIN_Y: f32 = 0.7;
const GROUNDING_MAX_UPWARD_SPEED: f32 = 0.1;
const GROUND_STICK_SPEED: f32 = 0.6;
const WALK_STAMINA_RECOVERY_FACTOR: f32 = 0.5;
const WORLD_MIN_Y: f32 = -15.0;
const WORLD_MIN_Z: f32 = -140.0;
pub(super) const DODGE_ANIMATION_BLEND: Duration = Duration::from_millis(50);
pub(super) const LOCOMOTION_ANIMATION_BLEND: Duration = Duration::from_millis(160);
pub(super) const RUN_SPEED: f32 = 4.0;
pub(super) const SPRINT_SPEED: f32 = 8.0;
const SPRINT_STAMINA_PER_SECOND: f32 = 14.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pace {
    #[default]
    Walk,
    Run,
    Sprint,
}

impl Pace {
    pub(crate) fn speed(self) -> f32 {
        match self {
            Self::Walk => WALK_SPEED,
            Self::Run => RUN_SPEED,
            Self::Sprint => SPRINT_SPEED,
        }
    }
}

#[derive(Component)]
pub(crate) struct Stamina {
    pub(super) current: f32,
    maximum: f32,
    recovery_per_second: f32,
}

impl Default for Stamina {
    fn default() -> Self {
        let effects = AttributeRanks::default().effects();
        Self::new(effects.stamina, effects.stamina_per_second)
    }
}

impl Stamina {
    pub(crate) fn new(maximum: f32, recovery_per_second: f32) -> Self {
        Self {
            current: maximum,
            maximum,
            recovery_per_second,
        }
    }

    pub(crate) fn tuning(&self) -> (f32, f32) {
        (self.maximum, self.recovery_per_second)
    }

    pub(crate) fn set_tuning(&mut self, maximum: f32, recovery_per_second: f32) {
        self.maximum = maximum;
        self.recovery_per_second = recovery_per_second;
        self.current = self.current.min(maximum);
    }

    pub(crate) fn fraction(&self) -> f32 {
        self.current / self.maximum
    }

    fn can_spend(&self, cost: f32) -> bool {
        self.current >= cost
    }

    pub(crate) fn try_spend(&mut self, cost: f32) -> bool {
        if self.can_spend(cost) {
            self.current -= cost;
            true
        } else {
            false
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.current == 0.0
    }

    pub(super) fn drain_sprint(&mut self, delta: f32) {
        self.current = (self.current - SPRINT_STAMINA_PER_SECOND * delta).max(0.0);
    }

    pub(crate) fn recover(&mut self, delta: f32) {
        if self.current < self.maximum {
            self.current = (self.current + self.recovery_per_second * delta).min(self.maximum);
        }
    }
}

#[derive(Component, Default)]
pub(crate) struct Dodge {
    hop: Option<DodgeHop>,
    cooldown: f32,
}

struct DodgeHop {
    heading: Vec3,
    remaining: f32,
}

impl Dodge {
    pub(crate) fn active(&self) -> bool {
        self.hop.is_some()
    }

    pub(super) fn ready(&self) -> bool {
        self.cooldown <= 0.0 && !self.active()
    }

    fn start(&mut self, heading: Vec3) {
        self.hop = Some(DodgeHop {
            heading,
            remaining: DODGE_DURATION,
        });
        self.cooldown = DODGE_COOLDOWN;
    }

    fn advance(&mut self, delta: f32) {
        if let Some(hop) = &mut self.hop {
            hop.remaining -= delta;
            if hop.remaining <= 0.0 {
                self.hop = None;
            }
        }
    }
}

#[derive(Component, Default, PartialEq)]
pub(super) struct FootstepCadence(f32);

pub(super) fn footsteps(
    time: Res<Time>,
    assets: Res<AssetServer>,
    mut players: Query<(&Player, &mut FootstepCadence)>,
    mut commands: Commands,
) {
    for (player, mut elapsed) in &mut players {
        if !player.grounded || player.movement_direction == Vec2::ZERO {
            elapsed.set_if_neq(FootstepCadence(0.0));
            continue;
        }
        elapsed.0 -= time.delta_secs();
        if elapsed.0 <= 0.0 {
            commands.spawn((
                AudioPlayer::new(assets.load("sounds/footstep05.ogg")),
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(FOOTSTEP_VOLUME)),
            ));
            elapsed.0 = match player.pace {
                Pace::Walk => WALK_FOOTSTEP_SECONDS,
                Pace::Run => RUN_FOOTSTEP_SECONDS,
                Pace::Sprint => SPRINT_FOOTSTEP_SECONDS,
            };
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct MovementInput {
    pub(super) direction: Vec2,
    pub(super) sprint: bool,
    jump: bool,
    dodge: Option<Vec2>,
}

impl MovementInput {
    // Frame-based attacks can run before the next fixed step consumes the request.
    pub(crate) fn dodge_pending(
        &self,
        grounded: bool,
        dodge: &Dodge,
        stamina: &Stamina,
        cost: f32,
    ) -> bool {
        self.dodge.is_some() && grounded && dodge.ready() && stamina.can_spend(cost)
    }
}

pub(crate) fn input(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    dialogue: Res<Dialogue>,
    mut input: ResMut<MovementInput>,
    mut players: Query<&mut Player>,
    log: Option<Res<PlayerMenu>>,
) {
    input.direction = Vec2::ZERO;
    input.sprint = bindings.pressed(Control::Sprint, &keys);
    let blocked = dialogue.is_open()
        || log.is_some_and(|log| log.is_open())
        || keys.just_pressed(KeyCode::Escape);
    let toggle_run = !blocked && bindings.just_pressed(Control::ToggleRun, &keys);
    for mut player in &mut players {
        if toggle_run {
            player.run_enabled = !player.run_enabled;
        }
        if !input.sprint && player.sprint_exhausted {
            player.sprint_exhausted = false;
        }
    }
    if blocked {
        input.jump = false;
        input.dodge = None;
        return;
    }
    input.direction = Vec2::new(
        bindings.pressed(Control::Right, &keys) as i8 as f32
            - bindings.pressed(Control::Left, &keys) as i8 as f32,
        bindings.pressed(Control::Forward, &keys) as i8 as f32
            - bindings.pressed(Control::Backward, &keys) as i8 as f32,
    )
    .normalize_or_zero();
    input.jump |= bindings.just_pressed(Control::Jump, &keys);
    if bindings.just_pressed(Control::Dodge, &keys) {
        input.dodge = Some(input.direction);
    }
}

fn camera_heading(direction: Vec2, yaw: f32) -> Vec3 {
    let (sin, cos) = yaw.sin_cos();
    let forward = Vec3::new(-sin, 0.0, -cos);
    let right = Vec3::new(cos, 0.0, -sin);
    forward * direction.y + right * direction.x
}

pub(crate) fn face_camera(
    orbit: Res<Orbit>,
    mut players: Query<(&mut Transform, &WeaponStance, &Player)>,
) {
    // Drawn weapons lock the model to the camera, so combat always faces the camera.
    // Holstered weapons free the camera: the model keeps its facing while idle and
    // turns toward its camera-relative travel direction while moving.
    let drawn_facing = Quat::from_rotation_y(orbit.yaw + std::f32::consts::PI);
    for (mut transform, stance, player) in &mut players {
        let facing = match stance {
            WeaponStance::Drawn => Some(drawn_facing),
            WeaponStance::Holstered => {
                let heading = camera_heading(player.movement_direction, orbit.yaw);
                if heading.length_squared() > 0.0 {
                    Some(Quat::from_rotation_y(heading.x.atan2(heading.z)))
                } else {
                    None
                }
            }
        };
        if let Some(facing) = facing
            && transform.rotation != facing
        {
            transform.rotation = facing;
        }
    }
}

pub(crate) fn movement(
    items: Res<Items>,
    time: Res<Time>,
    orbit: Res<Orbit>,
    dialogue: Res<Dialogue>,
    layout: Res<WorldLayout>,
    inventory: PlayerRead<Inventory>,
    equipment: PlayerRead<Equipment>,
    mut input: ResMut<MovementInput>,
    spatial: SpatialQuery,
    mover: MoveAndSlide,
    mut players: Query<(
        Entity,
        &mut Transform,
        &Collider,
        &mut Player,
        &mut Dodge,
        &mut Stamina,
        &WeaponAttack,
        &MovementCosts,
        &CarryLimits,
    )>,
) {
    let dodge_request = input.dodge.take();
    let jump_requested = std::mem::take(&mut input.jump);
    let delta = time.delta_secs();
    let weight = inventory.carried_weight(&equipment, &items);
    for (
        entity,
        mut transform,
        collider,
        mut player,
        mut dodge,
        mut stamina,
        attack,
        costs,
        limits,
    ) in &mut players
    {
        let can_act = !dialogue.is_open() && !attack.active();
        let agile = weight <= limits.capacity_kg;
        let can_move = weight <= limits.movement_limit_kg && can_act;
        dodge.cooldown = (dodge.cooldown - delta).max(0.0);
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        // Let the hop descend almost to the surface before ending its motion.
        let ground_distance = if dodge.active() {
            DODGE_GROUND_PROBE_DISTANCE
        } else {
            GROUND_PROBE_DISTANCE
        };
        let ground = spatial.cast_shape(
            &Collider::capsule(GROUND_PROBE_RADIUS, PLAYER_COLLIDER_LENGTH),
            transform.translation,
            Quat::IDENTITY,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(ground_distance),
            &filter,
        );
        player.grounded = (!player.jumping || player.velocity.y <= GROUNDING_MAX_UPWARD_SPEED)
            && ground.is_some_and(|h| h.normal1.y > GROUND_NORMAL_MIN_Y);
        if player.grounded || !can_move || !agile {
            dodge.hop = None;
        }
        if player.grounded {
            player.jumping = false;
            player.velocity.y = -GROUND_STICK_SPEED;
        }
        if can_move && agile && player.grounded {
            if let Some(direction) =
                dodge_request.filter(|_| dodge.ready() && stamina.try_spend(costs.dodge))
            {
                let heading = if direction == Vec2::ZERO {
                    camera_heading(Vec2::NEG_Y, orbit.yaw)
                } else {
                    camera_heading(direction, orbit.yaw)
                };
                dodge.start(heading);
                player.launch(DODGE_LAUNCH_SPEED);
            } else if jump_requested && stamina.try_spend(costs.jump) {
                player.launch(JUMP_SPEED);
            }
        }
        let (horizontal, gravity) = player.travel_velocity(
            &dodge,
            &mut stamina,
            &input,
            can_move,
            agile,
            orbit.yaw,
            delta,
        );
        player.velocity.x = horizontal.x;
        player.velocity.z = horizontal.z;
        player.velocity.y = (player.velocity.y - gravity * delta).max(-MAX_FALL_SPEED);
        let velocity = player.velocity;
        let output = mover.move_and_slide(
            collider,
            transform.translation,
            Quat::IDENTITY,
            velocity,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &filter,
            |hit| {
                // Suppress upward motion introduced by pushing against steep surfaces.
                if hit.normal.y < GROUND_NORMAL_MIN_Y && hit.velocity.y > velocity.y.max(0.0) {
                    hit.velocity.y = velocity.y.max(0.0);
                }
                MoveAndSlideHitResponse::Accept
            },
        );
        transform.translation = output.position;
        // Follow the slope this tick without carrying its upward motion as jump velocity.
        player.velocity.y = output.projected_velocity.y.min(velocity.y.max(0.0));
        dodge.advance(delta);
        if transform.translation.y < WORLD_MIN_Y
            || !layout.recovery_x().contains(&transform.translation.x)
            || transform.translation.z > layout.recovery_max_z()
            || transform.translation.z < WORLD_MIN_Z
        {
            player.respawn(&mut dodge, &mut transform, layout.start());
        } else if can_act && player.grounded && player.pace == Pace::Walk {
            let recovery_time = if player.movement_direction == Vec2::ZERO {
                delta
            } else {
                delta * WALK_STAMINA_RECOVERY_FACTOR
            };
            stamina.recover(recovery_time);
        }
    }
}
