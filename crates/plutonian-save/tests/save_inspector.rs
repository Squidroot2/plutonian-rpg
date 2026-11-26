use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, UNIX_EPOCH},
};

const SAVE: &[u8] = include_bytes!("../../../tests/fixtures/inspector-v12.save");

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "plutonian-inspector-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create inspector test directory");
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn inspect(args: &[&Path], json: bool, data_home: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_plutonian-save"));
    command.arg("inspect");
    if json {
        command.arg("--json");
    }
    command.args(args);
    if let Some(home) = data_home {
        command.env("XDG_DATA_HOME", home);
    }
    command.output().expect("run inspector")
}

fn checksum(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_plutonian-save"))
        .args(args)
        .output()
        .expect("run save CLI")
}

#[test]
fn help_and_version_describe_the_save_cli() {
    for flag in ["--help", "-h"] {
        let top = cli(&[flag]);
        assert_eq!(top.status.code(), Some(0));
        let help = String::from_utf8_lossy(&top.stdout);
        assert!(help.contains("inspect") && help.contains("create"));

        let inspect = cli(&["inspect", flag]);
        assert_eq!(inspect.status.code(), Some(0));
        let help = String::from_utf8_lossy(&inspect.stdout);
        for expected in [
            "[PATH]",
            "--json",
            "save file",
            "directory",
            "default save directory",
        ] {
            assert!(help.contains(expected), "{help}");
        }
    }

    let version = cli(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert_eq!(
        version.stdout,
        concat!("plutonian-save ", env!("CARGO_PKG_VERSION"), "\n").as_bytes()
    );
}

#[test]
fn invalid_arguments_use_clap_usage_errors() {
    for args in [
        &["unknown-command"][..],
        &["inspect", "--unknown-flag"],
        &["inspect", "first.save", "second.save"],
    ] {
        let result = cli(args);
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.starts_with("error:") && error.contains("Usage:"),
            "{error}"
        );
    }
}

#[test]
fn real_game_save_reports_text_and_json_without_changing_any_files() {
    let dir = TestDirectory::new();
    let file = dir.0.join("game.save");
    let neighbor = dir.0.join("neighbor.txt");
    fs::write(&file, SAVE).expect("write saved fixture");
    fs::write(&neighbor, b"unchanged").expect("write neighbor");
    let before = checksum(&fs::read(&file).expect("read save"));
    let text = inspect(&[&file], false, None);
    let json = inspect(&[&file], true, None);
    assert!(text.status.success());
    assert!(json.status.success());
    let report = String::from_utf8(text.stdout).expect("text is UTF-8");
    let data: serde_json::Value = serde_json::from_slice(&json.stdout).expect("JSON report");
    assert_eq!(data["format_version"], 12);
    assert_eq!(data["player_name"], "Smoke");
    assert!(report.contains("Smoke") && report.contains("last_light") && report.contains("health"));
    assert!(data["journal"]["progress"].get("last_light").is_some());
    assert!(data["encounters"]["relay_roach"]["actor"]["health"].is_number());
    assert_eq!(
        checksum(&fs::read(&file).expect("read inspected save")),
        before
    );
    assert_eq!(fs::read(&neighbor).expect("read neighbor"), b"unchanged");
    assert_eq!(fs::read_dir(&dir.0).expect("list directory").count(), 2);
}

#[test]
fn explicit_and_default_directories_select_the_newest_save() {
    let dir = TestDirectory::new();
    let saves = dir.0.join("plutonian-rpg");
    fs::create_dir(&saves).expect("save directory");
    let old = saves.join("old.save");
    let new = saves.join("new.save");
    fs::write(&old, b"bad header").expect("old save");
    fs::write(&new, SAVE).expect("new save");
    for (path, seconds) in [(&old, 100), (&new, 200)] {
        fs::File::options()
            .write(true)
            .open(path)
            .expect("open save")
            .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
            .expect("set save time");
    }
    assert!(inspect(&[&saves], true, None).status.success());
    assert!(inspect(&[], true, Some(&dir.0)).status.success());
    fs::File::options()
        .write(true)
        .open(&old)
        .expect("open old save")
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(300)))
        .expect("make bad save newest");
    let bad = inspect(&[&saves], false, None);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("Invalid or unsupported save header"));
}

#[test]
fn invalid_and_missing_saves_fail_without_panicking() {
    let dir = TestDirectory::new();
    let file = dir.0.join("invalid.save");
    let mut wrong_version = SAVE.to_vec();
    wrong_version[4] = 11;
    for bytes in [
        wrong_version.as_slice(),
        &SAVE[..6],
        &vec![0; 1024 * 1024 + 1],
    ] {
        fs::write(&file, bytes).expect("write invalid save");
        let result = inspect(&[&file], true, None);
        assert_eq!(result.status.code(), Some(1));
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.starts_with("Could not inspect save: "), "{error}");
        assert!(
            error.contains("Invalid")
                || error.contains("EOF")
                || error.contains("large")
                || error.contains("decode")
                || error.contains("end"),
            "{error}"
        );
    }
    fs::remove_file(&file).expect("remove invalid save");
    let result = inspect(&[&dir.0], false, None);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).starts_with("Could not inspect save: "));
    assert!(String::from_utf8_lossy(&result.stderr).contains("No .save files"));
}

const GOLDEN_JSON: &str =
    include_str!("../../../golden-saves/after-old-relay/after-old-relay.json");

fn create(input: &Path, output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_plutonian-save"))
        .args(["create"])
        .arg(input)
        .arg("--output")
        .arg(output)
        .env(
            "BEVY_ASSET_ROOT",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        )
        .output()
        .expect("run save creator")
}

#[test]
fn golden_fixture_and_real_bed_save_round_trip_through_create() {
    let dir = TestDirectory::new();
    let original = dir.0.join("original.save");
    fs::write(&original, SAVE).expect("write captured bed save");
    let inspected = inspect(&[&original], true, None);
    assert!(inspected.status.success());

    for source in [GOLDEN_JSON.as_bytes(), &inspected.stdout] {
        let input = dir.0.join("scenario.json");
        let output = dir.0.join("created.save");
        fs::write(&input, source).expect("write input JSON");
        let result = create(&input, &output);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let round_trip = inspect(&[&output], true, None);
        assert!(round_trip.status.success());
        let parse = |bytes: &[u8]| {
            serde_json::from_slice::<serde_json::Value>(bytes).expect("parse inspection JSON")
        };
        assert_eq!(parse(source), parse(&round_trip.stdout));
    }
}

#[test]
fn malformed_and_invalid_fixtures_create_no_output() {
    use serde_json::{Value, json};

    let dir = TestDirectory::new();
    let input = dir.0.join("invalid.json");
    let output = dir.0.join("generated").join("invalid.save");
    let fixture: Value = serde_json::from_str(GOLDEN_JSON).expect("valid golden fixture JSON");
    let mut wrong_version = fixture.clone();
    wrong_version["format_version"] = json!(0);
    let mut unknown_item = fixture.clone();
    unknown_item["inventory"]["unknown_item"] = json!(1);
    let mut invalid_stack = fixture.clone();
    invalid_stack["inventory"]["battery"] = json!(0);
    let mut unknown_encounter = fixture.clone();
    unknown_encounter["encounters"]["unknown_encounter"] =
        fixture["encounters"]["relay_roach"].clone();
    let mut unknown_pickup = fixture.clone();
    unknown_pickup["collected_pickups"] = json!(["unknown_pickup"]);
    let mut unknown_quest = fixture.clone();
    unknown_quest["journal"]["progress"]["unknown_quest"] =
        fixture["journal"]["progress"]["last_light"].clone();

    let mut cases = vec![("{".to_owned(), "EOF")];
    cases.extend(
        [
            (wrong_version, "Unsupported save format version"),
            (unknown_item, "unknown_item"),
            (invalid_stack, "Invalid save contents"),
            (unknown_encounter, "Unknown encounter ID unknown_encounter"),
            (unknown_pickup, "Unknown collected pickup ID unknown_pickup"),
            (unknown_quest, "Unknown saved quest unknown_quest"),
        ]
        .map(|(data, message)| (data.to_string(), message)),
    );

    for (source, message) in cases {
        fs::write(&input, source).expect("write invalid JSON fixture");
        let result = create(&input, &output);
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains(message), "{error}");
        assert!(!output.parent().expect("output has parent").exists());
    }
}

#[test]
fn create_requires_an_explicit_output_path() {
    let dir = TestDirectory::new();
    let input = dir.0.join("scenario.json");
    fs::write(&input, GOLDEN_JSON).expect("write valid input");
    let result = Command::new(env!("CARGO_BIN_EXE_plutonian-save"))
        .arg("create")
        .arg(&input)
        .env("XDG_DATA_HOME", &dir.0)
        .output()
        .expect("run save creator without output");
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--output"));
    assert_eq!(
        fs::read_dir(&dir.0).expect("list data directory").count(),
        1
    );
}

#[test]
fn schema_subcommand_describes_the_inspection_document() {
    let output = cli(&["schema"]);
    assert!(output.status.success());
    let schema: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("schema output is JSON");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["properties"]["format_version"]["type"], "integer");
    assert_eq!(schema["additionalProperties"], false);

    let fixture: serde_json::Value =
        serde_json::from_str(GOLDEN_JSON).expect("golden fixture is JSON");
    assert_eq!(fixture["format_version"], 12);
    assert!(fixture["format_version"].is_i64());
}
