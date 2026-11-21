use std::collections::BTreeMap;

use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};

use crate::{
    GameState,
    inventory::{Inventory, InventoryNotice, ItemKind, scene_ready},
    loading::{GameAssets, PendingScene},
    menu_ui::{MenuTab, PlayerMenu},
    weapon::WeaponAttack,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, bitcode::Encode, bitcode::Decode)]
pub enum EquipmentSlot {
    Body,
    Helmet,
    MainHand,
    OffHand,
}
impl EquipmentSlot {
    pub const ALL: [Self; 4] = [Self::Body, Self::Helmet, Self::MainHand, Self::OffHand];
    pub const HANDS: [Self; 2] = [Self::MainHand, Self::OffHand];

    pub fn name(self) -> &'static str {
        match self {
            Self::Body => "Body armor",
            Self::Helmet => "Helmet",
            Self::MainHand => "Main hand",
            Self::OffHand => "Off hand",
        }
    }

    fn is_hand(self) -> bool {
        Self::HANDS.contains(&self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handedness {
    OneHanded,
    TwoHanded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponKind {
    Spear,
    Shortsword,
}

impl WeaponKind {
    pub fn handedness(self) -> Handedness {
        match self {
            Self::Spear => Handedness::TwoHanded,
            Self::Shortsword => Handedness::OneHanded,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GearDefinition {
    slot: EquipmentSlot,
    pub defense: u16,
    pub pickup_size: Vec3,
}
impl ItemKind {
    pub fn weapon(self) -> Option<WeaponKind> {
        match self {
            Self::FieldSpear | Self::Trident => Some(WeaponKind::Spear),
            Self::Shortsword => Some(WeaponKind::Shortsword),
            _ => None,
        }
    }

    pub fn fits_slot(self, slot: EquipmentSlot) -> bool {
        self.gear().is_some_and(|gear| gear.slot == slot)
            || (slot == EquipmentSlot::OffHand
                && self
                    .weapon()
                    .is_some_and(|weapon| weapon.handedness() == Handedness::OneHanded))
    }

    pub fn gear(self) -> Option<GearDefinition> {
        let (slot, defense, pickup_size) = match self {
            Self::FieldArmor => (EquipmentSlot::Body, 2, Vec3::new(0.86, 0.76, 0.8)),
            Self::ReinforcedArmor => (EquipmentSlot::Body, 5, Vec3::new(1.02, 0.8, 0.84)),
            Self::FieldHelmet => (EquipmentSlot::Helmet, 1, Vec3::new(0.9, 0.58, 0.72)),
            Self::ReinforcedHelmet => (EquipmentSlot::Helmet, 3, Vec3::new(1.0, 0.64, 0.8)),
            Self::FieldSpear => (EquipmentSlot::MainHand, 0, Vec3::new(0.28, 0.23, 2.3)),
            Self::Trident => (EquipmentSlot::MainHand, 0, Vec3::new(0.58, 0.23, 2.3)),
            Self::Shortsword => (EquipmentSlot::MainHand, 0, Vec3::new(0.26, 0.2, 0.9)),
            _ => return None,
        };
        Some(GearDefinition {
            slot,
            defense,
            pickup_size,
        })
    }
}
#[derive(Resource, Clone, Default, Debug, PartialEq, Eq, bitcode::Encode, bitcode::Decode)]
pub struct Equipment(pub(crate) BTreeMap<EquipmentSlot, ItemKind>);
impl Equipment {
    pub(crate) fn valid_slots(&self) -> bool {
        !(self.off_hand_reserved() && self.get(EquipmentSlot::OffHand).is_some())
            && self.0.iter().all(|(&slot, &item)| item.fits_slot(slot))
    }

    pub fn get(&self, slot: EquipmentSlot) -> Option<ItemKind> {
        self.0.get(&slot).copied()
    }
    pub fn weapon(&self, hand: EquipmentSlot) -> Option<WeaponKind> {
        self.get(hand).and_then(ItemKind::weapon)
    }

    pub fn off_hand_reserved(&self) -> bool {
        self.weapon(EquipmentSlot::MainHand)
            .is_some_and(|weapon| weapon.handedness() == Handedness::TwoHanded)
    }

    pub fn count(&self, item: ItemKind) -> u32 {
        self.0
            .values()
            .filter(|&&equipped| equipped == item)
            .count() as u32
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
        let slot = if item.fits_slot(EquipmentSlot::OffHand)
            && self
                .weapon(EquipmentSlot::MainHand)
                .is_some_and(|weapon| weapon.handedness() == Handedness::OneHanded)
            && self.get(EquipmentSlot::OffHand).is_none()
        {
            EquipmentSlot::OffHand
        } else {
            gear.slot
        };
        self.equip_in(bag, slot, item)
    }

    pub fn equip_in(
        &mut self,
        bag: &mut Inventory,
        slot: EquipmentSlot,
        item: ItemKind,
    ) -> Result<bool, &'static str> {
        if !item.fits_slot(slot) {
            return Err("This item cannot be equipped in that slot.");
        }
        if slot == EquipmentSlot::OffHand && self.off_hand_reserved() {
            return Err("The main-hand weapon uses both hands.");
        }
        if self.get(slot) == Some(item) {
            return Ok(false);
        }
        if bag.count(item) == 0 {
            return Err("This item is no longer in the bag.");
        }

        let mut displaced = BTreeMap::new();
        if let Some(previous) = self.get(slot) {
            displaced.insert(previous, 1);
        }
        let reserves_off_hand = item
            .weapon()
            .is_some_and(|weapon| weapon.handedness() == Handedness::TwoHanded);
        if reserves_off_hand && let Some(previous) = self.get(EquipmentSlot::OffHand) {
            *displaced.entry(previous).or_insert(0) += 1;
        }
        if displaced
            .iter()
            .any(|(&old, &count)| bag.count(old).checked_add(count).is_none())
        {
            return Err("The bag cannot accept the displaced equipment.");
        }
        assert!(
            bag.remove(item, 1),
            "equipment prevalidation must guarantee one owned item is available"
        );
        for (old, count) in displaced {
            assert!(
                bag.add(old, count),
                "equipment prevalidation must guarantee room to return displaced equipment"
            );
        }
        if reserves_off_hand {
            self.0.remove(&EquipmentSlot::OffHand);
        }
        self.0.insert(slot, item);
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
pub enum EquipRequest {
    Quick(ItemKind),
    InSlot(EquipmentSlot, ItemKind),
}
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
    menu: Res<PlayerMenu>,
    thrust: Option<ResMut<WeaponAttack>>,
    mut equipment: ResMut<Equipment>,
    mut bag: ResMut<Inventory>,
    mut notice: ResMut<InventoryNotice>,
    assets: Option<Res<AssetServer>>,
    mut commands: Commands,
) {
    if *state.get() != GameState::Playing
        || !(menu.accepts(MenuTab::Inventory) || menu.accepts(MenuTab::Equipment))
    {
        return;
    }
    let (item, result) = match *request {
        EquipRequest::Quick(item) => (item, equipment.equip(&mut bag, item)),
        EquipRequest::InSlot(slot, item) => (item, equipment.equip_in(&mut bag, slot, item)),
    };
    notice.0 = match result {
        Ok(true) => {
            if item.weapon().is_some()
                && let Some(mut thrust) = thrust
            {
                thrust.cancel();
            }
            if let Some(assets) = assets {
                commands.spawn((
                    AudioPlayer::new(assets.load("sounds/cloth4.ogg")),
                    PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(0.2)),
                ));
            }
            format!("Equipped {}", item.name())
        }
        Ok(false) => "Already equipped.".into(),
        Err(error) => error.into(),
    };
}
fn unequip(
    request: On<UnequipRequest>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    thrust: Option<ResMut<WeaponAttack>>,
    mut equipment: ResMut<Equipment>,
    mut bag: ResMut<Inventory>,
    mut notice: ResMut<InventoryNotice>,
    assets: Option<Res<AssetServer>>,
    mut commands: Commands,
) {
    if *state.get() != GameState::Playing || !menu.accepts(MenuTab::Equipment) {
        return;
    }
    notice.0 = match equipment.unequip(&mut bag, request.0) {
        Ok(true) => {
            if request.0.is_hand()
                && let Some(mut thrust) = thrust
            {
                thrust.cancel();
            }
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
    handedness: Option<Handedness>,
    worn: Option<Entity>,
}

pub(crate) const EQUIPMENT_SOCKETS: [(&str, EquipmentSlot, Option<Handedness>); 5] = [
    ("spine", EquipmentSlot::Body, None),
    ("head", EquipmentSlot::Helmet, None),
    (
        "spear_grip",
        EquipmentSlot::MainHand,
        Some(Handedness::TwoHanded),
    ),
    (
        "sword_grip_right",
        EquipmentSlot::MainHand,
        Some(Handedness::OneHanded),
    ),
    (
        "sword_grip_left",
        EquipmentSlot::OffHand,
        Some(Handedness::OneHanded),
    ),
];

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
        let Some((_, slot, handedness)) = EQUIPMENT_SOCKETS
            .into_iter()
            .find(|(bone, _, _)| *bone == name.as_str())
        else {
            continue;
        };
        commands.entity(entity).insert(EquipmentAttachment {
            slot,
            handedness,
            worn: None,
        });
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
        let desired = equipment
            .get(slot)
            .filter(|item| item.weapon().map(WeaponKind::handedness) == attachment.handedness);
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
    fn dual_wielding_conserves_weight_and_a_two_handed_swap_returns_both_swords() {
        use EquipmentSlot::{MainHand, OffHand};
        use ItemKind::{FieldSpear, Shortsword};

        let mut bag = Inventory::default();
        let mut equipment = Equipment::default();
        assert!(bag.add(Shortsword, 2));
        assert!(bag.add(FieldSpear, 1));
        let weight = bag.carried_weight(&equipment);
        assert_eq!(equipment.equip(&mut bag, Shortsword), Ok(true));
        assert_eq!(equipment.equip(&mut bag, Shortsword), Ok(true));
        assert_eq!(equipment.get(MainHand), Some(Shortsword));
        assert_eq!(equipment.get(OffHand), Some(Shortsword));
        assert_eq!(equipment.count(Shortsword), 2);
        assert_eq!(bag.carried_weight(&equipment), weight);
        assert!(bag.drop_stack(Shortsword, 1, Some(Vec3::ZERO)).is_none());
        assert!(equipment.equip_in(&mut bag, OffHand, FieldSpear).is_err());

        assert_eq!(equipment.equip(&mut bag, FieldSpear), Ok(true));
        assert_eq!(equipment.get(MainHand), Some(FieldSpear));
        assert_eq!(equipment.get(OffHand), None);
        assert!(equipment.off_hand_reserved());
        assert_eq!(bag.count(Shortsword), 2);
        assert_eq!(bag.carried_weight(&equipment), weight);
        assert!(equipment.equip_in(&mut bag, OffHand, Shortsword).is_err());
        assert!(equipment.valid_slots());

        assert_eq!(equipment.unequip(&mut bag, MainHand), Ok(true));
        assert_eq!(equipment.equip_in(&mut bag, OffHand, Shortsword), Ok(true));
        assert_eq!(equipment.get(MainHand), None);
        assert_eq!(equipment.get(OffHand), Some(Shortsword));
    }

    #[test]
    fn a_swap_that_would_overflow_the_bag_preserves_both_hands() {
        use ItemKind::{Shortsword, Trident};

        let mut bag = Inventory::default();
        let mut equipment = Equipment::default();
        assert!(bag.add(Shortsword, 2));
        assert_eq!(equipment.equip(&mut bag, Shortsword), Ok(true));
        assert_eq!(equipment.equip(&mut bag, Shortsword), Ok(true));
        assert!(bag.add(Shortsword, u32::MAX - 1));
        assert!(bag.add(Trident, 1));

        let before = equipment.clone();
        assert!(equipment.equip(&mut bag, Trident).is_err());
        assert_eq!(equipment, before);
        assert_eq!(bag.count(Shortsword), u32::MAX - 1);
        assert_eq!(bag.count(Trident), 1);
    }

    #[test]
    fn equipment_requests_require_the_corresponding_menu_tab() {
        use bevy::state::app::StatesPlugin;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<Equipment>()
            .init_resource::<Inventory>()
            .init_resource::<InventoryNotice>()
            .init_resource::<PlayerMenu>()
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
        app.world_mut()
            .trigger(EquipRequest::Quick(ItemKind::FieldArmor));
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
            .resource_mut::<PlayerMenu>()
            .show(MenuTab::Equipment);
        app.world_mut()
            .trigger(EquipRequest::Quick(ItemKind::FieldArmor));
        assert_eq!(
            app.world().resource::<Equipment>().get(EquipmentSlot::Body),
            Some(ItemKind::FieldArmor)
        );
        app.world_mut()
            .resource_mut::<PlayerMenu>()
            .show(MenuTab::Inventory);
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
            .resource_mut::<PlayerMenu>()
            .show(MenuTab::Equipment);
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
