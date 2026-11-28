# Configuration Design

This document describes the general configuration file owned by `src/config.rs`,
its location rule, its TOML format, and its failure contract.
Keybinds and master volume share the same document.

## Location

The config file is `config.toml` inside a `plutonian-rpg` directory under the config home.
`config_path()` in `src/config.rs` resolves it as
`$XDG_CONFIG_HOME/plutonian-rpg/config.toml` when `XDG_CONFIG_HOME` is absolute,
else `~/.config/plutonian-rpg/config.toml` when `HOME` is absolute,
else configuration is unavailable.
This mirrors how `data_path()` in `src/save.rs` resolves saves under `$XDG_DATA_HOME`,
with the same absolute-path posture:
when no directory resolves, the game runs on defaults and persists nothing.
The parent directory is created on the first write.
Smoke runs and tests use isolated temporary files
so they never touch the player's real configuration.
Launching with `--config-dir <dir>` points both loading and saving
at `<dir>/config.toml` instead of the XDG path,
the same relationship `--save-dir` has to the saves inside it.
The precedence is smoke isolation first, then `--config-dir`, then XDG:
a smoke run ignores `--config-dir`,
just as smoke save runs ignore the player's save directory.

## Format

The file is one TOML document, chosen because it is easy to edit by hand.
The top-level `master_volume` number is a linear level from `0.0` (mute)
to `1.0` (full volume), defaulting to `1.0` when absent.
Keybinds live in a `[keybinds]` table mapping each `Control` to its binding.
A control with only a primary binding maps to a single string;
a control with a secondary maps to an array of two strings.
The string `none` denotes an empty slot in either position.
When both slots are empty the value is `"none"`;
when only the primary is empty the value is `["none", "<secondary>"]`.
Omitted controls retain their defaults rather than becoming unbound.
The written file carries a comment header saying it is safe to edit by hand,
that deleting the file resets everything to defaults,
and that an invalid file stops the game with an error naming the problem.

Control keys are stable snake-case names such as `forward` and `off_hand_attack`,
defined by `Control::config_key`, not display labels.
Binding strings are stable human-readable names:
keyboard keys use their `KeyCode` variant names such as `KeyW` and `Space`,
and mouse buttons use `MouseLeft`, `MouseRight`, `MouseMiddle`,
`MouseBack`, `MouseForward`, and `MouseOther(N)` for auxiliary buttons.
Every binding the settings capture flow can produce round-trips exactly,
including the mouse-bound attack controls and secondary bindings.

## Loading

`SettingsPlugin` loads the bindings once at startup,
so every consumer reads the configured bindings from the first frame.
A missing file is a first run, not an error, and loads defaults.
A `[keybinds]` table missing a control leaves that control on its default,
so hand edits and future controls stay forgiving.
An out-of-range master volume is clamped to `0.0`–`1.0`.
An entry that is not a number is a broken config.
Unknown top-level tables are ignored:
sections this build does not know are not this build's business.
Anything else is a broken config and panics:
malformed TOML, a `[keybinds]` value that is not a string or binding array,
an unknown control name, or a binding string naming no key or mouse button.

## Saving

When a binding changes through the settings UI,
a successful `Keybinds::assign` from the capture flow,
clearing either slot with Delete, or Reset Bindings,
the file is rewritten immediately with the full current `[keybinds]` table.
The write is synchronous, never deferred to exit,
so a crash right after a rebind cannot lose it.
Rewriting either setting preserves the other setting and any other top-level tables,
and writes through a temporary file plus rename like game saves.
Code paths that build `Keybinds` directly in tests and headless runs
never touch the file.

## Failure contract

A broken config panics during startup load.
There is no recovery UI, no backup, and no silent fallback:
a hand edit that breaks the file must be loud, not hidden.
The panic message names the config file's full path,
the specific problem (the TOML parse error, the unknown control name,
or the unrecognized binding string and the control it was assigned to),
and the remedy: fix the entry, or delete the file to reset to defaults.
