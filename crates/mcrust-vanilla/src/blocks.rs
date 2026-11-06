use std::collections::HashMap;

/// Face order is part of the chunk mesh format. Keep it stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum Face {
    East = 0,
    West = 1,
    Up = 2,
    Down = 3,
    South = 4,
    North = 5,
}

impl Face {
    pub const ALL: [Self; 6] = [
        Self::East,
        Self::West,
        Self::Up,
        Self::Down,
        Self::South,
        Self::North,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn normal(self) -> [i32; 3] {
        match self {
            Self::East => [1, 0, 0],
            Self::West => [-1, 0, 0],
            Self::Up => [0, 1, 0],
            Self::Down => [0, -1, 0],
            Self::South => [0, 0, 1],
            Self::North => [0, 0, -1],
        }
    }

    /// Baked directional shading, close to vanilla's fixed face light.
    pub fn shade(self) -> f32 {
        match self {
            Self::Up => 1.0,
            Self::Down => 0.5,
            Self::North | Self::South => 0.8,
            Self::East | Self::West => 0.6,
        }
    }
}

/// A block type known to the port. Geometry is currently a full cube.
#[derive(Debug, Clone)]
pub struct BlockDef {
    /// Vanilla id, e.g. `grass_block`.
    pub id: &'static str,
    pub display: &'static str,
    /// Texture name (without `block/` and `.png`) per [`Face`].
    pub textures: [&'static str; 6],
    /// Has a collision box.
    pub solid: bool,
    /// Fully hides the neighbouring face.
    pub opaque: bool,
    pub liquid: bool,
    pub translucent: bool,
}

impl BlockDef {
    pub fn texture(&self, face: Face) -> &'static str {
        self.textures[face.index()]
    }

    pub fn is_air(&self) -> bool {
        self.id == "air"
    }
}

/// Block ids, matching the order of [`BlockRegistry::builtin`].
pub mod ids {
    pub const AIR: u16 = 0;
    pub const STONE: u16 = 1;
    pub const DIRT: u16 = 2;
    pub const GRASS_BLOCK: u16 = 3;
    pub const SAND: u16 = 4;
    pub const GRAVEL: u16 = 5;
    pub const OAK_LOG: u16 = 6;
    pub const OAK_LEAVES: u16 = 7;
    pub const WATER: u16 = 8;
    pub const BEDROCK: u16 = 9;
    pub const SNOW_BLOCK: u16 = 10;
    pub const CLAY: u16 = 11;
    pub const SANDSTONE: u16 = 12;
    pub const COBBLESTONE: u16 = 13;
    pub const GLASS: u16 = 14;
    pub const OAK_PLANKS: u16 = 15;
    pub const COAL_ORE: u16 = 16;
    pub const IRON_ORE: u16 = 17;
    pub const ANDESITE: u16 = 18;
    pub const GRANITE: u16 = 19;
    pub const DIORITE: u16 = 20;
}

/// Lookup table for block ids -> definitions.
#[derive(Debug, Clone)]
pub struct BlockRegistry {
    blocks: Vec<BlockDef>,
    by_id: HashMap<&'static str, u16>,
}

impl BlockRegistry {
    /// The seed registry. It only contains blocks the first renderer can draw,
    /// using their real 26.3 textures.
    pub fn builtin() -> Self {
        let mut blocks = Vec::new();
        for def in builtin_blocks() {
            blocks.push(def);
        }
        let by_id = blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.id, index as u16))
            .collect();
        Self { blocks, by_id }
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn get(&self, id: u16) -> &BlockDef {
        self.blocks
            .get(id as usize)
            .unwrap_or_else(|| &self.blocks[ids::AIR as usize])
    }

    pub fn id_of(&self, name: &str) -> Option<u16> {
        self.by_id.get(name).copied()
    }

    pub fn all(&self) -> &[BlockDef] {
        &self.blocks
    }

    /// Every texture referenced by the registry, deduplicated.
    pub fn texture_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self
            .blocks
            .iter()
            .flat_map(|block| block.textures)
            .filter(|name| !name.is_empty())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

impl Default for BlockRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

fn builtin_blocks() -> Vec<BlockDef> {
    fn cube(
        id: &'static str,
        display: &'static str,
        texture: &'static str,
        solid: bool,
        opaque: bool,
    ) -> BlockDef {
        BlockDef {
            id,
            display,
            textures: [texture; 6],
            solid,
            opaque,
            liquid: false,
            translucent: false,
        }
    }

    fn column(
        id: &'static str,
        display: &'static str,
        side: &'static str,
        top: &'static str,
        bottom: &'static str,
    ) -> BlockDef {
        BlockDef {
            id,
            display,
            textures: [side, side, top, bottom, side, side],
            solid: true,
            opaque: true,
            liquid: false,
            translucent: false,
        }
    }

    vec![
        BlockDef {
            id: "air",
            display: "Air",
            textures: [""; 6],
            solid: false,
            opaque: false,
            liquid: false,
            translucent: false,
        },
        cube("stone", "Stone", "stone", true, true),
        cube("dirt", "Dirt", "dirt", true, true),
        BlockDef {
            id: "grass_block",
            display: "Grass Block",
            textures: [
                "grass_block_side",
                "grass_block_side",
                "grass_block_top",
                "dirt",
                "grass_block_side",
                "grass_block_side",
            ],
            solid: true,
            opaque: true,
            liquid: false,
            translucent: false,
        },
        cube("sand", "Sand", "sand", true, true),
        cube("gravel", "Gravel", "gravel", true, true),
        column(
            "oak_log",
            "Oak Log",
            "oak_log",
            "oak_log_top",
            "oak_log_top",
        ),
        BlockDef {
            id: "oak_leaves",
            display: "Oak Leaves",
            textures: ["oak_leaves"; 6],
            solid: true,
            opaque: false,
            liquid: false,
            translucent: false,
        },
        BlockDef {
            id: "water",
            display: "Water",
            textures: ["water_still"; 6],
            solid: false,
            opaque: false,
            liquid: true,
            translucent: true,
        },
        cube("bedrock", "Bedrock", "bedrock", true, true),
        cube("snow_block", "Snow Block", "snow", true, true),
        cube("clay", "Clay", "clay", true, true),
        column(
            "sandstone",
            "Sandstone",
            "sandstone",
            "sandstone_top",
            "sandstone_bottom",
        ),
        cube("cobblestone", "Cobblestone", "cobblestone", true, true),
        BlockDef {
            id: "glass",
            display: "Glass",
            textures: ["glass"; 6],
            solid: true,
            opaque: false,
            liquid: false,
            translucent: false,
        },
        cube("oak_planks", "Oak Planks", "oak_planks", true, true),
        cube("coal_ore", "Coal Ore", "coal_ore", true, true),
        cube("iron_ore", "Iron Ore", "iron_ore", true, true),
        cube("andesite", "Andesite", "andesite", true, true),
        cube("granite", "Granite", "granite", true, true),
        cube("diorite", "Diorite", "diorite", true, true),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable() {
        let registry = BlockRegistry::builtin();
        assert_eq!(registry.get(ids::AIR).id, "air");
        assert_eq!(registry.get(ids::GRASS_BLOCK).id, "grass_block");
        assert_eq!(registry.get(ids::WATER).id, "water");
        assert_eq!(registry.id_of("oak_log"), Some(ids::OAK_LOG));
        assert_eq!(registry.len(), 21);
    }

    #[test]
    fn textures_are_deduplicated() {
        let registry = BlockRegistry::builtin();
        let names = registry.texture_names();
        assert!(names.contains(&"stone"));
        assert!(names.contains(&"grass_block_top"));
        assert_eq!(names.iter().filter(|name| **name == "stone").count(), 1);
    }

    #[test]
    fn face_order_is_stable() {
        assert_eq!(Face::ALL.len(), 6);
        assert_eq!(Face::East.index(), 0);
        assert_eq!(Face::North.index(), 5);
        assert_eq!(Face::Up.normal(), [0, 1, 0]);
    }
}
