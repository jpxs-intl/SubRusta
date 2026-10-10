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

impl BoneId {
    /// Every bone in index order.
    pub const ALL: [BoneId; BONE_COUNT] = [
        BoneId::Pelvis,
        BoneId::Stomach,
        BoneId::Torso,
        BoneId::Head,
        BoneId::ShoulderLeft,
        BoneId::ForearmLeft,
        BoneId::HandLeft,
        BoneId::ShoulderRight,
        BoneId::ForearmRight,
        BoneId::HandRight,
        BoneId::ThighLeft,
        BoneId::ShinLeft,
        BoneId::FootLeft,
        BoneId::ThighRight,
        BoneId::ShinRight,
        BoneId::FootRight,
    ];

    /// The bone at index `i`, if there is one.
    pub const fn from_index(i: usize) -> Option<BoneId> {
        if i < BONE_COUNT { Some(Self::ALL[i]) } else { None }
    }

    /// The bone's index into a human's bones.
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl TryFrom<usize> for BoneId {
    type Error = usize;

    fn try_from(i: usize) -> Result<Self, usize> {
        BoneId::from_index(i).ok_or(i)
    }
}

impl From<BoneId> for usize {
    fn from(b: BoneId) -> usize {
        b as usize
    }
}

// TODO: name `unk_3c`, `shape` and `shape_size` once their readers are ported
pub struct BoneTemplate {
    pub parent: BoneId,
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
        let mut p = BONES[id].parent as usize;

        while p != 0 {
            o = Vec3::new(o.x + BONES[p].offset.x, o.y + BONES[p].offset.y, o.z + BONES[p].offset.z);
            p = BONES[p].parent as usize;
        }

        o
    }
}

const fn bone(parent: BoneId, offset: Vec3, joint: Vec3, half_extents: Vec3, mass: f32, limit_min: Vec3, limit_max: Vec3, shape: i32, shape_size: [f32; 2]) -> BoneTemplate {
    BoneTemplate { parent, offset, joint, margin: 0.0625, half_extents, mass, unk_3c: 0.0, limit_min, limit_max, shape, shape_size }
}

const fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

const TORSO: Vec3 = v(0.25, 0.125, 0.09375);
const SPINE_MIN: Vec3 = v(-45.0_f32.to_radians(), -45.0_f32.to_radians(), -22.5_f32.to_radians());
const SPINE_MAX: Vec3 = v(22.5_f32.to_radians(), 45.0_f32.to_radians(), 22.5_f32.to_radians());
const ARM: Vec3 = v(0.0625, 21.0 / 128.0, 0.0625);
const HAND: Vec3 = v(0.0625, 0.0625, 0.0625);
const SHOULDER_MIN: Vec3 = v(-90.0_f32.to_radians(), -67.5_f32.to_radians(), -90.0_f32.to_radians());
const SHOULDER_MAX: Vec3 = v(90.0_f32.to_radians(), 67.5_f32.to_radians(), 90.0_f32.to_radians());
const ELBOW_MAX: Vec3 = v(90.0_f32.to_radians(), 0.0, 0.0);
const WRIST_MIN: Vec3 = v(-90.0_f32.to_radians(), -45.0_f32.to_radians(), -112.5_f32.to_radians());
const WRIST_MAX: Vec3 = v(90.0_f32.to_radians(), 45.0_f32.to_radians(), 112.5_f32.to_radians());
const LEG: Vec3 = v(0.09375, 0.25, 0.09375);
const FOOT: Vec3 = v(0.09375, 0.0625, 0.1875);
const HIP_MIN: Vec3 = v(-78.75_f32.to_radians(), -22.5_f32.to_radians(), -22.5_f32.to_radians());
const HIP_MAX: Vec3 = v(90.0_f32.to_radians(), 22.5_f32.to_radians(), 22.5_f32.to_radians());
const KNEE_MIN: Vec3 = v(-157.5_f32.to_radians(), 0.0, 0.0);
const ANKLE_MIN: Vec3 = v(-45.0_f32.to_radians(), -45.0_f32.to_radians(), -22.5_f32.to_radians());
const ANKLE_MAX: Vec3 = v(45.0_f32.to_radians(), 45.0_f32.to_radians(), 22.5_f32.to_radians());

pub static BONES: [BoneTemplate; BONE_COUNT] = [
    bone(BoneId::Pelvis, v(0.0, 0.0, 0.0), v(0.0, -0.125, 0.0), TORSO, 14.0, Vec3::ZERO, Vec3::ZERO, 0, [0.1875, 0.125]),
    bone(BoneId::Pelvis, v(0.0, 0.25, 0.0), v(0.0, 0.125, 0.0), TORSO, 14.0, SPINE_MIN, SPINE_MAX, 0, [0.125, 0.125]),
    bone(BoneId::Stomach, v(0.0, 0.25, 0.0), v(0.0, 0.125, 0.0), TORSO, 14.0, SPINE_MIN, SPINE_MAX, 0, [0.125, 0.125]),
    bone(BoneId::Torso, v(0.0, 0.28125, 0.0), v(0.0, 0.1875, 0.0), v(0.125, 0.125, 0.125), 8.0, v(-45.0_f32.to_radians(), -67.5_f32.to_radians(), -33.75_f32.to_radians()), v(45.0_f32.to_radians(), 67.5_f32.to_radians(), 33.75_f32.to_radians()), 1, [0.0625, 7.0 / 64.0]),
    BoneTemplate { unk_3c: 0.375, ..bone(BoneId::Torso, v(-0.1875, -13.0 / 128.0, 0.0), v(-0.1875, 0.0625, 0.0), ARM, 3.0, SHOULDER_MIN, SHOULDER_MAX, 1, [0.25, 0.0625]) },
    bone(BoneId::ShoulderLeft, v(0.0, -21.0 / 64.0, 0.0), v(0.0, -21.0 / 128.0, 0.0), ARM, 2.5, Vec3::ZERO, ELBOW_MAX, 1, [0.25, 0.0625]),
    bone(BoneId::ForearmLeft, v(0.0, -29.0 / 128.0, 0.0), v(0.0, -21.0 / 128.0, 0.0), HAND, 1.5, WRIST_MIN, WRIST_MAX, 1, [0.0625, 0.0625]),
    bone(BoneId::Torso, v(0.1875, -13.0 / 128.0, 0.0), v(0.1875, 0.0625, 0.0), ARM, 3.0, SHOULDER_MIN, SHOULDER_MAX, 1, [0.25, 0.0625]),
    bone(BoneId::ShoulderRight, v(-0.0, -21.0 / 64.0, 0.0), v(-0.0, -21.0 / 128.0, 0.0), ARM, 2.5, Vec3::ZERO, ELBOW_MAX, 1, [0.25, 0.0625]),
    bone(BoneId::ForearmRight, v(-0.0, -29.0 / 128.0, 0.0), v(-0.0, -21.0 / 128.0, 0.0), HAND, 1.5, WRIST_MIN, WRIST_MAX, 1, [0.0625, 0.0625]),
    bone(BoneId::Pelvis, v(-0.09375, -0.25, 0.0), v(-0.09375, 0.0, 0.0), LEG, 12.0, HIP_MIN, HIP_MAX, 1, [0.375, 0.09375]),
    bone(BoneId::ThighLeft, v(0.0, -0.5, 0.0), v(0.0, -0.25, 0.0), LEG, 7.0, KNEE_MIN, Vec3::ZERO, 1, [0.28125, 0.0625]),
    bone(BoneId::ShinLeft, v(0.0, -0.25, -0.0625), v(0.0, -0.25, 0.0), FOOT, 3.0, ANKLE_MIN, ANKLE_MAX, 2, [0.125, 0.0625]),
    bone(BoneId::Pelvis, v(0.09375, -0.25, 0.0), v(0.09375, 0.0, 0.0), LEG, 12.0, HIP_MIN, HIP_MAX, 1, [0.375, 0.09375]),
    bone(BoneId::ThighRight, v(-0.0, -0.5, 0.0), v(-0.0, -0.25, 0.0), LEG, 7.0, KNEE_MIN, Vec3::ZERO, 1, [0.28125, 0.0625]),
    bone(BoneId::ShinRight, v(-0.0, -0.25, -0.0625), v(-0.0, -0.25, 0.0), FOOT, 3.0, ANKLE_MIN, ANKLE_MAX, 2, [0.125, 0.0625]),
];
