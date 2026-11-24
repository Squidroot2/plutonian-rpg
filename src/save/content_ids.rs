use super::*;

// The save wire format stores item and encounter IDs as strings so authored
// IDs stay stable across code changes. `WireSnapshot` mirrors the runtime
// `Snapshot` field-for-field, with string IDs in place of the runtime ID
// types; collected pickups and credited cavern defeats travel alongside it
// as string sets in `encode`/`decode`.
#[derive(Clone, bitcode::Encode, bitcode::Decode)]
struct WireSnapshot {
    journal: SavedJournal,
    inventory: Inventory<String>,
    equipment: Equipment<String>,
    chests: ChestStorage<String>,
    dungeon: DungeonProgress,
    pickups: Vec<SavedPickup<String>>,
    nemi: SavedActor,
    nemi_state: NemiState,
    encounters: BTreeMap<String, SavedEncounter>,
    progression: SavedProgress,
    experience: SavedExperience<String>,
    guards_hostile: bool,
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

fn string_inventory(inventory: &Inventory) -> Inventory<String> {
    Inventory(
        inventory
            .0
            .iter()
            .map(|(&id, &quantity)| (string_item(id), quantity))
            .collect(),
    )
}

fn parse_inventory(inventory: Inventory<String>) -> io::Result<Inventory> {
    inventory
        .0
        .into_iter()
        .map(|(id, quantity)| Ok((parse_item(&id)?, quantity)))
        .collect::<io::Result<_>>()
        .map(Inventory)
}

pub(super) fn encode(snapshot: &Snapshot) -> Vec<u8> {
    let wire = WireSnapshot {
        journal: snapshot.journal.clone(),
        inventory: string_inventory(&snapshot.inventory),
        equipment: Equipment(
            snapshot
                .equipment
                .0
                .iter()
                .map(|(&slot, &id)| (slot, string_item(id)))
                .collect(),
        ),
        chests: ChestStorage(
            snapshot
                .chests
                .0
                .iter()
                .map(|(&chest, inventory)| (chest, string_inventory(inventory)))
                .collect(),
        ),
        dungeon: snapshot.dungeon,
        pickups: snapshot
            .pickups
            .iter()
            .map(|pickup| SavedPickup {
                stack: Pickup {
                    item: string_item(pickup.stack.item),
                    quantity: pickup.stack.quantity,
                },
                position: pickup.position,
            })
            .collect(),
        nemi: snapshot.nemi.clone(),
        nemi_state: snapshot.nemi_state.clone(),
        encounters: snapshot
            .encounters
            .iter()
            .map(|(&id, actor)| (string_encounter(id), actor.clone()))
            .collect(),
        progression: snapshot.progression.clone(),
        experience: SavedExperience {
            player: snapshot.experience.player,
            nemi_contributed: snapshot.experience.nemi_contributed,
            encounter_contributions: snapshot
                .experience
                .encounter_contributions
                .iter()
                .copied()
                .map(string_encounter)
                .collect(),
        },
        guards_hostile: snapshot.guards_hostile,
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
    let (wire, defeats, collected): (WireSnapshot, BTreeSet<String>, BTreeSet<String>) =
        bitcode::decode(bytes).map_err(invalid_save)?;
    let mut journal = wire.journal;
    let defeats = defeats
        .iter()
        .map(|id| match parse_encounter(id)? {
            EncounterId::Cave(id) => Ok(id),
            _ => Err(invalid_save(format!(
                "Invalid credited cavern encounter {id}"
            ))),
        })
        .collect::<io::Result<_>>()?;
    set_cavern_defeats(&mut journal, defeats)?;
    Ok(Snapshot {
        journal,
        inventory: parse_inventory(wire.inventory)?,
        equipment: Equipment(
            wire.equipment
                .0
                .into_iter()
                .map(|(slot, id)| Ok((slot, parse_item(&id)?)))
                .collect::<io::Result<_>>()?,
        ),
        chests: ChestStorage(
            wire.chests
                .0
                .into_iter()
                .map(|(chest, inventory)| Ok((chest, parse_inventory(inventory)?)))
                .collect::<io::Result<_>>()?,
        ),
        dungeon: wire.dungeon,
        pickups: wire
            .pickups
            .into_iter()
            .map(|pickup| {
                Ok(SavedPickup {
                    stack: Pickup {
                        item: parse_item(&pickup.stack.item)?,
                        quantity: pickup.stack.quantity,
                    },
                    position: pickup.position,
                })
            })
            .collect::<io::Result<_>>()?,
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
        collected_pickups: collected
            .into_iter()
            .map(|id| ContentId::parse(&id).map_err(invalid_save))
            .collect::<io::Result<_>>()?,
        caravan: BTreeMap::new(),
    })
}
