# Porting notes

## Ground rules

- The decompiled vanilla tree is a **reference**, not a source to translate line by line.
  Mojang's code is copyrighted; copying it into this repository (even translated) is not okay.
  Read it, understand the data flow and constants, then write original Rust.
- Everything in `crates/` is original code. Anything derived from Mojang's data (block ids,
  texture names, model JSON) is loaded at runtime from the downloaded resource pack.
- Keep the game runnable at every commit. A small working slice beats a big unfinished rewrite.

## Where vanilla does what

| Vanilla (26.3, unobfuscated) | mcrust | Status |
| --- | --- | --- |
| `net.minecraft.client.Minecraft` | `mcrust-client` app loop | partial |
| `net.minecraft.client.renderer.LevelRenderer` | `chunks::ChunkManager` | partial |
| `net.minecraft.client.renderer.chunk.SectionRenderDispatcher` | `mcrust_world::mesh_chunk` | partial (cubes only) |
| `net.minecraft.client.renderer.texture.TextureAtlas` | `mcrust_vanilla::Atlas` | partial (blocks only) |
| `net.minecraft.world.level.block.Blocks` | `mcrust_vanilla::BlockRegistry` | 21 / ~1100 blocks |
| `net.minecraft.world.level.block.state.BlockState` | `BlockDef` | flat, no properties |
| `net.minecraft.world.level.chunk.LevelChunk` | `mcrust_world::Chunk` | partial (no sections/palette/light) |
| `net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator` | `TerrainGenerator` | placeholder |
| `net.minecraft.world.level.levelgen.NoiseRouter` | value-noise FBM | placeholder |
| `net.minecraft.nbt.CompoundTag` | — | not started |
| `net.minecraft.world.level.chunk.storage.RegionFile` | — | not started |
| `net.minecraft.client.multiplayer.ClientLevel` | `ChunkManager` map | partial |
| `net.minecraft.client.multiplayer.ClientPacketListener` | `mcrust-net` (planned) | not started |
| `net.minecraft.world.entity.Entity` | — | not started |
| `net.minecraft.world.inventory.AbstractContainerMenu` | — | not started |

## Milestones

### 0. First Light — done

- [x] Version manifest → version JSON → `client.jar` → libraries → asset index
- [x] Hash-verified, idempotent installs under `~/.mcrust`
- [x] Extract `assets/minecraft` from the jar
- [x] Real block-texture atlas with animation frame 0
- [x] Bevy + wgpu window, fly camera, fog, lighting
- [x] Infinite chunk streaming with background generation and meshing
- [x] Face culling, AO, water pass
- [x] CI (Linux/Windows/macOS) and a live Mojang pipeline smoke test

### 1. Data-driven blocks — next

- [ ] Load `blockstates/*.json` (variants + multipart)
- [ ] Resolve `models/**` including `parent`, `textures` variables and `elements`
- [ ] Generate the full block id table from `Blocks.java` / `BlockIds.java`
- [ ] Generate blockstate variants from their properties (facing, axis, half, waterlogged, …)
- [ ] Support cube rotation (`x`, `y`, `uvlock`), slabs, stairs, fences, cross models
- [ ] Biome tint for grass/leaves from the `grass_colormap` / biome JSON
- [ ] Chunk section palette compression (replace the flat `u16` vector)

Start by decompiling with `mcrust decompile`, then read:

- `net/minecraft/client/renderer/block/model/BlockStateModelLoader.java`
- `net/minecraft/client/renderer/block/model/BlockModel.java`
- `net/minecraft/client/renderer/block/BlockRenderDispatcher.java`
- `net/minecraft/world/level/block/state/BlockState.java`

### 2. Player

- [ ] AABB collision against `solid` blocks (vanilla `Entity.move`)
- [ ] Walk/sprint/jump physics (0.6 block step, gravity 0.08, drag 0.98 — constants in
      `Entity.java` / `LivingEntity.java`)
- [ ] Raycast block targeting (`BlockGetter.clip`, 4.5 block reach)
- [ ] Break/place with a hotbar, block outline and hand swing
- [ ] Save/load options and a spawn point

### 3. World generation

- [ ] Port `NoiseRouter` / `DensityFunction` and `NoiseBasedChunkGenerator`
- [ ] Load `data/minecraft/worldgen/**` (already extracted in the jar if needed)
- [ ] Biomes, caves, aquifers, surface rules
- [ ] Cross-chunk features (trees, structures)

### 4. Lighting

- [ ] Skylight + block light per section (`DataLayer`)
- [ ] Light propagation (`BlockLightEngine`, `SkyLightEngine`)
- [ ] Packed light into vertex/ambient terms instead of the current baked AO shade

### 5. Persistence and networking

- [ ] NBT reader/writer (`CompoundTag`, `ListTag`, varint arrays)
- [ ] Anvil region files (`RegionFile`, `.mca`, zlib chunks)
- [ ] Player save format
- [ ] Protocol 777 handshake/status/login
- [ ] Chunk packets (`ClientboundLevelChunkWithLightPacket`) → `Chunk`
- [ ] Entity movement, chat, keep-alive

### 6. Everything else

Entities, items, inventory, crafting, UI, sounds, particles, redstone, command system, resource
packs, data packs, modding hooks.

## Adding a block today

1. Add a `BlockDef` entry in `crates/mcrust-vanilla/src/blocks.rs` (`cube` or `column` helper).
2. Append it to `builtin_blocks()` and add a constant in `ids` (order is the id).
3. Update the `ids_are_stable` test.
4. The atlas picks up new texture names automatically; missing PNGs render as a magenta checker.

Once the model loader lands, this hand-written table goes away.

## Performance notes

- Chunk generation and meshing run on `AsyncComputeTaskPool`; cap in-flight work in
  `chunks.rs` (`MAX_GENERATING`, `MAX_MESHING`).
- One vertex is 3 floats position + 3 normal + 2 UV + 4 colour = 48 bytes. A full chunk column is
  ~2 MB of mesh data worst case, so keep render distance sane (8–12 by default).
- The flat `u16` block array is 196 KB/column; a palette + section bit-packing is the next big win.
