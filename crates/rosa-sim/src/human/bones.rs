use glam::Vec3;

pub const BONE_COUNT: usize = 16;

#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoneId {
    Pelvis = 0,
    Stomach = 1,
    Torso = 2,
    Head = 3,
    ShoulderLeft = 4,
    ForearmLeft = 5,
    HandLeft = 6,
    ShoulderRight = 7,
    ForearmRight = 8,
    HandRight = 9,
    ThighLeft = 10,
    ShinLeft = 11,
    FootLeft = 12,
    ThighRight = 13,
    ShinRight = 14,
    FootRight = 15,
}

// TODO: name `unk_3c`, `shape` and `shape_size` once their readers are ported
pub struct BoneTemplate {
    pub parent: usize,
    pub offset: Vec3,
    pub joint: Vec3,
    pub margin: f32,
    pub half_extents: Vec3,
    pub mass: f32,
    pub unk_3c: f32,
    pub limit_min: Vec3,
    pub limit_max: Vec3,
    pub shape: i32,
    pub shape_size: [f32; 2],
}

impl BoneTemplate {
    pub fn inv_inertia(&self) -> Vec3 {
        let (s, m) = (self.half_extents, self.margin);
        let (a, b, c) = ((s.y + s.y) + m, (s.z + s.z) + m, (s.x + s.x) + m);
        let (a, b, c) = (a * a, b * b, c * c);
        Vec3::new(1.0 / ((a + b) / 9.0), 1.0 / ((b + c) / 9.0), 1.0 / ((a + c) / 9.0))
    }

    pub fn rest_offset(id: usize) -> Vec3 {
        let mut o = BONES[id].offset;
        let mut p = BONES[id].parent;
        while p != 0 {
            o = Vec3::new(o.x + BONES[p].offset.x, o.y + BONES[p].offset.y, o.z + BONES[p].offset.z);
            p = BONES[p].parent;
        }
        o
    }
}

const fn bone(parent: usize, offset: Vec3, joint: Vec3, half_extents: Vec3, mass: f32, limit_min: Vec3, limit_max: Vec3, shape: i32, shape_size: [f32; 2]) -> BoneTemplate {
    BoneTemplate { parent, offset, joint, margin: 0.0625, half_extents, mass, unk_3c: 0.0, limit_min, limit_max, shape, shape_size }
}

const fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

const TORSO: Vec3 = v(0.25, 0.125, 0.09375);
const SPINE_MIN: Vec3 = v(-0.7853982, -0.7853982, -0.3926991);
const SPINE_MAX: Vec3 = v(0.3926991, 0.7853982, 0.3926991);
const ARM: Vec3 = v(0.0625, 0.1640625, 0.0625);
const HAND: Vec3 = v(0.0625, 0.0625, 0.0625);
const SHOULDER_MIN: Vec3 = v(-1.5707964, -1.1780972, -1.5707964);
const SHOULDER_MAX: Vec3 = v(1.5707964, 1.1780972, 1.5707964);
const ELBOW_MAX: Vec3 = v(1.5707964, 0.0, 0.0);
const WRIST_MIN: Vec3 = v(-1.5707964, -0.7853982, -1.9634954);
const WRIST_MAX: Vec3 = v(1.5707964, 0.7853982, 1.9634954);
const LEG: Vec3 = v(0.09375, 0.25, 0.09375);
const FOOT: Vec3 = v(0.09375, 0.0625, 0.1875);
const HIP_MIN: Vec3 = v(-1.3744467, -0.3926991, -0.3926991);
const HIP_MAX: Vec3 = v(1.5707964, 0.3926991, 0.3926991);
const KNEE_MIN: Vec3 = v(-2.7488935, 0.0, 0.0);
const ANKLE_MIN: Vec3 = v(-0.7853982, -0.7853982, -0.3926991);
const ANKLE_MAX: Vec3 = v(0.7853982, 0.7853982, 0.3926991);

pub static BONES: [BoneTemplate; BONE_COUNT] = [
    bone(0, v(0.0, 0.0, 0.0), v(0.0, -0.125, 0.0), TORSO, 14.0, Vec3::ZERO, Vec3::ZERO, 0, [0.1875, 0.125]),
    bone(0, v(0.0, 0.25, 0.0), v(0.0, 0.125, 0.0), TORSO, 14.0, SPINE_MIN, SPINE_MAX, 0, [0.125, 0.125]),
    bone(1, v(0.0, 0.25, 0.0), v(0.0, 0.125, 0.0), TORSO, 14.0, SPINE_MIN, SPINE_MAX, 0, [0.125, 0.125]),
    bone(2, v(0.0, 0.28125, 0.0), v(0.0, 0.1875, 0.0), v(0.125, 0.125, 0.125), 8.0, v(-0.7853982, -1.1780972, -0.5890486), v(0.7853982, 1.1780972, 0.5890486), 1, [0.0625, 0.109375]),
    BoneTemplate { unk_3c: 0.375, ..bone(2, v(-0.1875, -0.1015625, 0.0), v(-0.1875, 0.0625, 0.0), ARM, 3.0, SHOULDER_MIN, SHOULDER_MAX, 1, [0.25, 0.0625]) },
    bone(4, v(0.0, -0.328125, 0.0), v(0.0, -0.1640625, 0.0), ARM, 2.5, Vec3::ZERO, ELBOW_MAX, 1, [0.25, 0.0625]),
    bone(5, v(0.0, -0.2265625, 0.0), v(0.0, -0.1640625, 0.0), HAND, 1.5, WRIST_MIN, WRIST_MAX, 1, [0.0625, 0.0625]),
    bone(2, v(0.1875, -0.1015625, 0.0), v(0.1875, 0.0625, 0.0), ARM, 3.0, SHOULDER_MIN, SHOULDER_MAX, 1, [0.25, 0.0625]),
    bone(7, v(-0.0, -0.328125, 0.0), v(-0.0, -0.1640625, 0.0), ARM, 2.5, Vec3::ZERO, ELBOW_MAX, 1, [0.25, 0.0625]),
    bone(8, v(-0.0, -0.2265625, 0.0), v(-0.0, -0.1640625, 0.0), HAND, 1.5, WRIST_MIN, WRIST_MAX, 1, [0.0625, 0.0625]),
    bone(0, v(-0.09375, -0.25, 0.0), v(-0.09375, 0.0, 0.0), LEG, 12.0, HIP_MIN, HIP_MAX, 1, [0.375, 0.09375]),
    bone(10, v(0.0, -0.5, 0.0), v(0.0, -0.25, 0.0), LEG, 7.0, KNEE_MIN, Vec3::ZERO, 1, [0.28125, 0.0625]),
    bone(11, v(0.0, -0.25, -0.0625), v(0.0, -0.25, 0.0), FOOT, 3.0, ANKLE_MIN, ANKLE_MAX, 2, [0.125, 0.0625]),
    bone(0, v(0.09375, -0.25, 0.0), v(0.09375, 0.0, 0.0), LEG, 12.0, HIP_MIN, HIP_MAX, 1, [0.375, 0.09375]),
    bone(13, v(-0.0, -0.5, 0.0), v(-0.0, -0.25, 0.0), LEG, 7.0, KNEE_MIN, Vec3::ZERO, 1, [0.28125, 0.0625]),
    bone(14, v(-0.0, -0.25, -0.0625), v(-0.0, -0.25, 0.0), FOOT, 3.0, ANKLE_MIN, ANKLE_MAX, 2, [0.125, 0.0625]),
];
