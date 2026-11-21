use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::{
    inventory::{Inventory, ItemKind},
    world::{SpawnWorld, WorldLayout},
};

pub(crate) struct StoragePlugin;

impl Plugin for StoragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChestStorage>()
            .add_observer(spawn_chests);
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ChestId {
    Lamp,
    Supply,
    Gear,
}

impl ChestId {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Lamp => "Lamp chest",
            Self::Supply => "Supply chest",
            Self::Gear => "Gear chest",
        }
    }
}

#[derive(Resource)]
pub(crate) struct ChestStorage(BTreeMap<ChestId, Inventory>);

impl Default for ChestStorage {
    fn default() -> Self {
        use ItemKind::*;

        let stocked = |stacks: &[(ItemKind, u32)]| {
            let mut inventory = Inventory::default();
            for &(item, quantity) in stacks {
                assert!(inventory.add(item, quantity), "invalid starter stack");
            }
            inventory
        };
        Self(BTreeMap::from([
            (ChestId::Lamp, stocked(&[(Battery, 2), (MethanePouch, 2)])),
            (ChestId::Supply, stocked(&[(Scrap, 2), (FieldArmor, 1)])),
            (ChestId::Gear, stocked(&[(FieldHelmet, 1)])),
        ]))
    }
}

impl ChestStorage {
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
            chest,
            Name::new(chest.name()),
            Transform::from_translation(position),
        ));
    }
}
