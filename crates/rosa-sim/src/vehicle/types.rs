use glam::Vec3;

use rosa_map::file_types::tst::TstFile;
use rosa_protocol::clientbound::game::VehicleKind;

use super::sbv::VehicleBody;

pub const VEHICLE_TYPES: usize = 17;
const COPLANAR: f32 = 1.0 / 65536.0;
const NODE_SEARCH: f32 = 65536.0;
const WHEEL_SEARCH_SCALE: f32 = 4.0;
const LOWER_NODE_MASS: f32 = 4.0;
const RADIUS_MARGIN: f32 = 1.125;
const CAGE_PROBE: f32 = 0.25;
const SBV_CAGE: [i32; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

/// A point mass of the chassis (+0x44, 0x14 each): its rest position and share of the chassis mass.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChassisNode {
    // TODO: name once its readers are ported (+0x00, never set by the type builders)
    pub unk_00: i32,
    pub pos: Vec3,
    pub mass: f32,
}

/// A spring between two chassis nodes (+0x858, 0xc each).
#[derive(Clone, Copy, Debug)]
pub struct NodeEdge {
    // TODO: name once its readers are ported (always 0 from the type builders)
    pub kind: i32,
    pub a: i32,
    pub b: i32,
}

/// A deformable panel of a body built in code (+0x1458 count, +0x145c, 0x40 each): three or four corners, each a
/// chassis node or the point a weight of the way from it to a second node.
#[derive(Clone, Copy, Debug, Default)]
pub struct Panel {
    pub count: i32,
    pub nodes: [i32; 4],
    /// The second node of each corner (+0x14), -1 for a corner on its node.
    pub towards: [i32; 4],
    pub weights: [f32; 4],
    // TODO: name once the panel damage is ported (+0x34, +0x38)
    pub unk_34: f32,
    pub unk_38: f32,
    /// 1 for the windows (+0x3c), which are left out of the parts.
    pub window: i32,
}

/// A group of chassis nodes (+0xa86c, 0x34 each).
#[derive(Clone, Copy, Debug, Default)]
pub struct Part {
    pub count: i32,
    pub nodes: [i32; 4],
}

/// A wheel of the type (+0x1787c, 0x50 each), hung between two chassis nodes by the weights `weight_a` and `weight_b`.
#[derive(Clone, Copy, Debug, Default)]
pub struct WheelDef {
    pub state: i32,
    pub node_a: i32,
    pub node_b: i32,
    pub weight_a: f32,
    pub weight_b: f32,
    pub mass: f32,
    pub radius: f32,
    pub spin_response: f32,
    /// Between its two nodes, raised by `vertical_offset`, around the chassis centroid (+0x20).
    pub local_pos: Vec3,
    pub vertical_offset: f32,
    /// The suspension, copied into each wheel: how hard it is pulled to its mount (+0x30), how much of the
    /// relative velocity is damped (+0x34) and the extra damping along the chassis' up axis (+0x38).
    pub spring: f32,
    pub damping: f32,
    pub travel_damping: f32,
    // TODO: name once its readers are ported (+0x3c)
    pub unk_3c: u32,
    /// The wheel's drive configuration, the second argument of vehicletype_attach_wheels_sized (+0x40).
    pub drive: i32,
    /// Between its two nodes, around the centre of mass (+0x44).
    pub mass_offset: Vec3,
}

/// A vehicle type (vehicleTypeDefinitions, 0x185c0 each).
#[derive(Clone, Debug, Default)]
pub struct VehicleType {
    /// 1 for the types built from a body file (+0x00): their cage is the node list at +0x548 and traces hit their
    /// body slot's meshes rather than the panels.
    pub body_file: i32,
    /// The state a vehicle of this type spawns in (+0x08, controllableState).
    pub controllable_state: i32,
    pub name: String,
    pub price: i32,
    /// The chassis body's mass (+0x38).
    pub mass: f32,
    /// How far the chassis reaches from the cage centre, with a margin (+0x3c).
    pub radius: f32,
    pub nodes: Vec<ChassisNode>,
    /// The chassis nodes that frame the car (+0x544 count, +0x548).
    pub cage: Vec<i32>,
    /// The cage weights of a point a quarter radius right of (+0x648) and ahead of (+0x748) the cage centre, which the
    /// simulation uses to find the deformed chassis' axes.
    pub cage_right: Vec<f32>,
    pub cage_forward: Vec<f32>,
    pub centroid: Vec3,
    pub edges: Vec<NodeEdge>,
    /// The chassis nodes around the cage mean (+0x7864 count, +0x7868).
    pub centred_nodes: Vec<Vec3>,
    pub panels: Vec<Panel>,
    /// The panels that are not windows (+0xa868 count).
    pub parts: Vec<Part>,
    /// The engine's power as a multiple of 213 (+0x17874).
    pub engine_power: f32,
    pub wheels: Vec<WheelDef>,
    /// The distance between the left and right wheels (+0x1827c) and between the front and rear wheels (+0x18280).
    pub track: f32,
    pub wheelbase: f32,
    /// Where each seat is, around the chassis (+0x18318 count, +0x1831c).
    pub seats: Vec<Vec3>,
    pub center_of_mass: Vec3,
    /// The node mean around the centre of mass (+0x18534).
    pub centroid_offset: Vec3,
    pub inertia: Vec3,
    /// The body file's render vertices (sbv +0xd020), which the vehicle's bounds cover.
    pub render_verts: Vec<Vec3>,
    /// The collision mesh over the chassis points (sbv +0x2bb70), which the chassis sweeps against the level.
    pub mesh: Option<crate::sim::hull::ConvexHull>,
    /// The body file's faces over the chassis points (sbv +0x7018 count, +0x7020), three or four corners each, which
    /// humans collide with.
    pub chassis_faces: Vec<Vec<usize>>,
    /// The render mesh's faces over `render_verts` (the file's subshapes, sbv +0x10020 count, +0x10028), which
    /// traces hit.
    pub render_faces: Vec<Vec<usize>>,
    /// The windows' corners (the file's parts, sbv +0x2a024 count, +0x2a02c), traced from either side until broken.
    pub windows: Vec<[Vec3; 4]>,
}

/// A vehicle type built from a body file and the settings vehicle setup gives it.
struct SbvSpec {
    kind: VehicleKind,
    file: &'static str,
    name: &'static str,
    price: i32,
    mass: f32,
    engine_power: f32,
    wheel_radius: f32,
    wheel_mass: f32,
    drive: i32,
    seats: &'static [[f32; 3]],
}

const SBV_TYPES: [SbvSpec; 9] = [
    SbvSpec { kind: VehicleKind::TownCar, file: "park5", name: "Town Car", price: 1000, mass: 1000.0, engine_power: 0.875, wheel_radius: 0.375, wheel_mass: 12.0, drive: 0, seats: &[[-0.375, -0.5625, -0.375], [0.375, -0.5625, -0.375], [-0.375, -0.5625, 0.75], [0.375, -0.5625, 0.75]] },
    SbvSpec { kind: VehicleKind::Test, file: "turbo5", name: "Test", price: 20000, mass: 800.0, engine_power: 1.0, wheel_radius: 0.3125, wheel_mass: 12.0, drive: 2, seats: &[[-0.375, -0.5625, -0.125], [0.375, -0.5625, 0.125]] },
    SbvSpec { kind: VehicleKind::Hatchback, file: "golf5", name: "Hatchback", price: 2000, mass: 800.0, engine_power: 0.875, wheel_radius: 0.3125, wheel_mass: 10.0, drive: 2, seats: &[[-0.375, -0.5625, -0.125], [0.375, -0.5625, -0.125], [-0.375, -0.5625, 1.0], [0.375, -0.5625, 1.0]] },
    SbvSpec { kind: VehicleKind::Turbo, file: "turbo5", name: "Turbo", price: 20000, mass: 800.0, engine_power: 1.125, wheel_radius: 0.3125, wheel_mass: 12.0, drive: 2, seats: &[[-0.375, -0.625, -0.125], [0.375, -0.625, 0.125]] },
    SbvSpec { kind: VehicleKind::TurboS, file: "turbo5", name: "Turbo S", price: 40000, mass: 750.0, engine_power: 1.5, wheel_radius: 0.3125, wheel_mass: 12.0, drive: 2, seats: &[[-0.375, -0.625, -0.125], [0.375, -0.625, 0.125]] },
    SbvSpec { kind: VehicleKind::Van, file: "van4", name: "Van", price: 3000, mass: 2100.0, engine_power: 1.0, wheel_radius: 0.375, wheel_mass: 20.0, drive: 0, seats: &VAN_SEATS },
    SbvSpec { kind: VehicleKind::Van2, file: "van4", name: "Van2", price: 3000, mass: 2100.0, engine_power: 1.0, wheel_radius: 0.375, wheel_mass: 20.0, drive: 0, seats: &VAN_SEATS },
    SbvSpec { kind: VehicleKind::Minivan, file: "minivan2", name: "Minivan", price: 100, mass: 1200.0, engine_power: 1.0, wheel_radius: 0.375, wheel_mass: 12.0, drive: 0, seats: &[[-0.375, -0.5, -0.4375], [0.375, -0.5, -0.4375], [-0.375, -0.5, 0.4375], [0.375, -0.5, 0.4375], [-0.375, -0.5, 1.3125], [0.375, -0.5, 1.3125]] },
    SbvSpec { kind: VehicleKind::Beamer, file: "beamer2", name: "Beamer", price: 10000, mass: 900.0, engine_power: 1.25, wheel_radius: 0.3125, wheel_mass: 12.0, drive: 2, seats: &[[-0.375, -0.5, -0.25], [0.375, -0.5, -0.25], [-0.375, -0.5, 0.625], [0.375, -0.5, 0.625]] },
];

/// How far load_tst raises and moves back the shapes it reads.
const TST_SHIFT: f32 = 0.625;

const VAN_SEATS:[[f32; 3]; 6] = [[-0.375, -0.5, -0.875], [0.375, -0.5, -0.875], [-0.375, -0.5, 0.5], [0.375, -0.5, 0.5], [-0.375, -0.5, 1.875], [0.375, -0.5, 1.875]];

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn length(d: Vec3) -> f32 {
    ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt()
}

fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new((b.x - a.x) * t + a.x, (b.y - a.y) * t + a.y, t * (b.z - a.z) + a.z)
}

fn mean<'a>(points: impl Iterator<Item = &'a Vec3>) -> (Vec3, f32) {
    let (sum, n) = points.fold((Vec3::ZERO, 0usize), |(s, n), p| (Vec3::new(s.x + p.x, s.y + p.y, s.z + p.z), n + 1));
    (sum, 1.0 / n as f32)
}

/// compute_mean_value_coordinates: weights over `verts` that reproduce `q`, each vertex weighted by the query's
/// barycentric coordinate for it, averaged over every tetrahedron the cage's vertices span.
pub fn mean_value_coordinates(verts: &[Vec3], q: Vec3) -> Vec<f32> {
    let n = verts.len();
    let cross = |e1: Vec3, e2: Vec3| Vec3::new(e1.y * e2.z - e1.z * e2.y, e1.z * e2.x - e2.z * e1.x, e1.x * e2.y - e2.x * e1.y);
    let unit = |c: Vec3| {
        let len = ((c.y * c.y + c.x * c.x) + c.z * c.z).sqrt();
        if len == 0.0 {
            return Vec3::ZERO;
        }
        let inv = 1.0 / len;
        Vec3::new(c.x * inv, c.y * inv, c.z * inv)
    };
    let mut count = 0;
    for i in 0..n.saturating_sub(1) {
        for j in i + 1..n {
            let a = verts[j];
            for k in j + 1..n {
                for l in k + 1..n {
                    let nrm = unit(cross(sub(verts[l], a), sub(verts[k], a)));
                    let d = sub(verts[i], a);
                    let dot = (nrm.y * d.y + nrm.x * d.x) + nrm.z * d.z;
                    let dist = if 0.0 > dot { ((-nrm.y) * d.y - d.x * nrm.x) - nrm.z * d.z } else { dot };
                    if dist.abs() >= COPLANAR {
                        count += 1;
                    }
                }
            }
        }
    }
    let weights: Vec<f32> = (0..n)
        .map(|p| {
            let mut w = 0.0f32;
            for a in 0..n.saturating_sub(1) {
                let o = verts[a];
                for b in a + 1..n.saturating_sub(1) {
                    if b == p || a == p {
                        continue;
                    }
                    for c in b + 1..n {
                        if c == p {
                            continue;
                        }
                        let e1 = sub(verts[b], o);
                        let e2 = sub(verts[c], o);
                        let c3 = Vec3::new(e2.y * e1.z - e2.z * e1.y, e2.z * e1.x - e1.z * e2.x, e2.x * e1.y - e1.x * e2.y);
                        let len = ((c3.y * c3.y + c3.x * c3.x) + c3.z * c3.z).sqrt();
                        let mut nrm = if len == 0.0 {
                            Vec3::ZERO
                        } else {
                            let inv = 1.0 / len;
                            Vec3::new(c3.x * inv, c3.y * inv, inv * c3.z)
                        };
                        let d = sub(verts[p], o);
                        let mut dot = (nrm.y * d.y + nrm.x * d.x) + nrm.z * d.z;
                        if 0.0 > dot {
                            nrm = Vec3::new(-nrm.x, -nrm.y, -nrm.z);
                            dot = d.z * nrm.z + (d.x * nrm.x + d.y * nrm.y);
                        }
                        if dot.abs() < COPLANAR {
                            continue;
                        }
                        let num = nrm.z * (q.z - o.z) + (nrm.y * (q.y - o.y) + nrm.x * (q.x - o.x));
                        w += num / dot;
                    }
                }
            }
            w
        })
        .collect();
    weights.into_iter().map(|w| w / count as f32).collect()
}

impl VehicleType {
    /// vehicletype_add_node_edge: a spring between `a` and `b`, once per pair.
    fn add_node_edge(&mut self, kind: i32, a: i32, b: i32) {
        if self.edges.iter().any(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a)) {
            return;
        }
        self.edges.push(NodeEdge { kind, a, b });
    }

    /// vehicle_type_add_wheel_definition: a wheel hung between nodes `a` and `b`.
    #[allow(clippy::too_many_arguments)]
    fn add_wheel(&mut self, state: i32, a: i32, b: i32, weights: [f32; 2], radius: f32, mass: f32, spin_response: f32, vertical_offset: f32, tuning: [f32; 3]) {
        let (pa, pb) = (self.nodes[a as usize].pos, self.nodes[b as usize].pos);
        let mut local_pos = lerp(pa, pb, weights[1]);
        local_pos.y += vertical_offset;
        self.wheels.push(WheelDef {
            state,
            node_a: a,
            node_b: b,
            weight_a: weights[0],
            weight_b: weights[1],
            mass,
            radius,
            spin_response,
            local_pos,
            vertical_offset,
            spring: tuning[0],
            damping: tuning[1],
            travel_damping: tuning[2],
            ..Default::default()
        });
    }

    /// A wheel at each of the body file's mounts, hung between its nearest node and the nearest node beyond it, then
    /// the track and wheelbase between the mounts.
    fn hang_wheels(&mut self, body: &VehicleBody, kind: VehicleKind, drive: i32, radius: f32, mass: f32, mounts: usize) {
        let (vertical_offset, travel_damping) = match kind {
            VehicleKind::TownCar => (-0.0625, 0.5),
            VehicleKind::Turbo | VehicleKind::TurboS => (0.0, 0.75),
            _ => (-0.03125, 0.625),
        };
        let spin_response = 1.0 / (0.625 * (radius * radius));
        for w in body.file.wheels.iter().take(mounts) {
            let p = w.pos.0;
            let near = |pick: &dyn Fn(usize, Vec3) -> Option<f32>| {
                body.file.nodes.iter().enumerate().fold((0usize, NODE_SEARCH), |(best_i, best), (i, n)| match pick(i, n.pos.0) {
                    Some(d) => (if best > d { i } else { best_i }, if d < best { d } else { best }),
                    None => (best_i, best),
                })
            };
            let (a, _) = near(&|_, n| {
                let (dx, dy, dz) = (p.x - n.x, (p.y - n.y) * WHEEL_SEARCH_SCALE, p.z - n.z);
                Some((dz * dz + (dx * dx + dy * dy)).sqrt())
            });
            let na = body.file.nodes[a].pos.0;
            let (b, _) = near(&|i, n| {
                if i == a {
                    return None;
                }
                let (dx, dy, dz) = ((p.x - n.x) * WHEEL_SEARCH_SCALE, (p.y - n.y) * WHEEL_SEARCH_SCALE, p.z - n.z);
                let d = (dz * dz + (dx * dx + dy * dy)).sqrt();
                let (wa, an) = (sub(p, na), sub(n, na));
                let dot = (p.z - na.z) * an.z + ((p.y - na.y) * an.y + wa.x * an.x);
                (dot >= 0.0).then_some(d)
            });
            let nb = body.file.nodes[b].pos.0;
            let to_b = length(sub(nb, p));
            let to_a = length(sub(na, p));
            let sum = to_a + to_b;
            self.add_wheel(0, a as i32, b as i32, [to_b / sum, to_a / sum], radius, mass, spin_response, vertical_offset, [0.25, 0.5, travel_damping]);
            if let Some(last) = self.wheels.last_mut() {
                last.drive = drive;
            }
        }
        let wp = |k: usize| body.file.wheels.get(k).map_or(Vec3::ZERO, |w| w.pos.0);
        self.track = (wp(0).x - wp(1).x).abs();
        self.wheelbase = (wp(0).z - wp(2).z).abs();
    }

    /// vehicletype_attach_wheels_sized: the body file's chassis nodes (heavier below the floor) and springs, the
    /// first eight nodes as the cage, a wheel at each mount hung between its nearest node and the nearest node beyond
    /// it, then the track, wheelbase, centre of mass (nodes below the floor at mass 4) and inertia.
    fn attach_wheels(&mut self, body: &VehicleBody, kind: VehicleKind, drive: i32, radius: f32, mass: f32, mounts: usize) {
        self.nodes.extend(body.file.nodes.iter().map(|n| ChassisNode { unk_00: 0, pos: n.pos.0, mass: if n.pos.0.y < 0.0 { 2.0 } else { 1.0 } }));
        for e in &body.file.edges {
            self.add_node_edge(0, e.a, e.b);
        }
        self.cage = SBV_CAGE.to_vec();
        self.hang_wheels(body, kind, drive, radius, mass, mounts);

        let mut com = Vec3::ZERO;
        let mut total = 0.0f32;
        for n in self.nodes.iter_mut() {
            if 0.0 > n.pos.y {
                n.mass = LOWER_NODE_MASS;
            }
            let m = n.mass;
            com = Vec3::new(n.pos.x * m + com.x, n.pos.y * m + com.y, m * n.pos.z + com.z);
            total += m;
        }
        let inv = 1.0 / total;
        com = Vec3::new(com.x * inv, com.y * inv, com.z * inv);
        self.center_of_mass = com;
        let (inertia, total) = self.nodes.iter().fold((Vec3::ZERO, 0.0f32), |(acc, total), n| {
            let (dy, dx, dz) = ((n.pos.y - com.y) * (n.pos.y - com.y), (n.pos.x - com.x) * (n.pos.x - com.x), (n.pos.z - com.z) * (n.pos.z - com.z));
            let m = n.mass;
            (Vec3::new((dy + dz) * m + acc.x, (dz + dx) * m + acc.y, (dy + dx) * m + acc.z), total + m)
        });
        let inv = 1.0 / total;
        self.inertia = Vec3::new(inertia.x * inv, inertia.y * inv, inv * inertia.z);
        let positions: Vec<Vec3> = self.nodes.iter().map(|n| n.pos).collect();
        let (sum, inv) = mean(positions.iter());
        self.centroid_offset = Vec3::new(sum.x * inv - com.x, sum.y * inv - com.y, sum.z * inv - com.z);
        for w in self.wheels.iter_mut() {
            let p = lerp(positions[w.node_a as usize], positions[w.node_b as usize], w.weight_b);
            w.mass_offset = sub(p, com);
        }
    }

    /// vehicletype_precompute_physics: the node masses as shares, the centroid (wheels around it), the radius around
    /// the cage centre, the cage weights of its right and forward probes and the nodes around the cage mean.
    fn precompute_physics(&mut self, kind: VehicleKind) {
        let total = self.nodes.iter().fold(0.0f32, |t, n| t + n.mass);
        for n in self.nodes.iter_mut() {
            n.mass /= total;
        }
        let positions: Vec<Vec3> = self.nodes.iter().map(|n| n.pos).collect();
        let (sum, inv) = mean(positions.iter());
        let centroid = Vec3::new(sum.x * inv, sum.y * inv, sum.z * inv);
        self.centroid = centroid;
        for w in self.wheels.iter_mut() {
            w.local_pos = sub(w.local_pos, centroid);
        }
        let c = self.cage.len();
        let (sum, inv) = mean(positions[..c].iter());
        let centre = Vec3::new(sum.x * inv, sum.y * inv, inv * sum.z);
        let reach = positions.iter().fold(0.0f32, |r, &p| {
            let d = sub(centre, p);
            let d = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();
            if d > r { d } else { r }
        });
        self.radius = reach * RADIUS_MARGIN;
        let probe = self.radius * CAGE_PROBE;
        let (cage, centre) = if self.body_file != 0 {
            let cage: Vec<Vec3> = self.cage.iter().map(|&i| positions[i as usize]).collect();
            let (sum, inv) = mean(cage.iter());
            (cage, Vec3::new(sum.x * inv, sum.y * inv, sum.z * inv))
        } else {
            (positions[..c].to_vec(), centre)
        };
        self.cage_forward = mean_value_coordinates(&cage, Vec3::new(0.0 * probe + centre.x, 0.0 * probe + centre.y, probe * 1.0 + centre.z));
        let probe = CAGE_PROBE * self.radius;
        self.cage_right = mean_value_coordinates(&cage, Vec3::new(centre.x + 1.0 * probe, centre.y + 0.0 * probe, centre.z + probe * 0.0));
        let (sum, inv) = mean(positions[..c].iter());
        let mx = sum.x * inv;
        self.centred_nodes = positions.iter().map(|p| Vec3::new(p.x - mx, p.y - sum.y * inv, p.z - sum.z * inv)).collect();
        self.parts = self.panels.iter().filter(|p| p.window != 1).map(|p| Part { count: p.count, nodes: p.nodes }).collect();
        if kind == VehicleKind::TownCar {
            self.parts.push(Part { count: 4, nodes: [10, 11, 13, 12] });
        }
    }

    /// The body slot's collision mesh, chassis and render faces and windows.
    fn body_meshes(&mut self, body: &VehicleBody) {
        self.render_verts = body.file.vertices.iter().map(|v| v.0).collect();
        self.mesh = Some(body.mesh.clone());
        self.chassis_faces = body.file.faces.iter().map(|f| f.indices.iter().map(|&i| i as usize).collect()).collect();
        self.render_faces = body.file.subshapes.iter().map(|s| s.nodes.iter().map(|n| n.id as usize).collect()).collect();
        self.windows = body.file.parts.iter().map(|p| std::array::from_fn(|k| p.points.get(k).map_or(Vec3::ZERO, |v| v.0))).collect();
    }

    /// load_tst: a test shape file's mesh replaces the render vertices and faces and its parts the windows (raised and
    /// moved back by 0.625 but for Van2), and Van2's second mesh replaces the chassis nodes and faces. A block the
    /// file does not hold leaves the list empty.
    /// Van2 is built from van.tst, which leaves its body slot (and so its collision mesh) empty.
    fn load_tst(&mut self, path: &std::path::Path, kind: VehicleKind) {
        let tst = TstFile::load(path, kind == VehicleKind::Van2).unwrap_or_default();
        let shift = |p: Vec3| if kind != VehicleKind::Van2 { Vec3::new(p.x, p.y + TST_SHIFT, p.z + TST_SHIFT) } else { p };
        let mesh = tst.mesh.as_ref();
        self.render_verts = mesh.map_or(Vec::new(), |m| m.vertices.iter().map(|v| shift(v.pos.0)).collect());
        self.render_faces = mesh.map_or(Vec::new(), |m| m.faces.iter().map(|f| f.corners.iter().map(|c| c.vertex as usize).collect()).collect());
        self.windows = tst.parts.as_ref().map_or(Vec::new(), |p| {
            p.parts
                .iter()
                .map(|part| {
                    let point = |k: usize| part.points.get(k).map_or(Vec3::ZERO, |v| v.0);
                    let last = if part.points.len() > 3 { point(3) } else { point(2) };
                    [point(0), point(1), point(2), last].map(shift)
                })
                .collect()
        });
        if kind == VehicleKind::Van2 {
            let chassis = tst.chassis.as_ref();
            self.chassis_faces = chassis.map_or(Vec::new(), |m| m.faces.iter().map(|f| f.corners.iter().map(|c| c.vertex as usize).collect()).collect());
            // TODO: the chassis mesh's vertices become the body slot's nodes (+0x4014), which the collision hull
            // sweep reads; with none the hull is left out
            if chassis.is_none_or(|m| m.vertices.is_empty()) {
                self.mesh = None;
            }
        }
    }

    /// Helicopter (type 12, load_helicopter): the park5 body without its wheel mounts, reshaped by helitest.tst.
    fn helicopter(data: &std::path::Path) -> Option<Self> {
        let body = VehicleBody::load(&data.join("park5.sbv"))?;
        let mut t = VehicleType { body_file: 1, controllable_state: 2, name: "Helicopter".to_string(), price: 1000000, mass: 1000.0, ..Default::default() };
        t.body_meshes(&body);
        t.load_tst(&data.join("helitest.tst"), VehicleKind::Helicopter);
        t.attach_wheels(&body, VehicleKind::Helicopter, 0, 0.375, 12.0, 0);
        t.seats = vec![Vec3::new(-0.375, -0.5625, -0.875), Vec3::new(0.375, -0.5625, -0.875), Vec3::new(-0.375, -0.5625, 0.75), Vec3::new(0.375, -0.5625, 0.75)];
        t.precompute_physics(VehicleKind::Helicopter);
        Some(t)
    }

    fn from_sbv(spec: &SbvSpec, data: &std::path::Path) -> Option<Self> {
        let body = VehicleBody::load(&data.join(format!("{}.sbv", spec.file)))?;
        let mut t = VehicleType {
            body_file: 1,
            controllable_state: 1,
            name: spec.name.to_string(),
            price: spec.price,
            mass: spec.mass,
            engine_power: spec.engine_power,
            ..Default::default()
        };
        t.attach_wheels(&body, spec.kind, spec.drive, spec.wheel_radius, spec.wheel_mass, body.file.wheels.len());
        t.seats = spec.seats.iter().map(|s| Vec3::from_array(*s)).collect();
        t.body_meshes(&body);
        if spec.kind == VehicleKind::Van2 {
            t.load_tst(&data.join("van.tst"), VehicleKind::Van2);
        }
        t.precompute_physics(spec.kind);
        Some(t)
    }
}

/// A panel on four chassis nodes.
fn panel(nodes: [i32; 4]) -> Panel {
    Panel { count: 4, nodes, towards: [-1; 4], unk_34: 1.0, unk_38: 1.0, ..Default::default() }
}

/// A window panel, some of its corners part way to a second node.
fn window(nodes: [i32; 4], towards: [i32; 4], weights: [f32; 4], unk_38: f32) -> Panel {
    Panel { count: 4, nodes, towards, weights, unk_34: 1.0, unk_38, window: 1 }
}

impl VehicleType {
    /// A spring between every two of `nodes`.
    fn add_node_edges(&mut self, nodes: &[i32]) {
        for (i, &a) in nodes.iter().enumerate() {
            for &b in &nodes[i + 1..] {
                self.add_node_edge(0, a, b);
            }
        }
    }

    /// The saloon body load_towncar builds for Town Car 2, Metro and Limo: a box of 16 nodes in four slices along
    /// `z` (the first four heavier), a roof of four nodes over the middle slice, maybe a node ahead at `nose`, and the
    /// springs and panels of the three sections.
    fn saloon(&mut self, z: [f32; 4], roof_y: f32, roof_z: [f32; 2], nose: Option<f32>) {
        for i in 0..16 {
            let x = if i & 1 != 0 { 1.0 } else { -1.0 };
            let (y, mass) = match i {
                0..=3 => (-0.4375, 2.0),
                4..=7 => (-0.4375, 1.0),
                _ => (0.4375, 1.0),
            };
            self.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(x, y, z[(i >> 1) & 3]), mass });
        }
        for i in 0..4 {
            let x = if i & 1 != 0 { 0.875 } else { -0.875 };
            self.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(x, roof_y, if i <= 1 { roof_z[0] } else { roof_z[1] }), mass: 0.5 });
        }
        self.cage = vec![0; 16];
        if let Some(nose) = nose {
            self.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(0.0, 0.0, nose), mass: 1.0 });
        }
        let mut f = 1;
        for row in 0..3 {
            let e = 2 * row;
            let (g, h, i, j, k, l) = (f + 2, f + 1, f + 7, f + 8, f + 10, f + 9);
            let ring = [e, f, g, h, i, j, k, l];
            self.add_node_edges(&ring);
            if row == 0 && nose.is_some() {
                for &n in &ring {
                    self.add_node_edge(0, n, 20);
                }
            }
            self.panels.push(panel([e, h, g, f]));
            self.panels.push(panel([i, l, h, e]));
            if row == 0 {
                self.panels.push(panel([j, i, 0, f]));
            }
            self.panels.push(panel([k, j, f, g]));
            if row == 2 {
                self.panels.push(panel([l, k, g, h]));
            }
            if row != 1 {
                self.panels.push(panel([i, j, k, l]));
            }
            f = g;
        }
        self.add_node_edges(&[10, 11, 13, 12, 16, 17, 19, 18]);
    }

    /// The four wheels of a saloon, hung along its lower sides.
    fn saloon_wheels(&mut self) {
        for (a, b, weights) in [(0, 2, [0.25, 0.75]), (1, 3, [0.25, 0.75]), (2, 4, [0.125, 0.875]), (3, 5, [0.125, 0.875])] {
            self.add_wheel(0, a, b, weights, 0.375, 12.0, 128.0 / 9.0, -0.125, [0.25, 0.5, 0.25]);
        }
    }

    /// Town Car 2 (type 1).
    fn town_car_2() -> Self {
        let mut t = VehicleType { controllable_state: 1, name: "Town Car 2".to_string(), price: 1000, mass: 1000.0, engine_power: 1.0, ..Default::default() };
        let z = [-3.0625, -1.3125, 1.75, 2.625];
        t.saloon(z, 1.09375, [-0.4375, 1.3125], Some(-2.1875));
        t.panels.extend([
            window([16, 18, 12, 10], [-1, 16, 10, -1], [0.0, 0.5, 0.4375, 0.0], 0.0),
            window([16, 18, 12, 10], [18, -1, -1, 12], [0.5, 0.0, 0.0, 0.5625], 0.0),
            window([17, 16, 10, 11], [-1; 4], [0.0; 4], 6.0),
            window([19, 17, 11, 13], [17, -1, -1, 11], [0.5, 0.0, 0.0, 0.4375], 0.0),
            window([19, 17, 11, 13], [-1, 19, 13, -1], [0.0, 0.5, 0.5625, 0.0], 0.0),
            window([18, 19, 13, 12], [-1; 4], [0.0; 4], 0.0),
            panel([16, 17, 19, 18]),
        ]);
        t.saloon_wheels();
        t.seats = vec![Vec3::new(-0.375, -0.375, -0.25), Vec3::new(0.375, -0.375, -0.25), Vec3::new(-0.375, -0.375, 1.0), Vec3::new(0.375, -0.375, 1.0)];
        t.track = 1.0;
        t.wheelbase = ((z[2] - z[1]) * 0.875 + z[1]) - ((z[0] - z[1]) * 0.25 + z[1]);
        t.precompute_physics(VehicleKind::TownCar2);
        t
    }

    /// Metro (type 2).
    fn metro() -> Self {
        let mut t = VehicleType { controllable_state: 1, name: "Metro".to_string(), price: 100, mass: 600.0, engine_power: 0.4375, ..Default::default() };
        let z = [-2.40625, -1.3125, 1.3125, 2.1875];
        t.saloon(z, 1.09375, [-0.4375, 0.875], None);
        t.panels.extend([
            window([16, 18, 12, 10], [-1; 4], [0.0; 4], 0.0),
            window([17, 16, 10, 11], [-1; 4], [0.0; 4], 4.0),
            window([19, 17, 11, 13], [-1; 4], [0.0; 4], 0.0),
            window([18, 19, 13, 12], [-1; 4], [0.0; 4], 0.0),
            Panel { unk_38: 0.0, ..panel([16, 17, 19, 18]) },
        ]);
        t.saloon_wheels();
        t.seats = vec![Vec3::new(-0.375, -0.375, 0.0), Vec3::new(0.375, -0.375, 0.0)];
        t.track = 1.0;
        t.wheelbase = ((z[2] - z[1]) * 0.875 + z[1]) - ((z[0] - z[1]) * 0.25 + z[1]);
        t.precompute_physics(VehicleKind::Metro);
        t
    }

    /// Limo (type 3).
    fn limo() -> Self {
        let mut t = VehicleType { controllable_state: 1, name: "Limo".to_string(), price: 100, mass: 1500.0, engine_power: 1.0, ..Default::default() };
        let z = [-3.9375, -2.1875, 2.625, 3.5];
        t.saloon(z, 1.09375, [-1.3125, 2.1875], None);
        t.panels.extend([
            window([16, 18, 12, 10], [-1, 16, 10, -1], [0.0, 0.75, 0.625, 0.0], 0.0),
            window([16, 18, 12, 10], [18, 16, 10, 12], [0.25, 0.25, 0.25, 0.375], 6.0),
            window([16, 18, 12, 10], [18, -1, -1, 12], [0.75, 0.0, 0.0, 0.75], 0.0),
            window([17, 16, 10, 11], [-1; 4], [0.0; 4], 6.0),
            window([19, 17, 11, 13], [17, -1, -1, 11], [0.75, 0.0, 0.0, 0.625], 0.0),
            window([19, 17, 11, 13], [17, 19, 13, 11], [0.25, 0.25, 0.375, 0.25], 6.0),
            window([19, 17, 11, 13], [-1, 19, 13, -1], [0.0, 0.75, 0.75, 0.0], 0.0),
            window([18, 19, 13, 12], [-1; 4], [0.0; 4], 0.0),
            panel([16, 17, 19, 18]),
        ]);
        t.saloon_wheels();
        t.seats = vec![Vec3::new(-0.375, -0.375, -1.25), Vec3::new(0.375, -0.375, -1.25), Vec3::new(-0.375, -0.375, 2.0), Vec3::new(0.375, -0.375, 2.0)];
        t.track = 1.0;
        t.wheelbase = ((z[2] - z[1]) * 0.875 + z[1]) - ((z[0] - z[1]) * 0.25 + z[1]);
        t.precompute_physics(VehicleKind::Limo);
        t
    }

    /// Truck (type 10): a chassis of 12 nodes over three slices, a cab of 8 above its front two slices and a hitch
    /// node behind, on six wheels.
    fn truck() -> Self {
        let mut t = VehicleType { controllable_state: 1, name: "Truck".to_string(), price: 1000, mass: 9000.0, engine_power: 1.0, ..Default::default() };
        let z = [-2.0, 0.0, 2.5];
        for i in 0..12 {
            let x = if i & 1 != 0 { 1.125 } else { -1.125 };
            let (y, mass) = match i {
                0..=3 => (-0.5, 4.0),
                4..=5 => (-0.5, 2.0),
                _ => (-0.125, 1.0),
            };
            t.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(x, y, z[(i >> 1) % 3]), mass });
        }
        for i in 0..8 {
            let x = if i & 1 != 0 { 1.125 } else { -1.125 };
            let y = if i > 3 { 1.875 } else { 1.25 };
            let zc = if (i & 3) <= 1 { z[0] } else { z[1] };
            t.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(x, y, zc), mass: 0.5 });
        }
        t.cage = vec![0; 20];
        t.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(0.0, 0.125, 1.5), mass: 2.0 });
        let mut f = 1;
        for row in 0..2 {
            let e = 2 * row;
            let (g, h) = (f + 2, f + 1);
            let ring = [e, f, g, h, f + 5, f + 6, f + 8, f + 7];
            t.add_node_edges(&ring);
            if row == 1 {
                for &n in &ring {
                    t.add_node_edge(0, n, 20);
                }
            }
            t.panels.push(panel([e, h, g, f]));
            t.panels.push(panel([f + 5, f + 7, h, e]));
            if row == 0 {
                t.panels.push(panel([f + 6, f + 5, 0, f]));
                t.panels.push(panel([f + 8, f + 6, f, g]));
            } else {
                t.panels.push(panel([f + 8, f + 6, f, g]));
                t.panels.push(panel([f + 7, f + 8, g, h]));
                t.panels.push(panel([f + 5, f + 6, f + 8, f + 7]));
            }
            f = g;
        }
        t.add_node_edges(&[6, 7, 9, 8, 12, 13, 15, 14]);
        t.panels.extend([panel([12, 14, 8, 6]), panel([13, 12, 6, 7]), panel([15, 13, 7, 9]), panel([14, 15, 9, 8])]);
        t.add_node_edges(&[4, 5, 18, 19]);
        t.add_node_edges(&[12, 13, 15, 14, 16, 17, 19, 18]);
        t.panels.extend([
            window([16, 18, 14, 12], [-1, 16, 12, -1], [0.0, 0.7, 0.625, 0.0], 0.0),
            window([16, 18, 14, 12], [18, 16, 12, 14], [0.3, 0.25, 0.225, 0.375], 0.0),
            window([16, 18, 14, 12], [18, -1, -1, 14], [0.75, 0.0, 0.0, 0.775], 0.0),
            window([17, 16, 12, 13], [-1; 4], [0.0; 4], 6.0),
            window([19, 17, 13, 15], [17, -1, -1, 13], [0.7, 0.0, 0.0, 0.625], 0.0),
            window([19, 17, 13, 15], [17, 19, 15, 13], [0.25, 0.3, 0.375, 0.225], 0.0),
            window([19, 17, 13, 15], [-1, 19, 15, -1], [0.0, 0.75, 0.775, 0.0], 0.0),
            window([18, 19, 15, 14], [-1; 4], [0.0; 4], 0.0),
            panel([16, 17, 19, 18]),
        ]);
        for (a, b, weights) in [(0, 2, [0.75, 0.25]), (1, 3, [0.75, 0.25]), (2, 4, [0.25, 0.75]), (3, 5, [0.25, 0.75]), (2, 4, [0.5, 0.5]), (3, 5, [0.5, 0.5])] {
            t.add_wheel(0, a, b, weights, 0.375, 80.0, 512.0 / 45.0, -0.125, [0.25, 0.5, 0.5]);
        }
        t.track = 1.125;
        t.wheelbase = (z[1] + (z[2] - z[1]) * 0.75) - ((z[0] - z[1]) * 0.75 + z[1]);
        t.seats = vec![Vec3::new(-0.375, 0.125, -0.875), Vec3::new(0.375, 0.125, -0.875)];
        t.precompute_physics(VehicleKind::Truck);
        t
    }

    /// Trailer (type 11): a box of 12 nodes over three slices, an axle frame of four below its back slice and a
    /// hitch node ahead, on four wheels.
    fn trailer() -> Self {
        let mut t = VehicleType { name: "Trailer".to_string(), price: 1000, mass: 9000.0, ..Default::default() };
        let z = [-7.25, 0.0, 8.0];
        for i in 0..12 {
            let x = if i & 1 != 0 { 1.125 } else { -1.125 };
            let (y, mass) = match i {
                0..=1 => (0.0, 2.0),
                2..=3 => (-0.125, 2.0),
                4..=5 => (-0.125, 1.0),
                _ => (3.0, 1.0),
            };
            t.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(x, y, z[(i >> 1) % 3]), mass });
        }
        for i in 0..4 {
            let x = if i & 1 != 0 { 1.125 } else { -1.125 };
            t.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(x, -0.5, if i <= 1 { 6.0 } else { 8.0 }), mass: 1.0 });
        }
        t.cage = vec![0; 16];
        t.nodes.push(ChassisNode { unk_00: 0, pos: Vec3::new(0.0, 0.125, -7.0), mass: 2.0 });
        let mut f = 1;
        for row in 0..2 {
            let e = 2 * row;
            let (g, h) = (f + 2, f + 1);
            let ring = [e, f, g, h, f + 5, f + 6, f + 8, f + 7];
            t.add_node_edges(&ring);
            if row == 0 {
                for &n in &ring {
                    t.add_node_edge(0, n, 16);
                }
            }
            t.panels.push(panel([e, h, g, f]));
            t.panels.push(panel([f + 5, f + 7, h, e]));
            if row == 0 {
                t.panels.push(panel([f + 6, f + 5, 0, f]));
                t.panels.push(panel([f + 8, f + 6, f, g]));
            } else {
                t.panels.push(panel([f + 8, f + 6, f, g]));
                t.panels.push(panel([f + 7, f + 8, g, h]));
            }
            t.panels.push(panel([f + 5, f + 6, f + 8, f + 7]));
            f = g;
        }
        t.add_node_edges(&[12, 13, 15, 14, 2, 3, 5, 4]);
        t.add_node_edges(&[12, 13, 15, 14, 8, 9, 11, 10]);
        for (a, b, weights) in [(12, 14, [0.75, 0.25]), (13, 15, [0.75, 0.25]), (12, 14, [0.25, 0.75]), (13, 15, [0.25, 0.75])] {
            t.add_wheel(0, a, b, weights, 0.375, 80.0, 512.0 / 45.0, -0.125, [0.25, 0.5, 0.5]);
        }
        t.track = 1.125;
        t.wheelbase = ((z[2] - z[1]) * 0.75 + z[1]) - ((z[0] - z[1]) * 0.75 + z[1]);
        t.precompute_physics(VehicleKind::Trailer);
        t
    }

    /// vehicletype_attach_wheels, the older wheel setup the train uses: the body file's chassis nodes (heavier below
    /// the floor) without its springs, the first eight as the cage, a wheel at each mount as the sized setup hangs
    /// them but placed at the mount itself, the inertia about the origin and no centre of mass.
    fn attach_wheels_unsized(&mut self, body: &VehicleBody, kind: VehicleKind, drive: i32, radius: f32, mass: f32) {
        self.nodes.extend(body.file.nodes.iter().map(|n| ChassisNode { unk_00: 0, pos: n.pos.0, mass: if n.pos.0.y < 0.0 { 2.0 } else { 1.0 } }));
        self.cage = SBV_CAGE.to_vec();
        self.hang_wheels(body, kind, drive, radius, mass, body.file.wheels.len());
        for n in self.nodes.iter_mut() {
            if 0.0 > n.pos.y {
                n.mass = LOWER_NODE_MASS;
            }
        }
        let (inertia, total) = self.nodes.iter().fold((Vec3::ZERO, 0.0f32), |(acc, total), n| {
            let (dy, dx, dz) = (n.pos.y * n.pos.y, n.pos.x * n.pos.x, n.pos.z * n.pos.z);
            let m = n.mass;
            (Vec3::new((dy + dz) * m + acc.x, (dz + dx) * m + acc.y, (dy + dx) * m + acc.z), total + m)
        });
        let inv = 1.0 / total;
        self.inertia = Vec3::new(inertia.x * inv, inertia.y * inv, inv * inertia.z);
        for (w, mount) in self.wheels.iter_mut().zip(&body.file.wheels) {
            let p = mount.pos.0;
            w.local_pos = Vec3::new(p.x, p.y - 0.125, p.z);
            w.mass_offset = w.local_pos;
        }
    }

    /// Train (type 13, load_train), which is never given its physics precomputation.
    fn train(data: &std::path::Path) -> Option<Self> {
        let body = VehicleBody::load(&data.join("train04.sbv"))?;
        let mut t = VehicleType { body_file: 1, name: "Train".to_string(), price: 1000, mass: 320000.0, ..Default::default() };
        t.attach_wheels_unsized(&body, VehicleKind::Train, 0, 0.375, 12.0);
        t.body_meshes(&body);
        Some(t)
    }
}

/// The type a dealership or round corporation stocks: mostly town cars, hatchbacks, minivans and vans, now and then a
/// beamer, rarely a turbo and very rarely a Turbo S.
pub fn random_stock_vehicle() -> VehicleKind {
    match crate::rng::rand() & 15 {
        0..=1 => VehicleKind::Van,
        2..=5 => VehicleKind::TownCar,
        6..=9 => VehicleKind::Hatchback,
        10..=12 => VehicleKind::Minivan,
        13..=14 => VehicleKind::Beamer,
        _ if crate::rng::rand() & 15 == 0 => VehicleKind::TurboS,
        _ => VehicleKind::Turbo,
    }
}

/// The vehicle type table (load_towncar and friends): the body file cars and the types built in code.
pub fn vehicle_types(data: &std::path::Path) -> Vec<VehicleType> {
    let mut types = vec![VehicleType::default(); VEHICLE_TYPES];
    for spec in &SBV_TYPES {
        if let Some(t) = VehicleType::from_sbv(spec, data) {
            types[spec.kind as usize] = t;
        }
    }
    types[VehicleKind::TownCar2 as usize] = VehicleType::town_car_2();
    types[VehicleKind::Metro as usize] = VehicleType::metro();
    types[VehicleKind::Limo as usize] = VehicleType::limo();
    types[VehicleKind::Truck as usize] = VehicleType::truck();
    types[VehicleKind::Trailer as usize] = VehicleType::trailer();
    if let Some(t) = VehicleType::helicopter(data) {
        types[VehicleKind::Helicopter as usize] = t;
    }
    if let Some(t) = VehicleType::train(data) {
        types[VehicleKind::Train as usize] = t;
    }
    types
}
