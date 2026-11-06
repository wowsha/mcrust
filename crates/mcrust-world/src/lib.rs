//! Chunk storage, deterministic terrain generation and CPU chunk meshing.
//!
//! The coordinate system follows vanilla: X/Z are block coordinates, Y is
//! vertical with the world spanning -64..=319.

mod chunk;
mod generator;
mod mesh;

pub use chunk::{CHUNK_SIZE, Chunk, ChunkPos, WORLD_HEIGHT, WORLD_MAX_Y, WORLD_MIN_Y};
pub use generator::{SEA_LEVEL, TerrainGenerator};
pub use mesh::{ChunkMesh, ChunkNeighborhood, MeshData, mesh_chunk};
