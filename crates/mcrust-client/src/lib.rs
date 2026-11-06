//! Bevy client: loads the vanilla resource pack, builds a texture atlas and
//! streams meshed chunks around the player.

mod camera;
mod chunks;

use std::sync::Arc;

use anyhow::Result;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::WindowResolution;
use mcrust_assets::{InstalledVersion, Paths};
use mcrust_vanilla::{Atlas, BlockRegistry, ResourcePack};
use mcrust_world::TerrainGenerator;

pub use camera::FlyCamera;

pub struct ClientOptions {
    pub version: String,
    pub render_distance: i32,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            version: mcrust_assets::DEFAULT_VERSION.to_string(),
            render_distance: 8,
            seed: 0x5eed_1234_abcd_0001,
            width: 1280,
            height: 720,
        }
    }
}

/// Data needed to mesh chunks. Cloned handles only; the heavy data is shared.
#[derive(Resource, Clone)]
pub struct GameAssets {
    pub atlas: Arc<Atlas>,
    pub registry: Arc<BlockRegistry>,
    pub generator: Arc<TerrainGenerator>,
    pub block_material: Handle<StandardMaterial>,
    pub water_material: Handle<StandardMaterial>,
    pub render_distance: i32,
}

/// Built before the app starts (atlas decoding should not block the first frame).
#[derive(Resource)]
struct PreparedAssets {
    atlas: Arc<Atlas>,
    registry: Arc<BlockRegistry>,
    generator: Arc<TerrainGenerator>,
    render_distance: i32,
}

pub fn run(options: ClientOptions) -> Result<()> {
    let paths = Paths::discover()?;
    let installed = InstalledVersion::load(&paths, &options.version)?;
    let pack = anyhow::Context::with_context(ResourcePack::open(&installed.resources_dir), || {
        format!(
            "resource pack missing at {}. Run `mcrust setup --version {}`.",
            installed.resources_dir.display(),
            options.version
        )
    })?;

    let registry = Arc::new(BlockRegistry::builtin());
    let atlas = Arc::new(Atlas::build(&pack, registry.texture_names())?);
    tracing::info!(
        version = %installed.id,
        blocks = registry.len(),
        tiles = atlas.tile_count(),
        atlas = format!("{}x{}", atlas.width(), atlas.height()),
        "loaded vanilla assets"
    );

    let generator = Arc::new(TerrainGenerator::new(options.seed));
    let render_distance = options.render_distance.clamp(2, 24);

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("mcrust {} — Rust port", installed.id),
                        resolution: WindowResolution::new(options.width, options.height),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .insert_resource(ClearColor(Color::srgb(0.55, 0.74, 0.98)))
        .insert_resource(PreparedAssets {
            atlas,
            registry,
            generator,
            render_distance,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, (camera::fly_camera, chunks::manage_chunks).chain())
        .run();

    Ok(())
}

fn setup(
    mut commands: Commands,
    prepared: Res<PreparedAssets>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (width, height) = (prepared.atlas.width(), prepared.atlas.height());
    let image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        prepared.atlas.image().as_raw().clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let atlas_image = images.add(image);

    let block_material = materials.add(StandardMaterial {
        base_color_texture: Some(atlas_image.clone()),
        base_color: Color::WHITE,
        perceptual_roughness: 0.95,
        metallic: 0.0,
        // Leaves and glass have cut-out pixels.
        alpha_mode: AlphaMode::Mask(0.5),
        ..default()
    });

    let water_material = materials.add(StandardMaterial {
        base_color_texture: Some(atlas_image),
        base_color: Color::srgba(0.45, 0.65, 1.0, 0.72),
        perceptual_roughness: 0.15,
        metallic: 0.0,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });

    let spawn_x = 8.5;
    let spawn_z = 8.5;
    let spawn_y = prepared.generator.height_at(8, 8) as f32 + 2.2;
    let view_distance = (prepared.render_distance * 16) as f32;

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(spawn_x, spawn_y, spawn_z),
        FlyCamera::new(),
        DistanceFog {
            color: Color::srgb(0.62, 0.77, 0.98),
            falloff: FogFalloff::Linear {
                start: view_distance * 0.55,
                end: view_distance * 0.98,
            },
            ..default()
        },
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.88, 0.93, 1.0),
        brightness: 420.0,
        ..default()
    });

    commands.spawn((
        DirectionalLight {
            illuminance: 11_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.7, 0.0)),
    ));

    commands.insert_resource(GameAssets {
        atlas: prepared.atlas.clone(),
        registry: prepared.registry.clone(),
        generator: prepared.generator.clone(),
        block_material,
        water_material,
        render_distance: prepared.render_distance,
    });
    commands.insert_resource(chunks::ChunkManager::new(prepared.render_distance));

    tracing::info!(
        "world ready; WASD to fly, Space/Shift for up/down, Ctrl for speed, Esc to release the mouse"
    );
}
