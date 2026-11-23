use super::*;

macro_rules! legacy_items {
    ($($item:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, bitcode::Decode)]
        #[cfg_attr(test, derive(bitcode::Encode))]
        pub(super) enum LegacyItem {
            $($item),+
        }

        impl LegacyItem {
            pub(super) fn current(self) -> ItemKind {
                match self { $(Self::$item => ItemKind::$item),+ }
            }

            #[cfg(test)]
            pub(super) fn from_current(id: ItemKind) -> Self {
                match id {
                    $(ItemKind::$item => Self::$item,)+
                    _ => panic!("legacy fixtures use shipped item IDs"),
                }
            }
        }
    };
}

// Preserve discriminant order and packing for the original inventory, equipment,
// chest and pickup codecs. Earlier schemas used the same bitcode packing width.
legacy_items!(
    Battery,
    Scrap,
    MethanePouch,
    BeaconCore,
    FieldArmor,
    ReinforcedArmor,
    FieldHelmet,
    ReinforcedHelmet,
    FieldSpear,
    Trident,
    LargeBoulder,
    WormMeat,
    RoachMeat,
    Shortsword,
    Mycelium,
);

pub(super) type V10Snapshot =
    Snapshot<Equipment<String>, SavedProgress, SavedExperience<String>, String, bool, String>;

impl<E, P, X, I: Ord, G, K: Ord> Snapshot<E, P, X, I, G, K> {
    /// Maps the places a checkpoint stores item IDs. The caller supplies the
    /// decoded equipment separately because v1 used different slot keys.
    pub(super) fn map_items<T: Ord, F>(
        self,
        mut map: impl FnMut(K) -> Result<T, F>,
    ) -> Result<Snapshot<E, P, X, I, G, T>, F> {
        let mut map_inventory = |inventory: Inventory<K>| {
            inventory
                .0
                .into_iter()
                .map(|(item, qty)| Ok((map(item)?, qty)))
                .collect::<Result<_, F>>()
                .map(Inventory)
        };
        let inventory = map_inventory(self.inventory)?;
        let chests = ChestStorage(
            self.chests
                .0
                .into_iter()
                .map(|(chest, inventory)| Ok((chest, map_inventory(inventory)?)))
                .collect::<Result<_, F>>()?,
        );
        let pickups = self
            .pickups
            .into_iter()
            .map(|pickup| {
                Ok(SavedPickup {
                    stack: Pickup {
                        item: map(pickup.stack.item)?,
                        quantity: pickup.stack.quantity,
                    },
                    position: pickup.position,
                })
            })
            .collect::<Result<_, F>>()?;
        Ok(Snapshot {
            journal: self.journal,
            inventory,
            equipment: self.equipment,
            chests,
            dungeon: self.dungeon,
            pickups,
            nemi: self.nemi,
            nemi_state: self.nemi_state,
            encounters: self.encounters,
            progression: self.progression,
            experience: self.experience,
            guards_hostile: self.guards_hostile,
            add_shortswords_on_spawn: self.add_shortswords_on_spawn,
            collected_pickups: self.collected_pickups,
            caravan: self.caravan,
            legacy_nemi_health: self.legacy_nemi_health,
        })
    }
}

fn string_item(id: ItemKind) -> String {
    id.as_str().to_owned()
}

fn string_encounter(id: EncounterId) -> String {
    id.key().as_str().to_owned()
}

fn parse_item(id: &str) -> io::Result<ItemKind> {
    ItemKind::parse(id).map_err(invalid_save)
}

fn parse_encounter(id: &str) -> io::Result<EncounterId> {
    ContentId::parse(id)
        .map(EncounterId::from_key)
        .map_err(invalid_save)
}

pub(super) fn encode(snapshot: &Snapshot) -> Vec<u8> {
    let wire = snapshot
        .clone()
        .map_items(|id| Ok::<_, std::convert::Infallible>(string_item(id)))
        .expect("validated runtime item IDs always produce strings");
    let wire = Snapshot {
        journal: wire.journal,
        inventory: wire.inventory,
        equipment: Equipment(
            wire.equipment
                .0
                .into_iter()
                .map(|(slot, id)| (slot, string_item(id)))
                .collect(),
        ),
        chests: wire.chests,
        dungeon: wire.dungeon,
        pickups: wire.pickups,
        nemi: wire.nemi,
        nemi_state: wire.nemi_state,
        encounters: wire
            .encounters
            .into_iter()
            .map(|(id, actor)| (string_encounter(id), actor))
            .collect(),
        progression: wire.progression,
        experience: SavedExperience {
            player: wire.experience.player,
            nemi_contributed: wire.experience.nemi_contributed,
            encounter_contributions: wire
                .experience
                .encounter_contributions
                .into_iter()
                .map(string_encounter)
                .collect(),
        },
        guards_hostile: wire.guards_hostile,
        add_shortswords_on_spawn: false,
        collected_pickups: BTreeSet::new(),
        caravan: BTreeMap::new(),
        legacy_nemi_health: false,
    };
    let pickups: BTreeSet<_> = snapshot
        .collected_pickups
        .iter()
        .map(|id| id.as_str())
        .collect();
    let defeats: BTreeSet<_> = snapshot
        .journal
        .cavern_defeats
        .iter()
        .copied()
        .map(|id| string_encounter(EncounterId::Cave(id)))
        .collect();
    bitcode::encode(&(wire, defeats, pickups))
}

pub(super) fn decode(bytes: &[u8]) -> io::Result<Snapshot> {
    let (wire, defeats, collected): (V10Snapshot, BTreeSet<String>, BTreeSet<String>) =
        bitcode::decode(bytes).map_err(invalid_save)?;
    let mut wire = wire.map_items(|id| parse_item(&id))?;
    let defeats = defeats
        .iter()
        .map(|id| match parse_encounter(id)? {
            EncounterId::Cave(id) => Ok(id),
            _ => Err(invalid_save(format!(
                "Invalid credited cavern encounter {id}"
            ))),
        })
        .collect::<io::Result<_>>()?;
    set_cavern_defeats(&mut wire.journal, defeats)?;
    Ok(Snapshot {
        journal: wire.journal,
        inventory: wire.inventory,
        equipment: Equipment(
            wire.equipment
                .0
                .into_iter()
                .map(|(slot, id)| Ok((slot, parse_item(&id)?)))
                .collect::<io::Result<_>>()?,
        ),
        chests: wire.chests,
        dungeon: wire.dungeon,
        pickups: wire.pickups,
        nemi: wire.nemi,
        nemi_state: wire.nemi_state,
        encounters: wire
            .encounters
            .into_iter()
            .map(|(id, actor)| Ok((parse_encounter(&id)?, actor)))
            .collect::<io::Result<_>>()?,
        progression: wire.progression,
        experience: SavedExperience {
            player: wire.experience.player,
            nemi_contributed: wire.experience.nemi_contributed,
            encounter_contributions: wire
                .experience
                .encounter_contributions
                .into_iter()
                .map(|id| parse_encounter(&id))
                .collect::<io::Result<_>>()?,
        },
        guards_hostile: wire.guards_hostile,
        add_shortswords_on_spawn: false,
        collected_pickups: collected
            .into_iter()
            .map(|id| ContentId::parse(&id).map_err(invalid_save))
            .collect::<io::Result<_>>()?,
        caravan: BTreeMap::new(),
        legacy_nemi_health: false,
    })
}
