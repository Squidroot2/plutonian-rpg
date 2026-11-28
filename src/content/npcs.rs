use super::{Catalog, Items};
use crate::{
    attributes::{AttributeId, AttributeRanks, DEFAULT_RANK, MAX_RANK},
    equipment::{Equipment, EquipmentSlot},
    inventory::{Inventory, ItemKind, Pickup},
};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Component, Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NpcId {
    Sella,
    Orren,
    Nemi,
    Isera,
    Vey,
    Tovan,
    Orla,
}

impl NpcId {
    pub(crate) const CARAVAN: [Self; 4] = [Self::Isera, Self::Vey, Self::Tovan, Self::Orla];

    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::Sella => "sella",
            Self::Orren => "orren",
            Self::Nemi => "nemi",
            Self::Isera => "isera",
            Self::Vey => "vey",
            Self::Tovan => "tovan",
            Self::Orla => "orla",
        }
    }

    pub(crate) fn persistent_id(key: &str) -> Option<Self> {
        std::iter::once(Self::Nemi)
            .chain(Self::CARAVAN)
            .find(|id| id.key() == key)
    }
}

#[derive(Asset, TypePath, Resource, Clone, Deserialize)]
#[serde(transparent)]
pub(crate) struct Npcs(Vec<NpcDefinition>);

impl Catalog for Npcs {
    const PATH: &'static str = "data://npcs/catalog.npc.ron";
    const EXTENSION: &'static str = "npc.ron";
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct NpcDefinition {
    id: NpcId,
    #[serde(default)]
    attributes: Ranks,
    #[serde(default)]
    equipment: Vec<Gear>,
    #[serde(default)]
    inventory: Vec<Pickup>,
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Ranks {
    vitality: u8,
    endurance: u8,
    agility: u8,
    strength: u8,
    marksmanship: u8,
}

impl Default for Ranks {
    fn default() -> Self {
        Self {
            vitality: DEFAULT_RANK,
            endurance: DEFAULT_RANK,
            agility: DEFAULT_RANK,
            strength: DEFAULT_RANK,
            marksmanship: DEFAULT_RANK,
        }
    }
}

impl Ranks {
    fn build(&self) -> Result<AttributeRanks, String> {
        let mut ranks = AttributeRanks::default();
        for (id, rank) in [
            (AttributeId::Vitality, self.vitality),
            (AttributeId::Endurance, self.endurance),
            (AttributeId::Agility, self.agility),
            (AttributeId::Strength, self.strength),
            (AttributeId::Marksmanship, self.marksmanship),
        ] {
            if !(1..=MAX_RANK).contains(&rank) {
                return Err(format!("{id:?} must be in 1..={MAX_RANK}"));
            }
            ranks = ranks.with_rank(id, rank);
        }
        Ok(ranks)
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Gear {
    slot: EquipmentSlot,
    item: ItemKind,
}

impl NpcDefinition {
    fn build(&self, items: &Items) -> Result<(AttributeRanks, Inventory, Equipment), String> {
        let ranks = self.attributes.build()?;
        let mut bag = Inventory::default();
        let mut slots = BTreeMap::new();
        for gear in &self.equipment {
            items.require(gear.item)?;
            if slots.insert(gear.slot, gear.item).is_some() {
                return Err(format!("Duplicate slot {:?}", gear.slot));
            }
        }
        let mut equipment = Equipment::default();
        // The main hand precedes the off hand so equip_in enforces two-handed reservations.
        for (slot, item) in slots {
            assert!(
                bag.add(item, 1),
                "each authored gear copy is equipped in the same pass"
            );
            equipment
                .equip_in(
                    &mut bag,
                    slot,
                    item,
                    ranks.rank(AttributeId::Strength),
                    items,
                )
                .map_err(|reason| format!("{item} in {slot:?}: {reason}"))?;
        }
        for &stack in &self.inventory {
            items.validate_stack(stack)?;
            if !bag.add(stack.item, stack.quantity) {
                return Err(format!("Invalid inventory quantity for {}", stack.item));
            }
        }
        if matches!(self.id, NpcId::Sella | NpcId::Orren)
            && equipment.weapon(EquipmentSlot::MainHand, items).is_none()
        {
            return Err("A guard needs a main-hand weapon for combat and corpse loot".into());
        }
        Ok((ranks, bag, equipment))
    }
}

impl Npcs {
    pub(crate) fn validate(&self, items: &Items) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for npc in &self.0 {
            let key = npc.id.key();
            if !seen.insert(npc.id) {
                return Err(format!("{}: duplicate NPC {key}", Self::PATH));
            }
            npc.build(items)
                .map_err(|error| format!("{} entry {key}: {error}", Self::PATH))?;
        }
        for id in [NpcId::Sella, NpcId::Orren, NpcId::Nemi]
            .into_iter()
            .chain(NpcId::CARAVAN)
        {
            if !seen.contains(&id) {
                return Err(format!("{}: missing NPC {}", Self::PATH, id.key()));
            }
        }
        Ok(())
    }

    pub(crate) fn loadout(
        &self,
        id: NpcId,
        items: &Items,
    ) -> (AttributeRanks, Inventory, Equipment) {
        self.0
            .iter()
            .find(|entry| entry.id == id)
            .expect("shipped NPC definitions are required")
            .build(items)
            .expect("NPC loadouts must be validated during boot")
    }
}

#[cfg(test)]
pub(crate) fn test_npcs() -> Npcs {
    let npcs: Npcs =
        ron::from_str(include_str!("../../data/npcs/catalog.npc.ron")).expect("NPC catalog parses");
    npcs.validate(super::test_items())
        .expect("NPC loadouts validate");
    npcs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_loadouts_validate_minimum_strength_and_two_handed_slots() {
        let items = crate::content::test_items();
        let file = include_str!("../../data/npcs/catalog.npc.ron");
        let weak: Npcs = ron::from_str(&file.replacen("strength: 98", "strength: 5", 1))
            .expect("an out-of-range loadout still parses");
        let error = weak
            .validate(items)
            .expect_err("a guard cannot equip above their Strength");
        assert!(error.contains("sella") && error.contains("Strength"));

        let occupied = file.replacen(
            r#"(slot: MainHand, item: "guard_polearm"),"#,
            r#"(slot: MainHand, item: "guard_polearm"), (slot: OffHand, item: "shortsword"),"#,
            1,
        );
        let occupied: Npcs = ron::from_str(&occupied).expect("off-hand entries parse");
        let error = occupied
            .validate(items)
            .expect_err("a polearm reserves the off hand");
        assert!(error.contains("sella") && error.contains("both hands"));
    }
}
