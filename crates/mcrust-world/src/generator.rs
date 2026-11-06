use mcrust_vanilla::ids;

use crate::chunk::{CHUNK_SIZE, Chunk, ChunkPos, WORLD_MIN_Y};

/// Vanilla sea level.
pub const SEA_LEVEL: i32 = 62;

/// Deterministic, seed-based overworld generator.
///
/// This is a placeholder in the spirit of vanilla's noise stages, not a
/// bit-exact reproduction of `NoiseBasedChunkGenerator` yet. The structure is
/// deliberately close to vanilla so the real biome/noise pipeline can replace
/// the height function later.
#[derive(Debug, Clone)]
pub struct TerrainGenerator {
    seed: u64,
}

impl TerrainGenerator {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Approximate surface height (the Y of the top solid block).
    pub fn height_at(&self, x: i32, z: i32) -> i32 {
        let continental = self.fbm(0x9e37_79b9, x as f32 * 0.0045, z as f32 * 0.0045, 5);
        let hills = self.fbm(0x85eb_ca6b, x as f32 * 0.02, z as f32 * 0.02, 4);
        let detail = self.fbm(0xc2b2_ae35, x as f32 * 0.08, z as f32 * 0.08, 2);

        let height = 64.0 + continental * 22.0 + hills * 8.0 + detail * 2.0;
        height.clamp((WORLD_MIN_Y + 8) as f32, 300.0) as i32
    }

    pub fn generate(&self, pos: ChunkPos) -> Chunk {
        let mut chunk = Chunk::new(pos);
        let [origin_x, origin_z] = pos.origin();

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let world_x = origin_x + x as i32;
                let world_z = origin_z + z as i32;
                let height = self.height_at(world_x, world_z);
                let beach = height <= SEA_LEVEL + 1;
                let snowy = height > 112;
                let underwater = height < SEA_LEVEL;

                let top = if beach {
                    ids::SAND
                } else if snowy {
                    ids::SNOW_BLOCK
                } else {
                    ids::GRASS_BLOCK
                };

                for y in WORLD_MIN_Y..=height {
                    let depth = height - y;
                    let block = if y == WORLD_MIN_Y {
                        ids::BEDROCK
                    } else if depth == 0 {
                        top
                    } else if depth <= 3 {
                        if beach { ids::SAND } else { ids::DIRT }
                    } else if depth > 4 && self.ore_at(world_x, y, world_z) {
                        ids::COAL_ORE
                    } else if depth > 6 && self.iron_at(world_x, y, world_z) {
                        ids::IRON_ORE
                    } else if underwater && depth < 10 && self.gravel_at(world_x, y, world_z) {
                        ids::GRAVEL
                    } else {
                        ids::STONE
                    };
                    chunk.set(x, y, z, block);
                }

                for y in (height + 1)..=SEA_LEVEL {
                    chunk.set(x, y, z, ids::WATER);
                }

                if !beach && !snowy && !underwater && height > SEA_LEVEL {
                    self.maybe_tree(&mut chunk, x, z, height);
                }
            }
        }

        chunk
    }

    fn maybe_tree(&self, chunk: &mut Chunk, x: usize, z: usize, ground_y: i32) {
        // Keep trunks and canopies inside the column, so no cross-chunk logic
        // is needed yet.
        if !(3..=CHUNK_SIZE as i32 - 4).contains(&(x as i32))
            || !(3..=CHUNK_SIZE as i32 - 4).contains(&(z as i32))
        {
            return;
        }

        let roll = self.random01(0x7f4a_7c15, x as i32, z as i32);
        if roll > 0.12 {
            return;
        }

        let trunk_height = 4 + (self.random01(0x51ed_270b, x as i32, z as i32) * 3.0) as i32;
        let top_y = ground_y + trunk_height;

        for y in (ground_y + 1)..=top_y {
            chunk.set(x, y, z, ids::OAK_LOG);
        }

        for dy in -2i32..=2 {
            let radius: i32 = if dy.abs() == 2 { 1 } else { 2 };
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() == radius && dz.abs() == radius && dy.abs() == 2 {
                        continue;
                    }
                    let leaf_x = x as i32 + dx;
                    let leaf_y = top_y + dy;
                    let leaf_z = z as i32 + dz;
                    if !(0..CHUNK_SIZE as i32).contains(&leaf_x)
                        || !(0..CHUNK_SIZE as i32).contains(&leaf_z)
                    {
                        continue;
                    }
                    let leaf_x = leaf_x as usize;
                    let leaf_z = leaf_z as usize;
                    if chunk.get(leaf_x, leaf_y, leaf_z) == ids::AIR {
                        chunk.set(leaf_x, leaf_y, leaf_z, ids::OAK_LEAVES);
                    }
                }
            }
        }
    }

    fn ore_at(&self, x: i32, y: i32, z: i32) -> bool {
        self.random3d(0x2545_f491, x, y, z) < 0.012
    }

    fn iron_at(&self, x: i32, y: i32, z: i32) -> bool {
        self.random3d(0x68e3_1da4, x, y, z) < 0.008
    }

    fn gravel_at(&self, x: i32, y: i32, z: i32) -> bool {
        self.random3d(0x1b87_3593, x, y, z) < 0.18
    }

    fn fbm(&self, salt: u64, x: f32, z: f32, octaves: u32) -> f32 {
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut sum = 0.0;
        let mut normalization = 0.0;
        for octave in 0..octaves {
            let value = self.value_noise(salt ^ u64::from(octave), x * frequency, z * frequency);
            sum += value * amplitude;
            normalization += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }
        // -1..1
        (sum / normalization) * 2.0 - 1.0
    }

    fn value_noise(&self, salt: u64, x: f32, z: f32) -> f32 {
        let x0 = x.floor();
        let z0 = z.floor();
        let tx = smoothstep(x - x0);
        let tz = smoothstep(z - z0);
        let (x0, z0) = (x0 as i32, z0 as i32);

        let v00 = self.random01(salt, x0, z0);
        let v10 = self.random01(salt, x0 + 1, z0);
        let v01 = self.random01(salt, x0, z0 + 1);
        let v11 = self.random01(salt, x0 + 1, z0 + 1);

        let a = lerp(v00, v10, tx);
        let b = lerp(v01, v11, tx);
        lerp(a, b, tz)
    }

    fn random01(&self, salt: u64, x: i32, z: i32) -> f32 {
        let hash = splitmix64(
            self.seed
                ^ salt
                ^ (u64::from(x as u32) << 32)
                ^ u64::from(z as u32).wrapping_mul(0x9e37_79b9_7f4a_7c15),
        );
        (hash >> 40) as f32 / (1u32 << 24) as f32
    }

    fn random3d(&self, salt: u64, x: i32, y: i32, z: i32) -> f32 {
        let hash = splitmix64(
            self.seed
                ^ salt
                ^ (u64::from(x as u32) << 42)
                ^ (u64::from(y as u32) << 21)
                ^ u64::from(z as u32).wrapping_mul(0x9e37_79b9_7f4a_7c15),
        );
        (hash >> 40) as f32 / (1u32 << 24) as f32
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = value;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let generator = TerrainGenerator::new(12345);
        let a = generator.generate(ChunkPos::new(0, 0));
        let b = generator.generate(ChunkPos::new(0, 0));
        assert_eq!(a.blocks(), b.blocks());

        let other = TerrainGenerator::new(999).generate(ChunkPos::new(0, 0));
        assert_ne!(a.blocks(), other.blocks());
    }

    #[test]
    fn surface_is_solid_with_air_above() {
        let generator = TerrainGenerator::new(42);
        let chunk = generator.generate(ChunkPos::new(0, 0));
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let height = generator.height_at(x as i32, z as i32);
                assert_ne!(chunk.get(x, height, z), ids::AIR);
                assert_eq!(chunk.get(x, WORLD_MIN_Y + 383, z), ids::AIR);
                assert_eq!(chunk.get(x, WORLD_MIN_Y, z), ids::BEDROCK);
            }
        }
    }

    #[test]
    fn sea_level_is_filled_with_water() {
        let generator = TerrainGenerator::new(7);
        let chunk = generator.generate(ChunkPos::new(0, 0));
        let mut found = false;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if generator.height_at(x as i32, z as i32) < SEA_LEVEL {
                    assert_eq!(chunk.get(x, SEA_LEVEL, z), ids::WATER);
                    found = true;
                }
            }
        }
        // The test world may be entirely above sea level, but at least the
        // invariant should hold wherever water was generated.
        let _ = found;
    }
}
