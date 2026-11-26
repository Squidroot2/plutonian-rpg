# The Plutonian

A third-person exploration RPG prototype set on a fictional Pluto.
You wake in a tent in Nivalis, a small settlement whose crossing beacon has gone dark.
Speak to Nemi, follow the blue waystones to the cavern, and bring its power core home.
The story follows the beacon's return, an abandoned relay, and a crossing caravan facing a strange threat.

## Run

Install Git LFS (`git-lfs`) and run `git lfs install` before cloning.
Without Git LFS, the working tree contains pointer files in place of binary assets.
To repair an existing clone after installing Git LFS, run `git lfs install` and then `git lfs pull` from the repository root.

Requires Rust (only tested with latest stable), a Vulkan-capable graphics driver, and a Linux desktop running X11 or Wayland.
Install your distribution's development packages for the window backends and ALSA audio.
For the package list used on Ubuntu 24.04, see the [CI workflow](.github/workflows/ci.yml).
For other distributions, see [Bevy's Linux setup guide](https://bevy.org/learn/quick-start/getting-started/setup/#installing-os-dependencies).

From the repository root, run:

```sh
cargo run --locked
```

The game opens in borderless fullscreen.
Run `cargo run --locked -- --windowed` to open it in a window.
The first build compiles Bevy and its renderer and can take several minutes.

## How to Play

See the [Guide](docs/guide/guide.md)

## License

Licensed under the [MIT License](LICENSE).
