# mcrust

A from-scratch **Minecraft: Java Edition** port to Rust, built on **Bevy** and **wgpu**.

`mcrust` downloads the official **Minecraft 26.3** client from Mojang's servers, extracts its
real resource pack (textures, models, blockstates, language files) and renders a streamed,
procedurally generated world with an authentic texture atlas, per-face shading and ambient
occlusion. No Mojang assets or code are stored in this repository.

> **Status: early but playable-ish.** You can fly around an infinite generated world with real
> 26.3 textures. Block placement, survival, multiplayer and the full vanilla feature set are on
> the roadmap — see [`docs/PORTING.md`](docs/PORTING.md).

## Quick start

Requirements:

- Rust stable **1.97+** (`rustup` recommended)
- A Vulkan / Metal / DX12 capable GPU (Bevy/wgpu picks the best backend automatically)
- Linux only: X11 or Wayland development libraries for the windowing stack

```bash
git clone https://github.com/wowsha/mcrust.git
cd mcrust

# First launch downloads the version manifest, client.jar (~40 MB), libraries and
# extracts assets/minecraft from the jar, then starts the Rust client.
cargo run --release -p mcrust-cli -- play
```

The first launch takes a minute or two depending on your connection. Everything is cached in
`~/.mcrust` (override with `MCRUST_HOME`), so subsequent launches are instant.

### Controls

| Input | Action |
| --- | --- |
| Left click | Grab the mouse |
| `Esc` | Release the mouse |
| `W` `A` `S` `D` | Fly |
| Mouse | Look |
| `Space` / `Shift` | Up / down |
| `Ctrl` | Sprint (3.5×) |

## Commands

```bash
mcrust                                  # same as `mcrust play`
mcrust setup --version 26.3             # download + verify + extract (idempotent)
mcrust setup --version 26.3 --assets all  # also mirror all ~500 MB of asset objects
mcrust play --render-distance 12        # start the Rust client
mcrust launch --dry-run                 # print the vanilla Java command line
mcrust launch --demo                    # actually run vanilla Java (comparison aid)
mcrust decompile                        # decompile client.jar for local reference
mcrust info                             # installation status, sizes, decompiled file count
mcrust paths                            # where everything lives
```

Use `RUST_LOG=debug` for verbose logs. `MCRUST_JAVA` / `JAVA_HOME` select the Java runtime used
by `mcrust launch` and `mcrust decompile`.

## What works today

- ✅ Official 26.3 metadata, SHA-1 verified downloads, resumable/parallel fetching
- ✅ Automatic resource-pack extraction from `client.jar`
- ✅ Real texture atlas built from vanilla `textures/block/*.png` (animated textures use frame 0)
- ✅ Bevy application with a fly camera, fog, sun + ambient light
- ✅ 16×384×16 chunk columns, infinite streaming with background generation/meshing
- ✅ Face culling, baked per-face shading and per-vertex ambient occlusion
- ✅ Separately blended water pass; cut-out leaves and glass
- ✅ Deterministic terrain with hills, beaches, oceans, ores and trees
- ✅ CI on Linux/Windows/macOS + a weekly live-Mojang asset pipeline check

## Architecture

| Crate | Responsibility |
| --- | --- |
| `mcrust-assets` | Version manifest, downloads, hash verification, native extraction, launching vanilla Java |
| `mcrust-vanilla` | Extracted resource packs, block definitions, texture atlas |
| `mcrust-world` | Chunk storage, terrain generation, CPU chunk meshing |
| `mcrust-client` | Bevy app, camera, chunk streaming, materials |
| `mcrust-cli` | The `mcrust` binary (setup / play / launch / decompile / info) |

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the data flow and
[`docs/PORTING.md`](docs/PORTING.md) for the vanilla → Rust mapping and roadmap.

## Using the decompiled vanilla client

Minecraft 26.3 ships with unobfuscated class names, so the decompiled source is directly
readable. `mcrust decompile` downloads [Vineflower](https://vineflower.org/) and produces a
reference tree at `~/.mcrust/vanilla/26.3/src`.

**It stays local.** Decompiled Minecraft code is Mojang's copyrighted work; the directory is in
`.gitignore` and must never be committed or redistributed. Use it to understand behaviour and
then write original Rust.

## Legal

- mcrust is not affiliated with Mojang or Microsoft.
- The repository contains no game assets, no `client.jar`, and no decompiled code.
- Everything is downloaded at runtime from Mojang's official endpoints and remains subject to the
  [Minecraft EULA](https://www.minecraft.net/eula).
- `mcrust launch` uses an offline/demo profile. It does not bypass authentication and cannot join
  online servers that require a Microsoft account.

## License

Rust code in this repository is dual-licensed under MIT or Apache-2.0, at your option.
Minecraft itself is © Mojang Synergies AB and is **not** covered by that license.
