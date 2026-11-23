use super::{Catalog, ContentId};
use crate::{
    attributes::MAX_RANK,
    equipment::{EquipmentSlot, Handedness, WeaponKind},
    inventory::Pickup,
};
use bevy::prelude::*;
use serde::Deserialize;
use std::{collections::BTreeMap, fmt, ops::Index};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct ItemKind(ContentId);

#[allow(non_upper_case_globals)]
impl ItemKind {
    pub const Battery: Self = Self(ContentId::new("battery"));
    pub const Scrap: Self = Self(ContentId::new("scrap"));
    pub const MethanePouch: Self = Self(ContentId::new("methane_pouch"));
    pub const BeaconCore: Self = Self(ContentId::new("beacon_core"));
    pub const FieldArmor: Self = Self(ContentId::new("field_armor"));
    pub const ReinforcedArmor: Self = Self(ContentId::new("reinforced_armor"));
    pub const FieldHelmet: Self = Self(ContentId::new("field_helmet"));
    pub const ReinforcedHelmet: Self = Self(ContentId::new("reinforced_helmet"));
    pub const FieldSpear: Self = Self(ContentId::new("field_spear"));
    pub const Trident: Self = Self(ContentId::new("trident"));
    pub const LargeBoulder: Self = Self(ContentId::new("large_boulder"));
    pub const WormMeat: Self = Self(ContentId::new("worm_meat"));
    pub const RoachMeat: Self = Self(ContentId::new("roach_meat"));
    pub const Shortsword: Self = Self(ContentId::new("shortsword"));
    pub const Mycelium: Self = Self(ContentId::new("mycelium"));

    pub const MATERIALS: [Self; 3] = [Self::Battery, Self::Scrap, Self::MethanePouch];
    const REQUIRED: [Self; 15] = [
        Self::Battery,
        Self::Scrap,
        Self::MethanePouch,
        Self::BeaconCore,
        Self::FieldArmor,
        Self::ReinforcedArmor,
        Self::FieldHelmet,
        Self::ReinforcedHelmet,
        Self::FieldSpear,
        Self::Trident,
        Self::LargeBoulder,
        Self::WormMeat,
        Self::RoachMeat,
        Self::Shortsword,
        Self::Mycelium,
    ];

    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        ContentId::parse(value).map(Self)
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ItemKind {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(output)
    }
}

#[derive(Asset, TypePath, Resource, Clone)]
pub struct Items {
    ordered: Vec<ItemKind>,
    definitions: BTreeMap<ItemKind, ItemDefinition>,
}

impl<'de> Deserialize<'de> for Items {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let entries = Vec::<ItemDefinition>::deserialize(deserializer)?;
        let mut definitions = BTreeMap::new();
        let mut ordered = Vec::with_capacity(entries.len());
        for entry in entries {
            let id = entry.id;
            if definitions.insert(id, entry).is_some() {
                return Err(D::Error::custom(format!("Duplicate item ID {id}")));
            }
            ordered.push(id);
        }
        Ok(Self {
            ordered,
            definitions,
        })
    }
}

impl Catalog for Items {
    const PATH: &'static str = "data://items/catalog.item.ron";
    const EXTENSION: &'static str = "item.ron";
}

impl Index<ItemKind> for Items {
    type Output = ItemDefinition;
    fn index(&self, id: ItemKind) -> &ItemDefinition {
        self.definitions
            .get(&id)
            .expect("item ID must come from a validated catalog or checkpoint")
    }
}

impl Items {
    pub(crate) fn ids(&self) -> impl Iterator<Item = ItemKind> + '_ {
        self.ordered.iter().copied()
    }

    pub(crate) fn gear_ids(&self) -> impl Iterator<Item = ItemKind> + '_ {
        self.ids().filter(|&id| self[id].gear().is_some())
    }

    pub(crate) fn require(&self, id: ItemKind) -> Result<(), String> {
        self.definitions
            .contains_key(&id)
            .then_some(())
            .ok_or_else(|| format!("Missing item definition {id}"))
    }

    pub(crate) fn validate_stack(&self, stack: Pickup) -> Result<(), String> {
        self.require(stack.item)?;
        if stack.quantity == 0 || stack.item == ItemKind::BeaconCore {
            return Err(format!("Invalid ordinary item stack {}", stack.item));
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        for id in ItemKind::REQUIRED {
            self.require(id)
                .map_err(|error| format!("{}: {error}", Self::PATH))?;
        }
        for id in self.ids() {
            self[id]
                .validate()
                .map_err(|error| format!("{} entry {id}: {error}", Self::PATH))?;
        }
        for (id, slot, profile) in [
            (ItemKind::ReinforcedArmor, EquipmentSlot::Body, None),
            (ItemKind::ReinforcedHelmet, EquipmentSlot::Helmet, None),
            (
                ItemKind::Trident,
                EquipmentSlot::MainHand,
                Some(WeaponKind::Spear),
            ),
        ] {
            if !self[id].fits_slot(slot) || self[id].weapon() != profile {
                return Err(format!(
                    "{} entry {id} must be compatible with the guard outfit",
                    Self::PATH
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDefinition {
    id: ItemKind,
    name: String,
    description: String,
    weight_kg: u32,
    pub(crate) pickup_model: String,
    pickup_size_m: [f32; 3],
    kind: ItemType,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GearDefinition {
    pub(crate) slot: EquipmentSlot,
    pub(crate) defense: u16,
    pub(crate) minimum_strength: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WeaponStats {
    pub(crate) damage: u16,
    pub(crate) reach_m: f32,
    pub(crate) contact_radius_m: f32,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
enum ItemType {
    Material,
    Armor {
        slot: ArmorSlot,
        defense: u16,
        minimum_strength: u8,
        worn_model: String,
    },
    Weapon {
        profile: WeaponKind,
        minimum_strength: u8,
        worn_model: String,
        stats: WeaponStats,
    },
}

#[derive(Clone, Copy, Deserialize)]
enum ArmorSlot {
    Body,
    Helmet,
}

impl ItemDefinition {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn description(&self) -> &str {
        &self.description
    }
    pub(crate) fn weight(&self) -> u64 {
        u64::from(self.weight_kg)
    }
    pub(crate) fn pickup_size(&self) -> Vec3 {
        self.pickup_size_m.into()
    }

    pub(crate) fn worn_model(&self) -> Option<&str> {
        match &self.kind {
            ItemType::Armor { worn_model, .. } | ItemType::Weapon { worn_model, .. } => {
                Some(worn_model)
            }
            ItemType::Material => None,
        }
    }

    pub(crate) fn weapon(&self) -> Option<WeaponKind> {
        self.attack().map(|(profile, _)| profile)
    }

    pub(crate) fn attack(&self) -> Option<(WeaponKind, WeaponStats)> {
        match self.kind {
            ItemType::Weapon { profile, stats, .. } => Some((profile, stats)),
            _ => None,
        }
    }

    pub(crate) fn gear(&self) -> Option<GearDefinition> {
        let (slot, defense, minimum_strength) = match self.kind {
            ItemType::Material => return None,
            ItemType::Armor {
                slot,
                defense,
                minimum_strength,
                ..
            } => {
                let slot = match slot {
                    ArmorSlot::Body => EquipmentSlot::Body,
                    ArmorSlot::Helmet => EquipmentSlot::Helmet,
                };
                (slot, defense, minimum_strength)
            }
            ItemType::Weapon {
                minimum_strength, ..
            } => (EquipmentSlot::MainHand, 0, minimum_strength),
        };
        Some(GearDefinition {
            slot,
            defense,
            minimum_strength,
        })
    }

    pub(crate) fn fits_slot(&self, slot: EquipmentSlot) -> bool {
        self.gear().is_some_and(|gear| gear.slot == slot)
            || (slot == EquipmentSlot::OffHand
                && self
                    .weapon()
                    .is_some_and(|weapon| weapon.handedness() == Handedness::OneHanded))
    }

    pub(crate) fn meets_strength(&self, strength: u8) -> bool {
        self.gear()
            .is_some_and(|gear| strength >= gear.minimum_strength)
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.name.trim().is_empty() || self.description.trim().is_empty() {
            return Err("Name and description are required");
        }
        if !is_model_path(&self.pickup_model)
            || self.worn_model().is_some_and(|path| !is_model_path(path))
        {
            return Err("Model paths must be GLBs relative to assets/models");
        }
        if !self.pickup_size_m.into_iter().all(super::positive) {
            return Err("Pickup dimensions must be finite and positive");
        }
        if self
            .gear()
            .is_some_and(|gear| !(1..=MAX_RANK).contains(&gear.minimum_strength))
        {
            return Err("Minimum Strength is outside the supported rank range");
        }
        if let Some((_, stats)) = self.attack()
            && (stats.damage == 0
                || !super::positive(stats.reach_m)
                || !super::positive(stats.contact_radius_m))
        {
            return Err("Weapon damage, reach, and contact radius must be positive");
        }
        if self.id == ItemKind::BeaconCore && self.gear().is_some() {
            return Err("The beacon core is a quest item");
        }
        Ok(())
    }
}

fn is_model_path(path: &str) -> bool {
    path.starts_with("models/")
        && path.ends_with(".glb")
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
        && !path.contains(['\\', '#', ':'])
}

#[cfg(test)]
pub(crate) fn test_items() -> &'static Items {
    static ITEMS: std::sync::OnceLock<Items> = std::sync::OnceLock::new();
    ITEMS.get_or_init(|| {
        let items: Items = ron::from_str(include_str!("../../data/items/catalog.item.ron"))
            .expect("shipped item catalog must parse");
        items
            .validate()
            .expect("shipped item catalog must validate");
        items
    })
}
