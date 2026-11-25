use std::{ffi::OsString, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let mut args = args.into_iter();
    if args.next().as_deref() != Some(std::ffi::OsStr::new("inspect")) {
        return Err("Usage: plutonian-save inspect [--json] [PATH]".into());
    }
    let mut json = false;
    let mut path = None;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!("Unknown option: {}", arg.to_string_lossy()));
        } else if path.replace(PathBuf::from(arg)).is_some() {
            return Err("Expected at most one save path".into());
        }
    }
    let data = plutonian_rpg::save_inspector::inspect(path.as_deref())
        .map_err(|error| format!("Could not inspect save: {error}"))?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&data).map_err(|error| error.to_string())?
        );
    } else {
        print!("{}", plutonian_rpg::save_inspector::text_report(&data));
    }
    Ok(())
}
