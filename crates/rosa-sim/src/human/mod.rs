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
pub mod seated;
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
    /// How hard the bone holds its pose (bone +0xc8): the human's strength, nothing for the pelvis and legs of a
    /// seated human.
    pub strength: f32,
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
    /// The seat taken in `vehicle` (record 0x54).
    pub seat: usize,
    /// Ticks the human has been getting out of its seat (record 0x74, 0 when not), which slides it outwards.
    pub seat_exit: i32,
    /// The gear stick a driver moves (controls 0 and 2, record 0x150 and 0x158).
    pub gear_x_input: f32,
    pub gear_y_input: f32,
    pub last_vehicle_cooldown: i32,
    /// Whether the traffic car this human sits in names it as its driver (traffic car +0x04), set before each tick.
    pub traffic_driver: bool,
    /// An immortal human (record 0x68) loses no blood, cannot die of its wounds and is not hurt by falls or
    /// vehicles; it recovers quickly while knocked down.
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
    /// Where the player looks around without turning (controls 6 and 7, record 0x168 and 0x16c): the waist and
    /// chest turn part of the way, and a pointing arm follows them.
    pub free_look_yaw: f32,
    pub free_look_pitch: f32,
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
    pub eat_cooldown: i32,
    pub progress_bar: i32,
    /// Ticks an immortal human knocked below 50 health is kept from being shot (record 0x6c, 1800 when knocked down).
    pub down_timer: i32,
    /// How far the body still has to turn to face the view (record 0xe0).
    pub view_turn: f32,
    /// The body's heading (record 0x100), turned towards the view a step at a time.
    pub body_yaw: f32,
    /// The pitch counterpart of `yaw_offset` (record 0x124): the seated human's look pitch.
    pub pitch_offset: f32,
    pub unk_128: f32,
    pub unk_12c: f32,
    /// Where the player looks (controls 4 and 5, record 0x160 and 0x164): the view's yaw from the body and its pitch,
    /// or the look around a seat.
    pub look_yaw: f32,
    pub look_pitch: f32,
    /// The body heading the client last saw (control 8, record 0x170), which the look yaw is relative to.
    pub client_body_yaw: f32,
    /// The vehicles whose bounds overlap the human's (record +0x218 count, +0x21c ids), at most 8.
    pub nearby_vehicles: Vec<usize>,
    // TODO: name once its writers are found (record 0xb4, zero for every human seen; its length sets how long getting
    // out of a seat takes)
    pub unk_b4: Vec3,
    /// Whether the human is bleeding (record 0x6d80): a hit starts it, bandaging or death stops it.
    pub bleeding: bool,
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
