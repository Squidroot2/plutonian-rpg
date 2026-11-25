//! Loads and validates the authored gameplay catalogs used to build a session.
//!
//! Items, enemies, NPCs, and world placements are defined as RON assets and
//! loaded together during boot. The catalogs are validated as a group before
//! becoming resources; failures or timeouts send boot to the failed state.
//! This module also defines the compact IDs shared by catalog references and
//! save data.

mod enemies;
mod items;
mod npcs;
mod placements;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use enemies::{Enemies, EnemyArchetype, EnemyDefinition};
pub(crate) use items::WeaponStats;
pub use items::{ItemKind, Items};
#[cfg(test)]
pub(crate) use npcs::test_npcs;
pub(crate) use npcs::{NpcId, Npcs};
pub(crate) use placements::{CollectedPickups, PlacedPickup, Placements};

use crate::{GameState, loading::LoadStatus};
use bevy::{
    asset::{AssetLoader, LoadContext, LoadState, io::Reader},
    prelude::*,
};
use serde::{Deserialize, Deserializer, de::Error as _};
use std::{fmt, io, marker::PhantomData, time::Instant};

/// Maximum byte length for authored IDs, which fits the fixed-size inline
/// representation of [`ContentId`] while keeping the accepted ID limit explicit.
/// IDs are ASCII, so this is also their maximum character count; checkpoints
/// serialize them as strings rather than storing the padded representation.
const MAX_ID_BYTES: usize = 48;
/// Maximum time allowed for the gameplay catalogs to finish loading during boot.
/// The timer starts when catalog handles are requested; if any catalog is still
/// unavailable after this duration, boot transitions to `GameState::Failed`.
const BOOT_TIMEOUT_SECONDS: u64 = 90;

/// A short authored ID. Its inline representation keeps inventory and encounter keys copyable.
/// Checkpoint schemas encode these IDs as strings.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ContentId([u8; MAX_ID_BYTES]);

impl ContentId {
    pub(crate) const fn new(id: &str) -> Self {
        let bytes = id.as_bytes();
        assert!(
            bytes.len() <= MAX_ID_BYTES,
            "built-in content ID is too long"
        );
        let mut value = [0; MAX_ID_BYTES];
        let mut i = 0;
        while i < bytes.len() {
            value[i] = bytes[i];
            i += 1;
        }
        Self(value)
    }

    pub(crate) fn parse(id: &str) -> Result<Self, String> {
        if id.is_empty()
            || id.len() > MAX_ID_BYTES
            || !id.as_bytes()[0].is_ascii_lowercase()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(format!(
                "Invalid content ID {id:?}. Use 1–{MAX_ID_BYTES} lowercase ASCII letters, digits or underscores, starting with a letter."
            ));
        }
        Ok(Self::new(id))
    }

    pub(crate) fn as_str(&self) -> &str {
        let end = self
            .0
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(MAX_ID_BYTES);
        std::str::from_utf8(&self.0[..end]).expect("content IDs contain only ASCII")
    }
}

impl fmt::Debug for ContentId {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(output)
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ContentId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

trait Catalog: Asset + for<'de> Deserialize<'de> {
    const PATH: &'static str;
    const EXTENSION: &'static str;
}

#[derive(TypePath)]
struct CatalogLoader<T>(PhantomData<T>);

impl<T: Catalog> AssetLoader for CatalogLoader<T> {
    type Asset = T;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<T, io::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        ron::de::from_bytes(&bytes).map_err(io::Error::other)
    }

    fn extensions(&self) -> &[&str] {
        &[T::EXTENSION]
    }
}

#[derive(Resource)]
struct CatalogHandles {
    items: Handle<Items>,
    enemies: Handle<Enemies>,
    placements: Handle<Placements>,
    npcs: Handle<Npcs>,
    started: Instant,
}

pub(crate) struct ContentPlugin;

impl Plugin for ContentPlugin {
    fn build(&self, app: &mut App) {
        register::<Items>(app);
        register::<Enemies>(app);
        register::<Placements>(app);
        register::<Npcs>(app);
        app.add_observer(placements::spawn_encounters)
            .add_systems(OnEnter(GameState::Boot), request_catalogs)
            .add_systems(Update, prepare_catalogs.run_if(in_state(GameState::Boot)));
    }
}

fn register<T: Catalog>(app: &mut App) {
    app.init_asset::<T>()
        .register_asset_loader(CatalogLoader::<T>(PhantomData));
}

pub(crate) fn read_save_catalogs(root: &std::path::Path) -> io::Result<(Items, Placements)> {
    fn read<T: Catalog>(root: &std::path::Path) -> io::Result<T> {
        let relative = T::PATH
            .strip_prefix("data://")
            .expect("catalog uses the data asset source");
        read_ron(root.join(relative))
    }

    let items = read(root)?;
    let enemies = read(root)?;
    let placements = read(root)?;
    let npcs = read(root)?;
    validate_catalogs(&items, &enemies, &placements, &npcs).map_err(io::Error::other)?;
    Ok((items, placements))
}

pub(crate) fn read_ron<T: serde::de::DeserializeOwned>(path: std::path::PathBuf) -> io::Result<T> {
    let read = || ron::de::from_bytes(&std::fs::read(&path)?).map_err(io::Error::other);
    read().map_err(|error: io::Error| {
        io::Error::new(
            error.kind(),
            format!("Could not read {}: {error}", path.display()),
        )
    })
}

fn validate_catalogs(
    items: &Items,
    enemies: &Enemies,
    placements: &Placements,
    npcs: &Npcs,
) -> Result<(), String> {
    items.validate()?;
    enemies.validate(items)?;
    placements.validate(items, enemies)?;
    npcs.validate(items)
}

fn request_catalogs(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(CatalogHandles {
        items: server.load(Items::PATH),
        enemies: server.load(Enemies::PATH),
        placements: server.load(Placements::PATH),
        npcs: server.load(Npcs::PATH),
        started: Instant::now(),
    });
}

fn prepare_catalogs(
    mut commands: Commands,
    server: Res<AssetServer>,
    handles: Res<CatalogHandles>,
    items: Res<Assets<Items>>,
    enemies: Res<Assets<Enemies>>,
    placements: Res<Assets<Placements>>,
    npcs: Res<Assets<Npcs>>,
    mut status: ResMut<LoadStatus>,
    mut next: ResMut<NextState<GameState>>,
) {
    for (id, path) in [
        (handles.items.id().untyped(), Items::PATH),
        (handles.enemies.id().untyped(), Enemies::PATH),
        (handles.placements.id().untyped(), Placements::PATH),
        (handles.npcs.id().untyped(), Npcs::PATH),
    ] {
        if let Some(LoadState::Failed(error)) = server.get_load_state(id) {
            status.error = Some(format!("Could not load {path}: {error}"));
            next.set(GameState::Failed);
            return;
        }
    }
    if handles.started.elapsed().as_secs() > BOOT_TIMEOUT_SECONDS {
        status.error = Some("Timed out while loading gameplay catalogs".into());
        next.set(GameState::Failed);
        return;
    }
    let (Some(items), Some(enemies), Some(placements), Some(npcs)) = (
        items.get(&handles.items),
        enemies.get(&handles.enemies),
        placements.get(&handles.placements),
        npcs.get(&handles.npcs),
    ) else {
        return;
    };
    if let Err(error) = validate_catalogs(items, enemies, placements, npcs) {
        status.error = Some(error);
        next.set(GameState::Failed);
        return;
    }
    commands.insert_resource(items.clone());
    commands.insert_resource(enemies.clone());
    commands.insert_resource(placements.clone());
    commands.insert_resource(npcs.clone());
    commands.remove_resource::<CatalogHandles>();
    next.set(GameState::Title);
}

pub(crate) fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
pub(crate) use enemies::test_enemies;
#[cfg(test)]
pub(crate) use items::test_items;

#[cfg(test)]
pub(crate) use placements::test_placements;
