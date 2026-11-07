use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures::check_ready};
use mcrust_world::{
    Chunk, ChunkMesh, ChunkNeighborhood, ChunkPos, MeshData, WORLD_MIN_Y, mesh_chunk,
};

use crate::{GameAssets, camera::FlyCamera};

const MAX_GENERATING: usize = 24;
const MAX_MESHING: usize = 8;
const KEEP_MARGIN: i32 = 2;

#[derive(Default)]
struct ChunkEntities {
    opaque: Option<Entity>,
    water: Option<Entity>,
}

#[derive(Resource)]
pub struct ChunkManager {
    radius: i32,
    center: ChunkPos,
    chunks: HashMap<ChunkPos, Arc<Chunk>>,
    generating: HashMap<ChunkPos, Task<Arc<Chunk>>>,
    meshing: HashMap<ChunkPos, Task<ChunkMesh>>,
    entities: HashMap<ChunkPos, ChunkEntities>,
    dirty: HashSet<ChunkPos>,
}

impl ChunkManager {
    pub fn new(radius: i32) -> Self {
        Self {
            radius,
            center: ChunkPos::new(0, 0),
            chunks: HashMap::new(),
            generating: HashMap::new(),
            meshing: HashMap::new(),
            entities: HashMap::new(),
            dirty: HashSet::new(),
        }
    }

    pub fn loaded_chunks(&self) -> usize {
        self.chunks.len()
    }
}

pub fn manage_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    assets: Res<GameAssets>,
    mut manager: ResMut<ChunkManager>,
    camera: Single<&Transform, With<FlyCamera>>,
    mut frames: Local<u32>,
) {
    let center = ChunkPos::from_block(
        camera.translation.x.floor() as i32,
        camera.translation.z.floor() as i32,
    );
    manager.update(center, &mut commands, &mut meshes, &assets);

    *frames = frames.wrapping_add(1);
    if frames.is_multiple_of(240) {
        tracing::debug!(
            loaded = manager.loaded_chunks(),
            meshing = manager.meshing.len(),
            center = ?center,
            "chunk streaming"
        );
    }
}

impl ChunkManager {
    fn update(
        &mut self,
        center: ChunkPos,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        assets: &GameAssets,
    ) {
        self.center = center;
        self.queue_generation(assets);
        self.poll_generation();
        self.spawn_meshing(assets);
        self.poll_meshing(commands, meshes, assets);
        self.unload(commands);
    }

    fn queue_generation(&mut self, assets: &GameAssets) {
        let budget = MAX_GENERATING.saturating_sub(self.generating.len());
        if budget == 0 {
            return;
        }

        let generate_radius = self.radius + 1;
        let mut candidates = Vec::new();
        for dz in -generate_radius..=generate_radius {
            for dx in -generate_radius..=generate_radius {
                let pos = self.center.offset(dx, dz);
                if self.chunks.contains_key(&pos) || self.generating.contains_key(&pos) {
                    continue;
                }
                candidates.push((dx * dx + dz * dz, pos));
            }
        }
        candidates.sort_unstable_by_key(|(distance, _)| *distance);

        let pool = AsyncComputeTaskPool::get();
        for (_, pos) in candidates.into_iter().take(budget) {
            let generator = assets.generator.clone();
            let task = pool.spawn(async move { Arc::new(generator.generate(pos)) });
            self.generating.insert(pos, task);
        }
    }

    fn poll_generation(&mut self) {
        let mut finished = Vec::new();
        for (pos, task) in self.generating.iter_mut() {
            if let Some(chunk) = check_ready(task) {
                finished.push((*pos, chunk));
            }
        }

        for (pos, chunk) in finished {
            self.generating.remove(&pos);
            self.chunks.insert(pos, chunk);
            self.dirty.insert(pos);
            for neighbour in pos.neighbors() {
                if self.entities.contains_key(&neighbour) {
                    self.dirty.insert(neighbour);
                }
            }
        }
    }

    fn spawn_meshing(&mut self, assets: &GameAssets) {
        let budget = MAX_MESHING.saturating_sub(self.meshing.len());
        if budget == 0 {
            return;
        }

        let mut candidates: Vec<(i32, ChunkPos)> = self
            .chunks
            .keys()
            .copied()
            .filter(|pos| !self.meshing.contains_key(pos))
            .filter(|pos| {
                let needs_mesh = self.dirty.contains(pos) || !self.entities.contains_key(pos);
                needs_mesh && self.neighbors_loaded(*pos)
            })
            .map(|pos| {
                let dx = pos.x - self.center.x;
                let dz = pos.z - self.center.z;
                (dx * dx + dz * dz, pos)
            })
            .collect();
        candidates.sort_unstable_by_key(|(distance, _)| *distance);

        let pool = AsyncComputeTaskPool::get();
        for (_, pos) in candidates.into_iter().take(budget) {
            let center = self.chunks[&pos].clone();
            let neighbors: Vec<(ChunkPos, Arc<Chunk>)> = pos
                .neighbors()
                .into_iter()
                .filter_map(|neighbor| {
                    self.chunks
                        .get(&neighbor)
                        .map(|chunk| (neighbor, chunk.clone()))
                })
                .collect();

            let registry = assets.registry.clone();
            let atlas = assets.atlas.clone();
            let task = pool.spawn(async move {
                let neighborhood = ChunkNeighborhood::new(center.as_ref(), |dx, dz| {
                    neighbors
                        .iter()
                        .find(|(neighbor, _)| neighbor.x == pos.x + dx && neighbor.z == pos.z + dz)
                        .map(|(_, chunk)| chunk.as_ref())
                });
                mesh_chunk(&neighborhood, &registry, &atlas)
            });

            self.dirty.remove(&pos);
            self.meshing.insert(pos, task);
        }
    }

    fn poll_meshing(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        assets: &GameAssets,
    ) {
        let mut finished = Vec::new();
        for (pos, task) in self.meshing.iter_mut() {
            if let Some(mesh) = check_ready(task) {
                finished.push((*pos, mesh));
            }
        }

        for (pos, chunk_mesh) in finished {
            self.meshing.remove(&pos);
            if !self.chunks.contains_key(&pos) {
                continue;
            }
            if let Some(old) = self.entities.remove(&pos) {
                despawn_pair(commands, old);
            }

            let [origin_x, origin_z] = pos.origin();
            let transform =
                Transform::from_xyz(origin_x as f32, WORLD_MIN_Y as f32, origin_z as f32);
            let mut entities = ChunkEntities::default();

            if !chunk_mesh.opaque.is_empty() {
                entities.opaque = Some(
                    commands
                        .spawn((
                            Mesh3d(meshes.add(to_bevy_mesh(&chunk_mesh.opaque))),
                            MeshMaterial3d(assets.block_material.clone()),
                            transform,
                        ))
                        .id(),
                );
            }
            if !chunk_mesh.translucent.is_empty() {
                entities.water = Some(
                    commands
                        .spawn((
                            Mesh3d(meshes.add(to_bevy_mesh(&chunk_mesh.translucent))),
                            MeshMaterial3d(assets.water_material.clone()),
                            transform,
                        ))
                        .id(),
                );
            }

            self.entities.insert(pos, entities);
        }
    }

    fn unload(&mut self, commands: &mut Commands) {
        let keep = self.radius + KEEP_MARGIN;
        let far: Vec<ChunkPos> = self
            .chunks
            .keys()
            .copied()
            .filter(|pos| {
                (pos.x - self.center.x).abs() > keep || (pos.z - self.center.z).abs() > keep
            })
            .collect();

        for pos in far {
            if let Some(entities) = self.entities.remove(&pos) {
                despawn_pair(commands, entities);
            }
            self.chunks.remove(&pos);
            self.dirty.remove(&pos);
            self.generating.remove(&pos);
            self.meshing.remove(&pos);
        }
    }

    fn neighbors_loaded(&self, pos: ChunkPos) -> bool {
        pos.neighbors()
            .iter()
            .all(|neighbor| self.chunks.contains_key(neighbor))
    }
}

fn despawn_pair(commands: &mut Commands, entities: ChunkEntities) {
    for entity in [entities.opaque, entities.water].into_iter().flatten() {
        commands.entity(entity).despawn();
    }
}

#[allow(dead_code)]
fn to_bevy_mesh(data: &MeshData) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, data.colors.clone());
    mesh.insert_indices(Indices::U32(data.indices.clone()));
    mesh
}
