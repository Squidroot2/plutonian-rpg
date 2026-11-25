use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
    process::ExitCode,
};

const USAGE: &str = "Usage: plutonian-save inspect [--json] [PATH]\n       plutonian-save create INPUT.json --output PATH.save";

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
    match args.next().as_deref().and_then(OsStr::to_str) {
        Some("inspect") => inspect(args),
        Some("create") => create(args),
        _ => Err(USAGE.into()),
    }
}

fn inspect(args: impl Iterator<Item = OsString>) -> Result<(), String> {
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

fn create(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let path = |arg: &OsString| !arg.is_empty() && !arg.to_string_lossy().starts_with('-');
    let input = args.next().filter(path).ok_or(USAGE)?;
    if args.next().as_deref() != Some(OsStr::new("--output")) {
        return Err(USAGE.into());
    }
    let output = args.next().filter(path).ok_or(USAGE)?;
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    let input = PathBuf::from(input);
    let output = PathBuf::from(output);
    plutonian_rpg::save_inspector::create(&input, &output).map_err(|error| {
        format!(
            "Could not create {} from {}: {error}",
            output.display(),
            input.display()
        )
    })
}
