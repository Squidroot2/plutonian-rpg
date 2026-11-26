use super::*;

// The save wire format stores item and encounter IDs as strings so authored
// IDs stay stable across code changes. `WireSnapshot` mirrors the runtime
// `Snapshot` field-for-field, with string IDs in place of the runtime ID
// types; collected pickups and credited cavern defeats travel alongside it
// as string sets in `encode`/`decode`.
#[derive(
    Clone,
    bitcode::Encode,
    bitcode::Decode,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[schemars(deny_unknown_fields)]
pub(super) struct WireSnapshot {
    player_name: String,
    journal: SavedJournal,
    inventory: Inventory<String>,
    equipment: Equipment<String>,
    chests: ChestStorage<String>,
    dungeons: BTreeMap<String, DungeonProgress>,
    pickups: Vec<SavedPickup<String>>,
    encounters: BTreeMap<String, SavedEncounter>,
    progression: SavedProgress,
    experience: SavedExperience<String, String>,
    world: SavedWorld,
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

pub(super) fn wire_snapshot(snapshot: &Snapshot) -> WireSnapshot {
    WireSnapshot {
        player_name: snapshot.player_name.clone(),
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
        dungeons: snapshot
            .dungeons
            .iter()
            .map(|(id, progress)| (id.as_str().to_owned(), *progress))
            .collect(),
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
        encounters: snapshot
            .encounters
            .iter()
            .map(|(&id, actor)| (string_encounter(id), actor.clone()))
            .collect(),
        progression: snapshot.progression.clone(),
        experience: SavedExperience {
            player: snapshot.experience.player,
            npc_contributions: snapshot
                .experience
                .npc_contributions
                .iter()
                .map(|id| id.key().to_owned())
                .collect(),
            encounter_contributions: snapshot
                .experience
                .encounter_contributions
                .iter()
                .copied()
                .map(string_encounter)
                .collect(),
        },
        world: snapshot.world.clone(),
    }
}

pub(super) fn encode(snapshot: &Snapshot) -> Vec<u8> {
    let wire = wire_snapshot(snapshot);
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
    into_snapshot(wire, defeats, collected)
}

pub(super) fn into_snapshot(
    wire: WireSnapshot,
    defeats: BTreeSet<String>,
    collected: BTreeSet<String>,
) -> io::Result<Snapshot> {
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
        player_name: wire.player_name,
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
        dungeons: wire
            .dungeons
            .into_iter()
            .map(|(id, progress)| Ok((ContentId::parse(&id).map_err(invalid_save)?, progress)))
            .collect::<io::Result<_>>()?,
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
        encounters: wire
            .encounters
            .into_iter()
            .map(|(id, actor)| Ok((parse_encounter(&id)?, actor)))
            .collect::<io::Result<_>>()?,
        progression: wire.progression,
        experience: SavedExperience {
            player: wire.experience.player,
            npc_contributions: wire
                .experience
                .npc_contributions
                .into_iter()
                .map(|id| {
                    NpcId::persistent_id(&id)
                        .ok_or_else(|| invalid_save(format!("Unknown persistent NPC {id}")))
                })
                .collect::<io::Result<_>>()?,
            encounter_contributions: wire
                .experience
                .encounter_contributions
                .into_iter()
                .map(|id| parse_encounter(&id))
                .collect::<io::Result<_>>()?,
        },
        world: wire.world,
        collected_pickups: collected
            .into_iter()
            .map(|id| ContentId::parse(&id).map_err(invalid_save))
            .collect::<io::Result<_>>()?,
        npcs: BTreeMap::new(),
    })
}
