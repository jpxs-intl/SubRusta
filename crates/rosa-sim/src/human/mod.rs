use glam::Vec3;
use rosa_physics::RotMatrix;
use rosa_protocol::{CharacterCustomization, Team};

use crate::PlayerId;

pub mod arms;
pub mod bones;
pub mod create;
pub mod damage;
pub mod ik;
pub mod inventory;
pub mod locomotion;
pub mod physics;
pub mod simulation;

pub use bones::{BONE_COUNT, BoneId};
pub use locomotion::Locomotion;

pub const MAX_HUMANS: usize = 256;

#[derive(Clone, Debug)]
pub struct Bone {
    pub body: usize,
    pub pos: Vec3,
    pub pos2: Vec3,
    pub vel: Vec3,
    pub rot: RotMatrix,
    pub ang_vel: Vec3,
    pub margin: f32,
    pub half_extents: Vec3,
    pub mass: f32,
    pub inertia: Vec3,
    pub inv_inertia: Vec3,
    pub min_inertia: f32,
    // TODO: name once its readers are ported (identity at spawn)
    pub unk_rot: RotMatrix,
    pub angles: Vec3,
    pub networked_rot: RotMatrix,
    pub limit_angles: [f32; 3],
    pub capsule: [Vec3; 2],
    pub joint: Option<usize>,
    pub ground_contact: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Joint {
    pub bond: usize,
    pub parent: usize,
    pub child: usize,
}

#[derive(Clone, Debug, Default)]
pub struct AttachmentGroup {
    pub points: Vec<(usize, Vec3)>,
    pub centroid: Vec3,
    pub prev_centroid: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct AttachmentLink {
    pub a: usize,
    pub b: usize,
    pub shared_body: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct AttachmentChain {
    pub end: usize,
    pub middle: usize,
    pub other: usize,
    pub body_a: usize,
    pub body_b: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Attachments {
    pub groups: Vec<AttachmentGroup>,
    pub links: Vec<AttachmentLink>,
    pub chains: Vec<AttachmentChain>,
}

pub struct Human {
    pub player: Option<PlayerId>,
    /// The account of the player this human belongs to (record 0x34), kept after the player leaves.
    pub account: Option<u32>,
    /// The player's share count, copied every tick (record 0x38) so a returning player can buy them back.
    pub stocks: i32,
    pub team: Option<Team>,
    pub customization: CharacterCustomization,
    // left eye offset, left eye scale, right eye offset, right eye scale
    pub face_shape: [Vec3; 4],
    pub health: i32,
    pub blood_level: i32,
    pub old_health: i32,
    pub chest_hp: i32,
    pub head_hp: i32,
    pub left_arm_hp: i32,
    pub right_arm_hp: i32,
    pub left_leg_hp: i32,
    pub right_leg_hp: i32,
    pub stamina: i32,
    pub max_stamina: i32,
    pub vehicle: Option<usize>,
    pub last_vehicle_cooldown: i32,
    pub is_immortal: bool,
    pub is_on_ground: bool,
    pub spawn_protection: i32,
    pub input_flags: u32,
    pub last_input_flags: u32,
    pub movement_state: i32,
    pub getup_ticks: i32,
    pub movement_mode: i32,
    pub is_standing: bool,
    pub view_yaw: f32,
    pub view_pitch: f32,
    pub yaw_offset: f32,
    pub strafe_input: f32,
    pub walk_input: f32,
    pub stance: f32,
    pub lean_forward: f32,
    pub lean_side: f32,
    pub pain: i32,
    // TODO: name once their writers are ported (copied into the locomotion step state when a step starts)
    pub unk_190: f32,
    pub unk_194: f32,
    // TODO: name once their writers are ported (extra yaw and pitch added to the body's turn towards the view)
    pub unk_168: f32,
    pub unk_16c: f32,
    pub hand_sway: Vec3,
    pub hand_sway_vel: Vec3,
    pub inventory: [InventorySlot; INVENTORY_SLOTS],
    pub actions: [QueuedAction; 8],
    pub actions_queued: i32,
    pub actions_finished: i32,
    pub action_type: i32,
    pub action_progress: f32,
    pub action_duration: i32,
    pub action_hand: i32,
    pub action_slot: i32,
    pub throw_pitch: f32,
    pub despawn_ticks: i32,
    pub last_vehicle: i32,
    pub strength: f32,
    // TODO: name these once more of their readers and writers are ported
    pub unk_2c: i32,
    pub unk_40: i32,
    // TODO: name once its readers are ported (record 0x3c, starts at 105, a burger adds 8 up to 105)
    pub unk_3c: i32,
    /// The progress bar shown to the player (record 0x6e04), e.g. while bandaging.
    pub progress_bar: i32,
    pub unk_68: i32,
    pub unk_6c: i32,
    pub unk_e0: f32,
    pub unk_100: f32,
    pub unk_128: f32,
    pub unk_12c: f32,
    pub unk_160: f32,
    pub unk_164: f32,
    pub unk_170: f32,
    pub unk_218: i32,
    pub unk_6d80: i32,
    pub unk_6adc: Vec3,
    pub pos: Vec3,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub grid_min: [i32; 3],
    pub grid_max: [i32; 3],
    pub bones: Vec<Bone>,
    pub joints: Vec<Joint>,
    pub attachments: Attachments,
    pub locomotion: Locomotion,
}

pub const INVENTORY_SLOTS: usize = 7;

/// One inventory slot: the two hands (0 right, 1 left) and five pockets.
#[derive(Clone, Copy, Debug, Default)]
pub struct InventorySlot {
    pub count: i32,
    pub items: [i32; 8],
}

/// An inventory action waiting in the human's queue (human_action_simulation).
#[derive(Clone, Copy, Debug, Default)]
pub struct QueuedAction {
    pub kind: i32,
    pub progress: f32,
    pub slot: i32,
    pub arg: i32,
}
