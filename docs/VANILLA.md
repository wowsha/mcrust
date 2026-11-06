# The vanilla pipeline

## Version facts (26.3)

| Field | Value |
| --- | --- |
| Version id | `26.3` (release, 2026-09-15) |
| Protocol | `777` |
| Java | 25 (`java-runtime-epsilon`) |
| World height | -64 … 319 (384) |
| Resource pack format | 97.1 |
| Data pack format | 121.0 |
| Class names | **unobfuscated** — no mappings needed |

Source of truth: <https://piston-meta.mojang.com/mc/game/version_manifest_v2.json>, then the
per-version JSON at `versions[].url`.

## Endpoints used

| Purpose | URL |
| --- | --- |
| Version manifest | `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` |
| Version JSON | `versions[].url` from the manifest |
| Client jar | `downloads.client.url` |
| Libraries | `libraries[].downloads.artifact.url` (`libraries.minecraft.net`) |
| Asset index | `assetIndex.url` |
| Asset objects | `https://resources.download.minecraft.net/<first 2 hash chars>/<hash>` |
| Log4j config | `logging.client.file.url` |

Every download is checked against the SHA-1 in the metadata before it is moved into place.
Partially downloaded files stay as `*.part` and are retried.

## On-disk layout

```text
~/.mcrust/
├── meta/version_manifest_v2.json
├── versions/26.3/
│   ├── 26.3.json                  # version metadata
│   ├── client.jar                 # official client (SHA-1 verified)
│   ├── resources/                 # extracted asset pack root
│   │   ├── .mcrust-pack           # jar hash marker (skips re-extraction)
│   │   └── assets/minecraft/…     # blockstates, models, textures, lang, …
│   └── natives/java/              # .so/.dll/.dylib for the Java launcher
├── libraries/                     # Maven tree + logging/client-1.21.2.xml
├── assets/
│   ├── indexes/34.json
│   └── objects/ab/cdef…           # only with `--assets all`
├── vanilla/26.3/src/              # decompiled Java (local reference only)
├── tools/vineflower.jar
└── data/                          # saves/options (vanilla launch working directory)
```

Override the root with `MCRUST_HOME=/path cargo run …`.

## Asset modes

`mcrust setup --assets <mode>`:

- `none` — metadata, client jar, libraries, resource pack. Enough to play.
- `index` — **default**: also downloads `assets/indexes/34.json`.
- `all` — mirrors every asset object (~485 MB: sounds, languages, fonts, …).

Block textures, models and blockstates come from `client.jar` directly, which is why the Rust
client does not need the 485 MB object store.

## Running vanilla Java

`mcrust launch` reconstructs the official command line from `arguments.jvm` / `arguments.game`:

- evaluates OS/feature rules (`is_demo_user`, `has_custom_resolution`)
- substitutes `${natives_directory}`, `${classpath}`, `${auth_player_name}`, `${assets_root}`, …
- extracts natives into `natives/java`
- finds Java 25 via `MCRUST_JAVA`, `JAVA_HOME`, or `PATH`
- uses an offline UUID and `accessToken=0`

It is intended for visual/behavioural comparison while porting. Online servers that require a
Microsoft account will reject the offline token. Use `--dry-run` to inspect the command.

## Decompilation

```bash
mcrust decompile            # ~2–10 minutes, ~300 MB, output in ~/.mcrust/vanilla/26.3/src
mcrust decompile --force
```

Uses Vineflower 1.12.0 (`-Xmx6G` by default). Because 26.3 is unobfuscated, the output is directly
readable: `net/minecraft/world/level/block/Blocks.java`, `…/levelgen/NoiseRouter.java`, etc.

The tree is excluded via `.gitignore` (`/vanilla/`, `decompiled/`, `*.class`) and by living outside
the repo. Never commit it, never ship it in a release, never paste it into an issue.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `version 26.3 is not installed` | `mcrust setup --version 26.3` |
| Hash mismatch loop | `mcrust setup --force` (re-downloads) |
| `Java not found` | install JDK 25, set `MCRUST_JAVA=/path/to/java` |
| Missing Vulkan/GL on Linux | install your GPU driver; `WGPU_BACKEND=vulkan` or `gl` |
| Window opens but world is empty | check `RUST_LOG=mcrust_client=debug`; generation errors are logged |
| Wrong GPU | `WGPU_ADAPTER_NAME=<name>` |
