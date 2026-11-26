use super::{Failures, Model, expectations::*, geometry::world_bounds};
use crate::{
    content::{Catalog, Items},
    enemy::ENEMY_MODELS,
};
use bevy::math::Vec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

type Paths = BTreeSet<String>;

pub(super) fn load_items(root: &Path, failures: &mut Failures) -> Option<Items> {
    let path = Path::new("data").join(Items::PATH.trim_start_matches("data://"));
    let result = fs::read(root.join(&path))
        .map_err(|error| error.to_string())
        .and_then(|bytes| ron::de::from_bytes::<Items>(&bytes).map_err(|error| error.to_string()));
    match result {
        Ok(items) => {
            if let Err(error) = items.validate() {
                failures.add(path.display(), error);
            }
            Some(items)
        }
        Err(error) => {
            failures.add(path.display(), error);
            None
        }
    }
}

pub(super) fn model_paths(
    root: &Path,
    items: Option<&Items>,
    rust_model_paths: impl Fn(&str) -> Result<Paths, String>,
    failures: &mut Failures,
) -> Paths {
    let mut paths = Paths::new();
    for source in files_with_extension(&root.join("src"), "rs", failures) {
        let found = fs::read_to_string(&source)
            .map_err(|error| error.to_string())
            .and_then(|code| rust_model_paths(&code));
        match found {
            Ok(found) => paths.extend(found),
            Err(error) => failures.add(source.display(), error),
        }
    }
    if let Some(items) = items {
        for id in items.ids() {
            let item = &items[id];
            paths.insert(item.pickup_model.clone());
            paths.extend(item.worn_model().map(str::to_owned));
        }
    }
    paths.extend(ENEMY_MODELS.iter().map(|model| model.path.to_owned()));
    let assets = root.join("assets");
    for file in files_with_extension(&assets, "glb", failures) {
        let relative = file
            .strip_prefix(&assets)
            .expect("asset walk yields paths under the asset directory");
        if !paths.contains(relative.to_string_lossy().as_ref()) {
            failures.add(
                relative.display(),
                "GLB has no code or item catalog reference",
            );
        }
    }
    paths
}

fn files_with_extension(dir: &Path, extension: &str, failures: &mut Failures) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            failures.add(dir.display(), error);
            return files;
        }
    };
    for entry in entries {
        match entry.and_then(|entry| Ok((entry.path(), entry.file_type()?))) {
            Ok((path, kind)) if kind.is_dir() => {
                files.extend(files_with_extension(&path, extension, failures))
            }
            Ok((path, _)) if path.extension() == Some(OsStr::new(extension)) => files.push(path),
            Ok(_) => (),
            Err(error) => failures.add(dir.display(), error),
        }
    }
    files.sort_unstable();
    files
}

pub(super) fn load_models(
    root: &Path,
    paths: &Paths,
    decode_glb: impl Fn(&[u8]) -> Result<Model, String>,
    failures: &mut Failures,
) -> BTreeMap<String, Model> {
    let mut models = BTreeMap::new();
    for path in paths {
        let Some(source) = blender_path(path) else {
            failures.add(path, "expected an asset-relative GLB path under models/");
            continue;
        };
        if !root.join(&source).is_file() {
            failures.add(path, format!("missing Blender source {}", source.display()));
        }
        let result = fs::read(root.join("assets").join(path))
            .map_err(|error| format!("could not read referenced GLB ({error})"))
            .and_then(|bytes| decode_glb(&bytes));
        match result {
            Ok(model) => {
                models.insert(path.clone(), model);
            }
            Err(error) => failures.add(path, error),
        }
    }
    models
}

fn blender_path(path: &str) -> Option<PathBuf> {
    let relative = Path::new(path.strip_prefix("models/")?);
    if relative.extension()? != "glb"
        || !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    let stem = relative.file_stem()?.to_str()?;
    let filename = format!("{}.blend", stem.strip_suffix("_worn").unwrap_or(stem));
    Some(Path::new("art").join(relative.with_file_name(filename)))
}

pub(super) fn check_pickups(
    items: &Items,
    models: &BTreeMap<String, Model>,
    failures: &mut Failures,
) {
    for id in items.ids() {
        let item = &items[id];
        let worn = item.worn_model();
        let (center_tolerance, clearance_tolerance) = if worn.is_some() {
            (POSITION_TOLERANCE_M, POSITION_TOLERANCE_M)
        } else if let Some((_, tolerance)) = LOOSE_PICKUP_CENTER_TOLERANCES
            .iter()
            .find(|(loot, _)| *loot == id)
        {
            (*tolerance, 0.0)
        } else {
            continue;
        };
        let path = &item.pickup_model;
        let Some(model) = models.get(path) else {
            continue;
        };
        if let Ok(bounds) = world_bounds(model).inspect_err(|error| failures.add(path, error)) {
            let min = Vec3::from_array(bounds.min);
            let max = Vec3::from_array(bounds.max);
            failures.require(
                min.y.abs() < POSITION_TOLERANCE_M,
                path,
                "pickup must have a base origin",
            );
            let horizontal = min + max;
            failures.require(
                horizontal.x.abs() < center_tolerance && horizontal.z.abs() < center_tolerance,
                path,
                "pickup must be centered horizontally",
            );
            let limit = item.pickup_size() + Vec3::splat(clearance_tolerance);
            failures.require(
                (max - min).cmple(limit).all(),
                path,
                format!("pickup exceeds {id} drop clearance {limit:?} (bounds {min:?} to {max:?})"),
            );
        }
        if let Some((worn_path, worn)) = worn.and_then(|path| Some((path, models.get(path)?))) {
            failures.require(
                model.meshes.len() == worn.meshes.len(),
                path,
                format!("worn and pickup mesh counts differ ({worn_path})"),
            );
        }
    }
}
