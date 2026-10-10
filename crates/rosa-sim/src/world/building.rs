use std::collections::HashMap;

use glam::{IVec3, Vec3};
use rosa_physics::{
    RotMatrix,
    rotation::{IDENTITY, rotate_orientation},
};
use rosa_protocol::Team;
use rosa_map::file_types::{
    csx::CityFileCSX,
    sbb::{BuildingFile, BuildingFileTile},
};

use crate::world::{
    area::{AreaGrid, BlockDims, CUBE, FOOTPRINT, MESH, TYPE_MASK},
    blocks::BlockTable,
    ground::Ground,
};

const RT_LAYERS: usize = 64;

#[derive(Clone, Copy, Default)]
struct Tile {
    block: u32,
    interior: u32,
    build: u32,
    edge_x: u32,
    edge_z: u32,
    floor: u32,
    top: u32,
    faces: u8,
    item_set: u32,
}

pub fn building_type(name: &str, roundcity: bool) -> i32 {
    if roundcity {
        return match name {
            "cornerstore" => 4,
            "bc-base01" => 0,
            "bc-base02" => 1,
            "bc-base03" => 2,
            _ => 9,
        };
    }
    match name {
        "base01" => 0,
        "base02" => 1,
        "base03" => 2,
        "base04" => 3,
        "base05" => 4,
        "base06" => 5,
        "cardealer" => 11,
        "gunstore" => 14,
        "burger" => 16,
        "bank" => 12,
        "suits" => 13,
        "labtest" => 10,
        _ => 9,
    }
}

/// The kinds of building record (buildings +0x00): a corporation's base, a car dealership, a lab, a clothing store, a
/// bank, a gun store and a burger shop.
pub const CORPORATION_BASE: i32 = 1;
pub const CAR_DEALER: i32 = 3;
pub const LAB: i32 = 4;
pub const CLOTHING_STORE: i32 = 5;
pub const BANK: i32 = 6;
pub const GUN_STORE: i32 = 8;
pub const BURGER_SHOP: i32 = 9;
/// What a clothing store sells (two suits, a third, two necklaces) and a burger shop's single burger.
const CLOTHING_STOCK: [(i32, i32); 5] = [(0, 1000), (1, 5000), (2, 10000), (3, 10000), (4, 100000)];
const BURGER_PRICE: i32 = 20;

/// A shop's stock entry (building +0xc9f8, 0xc each): an item or vehicle type, its price and a third value (a
/// dealership car's colour).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShopEntry {
    pub kind: i32,
    pub price: i32,
    pub extra: i32,
}

/// A building the game keeps a record of (buildings, 0xdb0c each, created in placement order): its kind, the box inside
/// it (+0x40, +0x4c) and what it sells (+0xc9f4 count).
#[derive(Clone, Debug, Default)]
pub struct BuildingRecord {
    pub kind: i32,
    pub interior_min: Vec3,
    pub interior_max: Vec3,
    pub shop: Vec<ShopEntry>,
}

impl BuildingRecord {
    /// is_in_building: whether `pos` is inside the box, grown by `margin`.
    pub fn contains(&self, pos: Vec3, margin: f32) -> bool {
        let (a, b) = (self.interior_min, self.interior_max);
        !(pos.x < a.x - margin) && !(b.x + margin <= pos.x) && !(pos.y < a.y - margin) && !(b.y + margin <= pos.y) && !(pos.z < a.z - margin) && margin + b.z > pos.z
    }
}

/// The record kind a building type gets, if any.
fn record_kind(building_type: i32) -> Option<i32> {
    match building_type {
        0..=5 => Some(CORPORATION_BASE),
        10 => Some(LAB),
        11 => Some(CAR_DEALER),
        12 => Some(BANK),
        13 => Some(CLOTHING_STORE),
        14 => Some(GUN_STORE),
        16 => Some(BURGER_SHOP),
        _ => None,
    }
}

/// The quarter turn the table turns by with each turn of its base building.
const CORPORATIONS: usize = 6;
/// The base's car spaces: a row of four 16 back and 16 across from the interior's middle, 4 apart, then the row
/// facing the other way 24 further across.
const CAR_ROW_START: f32 = 16.0;
const CAR_STEP: f32 = 4.0;
const CAR_ROW_GAP: f32 = 24.0;
const CAR_SPACES: usize = 8;
/// The table sits 4 along and 17.5 across from the middle (Round City places it so; elsewhere the base table block
/// does), and players spawn 12 along and 6 across from it.
const TABLE_ALONG: f32 = 4.0;
const TABLE_ACROSS: f32 = 17.5;
const SPAWN_ALONG: f32 = 12.0;
const SPAWN_ACROSS: f32 = 6.0;
/// In versus, players spawn in the middle of the base, 13.5 up.
const VERSUS_SPAWN_LIFT: f32 = 12.0;
const VERSUS_SPAWN_LIFT2: f32 = 1.5;
const VERSUS: u32 = 7;

/// A corporation's base (game_mode_state.corporations, 0x5bc4 each): the box inside it (one floor high), its vault,
/// the table, where players spawn and the spaces its cars appear in.
#[derive(Clone, Debug)]
pub struct CorporationBase {
    pub interior_min: Vec3,
    pub interior_max: Vec3,
    pub vault_min: Vec3,
    pub vault_max: Vec3,
    /// The way the table faces (+0x30): a half turn plus a quarter turn per turn of the base building.
    pub table_orientation: f32,
    pub table: Vec3,
    pub spawn: Vec3,
    pub car_spaces: Vec<(Vec3, RotMatrix)>,
    /// The two cells of the garage door (the last one placed) and the way it faces (+0x43c).
    pub door: Option<([IVec3; 2], u32)>,
}

impl Default for CorporationBase {
    fn default() -> Self {
        CorporationBase {
            interior_min: Vec3::ZERO,
            interior_max: Vec3::ZERO,
            vault_min: Vec3::ZERO,
            vault_max: Vec3::ZERO,
            table_orientation: std::f32::consts::PI,
            table: Vec3::ZERO,
            spawn: Vec3::ZERO,
            car_spaces: Vec::new(),
            door: None,
        }
    }
}

impl CorporationBase {
    /// Whether `pos` is inside the base (x and z only).
    pub fn contains(&self, pos: Vec3) -> bool {
        !(pos.x < self.interior_min.x) && !(self.interior_max.x + 0.0 <= pos.x) && !(pos.z < self.interior_min.z) && self.interior_max.z + 0.0 > pos.z
    }

    /// is_position_in_vault: whether `pos` is inside the vault grown by `tolerance` on every side.
    pub fn in_vault(&self, pos: Vec3, tolerance: f32) -> bool {
        let (a, b) = (self.vault_min, self.vault_max);
        !(pos.x < a.x - tolerance)
            && !(b.x + tolerance <= pos.x)
            && !(pos.y < a.y - tolerance)
            && !(b.y + tolerance <= pos.y)
            && !(pos.z < a.z - tolerance)
            && tolerance + b.z > pos.z
    }

    /// The table's frame: turned to the table's way, then half around.
    pub(crate) fn frame(&self) -> RotMatrix {
        let mut m = IDENTITY;
        let axis = m[1];
        rotate_orientation(&mut m, axis, self.table_orientation);
        let axis = m[1];
        rotate_orientation(&mut m, axis, std::f32::consts::PI);
        m
    }

    fn middle(&self) -> Vec3 {
        let (a, b) = (self.interior_min, self.interior_max);
        Vec3::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, (a.z + b.z) * 0.5)
    }
}

pub struct Buildings<'a> {
    csx: &'a CityFileCSX,
    custom_names: Vec<&'a str>,
    build_blocks: Vec<String>,
    item_sets: Vec<String>,
    runtime: Vec<Tile>,
    mesh_cache: HashMap<u32, u32>,
    pub generated: Vec<(u32, u32)>,
    pub records: Vec<BuildingRecord>,
    pub bases: Vec<CorporationBase>,
    game_type: u32,
}

impl<'a> Buildings<'a> {
    pub fn new(csx: &'a CityFileCSX, build_blocks: Vec<String>, item_sets: Vec<String>, game_type: u32) -> Self {
        Self {
            csx,
            custom_names: csx.custom_blocks().map(|(n, _)| n).collect(),
            build_blocks,
            item_sets,
            runtime: vec![Tile::default(); RT_LAYERS * 4096],
            mesh_cache: HashMap::new(),
            generated: Vec::new(),
            records: Vec::new(),
            bases: vec![CorporationBase::default(); CORPORATIONS],
            game_type,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn place(
        &mut self,
        area: &mut AreaGrid,
        ground: &mut Ground,
        tables: &mut BlockTable,
        dims: &dyn BlockDims,
        name: &str,
        pos: IVec3,
        rot: u32,
        roundcity: bool,
    ) {
        let Some(raw) = self.csx.get_building(name.to_string()) else { return };

        let off_y = raw.offsets.map_or(0, |o| o.0.y) as i32;
        let b = raw.rotated(rot as u8, &|bf, v| self.csx.special_block(bf, v));
        let off = b.offsets.map_or(IVec3::ZERO, |o| o.0.as_ivec3());

        let (w, l, h) = (b.width as i32, b.length as i32, b.height as i32);

        self.prepare_runtime(&self.setup(&b), w, l, h);

        let ty = building_type(name, roundcity);
        let corp = usize::try_from(ty).ok().and_then(|i| Team::CORPORATIONS.get(i).copied());
        let o = pos - off;
        let base = usize::try_from(ty).ok().filter(|&i| i < CORPORATIONS);
        if let Some(k) = base {
            let b = &mut self.bases[k];
            for _ in 0..(rot & 3) {
                b.table_orientation = (b.table_orientation as f64 + (std::f64::consts::PI / 2.0)) as f32;
            }
        }

        self.instantiate(area, ground, tables, dims, o, w, l, h, off_y, rot, corp, base);
        if let Some(kind) = record_kind(ty) {
            self.records.push(building_record(kind, &raw, o, rot));
        }
        if let Some(k) = base {
            self.place_base(k, &raw, o, rot, roundcity);
        }
    }

    /// The corporation part of area_instantiate_building_blocks: the base's interior (one floor high), its car spaces
    /// and its spawn point.
    fn place_base(&mut self, k: usize, raw: &BuildingFile, o: IVec3, rot: u32, roundcity: bool) {
        let (x, z) = interior_span(raw, rot);
        let cell = |v: i32| v as f32 * 4.0;
        let versus = self.game_type == VERSUS;
        let b = &mut self.bases[k];
        b.interior_min = Vec3::new(cell(o.x + x.0), cell(o.y), cell(o.z + z.0));
        b.interior_max = Vec3::new(cell(o.x + x.1), cell(o.y + 1), cell(o.z + z.1));
        let c = b.middle();
        let mut m = b.frame();
        let [r0, _, r2] = m;
        let mut p = Vec3::new(
            (r0.x * -CAR_ROW_START + c.x) + r2.x * CAR_ROW_START,
            (r0.y * -CAR_ROW_START + c.y) + r2.y * CAR_ROW_START,
            (-CAR_ROW_START * r0.z + c.z) + CAR_ROW_START * r2.z,
        );
        for n in 1..=CAR_SPACES {
            b.car_spaces.push((p, m));
            if n == 4 {
                let axis = m[1];
                rotate_orientation(&mut m, axis, std::f32::consts::PI);
                let r2 = m[2];
                p = Vec3::new(p.x + r2.x * CAR_ROW_GAP, p.y + r2.y * CAR_ROW_GAP, p.z + r2.z * CAR_ROW_GAP);
            } else if n != CAR_SPACES {
                let r0 = m[0];
                p = Vec3::new(p.x + r0.x * CAR_STEP, p.y + r0.y * CAR_STEP, p.z + r0.z * CAR_STEP);
            }
        }
        let c = b.middle();
        let [r0, _, r2] = b.frame();
        let t = Vec3::new(
            (r0.x * TABLE_ALONG + c.x) + r2.x * TABLE_ACROSS,
            (r0.y * TABLE_ALONG + c.y) + r2.y * TABLE_ACROSS,
            (TABLE_ALONG * r0.z + c.z) + TABLE_ACROSS * r2.z,
        );
        if roundcity {
            b.table = t;
        }
        b.spawn = Vec3::new(
            (r0.x * SPAWN_ALONG + t.x) + r2.x * SPAWN_ACROSS,
            (r0.y * SPAWN_ALONG + t.y) + r2.y * SPAWN_ACROSS,
            (r0.z * SPAWN_ALONG + t.z) + r2.z * SPAWN_ACROSS,
        );
        if versus {
            let (a, bb) = (b.interior_min, b.interior_max);
            b.spawn = Vec3::new((a.x + bb.x) * 0.5, (a.y + VERSUS_SPAWN_LIFT) + VERSUS_SPAWN_LIFT2, (a.z + bb.z) * 0.5);
        }
    }


    fn setup(&self, b: &BuildingFile) -> Vec<Tile> {
        let mut special = [0u32; 1024];
        for (i, n) in b.special_blocks.iter().enumerate().take(1024) {
            for (k, cn) in self.custom_names.iter().enumerate() {
                if *cn == n.as_str() {
                    special[i] = BlockTable::custom(k);
                }
            }
        }
        let mut build = [0u32; 1024];
        for (i, n) in b.build_blocks.iter().enumerate().take(1024) {
            for (k, gn) in self.build_blocks.iter().enumerate().take(64) {
                if gn == n.as_str() {
                    build[i] = k as u32;
                }
            }
        }
        let mut isets = [0u32; 1024];
        for (i, n) in b.item_sets.iter().enumerate().take(1024) {
            for (k, gn) in self.item_sets.iter().enumerate() {
                if gn == n.as_str() {
                    isets[i] = k as u32;
                }
            }
        }
        let resolve = |v: u32| {
            if v & TYPE_MASK == MESH {
                ((v >> 24) & 3).wrapping_add(special[(v & 1023) as usize] | MESH)
            } else {
                v
            }
        };
        b.tiles
            .iter()
            .map(|t: &BuildingFileTile| Tile {
                build: build.get(t.build_block as usize).copied().unwrap_or(0),
                block: resolve(t.block),
                interior: resolve(t.interior_block),
                edge_x: t.edge_x,
                edge_z: t.edge_z,
                floor: t.floor,
                top: 0,
                faces: t.texture_indices.iter().enumerate().fold(0, |m, (i, &w)| if w & 32768 == 0 { m | 1 << i } else { m }),
                item_set: if t.item_set == 0 {
                    0
                } else {
                    (t.item_set & (3 << 24)) | isets[(t.item_set & 1023) as usize] | (1 << 30)
                },
            })
            .collect()
    }

    #[inline]
    fn rt_index(x: i32, z: i32, y: i32) -> i64 {
        y as i64 * 4096 + z as i64 * 64 + x as i64
    }

    fn rt(&self, x: i32, z: i32, y: i32) -> Tile {
        let i = Self::rt_index(x, z, y);
        if i < 0 || i as usize >= self.runtime.len() { Tile::default() } else { self.runtime[i as usize] }
    }

    fn prepare_runtime(&mut self, mem: &[Tile], w: i32, l: i32, h: i32) {
        let at = |x: i32, z: i32, y: i32| mem[BuildingFile::idx(w as u32, l as u32, x as u32, z as u32, y as u32)];
        for y in 0..h {
            for z in 0..=l {
                for x in 0..=w {
                    let m = at(x, z, y);
                    let mut top = 1;

                    if y < h - 1 {
                        let a = at(x, z, y + 1);
                        if a.block != 0 && a.interior != 0 && a.floor == 0 {
                            top = 0;
                        }
                    }

                    let i = Self::rt_index(x, z, y) as usize;
                    self.runtime[i] = Tile { top, ..m };
                }
            }
        }
        let portal = |v: u32| v.wrapping_sub(3) <= 12;
        for y in 0..h {
            for z in 0..=l {
                for x in 0..=w {
                    let t = self.rt(x, z, y);
                    if t.block != CUBE {
                        continue;
                    }

                    let i = t.interior;
                    let qualifies = i == 65535 || i == 0 || i & FOOTPRINT == FOOTPRINT;
                    if !qualifies && !self.is_animated(i & 65535) {
                        continue;
                    }

                    let checks = [self.rt(x, z + 1, y).edge_x, t.edge_z, t.edge_x, self.rt(x + 1, z, y).edge_z];
                    let idx = Self::rt_index(x, z, y) as usize;

                    for (d, e) in checks.into_iter().enumerate() {
                        if portal(e) {
                            let f = d + 1;
                            self.runtime[idx].faces ^= 1 << f;
                        }
                    }
                }
            }
        }
    }

    fn is_animated(&self, id: u32) -> bool {
        self.custom_names
            .iter()
            .position(|n| *n == "garagedoor")
            .is_some_and(|k| (BlockTable::custom(k)..BlockTable::custom(k) + 4).contains(&id))
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate(
        &mut self,
        area: &mut AreaGrid,
        ground: &mut Ground,
        tables: &mut BlockTable,
        dims: &dyn BlockDims,
        o: IVec3,
        w: i32,
        l: i32,
        h: i32,
        off_y: i32,
        rot: u32,
        corp: Option<Team>,
        base: Option<usize>,
    ) {
        let flags = rot << 24;
        for y in 0..h {
            for z in 0..l {
                for x in 0..w {
                    if self.rt(x, z, y).block != 0 {
                        clear_collision_cell(area, o.x + x, o.y + y, o.z + z);
                    }
                }
            }
        }

        for y in 0..h {
            for z in 0..l {
                for x in 0..w {
                    let t = self.rt(x, z, y);
                    if t.block == 0 {
                        continue;
                    }
                    let p = o + IVec3::new(x, y, z);
                    area.create_block(p.x, p.y, p.z, t.block | flags, t.faces, dims);
                    if y == off_y {
                        ground.stamp_roadmap(p.x, p.z, p.y as f32 * 4.0);
                    }
                }
            }
        }

        let is_p = |v: u32| v.wrapping_sub(3) <= 12 || v == 1;
        let garage = tables.garage_door();

        for y in 0..h {
            for z in 0..l {
                for x in 0..w {
                    let t = self.rt(x, z, y);
                    let v = t.interior;
                    if v == 0 {
                        continue;
                    }
                    let p = o + IVec3::new(x, y, z);
                    if v == 65535 {
                        let zp = self.rt(x, z + 1, y);
                        let xp = self.rt(x + 1, z, y);
                        let mut key = (zp.edge_x << 8)
                            | t.edge_x
                            | (t.floor << 16)
                            | (t.top << 17)
                            | (xp.edge_z << 4)
                            | (t.edge_z << 12)
                            | (t.build << 18);

                        if t.edge_x == 0 {
                            if t.edge_z == 0 && (is_p(self.rt(x - 1, z, y).edge_x) || is_p(self.rt(x, z - 1, y).edge_z)) {
                                key |= 2;
                            }
                            if xp.edge_z == 0 && (is_p(xp.edge_x) || is_p(self.rt(x + 1, z - 1, y).edge_z)) {
                                key |= 2 << 4;
                            }
                        }

                        if zp.edge_x == 0 {
                            if xp.edge_z == 0 {
                                let d = self.rt(x + 1, z + 1, y);
                                if is_p(d.edge_x) || is_p(d.edge_z) {
                                    key |= 2 << 8;
                                }
                            }
                            if t.edge_z == 0 && (is_p(self.rt(x - 1, z + 1, y).edge_x) || is_p(zp.edge_z)) {
                                key |= 2 << 12;
                            }
                        }

                        let id = self.mesh_id(tables, key);
                        if area.layer1(p.x, p.y, p.z) == 0 {
                            area.place_with_footprint(p.x, p.y, p.z, id | flags | MESH, dims);
                        }
                    } else if (v as i32) < 0 {
                        area.place_with_footprint(p.x, p.y, p.z, flags | v, dims);
                        if let Some(k) = base {
                            let id = v & 65535;
                            let c = |n: i32| n as f32 * 4.0;
                            if tables.is_safe(id) {
                                let b = &mut self.bases[k];
                                b.vault_min = Vec3::new(c(p.x) + 0.25, c(p.y) + 0.125, c(p.z) + 0.25);
                                b.vault_max = Vec3::new(c(p.x + 1) - 0.25, c(p.y + 1) - 0.125, c(p.z + 1) - 0.25);
                            }
                            if tables.is_base_table(id) {
                                self.bases[k].table = Vec3::new(c(p.x) + 2.0, c(p.y) + 2.0, c(p.z) + 2.0);
                            }
                        }
                    }

                    if corp.is_some() && self.game_type != 7 {
                        let id = v & 65535;
                        if id >= garage[0] && id <= garage[3] {
                            let dir = (id.wrapping_sub(garage[0]) + 2) & 3;
                            let mut second = p;
                            
                            match dir {
                                0 | 2 => second.x = p.x + 1,
                                _ => second.z = p.z + 1,
                            }

                            for d in [p, second] {
                                area.set_object(d.x, d.y, d.z, dir << 10 | 6);
                            }
                            if let Some(k) = base {
                                self.bases[k].door = Some(([p, second], dir));
                            }
                        }
                    }
                }
            }
        }

        for y in 0..h {
            for z in 0..l {
                for x in 0..w {
                    let v = self.rt(x, z, y).item_set;
                    if v != 0 {
                        let p = o + IVec3::new(x, y, z);
                        area.set_item_set(p.x, p.y, p.z, v);
                    }
                }
            }
        }
    }

    fn mesh_id(&mut self, tables: &mut BlockTable, key: u32) -> u32 {
        let key = key & ((1 << 24) - 1);
        if let Some(&id) = self.mesh_cache.get(&key) {
            return id;
        }

        let id = tables.alloc_block_id().unwrap_or(0);

        self.mesh_cache.insert(key, id);
        self.generated.push((id, key));

        id
    }
}

pub fn clear_collision_cell(area: &mut AreaGrid, x: i32, y: i32, z: i32) {
    if x < 0 || y < 0 || z < 0 {
        return;
    }

    if area.layer0(x, y, z) & TYPE_MASK == CUBE {
        for (d, bit) in [
            (IVec3::new(0, -1, 0), 32u32),
            (IVec3::new(0, 1, 0), 1),
            (IVec3::new(0, 0, 1), 8),
            (IVec3::new(0, 0, -1), 2),
            (IVec3::new(-1, 0, 0), 16),
            (IVec3::new(1, 0, 0), 4),
        ] {
            let n = area.layer0(x + d.x, y + d.y, z + d.z);
            if n & TYPE_MASK == CUBE {
                area.set_layer0(x + d.x, y + d.y, z + d.z, n | bit);
            }
        }
    }

    area.clear_cell(x, y, z);
}

/// The record area_instantiate_building_blocks makes: the interior is the building past its offset margin, turned with
/// the building, from `o` (the building's lowest corner).
/// The interior's x and z extent from the building's lowest corner: past its offset margin, turned with the building.
fn interior_span(raw: &BuildingFile, rot: u32) -> ((i32, i32), (i32, i32)) {
    let off = raw.offsets.map_or(IVec3::ZERO, |v| v.0.as_ivec3());
    let (w, l) = (raw.width as i32, raw.length as i32);
    match rot & 3 {
        0 => ((off.x, w), (off.z, l)),
        1 => ((0, l - off.z), (off.x, w)),
        2 => ((0, w - off.x), (0, l - off.z)),
        _ => ((off.z, l), (0, w - off.x)),
    }
}

fn building_record(kind: i32, raw: &BuildingFile, o: IVec3, rot: u32) -> BuildingRecord {
    let off = raw.offsets.map_or(IVec3::ZERO, |v| v.0.as_ivec3());
    let h = raw.height as i32;
    let (x, z) = interior_span(raw, rot);
    let cell = |v: i32| v as f32 * 4.0;
    let shop = match kind {
        CLOTHING_STORE => CLOTHING_STOCK.iter().map(|&(k, price)| ShopEntry { kind: k, price, extra: 0 }).collect(),
        BURGER_SHOP => vec![ShopEntry { kind: 0, price: BURGER_PRICE, extra: 0 }],
        _ => Vec::new(),
    };
    BuildingRecord {
        kind,
        interior_min: Vec3::new(cell(o.x + x.0), cell(o.y + off.y), cell(o.z + z.0)),
        interior_max: Vec3::new(cell(o.x + x.1), cell(o.y + h), cell(o.z + z.1)),
        shop,
    }
}
