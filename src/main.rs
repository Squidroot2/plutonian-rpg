fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    #[cfg(not(feature = "smoke"))]
    assert!(
        !args.iter().any(|arg| arg.starts_with("--smoke-")),
        "smoke scenarios require building with --features smoke"
    );
    let arena = args.iter().any(|arg| arg == "--smoke-test");
    let world = args.iter().any(|arg| arg == "--smoke-world");
    let nemi = args.iter().any(|arg| arg == "--smoke-nemi");
    let cave = args.iter().any(|arg| arg == "--smoke-cave");
    let crossing = args.iter().any(|arg| arg == "--smoke-crossing");
    assert!(
        [arena, world, nemi, cave, crossing]
            .into_iter()
            .filter(|mode| *mode)
            .count()
            <= 1,
        "choose one smoke mode"
    );
    plutonian_rpg::run(plutonian_rpg::Options {
        arena,
        world,
        nemi,
        cave,
        crossing,
        windowed: args.iter().any(|arg| arg == "--windowed"),
    });
}
