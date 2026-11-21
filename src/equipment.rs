use std::collections::BTreeMap;

use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};

use crate::{
    GameState,
    inventory::{Inventory, InventoryNotice, ItemKind, scene_ready},
    loading::{GameAssets, PendingScene},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EquipmentSlot {
    Body,
    Helmet,
    Weapon,
}
impl EquipmentSlot {
    pub const ALL: [Self; 3] = [Self::Body, Self::Helmet, Self::Weapon];
    pub fn name(self) -> &'static str {
        match self {
            Self::Body => "Body armor",
            Self::Helmet => "Helmet",
            Self::Weapon => "Weapon",
        }
    }
    pub fn bone(self) -> &'static str {
        match self {
            Self::Body => "spine",
            Self::Helmet => "head",
            Self::Weapon => "spear_grip",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct GearDefinition {
    pub slot: EquipmentSlot,
    pub defense: u16,
    pub pickup_size: Vec3,
}
impl ItemKind {
    pub fn gear(self) -> Option<GearDefinition> {
        let (slot, defense, pickup_size) = match self {
            Self::FieldArmor => (EquipmentSlot::Body, 2, Vec3::new(0.86, 0.76, 0.8)),
            Self::ReinforcedArmor => (EquipmentSlot::Body, 5, Vec3::new(1.02, 0.8, 0.84)),
            Self::FieldHelmet => (EquipmentSlot::Helmet, 1, Vec3::new(0.9, 0.58, 0.72)),
            Self::ReinforcedHelmet => (EquipmentSlot::Helmet, 3, Vec3::new(1.0, 0.64, 0.8)),
            Self::FieldSpear => (EquipmentSlot::Weapon, 0, Vec3::new(0.28, 0.23, 2.3)),
            Self::Trident => (EquipmentSlot::Weapon, 0, Vec3::new(0.58, 0.23, 2.3)),
            _ => return None,
        };
        Some(GearDefinition {
            slot,
            defense,
            pickup_size,
        })
    }
}
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct Equipment(BTreeMap<EquipmentSlot, ItemKind>);
impl Equipment {
    pub fn get(&self, slot: EquipmentSlot) -> Option<ItemKind> {
        self.0.get(&slot).copied()
    }
    pub fn has_spear(&self) -> bool {
        matches!(
            self.get(EquipmentSlot::Weapon),
            Some(ItemKind::FieldSpear | ItemKind::Trident)
        )
    }
    pub(crate) fn weight(&self) -> u64 {
        self.0.values().map(|item| item.weight()).sum()
    }

    pub fn total_defense(&self) -> u16 {
        self.0
            .values()
            .filter_map(|item| item.gear())
            .map(|gear| gear.defense)
            .sum()
    }
    pub fn equip(&mut self, bag: &mut Inventory, item: ItemKind) -> Result<bool, &'static str> {
        let gear = item.gear().ok_or("This item cannot be equipped.")?;
        let previous = self.get(gear.slot);
        if previous == Some(item) {
            return Ok(false);
        }
        if bag.count(item) == 0 {
            return Err("This item is no longer in the bag.");
        }
        if previous.is_some_and(|old| bag.count(old) == u32::MAX) {
            return Err("The bag cannot accept the previous item.");
        }
        assert!(
            bag.remove(item, 1),
            "equipment prevalidation must guarantee one owned item is available"
        );
        if let Some(old) = previous {
            assert!(
                bag.add(old, 1),
                "equipment prevalidation must guarantee room to return the previous item"
            );
        }
        self.0.insert(gear.slot, item);
        Ok(true)
    }
    pub fn unequip(
        &mut self,
        bag: &mut Inventory,
        slot: EquipmentSlot,
    ) -> Result<bool, &'static str> {
        let Some(item) = self.get(slot) else {
            return Ok(false);
        };
        if !bag.add(item, 1) {
            return Err("The bag cannot accept the equipped item.");
        }
        self.0.remove(&slot);
        Ok(true)
    }
}
#[derive(Event)]
pub struct EquipRequest(pub ItemKind);
#[derive(Event)]
pub struct UnequipRequest(pub EquipmentSlot);
#[derive(Component, Clone, Copy, Debug)]
pub struct WornEquipment {
    pub slot: EquipmentSlot,
    pub item: ItemKind,
}

pub struct EquipmentPlugin;
impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Equipment>()
            .add_observer(equip)
            .add_observer(unequip)
            .add_systems(Update, sync_worn.run_if(resource_exists::<GameAssets>));
    }
}
fn equip(
    request: On<EquipRequest>,
    state: Res<State<GameState>>,
    mut equipment: ResMut<Equipment>,
    mut bag: ResMut<Inventory>,
    mut notice: ResMut<InventoryNotice>,
    assets: Option<Res<AssetServer>>,
    mut commands: Commands,
) {
    if *state.get() != GameState::Inventory {
        return;
    }
    notice.0 = match equipment.equip(&mut bag, request.0) {
        Ok(true) => {
            if let Some(assets) = assets {
                commands.spawn((
                    AudioPlayer::new(assets.load("sounds/cloth4.ogg")),
                    PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(0.2)),
                ));
            }
            format!("Equipped {}", request.0.name())
        }
        Ok(false) => "Already equipped.".into(),
        Err(error) => error.into(),
    };
}
fn unequip(
    request: On<UnequipRequest>,
    state: Res<State<GameState>>,
    mut equipment: ResMut<Equipment>,
    mut bag: ResMut<Inventory>,
    mut notice: ResMut<InventoryNotice>,
    assets: Option<Res<AssetServer>>,
    mut commands: Commands,
) {
    if *state.get() != GameState::Inventory {
        return;
    }
    notice.0 = match equipment.unequip(&mut bag, request.0) {
        Ok(true) => {
            if let Some(assets) = assets {
                commands.spawn((
                    AudioPlayer::new(assets.load("sounds/cloth3.ogg")),
                    PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(0.2)),
                ));
            }
            format!("Unequipped {}", request.0.name())
        }
        Ok(false) => "That slot is empty.".into(),
        Err(error) => error.into(),
    };
}
#[derive(Component)]
struct EquipmentAttachment {
    slot: EquipmentSlot,
    worn: Option<Entity>,
}

pub(crate) fn bind_equipment_attachments(
    event: On<WorldInstanceReady>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut commands: Commands,
) {
    for entity in children.iter_descendants(event.entity) {
        let Ok(name) = names.get(entity) else {
            continue;
        };
        if let Some(slot) = EquipmentSlot::ALL
            .into_iter()
            .find(|slot| slot.bone() == name.as_str())
        {
            commands
                .entity(entity)
                .insert(EquipmentAttachment { slot, worn: None });
        }
    }
}

fn sync_worn(
    mut commands: Commands,
    equipment: Res<Equipment>,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    mut attachments: Query<(Entity, &mut EquipmentAttachment)>,
    worn: Query<&WornEquipment>,
) {
    for (bone, mut attachment) in &mut attachments {
        if !equipment.is_changed() && !attachment.is_added() {
            continue;
        }
        let slot = attachment.slot;
        let desired = equipment.get(slot);
        let existing = attachment
            .worn
            .and_then(|entity| worn.get(entity).ok().map(|piece| (entity, piece.item)));
        if existing.map(|(_, item)| item) == desired {
            continue;
        }
        if let Some((entity, _)) = existing {
            commands.entity(entity).despawn();
        }
        attachment.worn = desired.map(|item| {
            let scene = gltfs
                .get(&assets.worn[&item])
                .expect("worn equipment models must remain loaded after asset validation")
                .scenes[0]
                .clone();
            commands
                .spawn((
                    Name::new(format!("Worn {}", item.name())),
                    WornEquipment { slot, item },
                    PendingScene,
                    WorldAssetRoot(scene),
                    Transform::default(),
                    ChildOf(bone),
                ))
                .observe(scene_ready)
                .id()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equip_swap_unequip_conserve_items_and_update_defense() {
        let mut bag = Inventory::default();
        let mut gear = Equipment::default();
        assert!(bag.add(ItemKind::FieldArmor, 2));
        assert!(bag.add(ItemKind::ReinforcedArmor, 1));
        assert!(bag.add(ItemKind::FieldHelmet, 1));
        assert_eq!(gear.equip(&mut bag, ItemKind::FieldArmor), Ok(true));
        assert_eq!(gear.equip(&mut bag, ItemKind::FieldHelmet), Ok(true));
        assert_eq!(gear.total_defense(), 3);
        assert_eq!(bag.count(ItemKind::FieldArmor), 1);
        assert_eq!(gear.equip(&mut bag, ItemKind::ReinforcedArmor), Ok(true));
        assert_eq!(gear.total_defense(), 6);
        assert_eq!(bag.count(ItemKind::FieldArmor), 2);
        assert_eq!(bag.count(ItemKind::ReinforcedArmor), 0);
        assert_eq!(gear.unequip(&mut bag, EquipmentSlot::Body), Ok(true));
        assert_eq!(gear.total_defense(), 1);
        assert_eq!(bag.count(ItemKind::ReinforcedArmor), 1);
    }
    #[test]
    fn invalid_and_repeated_operations_preserve_ownership() {
        let mut bag = Inventory::default();
        let mut gear = Equipment::default();
        assert!(gear.equip(&mut bag, ItemKind::FieldArmor).is_err());
        bag.add(ItemKind::Battery, 1);
        assert!(gear.equip(&mut bag, ItemKind::Battery).is_err());
        bag.add(ItemKind::FieldHelmet, 2);
        gear.equip(&mut bag, ItemKind::FieldHelmet)
            .expect("the owned field helmet must equip into the empty helmet slot");
        assert_eq!(gear.equip(&mut bag, ItemKind::FieldHelmet), Ok(false));
        assert_eq!(bag.count(ItemKind::FieldHelmet), 1);
        assert_eq!(gear.unequip(&mut bag, EquipmentSlot::Body), Ok(false));
        gear.unequip(&mut bag, EquipmentSlot::Helmet)
            .expect("the bag must accept the unequipped helmet");
        assert_eq!(bag.count(ItemKind::FieldHelmet), 2);
        assert_eq!(gear.total_defense(), 0);
    }
    #[test]
    fn failed_swap_or_unequip_does_not_lose_equipment() {
        let mut bag = Inventory::default();
        let mut gear = Equipment::default();
        bag.add(ItemKind::FieldArmor, 1);
        gear.equip(&mut bag, ItemKind::FieldArmor)
            .expect("the owned field armor must equip into the empty body slot");
        bag.add(ItemKind::FieldArmor, u32::MAX);
        bag.add(ItemKind::ReinforcedArmor, 1);
        assert!(gear.equip(&mut bag, ItemKind::ReinforcedArmor).is_err());
        assert!(gear.unequip(&mut bag, EquipmentSlot::Body).is_err());
        assert_eq!(gear.get(EquipmentSlot::Body), Some(ItemKind::FieldArmor));
        assert_eq!(bag.count(ItemKind::ReinforcedArmor), 1);
    }
    #[test]
    fn dropping_bag_copies_does_not_remove_worn_items() {
        let mut bag = Inventory::default();
        let mut gear = Equipment::default();
        bag.add(ItemKind::FieldArmor, 2);
        gear.equip(&mut bag, ItemKind::FieldArmor)
            .expect("one of the two owned armor pieces must equip");
        assert!(
            bag.drop_stack(ItemKind::FieldArmor, 1, Some(Vec3::ZERO))
                .is_some()
        );
        assert!(
            bag.drop_stack(ItemKind::FieldArmor, 1, Some(Vec3::ZERO))
                .is_none()
        );
        assert_eq!(gear.get(EquipmentSlot::Body), Some(ItemKind::FieldArmor));
        gear.unequip(&mut bag, EquipmentSlot::Body)
            .expect("the empty bag must accept the equipped armor");
        assert!(
            bag.drop_stack(ItemKind::FieldArmor, 1, Some(Vec3::ZERO))
                .is_some()
        );
    }
    #[test]
    fn requests_only_change_equipment_while_inventory_is_open() {
        use bevy::state::app::StatesPlugin;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<Equipment>()
            .init_resource::<Inventory>()
            .init_resource::<InventoryNotice>()
            .add_observer(equip)
            .add_observer(unequip);
        app.finish();
        app.cleanup();
        app.world_mut()
            .resource_mut::<Inventory>()
            .add(ItemKind::FieldArmor, 1);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.update();
        app.world_mut().trigger(EquipRequest(ItemKind::FieldArmor));
        assert_eq!(
            app.world().resource::<Equipment>().get(EquipmentSlot::Body),
            None
        );
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(ItemKind::FieldArmor),
            1
        );
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Inventory);
        app.update();
        app.world_mut().trigger(EquipRequest(ItemKind::FieldArmor));
        assert_eq!(
            app.world().resource::<Equipment>().get(EquipmentSlot::Body),
            Some(ItemKind::FieldArmor)
        );
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.update();
        app.world_mut().trigger(UnequipRequest(EquipmentSlot::Body));
        assert_eq!(
            app.world().resource::<Equipment>().get(EquipmentSlot::Body),
            Some(ItemKind::FieldArmor)
        );
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(ItemKind::FieldArmor),
            0
        );
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Inventory);
        app.update();
        app.world_mut().trigger(UnequipRequest(EquipmentSlot::Body));
        assert_eq!(
            app.world().resource::<Equipment>().get(EquipmentSlot::Body),
            None
        );
        assert_eq!(
            app.world()
                .resource::<Inventory>()
                .count(ItemKind::FieldArmor),
            1
        );
    }
}
