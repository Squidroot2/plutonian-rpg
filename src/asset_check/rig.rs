mod spear;
use az::Az;

use super::{
    Animation, Failures, Model, Property, Transform, check_clip_names, check_duration,
    expectations::*, geometry::trs,
};
use crate::{
    content::{ItemKind, Items},
    equipment::{EQUIPMENT_SOCKETS, EquipmentSlot, Handedness, WeaponKind},
    loading::CHARACTER_MODEL,
    player::{
        ACTOR_DEATH_CLIP, CLIPS, SPEAR_THRUST_CLIP, SWORD_SLASH_CLIPS, WEAPON_ARM_BONES,
        directional_walk_clips,
    },
    weapon::ATTACK_CLIP_FPS,
};
use bevy::math::{Mat4, Vec3, Vec4};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check_character(model: &Model, failures: &mut Failures) {
    check_clip_names(CHARACTER_MODEL, model, CLIPS, failures);
    let spear = socket(EquipmentSlot::MainHand, Handedness::TwoHanded);
    require_node(model, CHARACTER_MODEL, spear, failures);
    check_duration(
        CHARACTER_MODEL,
        model,
        CLIPS[ACTOR_DEATH_CLIP],
        ACTOR_DEATH_DURATION_S,
        failures,
    );
    check_duration(
        CHARACTER_MODEL,
        model,
        CLIPS[SPEAR_THRUST_CLIP],
        WeaponKind::Spear.timing().1,
        failures,
    );
    for clip in SWORD_SLASH_CLIPS {
        check_duration(
            CHARACTER_MODEL,
            model,
            CLIPS[clip],
            WeaponKind::Shortsword.timing().1,
            failures,
        );
    }
    check_directional_walks(model, failures);
}

fn check_directional_walks(model: &Model, failures: &mut Failures) {
    let root = require_node(
        model,
        CHARACTER_MODEL,
        crate::equipment::HOLSTER_ROOT_BONE,
        failures,
    );
    let legs = LEGS.map(|name| require_node(model, CHARACTER_MODEL, name, failures));
    for name in directional_walk_clips() {
        let Some(clip) = model.animation(name) else {
            continue;
        };
        for track in clip.tracks.iter().filter(|track| Some(track.node) == root) {
            failures.require(
                track.values.windows(2).all(|pair| pair[0] == pair[1]),
                CHARACTER_MODEL,
                format!("{name}: {:?} moves the root", track.property),
            );
        }
        for (leg, name_of_leg) in legs.into_iter().zip(LEGS) {
            let Some(leg) = leg else {
                continue;
            };
            let animated_loop = clip
                .tracks
                .iter()
                .find(|track| track.node == leg && track.property == Property::Rotation)
                .is_some_and(|track| animated_loop(&track.values));
            failures.require(
                animated_loop,
                CHARACTER_MODEL,
                format!("{name}: {name_of_leg} must animate and loop"),
            );
        }
    }
}

fn animated_loop(values: &[[f32; 4]]) -> bool {
    let (Some(first), Some(last)) = (values.first(), values.last()) else {
        return false;
    };
    values
        .iter()
        .find(|&value| value != first)
        .is_some_and(|second| values.iter().any(|value| value != first && value != second))
        && Vec4::from_array(*first).distance(Vec4::from_array(*last)) < POSITION_TOLERANCE_M
}

pub(super) fn check_weapons(
    model: &Model,
    items: &Items,
    models: &BTreeMap<String, Model>,
    failures: &mut Failures,
) {
    let worn = |id| {
        items.require(id).ok()?;
        let path = items[id].worn_model()?;
        Some((path, models.get(path)?))
    };
    spear::check(
        model,
        worn(ItemKind::FieldSpear),
        worn(ItemKind::Trident),
        failures,
    );
    check_sword(model, worn(ItemKind::Shortsword), failures);
}

fn check_sword(model: &Model, worn: Option<(&str, &Model)>, failures: &mut Failures) {
    if let Some((path, sword)) = worn
        && let Some(wrap) = require_node(sword, path, SWORD_HILT_WRAP, failures)
    {
        failures.require(
            sword.nodes[wrap].transform.translation().length() < POSITION_TOLERANCE_M,
            path,
            "shortsword hilt wrap must be centered on the wrist origin",
        );
    }
    let parents = parents(model, failures);
    let (contact, recovery) = attack_frames(WeaponKind::Shortsword);
    for ((clip, slot), [_, forearm]) in SWORD_SLASH_CLIPS
        .into_iter()
        .zip([EquipmentSlot::MainHand, EquipmentSlot::OffHand])
        .zip(WEAPON_ARM_BONES)
    {
        let name = CLIPS[clip];
        let grip_name = socket(slot, Handedness::OneHanded);
        let Some(grip) = require_node(model, CHARACTER_MODEL, grip_name, failures) else {
            continue;
        };
        let parent = require_node(model, CHARACTER_MODEL, forearm, failures);
        failures.require(
            parent.is_some() && parents[grip] == parent,
            CHARACTER_MODEL,
            format!("{grip_name}: sword grip must follow {forearm}"),
        );
        let Some(animation) = model.animation(name) else {
            continue;
        };
        let poses = PoseTracks::new(animation, failures);
        let [guard, chamber, impact, recovered] =
            [0, SWORD_CHAMBER_FRAME, contact, recovery].map(|frame| {
                poses
                    .world_position(model, &parents, grip, frame)
                    .inspect_err(|error| failures.add(CHARACTER_MODEL, error))
            });
        if let (Ok(chamber), Ok(impact)) = (chamber, impact) {
            failures.require(
                impact.z - chamber.z > SWORD_IMPACT_TRAVEL_M,
                CHARACTER_MODEL,
                format!("{name}: striking hand must advance through contact"),
            );
        }
        if let (Ok(guard), Ok(recovered)) = (guard, recovered) {
            failures.require(
                guard.distance(recovered) < POSITION_TOLERANCE_M,
                CHARACTER_MODEL,
                format!("{name}: striking hand must return to guard"),
            );
        }
    }
}

fn parents(model: &Model, failures: &mut Failures) -> Vec<Option<usize>> {
    let mut parents = vec![None; model.nodes.len()];
    for (parent, node) in model.nodes.iter().enumerate() {
        for &child in &node.children {
            if parents[child].replace(parent).is_some() {
                failures.add(
                    CHARACTER_MODEL,
                    format!("node {child} has multiple parents"),
                );
            }
        }
    }
    parents
}

fn socket(slot: EquipmentSlot, handedness: Handedness) -> &'static str {
    EQUIPMENT_SOCKETS
        .iter()
        .find(|(_, candidate, hand)| *candidate == slot && *hand == Some(handedness))
        .expect("checked weapon slots must have authored rig sockets")
        .0
}

fn require_node(model: &Model, path: &str, name: &str, failures: &mut Failures) -> Option<usize> {
    let node = model.node_index(name);
    failures.require(node.is_some(), path, format!("expected node {name:?}"));
    node
}

fn attack_frames(kind: WeaponKind) -> (u32, u32) {
    let (impact, recovery) = kind.timing();
    (sample_frame(impact), sample_frame(recovery))
}

fn sample_frame(seconds: f32) -> u32 {
    (seconds * ATTACK_CLIP_FPS).round().az::<u32>()
}

struct PoseTracks<'a> {
    name: &'a str,
    tracks: BTreeMap<(usize, Property), BTreeMap<u32, [f32; 4]>>,
}

impl<'a> PoseTracks<'a> {
    fn new(animation: &'a Animation, failures: &mut Failures) -> Self {
        let mut tracks = BTreeMap::new();
        for track in &animation.tracks {
            let samples: BTreeMap<_, _> = track
                .times
                .iter()
                .copied()
                .map(sample_frame)
                .zip(track.values.iter().copied())
                .collect();
            if samples.len() != track.times.len()
                || tracks
                    .insert((track.node, track.property), samples)
                    .is_some()
            {
                failures.add(
                    CHARACTER_MODEL,
                    format!(
                        "{}: duplicate or unreadable {:?} samples on node {}",
                        animation.name, track.property, track.node
                    ),
                );
            }
        }
        Self {
            name: &animation.name,
            tracks,
        }
    }

    fn local(&self, model: &Model, index: usize, frame: u32) -> Result<Mat4, String> {
        let node = &model.nodes[index];
        let sample = |property| -> Result<Option<[f32; 4]>, String> {
            let Some(track) = self.tracks.get(&(index, property)) else {
                return Ok(None);
            };
            if let Some(sample) = track.get(&frame) {
                return Ok(Some(*sample));
            }
            if let Some(first) = track.values().next()
                && track.values().all(|value| value == first)
            {
                return Ok(Some(*first));
            }
            let label = node.name.as_deref().unwrap_or("<unnamed>");
            Err(format!(
                "{} frame {frame}: {label} missing varying {property:?} sample",
                self.name
            ))
        };

        let translation = sample(Property::Translation)?.map(|[x, y, z, _]| [x, y, z]);
        let rotation = sample(Property::Rotation)?;
        let scale = sample(Property::Scale)?.map(|[x, y, z, _]| [x, y, z]);
        match node.transform {
            Transform::Trs {
                translation: rest_translation,
                rotation: rest_rotation,
                scale: rest_scale,
            } => Ok(trs(
                translation.unwrap_or(rest_translation),
                rotation.unwrap_or(rest_rotation),
                scale.unwrap_or(rest_scale),
            )),
            Transform::Matrix(matrix)
                if translation.is_none() && rotation.is_none() && scale.is_none() =>
            {
                Ok(Mat4::from_cols_array_2d(&matrix))
            }
            Transform::Matrix(_) => Err(format!(
                "{} frame {frame}: animated node {index} uses a matrix",
                self.name
            )),
        }
    }

    fn world_position(
        &self,
        model: &Model,
        parents: &[Option<usize>],
        index: usize,
        frame: u32,
    ) -> Result<Vec3, String> {
        let mut next = Some(index);
        let mut visited = BTreeSet::new();
        let mut world = Mat4::IDENTITY;
        while let Some(index) = next {
            if !visited.insert(index) {
                return Err(format!("{}: rig hierarchy contains a cycle", self.name));
            }
            world = self.local(model, index, frame)? * world;
            next = parents[index];
        }
        Ok(world.w_axis.truncate())
    }
}
