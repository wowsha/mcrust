//! Vanilla data: the extracted resource pack, the block registry and the
//! texture atlas builder.
//!
//! The block table is intentionally small for now. It is the seed of a
//! generated registry: `Blocks.java` in the decompiled vanilla sources is the
//! source of truth for ids, and `assets/minecraft/blockstates` + `models`
//! describe the geometry. See `docs/PORTING.md`.

mod atlas;
mod blocks;
mod pack;

pub use atlas::{Atlas, UvRect};
pub use blocks::{BlockDef, BlockRegistry, Face, ids};
pub use pack::ResourcePack;
