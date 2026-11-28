mod areas;
mod expectations;
mod geometry;
mod inventory;
mod rig;

use crate::{
    enemy::{ENEMY_MODELS, EnemyModel},
    loading::CHARACTER_MODEL,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Bounds of one primitive in its mesh's local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

pub enum Transform {
    Matrix([[f32; 4]; 4]),
    Trs {
        translation: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
}

pub struct Node {
    pub name: Option<String>,
    pub transform: Transform,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Property {
    Translation,
    Rotation,
    Scale,
}

pub struct Track {
    pub node: usize,
    pub property: Property,
    pub times: Vec<f32>,
    /// Translation and scale use the first three components.
    pub values: Vec<[f32; 4]>,
}

pub struct Animation {
    pub name: String,
    pub duration_s: Option<f32>,
    pub tracks: Vec<Track>,
}

/// Data decoded by the tool crate. The game library owns the validation rules
/// and can share runtime declarations without depending on the GLB decoder.
pub struct Model {
    pub scenes: Vec<Vec<usize>>,
    pub meshes: Vec<Vec<Bounds>>,
    pub nodes: Vec<Node>,
    pub has_skin: bool,
    pub has_external_images: bool,
    pub has_external_buffers: bool,
    pub animations: Vec<Animation>,
    pub decode_failures: Vec<String>,
}

impl Model {
    fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations.iter().find(|clip| clip.name == name)
    }

    fn node_index(&self, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|node| node.name.as_deref() == Some(name))
    }
}

#[derive(Default)]
struct Failures(Vec<String>);

impl Failures {
    fn add(&mut self, path: impl std::fmt::Display, expectation: impl std::fmt::Display) {
        self.0.push(format!("{path}: {expectation}"));
    }

    fn require(&mut self, pass: bool, path: &str, message: impl std::fmt::Display) {
        if !pass {
            self.add(path, message);
        }
    }
}

/// Checks the inventory and model contracts relative to the repository root.
/// The frontend supplies the two parsers and decides how to print the failures.
pub fn check_repository(
    root: &Path,
    rust_model_paths: impl Fn(&str) -> Result<BTreeSet<String>, String>,
    decode_glb: impl Fn(&[u8]) -> Result<Model, String>,
) -> Vec<String> {
    let mut failures = Failures::default();
    let items = inventory::load_items(root, &mut failures);
    let paths = inventory::model_paths(root, items.as_ref(), rust_model_paths, &mut failures);
    let models = inventory::load_models(root, &paths, decode_glb, &mut failures);
    for (path, model) in &models {
        check_structure(path, model, &mut failures);
    }
    areas::check(&paths, &models, &mut failures);
    check_enemies(ENEMY_MODELS, &models, &mut failures);
    if let Some(character) = models.get(CHARACTER_MODEL) {
        rig::check_character(character, &models, &mut failures);
        if let Some(items) = &items {
            rig::check_weapons(character, items, &models, &mut failures);
        }
    }
    if let Some(items) = &items {
        inventory::check_pickups(items, &models, &mut failures);
    }
    failures.0
}

fn check_structure(path: &str, model: &Model, failures: &mut Failures) {
    failures.require(model.scenes.len() == 1, path, "expected one scene");
    failures.require(!model.meshes.is_empty(), path, "expected at least one mesh");
    failures.require(!model.has_external_images, path, "external image");
    failures.require(!model.has_external_buffers, path, "external buffer");
    for issue in &model.decode_failures {
        failures.add(path, issue);
    }
}

fn check_enemies(
    registry: &[EnemyModel],
    models: &BTreeMap<String, Model>,
    failures: &mut Failures,
) {
    for enemy in registry {
        let Some(model) = models.get(enemy.path) else {
            // The inventory reports missing or unreadable models.
            continue;
        };
        let clips = enemy.clips.into_iter().chain(enemy.locomotion);
        check_clip_names(
            enemy.path,
            model,
            clips.clone().map(|clip| clip.name),
            failures,
        );
        for clip in clips {
            check_duration(enemy.path, model, clip.name, clip.duration_s, failures);
        }
    }
}

fn check_clip_names<'a>(
    path: &str,
    model: &Model,
    expected: impl IntoIterator<Item = &'a str>,
    failures: &mut Failures,
) {
    failures.require(model.has_skin, path, "expected a skin");
    let expected: BTreeSet<_> = expected.into_iter().collect();
    let mut found = BTreeSet::new();
    for animation in &model.animations {
        if !found.insert(animation.name.as_str()) {
            failures.add(path, format!("duplicate clip {:?}", animation.name));
        }
    }
    for name in expected.difference(&found) {
        failures.add(
            path,
            format!("expected clip {name:?} not found (found: {found:?})"),
        );
    }
    for name in found.difference(&expected) {
        failures.add(path, format!("unexpected clip {name:?}"));
    }
}

fn check_duration(path: &str, model: &Model, name: &str, expected_s: f32, failures: &mut Failures) {
    if let Some(clip) = model.animation(name) {
        match clip.duration_s {
            Some(seconds) if (seconds - expected_s).abs() < expectations::DURATION_TOLERANCE_S => {}
            Some(seconds) => failures.add(
                path,
                format!("clip {name:?} expected duration {expected_s:.3} s (found {seconds:.3} s)"),
            ),
            None => failures.add(path, format!("clip {name:?} has no readable duration")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemy::EnemyClip;

    #[test]
    fn registry_entry_checks_missing_clips_and_incorrect_durations() {
        let registry = [EnemyModel {
            path: "fixture.glb",
            clips: [
                EnemyClip::new("FixtureIdle", 2.0),
                EnemyClip::new("FixtureAttack", 1.0),
                EnemyClip::new("FixtureDeath", 1.0),
            ],
            locomotion: Some(EnemyClip::new("FixtureWalk", 1.0)),
            visual_offset: bevy::math::Vec3::ZERO,
        }];
        let model = Model {
            scenes: vec![vec![]],
            meshes: vec![],
            nodes: vec![],
            has_skin: true,
            has_external_images: false,
            has_external_buffers: false,
            animations: [
                ("FixtureIdle", 2.0),
                ("FixtureDeath", 0.5),
                ("FixtureWalk", 1.0),
            ]
            .map(|(name, seconds)| Animation {
                name: name.into(),
                duration_s: Some(seconds),
                tracks: vec![],
            })
            .into(),
            decode_failures: vec![],
        };
        let models = BTreeMap::from([(registry[0].path.into(), model)]);
        let mut failures = Failures::default();
        check_enemies(&registry, &models, &mut failures);
        assert_eq!(failures.0.len(), 2);
        assert!(
            failures.0[0].starts_with("fixture.glb: expected clip \"FixtureAttack\" not found")
        );
        assert_eq!(
            failures.0[1],
            "fixture.glb: clip \"FixtureDeath\" expected duration 1.000 s (found 0.500 s)"
        );
    }
}
