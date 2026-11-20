# The Plutonian

A third-person exploration RPG prototype built with Bevy 0.20 and Blender. You are a Plutonian returning to Nivalis, a small settlement whose crossing beacon has gone dark. Speak to Nemi, follow the blue waystones to the cavern, and bring its power core home.

## Run

Requires Rust 1.97.1 or newer, a Vulkan-capable graphics driver, and a Linux desktop. Install Bevy's [Linux development dependencies](https://bevy.org/learn/quick-start/getting-started/setup/#installing-os-dependencies) for your distribution, including ALSA/udev only if you enable audio/gamepad features later. This build uses X11 or Wayland and includes no audio or gamepad integration.

```sh
cargo run --locked
```

The first build compiles Bevy and its renderer and can take several minutes. Run from the repository root so Bevy finds `assets/`. `Cargo.lock` pins the dependency versions used for this application.

| Control | Action |
| --- | --- |
| WASD | Move relative to the camera |
| Mouse | Orbit camera |
| Scroll | Adjust camera distance |
| Left Shift | Sprint |
| Space | Jump (grounded only) |
| E | Speak / collect / dismiss dialogue |
| Enter | Interact / dismiss dialogue |
| Escape | Dismiss dialogue, then pause / resume |

The pause menu also has Resume and Quit to desktop buttons. Losing window focus pauses the game. Progress lasts for the current session; relaunching starts a fresh quest. Falling beyond the playable area returns you to the outpost without removing quest progress.

## World and implementation

The playable route connects the outpost, an inclined ice shelf, and a shallow cavern. Follow the luminous waystones toward the cavern; the objective displays the distance to the current destination. The power core is carried as quest state and disappears from its plinth when collected. Returning it to Nemi lights the beacon and completes the quest. You can continue exploring afterward.

`src/` separates loading, world assembly, player movement/animation, camera, quest interactions, and UI into plugins. Avian's kinematic move-and-slide controller runs at a fixed timestep with capsule ground detection and tuned low gravity. The camera uses a sphere cast to avoid passing through geometry. Interaction requires a nearby target with a clear line of sight. There are no save files or migrations.

The first milestone includes one quest and one speaking NPC. Combat, survival, inventory screens, levelling, controller support, and persistent saves are deferred. This is fictional Pluto: terrain, illumination, and gravity favor the experience over physical accuracy. Assets are original, generated locally with the included Blender script; no external models or textures are required.

## Blender workflow

Blender 5.3 was used for the included editable sources in `art/`. Runtime exports are checked in under `assets/models/`, so Blender is not needed to play.

| Source / export | Purpose |
| --- | --- |
| `plutonian` | Shared skinned character, with `Idle`, `Walk`, `Run`, and `Jump` actions |
| `outpost` | Ice shelf, habitats, waystones, cavern, beacon tower, distant sky details |
| `collision` | Simplified static geometry, hidden at runtime |
| `core` | Collectible beacon power core |

To re-export **your edited `.blend` sources** without regenerating them:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py -- --export
```

To **rebuild all source art and exports from the procedural designs**, overwriting the four `.blend` sources:

```sh
blender --background -noaudio --factory-startup --python-exit-code 1 --python scripts/assets.py
```

The scripts run in a separate background Blender process; a live MCP connection is not required. One Blender unit equals one meter. Author with Z up and the character facing -Y; glTF export converts to Bevy's Y-up coordinates, with the character facing +Z. Keep scale applied and keep the four animation names unchanged. Materials use Principled BSDF base color, roughness, metallic, and emission. Animation is exported as sampled actions. Keep visible environment edits and `collision.blend` aligned; collision does not regenerate from edited outpost sources during `--export`.

The authored environment contains the tower; Bevy owns its switchable lens and light, NPC/player placement, and the floating core placement. These placements live in the world plugin. Add collision meshes to `collision.blend` for solid additions. The loader waits for scene instantiation and collider construction, and reports missing assets, animation names, or load timeout on screen.

## Verification

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo run --locked -- --smoke-test
python3 scripts/check_assets.py
```

The desktop smoke test opens a real game window, drives keyboard input through the controller to complete the quest, checks pause/resume, and verifies jumping, landing, and fall recovery. It writes `/tmp/plutonian-outpost.png`, `/tmp/plutonian-cavern.png`, and `/tmp/plutonian-beacon.png`, prints `SMOKE PASS`, and exits on success; it fails if the quest cannot complete within three minutes. Run it on an otherwise idle desktop without supplying additional mouse/keyboard input.

Manual acceptance: walk and sprint, jump and land, traverse the trail rise, push against habitat/cavern walls, orbit the camera near obstructions, try interacting out of range or through a wall, read/dismiss each dialogue, collect the core once, return to Nemi, confirm the lit beacon, pause/resume using keys and buttons, fall off the ice shelf, and restart to confirm a fresh quest. Automated checks cannot establish how comfortable the movement or composition feels.
