use std::{fs, path::Path, process::Command};

#[test]
fn rejects_a_placement_that_references_an_unknown_item() {
    let root = std::env::temp_dir().join(format!(
        "plutonian-validate-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    copy_data(&root);
    let placements = root.join("world/catalog.placement.ron");
    let content = fs::read_to_string(&placements).expect("read fixture placements");
    let broken = content.replacen("battery", "missing_item", 1);
    assert_ne!(content, broken, "fixture must contain a battery reference");
    fs::write(placements, broken).expect("write broken fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_plutonian-validate"))
        .arg("--data-root")
        .arg(&root)
        .output()
        .expect("run validator");
    let _ = fs::remove_dir_all(&root);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("world/catalog.placement.ron"), "{stderr}");
    assert!(stderr.contains("missing_item"), "{stderr}");
}

fn copy_data(root: &Path) {
    for directory in ["items", "enemies", "npcs", "world", "dialogue", "quests"] {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data")
            .join(directory);
        let destination = root.join(directory);
        fs::create_dir_all(&destination).expect("create fixture directory");
        for entry in fs::read_dir(source).expect("read shipped catalog directory") {
            let entry = entry.expect("read catalog entry");
            fs::copy(entry.path(), destination.join(entry.file_name()))
                .expect("copy catalog fixture");
        }
    }
}
