use super::{Failures, Model, Node, expectations::*, geometry::world_bounds};
use crate::{
    crossing::{
        GATE_ENTITY_NAME, GATE_MODEL, GATE_SIZE, VELLIK_LEGS, VELLIK_MODEL, WAGON_MODEL,
        WAGON_WHEEL_PREFIX,
    },
    loading::{OUTPOST_COLLISION_MODEL, OUTPOST_MODEL},
};
use bevy::math::Vec3;
use std::collections::{BTreeMap, BTreeSet};

type Models = BTreeMap<String, Model>;
type NamedNodes<'a> = BTreeMap<&'a str, &'a Node>;

pub(super) fn check(paths: &BTreeSet<String>, models: &Models, failures: &mut Failures) {
    for path in paths {
        let visual = if path == OUTPOST_COLLISION_MODEL {
            OUTPOST_MODEL.to_owned()
        } else if let Some(stem) = path.strip_suffix("_collision.glb") {
            format!("{stem}.glb")
        } else {
            continue;
        };
        if !paths.contains(&visual) {
            failures.add(
                path,
                format!("collision has no referenced visual {visual:?}"),
            );
        }
        if let (Some(visual_model), Some(collision)) = (models.get(&visual), models.get(path)) {
            check_pair(&visual, visual_model, path, collision, failures);
        }
    }
    for requirements in AREAS {
        let visual = requirements.visual;
        let collision = if visual == OUTPOST_MODEL {
            OUTPOST_COLLISION_MODEL.to_owned()
        } else {
            visual.replace(".glb", "_collision.glb")
        };
        if paths.contains(visual) && !paths.contains(&collision) {
            failures.add(
                visual,
                format!("expected collision companion {collision:?}"),
            );
        }
    }
    check_outpost(models, failures);
    check_crossing_models(models, failures);
}

fn check_pair(
    visual_path: &str,
    visual: &Model,
    collision_path: &str,
    collision: &Model,
    failures: &mut Failures,
) {
    let visuals = named_nodes(visual_path, visual, failures);
    let solids = named_nodes(collision_path, collision, failures);
    for (name, node) in &solids {
        let Some(visual_node) = visuals.get(name) else {
            failures.add(
                collision_path,
                format!("{name}: no matching visual in {visual_path}"),
            );
            continue;
        };
        failures.require(
            node.transform.matrix() == visual_node.transform.matrix(),
            collision_path,
            format!("{name}: collider misaligned with {visual_path}"),
        );
        let collision_bounds = node.mesh.map(|mesh| &collision.meshes[mesh]);
        let visual_bounds = visual_node.mesh.map(|mesh| &visual.meshes[mesh]);
        failures.require(
            collision_bounds == visual_bounds,
            collision_path,
            format!("{name}: collider bounds differ from {visual_path}"),
        );
    }
    for (name, node) in &visuals {
        if node.mesh.is_some()
            && (name.contains(SOLID_NAME_MARKER)
                || visual_path == OUTPOST_MODEL && is_waystone(name))
        {
            require_body(collision_path, &solids, name, failures);
        }
    }
    if let Some(required) = AREAS.iter().find(|area| area.visual == visual_path) {
        for name in required.bodies {
            require_body(collision_path, &solids, name, failures);
        }
        for home in required.homes {
            check_home(
                visual_path,
                collision_path,
                &visuals,
                &solids,
                home,
                failures,
            );
        }
    }
}

fn named_nodes<'a>(path: &str, model: &'a Model, failures: &mut Failures) -> NamedNodes<'a> {
    let mut nodes = NamedNodes::new();
    for (index, node) in model.nodes.iter().enumerate() {
        match node.name.as_deref() {
            Some(name) if !nodes.contains_key(name) => {
                nodes.insert(name, node);
            }
            Some(name) => failures.add(path, format!("duplicate node {name:?}")),
            None => failures.add(path, format!("node {index} is unnamed")),
        }
    }
    nodes
}

fn require_body(path: &str, bodies: &NamedNodes, name: &str, failures: &mut Failures) {
    failures.require(
        bodies.get(name).is_some_and(|node| node.mesh.is_some()),
        path,
        format!("missing collider {name:?}"),
    );
}

fn check_home(
    visual_path: &str,
    collision_path: &str,
    visuals: &NamedNodes,
    solids: &NamedNodes,
    home: &Home,
    failures: &mut Failures,
) {
    let room = format!("{VILLAGE_PREFIX}{} ", home.name);
    let bed_prefix = format!("{room}bed ");
    let beds: BTreeSet<_> = visuals
        .iter()
        .filter(|(name, node)| {
            node.mesh.is_some() && name.starts_with(&bed_prefix) && name.ends_with(" frame")
        })
        .map(|(&name, _)| name.to_owned())
        .collect();
    let expected: BTreeSet<_> = (1..=home.beds)
        .map(|i| format!("{room}bed {i} frame"))
        .collect();
    failures.require(
        beds == expected,
        visual_path,
        format!(
            "{}: expected {} bed frames (found {beds:?})",
            home.name, home.beds
        ),
    );
    for part in HOME_BODY_SUFFIXES {
        require_body(collision_path, solids, &format!("{room}{part}"), failures);
    }
}

fn is_waystone(name: &str) -> bool {
    name.starts_with(WAYSTONE_PREFIX) && !name.starts_with(WAYSTONE_LAMP_PREFIX)
}

fn check_outpost(models: &Models, failures: &mut Failures) {
    if let Some(model) = models.get(OUTPOST_MODEL) {
        let meshes = model
            .nodes
            .iter()
            .filter(|node| node.mesh.is_some())
            .filter_map(|node| node.name.as_deref());
        failures.require(
            meshes.clone().any(is_waystone),
            OUTPOST_MODEL,
            "expected trail pillar bodies",
        );
        for name in meshes {
            if RETIRED_OUTPOST_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                failures.add(OUTPOST_MODEL, format!("unexpected retired mesh {name:?}"));
            }
        }
    }
    for path in [OUTPOST_MODEL, OUTPOST_COLLISION_MODEL] {
        if let Some(model) = models.get(path) {
            failures.require(
                model.node_index(GATE_ENTITY_NAME).is_none(),
                path,
                "gate leaf must be exported separately",
            );
        }
    }
}

fn check_crossing_models(models: &Models, failures: &mut Failures) {
    if let Some(gate) = models.get(GATE_MODEL) {
        let count = gate
            .node_index(GATE_LEAF)
            .map(|node| gate.nodes[node].children.len());
        failures.require(
            count == Some(GATE_CHILDREN),
            GATE_MODEL,
            format!("expected {GATE_LEAF:?} with {GATE_CHILDREN} children (found {count:?})"),
        );
        let high = GATE_SIZE / 2.0;
        let low = (-high).with_z(GATE_BACK_Z_M);
        match world_bounds(gate) {
            Ok(bounds) => {
                let min = Vec3::from_array(bounds.min);
                let max = Vec3::from_array(bounds.max);
                failures.require(
                    (min - low).abs().max_element() < POSITION_TOLERANCE_M
                        && (max - high).abs().max_element() < POSITION_TOLERANCE_M,
                    GATE_MODEL,
                    format!("expected gate bounds {low:?} to {high:?} (found {min:?} to {max:?})"),
                );
            }
            Err(error) => failures.add(GATE_MODEL, error),
        }
    }
    if let Some(vellik) = models.get(VELLIK_MODEL) {
        let legs: Vec<_> = vellik
            .nodes
            .iter()
            .filter(|node| {
                node.name
                    .as_deref()
                    .is_some_and(|name| name.starts_with(VELLIK_LEG_PREFIX))
            })
            .collect();
        let expected = VELLIK_LEGS.as_flattened();
        failures.require(
            legs.len() == expected.len(),
            VELLIK_MODEL,
            format!("expected {} leg nodes", expected.len()),
        );
        for &name in expected {
            require_moving_part(
                VELLIK_MODEL,
                name,
                vellik.node_index(name).map(|index| &vellik.nodes[index]),
                failures,
            );
        }
    }
    if let Some(wagon) = models.get(WAGON_MODEL) {
        let wheels: Vec<_> = wagon
            .nodes
            .iter()
            .filter_map(|node| {
                let name = node.name.as_deref()?;
                name.starts_with(WAGON_WHEEL_PREFIX).then_some((name, node))
            })
            .collect();
        failures.require(
            wheels.len() == WAGON_WHEELS,
            WAGON_MODEL,
            format!("expected {WAGON_WHEELS} wheel nodes"),
        );
        for (name, node) in wheels {
            require_moving_part(WAGON_MODEL, name, Some(node), failures);
        }
    }
}

fn require_moving_part(path: &str, name: &str, node: Option<&Node>, failures: &mut Failures) {
    failures.require(
        node.is_some_and(|node| !node.children.is_empty()),
        path,
        format!("expected {name:?} with children"),
    );
}
