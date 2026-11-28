use bevy::prelude::AppExit;

fn main() -> AppExit {
    match plutonian_rpg::art_preview::run(std::env::args_os().skip(1)) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("{error}");
            AppExit::from_code(2)
        }
    }
}
