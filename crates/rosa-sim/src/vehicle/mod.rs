//! Vehicles: the type table built from the body files and the vehicles in the world.

pub mod sbv;
pub mod physics;
pub mod types;

use glam::Vec3;
use rosa_physics::{RigidBodies, RotMatrix, Table, body::RigidBodyType, bond::{Bond, ItemAngular, ItemPoint}};

use types::VehicleType;

pub const MAX_VEHICLES: usize = 0x200;
/// Only the first 128 vehicles are ever sent to clients.
pub const NETWORKED_VEHICLES: usize = 0x80;
pub const MAX_SEATS: usize = 8;
const WHEEL_HALF_WIDTH: f32 = 0.125;
const WHEEL_HEALTH: i32 = 8;
const ENGINE_POWER_SCALE: f32 = 213.0;
const FULL_HEALTH: i32 = 100;
/// A train's bogies: 80000 each, set 6 along and 1 (the front 1.25) below the chassis.
const BOGIE_MASS: f32 = 80000.0;
const BOGIE_INERTIA: Vec3 = Vec3::new(0.9375, 1.5, 0.9375);
/// The control flags a vehicle starts with: the handbrake on.
pub const HANDBRAKE: i32 = 4;

/// A wheel of a vehicle (vehicle +0x3944, 0xac each), copied from its type's wheel and given its own body.
#[derive(Clone, Debug, Default)]
pub struct Wheel {
    pub body: usize,
    pub half_width: f32,
    pub popped: i32,
    pub drive: i32,
    pub health: i32,
    pub node_a: i32,
    pub node_b: i32,
    pub weight_a: f32,
    pub weight_b: f32,
    pub local_pos: Vec3,
    pub mass: f32,
    pub radius: f32,
    pub spin_response: f32,
    /// The wheel body's spin about its axle (+0x40) and the angle it has turned through (+0x44).
    pub spin: f32,
    pub angle: f32,
    /// How far the wheel is steered about the chassis' up axis (+0x48).
    pub steer: f32,
    /// The wheel body's position (+0x4c) and velocity (+0x64) after the last vehicle step, and the velocity before
    /// that (+0x58).
    pub world_pos: Vec3,
    pub prev_vel: Vec3,
    pub vel: Vec3,
    /// How far the wheel has travelled along the chassis' up axis (-1 to 1) for the clients (+0x70).
    pub visual_height: f32,
    pub vertical_offset: f32,
    /// The suspension: how hard the wheel is pulled to its mount (+0x78), how much of the relative velocity is
    /// damped (+0x7c) and the extra damping along the chassis' up axis (+0x80).
    pub spring: f32,
    pub damping: f32,
    pub travel_damping: f32,
    pub skid: f32,
}

/// A vehicle in the world (vehicles, 0x5168 each).
#[derive(Clone, Debug)]
pub struct Vehicle {
    pub kind: rosa_protocol::clientbound::game::VehicleKind,
    /// The type's +0x08 flag (controllableState); a vehicle in state 3 is not sent to clients.
    pub controllable_state: i32,
    pub health: i32,
    pub last_driver: i32,
    pub color: i32,
    pub despawn_time: u16,
    pub spawned_state: u16,
    pub locked: bool,
    /// The player who owns the vehicle (+0x24, -1 for none): hurting someone in your own vehicle earns no
    /// criminal rating.
    pub owner: i32,
    pub body: usize,
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub rot: RotMatrix,
    pub prev_rot: RotMatrix,
    /// The chassis body's velocity (+0x8c), the one before (+0x98) and its angular velocity (+0xa4).
    pub vel: Vec3,
    pub prev_vel: Vec3,
    pub ang_vel: Vec3,
    /// The render shape's bounds (+0xb0, +0xbc) and the level blocks they cover (+0xc8, +0xd4).
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    pub block_min: [i32; 3],
    pub block_max: [i32; 3],
    /// The driver's controls (+0x3600..+0x360c): the gear stick, steering and gas.
    pub gear_x: f32,
    pub steer_control: f32,
    pub gear_y: f32,
    pub gas_control: f32,
    /// The driver's control flags (+0x3620): 2 holds the clutch, 4 the handbrake; -1 is a car nobody controls.
    pub controls: i32,
    /// The human in each seat (+0x3628).
    pub occupants: [i32; MAX_SEATS],
    pub traffic_car: i32,
    /// Set when a player takes the driver's seat of a traffic car with free hands, for the traffic to hand it over
    /// (is_bot 2) on its next tick.
    pub traffic_taken: bool,
    /// The engine (+0x3898..+0x38b8): its speed, throttle and power, the gearbox speed and inertia, the engine's
    /// inertia and losses.
    pub engine_speed: f32,
    pub throttle: f32,
    pub engine_power: f32,
    pub gearbox_speed: f32,
    pub gearbox_inertia: f32,
    // TODO: name once apply_vehicle_wheel_forces is fully understood (+0x38ac, 64 every step)
    pub unk_38ac: f32,
    pub engine_inertia: f32,
    // TODO: name once apply_vehicle_wheel_forces is fully understood (+0x38b4 and +0x38b8, 199.99998 and 0.005)
    pub unk_38b4: f32,
    pub unk_38b8: f32,
    /// The gear (-1 reverse, 0 neutral, 1 to 4), the gear last step, its ratio and inverse, the clutch (1 engaged)
    /// and the brake (+0x38bc..+0x38d4).
    pub gear: i32,
    pub prev_gear: i32,
    pub gear_ratio: f32,
    pub inv_gear_ratio: f32,
    pub clutch: f32,
    pub brake: f32,
    /// The driven wheels (+0x38e0 count, +0x38e4).
    pub driven: Vec<usize>,
    pub engine_rpm: i32,
    /// Automatic gear changes (+0x3934) and the ticks until the next one (+0x3938).
    pub auto_shift: i32,
    pub shift_delay: i32,
    /// The steering angle the clients draw (+0x393c).
    pub steer: f32,
    pub wheels: Vec<Wheel>,
    /// Which of the type's windows are broken (windowStates).
    pub broken_windows: Vec<bool>,
    /// How hard the last crashes were, played back as crash sounds (+0x5140).
    pub crash: f32,
    pub track: f32,
    pub wheelbase: f32,
    pub seats: Vec<Vec3>,
    /// A train's two bogies (+0x4fbc, 12 apart): each one's body and the angular bond turning it with the train.
    pub bogies: Vec<(usize, usize)>,
    /// The track piece a train's bogies last touched (+0x4fb0) and which train spawn it came from (+0x4fb8).
    pub train_segment: i32,
    pub train_index: i32,
    /// Ticks a train has stood at its station (+0x4fac), its running direction (+0x4fb4, 1 reversed) and whether its
    /// horn is sounding (+0x2808).
    pub train_wait: i32,
    pub train_reverse: i32,
    pub horn: bool,
}

/// The chassis body's centre: the vehicle's position plus its centre of mass turned by `rot`.
fn body_centre(pos: Vec3, rot: &RotMatrix, com: Vec3) -> Vec3 {
    let [r0, r1, r2] = *rot;
    Vec3::new(
        ((com.x * r0.x + pos.x) + com.y * r1.x) + r2.x * com.z,
        ((r0.y * com.x + pos.y) + r1.y * com.y) + r2.y * com.z,
        ((com.z * r2.z + r1.z * com.y) + r0.z * com.x) + pos.z,
    )
}

/// spawn_vehicle: a vehicle of type `kind` at `pos` turned by `rot`, its chassis body at the type's centre of mass
/// and a body for each wheel.
#[allow(clippy::too_many_arguments)]
pub fn spawn_vehicle(vehicles: &mut Table<Vehicle>, bodies: &mut RigidBodies, types: &[VehicleType], kind: rosa_protocol::clientbound::game::VehicleKind, color: i32, pos: Vec3, rot: RotMatrix, vel: Option<Vec3>) -> Option<usize> {
    // TODO: the helicopter's rotor body (type 12: 160 at 1.5 up, coefs 4, 0.125, 4, a point bond at (0, 1.5, 0) and an
    // angular bond); the cage weights the record keeps; the per-connection update state
    let t = types.get(kind as usize)?;
    let centre = body_centre(pos, &rot, t.center_of_mass);
    let body = bodies.create(RigidBodyType::Vehicle, centre, rot, vel, t.inertia, t.mass)?;
    let [r0, r1, r2] = rot;
    let wheels = t
        .wheels
        .iter()
        .filter_map(|w| {
            let l = w.mass_offset;
            let world_pos = Vec3::new(
                ((l.y * r1.x + l.x * r0.x) + centre.x) + r2.x * l.z,
                ((r0.y * l.x + centre.y) + r1.y * l.y) + r2.y * l.z,
                ((r0.z * l.x + centre.z) + r1.z * l.y) + l.z * r2.z,
            );
            let r = w.radius;
            let side = (r * r * 3.0 + 0.0625) * (1.0 / 12.0);
            let inertia = Vec3::new(r * r * 0.5, side, side);
            let wheel_body = bodies.create(RigidBodyType::Wheel, world_pos, rot, vel, inertia, w.mass)?;
            Some(Wheel {
                body: wheel_body,
                half_width: WHEEL_HALF_WIDTH,
                popped: w.state,
                drive: w.drive,
                health: WHEEL_HEALTH,
                node_a: w.node_a,
                node_b: w.node_b,
                weight_a: w.weight_a,
                weight_b: w.weight_b,
                local_pos: l,
                mass: w.mass,
                radius: w.radius,
                spin_response: w.spin_response,
                world_pos,
                vertical_offset: w.vertical_offset,
                spring: w.spring,
                damping: w.damping,
                travel_damping: w.travel_damping,
                ..Default::default()
            })
        })
        .collect();
    let mut bogies = Vec::new();
    if kind == rosa_protocol::clientbound::game::VehicleKind::Train {
        let [_, r1, r2] = rot;
        let ends = [
            (Vec3::new((centre.x - r1.x) + r2.x * -6.0, r2.y * -6.0 + (centre.y - r1.y), -6.0 * r2.z + (centre.z - r1.z)), Vec3::new(0.0, -1.0, -6.0)),
            (Vec3::new((-1.25 * r1.x + centre.x) + r2.x * 6.0, r2.y * 6.0 + (r1.y * -1.25 + centre.y), 6.0 * r2.z + (r1.z * -1.25 + centre.z)), Vec3::new(0.0, -1.0, 6.0)),
        ];
        for (at, anchor) in ends {
            let bogie = bodies.create(RigidBodyType::Vehicle, at, rot, vel, BOGIE_INERTIA, BOGIE_MASS)?;
            bodies.create_bond(Bond::ItemPoint(ItemPoint::vehicle(body, bogie, anchor, Vec3::ZERO)));
            let turn = bodies.create_bond(Bond::ItemAngular(ItemAngular::vehicle(body, bogie)))?;
            bogies.push((bogie, turn));
        }
    }
    let driven = if kind == rosa_protocol::clientbound::game::VehicleKind::Hatchback { vec![0, 1] } else { vec![2, 3] };
    let vel = vel.unwrap_or(Vec3::ZERO);
    vehicles.insert(Vehicle {
        kind,
        controllable_state: t.controllable_state,
        health: FULL_HEALTH,
        last_driver: -1,
        color,
        despawn_time: 0xffff,
        spawned_state: 0,
        locked: false,
        owner: -1,
        body,
        pos,
        prev_pos: pos,
        rot,
        prev_rot: rot,
        vel,
        prev_vel: vel,
        ang_vel: Vec3::ZERO,
        bounds_min: Vec3::ZERO,
        bounds_max: Vec3::ZERO,
        block_min: [0; 3],
        block_max: [0; 3],
        gear_x: 0.0,
        steer_control: 0.0,
        gear_y: 0.0,
        gas_control: 0.0,
        controls: HANDBRAKE,
        occupants: [-1; MAX_SEATS],
        traffic_car: -1,
        traffic_taken: false,
        engine_speed: 0.0,
        throttle: 0.0,
        engine_power: ENGINE_POWER_SCALE * t.engine_power,
        gearbox_speed: 0.0,
        gearbox_inertia: 0.0,
        unk_38ac: 0.0,
        engine_inertia: 12.0,
        unk_38b4: f32::from_bits(0x4347ffff),
        unk_38b8: f32::from_bits(0x3ba3d70b),
        gear: 0,
        prev_gear: 0,
        gear_ratio: 0.0,
        inv_gear_ratio: 0.0,
        clutch: 0.0,
        brake: 0.0,
        driven,
        engine_rpm: 0,
        auto_shift: 0,
        shift_delay: 0,
        steer: 0.0,
        wheels,
        broken_windows: vec![false; t.windows.len()],
        crash: 0.0,
        track: t.track,
        wheelbase: t.wheelbase,
        seats: t.seats.clone(),
        bogies,
        train_segment: 0,
        train_index: 0,
        train_wait: 0,
        train_reverse: 0,
        horn: false,
    })
}
