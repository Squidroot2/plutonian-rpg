#[cfg(test)]
use crate::player::PlayerWorldExt as _;
use std::collections::BTreeMap;

use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};

use crate::{
    GameState,
    attributes::{AttributeId, AttributeRanks},
    combat::{CombatSystems, Dead},
    content::Items,
    inventory::{Inventory, InventoryNotice, ItemKind},
    loading::{GameAssets, PendingScene, scene_ready},
    player::Player,
    ui::menu_ui::{MenuTab, PlayerMenu},
    weapon::{WeaponAttack, WeaponStance},
};

#[derive(
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
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

#[derive(
    Component,
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
pub struct Equipment<I = ItemKind>(pub(crate) BTreeMap<EquipmentSlot, I>);
impl<I> Default for Equipment<I> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl Equipment {
    pub(crate) fn valid_slots(&self, items: &Items) -> bool {
        !(self.off_hand_reserved(items) && self.get(EquipmentSlot::OffHand).is_some())
            && self
                .0
                .iter()
                .all(|(&slot, &item)| items[item].fits_slot(slot))
    }

    pub fn get(&self, slot: EquipmentSlot) -> Option<ItemKind> {
        self.0.get(&slot).copied()
    }
    pub fn weapon(&self, hand: EquipmentSlot, items: &Items) -> Option<WeaponKind> {
        self.get(hand).and_then(|item| items[item].weapon())
    }

    pub fn off_hand_reserved(&self, items: &Items) -> bool {
        self.weapon(EquipmentSlot::MainHand, items)
            .is_some_and(|weapon| weapon.handedness() == Handedness::TwoHanded)
    }

    pub fn count(&self, item: ItemKind) -> u32 {
        self.0
            .values()
            .filter(|&&equipped| equipped == item)
            .count()
            .try_into()
            .expect("equipment slot count fits in u32")
    }
    pub(crate) fn weight(&self, items: &Items) -> u64 {
        self.0.values().map(|item| items[*item].weight()).sum()
    }

    pub fn total_defense(&self, items: &Items) -> u16 {
        self.0
            .values()
            .filter_map(|item| items[*item].gear())
            .map(|gear| gear.defense)
            .fold(0, u16::saturating_add)
    }
    pub fn equip(
        &mut self,
        bag: &mut Inventory,
        item: ItemKind,
        strength: u8,
        items: &Items,
    ) -> Result<bool, &'static str> {
        let gear = items[item].gear().ok_or("This item cannot be equipped.")?;
        let slot = if items[item].fits_slot(EquipmentSlot::OffHand)
            && self
                .weapon(EquipmentSlot::MainHand, items)
                .is_some_and(|weapon| weapon.handedness() == Handedness::OneHanded)
            && self.get(EquipmentSlot::OffHand).is_none()
        {
            EquipmentSlot::OffHand
        } else {
            gear.slot
        };
        self.equip_in(bag, slot, item, strength, items)
    }

    pub fn equip_in(
        &mut self,
        bag: &mut Inventory,
        slot: EquipmentSlot,
        item: ItemKind,
        strength: u8,
        items: &Items,
    ) -> Result<bool, &'static str> {
        if !items[item].fits_slot(slot) {
            return Err("This item cannot be equipped in that slot.");
        }
        if !items[item].meets_strength(strength) {
            return Err("Strength is too low to equip this item.");
        }
        if slot == EquipmentSlot::OffHand && self.off_hand_reserved(items) {
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
        let reserves_off_hand = items[item]
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
    // Read by the smoke scenarios when asserting on worn gear.
    #[allow(dead_code)]
    pub slot: EquipmentSlot,
    pub item: ItemKind,
}

pub struct EquipmentPlugin;
impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(equip).add_observer(unequip).add_systems(
            Update,
            sync_worn
                .after(CombatSystems::Detect)
                .run_if(resource_exists::<GameAssets>),
        );
    }
}
fn equip(
    request: On<EquipRequest>,
    items: Res<Items>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    mut players: Query<
        (
            &AttributeRanks,
            &mut WeaponAttack,
            &mut Equipment,
            &mut Inventory,
        ),
        (With<Player>, Without<Dead>),
    >,
    mut notice: ResMut<InventoryNotice>,
    assets: Option<Res<AssetServer>>,
    mut commands: Commands,
) {
    if *state.get() != GameState::Playing
        || !(menu.accepts(MenuTab::Inventory) || menu.accepts(MenuTab::Equipment))
    {
        return;
    }
    let Ok((ranks, mut attack, mut equipment, mut bag)) = players.single_mut() else {
        return;
    };
    let strength = ranks.rank(AttributeId::Strength);
    let (item, result) = match *request {
        EquipRequest::Quick(item) => (item, equipment.equip(&mut bag, item, strength, &items)),
        EquipRequest::InSlot(slot, item) => (
            item,
            equipment.equip_in(&mut bag, slot, item, strength, &items),
        ),
    };
    notice.0 = match result {
        Ok(true) => {
            if items[item].weapon().is_some() {
                attack.cancel();
            }
            if let Some(assets) = assets {
                commands.spawn((
                    AudioPlayer::new(assets.load("sounds/cloth4.ogg")),
                    PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(0.2)),
                ));
            }
            format!("Equipped {}", items[item].name())
        }
        Ok(false) => "Already equipped.".into(),
        Err(error) => error.into(),
    };
}
fn unequip(
    request: On<UnequipRequest>,
    state: Res<State<GameState>>,
    menu: Res<PlayerMenu>,
    mut players: Query<
        (&mut WeaponAttack, &mut Equipment, &mut Inventory),
        (With<Player>, Without<Dead>),
    >,
    mut notice: ResMut<InventoryNotice>,
    assets: Option<Res<AssetServer>>,
    mut commands: Commands,
) {
    if *state.get() != GameState::Playing
        || (!menu.accepts(MenuTab::Equipment) && !menu.accepts(MenuTab::Inventory))
    {
        return;
    }
    let Ok((mut attack, mut equipment, mut bag)) = players.single_mut() else {
        return;
    };
    notice.0 = match equipment.unequip(&mut bag, request.0) {
        Ok(true) => {
            if request.0.is_hand() {
                attack.cancel();
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
    owner: Entity,
    slot: EquipmentSlot,
    handedness: Option<Handedness>,
    holster: Option<Entity>,
    worn: Option<Entity>,
}

pub(crate) const SPINE_BONE: &str = "spine";
pub(crate) const RIGHT_SWORD_GRIP: &str = "sword_grip_right";
pub(crate) const LEFT_SWORD_GRIP: &str = "sword_grip_left";
pub(crate) const HOLSTER_ROOT_BONE: &str = "root";
pub(crate) const EQUIPMENT_SOCKETS: [(&str, EquipmentSlot, Option<Handedness>); 5] = [
    (SPINE_BONE, EquipmentSlot::Body, None),
    ("head", EquipmentSlot::Helmet, None),
    (
        "spear_grip",
        EquipmentSlot::MainHand,
        Some(Handedness::TwoHanded),
    ),
    (
        RIGHT_SWORD_GRIP,
        EquipmentSlot::MainHand,
        Some(Handedness::OneHanded),
    ),
    (
        LEFT_SWORD_GRIP,
        EquipmentSlot::OffHand,
        Some(Handedness::OneHanded),
    ),
];

pub(crate) fn bind_equipment_attachments(
    event: On<WorldInstanceReady>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut commands: Commands,
) {
    let Ok(owner) = parents.get(event.entity).map(ChildOf::parent) else {
        return;
    };
    let bones = equipment_bones(event.entity, &children, &names);
    let (Some(&spine), Some(&root)) = (bones.get(SPINE_BONE), bones.get(HOLSTER_ROOT_BONE)) else {
        return;
    };
    for (name, slot, handedness) in EQUIPMENT_SOCKETS {
        let Some(&bone) = bones.get(name) else {
            continue;
        };
        let holster =
            handedness.map(|hands| spawn_holster(&mut commands, slot, hands, spine, root));
        commands.entity(bone).insert(EquipmentAttachment {
            owner,
            slot,
            handedness,
            holster,
            worn: None,
        });
    }
}

fn equipment_bones<'a>(
    scene: Entity,
    children: &Query<&Children>,
    names: &'a Query<&Name>,
) -> BTreeMap<&'a str, Entity> {
    children
        .iter_descendants(scene)
        .filter_map(|entity| names.get(entity).ok().map(|name| (name.as_str(), entity)))
        .collect()
}

fn spawn_holster(
    commands: &mut Commands,
    slot: EquipmentSlot,
    handedness: Handedness,
    spine: Entity,
    root: Entity,
) -> Entity {
    // Both worn weapon models point along their local +Z from the grip to the tip.
    let (name, parent, grip, direction) = match handedness {
        Handedness::TwoHanded => {
            let direction = Vec3::new(0.62, 0.78, 0.0).normalize();
            let grip = Vec3::new(0.0, 0.43, -0.40) - 0.86 * direction;
            ("polearm_holster", spine, grip, direction)
        }
        Handedness::OneHanded => {
            let (name, side) = match slot {
                EquipmentSlot::OffHand => ("left_hip_holster", 1.0),
                _ => ("right_hip_holster", -1.0),
            };
            let grip = Vec3::new(0.35 * side, 0.90, -0.23);
            let direction = Vec3::new(0.15 * side, -0.96, -0.24).normalize();
            (name, root, grip, direction)
        }
    };
    commands
        .spawn((
            Name::new(name),
            ChildOf(parent),
            Transform::from_translation(grip)
                .with_rotation(Quat::from_rotation_arc(Vec3::Z, direction)),
            Visibility::default(),
        ))
        .id()
}

fn sync_worn(
    items: Res<Items>,
    mut commands: Commands,
    actors: Query<(Ref<Equipment>, Ref<WeaponStance>)>,
    assets: Res<GameAssets>,
    gltfs: Res<Assets<Gltf>>,
    mut attachments: Query<(Entity, &mut EquipmentAttachment)>,
    worn: Query<(&WornEquipment, &ChildOf)>,
) {
    for (bone, mut attachment) in &mut attachments {
        let Ok((equipment, stance)) = actors.get(attachment.owner) else {
            continue;
        };
        if !equipment.is_changed() && !stance.is_changed() && !attachment.is_added() {
            continue;
        }
        let holstered = *stance == WeaponStance::Holstered;
        let mount = attachment.holster.filter(|_| holstered).unwrap_or(bone);
        let slot = attachment.slot;
        let desired = equipment.get(slot).filter(|item| {
            items[*item].weapon().map(WeaponKind::handedness) == attachment.handedness
        });
        let existing = attachment.worn.and_then(|entity| {
            worn.get(entity)
                .ok()
                .map(|(piece, parent)| (entity, piece.item, parent.parent()))
        });
        if let Some((entity, item, parent)) = existing {
            if Some(item) == desired {
                if parent != mount {
                    commands.entity(entity).insert(ChildOf(mount));
                }
                continue;
            }
            commands.entity(entity).despawn();
        }
        attachment.worn = desired
            .map(|item| spawn_worn(&items, &mut commands, &assets, &gltfs, slot, item, mount));
    }
}

fn spawn_worn(
    items: &Items,
    commands: &mut Commands,
    assets: &GameAssets,
    gltfs: &Assets<Gltf>,
    slot: EquipmentSlot,
    item: ItemKind,
    mount: Entity,
) -> Entity {
    let scene = gltfs
        .get(&assets.worn[&item])
        .expect("worn equipment models must remain loaded after asset validation")
        .scenes[0]
        .clone();
    commands
        .spawn((
            Name::new(format!("Worn {}", items[item].name())),
            WornEquipment { slot, item },
            PendingScene,
            WorldAssetRoot(scene),
            Transform::default(),
            ChildOf(mount),
        ))
        .observe(scene_ready)
        .id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_items;
    #[test]
    fn insufficient_strength_leaves_both_hands_and_inventory_intact() {
        use EquipmentSlot::{MainHand, OffHand};

        let mut bag = Inventory::default();
        assert!(bag.add(ItemKind::Shortsword, 2));
        assert!(bag.add(ItemKind::Trident, 1));
        let mut gear = Equipment::default();
        assert_eq!(
            gear.equip(&mut bag, ItemKind::Shortsword, 5, test_items()),
            Ok(true)
        );
        assert_eq!(
            gear.equip(&mut bag, ItemKind::Shortsword, 5, test_items()),
            Ok(true)
        );
        for attempt in [
            gear.equip(&mut bag, ItemKind::Trident, 7, test_items()),
            gear.equip_in(&mut bag, MainHand, ItemKind::Trident, 7, test_items()),
        ] {
            assert!(attempt.is_err());
        }
        assert_eq!(gear.get(MainHand), Some(ItemKind::Shortsword));
        assert_eq!(gear.get(OffHand), Some(ItemKind::Shortsword));
        assert_eq!(bag.count(ItemKind::Trident), 1);

        assert_eq!(
            gear.equip(&mut bag, ItemKind::Trident, 8, test_items()),
            Ok(true)
        );
        assert_eq!(gear.get(MainHand), Some(ItemKind::Trident));
        assert_eq!(gear.get(OffHand), None);
        assert_eq!(bag.count(ItemKind::Shortsword), 2);
    }

    #[test]
    fn equip_swap_unequip_conserve_items_and_update_defense() {
        let mut bag = Inventory::default();
        let mut gear = Equipment::default();
        assert!(bag.add(ItemKind::FieldArmor, 2));
        assert!(bag.add(ItemKind::ReinforcedArmor, 1));
        assert!(bag.add(ItemKind::FieldHelmet, 1));
        assert_eq!(
            gear.equip(&mut bag, ItemKind::FieldArmor, 10, test_items()),
            Ok(true)
        );
        assert_eq!(
            gear.equip(&mut bag, ItemKind::FieldHelmet, 10, test_items()),
            Ok(true)
        );
        assert_eq!(gear.total_defense(test_items()), 3);
        assert_eq!(bag.count(ItemKind::FieldArmor), 1);
        assert_eq!(
            gear.equip(&mut bag, ItemKind::ReinforcedArmor, 10, test_items()),
            Ok(true)
        );
        assert_eq!(gear.total_defense(test_items()), 6);
        assert_eq!(bag.count(ItemKind::FieldArmor), 2);
        assert_eq!(bag.count(ItemKind::ReinforcedArmor), 0);
        assert_eq!(gear.unequip(&mut bag, EquipmentSlot::Body), Ok(true));
        assert_eq!(gear.total_defense(test_items()), 1);
        assert_eq!(bag.count(ItemKind::ReinforcedArmor), 1);
    }
    #[test]
    fn invalid_and_repeated_operations_preserve_ownership() {
        let mut bag = Inventory::default();
        let mut gear = Equipment::default();
        assert!(
            gear.equip(&mut bag, ItemKind::FieldArmor, 10, test_items())
                .is_err()
        );
        bag.add(ItemKind::Battery, 1);
        assert!(
            gear.equip(&mut bag, ItemKind::Battery, 10, test_items())
                .is_err()
        );
        bag.add(ItemKind::FieldHelmet, 2);
        gear.equip(&mut bag, ItemKind::FieldHelmet, 10, test_items())
            .expect("the owned field helmet must equip into the empty helmet slot");
        assert_eq!(
            gear.equip(&mut bag, ItemKind::FieldHelmet, 10, test_items()),
            Ok(false)
        );
        assert_eq!(bag.count(ItemKind::FieldHelmet), 1);
        assert_eq!(gear.unequip(&mut bag, EquipmentSlot::Body), Ok(false));
        gear.unequip(&mut bag, EquipmentSlot::Helmet)
            .expect("the bag must accept the unequipped helmet");
        assert_eq!(bag.count(ItemKind::FieldHelmet), 2);
        assert_eq!(gear.total_defense(test_items()), 0);
    }
    #[test]
    fn failed_swap_or_unequip_does_not_lose_equipment() {
        let mut bag = Inventory::default();
        let mut gear = Equipment::default();
        bag.add(ItemKind::FieldArmor, 1);
        gear.equip(&mut bag, ItemKind::FieldArmor, 10, test_items())
            .expect("the owned field armor must equip into the empty body slot");
        bag.add(ItemKind::FieldArmor, u32::MAX);
        bag.add(ItemKind::ReinforcedArmor, 1);
        assert!(
            gear.equip(&mut bag, ItemKind::ReinforcedArmor, 10, test_items())
                .is_err()
        );
        assert!(gear.unequip(&mut bag, EquipmentSlot::Body).is_err());
        assert_eq!(gear.get(EquipmentSlot::Body), Some(ItemKind::FieldArmor));
        assert_eq!(bag.count(ItemKind::ReinforcedArmor), 1);
    }
    #[test]
    fn dropping_bag_copies_does_not_remove_worn_items() {
        let mut bag = Inventory::default();
        let mut gear = Equipment::default();
        bag.add(ItemKind::FieldArmor, 2);
        gear.equip(&mut bag, ItemKind::FieldArmor, 10, test_items())
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

        let mut bag = Inventory::default();
        let mut equipment = Equipment::default();
        assert!(bag.add(ItemKind::Shortsword, 2));
        assert!(bag.add(ItemKind::FieldSpear, 1));
        let weight = bag.carried_weight(&equipment, test_items());
        assert_eq!(
            equipment.equip(&mut bag, ItemKind::Shortsword, 10, test_items()),
            Ok(true)
        );
        assert_eq!(
            equipment.equip(&mut bag, ItemKind::Shortsword, 10, test_items()),
            Ok(true)
        );
        assert_eq!(equipment.get(MainHand), Some(ItemKind::Shortsword));
        assert_eq!(equipment.get(OffHand), Some(ItemKind::Shortsword));
        assert_eq!(equipment.count(ItemKind::Shortsword), 2);
        assert_eq!(bag.carried_weight(&equipment, test_items()), weight);
        assert!(
            bag.drop_stack(ItemKind::Shortsword, 1, Some(Vec3::ZERO))
                .is_none()
        );
        assert!(
            equipment
                .equip_in(&mut bag, OffHand, ItemKind::FieldSpear, 10, test_items())
                .is_err()
        );

        assert_eq!(
            equipment.equip(&mut bag, ItemKind::FieldSpear, 10, test_items()),
            Ok(true)
        );
        assert_eq!(equipment.get(MainHand), Some(ItemKind::FieldSpear));
        assert_eq!(equipment.get(OffHand), None);
        assert!(equipment.off_hand_reserved(test_items()));
        assert_eq!(bag.count(ItemKind::Shortsword), 2);
        assert_eq!(bag.carried_weight(&equipment, test_items()), weight);
        assert!(
            equipment
                .equip_in(&mut bag, OffHand, ItemKind::Shortsword, 10, test_items())
                .is_err()
        );
        assert!(equipment.valid_slots(test_items()));

        assert_eq!(equipment.unequip(&mut bag, MainHand), Ok(true));
        assert_eq!(
            equipment.equip_in(&mut bag, OffHand, ItemKind::Shortsword, 10, test_items()),
            Ok(true)
        );
        assert_eq!(equipment.get(MainHand), None);
        assert_eq!(equipment.get(OffHand), Some(ItemKind::Shortsword));
    }

    #[test]
    fn a_swap_that_would_overflow_the_bag_preserves_both_hands() {
        let mut bag = Inventory::default();
        let mut equipment = Equipment::default();
        assert!(bag.add(ItemKind::Shortsword, 2));
        assert_eq!(
            equipment.equip(&mut bag, ItemKind::Shortsword, 10, test_items()),
            Ok(true)
        );
        assert_eq!(
            equipment.equip(&mut bag, ItemKind::Shortsword, 10, test_items()),
            Ok(true)
        );
        assert!(bag.add(ItemKind::Shortsword, u32::MAX - 1));
        assert!(bag.add(ItemKind::Trident, 1));

        let before = equipment.clone();
        assert!(
            equipment
                .equip(&mut bag, ItemKind::Trident, 10, test_items())
                .is_err()
        );
        assert_eq!(equipment, before);
        assert_eq!(bag.count(ItemKind::Shortsword), u32::MAX - 1);
        assert_eq!(bag.count(ItemKind::Trident), 1);
    }

    #[test]
    fn equipment_requests_require_an_inventory_or_equipment_menu_tab() {
        use bevy::state::app::StatesPlugin;
        let mut app = App::new();
        app.insert_resource(test_items().clone());
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<InventoryNotice>()
            .init_resource::<PlayerMenu>()
            .add_observer(equip)
            .add_observer(unequip);
        app.finish();
        app.cleanup();
        app.world_mut().spawn(Player::default());
        app.world_mut()
            .actor_data_mut::<Inventory>()
            .add(ItemKind::FieldArmor, 1);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.update();
        app.world_mut()
            .trigger(EquipRequest::Quick(ItemKind::FieldArmor));
        assert_eq!(
            app.world()
                .actor_data::<Equipment>()
                .get(EquipmentSlot::Body),
            None
        );
        assert_eq!(
            app.world()
                .actor_data::<Inventory>()
                .count(ItemKind::FieldArmor),
            1
        );
        app.world_mut()
            .resource_mut::<PlayerMenu>()
            .show(MenuTab::Equipment);
        app.world_mut()
            .trigger(EquipRequest::Quick(ItemKind::FieldArmor));
        assert_eq!(
            app.world()
                .actor_data::<Equipment>()
                .get(EquipmentSlot::Body),
            Some(ItemKind::FieldArmor)
        );
        app.world_mut()
            .resource_mut::<PlayerMenu>()
            .show(MenuTab::Inventory);
        app.world_mut().trigger(UnequipRequest(EquipmentSlot::Body));
        assert_eq!(
            app.world()
                .actor_data::<Equipment>()
                .get(EquipmentSlot::Body),
            None
        );
        assert_eq!(
            app.world()
                .actor_data::<Inventory>()
                .count(ItemKind::FieldArmor),
            1
        );
        app.world_mut()
            .resource_mut::<PlayerMenu>()
            .show(MenuTab::Equipment);
        app.world_mut()
            .trigger(EquipRequest::Quick(ItemKind::FieldArmor));
        assert_eq!(
            app.world()
                .actor_data::<Equipment>()
                .get(EquipmentSlot::Body),
            Some(ItemKind::FieldArmor)
        );
        app.world_mut()
            .resource_mut::<PlayerMenu>()
            .show(MenuTab::Inventory);
        app.world_mut().trigger(UnequipRequest(EquipmentSlot::Body));
        assert_eq!(
            app.world()
                .actor_data::<Equipment>()
                .get(EquipmentSlot::Body),
            None
        );
        assert_eq!(
            app.world()
                .actor_data::<Inventory>()
                .count(ItemKind::FieldArmor),
            1
        );
    }
}
