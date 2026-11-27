use std::collections::BTreeMap;

use bevy::{gltf::Gltf, prelude::*};

use crate::{
    content::Items,
    equipment::WeaponKind,
    inventory::{Inventory, ItemKind},
    loading::{GameAssets, PendingScene, scene_ready},
    world::{SessionEntity, SpawnWorld, WorldLayout},
};

pub(crate) struct StoragePlugin;

impl Plugin for StoragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChestStorage>()
            .init_resource::<WeaponRackStorage>()
            .add_observer(spawn_chests)
            .add_systems(Update, sync_rack_display);
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

/// Marker for the outpost weapon rack interaction point.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WeaponRack;

/// Interaction point at the centre of the rack in the Barracks
/// (the guard hut at the outpost); see `RACK_SLOT_BASES`.
pub(crate) const RACK_POSITION: Vec3 = Vec3::new(-11.31, -0.45, -8.26);

/// Floor-level grip points for the rack's two slots, matching the slot
/// feet modelled in `scripts/village.py` (`barracks`): slot u=-2.21 and
/// u=-1.57 on the base line v=2.49, on the Barracks floor.
pub(crate) const RACK_SLOT_BASES: [Vec3; 2] = [
    Vec3::new(-11.49, -0.45, -7.94),
    Vec3::new(-11.49, -0.45, -8.58),
];

pub(crate) const RACK_NAME: &str = "Weapon rack";

/// Two rack slots, each empty or holding exactly one weapon item.
#[derive(
    Resource,
    Clone,
    Debug,
    PartialEq,
    Eq,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
pub(crate) struct WeaponRackStorage<I: Ord = ItemKind>(pub(crate) [Option<I>; 2]);

impl<I: Ord> Default for WeaponRackStorage<I> {
    fn default() -> Self {
        Self([None, None])
    }
}

impl WeaponRackStorage {
    pub(crate) fn count(&self, item: ItemKind) -> u32 {
        self.0.iter().filter(|slot| **slot == Some(item)).count() as u32
    }

    pub(crate) fn is_full(&self) -> bool {
        self.0.iter().all(Option::is_some)
    }

    /// Place `item` in the first empty slot; false when the rack is full.
    pub(crate) fn deposit(&mut self, item: ItemKind) -> bool {
        let Some(slot) = self.0.iter_mut().find(|slot| slot.is_none()) else {
            return false;
        };
        *slot = Some(item);
        true
    }

    /// Remove one occurrence of `item`; false when it is not racked.
    pub(crate) fn withdraw(&mut self, item: ItemKind) -> bool {
        let Some(slot) = self.0.iter_mut().find(|slot| **slot == Some(item)) else {
            return false;
        };
        *slot = None;
        true
    }

    /// Every occupied slot holds a weapon known to the catalog.
    pub(crate) fn valid_weapons(&self, items: &Items) -> bool {
        self.0
            .iter()
            .flatten()
            .all(|item| items.require(*item).is_ok() && items[*item].weapon().is_some())
    }

    pub(crate) fn as_inventory(&self) -> Inventory {
        let mut inventory = Inventory::default();
        for item in self.0.iter().flatten() {
            debug_assert!(inventory.add(*item, 1));
        }
        inventory
    }
}

/// One displayed weapon in the rack, reconciled by `sync_rack_display`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RackDisplay {
    pub(crate) slot: usize,
    pub(crate) item: ItemKind,
}

/// Display transform for a racked weapon, local to the rack entity.
/// Both worn models point along local +Z from grip to tip: polearms lean
/// back against the upper rail from the slot foot, while a shortsword is
/// too short to lean, so it hangs from its slot peg with the tip down.
fn rack_display_transform(slot: usize, profile: WeaponKind) -> Transform {
    let base = RACK_SLOT_BASES[slot] - RACK_POSITION;
    let (grip, direction) = match profile {
        WeaponKind::Spear => (base, Vec3::new(0.13, 1.67, 0.10)),
        WeaponKind::Shortsword => (
            base + Vec3::new(0.25, 1.01, 0.0),
            Vec3::new(0.10, -1.0, 0.06),
        ),
    };
    Transform::from_translation(grip)
        .with_rotation(Quat::from_rotation_arc(Vec3::Z, direction.normalize()))
}

fn sync_rack_display(
    rack_storage: Res<WeaponRackStorage>,
    items: Option<Res<Items>>,
    assets: Option<Res<GameAssets>>,
    gltfs: Option<Res<Assets<Gltf>>>,
    racks: Query<Entity, With<WeaponRack>>,
    displays: Query<(Entity, &RackDisplay, &ChildOf)>,
    mut commands: Commands,
) {
    if !rack_storage.is_changed() {
        return;
    }
    let Ok(rack) = racks.single() else {
        return;
    };
    let mut present: Vec<(usize, ItemKind)> = Vec::new();
    for (entity, display, parent) in &displays {
        let desired = rack_storage.0[display.slot] == Some(display.item);
        if parent.parent() == rack && desired {
            present.push((display.slot, display.item));
        } else {
            commands.entity(entity).despawn();
        }
    }
    let (Some(assets), Some(gltfs), Some(items)) = (assets, gltfs, items) else {
        return;
    };
    for (slot, item) in rack_storage.0.iter().enumerate() {
        let Some(item) = item else { continue };
        if present.contains(&(slot, *item)) {
            continue;
        }
        let Some(profile) = items[*item].weapon() else {
            continue;
        };
        let Some(handle) = assets.worn.get(item) else {
            continue;
        };
        let Some(gltf) = gltfs.get(handle) else {
            continue;
        };
        commands
            .spawn((
                Name::new(format!("Racked {}", items[*item].name())),
                RackDisplay { slot, item: *item },
                PendingScene,
                WorldAssetRoot(gltf.scenes[0].clone()),
                rack_display_transform(slot, profile),
                ChildOf(rack),
            ))
            .observe(scene_ready);
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
    // The rack stands against the Barracks rear wall; `RACK_POSITION` is
    // the interaction point between its two modelled slots.
    commands.spawn((
        SessionEntity,
        WeaponRack,
        Name::new(RACK_NAME),
        Transform::from_translation(RACK_POSITION),
        Visibility::default(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_items;

    #[test]
    fn rack_holds_two_weapons_in_any_combination_and_no_more() {
        let mut rack = WeaponRackStorage::default();
        assert!(rack.deposit(ItemKind::FieldSpear));
        assert!(rack.deposit(ItemKind::FieldSpear));
        assert_eq!(rack.count(ItemKind::FieldSpear), 2);
        assert!(rack.is_full());
        assert!(!rack.deposit(ItemKind::Shortsword));
        assert!(rack.withdraw(ItemKind::FieldSpear));
        assert_eq!(rack.count(ItemKind::FieldSpear), 1);
        assert!(rack.deposit(ItemKind::Trident));
        assert!(!rack.withdraw(ItemKind::Shortsword));
        assert!(rack.valid_weapons(test_items()));
    }

    #[test]
    fn rack_display_anchors_lean_polearms_up_and_hang_shortswords_down() {
        for slot in 0..2 {
            let spear = rack_display_transform(slot, WeaponKind::Spear);
            let tip = spear.transform_point(Vec3::new(0.0, 0.0, 1.94));
            assert!(tip.y > 1.5, "spear tip must clear the upper rail");
            let sword = rack_display_transform(slot, WeaponKind::Shortsword);
            let tip = sword.transform_point(Vec3::new(0.0, 0.0, 0.68));
            assert!(tip.y < sword.translation.y, "shortsword hangs tip-down");
            assert!(tip.y > -0.6, "shortsword tip stays above the base");
        }
    }
}
