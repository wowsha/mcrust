use mcrust_vanilla::ids;

pub const CHUNK_SIZE: usize = 16;
pub const WORLD_MIN_Y: i32 = -64;
pub const WORLD_HEIGHT: usize = 384;
pub const WORLD_MAX_Y: i32 = WORLD_MIN_Y + WORLD_HEIGHT as i32 - 1;

/// Chunk column coordinates (not block coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub fn from_block(x: i32, z: i32) -> Self {
        Self {
            x: x.div_euclid(CHUNK_SIZE as i32),
            z: z.div_euclid(CHUNK_SIZE as i32),
        }
    }

    pub fn origin(self) -> [i32; 2] {
        [self.x * CHUNK_SIZE as i32, self.z * CHUNK_SIZE as i32]
    }

    pub fn offset(self, dx: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.z + dz)
    }

    /// The eight surrounding columns, north to south, west to east.
    pub fn neighbors(self) -> [Self; 8] {
        [
            self.offset(-1, -1),
            self.offset(0, -1),
            self.offset(1, -1),
            self.offset(-1, 0),
            self.offset(1, 0),
            self.offset(-1, 1),
            self.offset(0, 1),
            self.offset(1, 1),
        ]
    }
}

/// A 16 x 384 x 16 column of block ids.
#[derive(Debug, Clone)]
pub struct Chunk {
    pos: ChunkPos,
    blocks: Vec<u16>,
}

impl Chunk {
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            pos,
            blocks: vec![ids::AIR; CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT],
        }
    }

    pub fn filled(pos: ChunkPos, block: u16) -> Self {
        Self {
            pos,
            blocks: vec![block; CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT],
        }
    }

    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    pub fn blocks(&self) -> &[u16] {
        &self.blocks
    }

    /// Chunk-local X/Z, world Y.
    pub fn index(x: usize, y: i32, z: usize) -> Option<usize> {
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE {
            return None;
        }
        let local_y = y - WORLD_MIN_Y;
        if !(0..WORLD_HEIGHT as i32).contains(&local_y) {
            return None;
        }
        Some((local_y as usize * CHUNK_SIZE + z) * CHUNK_SIZE + x)
    }

    pub fn get(&self, x: usize, y: i32, z: usize) -> u16 {
        Self::index(x, y, z).map_or(ids::AIR, |index| self.blocks[index])
    }

    pub fn set(&mut self, x: usize, y: i32, z: usize, block: u16) {
        if let Some(index) = Self::index(x, y, z) {
            self.blocks[index] = block;
        }
    }

    pub fn non_air_count(&self) -> usize {
        self.blocks
            .iter()
            .filter(|block| **block != ids::AIR)
            .count()
    }

    pub fn highest_non_air(&self, x: usize, z: usize) -> Option<i32> {
        for local_y in (0..WORLD_HEIGHT).rev() {
            let y = WORLD_MIN_Y + local_y as i32;
            if self.get(x, y, z) != ids::AIR {
                return Some(y);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_positions_floor_correctly() {
        assert_eq!(ChunkPos::from_block(0, 0), ChunkPos::new(0, 0));
        assert_eq!(ChunkPos::from_block(15, 15), ChunkPos::new(0, 0));
        assert_eq!(ChunkPos::from_block(16, -1), ChunkPos::new(1, -1));
        assert_eq!(ChunkPos::new(-1, 2).origin(), [-16, 32]);
    }

    #[test]
    fn get_set_and_bounds() {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        assert_eq!(chunk.get(0, 0, 0), ids::AIR);
        chunk.set(3, 10, 7, ids::STONE);
        assert_eq!(chunk.get(3, 10, 7), ids::STONE);
        assert_eq!(chunk.get(16, 10, 7), ids::AIR);
        assert_eq!(chunk.get(3, WORLD_MAX_Y + 1, 7), ids::AIR);
        assert_eq!(chunk.non_air_count(), 1);
        assert_eq!(chunk.highest_non_air(3, 7), Some(10));
    }
}
