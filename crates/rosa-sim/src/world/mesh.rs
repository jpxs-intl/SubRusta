use glam::{IVec3, Vec3};

use crate::world::collide::calculate_face_normal;

pub const FACE_NEG_X: u32 = 1 << 16;
pub const FACE_POS_X: u32 = 2 << 16;
pub const FACE_DOWN: u32 = 3 << 16;
pub const FACE_UP: u32 = 4 << 16;
pub const FACE_NEG_Z: u32 = 5 << 16;
pub const FACE_POS_Z: u32 = 6 << 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Degree {
    Linear = 1,
    Quadratic = 2,
    Cubic = 3,
}

impl Degree {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            1 => Some(Self::Linear),
            2 => Some(Self::Quadratic),
            3 => Some(Self::Cubic),
            _ => None,
        }
    }
}

pub fn bezier(deg: Option<Degree>, t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let om = 1.0 - t;
    match deg {
        Some(Degree::Linear) => p0 * om + t * p3,
        Some(Degree::Quadratic) => (((p1 + p1) * om * t) + om * (p0 * om)) + t * (p3 * t),
        Some(Degree::Cubic) => {
            ((p0 * om * om * om + (p1 * 3.0) * om * om * t) + (om * (p2 * 3.0)) * t * t) + t * ((p3 * t) * t)
        }
        None => 0.0,
    }
}

#[derive(Clone, Default, Debug)]
pub struct BlockMesh {
    pub verts: Vec<Vec3>,
    pub quads: Vec<[i32; 4]>,
    pub mats: Vec<u32>,
    pub walls: Vec<[Vec3; 4]>,
    pub dims: IVec3,
}

#[derive(Default)]
pub struct MeshBuilder {
    pub ctrl: [Vec3; 16],
    pub pos: Vec<Vec3>,
    pub quads: Vec<[i32; 4]>,
    pub flags: Vec<u32>,
    pub walls: Vec<[Vec3; 4]>,
}

impl MeshBuilder {
    pub fn reset(&mut self) {
        self.pos.clear();
        self.quads.clear();
        self.flags.clear();
        self.walls.clear();
    }

    pub fn set_corners(&mut self, p0: [f32; 3], p3: [f32; 3], p15: [f32; 3], p12: [f32; 3]) {
        self.ctrl[0] = Vec3::from(p0);
        self.ctrl[3] = Vec3::from(p3);
        self.ctrl[15] = Vec3::from(p15);
        self.ctrl[12] = Vec3::from(p12);
    }

    pub fn append_bezier(&mut self, u_seg: i32, u_deg: Option<Degree>, v_seg: i32, v_deg: Option<Degree>, face_flags: u32) {
        let first = self.pos.len() as i32;
        let mut sv = 0.0f32;
        for _ in 0..=v_seg {
            let row: [Vec3; 4] = std::array::from_fn(|c| {
                let col = |k: usize| {
                    let p = |r: usize| self.ctrl[r * 4 + c][k];
                    bezier(v_deg, sv, p(0), p(1), p(2), p(3))
                };
                Vec3::new(col(0), col(1), col(2))
            });
            let mut su = 0.0f32;
            for _ in 0..=u_seg {
                let e = |k: usize| bezier(u_deg, su, row[0][k], row[1][k], row[2][k], row[3][k]);
                self.pos.push(Vec3::new(e(0), e(1), e(2)));
                su += 1.0 / u_seg as f32;
            }
            sv += 1.0 / v_seg as f32;
        }

        let mut next = self.pos.len() as i32;
        let mut top = first;
        let mut bottom = top + u_seg + 1;
        for r in 0..v_seg {
            for q in 0..u_seg {
                let mut dup = |i: i32| {
                    let p = self.pos[i as usize];
                    self.pos.push(p);
                    next += 1;
                    next - 1
                };
                let (a, b) = if q > 0 || r > 0 {
                    let a = dup(top + q);
                    (a, if r == 0 { top + q + 1 } else { dup(top + q + 1) })
                } else {
                    (top + q, top + q + 1)
                };
                let d = if q > 0 { dup(bottom + q) } else { bottom + q };
                self.quads.push([a, b, bottom + 1 + q, d]);
                self.flags.push(face_flags);
            }
            top += u_seg + 1;
            bottom += u_seg + 1;
        }
    }

    pub fn add_box(&mut self, p: &[Vec3; 8], tex: u32, mask: u32) {
        for (r, f) in CUBE_FACES.iter().enumerate() {
            if mask & (1 << r) == 0 {
                continue;
            }
            let n = calculate_face_normal(p[f[0]], p[f[1]], p[f[2]]);
            let v = self.pos.len() as i32;
            self.pos.extend(f.iter().map(|&i| p[i]));
            self.flags.push(tex | dir_flag(n));
            self.quads.push([v, v + 1, v + 2, v + 3]);
        }
    }

    pub fn rotate_grid(&mut self, turns: i32, mut flags: u32) -> u32 {
        for _ in 0..turns.max(0) {
            for p in &mut self.ctrl {
                (p.x, p.z) = (1.0 - p.z, p.x);
            }
            flags = match flags {
                FACE_POS_Z => FACE_NEG_X,
                FACE_NEG_X => FACE_NEG_Z,
                FACE_NEG_Z => FACE_POS_X,
                FACE_POS_X => FACE_POS_Z,
                f => f,
            };
        }
        flags
    }

    pub fn face_by_direction(&mut self, dir: i32, off: Vec3) {
        let flags = match dir {
            0 => {
                self.set_corners([0.0, Q, 0.0], [1.0, Q, 0.0], [1.0, Q, 1.0], [0.0, Q, 1.0]);
                FACE_UP
            }
            5 => {
                self.set_corners([0.0, 0.875, 1.0], [1.0, 0.875, 1.0], [1.0, 0.875, 0.0], [0.0, 0.875, 0.0]);
                FACE_DOWN
            }
            _ => {
                self.set_corners([0.0, 1.0, Q], [1.0, 1.0, Q], [1.0, 0.0, Q], [0.0, 0.0, Q]);
                self.rotate_grid(dir - 1, FACE_POS_Z)
            }
        };
        for i in [0, 3, 15, 12] {
            self.ctrl[i] += off;
        }
        self.append_bezier(1, Some(Degree::Linear), 1, Some(Degree::Linear), flags | dir as u32);
    }

    pub fn inset_face(&mut self, dir: i32, mask: u32) {
        const E: f32 = 0.9375;
        let flags = match dir {
            0 => {
                let (mut p0, mut p3, mut p15, mut p12) = ([0.0, Q, 0.0], [1.0, Q, 0.0], [1.0, Q, 1.0], [0.0, Q, 1.0]);
                if mask & 2 != 0 {
                    (p0[2], p3[2]) = (Q, Q);
                }
                if mask & 4 != 0 {
                    (p3[0], p15[0]) = (E, E);
                }
                if mask & 8 != 0 {
                    (p15[2], p12[2]) = (E, E);
                }
                if mask & 16 != 0 {
                    (p0[0], p12[0]) = (Q, Q);
                }
                self.set_corners(p0, p3, p15, p12);
                FACE_UP
            }
            5 => {
                let (mut p0, mut p3, mut p15, mut p12) = ([0.0, 0.875, 1.0], [1.0, 0.875, 1.0], [1.0, 0.875, 0.0], [0.0, 0.875, 0.0]);
                if mask & 2 != 0 {
                    (p15[2], p12[2]) = (Q, Q);
                }
                if mask & 4 != 0 {
                    (p3[0], p15[0]) = (E, E);
                }
                if mask & 8 != 0 {
                    (p0[2], p3[2]) = (E, E);
                }
                if mask & 16 != 0 {
                    (p0[0], p12[0]) = (Q, Q);
                }
                self.set_corners(p0, p3, p15, p12);
                FACE_DOWN
            }
            _ => {
                self.set_corners([0.0, 1.0, Q], [1.0, 1.0, Q], [1.0, 0.0, Q], [0.0, 0.0, Q]);
                self.rotate_grid(dir - 1, FACE_POS_Z)
            }
        };
        self.append_bezier(1, Some(Degree::Linear), 1, Some(Degree::Linear), flags | dir as u32);
    }

    pub fn box_geometry(&mut self, turns: i32, mask: u32, codes: &[i32; 6], o: Vec3, s: Vec3) {
        for (r, f) in CUBE_FACES.iter().enumerate() {
            if mask & (1 << r) == 0 {
                continue;
            }
            let c = |i: usize| {
                let u = CUBE_CORNERS[f[i]];
                Vec3::new(u.x * s.x + o.x, u.y * s.y + o.y, u.z * s.z + o.z)
            };
            self.ctrl[0] = c(0);
            self.ctrl[3] = c(1);
            self.ctrl[15] = c(2);
            self.ctrl[12] = c(3);
            let rot = self.rotate_grid(turns, [FACE_DOWN, FACE_POS_Z, FACE_NEG_X, FACE_NEG_Z, FACE_POS_X, FACE_UP][r]);
            let mut code = codes[r];
            if (code - 1) as u32 <= 3 {
                code = ((code - 1 + turns) & 3) + 1;
            }
            if (code - 9) as u32 <= 3 {
                code = ((code - 9 + turns) & 3) + 9;
            }
            self.append_bezier(1, Some(Degree::Linear), 1, Some(Degree::Linear), code as u32 | rot);
        }
    }

    pub fn turn(&mut self, dims: IVec3) -> IVec3 {
        let d = dims.z as f32;
        for p in &mut self.pos {
            (p.x, p.z) = (d - p.z, p.x);
        }
        for f in &mut self.flags {
            let v = *f;
            let dir = v & (15 << 16);
            if dir == FACE_DOWN || dir == FACE_UP {
                *f = (v & !(15 << 12)) | (((v & (15 << 12)) + (1 << 12)) & (3 << 12));
                continue;
            }
            let code = v & 65535;
            let next = if code.wrapping_sub(1) <= 2 { code + 1 } else if code == 4 { 1 } else { code };
            *f = match dir {
                0 => next,
                FACE_POS_Z => next | FACE_NEG_X,
                FACE_NEG_X => next | FACE_NEG_Z,
                FACE_NEG_Z => next | FACE_POS_X,
                FACE_POS_X => next | FACE_POS_Z,
                _ => v,
            };
        }
        IVec3::new(dims.z, dims.y, dims.x)
    }

    pub fn finalize(&mut self, dims: IVec3) -> BlockMesh {
        let mut k = 0usize;

        for i in 0..self.quads.len() {
            let on_face = (0..6).any(|r| {
                let axis = [0, 1, 2, 2, 1, 0][r];
                let val = if r <= 2 { 0.0 } else { 1.0 };

                self.quads[i].iter().all(|&vi| self.pos[vi as usize][axis] == val)
            });

            if on_face {
                self.quads.swap(k, i);
                self.flags.swap(k, i);

                k += 1;
            }
        }

        BlockMesh {
            verts: self.pos.clone(),
            quads: self.quads.clone(),
            mats: self.flags.iter().map(|f| f & 4095).collect(),
            walls: self.walls.clone(),
            dims,
        }
    }
}

pub fn dir_flag(n: Vec3) -> u32 {
    if n.y > 0.707 {
        FACE_UP
    } else if !(-0.707 <= n.y) {
        FACE_DOWN
    } else if n.x > 0.707 {
        FACE_POS_X
    } else if !(-0.707 <= n.x) {
        FACE_NEG_X
    } else if n.z > 0.707 {
        FACE_POS_Z
    } else {
        FACE_NEG_Z
    }
}

pub const Q: f32 = 0.0625;

pub const CUBE_CORNERS: [Vec3; 8] = [
    Vec3::new(0.0, 0.0, 0.0),
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(1.0, 0.0, 1.0),
    Vec3::new(0.0, 0.0, 1.0),
    Vec3::new(0.0, 1.0, 0.0),
    Vec3::new(1.0, 1.0, 0.0),
    Vec3::new(1.0, 1.0, 1.0),
    Vec3::new(0.0, 1.0, 1.0),
];
pub const CUBE_FACES: [[usize; 4]; 6] = [[3, 2, 1, 0], [7, 6, 2, 3], [4, 7, 3, 0], [5, 4, 0, 1], [6, 5, 1, 2], [4, 5, 6, 7]];
pub const CUBE_NORMALS: [Vec3; 6] = [Vec3::NEG_Y, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z, Vec3::X, Vec3::Y];
