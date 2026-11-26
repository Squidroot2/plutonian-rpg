use clap::{Parser, Subcommand};
use std::{path::PathBuf, process::ExitCode};

/// Inspect, create, and describe current-version Plutonian RPG saves.
#[derive(Parser)]
#[command(name = "plutonian-save", version)]
struct Cli {
    #[command(subcommand)]
    command: SaveCommand,
}

#[derive(Subcommand)]
enum SaveCommand {
    /// Read a save and print its contents as text or JSON.
    Inspect {
        /// Print pretty JSON for scripting or authoring golden saves.
        #[arg(long)]
        json: bool,
        /// A save file or a directory whose newest save is inspected.
        /// Omit this argument to use the default save directory.
        path: Option<PathBuf>,
    },
    /// Validate a JSON fixture and write a game save.
    Create {
        /// JSON fixture in the format emitted by inspect --json.
        #[arg(value_name = "INPUT.json")]
        input: PathBuf,
        /// Destination for the generated save.
        #[arg(long, value_name = "PATH.save")]
        output: PathBuf,
    },
    /// Print the JSON Schema for inspection documents.
    Schema,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        SaveCommand::Inspect { json, path } => inspect(json, path),
        SaveCommand::Create { input, output } => create(input, output),
        SaveCommand::Schema => plutonian_rpg::save_inspector::schema()
            .map(|schema| println!("{schema}"))
            .map_err(|error| error.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn inspect(json: bool, path: Option<PathBuf>) -> Result<(), String> {
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

fn create(input: PathBuf, output: PathBuf) -> Result<(), String> {
    plutonian_rpg::save_inspector::create(&input, &output).map_err(|error| {
        format!(
            "Could not create {} from {}: {error}",
            output.display(),
            input.display()
        )
    })
}
