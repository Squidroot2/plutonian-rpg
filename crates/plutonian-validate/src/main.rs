use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

/// Validate authored gameplay catalogs without starting the game.
#[derive(Parser)]
#[command(name = "plutonian-validate", version)]
struct Cli {
    /// Directory containing the items, enemies, NPCs, world, dialogue, and quests catalogs.
    #[arg(long, default_value = "data")]
    data_root: PathBuf,
}

fn main() -> ExitCode {
    match plutonian_rpg::validate_data_root(&Cli::parse().data_root) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
