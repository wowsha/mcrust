# Contributing

Thanks for helping port Minecraft to Rust.

## Setup

```bash
rustup toolchain install stable
cargo run -p mcrust-cli -- setup --version 26.3
cargo run -p mcrust-cli -- play
```

Linux build dependencies: `pkg-config libudev-dev` plus your usual X11/Wayland and Vulkan packages.

## Before opening a PR

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three commands on every PR, plus release builds for Linux/Windows/macOS on `main`.

## Ground rules

1. **No Mojang code or assets in the repository.** Do not commit `client.jar`, extracted textures,
   `.mcrust` contents, or decompiled Java. They are already ignored, but check `git status`.
2. Read the decompiled source for behaviour, then write original Rust. Do not translate methods
   line by line.
3. Keep the world runnable. Prefer a small vertical slice (data → mesh → render) over a large
   unused abstraction.
4. New gameplay code needs a unit test that does not require a GPU or network.
5. Public APIs get doc comments; internal modules stay small and single-purpose.

## Good first issues

- Add more full-cube blocks to `BlockRegistry` (real texture names only).
- Improve terrain: rivers, mountains, biome variation.
- Add a debug overlay (FPS, chunk count, position) using Bevy UI.
- Add a screenshot key (F2) writing into `~/.mcrust/data/screenshots`.
- Write the NBT reader with test vectors.
- Port `blockstates` JSON parsing behind a feature flag.

## Commit messages

Short imperative subject, optional body explaining *why*. Example:

```text
world: cull faces across chunk borders

Meshing waited for all eight neighbours, but border faces were still emitted
because the neighbourhood lookup ignored diagonals.
```

## Reporting bugs

Include the output of `mcrust info`, your OS/GPU, `RUST_LOG=debug` logs, and reproduction steps.
Do not attach decompiled source or Mojang assets.
