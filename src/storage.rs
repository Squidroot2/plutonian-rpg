use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::{
    inventory::{Inventory, ItemKind},
    world::{SessionEntity, SpawnWorld, WorldLayout},
};

pub(crate) struct StoragePlugin;

impl Plugin for StoragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChestStorage>()
            .add_observer(spawn_chests);
    }
}

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    bitcode::Encode,
    bitcode::Decode,
    serde::Deserialize,
    serde::Serialize,
    schemars::JsonSchema,
)]
pub(crate) enum ChestId {
    Lamp,
    Supply,
    Gear,
}

impl ChestId {
    pub(crate) const ALL: [Self; 3] = [Self::Lamp, Self::Supply, Self::Gear];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Lamp => "Lamp chest",
            Self::Supply => "Supply chest",
            Self::Gear => "Gear chest",
        }
    }
}

#[derive(
    Resource,
    Clone,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
pub(crate) struct ChestStorage<I: Ord = ItemKind>(pub(crate) BTreeMap<ChestId, Inventory<I>>);

impl Default for ChestStorage {
    fn default() -> Self {
        Self(
            ChestId::ALL
                .into_iter()
                .map(|id| (id, Inventory::default()))
                .collect(),
        )
    }
}

impl ChestStorage {
    pub(crate) fn valid_stacks(&self) -> bool {
        self.0.len() == 3 && self.0.values().all(Inventory::valid_stacks)
    }

    pub(crate) fn get(&self, chest: ChestId) -> &Inventory {
        &self.0[&chest]
    }

    pub(crate) fn get_mut(&mut self, chest: ChestId) -> &mut Inventory {
        self.0
            .get_mut(&chest)
            .expect("every chest has an inventory")
    }
}

fn spawn_chests(_: On<SpawnWorld>, layout: Res<WorldLayout>, mut commands: Commands) {
    if *layout != WorldLayout::Outpost {
        return;
    }
    // Target points sit above the lids of the three crates in the starting tent GLB.
    for (chest, position) in [
        (ChestId::Lamp, Vec3::new(7.84, -0.09, -4.75)),
        (ChestId::Supply, Vec3::new(8.15, 0.0, -1.31)),
        (ChestId::Gear, Vec3::new(9.43, -0.22, -0.98)),
    ] {
        commands.spawn((
            SessionEntity,
            chest,
            Name::new(chest.name()),
            Transform::from_translation(position),
        ));
    }
}
