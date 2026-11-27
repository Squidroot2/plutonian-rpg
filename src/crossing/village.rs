use crate::{
    GameState,
    combat::Dead,
    content::NpcId,
    dialogue::Speaker,
    dungeon::Area,
    enemy::{self, GroundMovement},
    nemi::{self, FleeState},
    player::NpcGait,
    terrain::{ground_heading, walkable_height},
    world::SessionEntity,
};
use avian3d::prelude::*;
use bevy::{
    ecs::{schedule::ScheduleConfigs, system::ScheduleSystem},
    prelude::*,
};

const WALK_SPEED_MPS: f32 = 1.95;
const WAYPOINT_RADIUS_M: f32 = 0.16;
const ACTOR_PROBE_Y: f32 = 0.6;
const ACTOR_PROBE_RANGE_M: f32 = 3.0;
const SPEAKER_HEIGHT_M: f32 = 0.95;

#[derive(Clone, Copy)]
pub(crate) struct Doorway {
    pub(crate) outside: Vec2,
    pub(crate) threshold: Vec2,
    width_m: f32,
    depth_m: f32,
}

impl Doorway {
    /// Route a returning resident around the walls and through the opening.
    pub(crate) fn approach(self, from: Vec2, home: Vec2) -> Vec2 {
        const WALL_CLEARANCE_M: f32 = 0.55;
        const DOOR_ALIGNMENT_M: f32 = 0.45;
        let inward = (self.threshold - self.outside).normalize();
        let across = inward.perp();
        let offset = from - self.threshold;
        let depth = offset.dot(inward);
        let side = offset.dot(across);
        let wall = self.width_m * 0.5;
        if depth >= 0.12 && depth < self.depth_m && side.abs() < wall {
            return home;
        }
        let clear_wall = wall + WALL_CLEARANCE_M;
        let around_wall = across * clear_wall.copysign(side);
        if depth >= self.depth_m && side.abs() < clear_wall - 0.05 {
            return self.threshold + inward * depth + around_wall;
        }
        if depth > -WALL_CLEARANCE_M && side.abs() >= wall {
            return self.outside + around_wall;
        }
        if side.abs() > DOOR_ALIGNMENT_M || (from - self.outside).dot(inward) < -0.25 {
            return self.outside;
        }
        home
    }
}

pub(super) struct Residence {
    route: &'static [Vec2],
    delay_seconds: f32,
    pub(super) facing: Vec3,
    pub(super) entrance: Option<Doorway>,
}

impl Residence {
    pub(super) fn home(&self) -> Vec2 {
        *self
            .route
            .last()
            .expect("a residence has a standing position")
    }

    pub(super) fn pose(
        &self,
        pose: Transform,
        spatial: &SpatialQuery,
        player: Option<Entity>,
    ) -> Transform {
        let point = self.home();
        let probe = Vec3::new(point.x, ACTOR_PROBE_Y, point.y);
        let filter = SpatialQueryFilter::from_excluded_entities(player);
        let y = walkable_height(spatial, &filter, probe, ACTOR_PROBE_RANGE_M, 0.0)
            .unwrap_or(pose.translation.y);
        pose.with_translation(probe.with_y(y))
            .with_rotation(ground_heading(self.facing))
    }
}

const FAMILY_DOOR: Doorway = Doorway {
    outside: Vec2::new(-5.1, -1.0),
    threshold: Vec2::new(-6.4, -1.0),
    width_m: 7.2,
    depth_m: 7.2,
};

const WORKSHOP_DOOR: Doorway = Doorway {
    outside: Vec2::new(-8.3, 8.0),
    threshold: Vec2::new(-9.4, 8.0),
    width_m: 6.6,
    depth_m: 7.2,
};

const ISERA: Residence = Residence {
    route: &[
        Vec2::new(-34.8, 22.6),
        Vec2::new(-30.0, 22.6),
        Vec2::new(-29.8, 15.8),
        Vec2::new(-18.0, 15.6),
        Vec2::new(-7.0, 12.6),
        Vec2::new(-5.25, 4.0),
        FAMILY_DOOR.outside,
        FAMILY_DOOR.threshold,
        Vec2::new(-8.15, -1.0),
    ],
    delay_seconds: 1.45,
    facing: Vec3::new(1.0, 0.0, 0.4),
    entrance: Some(FAMILY_DOOR),
};

const VEY: Residence = Residence {
    route: &[
        Vec2::new(-32.7, 15.1),
        Vec2::new(-18.0, 14.9),
        Vec2::new(-7.6, 12.2),
        Vec2::new(-4.65, 3.8),
        Vec2::new(-4.55, -2.65),
        Vec2::new(-5.8, -2.65),
    ],
    delay_seconds: 0.25,
    facing: Vec3::new(0.7, 0.0, 1.0),
    entrance: None,
};

const TOVAN: Residence = Residence {
    route: &[
        Vec2::new(-20.4, 21.8),
        Vec2::new(-17.6, 16.1),
        Vec2::new(-8.6, 13.2),
        WORKSHOP_DOOR.outside,
        WORKSHOP_DOOR.threshold,
        Vec2::new(-11.55, 8.0),
        Vec2::new(-11.70, 7.22),
    ],
    delay_seconds: 1.05,
    facing: Vec3::new(1.0, 0.0, 0.35),
    entrance: Some(WORKSHOP_DOOR),
};

const ORLA: Residence = Residence {
    route: &[
        Vec2::new(-25.5, 14.0),
        Vec2::new(-18.0, 14.1),
        Vec2::new(-8.4, 12.9),
        Vec2::new(0.5, 11.9),
        Vec2::new(6.5, 13.1),
        Vec2::new(7.75, 10.6),
        Vec2::new(8.55, 9.0),
    ],
    delay_seconds: 0.65,
    facing: Vec3::new(-1.0, 0.0, 0.25),
    entrance: None,
};

pub(super) fn residence(id: NpcId) -> &'static Residence {
    match id {
        NpcId::Isera => &ISERA,
        NpcId::Vey => &VEY,
        NpcId::Tovan => &TOVAN,
        NpcId::Orla => &ORLA,
        _ => unreachable!("only caravan villagers have homeward routes"),
    }
}

#[derive(Component)]
pub(crate) struct Homebound {
    residence: &'static Residence,
    next: usize,
    delay_seconds: f32,
}

impl Homebound {
    pub(super) fn new(residence: &'static Residence) -> Self {
        Self {
            residence,
            next: 0,
            delay_seconds: residence.delay_seconds,
        }
    }
}

pub(super) fn movement_systems() -> ScheduleConfigs<ScheduleSystem> {
    (
        walk_home
            .after(super::caravan::settle_villagers)
            .before(enemy::move_ground_enemies),
        animate_homeward
            .after(enemy::move_ground_enemies)
            .before(nemi::move_fleeing_actors),
    )
        .run_if(in_state(GameState::Playing))
        .run_if(enemy::no_dialogue)
}

fn walk_home(
    time: Res<Time<Fixed>>,
    mut residents: Query<
        (
            Entity,
            &NpcId,
            &Transform,
            &mut Homebound,
            &mut GroundMovement,
            &mut FleeState,
        ),
        Without<Dead>,
    >,
    mut commands: Commands,
) {
    for (entity, id, pose, mut walk, mut movement, mut flee) in &mut residents {
        movement.destination = None;
        if !flee.can_talk() {
            continue;
        }
        if walk.delay_seconds > 0.0 {
            walk.delay_seconds -= time.delta_secs();
            continue;
        }

        let route = walk.residence.route;
        while let Some(next) = route.get(walk.next) {
            if pose.translation.xz().distance(*next) > WAYPOINT_RADIUS_M {
                movement.destination = Some(Vec3::new(next.x, 0.0, next.y));
                break;
            }
            walk.next += 1;
        }
        if walk.next == route.len() {
            flee.set_residence(pose.translation, walk.residence.entrance);
            commands
                .entity(entity)
                .remove::<(Homebound, GroundMovement)>()
                .insert((
                    Transform {
                        rotation: ground_heading(walk.residence.facing),
                        ..*pose
                    },
                    speaker(*id),
                    NpcGait::Idle,
                ));
        }
    }
}

fn animate_homeward(
    mut actors: Query<(&GroundMovement, &FleeState, &mut NpcGait), With<Homebound>>,
) {
    for (movement, flee, mut gait) in &mut actors {
        if flee.can_talk() {
            gait.set_if_neq(if movement.moving {
                NpcGait::Walk(movement.speed)
            } else {
                NpcGait::Idle
            });
        }
    }
}

pub(super) fn travel() -> GroundMovement {
    GroundMovement::new(WALK_SPEED_MPS, 0.0)
}

pub(super) fn speaker(id: NpcId) -> Speaker {
    Speaker::new(id.key(), Area::Surface, SPEAKER_HEIGHT_M)
}

pub(super) fn spawn_sample_station(commands: &mut Commands) {
    commands.spawn((
        SessionEntity,
        super::SampleStation,
        Name::new("Tovan’s inspection bench"),
        Transform::from_xyz(-13.35, 0.02, 6.30),
    ));
}

pub(crate) fn light_homes(commands: &mut Commands) {
    for (name, position, amber) in [
        ("Family hearth", Vec3::new(-12.79, 0.45, -1.62), true),
        ("Workshop bench light", Vec3::new(-13.75, 0.70, 6.30), false),
        ("Orla’s hearth", Vec3::new(14.18, 0.36, 10.62), true),
        ("Keeper’s lens light", Vec3::new(5.80, 0.43, 18.34), false),
        ("Barracks hearth", Vec3::new(-11.75, 0.44, -10.70), true),
    ] {
        commands.spawn((
            SessionEntity,
            Name::new(name),
            Transform::from_translation(position),
            PointLight {
                color: if amber {
                    Color::srgb(1.0, 0.57, 0.28)
                } else {
                    Color::srgb(0.43, 0.69, 0.90)
                },
                intensity: 8_000.0,
                range: 6.5,
                ..default()
            },
        ));
    }
    // Ceiling work lights illuminate the aisle and both bunks from above.
    for (name, position) in [
        ("Barracks watch lamp", Vec3::new(-6.85, 1.99, -7.43)),
        ("Barracks bunk lamp", Vec3::new(-10.52, 1.99, -11.49)),
    ] {
        commands.spawn((
            SessionEntity,
            Name::new(name),
            Transform::from_translation(position),
            PointLight {
                color: Color::srgb(1.0, 0.57, 0.28),
                intensity: 16_000.0,
                range: 6.0,
                ..default()
            },
        ));
    }
    commands.spawn((
        SessionEntity,
        Name::new("Village approach lantern"),
        Transform::from_xyz(-11.7, 0.85, 12.6),
        PointLight {
            color: Color::srgb(0.84, 0.51, 0.28),
            intensity: 8_500.0,
            range: 9.0,
            ..default()
        },
    ));
}
