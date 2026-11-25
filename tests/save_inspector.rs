use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, UNIX_EPOCH},
};

const SAVE: &[u8] = include_bytes!("fixtures/inspector-v12.save");

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
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(!error.contains("panicked"));
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
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("No .save files"));
}
