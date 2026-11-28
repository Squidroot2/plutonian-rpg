//! General configuration file shared by game settings.
//!
//! The file is TOML so players can edit it by hand.
//! Keybinds and top-level settings share the same document.
//! A missing file is a first run, not an error: loading falls back
//! to defaults, and saving recreates the file.
//! A malformed file stops the game at startup with a panic naming
//! the file, the specific problem, and the delete-to-reset remedy.

use crate::controls::{Binding, Control, Keybinds};
use az::CheckedAs;
use bevy::prelude::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use strum::VariantArray;

pub(crate) const CONFIG_FILE_NAME: &str = "config.toml";
const KEYBINDS_TABLE: &str = "keybinds";
const MASTER_VOLUME: &str = "master_volume";
pub(crate) const DEFAULT_MASTER_VOLUME: f32 = 1.0;

const CONFIG_HEADER: &str = "# Plutonian configuration. Safe to edit by hand.\n# Deleting this file resets everything to defaults.\n# An invalid file stops the game at startup with an error naming the problem.\n";

/// The resolved config file, if configuration is available on this machine.
#[derive(Resource, Clone)]
pub(crate) struct ConfigFile(pub(crate) Option<PathBuf>);

impl ConfigFile {
    pub(crate) fn save(&self, bindings: &Keybinds) {
        save_keybinds(self.0.as_deref(), bindings);
    }

    pub(crate) fn save_master_volume(&self, volume: f32) {
        save_value(
            self.0.as_deref(),
            MASTER_VOLUME,
            toml::Value::Float(f64::from(volume)),
        );
    }
}

pub(crate) fn load_master_volume(path: Option<&Path>) -> f32 {
    let Some(path) = path.filter(|path| path.exists()) else {
        return DEFAULT_MASTER_VOLUME;
    };
    let result: Result<f32, String> = (|| {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("could not read the file: {error}"))?;
        let document: toml::Table = text
            .parse()
            .map_err(|error| format!("TOML parse error: {error}"))?;
        let Some(value) = document.get(MASTER_VOLUME) else {
            return Ok(DEFAULT_MASTER_VOLUME);
        };
        let number = value
            .as_float()
            .or_else(|| {
                value
                    .as_integer()
                    .map(|value| value.checked_as::<f64>().expect("TOML integers fit in f64"))
            })
            .ok_or_else(|| format!("{MASTER_VOLUME} must be a number"))?;
        Ok(number
            .clamp(0.0, 1.0)
            .checked_as::<f32>()
            .expect("clamped volume fits in f32"))
    })();
    result.unwrap_or_else(|problem| broken_config(path, problem))
}

/// Resolve `$XDG_CONFIG_HOME/plutonian-rpg/config.toml` when
/// `XDG_CONFIG_HOME` is absolute, else `~/.config/plutonian-rpg/config.toml`
/// when `HOME` is absolute, else `None`.
///
/// This mirrors `data_path` in `src/save.rs`, which resolves the save
/// directory under `$XDG_DATA_HOME` with the same absolute-path rules.
pub(crate) fn config_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".config"))
        })
        .map(|config| config.join("plutonian-rpg").join(CONFIG_FILE_NAME))
}

/// Pick the config file for this run: a unique temporary file for smoke
/// runs, the override directory when one is given, else the normal path.
/// Smoke runs stay isolated from the player's real configuration.
pub(crate) fn select_config_file(
    smoke: bool,
    override_directory: Option<PathBuf>,
) -> Option<PathBuf> {
    if smoke {
        return Some(
            std::env::temp_dir()
                .join(format!("plutonian-config-smoke-{}", unique_suffix()))
                .join(CONFIG_FILE_NAME),
        );
    }
    if let Some(directory) = override_directory {
        return Some(directory.join(CONFIG_FILE_NAME));
    }
    config_path()
}

/// Load the configured keybinds, falling back to defaults when no file
/// exists or configuration is unavailable.
/// A broken file panics with the file path, the specific problem,
/// and the delete-to-reset remedy.
pub(crate) fn load_keybinds(path: Option<&Path>) -> Keybinds {
    let Some(path) = path else {
        return Keybinds::default();
    };
    if !path.exists() {
        return Keybinds::default();
    }
    match try_load_keybinds(path) {
        Ok(bindings) => bindings,
        Err(problem) => broken_config(path, problem),
    }
}

fn broken_config(path: &Path, problem: impl std::fmt::Display) -> ! {
    panic!(
        "Broken config {}: {problem}. Fix the entry, or delete the file to reset to defaults.",
        path.display()
    )
}

fn try_load_keybinds(path: &Path) -> Result<Keybinds, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("could not read the file: {error}"))?;
    parse_keybinds(&text)
}

/// Parse a config document, ignoring every top-level table except
/// `[keybinds]`. Missing controls keep their defaults so hand edits
/// and future controls stay forgiving.
fn parse_keybinds(text: &str) -> Result<Keybinds, String> {
    let document: toml::Table = text
        .parse()
        .map_err(|error| format!("TOML parse error: {error}"))?;
    let mut bindings = Keybinds::default();
    let Some(table) = document.get(KEYBINDS_TABLE) else {
        return Ok(bindings);
    };
    let table = table
        .as_table()
        .ok_or_else(|| format!("[{KEYBINDS_TABLE}] must be a table"))?;
    for (name, value) in table {
        let (control, primary, secondary) = parse_keybinds_entry(name, value)?;
        bindings.apply_config(control, primary, secondary);
    }
    Ok(bindings)
}

fn parse_keybinds_entry(
    name: &str,
    value: &toml::Value,
) -> Result<(Control, Option<Binding>, Option<Binding>), String> {
    let control =
        Control::from_config_key(name).ok_or_else(|| format!("unknown control '{name}'"))?;
    match value {
        toml::Value::String(name) => Ok((control, parse_binding(control, name)?, None)),
        toml::Value::Array(names) => parse_binding_list(control, names),
        _ => Err(format!(
            "invalid binding for control '{}': expected a string or an array of binding strings",
            control.config_key()
        )),
    }
}

fn parse_binding_list(
    control: Control,
    names: &[toml::Value],
) -> Result<(Control, Option<Binding>, Option<Binding>), String> {
    let mut parsed = Vec::with_capacity(names.len());
    for name in names {
        let Some(name) = name.as_str() else {
            return Err(format!(
                "invalid binding for control '{}': expected a string or an array of binding strings",
                control.config_key()
            ));
        };
        parsed.push(parse_binding(control, name)?);
    }
    match parsed.as_slice() {
        [primary] => Ok((control, *primary, None)),
        [primary, secondary] => Ok((control, *primary, *secondary)),
        _ => Err(format!(
            "invalid binding for control '{}': expected one or two binding strings",
            control.config_key()
        )),
    }
}

fn parse_binding(control: Control, name: &str) -> Result<Option<Binding>, String> {
    if name == "none" {
        return Ok(None);
    }
    Binding::from_config_name(name).map(Some).ok_or_else(|| {
        format!(
            "unrecognized binding '{name}' for control '{}'",
            control.config_key()
        )
    })
}

/// Rewrite the file with the full current `[keybinds]` table,
/// preserving any other top-level tables already in the file.
/// Does nothing when configuration is unavailable.
/// Writes through a temporary file beside the destination so a crash
/// during a rebind cannot leave a half-written file.
pub(crate) fn save_keybinds(path: Option<&Path>, bindings: &Keybinds) {
    let Some(path) = path else {
        return;
    };
    if let Err(error) = try_save_keybinds(path, bindings) {
        warn!("Could not save config {}: {error}", path.display());
    }
}

fn try_save_keybinds(path: &Path, bindings: &Keybinds) -> std::io::Result<()> {
    let mut document = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    document.insert(
        KEYBINDS_TABLE.to_owned(),
        toml::Value::Table(keybinds_table(bindings)),
    );
    write_document(path, document)
}

fn save_value(path: Option<&Path>, key: &str, value: toml::Value) {
    let Some(path) = path else {
        return;
    };
    let mut document = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    document.insert(key.to_owned(), value);
    if let Err(error) = write_document(path, document) {
        warn!("Could not save config {}: {error}", path.display());
    }
}

fn write_document(path: &Path, document: toml::Table) -> std::io::Result<()> {
    let mut text = String::from(CONFIG_HEADER);
    text.push_str(&toml::to_string(&document).map_err(std::io::Error::other)?);
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Config path has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let temp = path.with_extension(format!("{}.tmp", unique_suffix()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn keybinds_table(bindings: &Keybinds) -> toml::Table {
    let mut table = toml::Table::new();
    for control in Control::VARIANTS {
        let primary = bindings
            .get(*control)
            .map_or_else(|| "none".into(), Binding::config_name);
        let value = match bindings.get_secondary(*control) {
            Some(secondary) => toml::Value::Array(vec![
                toml::Value::String(primary),
                toml::Value::String(secondary.config_name()),
            ]),
            None => toml::Value::String(primary),
        };
        table.insert(control.config_key().to_owned(), value);
    }
    table
}

fn unique_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{}", std::process::id(), now.as_nanos(), count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::KeybindSlot;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("plutonian-config-test-{}", unique_suffix()));
            fs::create_dir_all(&dir).expect("create the test config directory");
            Self(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join(CONFIG_FILE_NAME)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn panic_message(path: &Path, loader: impl FnOnce() + std::panic::UnwindSafe) -> String {
        match std::panic::catch_unwind(loader) {
            Ok(()) => panic!("expected {} to panic", path.display()),
            Err(payload) => {
                if let Some(message) = payload.downcast_ref::<String>() {
                    message.clone()
                } else if let Some(message) = payload.downcast_ref::<&str>() {
                    (*message).to_owned()
                } else {
                    panic!("panic payload is not a message");
                }
            }
        }
    }

    #[test]
    fn missing_file_and_unavailable_configuration_load_defaults() {
        let dir = TestDirectory::new();
        assert_eq!(
            load_keybinds(Some(&dir.file())),
            Keybinds::default(),
            "a first run without a file must load defaults"
        );
        assert_eq!(load_keybinds(None), Keybinds::default());
    }

    #[test]
    fn master_volume_loads_defaults_clamps_and_rejects_non_numbers() {
        let dir = TestDirectory::new();
        assert_eq!(load_master_volume(Some(&dir.file())), DEFAULT_MASTER_VOLUME);
        fs::write(dir.file(), "master_volume = 1.4\n").expect("write out-of-range value");
        assert_eq!(load_master_volume(Some(&dir.file())), 1.0);
        fs::write(dir.file(), "master_volume = \"loud\"\n").expect("write invalid value");
        let message = panic_message(&dir.file(), || {
            load_master_volume(Some(&dir.file()));
        });
        assert!(
            message.contains(&dir.file().display().to_string()),
            "{message}"
        );
        assert!(
            message.contains("master_volume must be a number"),
            "{message}"
        );
        assert!(message.contains("delete the file"), "{message}");
    }

    #[test]
    fn saving_settings_preserves_each_other() {
        let dir = TestDirectory::new();
        let config = ConfigFile(Some(dir.file()));
        config.save_master_volume(0.4);
        let mut bindings = Keybinds::default();
        bindings.apply_config(Control::Jump, Some(Binding::Key(KeyCode::KeyJ)), None);
        config.save(&bindings);
        assert_eq!(load_master_volume(config.0.as_deref()), 0.4);
        config.save_master_volume(0.6);
        assert_eq!(load_keybinds(config.0.as_deref()), bindings);
        assert_eq!(load_master_volume(config.0.as_deref()), 0.6);
    }

    #[test]
    fn empty_primary_round_trips_with_and_without_secondary() {
        let dir = TestDirectory::new();
        let mut bindings = Keybinds::default();
        bindings.clear_slot(Control::Jump, KeybindSlot::Primary);
        bindings.clear_slot(Control::MenuUp, KeybindSlot::Primary);
        save_keybinds(Some(&dir.file()), &bindings);
        let text = fs::read_to_string(dir.file()).expect("read saved config");
        assert!(text.contains("jump = \"none\""), "{text}");
        assert!(text.contains("menu_up = [\"none\", \"ArrowUp\"]"), "{text}");
        assert_eq!(load_keybinds(Some(&dir.file())), bindings);
        let parsed = parse_keybinds("[keybinds]\njump = [\"Space\", \"none\"]")
            .expect("explicit empty secondary parses");
        assert_eq!(parsed.get_secondary(Control::Jump), None);
    }

    #[test]
    fn every_control_round_trips_through_save_and_load() {
        let dir = TestDirectory::new();
        let mut bindings = Keybinds::default();
        // Give every control a distinct primary, with mouse bindings on the
        // attack controls and secondaries on a few others.
        let distinct = [
            KeyCode::KeyZ,
            KeyCode::KeyQ,
            KeyCode::KeyU,
            KeyCode::KeyO,
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
            KeyCode::F13,
            KeyCode::F14,
            KeyCode::F15,
        ];
        for (control, key) in Control::VARIANTS.iter().zip(distinct) {
            if *control == Control::Attack || *control == Control::OffHandAttack {
                continue;
            }
            bindings.apply_config(*control, Some(Binding::Key(key)), None);
        }
        bindings.apply_config(
            Control::Attack,
            Some(Binding::Mouse(MouseButton::Middle)),
            Some(Binding::Mouse(MouseButton::Other(7))),
        );
        bindings.apply_config(
            Control::OffHandAttack,
            Some(Binding::Mouse(MouseButton::Back)),
            None,
        );
        bindings.apply_config(
            Control::Jump,
            Some(Binding::Key(KeyCode::KeyJ)),
            Some(Binding::Key(KeyCode::Numpad0)),
        );

        save_keybinds(Some(&dir.file()), &bindings);
        let text = fs::read_to_string(dir.file()).expect("config file is written");
        assert!(text.starts_with(CONFIG_HEADER), "header comment: {text}");
        assert!(text.contains("[keybinds]"), "keybinds table: {text}");

        let loaded = load_keybinds(Some(&dir.file()));
        for control in Control::VARIANTS {
            assert_eq!(
                loaded.get(*control),
                bindings.get(*control),
                "primary for {}",
                control.config_key()
            );
            assert_eq!(
                loaded.get_secondary(*control),
                bindings.get_secondary(*control),
                "secondary for {}",
                control.config_key()
            );
        }
    }

    #[test]
    fn assign_round_trips_every_control() {
        // Every binding the settings capture flow can produce must survive
        // a save-load cycle through the real `assign` path.
        let dir = TestDirectory::new();
        let mut bindings = Keybinds::default();
        let distinct = [
            KeyCode::KeyZ,
            KeyCode::KeyQ,
            KeyCode::KeyU,
            KeyCode::KeyO,
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
            KeyCode::F13,
            KeyCode::F14,
            KeyCode::F15,
        ];
        for (control, key) in Control::VARIANTS.iter().zip(distinct) {
            if *control == Control::Attack || *control == Control::OffHandAttack {
                continue;
            }
            bindings
                .assign(*control, KeybindSlot::Primary, Binding::Key(key))
                .expect("distinct keys must assign without conflicts");
        }
        bindings
            .assign(
                Control::Attack,
                KeybindSlot::Primary,
                Binding::Mouse(MouseButton::Middle),
            )
            .expect("attack accepts mouse buttons");
        bindings
            .assign(
                Control::OffHandAttack,
                KeybindSlot::Primary,
                Binding::Mouse(MouseButton::Back),
            )
            .expect("off-hand attack accepts mouse buttons");
        save_keybinds(Some(&dir.file()), &bindings);
        assert_eq!(load_keybinds(Some(&dir.file())), bindings);
    }

    #[test]
    fn missing_controls_keep_defaults_and_unknown_tables_are_ignored() {
        let dir = TestDirectory::new();
        fs::write(
            dir.file(),
            "[keybinds]\njump = \"KeyJ\"\n[other]\nfuture = true\n",
        )
        .expect("write the hand-edited config");
        let loaded = load_keybinds(Some(&dir.file()));
        assert_eq!(loaded.get(Control::Jump), Some(Binding::Key(KeyCode::KeyJ)));
        assert_eq!(
            loaded.get(Control::Forward),
            Keybinds::default().get(Control::Forward)
        );

        // Rewriting preserves the unknown section for future settings.
        save_keybinds(Some(&dir.file()), &loaded);
        let text = fs::read_to_string(dir.file()).expect("reread the config");
        assert!(text.contains("[other]"), "other sections survive: {text}");
        assert!(
            text.contains("future = true"),
            "other values survive: {text}"
        );
    }

    #[test]
    fn hand_edited_binding_takes_effect_on_next_launch() {
        let dir = TestDirectory::new();
        save_keybinds(Some(&dir.file()), &Keybinds::default());
        let text = fs::read_to_string(dir.file()).expect("read the written config");
        let edited = text.replace("jump = \"Space\"", "jump = \"KeyJ\"");
        assert_ne!(edited, text, "the default jump binding must be present");
        fs::write(dir.file(), edited).expect("write the hand edit");
        assert_eq!(
            load_keybinds(Some(&dir.file())).get(Control::Jump),
            Some(Binding::Key(KeyCode::KeyJ))
        );
    }

    #[test]
    fn malformed_toml_stops_startup_with_path_problem_and_remedy() {
        let dir = TestDirectory::new();
        fs::write(dir.file(), "[keybinds\njump = ").expect("write the broken config");
        let message = panic_message(&dir.file(), || {
            load_keybinds(Some(&dir.file()));
        });
        assert!(
            message.contains(&dir.file().display().to_string()),
            "{message}"
        );
        assert!(message.contains("TOML parse error"), "{message}");
        assert!(message.contains("delete the file"), "{message}");
    }

    #[test]
    fn unknown_control_stops_startup_with_path_problem_and_remedy() {
        let dir = TestDirectory::new();
        fs::write(dir.file(), "[keybinds]\nfly = \"KeyF\"\n").expect("write the broken config");
        let message = panic_message(&dir.file(), || {
            load_keybinds(Some(&dir.file()));
        });
        assert!(
            message.contains(&dir.file().display().to_string()),
            "{message}"
        );
        assert!(message.contains("unknown control 'fly'"), "{message}");
        assert!(message.contains("delete the file"), "{message}");
    }

    #[test]
    fn unrecognized_binding_stops_startup_with_path_problem_and_remedy() {
        let dir = TestDirectory::new();
        fs::write(dir.file(), "[keybinds]\njump = \"NotAKey\"\n").expect("write the broken config");
        let message = panic_message(&dir.file(), || {
            load_keybinds(Some(&dir.file()));
        });
        assert!(
            message.contains(&dir.file().display().to_string()),
            "{message}"
        );
        assert!(
            message.contains("unrecognized binding 'NotAKey' for control 'jump'"),
            "{message}"
        );
        assert!(message.contains("delete the file"), "{message}");
    }

    #[test]
    fn non_string_binding_stops_startup_with_path_problem_and_remedy() {
        let dir = TestDirectory::new();
        fs::write(dir.file(), "[keybinds]\njump = 42\n").expect("write the broken config");
        let message = panic_message(&dir.file(), || {
            load_keybinds(Some(&dir.file()));
        });
        assert!(
            message.contains(&dir.file().display().to_string()),
            "{message}"
        );
        assert!(
            message.contains("invalid binding for control 'jump'"),
            "{message}"
        );
        assert!(message.contains("delete the file"), "{message}");
    }

    #[test]
    fn override_directory_selects_config_toml_inside_it() {
        let dir = TestDirectory::new();
        let selected =
            select_config_file(false, Some(dir.0.clone())).expect("override selects a file");
        assert_eq!(selected, dir.file());
    }

    #[test]
    fn smoke_isolation_wins_over_the_override_directory() {
        let dir = TestDirectory::new();
        let selected = select_config_file(true, Some(dir.0.clone())).expect("smoke selects a file");
        assert!(selected.ends_with(CONFIG_FILE_NAME));
        assert!(
            !selected.starts_with(&dir.0),
            "smoke runs must not write into the override: {}",
            selected.display()
        );
    }

    #[test]
    fn broken_override_config_stops_startup_naming_the_override_path() {
        let dir = TestDirectory::new();
        let selected =
            select_config_file(false, Some(dir.0.clone())).expect("override selects a file");
        fs::write(&selected, "[keybinds]\njump = \"NotAKey\"\n")
            .expect("write the broken override config");
        let message = panic_message(&selected, || {
            load_keybinds(Some(&selected));
        });
        assert!(
            message.contains(&selected.display().to_string()),
            "{message}"
        );
        assert!(
            message.contains("unrecognized binding 'NotAKey' for control 'jump'"),
            "{message}"
        );
        assert!(message.contains("delete the file"), "{message}");
    }

    #[test]
    fn smoke_and_override_directories_select_isolated_files() {
        let smoke = select_config_file(true, None).expect("smoke selects a file");
        assert!(smoke.ends_with(CONFIG_FILE_NAME));
        assert_ne!(
            select_config_file(true, None),
            select_config_file(true, None),
            "parallel smoke runs must not share a file"
        );
        let dir = TestDirectory::new();
        assert_eq!(
            select_config_file(false, Some(dir.0.clone())),
            Some(dir.file())
        );
    }
}
