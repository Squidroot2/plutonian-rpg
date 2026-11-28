//! Musket export contracts used before the ranged item and playback are wired in.

use super::{Failures, Model, PoseTracks, check_duration, parents, require_node};
use crate::{
    asset_check::{
        expectations::{HAND_WRAP_TOLERANCE_M, POSITION_TOLERANCE_M},
        inventory::{check_equipment_pair, check_pickup_bounds},
    },
    equipment::{LEFT_SWORD_GRIP, RIGHT_SWORD_GRIP, SPINE_BONE},
    loading::CHARACTER_MODEL,
    player::WEAPON_ARM_BONES,
    weapon::ATTACK_CLIP_FPS,
};
use bevy::math::{Mat4, Vec3};
use std::collections::BTreeMap;

const PICKUP: &str = "models/methane_musket.glb";
const WORN: &str = "models/methane_musket_worn.glb";
const SLUG: &str = "models/methane_slug.glb";
const FLASH: &str = "models/methane_muzzle_flash.glb";
const GRIP: &str = "musket_grip";
const HOLSTER: &str = "musket_holster";
const MUZZLE: Vec3 = Vec3::new(0.0, 0.165, 1.06);
const PICKUP_CLEARANCE_M: Vec3 = Vec3::new(0.22, 0.38, 1.46);
/// Gap between the jaw blocks authored by `scripts/village.py`.
const RACK_GRIP_OPENING_M: f32 = 0.078;

const FIRE: &str = "MusketFire";
const RELOAD: &str = "MusketReload";
const SHOT_FRAME: u16 = 1;
const FIRE_END: u16 = 12;
const RELOAD_LEAVE: u16 = 8;
const RELOAD_REGRIP: u16 = 54;
const RELOAD_END: u16 = 60;
pub(super) const CLIPS: [(&str, u16); 4] = [
    ("MusketIdle", 48),
    ("MusketAim", 24),
    (FIRE, FIRE_END),
    (RELOAD, RELOAD_END),
];

const FLASH_CLIP: &str = "MethaneMuzzleFlash";
const FLASH_END_S: f32 = 10.0 / ATTACK_CLIP_FPS;

pub(super) fn check(character: &Model, models: &BTreeMap<String, Model>, failures: &mut Failures) {
    for (name, frames) in CLIPS {
        check_duration(
            CHARACTER_MODEL,
            character,
            name,
            f32::from(frames) / ATTACK_CLIP_FPS,
            failures,
        );
    }
    if let Some(slug) = models.get(SLUG) {
        require_node(slug, SLUG, "Slug", failures);
        require_node(slug, SLUG, "Tapered wake", failures);
    }
    if let Some(flash) = models.get(FLASH) {
        let [ignition, smoke] =
            ["Ignition", "Smoke"].map(|node| require_node(flash, FLASH, node, failures));
        let animated = |node| {
            flash.animation(FLASH_CLIP).is_some_and(|clip| {
                clip.tracks.iter().any(|track| {
                    Some(track.node) == node
                        && track.values.windows(2).any(|pair| pair[0] != pair[1])
                })
            })
        };
        failures.require(
            flash.animations.len() == 1 && animated(ignition) && animated(smoke),
            FLASH,
            "expected one MethaneMuzzleFlash clip animating both ignition and smoke",
        );
        check_duration(FLASH, flash, FLASH_CLIP, FLASH_END_S, failures);
    }
    if let Some(pickup) = models.get(PICKUP) {
        check_pickup_bounds(
            PICKUP,
            pickup,
            PICKUP_CLEARANCE_M,
            POSITION_TOLERANCE_M,
            failures,
        );
        if let Some(worn) = models.get(WORN) {
            check_equipment_pair((PICKUP, pickup), (WORN, worn), failures);
        }
    }
    if let Some(worn) = models.get(WORN) {
        check_worn(character, worn, failures);
    }
}

fn check_worn(character: &Model, worn: &Model, failures: &mut Failures) {
    let [rear, support, muzzle] = ["Trigger grip", "Forward hand wrap", "MusketMuzzle"]
        .map(|name| require_node(worn, WORN, name, failures));
    if let Some(muzzle) = muzzle {
        let aim = worn.nodes[muzzle].transform.matrix();
        failures.require(
            aim.abs_diff_eq(Mat4::from_translation(MUZZLE), POSITION_TOLERANCE_M),
            WORN,
            "MusketMuzzle must sit on the bore with +Z pointing downrange",
        );
    }
    let (Some(rear), Some(support)) = (rear, support) else {
        return;
    };
    let rear_node = &worn.nodes[rear];
    let wraps = [rear_node, &worn.nodes[support]].map(|node| node.transform.translation());
    failures.require(
        wraps[0].length() < POSITION_TOLERANCE_M,
        WORN,
        "trigger grip must be centered on the grip origin",
    );
    failures.require(
        rear_node.mesh.is_some_and(|mesh| {
            worn.meshes[mesh]
                .iter()
                .all(|bounds| bounds.max[0] - bounds.min[0] <= RACK_GRIP_OPENING_M)
        }),
        WORN,
        "trigger grip must fit between the Barracks rack jaws",
    );
    check_grip(character, wraps, failures);
}

fn check_grip(character: &Model, wraps: [Vec3; 2], failures: &mut Failures) {
    let Some(grip) = require_node(character, CHARACTER_MODEL, GRIP, failures) else {
        return;
    };
    let spine = require_node(character, CHARACTER_MODEL, SPINE_BONE, failures);
    let parents = parents(character, failures);
    let holster = require_node(character, CHARACTER_MODEL, HOLSTER, failures);
    for (name, index) in [(GRIP, Some(grip)), (HOLSTER, holster)] {
        if let Some(index) = index {
            failures.require(
                spine.is_some() && parents[index] == spine,
                CHARACTER_MODEL,
                format!("{name} must follow the spine"),
            );
        }
    }
    if let Some(holster) = holster {
        let direction = character.nodes[holster]
            .transform
            .matrix()
            .z_axis
            .truncate();
        failures.require(
            direction.abs_diff_eq(Vec3::new(0.62, 0.78, 0.0).normalize(), POSITION_TOLERANCE_M),
            CHARACTER_MODEL,
            "musket holster must point across the back in the spine plane",
        );
    }
    let hands = std::array::from_fn(|side| {
        let [arm, forearm] = WEAPON_ARM_BONES[side];
        let wrist = [RIGHT_SWORD_GRIP, LEFT_SWORD_GRIP][side];
        Some([
            require_node(character, CHARACTER_MODEL, arm, failures)?,
            require_node(character, CHARACTER_MODEL, forearm, failures)?,
            require_node(character, CHARACTER_MODEL, wrist, failures)?,
        ])
    });
    for (name, last) in CLIPS {
        let Some(clip) = character.animation(name) else {
            continue;
        };
        let poses = PoseTracks::new(clip, failures);
        for frame in 0..=last {
            if let Err(error) = check_grip_frame(character, grip, &hands, wraps, &poses, frame) {
                failures.add(CHARACTER_MODEL, error);
            }
        }
    }
}

fn check_grip_frame(
    character: &Model,
    grip: usize,
    hands: &[Option<[usize; 3]>; 2],
    wraps: [Vec3; 2],
    poses: &PoseTracks,
    frame: u16,
) -> Result<(), String> {
    let sample = |node| poses.local(character, node, u32::from(frame));
    let mount = sample(grip)?;
    let name = poses.name;
    if name == FIRE
        && frame == SHOT_FRAME
        && !mount
            .z_axis
            .truncate()
            .abs_diff_eq(Vec3::Z, POSITION_TOLERANCE_M)
    {
        return Err(format!(
            "{name} frame {frame}: muzzle must point straight and level"
        ));
    }
    for (side, (hand, wrap)) in hands.iter().zip(wraps).enumerate() {
        let Some([arm, forearm, wrist]) = *hand else {
            continue;
        };
        // The support hand is free to leave the fore-end during the transfer.
        if side == 1 && name == RELOAD && (RELOAD_LEAVE + 1..RELOAD_REGRIP).contains(&frame) {
            continue;
        }
        let hand = sample(arm)? * sample(forearm)? * sample(wrist)?;
        if hand
            .w_axis
            .truncate()
            .distance(mount.transform_point3(wrap))
            > HAND_WRAP_TOLERANCE_M
        {
            return Err(format!("{name} frame {frame}: hand {side} lost the grip"));
        }
    }
    Ok(())
}
