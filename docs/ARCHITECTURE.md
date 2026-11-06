# Architecture

## Data flow

```text
piston-meta.mojang.com
        │  version_manifest_v2.json
        ▼
   mcrust setup
        ├── 26.3.json ──────────────► ~/.mcrust/versions/26.3/26.3.json
        ├── client.jar ─────────────► ~/.mcrust/versions/26.3/client.jar
        ├── libraries (SHA-1) ──────► ~/.mcrust/libraries/...
        ├── natives (.so/.dll) ─────► ~/.mcrust/versions/26.3/natives/java
        ├── assets index ───────────► ~/.mcrust/assets/indexes/34.json
        └── assets/minecraft/** ────► ~/.mcrust/versions/26.3/resources
                                          │
                       ResourcePack::open ─┘
                              │
                  Atlas::build(pack, texture names)
                              │
          ┌───────────────────┴───────────────────┐
          ▼                                       ▼
   Bound image (Bevy)                    UV rect table
          │                                       │
   StandardMaterial                    Chunk meshing (CPU)
                                                  │
                                          Mesh (Bevy/wgpu)
```

The client never downloads at render time. `mcrust play` checks
`~/.mcrust/versions/<id>/resources/assets/minecraft/textures/block/stone.png`; if it is missing it
runs the installer first.

## Crates

### `mcrust-assets`

- `meta.rs` — serde models for the manifest, version JSON, arguments, libraries, asset index.
- `rules.rs` — vanilla OS/feature rule evaluation (the same algorithm the official launcher uses).
- `download.rs` — `ureq` (rustls, ring) client with retries, streaming writes to `.part` files,
  SHA-1 verification and a scoped worker pool.
- `install.rs` — resolves the requested version, filters libraries, extracts natives, downloads the
  asset index/objects, extracts `assets/minecraft` from `client.jar` into a resource pack.
- `launch.rs` — reconstructs the Java command line for side-by-side comparison runs.

Downloads are idempotent: existing files are hash-checked and reused.

### `mcrust-vanilla`

- `ResourcePack` — safe, read-only view of a pack directory. Rejects `..` traversal.
- `BlockRegistry` — a deliberately small seed registry (21 blocks) keyed by stable numeric ids.
- `Atlas` — stitches every referenced 16×16 texture into one padded RGBA atlas.

### `mcrust-world`

- `Chunk` — 16×384×16 `u16` block ids. `WORLD_MIN_Y = -64`, `WORLD_HEIGHT = 384` (vanilla 1.18+
  overworld dimensions).
- `TerrainGenerator` — deterministic value-noise FBM heightmap, beaches, sea level 62, ore
  distribution, oak trees. Not yet vanilla's `NoiseBasedChunkGenerator`, but structured so it can
  be replaced without touching the renderer.
- `mesh_chunk` — emits two meshes per column (opaque, translucent). It culls faces against opaque
  neighbours (including across chunk borders), computes vanilla-style per-vertex AO and bakes
  directional shading into vertex colours.

### `mcrust-client`

`ChunkManager` streams columns around the camera:

1. Queue generation for a square of `radius + 1` columns, nearest first.
2. Generation tasks run on Bevy's `AsyncComputeTaskPool`.
3. A column is meshed only when all eight neighbours are loaded, so border faces are correct.
4. Meshing tasks return CPU `MeshData`; the main thread uploads it as a Bevy `Mesh`.
5. New neighbours mark already-meshed columns dirty so seams are re-culled.
6. Columns beyond `radius + 2` are despawned and dropped.

Vertex colour (`Mesh::ATTRIBUTE_COLOR`) carries AO × face shade. `StandardMaterial` multiplies it
into `base_color`, so no custom shader is required yet.

## Coordinate system

- Block coordinates: X east, Y up, Z south (vanilla convention).
- Chunk column coordinates: `x.div_euclid(16)`, `z.div_euclid(16)`; supports negative coordinates.
- Chunk meshes use chunk-local X/Z and `y - WORLD_MIN_Y` for Y; the entity transform is placed at
  `(chunk_x * 16, -64, chunk_z * 16)`.
- Faces use a fixed order (`East, West, Up, Down, South, North`) that is part of the mesh format.

## Rendering

- One texture atlas, nearest-neighbour sampling (`ImagePlugin::default_nearest`), 2 px edge padding
  per tile to avoid bleeding.
- `AlphaMode::Mask(0.5)` handles leaves/glass cut-outs.
- Water is a second mesh with `AlphaMode::Blend`, `cull_mode: None`.
- Directional light + `GlobalAmbientLight`; linear `DistanceFog` hides chunk pop-in.

## Error handling

Library crates return `anyhow::Result` with context; the CLI prints `{error:#}` and exits 1.
Installer operations log recoverable problems (e.g. a native jar that fails to extract) and keep
going, because Java is not required to run the Rust client.

## Testing

`cargo test --workspace` covers pure logic only (no GPU, no network):

- rule evaluation and manifest parsing
- maven path resolution and SHA-1 helpers
- atlas layout, animation cropping, path traversal rejection
- chunk indexing and generation determinism
- face culling, cross-chunk culling, water pass, AO colours

The `.github/workflows/vanilla.yml` job performs the live end-to-end download against Mojang on a
schedule, so a change to their metadata breaks CI loudly instead of silently.
