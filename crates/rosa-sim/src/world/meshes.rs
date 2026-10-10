use glam::{IVec3, Vec3};
use rosa_map::file_types::sbl::BlockFile;

use crate::world::{
    blocks::{BlockTable, CURB_RAMP, DOOR_FRAME, LAMP, WINDOW_FRAME, slope_id},
    collide::calculate_face_normal,
    mesh::{BlockMesh, CUBE_CORNERS, Degree, FACE_DOWN, FACE_POS_X, FACE_POS_Z, MeshBuilder, Q, dir_flag},
};

const LINEAR: Option<Degree> = Some(Degree::Linear);

pub struct BlockMeshes {
    pub meshes: Vec<BlockMesh>,
    pub b: MeshBuilder,
}

impl BlockMeshes {
    pub fn new(tables: &BlockTable, customs: &[&BlockFile]) -> Self {
        let mut new_mesh = Self { meshes: Vec::new(), b: MeshBuilder::default() };

        for (k, f) in customs.iter().enumerate() {
            new_mesh.custom_block(BlockTable::custom(k), f);
        }

        new_mesh.builtin(tables);

        new_mesh
    }

    pub fn get(&self, id: u32) -> Option<&BlockMesh> {
        self.meshes.get(id as usize)
    }

    fn store(&mut self, id: u32, dims: IVec3) {
        let m = self.b.finalize(dims);

        let id = id as usize;

        if self.meshes.len() <= id {
            self.meshes.resize(id + 1, BlockMesh::default());
        }

        self.meshes[id] = m;
    }

    fn store_turns(&mut self, id: u32, dims: IVec3) {
        self.store(id, dims);
        let mut d = dims;
        for k in 1..4 {
            d = self.b.turn(d);
            self.store(id + k, d);
        }
    }

    fn custom_block(&mut self, id: u32, f: &BlockFile) {
        let b = &mut self.b;
        b.reset();
        for bx in &f.boxes {
            let p: [Vec3; 8] = std::array::from_fn(|i| bx.pos[i].0);
            b.add_box(&p, bx.texture[0], bx.side_flag);
        }
        for sf in &f.block_surface {
            for (i, v) in sf.vertex_data.iter().flatten().enumerate() {
                b.ctrl[i] = v.pos.0;
            }
            let mut flags = sf.texture;
            if sf.project_texture != 0 {
                flags |= dir_flag(calculate_face_normal(b.ctrl[0], b.ctrl[3], b.ctrl[15]));
            }
            b.append_bezier(sf.order_y as i32, Degree::from_i32(sf.order_x as i32), sf.tess_y as i32, Degree::from_i32(sf.tess_x as i32), flags);
        }
        let (w, h, d) = (f.size.0.x as i32, f.size.0.y as i32, f.size.0.z as i32);
        let mut faces = |on: u32, dir: i32, outer: i32, inner: i32, at: &dyn Fn(f32, f32) -> Vec3| {
            if on != 0 {
                for o in 0..outer {
                    for i in 0..inner {
                        b.face_by_direction(dir, at(o as f32, i as f32));
                    }
                }
            }
        };
        let (wf, hf, df) = (w as f32, h as f32, d as f32);
        faces(f.floor, 0, d, w, &|o, i| Vec3::new(i, 0.0, o));
        faces(f.ceiling, 5, d, w, &|o, i| Vec3::new(i, hf - 1.0, o));
        faces(f.wall_pz, 3, h, w, &|o, i| Vec3::new(i, o, df - 1.0));
        faces(f.wall_nx, 4, h, d, &|o, i| Vec3::new(0.0, o, i));
        faces(f.wall_px, 1, h, w, &|o, i| Vec3::new(i, o, 0.0));
        faces(f.wall_nz, 2, h, d, &|o, i| Vec3::new(wf - 1.0, o, i));
        self.store_turns(id, IVec3::new(w, h, d));
    }

    fn builtin(&mut self, tables: &BlockTable) {
        let one = IVec3::ONE;

        let b = &mut self.b;
        b.reset();
        for (i, p) in [
            (0, [0.0, 0.0, 0.0]),
            (3, [1.0, 0.0, 0.0]),
            (4, [0.0, 0.0, 0.25]),
            (7, [1.0, 0.0, 0.25]),
            (8, [0.0, Q, 0.75]),
            (11, [1.0, Q, 0.75]),
            (12, [0.0, Q, 1.0]),
            (15, [1.0, Q, 1.0]),
        ] {
            b.ctrl[i] = Vec3::from(p);
        }
        b.append_bezier(1, LINEAR, 8, Some(Degree::Cubic), FACE_DOWN);
        self.store_turns(CURB_RAMP, one);

        for i in 0..16u32 {
            let b = &mut self.b;
            b.reset();
            let mut quad = |p0, p3, p15, p12, flags| {
                b.set_corners(p0, p3, p15, p12);
                b.append_bezier(1, LINEAR, 1, LINEAR, flags);
            };
            quad([0.0, Q, 0.0], [1.0, Q, 0.0], [1.0, Q, 1.0], [0.0, Q, 1.0], 5);
            if i & 1 != 0 {
                quad([1.0, Q, 0.0], [0.0, Q, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0);
            }
            if i & 8 != 0 {
                quad([0.0, Q, 0.0], [0.0, Q, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 0);
            }
            if i & 4 != 0 {
                quad([0.0, Q, 1.0], [1.0, Q, 1.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0], 0);
            }
            if i & 2 != 0 {
                quad([1.0, Q, 1.0], [1.0, Q, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], 0);
            }
            self.store(i, one);
        }

        let mut height = 0.0f32;
        for w in 4..=12i32 {
            height = 1.0 / w as f32;
            let corners = |row: i32, lo: f32, hi: f32| {
                [
                    if row <= 1 { lo } else { hi },
                    if (row - 1) as u32 <= 1 { lo } else { hi },
                    if (row - 2) as u32 <= 1 { lo } else { hi },
                    if row == 0 || row == 3 { lo } else { hi },
                ]
            };
            for row in 0..4i32 {
                let mut y = 0.0f32;
                for k in 0..w {
                    let y0 = y + Q;
                    let y1 = height + y0;
                    let top = corners(row, y0, y1);
                    let p: [Vec3; 8] = std::array::from_fn(|i| {
                        let u = CUBE_CORNERS[i];
                        let ty = top[i & 3];
                        Vec3::new(u.x, if i < 4 { ty - Q } else { ty }, u.z)
                    });
                    self.b.reset();
                    self.b.add_box(&p, 0, 62);
                    self.store(slope_id(w, true, row, k).unwrap(), IVec3::new(1, (y1 >= 1.0) as i32 + 1, 1));
                    y += height;
                }
            }
            for row in 0..4i32 {
                let mut y = 0.0f32;
                for k in 0..w {
                    let y1 = height + y;
                    let [c0, c1, c2, c3] = corners(row, y, y1);
                    self.b.reset();
                    self.b.set_corners([0.0, c0, 0.0], [1.0, c1, 0.0], [1.0, c2, 1.0], [0.0, c3, 1.0]);
                    self.b.append_bezier(1, LINEAR, 1, LINEAR, 0);
                    self.store(slope_id(w, false, row, k).unwrap(), one);
                    y = y1;
                }
            }
        }

        for i in 0..4u32 {
            let h = |up: bool| if up { 1.0 } else { 0.0 };
            self.b.reset();
            self.b.set_corners(
                [0.0, h(i == 0 || i == 3), 0.0],
                [1.0, h(i <= 1), 0.0],
                [1.0, h(i.wrapping_sub(1) <= 1), 1.0],
                [0.0, h(i.wrapping_sub(2) <= 1), 1.0],
            );
            self.b.append_bezier(1, LINEAR, 1, LINEAR, 0);
            self.store(tables.edge_cap(i), one);
        }

        let ones = [1i32; 6];
        let z = Vec3::ZERO;
        self.b.reset();
        self.b.box_geometry(0, 34, &ones, z, Vec3::new(1.0, height, 1.0));
        self.b.box_geometry(0, 42, &ones, z, Vec3::new(1.0, 0.3125, Q));
        self.store_turns(WINDOW_FRAME, one);

        self.b.reset();
        self.b.box_geometry(0, 50, &ones, z, Vec3::new(1.0, height, 1.0));
        self.b.box_geometry(0, 50, &ones, z, Vec3::new(Q, 0.3125, Q));
        self.store_turns(DOOR_FRAME, one);

        self.b.reset();
        self.b.box_geometry(0, 42, &ones, z, Vec3::new(1.0, Q, 1.0));
        self.b.box_geometry(0, 63, &[0; 6], Vec3::new(0.5, 0.0, 0.5), Vec3::new(Q, 0.3125, Q));
        self.b.box_geometry(0, 63, &[0; 6], Vec3::new(0.0, 0.25, 0.4375), Vec3::new(1.0, 0.1875, 0.125));
        self.store_turns(LAMP, one);
    }

    pub fn build_generated(&mut self, id: u32, key: u32, portals: &PortalGroups) {
        let modes: [i32; 4] = std::array::from_fn(|i| ((key >> (4 * i)) & 15) as i32);
        let group = ((key >> 18) & 15) as usize;
        let floor = key & (1 << 16) != 0;
        let top = key & (1 << 17) != 0;
        let portal = |m: i32| ((m - 3) as u32 <= 12).then(|| portals.get(group, (m - 3) as usize));
        let b = &mut self.b;
        b.reset();
        for (i, &m) in modes.iter().enumerate() {
            if let Some(p) = portal(m)
                && (p.ty == 0 || i <= 1)
            {
                b.portal_windows(i as i32, p);
            }
        }
        let mut mask = floor as u32;
        for (i, &m) in modes.iter().enumerate() {
            if m > 0 && m != 2 {
                mask |= 2 << i;
            }
        }
        if top {
            if floor {
                b.inset_face(0, mask | 32);
            }
            b.inset_face(5, mask | 32);
        } else if floor {
            b.inset_face(0, mask);
        }
        for (i, &m) in modes.iter().enumerate() {
            if m == 1 {
                b.face_by_direction(i as i32 + 1, Vec3::ZERO);
            }
            if let Some(p) = portal(m) {
                b.portal_frame(i as i32, p);
            }
            if m == 2 {
                b.edge_face_pair(i as i32, Q, Q);
            }
        }
        self.store(id, IVec3::ONE);
    }
}

#[derive(Clone, Default, Debug)]
pub struct Portal {
    pub ty: i32,
    pub rect: [f32; 4],
    pub windows: Vec<[f32; 4]>,
    pub beams: Vec<(i32, [f32; 4])>,
}

#[derive(Default)]
pub struct PortalGroups {
    groups: Vec<Vec<Portal>>,
    empty: Portal,
}

impl PortalGroups {
    pub fn load(dir: &std::path::Path, names: &[String]) -> Self {
        let groups = names
            .iter()
            .take(64)
            .map(|name| std::fs::read(dir.join(format!("{name}.sbl"))).map(|b| Self::parse(&b)).unwrap_or_default())
            .collect();
        Self { groups, empty: Portal::default() }
    }

    fn parse(b: &[u8]) -> Vec<Portal> {
        let mut o = 0usize;
        let mut word = || {
            let v = b.get(o..o + 4).map_or([0; 4], |s| s.try_into().unwrap());
            o += 4;
            v
        };
        if i32::from_le_bytes(word()) != 1 {
            return Vec::new();
        }
        let list = |word: &mut dyn FnMut() -> [u8; 4]| {
            let n = i32::from_le_bytes(word());
            (0..n.max(0)).map(|_| (i32::from_le_bytes(word()), std::array::from_fn(|_| f32::from_le_bytes(word())))).collect::<Vec<_>>()
        };
        (0..16)
            .map(|_| {
                let ty = i32::from_le_bytes(word());
                let rect = std::array::from_fn(|_| f32::from_le_bytes(word()));
                for _ in 0..i32::from_le_bytes(word()).max(0) * 3 {
                    word();
                }
                let windows = list(&mut word).into_iter().map(|(_, r)| r).collect();
                let beams = list(&mut word);
                Portal { ty, rect, windows, beams }
            })
            .collect()
    }

    pub fn get(&self, group: usize, idx: usize) -> &Portal {
        self.groups.get(group).and_then(|g| g.get(idx)).unwrap_or(&self.empty)
    }
}

impl MeshBuilder {
    pub fn edge_face_pair(&mut self, turns: i32, e: f32, d: f32) {
        self.set_corners([0.0, 1.0, d], [e, 1.0, d], [e, 0.0, d], [0.0, 0.0, d]);

        let f = self.rotate_grid(turns, FACE_POS_Z);
        self.append_bezier(1, LINEAR, 1, LINEAR, f | (turns + 1) as u32);
        self.set_corners([e, 1.0, d], [e, 1.0, 0.0], [e, 0.0, 0.0], [e, 0.0, d]);

        let f = self.rotate_grid(turns, FACE_POS_X);
        self.append_bezier(1, LINEAR, 1, LINEAR, f | (((turns + 3) & 3) + 1) as u32);
    }

    pub fn portal_windows(&mut self, turns: i32, p: &Portal) {
        let depth = if p.ty == 1 { 0.003_906_25 } else { 0.015_625 };

        for &[x, w, y, h] in &p.windows {
            let x = if p.ty != 1 || turns <= 1 { 1.0 - (x + w) } else { x };
            let (right, top) = (w + x, h + y);
            self.set_corners([x, top, depth], [right, top, depth], [right, y, depth], [x, y, depth]);
            self.rotate_grid(turns, 0);
            self.walls.push([self.ctrl[0], self.ctrl[3], self.ctrl[15], self.ctrl[12]]);
        }
    }

    pub fn portal_frame(&mut self, turns: i32, p: &Portal) {
        let [px, pw, py, ph] = p.rect;
        let mirror = !(p.ty == 1 && turns > 1);
        let back = if p.ty != 1 { 8 } else { 0 };
        let mut c = [0, 1, 2, 11, 4, 5];
        let x = if mirror { 1.0 - (px + pw) } else { px };

        if x > 0.0 {
            c[4] = 6;
            self.box_geometry(turns, 18 | back, &c, Vec3::ZERO, Vec3::new(x, 1.0, Q));
        }

        let right = pw + x;
        if 1.0 > right {
            c[2] = 6;
            self.box_geometry(turns, 6 | back, &c, Vec3::new(right, 0.0, 0.0), Vec3::new(1.0 - right, 1.0, Q));
        }

        if py > 0.0 {
            c[5] = 6;
            self.box_geometry(turns, 34 | back, &c, Vec3::ZERO, Vec3::new(1.0, py, Q));
        }

        let top = py + ph;
        if 1.0 > top {
            c[0] = 6;
            self.box_geometry(turns, 3 | back, &c, Vec3::new(0.0, top, 0.0), Vec3::new(1.0, 1.0 - top, Q));
        }

        for &(ty, [bx, bw, by, bh]) in &p.beams {
            let bx = if mirror { 1.0 - (bx + bw) } else { bx };

            let (z, d) = match ty {
                0 => (0.0, Q),
                1 => {
                    c = [6; 6];
                    (-Q, 0.1875)
                }
                2 => {
                    c = [6; 6];
                    (-Q, 0.3125)
                }
                _ => {
                    c = [7; 6];
                    (0.007_812_5, 0.015_625)
                }
            };

            self.box_geometry(turns, 63, &c, Vec3::new(bx, by, z), Vec3::new(bw, bh, d));
        }
    }
}
