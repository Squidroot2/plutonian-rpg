use super::{Failures, Model, PoseTracks, Property, attack_frames, require_node, socket};
use crate::{
    asset_check::{expectations::*, geometry::world_bounds},
    equipment::{EquipmentSlot, HOLSTER_ROOT_BONE, Handedness, SPINE_BONE, WeaponKind},
    loading::CHARACTER_MODEL,
    player::{CLIPS, SPEAR_LOCOMOTION, SPEAR_THRUST_CLIP, WEAPON_ARM_BONES},
};
use bevy::math::Vec3;
use std::collections::BTreeMap;

type Worn<'a> = (&'a str, &'a Model);

pub(super) fn check(
    character: &Model,
    spear: Option<Worn>,
    trident: Option<Worn>,
    failures: &mut Failures,
) {
    let spear_wraps = wraps(spear, failures);
    let trident_wraps = wraps(trident, failures);
    if let (Some((spear_path, spear)), Some((trident_path, trident))) = (spear, trident) {
        for ((spear, trident), hand) in spear_wraps.into_iter().zip(trident_wraps).zip(SPEAR_HANDS)
        {
            if let (Some(spear), Some(trident)) = (spear, trident) {
                failures.require(
                    spear.distance(trident) < POSITION_TOLERANCE_M,
                    trident_path,
                    format!("{:?} grip point differs from {spear_path}", hand.wrap),
                );
            }
        }
        let spear = world_bounds(spear).inspect_err(|error| failures.add(spear_path, error));
        let trident = world_bounds(trident).inspect_err(|error| failures.add(trident_path, error));
        if let (Ok(spear), Ok(trident)) = (spear, trident) {
            failures.require(
                (trident.max[2] - spear.max[2]).abs() < POSITION_TOLERANCE_M,
                trident_path,
                format!("tip must have the same forward reach as {spear_path}"),
            );
        }
    }

    let grip_name = socket(EquipmentSlot::MainHand, Handedness::TwoHanded);
    let Some(grip) = character.node_index(grip_name) else {
        return;
    };
    let root = character.node_index(HOLSTER_ROOT_BONE);
    let spine = require_node(character, CHARACTER_MODEL, SPINE_BONE, failures);
    let lead = require_node(character, CHARACTER_MODEL, LEGS[1], failures);
    let hands = std::array::from_fn(|index| {
        let [name, forearm] = WEAPON_ARM_BONES[index];
        let arm = require_node(character, CHARACTER_MODEL, name, failures)?;
        let forearm = require_node(character, CHARACTER_MODEL, forearm, failures)?;
        let rest =
            character.nodes[arm].transform.matrix() * character.nodes[forearm].transform.matrix();
        let Some(inverse) = rest.try_inverse() else {
            failures.add(
                CHARACTER_MODEL,
                format!("{name}: rest pose is not invertible"),
            );
            return None;
        };
        Some(Hand {
            arm,
            forearm,
            local_center: inverse.transform_point3(SPEAR_HANDS[index].rest_center.into()),
            name,
        })
    });
    let (Some(root), Some(spine)) = (root, spine) else {
        return;
    };
    let rig = SpearRig {
        model: character,
        grip,
        root,
        spine,
        lead,
        hands,
        wraps: spear_wraps,
    };
    for index in SPEAR_LOCOMOTION.chain(std::iter::once(SPEAR_THRUST_CLIP)) {
        if let Some(animation) = character.animation(CLIPS[index]) {
            let poses = PoseTracks::new(animation, failures);
            rig.check_clip(
                &poses,
                index == SPEAR_THRUST_CLIP,
                spear.map(|(path, _)| path),
                failures,
            );
        }
    }
}

fn wraps(worn: Option<Worn>, failures: &mut Failures) -> [Option<Vec3>; 2] {
    let Some((path, model)) = worn else {
        return [None; 2];
    };
    SPEAR_HANDS.map(|hand| {
        let index = require_node(model, path, hand.wrap, failures)?;
        Some(model.nodes[index].transform.translation())
    })
}

#[derive(Clone, Copy)]
struct Hand {
    arm: usize,
    forearm: usize,
    local_center: Vec3,
    name: &'static str,
}

struct SpearRig<'a> {
    model: &'a Model,
    grip: usize,
    root: usize,
    spine: usize,
    lead: Option<usize>,
    hands: [Option<Hand>; 2],
    wraps: [Option<Vec3>; 2],
}

impl SpearRig<'_> {
    fn check_clip(
        &self,
        poses: &PoseTracks,
        is_thrust: bool,
        weapon: Option<&str>,
        failures: &mut Failures,
    ) {
        let Some(frames) = poses
            .tracks
            .get(&(self.grip, Property::Translation))
            .filter(|frames| !frames.is_empty())
        else {
            failures.add(
                CHARACTER_MODEL,
                format!("{}: expected grip translation samples", poses.name),
            );
            return;
        };
        let (impact, recovery) = attack_frames(WeaponKind::Spear);
        let mut travel = BTreeMap::new();
        for &frame in frames.keys() {
            let world_grip = self.check_frame(poses, frame, is_thrust, impact, failures);
            if let Ok(position) =
                world_grip.inspect_err(|error| failures.add(CHARACTER_MODEL, error))
                && is_thrust
            {
                travel.insert(frame, position);
            }
            for (hand, wrap) in self.hands.into_iter().zip(self.wraps) {
                if let Some(hand) = hand
                    && let Err(error) = self.check_hand(poses, frame, hand, wrap, weapon, failures)
                {
                    failures.add(CHARACTER_MODEL, error);
                }
            }
        }
        if is_thrust {
            check_travel(poses.name, &travel, impact, recovery, failures);
        }
    }

    fn check_frame(
        &self,
        poses: &PoseTracks,
        frame: u32,
        is_thrust: bool,
        impact: u32,
        failures: &mut Failures,
    ) -> Result<Vec3, String> {
        let grip = poses.local(self.model, self.grip, frame)?;
        let root = poses.local(self.model, self.root, frame)?;
        let model_grip = root * poses.local(self.model, self.spine, frame)? * grip;
        let name = poses.name;
        failures.require(
            GRIP_HEIGHT_M.contains(&model_grip.w_axis.y),
            CHARACTER_MODEL,
            format!("{name} frame {frame}: grip moved away from hip height"),
        );
        failures.require(
            grip.w_axis.x < SHAFT_SIDE_LIMIT_M,
            CHARACTER_MODEL,
            format!("{name} frame {frame}: shaft moved away from the right side"),
        );
        if !is_thrust {
            failures.require(
                grip.z_axis.x < SHAFT_OUTWARD_LIMIT,
                CHARACTER_MODEL,
                format!("{name} frame {frame}: shaft no longer angles outward"),
            );
        } else if frame == impact {
            failures.require(
                model_grip.z_axis.x.abs() < POSITION_TOLERANCE_M
                    && model_grip.z_axis.y.abs() < POSITION_TOLERANCE_M,
                CHARACTER_MODEL,
                format!("{name}: shaft must point straight and level at impact"),
            );
            if let Some(lead) = self.lead {
                let rest_shoe = Vec3::from_array(LEAD_SHOE_REST_M);
                let inverse = self.model.nodes[lead]
                    .transform
                    .matrix()
                    .try_inverse()
                    .ok_or_else(|| format!("{name}: lead leg rest pose is not invertible"))?;
                let shoe = inverse.transform_point3(rest_shoe);
                let planted = (root * poses.local(self.model, lead, frame)?).transform_point3(shoe);
                failures.require(
                    planted.z > rest_shoe.z + LEAD_SHOE_ADVANCE_M
                        && (planted.y - rest_shoe.y).abs() < LEAD_SHOE_VERTICAL_TOLERANCE_M,
                    CHARACTER_MODEL,
                    format!("{name}: lead foot must plant forward at impact"),
                );
            }
        }
        Ok(model_grip.w_axis.truncate())
    }

    fn check_hand(
        &self,
        poses: &PoseTracks,
        frame: u32,
        hand: Hand,
        wrap: Option<Vec3>,
        weapon: Option<&str>,
        failures: &mut Failures,
    ) -> Result<(), String> {
        // Both hand and shaft are evaluated in spine space, so the spine's motion cancels.
        let forearm = poses.local(self.model, hand.arm, frame)?
            * poses.local(self.model, hand.forearm, frame)?;
        let elbow = forearm.w_axis;
        let name = poses.name;
        failures.require(
            elbow.z > ELBOW_FRONT_CLEARANCE_M || elbow.x.abs() > ELBOW_SIDE_CLEARANCE_M,
            CHARACTER_MODEL,
            format!(
                "{name} frame {frame}: {} elbow folds into the chest",
                hand.name
            ),
        );
        if let (Some(wrap), Some(weapon)) = (wrap, weapon) {
            let center = forearm.transform_point3(hand.local_center);
            let wrap = poses
                .local(self.model, self.grip, frame)?
                .transform_point3(wrap);
            failures.require(
                center.distance(wrap) < HAND_WRAP_TOLERANCE_M,
                CHARACTER_MODEL,
                format!(
                    "{name} frame {frame}: {} lost the grip on {weapon}",
                    hand.name
                ),
            );
        }
        Ok(())
    }
}

fn check_travel(
    name: &str,
    travel: &BTreeMap<u32, Vec3>,
    impact: u32,
    recovery: u32,
    failures: &mut Failures,
) {
    let [carry, windup, contact, recovered] =
        [0, SPEAR_WINDUP_FRAME, impact, recovery].map(|frame| {
            let position = travel.get(&frame);
            failures.require(
                position.is_some(),
                CHARACTER_MODEL,
                format!("{name}: missing grip pose at frame {frame}"),
            );
            position
        });
    if let (Some(carry), Some(windup)) = (carry, windup) {
        failures.require(
            carry.z - windup.z > SPEAR_WINDUP_TRAVEL_M,
            CHARACTER_MODEL,
            format!("{name}: wind-up must draw the shaft back"),
        );
    }
    if let (Some(contact), Some(windup)) = (contact, windup) {
        failures.require(
            contact.z - windup.z > SPEAR_IMPACT_TRAVEL_M,
            CHARACTER_MODEL,
            format!("{name}: impact must drive the shaft forward"),
        );
    }
    if let (Some(carry), Some(recovered)) = (carry, recovered) {
        failures.require(
            carry.distance(*recovered) < POSITION_TOLERANCE_M,
            CHARACTER_MODEL,
            format!("{name}: recovery must return to the carry pose"),
        );
    }
}
