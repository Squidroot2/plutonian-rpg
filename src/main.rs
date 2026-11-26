use std::{ffi::OsString, path::PathBuf};

fn main() {
    let options = parse_args(std::env::args_os().skip(1)).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    plutonian_rpg::run(options);
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<plutonian_rpg::Options, String> {
    let mut options = plutonian_rpg::Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--save-dir" {
            let value = args
                .next()
                .filter(|value| !value.is_empty() && !value.to_string_lossy().starts_with("--"))
                .ok_or("--save-dir requires a non-empty directory path")?;
            options.save_dir = Some(PathBuf::from(value));
        } else if arg == "--smoke-test" {
            options.arena = true;
        } else if arg == "--smoke-world" {
            options.world = true;
        } else if arg == "--smoke-nemi" {
            options.nemi = true;
        } else if arg == "--smoke-cave" {
            options.cave = true;
        } else if arg == "--smoke-crossing" {
            options.crossing = true;
        } else if arg == "--no-screenshots" {
            options.no_screenshots = true;
        } else if arg == "--windowed" {
            options.windowed = true;
        } else if cfg!(not(feature = "smoke")) && arg.to_string_lossy().starts_with("--smoke-") {
            return Err("smoke scenarios require building with --features smoke".into());
        }
    }
    let smoke_count = [
        options.arena,
        options.world,
        options.nemi,
        options.cave,
        options.crossing,
    ]
    .into_iter()
    .filter(|mode| *mode)
    .count();
    if cfg!(not(feature = "smoke")) && smoke_count > 0 {
        return Err("smoke scenarios require building with --features smoke".into());
    }
    if smoke_count > 1 {
        return Err("choose one smoke mode".into());
    }
    if options.no_screenshots && smoke_count == 0 {
        return Err("--no-screenshots requires a smoke scenario".into());
    }
    if smoke_count == 0
        && let Some(path) = options.save_dir.take()
    {
        options.save_dir = Some(
            std::path::absolute(path)
                .map_err(|error| format!("Could not resolve --save-dir: {error}"))?,
        );
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<plutonian_rpg::Options, String> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn save_dir_accepts_absolute_and_relative_paths() {
        assert_eq!(
            parse(&["--save-dir", "/tmp/golden"])
                .expect("absolute path")
                .save_dir,
            Some(PathBuf::from("/tmp/golden"))
        );
        assert_eq!(
            parse(&["--save-dir", "golden-saves/test"])
                .expect("relative path")
                .save_dir,
            Some(
                std::env::current_dir()
                    .expect("working directory")
                    .join("golden-saves/test")
            )
        );
        assert!(parse(&[]).expect("no override").save_dir.is_none());
    }

    #[cfg(feature = "smoke")]
    #[test]
    fn smoke_flags_and_save_directory_are_parsed_independently() {
        let options = parse(&["--save-dir", "golden", "--smoke-world"]).expect("smoke options");
        assert!(options.world);
        assert_eq!(options.save_dir, Some(PathBuf::from("golden")));
    }

    #[cfg(feature = "smoke")]
    #[test]
    fn smoke_screenshots_are_enabled_unless_explicitly_disabled() {
        assert!(
            !parse(&["--smoke-test"])
                .expect("arena options")
                .no_screenshots
        );
        assert!(
            parse(&["--smoke-test", "--no-screenshots"])
                .expect("arena options without screenshots")
                .no_screenshots
        );
        assert!(parse(&["--no-screenshots"]).is_err());
    }

    #[test]
    fn save_dir_requires_a_nonempty_value() {
        for args in [
            &["--save-dir"][..],
            &["--save-dir", ""][..],
            &["--save-dir", "--windowed"][..],
        ] {
            assert!(
                parse(args)
                    .err()
                    .expect("invalid argument")
                    .contains("--save-dir requires")
            );
        }
    }
}
