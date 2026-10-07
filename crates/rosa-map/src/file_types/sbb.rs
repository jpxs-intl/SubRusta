use binrw::BinRead;
use rosa_math::vector::{IntVector, Vector};
use glam::UVec3;

use crate::Char64;

#[derive(BinRead, Debug, Default, Clone)]
pub struct BuildingFileTile {
    pub block: u32,
    pub interior_block: u32,
    pub build_block: u32,
    pub edge_x: u32,
    pub edge_z: u32,
    pub floor: u32,

    pub texture_indices: [u16; 8],
    pub interior_texture_indices: [u16; 8],

    pub item_set: u32,
}

#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct BuildingFileWaypoint {
    pub waypoint: Vector,
    pub zero: u32,
}

#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct BuildingFileBot {
    pub waypoint_amount: u32,
    #[br(count = waypoint_amount)]
    pub waypoints: Vec<BuildingFileWaypoint>,
}

#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct BuildingFile {
    pub version: u32,
    pub name: Char64,
    pub width: u32,
    pub length: u32,
    pub height: u32,
    //#[br(if(version > 12))]
    pub offsets: Option<IntVector>,

    pub texture_count: u32,
    #[br(count = texture_count)]
    pub texture_names: Vec<Char64>,

    pub special_block_count: u32,
    #[br(count = special_block_count)]
    pub special_blocks: Vec<Char64>,

    pub build_block_count: u32,
    #[br(count = build_block_count)]
    pub build_blocks: Vec<Char64>,

    pub item_set_count: u32,
    #[br(count = item_set_count)]
    pub item_sets: Vec<Char64>,

    #[br(if(height > 0), count = height * (length + 1) * (width + 1))]
    pub tiles: Vec<BuildingFileTile>,

    #[br(if(version > 13))]
    pub waypoints_enabled: u32,
    #[br(if(version > 13))]
    pub bot_count: u32,
    #[br(if(version > 13), count = bot_count)]
    pub bots: Vec<BuildingFileBot>,
}

impl BuildingFile {
    #[inline]
    pub fn idx(width: u32, length: u32, x: u32, z: u32, y: u32) -> usize {
        ((y * (length + 1) + z) * (width + 1) + x) as usize
    }

    // Mimicking alex awesome here
    pub fn rotated(&self, quarter_turns: u8) -> BuildingFile {
        let mut b = self.clone();
        for _ in 0..(quarter_turns & 3) {
            b = b.rotate_90();
        }
        b
    }

    fn rotate_90(&self) -> BuildingFile {
        let (w, l, h) = (self.width, self.length, self.height);
        let (nw, nl) = (l, w);

        let mut tiles = vec![BuildingFileTile::default(); (h * (nl + 1) * (nw + 1)) as usize];

        for y in 0..h {
            for z in 0..=l {
                for x in 0..=w {
                    let mut t = self.tiles[Self::idx(w, l, x, z, y)].clone();
                    rotate_tile_orientation(&mut t);
                    let (nx, nz) = (l - z, x);
                    tiles[Self::idx(nw, nl, nx, nz, y)] = t;
                }
            }
        }

        let new_off = self.offsets.map(|o| IntVector(UVec3::new(l - o.0.z, o.0.y, o.0.x)));
        BuildingFile { width: nw, length: nl, tiles, offsets: new_off, ..self.clone() }
    }
}

fn rotate_tile_orientation(t: &mut BuildingFileTile) {
    if t.block & 0xE000_0000 == 0x8000_0000 {
        let id = t.block & 0xFFFF;
        let r = if (id + 1) & 3 != 0 { id + 1 } else { id - 3 };
        t.block = (t.block & 0xFFFF_0000) | r;
        std::mem::swap(&mut t.edge_x, &mut t.edge_z);
    }
}