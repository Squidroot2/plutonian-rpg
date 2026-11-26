use super::*;

const RELAY_PIKE: &str = r#"
    (
        id: "relay_pike",
        name: "Relay pike",
        description: "A long pike of relay alloy.",
        weight_kg: 5,
        pickup_model: "models/trident.glb",
        pickup_size_m: (0.58, 0.23, 2.3),
        kind: Weapon(profile: Spear, minimum_strength: 1, worn_model: "models/trident_worn.glb",
            stats: (damage: 27, reach_m: 3.5, contact_radius_m: 0.12)),
    ),
"#;
const BASIN_ROACH: &str = r#"
    (
        id: "basin_roach",
        level: 1,
        name: "Basin roach",
        health: 51,
        xp: 82,
        loot: (item: "relay_pike", quantity: 2),
        behavior: Roach(
            speed_mps: 3.0,
            bite: (range_m: 1.7, windup_seconds: 0.5, recovery_seconds: 0.9, damage: 18),
        ),
    ),
"#;

const PLACED_CONTENT: &str = r#"
(
    starter_stock: [
        (chest: Lamp, stacks: []),
        (chest: Supply, stacks: []),
        (chest: Gear, stacks: [(item: "relay_pike", quantity: 1)]),
    ],
    encounter_assignments: [(encounter: "relay_roach", enemy: "basin_roach")],
    pickups: [(id: "relay_pike_cache", area: Surface, position: (5.5, 2.0, 3.0),
        stack: (item: "relay_pike", quantity: 1))],
    encounters: [(id: "basin_roach_site", enemy: "basin_roach", area: Surface,
        position: (7.0, 2.0, 110.0))],
)
"#;

fn append_entry(file: &str, entry: &str) -> String {
    let list = file
        .trim_end()
        .strip_suffix(']')
        .expect("fixture list ends at its closing bracket");
    format!("{list}{entry}]")
}

pub(crate) fn variant_items() -> Items {
    let file = append_entry(
        include_str!("../../data/items/catalog.item.ron"),
        RELAY_PIKE,
    );
    let items: Items = ron::from_str(&file).expect("variant items parse");
    items.validate().expect("variant items validate");
    items
}

pub(crate) fn variant_enemies(items: &Items) -> Enemies {
    let file = append_entry(
        include_str!("../../data/enemies/catalog.enemy.ron"),
        BASIN_ROACH,
    );
    let enemies: Enemies = ron::from_str(&file).expect("variant enemies parse");
    enemies.validate(items).expect("variant enemies validate");
    enemies
}

pub(crate) fn variant_placements(items: &Items, enemies: &Enemies) -> Placements {
    let placements: Placements = ron::from_str(PLACED_CONTENT).expect("variant placements parse");
    placements
        .validate(items, enemies)
        .expect("variant placements validate");
    placements
}

#[test]
fn malformed_catalogs_identify_the_invalid_entry_or_reference() {
    let items = variant_items();
    let enemies = variant_enemies(&items);

    let file = append_entry(
        include_str!("../../data/items/catalog.item.ron"),
        &RELAY_PIKE.replace("\"relay_pike\"", "\"trident\""),
    );
    let Err(error) = ron::from_str::<Items>(&file) else {
        panic!("duplicate item IDs must be rejected");
    };
    assert!(error.to_string().contains("Duplicate item ID trident"));

    let file = append_entry(
        include_str!("../../data/items/catalog.item.ron"),
        &RELAY_PIKE.replace("reach_m: 3.5", "reach_m: -3.5"),
    );
    let invalid: Items = ron::from_str(&file).expect("negative reach still parses");
    let error = invalid
        .validate()
        .expect_err("negative reach must be rejected");
    assert!(error.contains(Items::PATH) && error.contains("relay_pike"));

    let file = append_entry(
        include_str!("../../data/enemies/catalog.enemy.ron"),
        &BASIN_ROACH.replace("xp: 82", "xp: 0"),
    );
    let invalid: Enemies = ron::from_str(&file).expect("zero XP still parses");
    let error = invalid
        .validate(&items)
        .expect_err("enemies must award positive XP");
    assert!(error.contains(Enemies::PATH) && error.contains("basin_roach"));

    let file = PLACED_CONTENT.replace("item: \"relay_pike\"", "item: \"missing_pike\"");
    let invalid: Placements = ron::from_str(&file).expect("unknown references still parse");
    let error = invalid
        .validate(&items, &enemies)
        .expect_err("missing items are invalid");
    assert!(error.contains(Placements::PATH) && error.contains("missing_pike"));

    let file = PLACED_CONTENT.replace(
        "encounter: \"relay_roach\"",
        "encounter: \"relay_archive_hushcap\"",
    );
    let invalid: Placements = ron::from_str(&file).expect("incompatible assignment still parses");
    let error = invalid
        .validate(&items, &enemies)
        .expect_err("quest encounter must keep its archetype");
    assert!(error.contains("relay_archive_hushcap") && error.contains("Hushcap"));
}
