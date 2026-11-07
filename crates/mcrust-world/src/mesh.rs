use mcrust_vanilla::{Atlas, BlockRegistry, Face};

use crate::chunk::{CHUNK_SIZE, Chunk, WORLD_HEIGHT, WORLD_MIN_Y};

/// CPU-side mesh ready to be uploaded into a Bevy `Mesh`.
#[derive(Debug, Default, Clone)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn face_count(&self) -> usize {
        self.indices.len() / 6
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
}

#[derive(Debug, Default, Clone)]
pub struct ChunkMesh {
    pub opaque: MeshData,
    pub translucent: MeshData,
}

impl ChunkMesh {
    pub fn is_empty(&self) -> bool {
        self.opaque.is_empty() && self.translucent.is_empty()
    }
}

/// The center chunk plus its eight neighbours, used for border culling and AO.
pub struct ChunkNeighborhood<'a> {
    center: &'a Chunk,
    neighbors: [[Option<&'a Chunk>; 3]; 3],
}

impl<'a> ChunkNeighborhood<'a> {
    /// `lookup(dx, dz)` returns the column at center + (dx, dz), or `None`.
    pub fn new(center: &'a Chunk, lookup: impl Fn(i32, i32) -> Option<&'a Chunk>) -> Self {
        let mut neighbors = [[None; 3]; 3];
        for (dz_index, dz) in [-1, 0, 1].into_iter().enumerate() {
            for (dx_index, dx) in [-1, 0, 1].into_iter().enumerate() {
                neighbors[dz_index][dx_index] = if dx == 0 && dz == 0 {
                    Some(center)
                } else {
                    lookup(dx, dz)
                };
            }
        }
        Self { center, neighbors }
    }

    pub fn center(&self) -> &'a Chunk {
        self.center
    }

    /// Chunk-local X/Z (may be -1..=16), world Y.
    pub fn block(&self, x: i32, y: i32, z: i32) -> u16 {
        if !(WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT as i32).contains(&y) {
            return 0;
        }
        let chunk_x = x.div_euclid(CHUNK_SIZE as i32);
        let chunk_z = z.div_euclid(CHUNK_SIZE as i32);
        if !(-1..=1).contains(&chunk_x) || !(-1..=1).contains(&chunk_z) {
            return 0;
        }
        let Some(chunk) = self.neighbors[(chunk_z + 1) as usize][(chunk_x + 1) as usize] else {
            return 0;
        };
        chunk.get(
            x.rem_euclid(CHUNK_SIZE as i32) as usize,
            y,
            z.rem_euclid(CHUNK_SIZE as i32) as usize,
        )
    }
}

/// Quad corners per face, counter-clockwise seen from outside the block.
const CORNERS: [[[f32; 3]; 4]; 6] = [
    // East (+X)
    [
        [1.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 1.0, 1.0],
    ],
    // West (-X)
    [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [0.0, 1.0, 0.0],
    ],
    // Up (+Y)
    [
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ],
    // Down (-Y)
    [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
    ],
    // South (+Z)
    [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ],
    // North (-Z)
    [
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
    ],
];

const UVS: [[[f32; 2]; 4]; 6] = [
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
];

/// Meshes one chunk column. Faces shared with opaque neighbours are skipped.
pub fn mesh_chunk(
    neighborhood: &ChunkNeighborhood,
    registry: &BlockRegistry,
    atlas: &Atlas,
) -> ChunkMesh {
    let chunk = neighborhood.center();
    let mut mesh = ChunkMesh::default();

    for local_y in 0..WORLD_HEIGHT {
        let world_y = WORLD_MIN_Y + local_y as i32;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block_id = chunk.get(x, world_y, z);
                let block = registry.get(block_id);
                if block.is_air() {
                    continue;
                }

                let target = if block.translucent {
                    &mut mesh.translucent
                } else {
                    &mut mesh.opaque
                };

                for face in Face::ALL {
                    let normal = face.normal();
                    let neighbour_id = neighborhood.block(
                        x as i32 + normal[0],
                        world_y + normal[1],
                        z as i32 + normal[2],
                    );
                    let neighbour = registry.get(neighbour_id);

                    let hidden = if block.translucent {
                        neighbour.opaque || neighbour_id == block_id
                    } else {
                        neighbour.opaque
                    };
                    if hidden {
                        continue;
                    }

                    emit_face(
                        target,
                        neighborhood,
                        registry,
                        atlas,
                        block.texture(face),
                        face,
                        [x as i32, world_y, z as i32],
                        local_y,
                    );
                }
            }
        }
    }

    mesh
}

#[allow(clippy::too_many_arguments)]
fn emit_face(
    target: &mut MeshData,
    neighborhood: &ChunkNeighborhood,
    registry: &BlockRegistry,
    atlas: &Atlas,
    texture: &str,
    face: Face,
    block: [i32; 3],
    local_y: usize,
) {
    let uv = atlas.uv_or_first(texture);
    let corners = &CORNERS[face.index()];
    let uvs = &UVS[face.index()];
    let normal = face.normal();
    let base = target.positions.len() as u32;

    let mut occlusion = [0.0f32; 4];
    for (index, corner) in corners.iter().enumerate() {
        let samples = ao_samples(face, *corner);
        let side_one = is_opaque(
            registry,
            neighborhood.block(
                block[0] + samples[0][0],
                block[1] + samples[0][1],
                block[2] + samples[0][2],
            ),
        );
        let side_two = is_opaque(
            registry,
            neighborhood.block(
                block[0] + samples[1][0],
                block[1] + samples[1][1],
                block[2] + samples[1][2],
            ),
        );
        let corner_block = is_opaque(
            registry,
            neighborhood.block(
                block[0] + samples[2][0],
                block[1] + samples[2][1],
                block[2] + samples[2][2],
            ),
        );

        let level = if side_one && side_two {
            0
        } else {
            3u8 - u8::from(side_one) - u8::from(side_two) - u8::from(corner_block)
        };
        occlusion[index] = 0.55 + 0.45 * (f32::from(level) / 3.0);

        let position = [
            block[0] as f32 + corner[0],
            local_y as f32 + corner[1],
            block[2] as f32 + corner[2],
        ];
        target.positions.push(position);
        target
            .normals
            .push([normal[0] as f32, normal[1] as f32, normal[2] as f32]);
        let atlas_uv = uv.map(uvs[index][0], uvs[index][1]);
        target.uvs.push(atlas_uv);

        let shade = face.shade() * occlusion[index];
        target.colors.push([shade, shade, shade, 1.0]);
    }

    // Use the diagonal that keeps AO gradients consistent.
    if occlusion[0] + occlusion[2] > occlusion[1] + occlusion[3] {
        target
            .indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        target
            .indices
            .extend_from_slice(&[base + 1, base + 2, base + 3, base + 1, base + 3, base]);
    }
}

fn is_opaque(registry: &BlockRegistry, block: u16) -> bool {
    registry.get(block).opaque
}

/// The three blocks touching a face corner, as offsets from the block origin.
fn ao_samples(face: Face, corner: [f32; 3]) -> [[i32; 3]; 3] {
    let normal = face.normal();
    let (first_axis, second_axis) = match face {
        Face::East | Face::West => ([0, 1, 0], [0, 0, 1]),
        Face::Up | Face::Down => ([1, 0, 0], [0, 0, 1]),
        Face::South | Face::North => ([1, 0, 0], [0, 1, 0]),
    };

    let first_sign = if corner[axis_index(first_axis)] == 0.0 {
        -1
    } else {
        1
    };
    let second_sign = if corner[axis_index(second_axis)] == 0.0 {
        -1
    } else {
        1
    };

    let mut side_one = normal;
    let mut side_two = normal;
    let mut diagonal = normal;
    for axis in 0..3 {
        side_one[axis] += first_axis[axis] * first_sign;
        side_two[axis] += second_axis[axis] * second_sign;
        diagonal[axis] += first_axis[axis] * first_sign + second_axis[axis] * second_sign;
    }

    [side_one, side_two, diagonal]
}

fn axis_index(axis: [i32; 3]) -> usize {
    if axis[0] != 0 {
        0
    } else if axis[1] != 0 {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcrust_vanilla::{ResourcePack, ids};
    use std::fs;

    fn test_atlas() -> Atlas {
        let dir = tempfile::tempdir().unwrap();
        let registry = BlockRegistry::builtin();
        let texture_dir = dir.path().join("assets/minecraft/textures/block");
        fs::create_dir_all(&texture_dir).unwrap();
        for name in registry.texture_names() {
            image::RgbaImage::from_pixel(16, 16, image::Rgba([120, 120, 120, 255]))
                .save(texture_dir.join(format!("{name}.png")))
                .unwrap();
        }
        let pack = ResourcePack::open(dir.path()).unwrap();
        Atlas::build(&pack, registry.texture_names()).unwrap()
    }

    fn single_block(block: u16) -> Chunk {
        let mut chunk = Chunk::new(crate::ChunkPos::new(0, 0));
        chunk.set(8, 0, 8, block);
        chunk
    }

    fn mesh_of(chunk: &Chunk) -> ChunkMesh {
        let registry = BlockRegistry::builtin();
        let atlas = test_atlas();
        let neighborhood = ChunkNeighborhood::new(chunk, |_, _| None);
        mesh_chunk(&neighborhood, &registry, &atlas)
    }

    #[test]
    fn one_block_has_six_faces() {
        let chunk = single_block(ids::STONE);
        let mesh = mesh_of(&chunk);
        assert_eq!(mesh.opaque.face_count(), 6);
        assert_eq!(mesh.opaque.vertex_count(), 24);
        assert_eq!(mesh.opaque.indices.len(), 36);
        assert!(mesh.translucent.is_empty());
    }

    #[test]
    fn shared_faces_are_culled() {
        let mut chunk = single_block(ids::STONE);
        chunk.set(9, 0, 8, ids::STONE);
        let mesh = mesh_of(&chunk);
        assert_eq!(mesh.opaque.face_count(), 10);
    }

    #[test]
    fn water_goes_to_the_translucent_mesh() {
        let chunk = single_block(ids::WATER);
        let mesh = mesh_of(&chunk);
        assert!(mesh.opaque.is_empty());
        assert_eq!(mesh.translucent.face_count(), 6);
    }

    #[test]
    fn neighbor_faces_are_culled_across_chunks() {
        let mut center = Chunk::new(crate::ChunkPos::new(0, 0));
        center.set(15, 0, 8, ids::STONE);
        let mut east = Chunk::new(crate::ChunkPos::new(1, 0));
        east.set(0, 0, 8, ids::STONE);

        let registry = BlockRegistry::builtin();
        let atlas = test_atlas();
        let neighborhood =
            ChunkNeighborhood::new(&center, |dx, dz| (dx == 1 && dz == 0).then_some(&east));
        let mesh = mesh_chunk(&neighborhood, &registry, &atlas);
        // Only the center chunk is meshed: its +X face is hidden by the
        // neighbour's block, leaving five visible faces.
        assert_eq!(mesh.opaque.face_count(), 5);
    }

    #[test]
    fn vertex_colors_include_face_shade() {
        let chunk = single_block(ids::STONE);
        let mesh = mesh_of(&chunk);
        for color in &mesh.opaque.colors {
            assert!(color[0] >= 0.5 && color[0] <= 1.0);
            assert_eq!(color[3], 1.0);
        }
    }
}
