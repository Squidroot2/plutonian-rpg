use bevy::{asset::AssetPath, prelude::*};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub(super) const USAGE: &str = "Usage: plutonian-art-preview <recipe.ron> <shot> [output.png]";
pub(super) const LOCAL_FILES: &str = "preview-files";
pub(super) const ACTOR_ORIGIN: Vec3 = Vec3::new(0.5, -0.15, -4.8);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    model: String,
    shots: BTreeMap<String, Shot>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Shot {
    pub mount: Mount,
    pub view: Option<View>,
    pub pose: Option<Pose>,
    #[serde(default)]
    pub hide_model: bool,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

#[derive(Deserialize)]
pub(super) enum Mount {
    Socket(String),
    Rack,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pose {
    pub clip: String,
    pub frame: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Effect {
    pub model: String,
    pub socket: String,
    #[serde(default)]
    pub offset: [f32; 3],
    pub pose: Option<Pose>,
}

#[derive(Clone, Copy, Deserialize)]
pub(super) enum View {
    Hand,
    Front,
    Shoulder,
    Back,
    Rack,
    Wide,
    Detail,
    Muzzle,
}

impl View {
    pub(super) fn camera(self) -> (Transform, Projection) {
        // Actor views are relative to the display point in the Nivalis courtyard.
        let (eye, target, fov_degrees) = match self {
            Self::Hand => (
                Vec3::new(-3.46, 1.64, -1.16),
                Vec3::new(-0.09, 1.1, 0.43),
                33.0,
            ),
            Self::Front => (Vec3::new(1.5, 1.6, 3.3), Vec3::new(0.0, 1.1, 0.2), 35.0),
            Self::Shoulder => (
                Vec3::new(-1.88, 1.71, -2.56),
                Vec3::new(-0.2, 1.28, 0.72),
                26.5,
            ),
            Self::Back => (
                Vec3::new(1.78, 1.67, -3.64),
                Vec3::new(0.0, 1.07, 0.0),
                32.0,
            ),
            Self::Wide => (
                Vec3::new(-6.0, 1.93, -0.18),
                Vec3::new(0.0, 1.17, 1.99),
                30.0,
            ),
            Self::Detail => (
                Vec3::new(-2.78, 1.56, 0.96),
                Vec3::new(0.0, 1.1, 0.44),
                29.0,
            ),
            Self::Muzzle => (
                Vec3::new(-3.32, 1.56, 0.59),
                Vec3::new(-0.25, 1.37, 0.97),
                26.5,
            ),
            Self::Rack => (
                Vec3::new(-8.21, 1.34, -9.53),
                Vec3::new(-6.85, 0.54, -7.22),
                32.0,
            ),
        };
        let origin = if matches!(self, Self::Rack) {
            Vec3::ZERO
        } else {
            ACTOR_ORIGIN
        };
        (
            Transform::from_translation(origin + eye).looking_at(origin + target, Vec3::Y),
            Projection::Perspective(PerspectiveProjection {
                fov: f32::to_radians(fov_degrees),
                ..default()
            }),
        )
    }
}

pub(super) fn select(
    args: impl IntoIterator<Item = OsString>,
) -> Result<(String, Shot, PathBuf), String> {
    let mut args = args.into_iter();
    let recipe_path = PathBuf::from(args.next().ok_or(USAGE)?);
    let name = args
        .next()
        .and_then(|name| name.into_string().ok())
        .ok_or(USAGE)?;
    let output = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    let bytes = fs::read(&recipe_path)
        .map_err(|error| format!("Could not read {}: {error}", recipe_path.display()))?;
    let mut recipe: Recipe = ron::de::from_bytes(&bytes)
        .map_err(|error| format!("Invalid recipe {}: {error}", recipe_path.display()))?;
    let shot = recipe.shots.remove(&name).ok_or_else(|| {
        let names = recipe.shots.keys().cloned().collect::<Vec<_>>().join(", ");
        format!(
            "No shot {name:?} in {}. Available shots: {names}",
            recipe_path.display()
        )
    })?;
    if shot.hide_model && !shot.effects.is_empty() {
        return Err("A shot that hides the model cannot mount effects on it".into());
    }
    if matches!(shot.mount, Mount::Rack) && shot.pose.is_some() {
        return Err("A rack shot has no actor to pose".into());
    }
    let output = match output {
        Some(path) => path,
        None if !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte)) =>
        {
            PathBuf::from(format!("/tmp/plutonian-art-{name}.png"))
        }
        None => return Err("Supply an output path for a shot with spaces or punctuation".into()),
    };
    if output
        .extension()
        .is_none_or(|extension| extension != "png")
    {
        return Err("The screenshot path must end in .png".into());
    }
    Ok((recipe.model, shot, output))
}

pub(super) fn model_path(model: &str) -> Result<AssetPath<'static>, String> {
    let path = Path::new(model);
    if path.file_name() == Some(path.as_os_str()) && path.extension().is_none() {
        return Ok(AssetPath::from_path_buf(
            Path::new("models").join(path.with_extension("glb")),
        ));
    }
    if path.extension().is_none_or(|extension| extension != "glb") {
        return Err(format!(
            "Expected a registry name or .glb path, got {model:?}"
        ));
    }
    if let Ok(relative) = path.strip_prefix("assets") {
        return Ok(AssetPath::from_path_buf(relative.to_owned()));
    }
    if path.starts_with("models") {
        return Ok(AssetPath::from_path_buf(path.to_owned()));
    }
    let absolute = path
        .canonicalize()
        .map_err(|error| format!("Cannot open GLB {model:?}: {error}"))?;
    let relative = absolute
        .strip_prefix("/")
        .expect("a canonical Linux file path is absolute");
    Ok(AssetPath::from_path_buf(relative.to_owned()).with_source(LOCAL_FILES))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_name_and_asset_relative_paths_resolve_to_the_same_model() {
        let stem = "methane_musket_worn";
        let model = model_path(stem).expect("registered export stem");
        for prefix in ["models", "assets/models"] {
            let path = format!("{prefix}/{stem}.glb");
            assert_eq!(model_path(&path).expect("export path"), model);
        }
    }

    #[test]
    fn musket_recipe_selects_the_shot_pose_and_muzzle_effects() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/contributing/previews/musket.ron"
        );
        let (model, shot, output) =
            select([path, "shot"].map(OsString::from)).expect("checked-in musket shot");
        assert_eq!(model, "methane_musket_worn");
        assert!(matches!(shot.mount, Mount::Socket(ref socket) if socket == "musket_grip"));
        let pose = shot.pose.expect("fire pose");
        assert_eq!((pose.clip.as_str(), pose.frame), ("MusketFire", 1));
        assert_eq!(shot.effects.len(), 2);
        assert!(
            shot.effects
                .iter()
                .all(|effect| effect.socket == "MusketMuzzle")
        );
        assert_eq!(output, Path::new("/tmp/plutonian-art-shot.png"));
    }
}
