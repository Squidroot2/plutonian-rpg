//! JSON and text tools for current-version saves.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Decode a save file, or the newest `.save` in a directory or the default save directory.
/// This reads the saved state without applying catalog-dependent adjustments.
pub fn inspect(path: Option<&Path>) -> io::Result<serde_json::Value> {
    let path = resolve_path(path, crate::save::data_path())?;
    let snapshot = crate::save::read_save(&path)?;
    crate::save::inspection_data(&snapshot)
}

/// Validate an inspection JSON document against current game data and atomically
/// write a save to the explicit output path.
pub fn create(input: &Path, output: &Path) -> io::Result<()> {
    crate::save::create_from_json(input, output)
}

/// Generate the JSON Schema for the inspection document accepted by `create`.
pub fn schema() -> Result<String, serde_json::Error> {
    crate::save::inspection_schema()
}

fn resolve_path(path: Option<&Path>, default: Option<PathBuf>) -> io::Result<PathBuf> {
    let path = path.map(Path::to_path_buf).or(default).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "No default save directory is available",
        )
    })?;
    if !path.is_dir() {
        return if path.exists() {
            Ok(path)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No save found at {}", path.display()),
            ))
        };
    }
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(&path)? {
        let entry = entry?;
        let file = entry.path();
        if file.extension().is_none_or(|extension| extension != "save") || !file.is_file() {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        if newest
            .as_ref()
            .is_none_or(|candidate| (modified, &file) > (candidate.0, &candidate.1))
        {
            newest = Some((modified, file));
        }
    }
    newest.map(|(_, file)| file).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("No .save files found in {}", path.display()),
        )
    })
}

/// Format all fields of an inspection as an indented report.
pub fn text_report(data: &serde_json::Value) -> String {
    let mut output = String::new();
    render_value(&mut output, data, 0);
    output
}

fn render_value(output: &mut String, value: &serde_json::Value, indent: usize) {
    use std::fmt::Write as _;
    match value {
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                let _ = writeln!(output, "{}(empty)", " ".repeat(indent));
            }
            for (key, value) in map {
                let _ = write!(output, "{}{key}:", " ".repeat(indent));
                if value.is_object() || value.is_array() {
                    output.push('\n');
                    render_value(output, value, indent + 2);
                } else {
                    let _ = writeln!(output, " {value}");
                }
            }
        }
        serde_json::Value::Array(values) => {
            if values.is_empty() {
                let _ = writeln!(output, "{}(empty)", " ".repeat(indent));
            }
            for value in values {
                let _ = write!(output, "{}-", " ".repeat(indent));
                if value.is_object() || value.is_array() {
                    output.push('\n');
                    render_value(output, value, indent + 2);
                } else {
                    let _ = writeln!(output, " {value}");
                }
            }
        }
        _ => {
            let _ = writeln!(output, "{}{}", " ".repeat(indent), value);
        }
    }
}
