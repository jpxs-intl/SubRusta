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

/// A building: from its own .sbb file, which has waypoints from version 14 (load_sbb), or from a city file, which
/// always does (load_building_csx), the bots read only when they are switched on (load_building_waypoints).
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
#[br(import(from_csx: bool))]
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

    #[br(if(from_csx || version > 13))]
    pub waypoints_enabled: u32,
    #[br(if((from_csx || version > 13) && waypoints_enabled == 1))]
    pub bot_count: u32,
    #[br(if((from_csx || version > 13) && waypoints_enabled == 1), count = bot_count)]
    pub bots: Vec<BuildingFileBot>,
}

impl BuildingFile {
    #[inline]
    pub fn idx(width: u32, length: u32, x: u32, z: u32, y: u32) -> usize {
        ((y * (length + 1) + z) * (width + 1) + x) as usize
    }

    /// `quarter_turns` applications of rotate_loaded_building_tiles_quarter_turn (load_map), with the
    /// offset rotated alongside. `special` resolves a special block (`0x8R00_IIII`) of this building;
    /// see [`SpecialBlock`] and `CityFileCSX::special_block`.
    pub fn rotated(&self, quarter_turns: u8, special: &dyn Fn(&BuildingFile, u32) -> SpecialBlock) -> BuildingFile {
        let mut b = self.clone();
        for _ in 0..(quarter_turns & 3) {
            b = b.rotate_90(special);
        }
        b
    }

    /// One quarter turn: tile (x, z) moves to (length - 1 - z, x), so the source row z == length
    /// is dropped (all but its x edge). Special blocks land `1 - footprint` further along the new
    /// x axis, x edges become z edges on the next cell over, and the side textures cycle.
    fn rotate_90(&self, special: &dyn Fn(&BuildingFile, u32) -> SpecialBlock) -> BuildingFile {
        let (w, l, h) = (self.width, self.length, self.height);
        let (nw, nl) = (l, w);
        let li = l as i32;

        let mut tiles = vec![BuildingFileTile::default(); (h * (nl + 1) * (nw + 1)) as usize];
        let at = |nx: i32, nz: u32, y: u32| -> Option<usize> {
            (0..=li).contains(&nx).then(|| Self::idx(nw, nl, nx as u32, nz, y))
        };
        // returns the destination column and the (possibly rotated) value of a special block
        let place = |v: u32, nx: i32| -> (i32, u32) {
            if v & 0xE000_0000 != 0x8000_0000 {
                return (nx, v);
            }
            let s = special(self, v);
            let v = if s.rotates { (v & !0x0300_0000) | ((v + 0x0100_0000) & 0x0300_0000) } else { v };
            (nx + 1 - s.footprint, v)
        };

        for y in 0..h {
            for z in 0..=li {
                for x in 0..=w {
                    let src = &self.tiles[Self::idx(w, l, x, z as u32, y)];
                    let nx = li - 1 - z;

                    // the x edge moves to the z edge of the next cell over (z == l still writes it)
                    if let Some(i) = at(nx + 1, x, y) {
                        tiles[i].edge_z = src.edge_x;
                    }
                    if z == li {
                        continue;
                    }

                    // exterior block + textures, only into an empty slot
                    let (bx, block) = place(src.block, nx);
                    if let Some(i) = at(bx, x, y) && tiles[i].block == 0 {
                        tiles[i].block = block;
                        tiles[i].texture_indices = rotate_faces(src.texture_indices);
                    }

                    // interior block + textures; a moved special leaves 0xc0000000 in its own slot
                    let (ix, interior) = place(src.interior_block, nx);
                    if let Some(i) = at(ix, x, y) && tiles[i].interior_block == 0 {
                        tiles[i].interior_block = interior;
                        tiles[i].interior_texture_indices = rotate_faces(src.interior_texture_indices);
                    }

                    let i = at(nx, x, y).unwrap();
                    let t = &mut tiles[i];
                    if interior & 0xE000_0000 == 0x8000_0000 && t.interior_block == 0 {
                        t.interior_block = 0xC000_0000;
                    }
                    t.build_block = src.build_block;
                    t.edge_x = src.edge_z;
                    t.floor = src.floor;
                    t.item_set = if src.item_set == 0 {
                        0
                    } else {
                        (src.item_set & !0x0300_0000) | ((src.item_set + 0x0100_0000) & 0x0300_0000)
                    };
                }
            }
        }

        // load_map: offset.x' = length - 1 - offset.z, offset.z' = offset.x (may go negative)
        let new_off = self.offsets.map(|o| IntVector(UVec3::new((li - 1 - o.0.z as i32) as u32, o.0.y, o.0.x)));
        BuildingFile { width: nw, length: nl, tiles, offsets: new_off, ..self.clone() }
    }
}

/// What rotate_loaded_building_tiles_quarter_turn needs to know about a special block.
#[derive(Clone, Copy, Debug)]
pub struct SpecialBlock {
    /// block_dimensions depth (+0xc8) of the resolved global block id
    pub footprint: i32,
    /// the global id is in builtin_block_count..custom_block_start_id, so its orientation cycles
    pub rotates: bool,
}

/// Face textures 1..=4 cycle with the quarter turn: dest[1..5] = src[4], src[1], src[2], src[3].
fn rotate_faces(t: [u16; 8]) -> [u16; 8] {
    [t[0], t[4], t[1], t[2], t[3], t[5], t[6], t[7]]
}