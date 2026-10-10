use std::path::Path;

use glam::{IVec3, Vec3};
use rosa_map::file_types::sis::ItemSetFile;
use rosa_physics::rotation::{IDENTITY, RotMatrix, rotate_orientation};
use rosa_protocol::clientbound::game::ItemKind;

use crate::sim::hull::ConvexHull;

/// The sets setup_item_sets builds itself; the .sis files follow.
pub const BUILT_IN_SETS: usize = 4;
/// The π the binary turns a cell's quarter turns by.
const HALF_TURN: f64 = 180.0_f64.to_radians();
const WATERMELON_LIE: f32 = 90.0_f32.to_radians();
const WATERMELON_TILT: f32 = 30.0_f32.to_radians();

/// One item of a set: its type, where it sits from the cell's floor centre and how it is turned.
#[derive(Clone, Debug)]
pub struct SetEntry {
    pub kind: i32,
    pub pos: Vec3,
    pub rot: RotMatrix,
}

#[derive(Clone, Debug, Default)]
pub struct ItemSet {
    pub name: String,
    pub entries: Vec<SetEntry>,
}

/// The item a trace hit in an item-set cell (line_intersect_result +0x5c, +0x60, +0x64).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetHit {
    pub set: u32,
    pub turn: u32,
    pub index: usize,
}

impl SetHit {
    /// The cell word spawn_item_from_grid_cell takes: the set and its quarter turns.
    pub fn cell_word(&self) -> u32 {
        (self.turn << 24) + self.set
    }
}

/// The item sets and the collision hull of each item type, which traces test set items against.
#[derive(Clone, Debug, Default)]
pub struct ItemSets {
    pub sets: Vec<ItemSet>,
    pub hulls: Vec<Option<ConvexHull>>,
}

fn entry(kind: ItemKind, x: f32, y: f32) -> SetEntry {
    SetEntry { kind: kind as i32, pos: Vec3::new(x, y, 0.0), rot: IDENTITY }
}

/// The table with three boxes stacked either side.
fn table_set() -> ItemSet {
    let entries = [(ItemKind::Table, 0.0, 0.75), (ItemKind::Box, 0.5, 1.5), (ItemKind::Box, 0.5, 2.0), (ItemKind::Box, 0.5, 2.5), (ItemKind::Box, -0.5, 1.5), (ItemKind::Box, -0.5, 2.0), (ItemKind::Box, -0.5, 2.5)];
    ItemSet { name: String::new(), entries: entries.iter().map(|&(k, x, y)| entry(k, x, y)).collect() }
}

/// The big box on two boxes with a row of four watermelons lying on it and three more tipped against them.
fn melon_set() -> ItemSet {
    let mut entries = vec![entry(ItemKind::BigBox, 0.0, 0.875), entry(ItemKind::Box, -0.5, 0.5), entry(ItemKind::Box, 0.5, 0.5)];
    entries.extend((0..4).map(|i| {
        let mut rot = IDENTITY;
        let axis = rot[0];
        rotate_orientation(&mut rot, axis, WATERMELON_LIE);
        SetEntry { kind: ItemKind::Watermelon as i32, pos: Vec3::new(0.5 - i as f32 * 0.25 * 1.5, 1.15625, 0.0), rot }
    }));
    entries.extend((0..3).map(|i| {
        let mut rot = IDENTITY;
        let axis = rot[0];
        rotate_orientation(&mut rot, axis, WATERMELON_LIE);
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, WATERMELON_TILT);
        SetEntry { kind: ItemKind::Watermelon as i32, pos: Vec3::new(0.5 - i as f32 * 0.25 * 1.5 - 0.1875, 1.4375, 0.0), rot }
    }));
    ItemSet { name: String::new(), entries }
}

/// Eight boxes, two wide and four high.
fn box_stack_set() -> ItemSet {
    let entries = (0..8).map(|i: i32| entry(ItemKind::Box, (i & 1) as f32 * 0.5 - 0.25, (i >> 1) as f32 * 0.5 + 0.5)).collect();
    ItemSet { name: String::new(), entries }
}

impl ItemSets {
    /// setup_item_sets: the built-in sets, then data/itemset/*.sis (`names`, in load order) from index 4, each item's
    /// type the last of `type_names` its name matches.
    pub fn load(data_dir: &Path, names: &[String], type_names: &[String], hulls: Vec<Option<ConvexHull>>) -> Self {
        let mut sets = vec![ItemSet::default(), table_set(), melon_set(), box_stack_set()];
        for name in names {
            let file = ItemSetFile::load(&data_dir.join("itemset").join(format!("{name}.sis"))).ok();
            let entries = file
                .map(|f| {
                    f.entries
                        .iter()
                        .map(|e| SetEntry {
                            kind: type_names.iter().rposition(|t| t.as_str() == e.name.as_str()).map_or(0, |k| k as i32),
                            pos: e.pos.0,
                            rot: e.rot.map(|r| r.0),
                        })
                        .collect()
                })
                .unwrap_or_default();
            sets.push(ItemSet { name: name.clone(), entries });
        }
        Self { sets, hulls }
    }

    pub fn set(&self, word: u32) -> Option<&ItemSet> {
        self.sets.get((word & 1023) as usize)
    }

    pub fn hull(&self, kind: i32) -> Option<&ConvexHull> {
        self.hulls.get(kind as usize)?.as_ref()
    }
}

/// Where item `e` of a set stands in `cell` (block size `s`) turned `turn` quarter turns, and its orientation.
pub fn entry_pose(e: &SetEntry, cell: IVec3, s: f32, turn: u32) -> (Vec3, RotMatrix) {
    let half = 0.5 * s;
    let base = Vec3::new(cell.x as f32 * s + half, cell.y as f32 * s, cell.z as f32 * s + half);
    let angle = (turn as f64 * HALF_TURN * 0.5) as f32;
    let mut m = IDENTITY;
    rotate_orientation(&mut m, Vec3::Y, angle);
    let [r0, r1, r2] = m;
    let p = e.pos;
    let pos = Vec3::new(
        ((r0.x * p.x + base.x) + r1.x * p.y) + r2.x * p.z,
        ((r0.y * p.x + base.y) + r1.y * p.y) + r2.y * p.z,
        ((r0.z * p.x + base.z) + r1.z * p.y) + r2.z * p.z,
    );
    let mut rot = e.rot;
    rotate_orientation(&mut rot, Vec3::Y, angle);
    (pos, rot)
}
