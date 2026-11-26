mod decode;

use plutonian_rpg::asset_check::check_repository;
use proc_macro2::{TokenStream, TokenTree};
use std::{collections::BTreeSet, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let failures = check_repository(Path::new("."), rust_model_paths, decode::parse);
    if failures.is_empty() {
        println!("Asset checks passed");
        ExitCode::SUCCESS
    } else {
        for failure in failures {
            eprintln!("{failure}");
        }
        ExitCode::FAILURE
    }
}

fn rust_model_paths(source: &str) -> Result<BTreeSet<String>, String> {
    let tokens = source
        .parse::<TokenStream>()
        .map_err(|error| error.to_string())?;
    let mut paths = BTreeSet::new();
    collect_paths(tokens, &mut paths);
    Ok(paths)
}

fn collect_paths(tokens: TokenStream, paths: &mut BTreeSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Group(group) => collect_paths(group.stream(), paths),
            TokenTree::Literal(literal) => {
                if let Ok(literal) = syn::parse_str::<syn::LitStr>(&literal.to_string()) {
                    let value = literal.value();
                    let path = value.split('#').next().unwrap_or_default();
                    if path.starts_with("models/") && path.ends_with(".glb") {
                        paths.insert(path.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_asset_literals_in_macros_and_ignores_comments() {
        let paths = rust_model_paths(
            r##"
            load!(r"models/raw.glb");
            server.load("models/scene.glb#Scene0");
            // server.load("models/comment.glb");
        "##,
        )
        .expect("fixture must tokenize");
        assert_eq!(
            paths,
            ["models/raw.glb".into(), "models/scene.glb".into()].into()
        );
    }
}
