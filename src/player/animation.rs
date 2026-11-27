use super::{
    DODGE_ANIMATION_BLEND, Dodge, LOCOMOTION_ANIMATION_BLEND, MovementInput, Pace, RUN_SPEED,
    WALK_SPEED,
};
use crate::{
    combat_feedback::DeathAnimation,
    content::Items,
    equipment::{
        Equipment, EquipmentSlot, HOLSTER_ROOT_BONE, LEFT_SWORD_GRIP, RIGHT_SWORD_GRIP, SPINE_BONE,
        WeaponKind,
    },
    player::Player,
    weapon::{Hand, WeaponAttack, WeaponStance},
};
use az::Az;
use bevy::{animation::AnimationTargetId, prelude::*, world_serialization::WorldInstanceReady};
use std::time::Duration;

#[derive(Component)]
pub(crate) enum ActorVisual {
    Player,
    Npc,
}

/// The locomotion pose requested by NPC movement or scripted travel.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum NpcGait {
    #[default]
    Idle,
    Walk(f32),
    Run(f32),
}

impl NpcGait {
    fn playback(self) -> (Gait, f32) {
        match self {
            Self::Idle => (Gait::Idle, 1.0),
            Self::Walk(speed) => (Gait::Walk, speed / WALK_SPEED),
            Self::Run(speed) => (Gait::Run, speed / RUN_SPEED),
        }
    }
}

#[derive(Component)]
pub(super) struct AnimatedActor {
    owner: Entity,
    current: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Gait {
    Idle,
    Walk,
    Run,
    Jump,
    WalkBack,
    StrafeLeft,
    StrafeRight,
    WalkForwardLeft,
    WalkForwardRight,
    WalkBackLeft,
    WalkBackRight,
}

impl From<Gait> for usize {
    fn from(gait: Gait) -> Self {
        match gait {
            Gait::Idle => 0,
            Gait::Walk => 1,
            Gait::Run => 2,
            Gait::Jump => 3,
            Gait::WalkBack => 4,
            Gait::StrafeLeft => 5,
            Gait::StrafeRight => 6,
            Gait::WalkForwardLeft => 7,
            Gait::WalkForwardRight => 8,
            Gait::WalkBackLeft => 9,
            Gait::WalkBackRight => 10,
        }
    }
}

impl Gait {
    pub(super) fn for_player(player: &Player) -> Self {
        if !player.grounded {
            return Self::Jump;
        }
        // The normalized keyboard axes are each zero, ±1, or ±1/√2.
        match (
            player.movement_direction.x.round().az::<i8>(),
            player.movement_direction.y.round().az::<i8>(),
        ) {
            (0, 1) if player.pace != Pace::Walk => Self::Run,
            (0, 1) => Self::Walk,
            (0, -1) => Self::WalkBack,
            (-1, 0) => Self::StrafeLeft,
            (1, 0) => Self::StrafeRight,
            (-1, 1) => Self::WalkForwardLeft,
            (1, 1) => Self::WalkForwardRight,
            (-1, -1) => Self::WalkBackLeft,
            (1, -1) => Self::WalkBackRight,
            _ => Self::Idle,
        }
    }

    pub(super) fn playback_speed(self, pace: Pace) -> f32 {
        match self {
            Self::Idle | Self::Jump => 1.0,
            Self::Run => pace.speed() / RUN_SPEED,
            _ => pace.speed() / WALK_SPEED,
        }
    }
}

const SPEAR_LOCOMOTION_START: usize = 11;
const SWORD_LOCOMOTION_START: usize = SPEAR_LOCOMOTION_START * 2;
pub(crate) const SPEAR_THRUST_CLIP: usize = SPEAR_LOCOMOTION_START * 3;
const SWORD_MAIN_SLASH_CLIP: usize = SPEAR_THRUST_CLIP + 1;
const SWORD_OFF_SLASH_CLIP: usize = SWORD_MAIN_SLASH_CLIP + 1;
pub(crate) const SWORD_SLASH_CLIPS: [usize; 2] = [SWORD_MAIN_SLASH_CLIP, SWORD_OFF_SLASH_CLIP];
pub(crate) const ACTOR_DEATH_CLIP: usize = SWORD_OFF_SLASH_CLIP + 1;
pub const CLIPS: [&str; ACTOR_DEATH_CLIP + 1] = [
    "Idle",
    "Walk",
    "Run",
    "Jump",
    "WalkBack",
    "StrafeLeft",
    "StrafeRight",
    "WalkForwardLeft",
    "WalkForwardRight",
    "WalkBackLeft",
    "WalkBackRight",
    "SpearIdle",
    "SpearWalk",
    "SpearRun",
    "SpearJump",
    "SpearWalkBack",
    "SpearStrafeLeft",
    "SpearStrafeRight",
    "SpearWalkForwardLeft",
    "SpearWalkForwardRight",
    "SpearWalkBackLeft",
    "SpearWalkBackRight",
    "SwordIdle",
    "SwordWalk",
    "SwordRun",
    "SwordJump",
    "SwordWalkBack",
    "SwordStrafeLeft",
    "SwordStrafeRight",
    "SwordWalkForwardLeft",
    "SwordWalkForwardRight",
    "SwordWalkBackLeft",
    "SwordWalkBackRight",
    "SpearThrust",
    "SwordMainSlash",
    "SwordOffSlash",
    "PlutonianDeath",
];

/// Directional walks are contiguous within each of the three locomotion sets.
pub(crate) fn directional_walk_clips() -> impl Iterator<Item = &'static str> {
    [0, SPEAR_LOCOMOTION_START, SWORD_LOCOMOTION_START]
        .into_iter()
        .flat_map(|start| {
            &CLIPS[start + usize::from(Gait::WalkBack)..start + SPEAR_LOCOMOTION_START]
        })
        .copied()
}

pub(crate) const SPEAR_LOCOMOTION: std::ops::Range<usize> =
    SPEAR_LOCOMOTION_START..SWORD_LOCOMOTION_START;
/// Upper arm and forearm, in main-hand then off-hand order.
pub(crate) const WEAPON_ARM_BONES: [[&str; 2]; 2] = [["arm-1", "forearm-1"], ["arm1", "forearm1"]];

#[derive(Resource)]
pub struct AnimationLibrary {
    pub graph: Handle<AnimationGraph>,
    pub nodes: [AnimationNodeIndex; CLIPS.len()],
    pub sword_guards: [AnimationNodeIndex; 2],
}

impl AnimationLibrary {}

pub(crate) fn add_sword_animation_masks(
    graph: &mut AnimationGraph,
    nodes: &[AnimationNodeIndex; CLIPS.len()],
    idle: Handle<AnimationClip>,
    clips: &Assets<AnimationClip>,
) -> Result<[AnimationNodeIndex; 2], String> {
    const BODY: u32 = 2;
    let idle_clip = clips
        .get(&idle)
        .ok_or("SwordIdle animation is unavailable")?;
    let arms = std::array::from_fn::<_, 2, _>(|index| {
        let [arm, forearm] = WEAPON_ARM_BONES[index];
        let grip = [RIGHT_SWORD_GRIP, LEFT_SWORD_GRIP][index];
        let bones = [arm, forearm, grip];
        [1, 2, 3].map(|end| {
            ["PlutonianRig", HOLSTER_ROOT_BONE, SPINE_BONE]
                .into_iter()
                .chain(bones[..end].iter().copied())
                .collect::<AnimationTargetId>()
        })
    });
    for (side, targets) in arms.iter().enumerate() {
        for &target in targets {
            if idle_clip.curves_for_target(target).is_none() {
                return Err("SwordIdle is missing an arm or wrist animation target".into());
            }
            graph.add_target_to_mask_group(
                target,
                u32::try_from(side).expect("there are only two animation mask sides"),
            );
        }
    }
    for &target in idle_clip.curves().keys() {
        if !arms
            .iter()
            .flatten()
            .any(|&arm_target| arm_target == target)
        {
            graph.add_target_to_mask_group(target, BODY);
        }
    }
    Ok([0, 1].map(|side| {
        let other_arm = 1 << (1 - side);
        // Both slashes can contribute to the torso, but each controls only its own arm.
        graph
            .get_mut(nodes[SWORD_SLASH_CLIPS[side]])
            .expect("sword slash nodes are registered with the player clips")
            .add_mask(other_arm);
        graph.add_clip_with_mask(idle.clone(), other_arm | (1 << BODY), 1.0, graph.root)
    }))
}
pub(crate) fn bind_actor_animation(
    event: On<WorldInstanceReady>,
    library: Res<AnimationLibrary>,
    visuals: Query<(&ActorVisual, &ChildOf)>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mut commands: Commands,
) {
    let Ok((_, parent)) = visuals.get(event.entity) else {
        return;
    };
    let mut death_players = Vec::new();
    for child in children.iter_descendants(event.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            let mut transitions = AnimationTransitions::new();
            transitions
                .play(
                    &mut player,
                    library.nodes[usize::from(Gait::Idle)],
                    Duration::ZERO,
                )
                .repeat();
            commands
                .entity(child)
                .insert((AnimationGraphHandle(library.graph.clone()), transitions));
            commands.entity(child).insert(AnimatedActor {
                owner: parent.parent(),
                current: Some(usize::from(Gait::Idle)),
            });
            death_players.push(child);
        }
    }
    if !death_players.is_empty() {
        commands.entity(parent.parent()).insert(DeathAnimation {
            players: death_players,
            clip: library.nodes[ACTOR_DEATH_CLIP],
        });
    }
}

pub(super) fn animate(
    items: Res<Items>,
    library: Res<AnimationLibrary>,
    actors: Query<
        (
            &Equipment,
            &WeaponStance,
            &WeaponAttack,
            Option<(&Player, &Dodge)>,
            Option<&NpcGait>,
        ),
        Without<crate::combat::Dead>,
    >,
    mut animations: Query<(
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        &mut AnimatedActor,
    )>,
) {
    for (mut animation, mut transitions, mut actor) in &mut animations {
        let Ok((equipment, stance, attack, player, npc_gait)) = actors.get(actor.owner) else {
            continue;
        };
        let (gait, speed, dodging) = if let Some((player, dodge)) = player {
            let gait = Gait::for_player(player);
            (gait, gait.playback_speed(player.pace), dodge.active())
        } else {
            let (gait, speed) = npc_gait.copied().unwrap_or_default().playback();
            (gait, speed, false)
        };
        let sword_times = Hand::ALL.map(|hand| {
            attack
                .current(hand)
                .filter(|swing| swing.weapon == WeaponKind::Shortsword)
                .map(|swing| swing.elapsed)
        });
        let swinging_swords = sword_times.iter().any(Option::is_some);
        let thrusting = attack
            .current(Hand::Main)
            .is_some_and(|swing| swing.weapon == WeaponKind::Spear);
        let carry = if *stance == WeaponStance::Drawn {
            equipment
                .weapon(EquipmentSlot::MainHand, &items)
                .or_else(|| equipment.weapon(EquipmentSlot::OffHand, &items))
        } else {
            None
        };
        let start = match carry {
            Some(WeaponKind::Spear) => SPEAR_LOCOMOTION_START,
            Some(WeaponKind::Shortsword) => SWORD_LOCOMOTION_START,
            None => 0,
        };
        let index = if thrusting {
            SPEAR_THRUST_CLIP
        } else {
            start + usize::from(gait)
        };
        if swinging_swords {
            if actor.current.take().is_some() {
                animation.stop_all();
                *transitions = AnimationTransitions::new();
            }
            for (side, elapsed) in sword_times.into_iter().enumerate() {
                let slash = library.nodes[SWORD_SLASH_CLIPS[side]];
                let guard = library.sword_guards[side];
                let (node, previous) = if elapsed.is_some() {
                    (slash, guard)
                } else {
                    (guard, slash)
                };
                // Sample both arms at their own combat time, including the frame a swing starts.
                animation
                    .stop(previous)
                    .play(node)
                    .pause()
                    .set_seek_time(elapsed.unwrap_or(0.0));
            }
            continue;
        }
        if actor.current != Some(index) {
            if actor.current.is_none() {
                animation.stop_all();
            }
            if thrusting {
                transitions
                    .play(&mut animation, library.nodes[index], Duration::ZERO)
                    .replay();
            } else {
                let changes_grip = actor.current.is_none_or(|current| {
                    current / SPEAR_LOCOMOTION_START != index / SPEAR_LOCOMOTION_START
                });
                let blend = if changes_grip {
                    Duration::ZERO
                } else if dodging {
                    DODGE_ANIMATION_BLEND
                } else {
                    LOCOMOTION_ANIMATION_BLEND
                };
                transitions
                    .play(&mut animation, library.nodes[index], blend)
                    .repeat();
            }
            actor.current = Some(index);
        }
        if !thrusting {
            if let Some(clip) = animation.animation_mut(library.nodes[index])
                && clip.speed() != speed
            {
                clip.set_speed(speed);
            }
        }
    }
}

pub(super) fn freeze(mut players: Query<&mut AnimationPlayer>, mut input: ResMut<MovementInput>) {
    *input = MovementInput::default();
    for mut player in &mut players {
        player.pause_all();
    }
}
pub(super) fn resume(mut players: Query<&mut AnimationPlayer>) {
    for mut player in &mut players {
        player.resume_all();
    }
}
