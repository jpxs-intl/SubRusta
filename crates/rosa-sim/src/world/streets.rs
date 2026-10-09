use glam::{IVec3, Vec3};

use super::roads::RoadNetwork;

/// The world size of a city grid unit.
const UNIT: f32 = 4.0;
/// Lanes run every unit across the street, half a unit off its edges' centre lines.
const LANE_INSET: f32 = 2.0;
/// The lanes sit just above the street surface.
const LANE_LIFT: f32 = 0.125;
/// The street's bounds reach one unit above its lower end and two above its higher end.
const BOUNDS_LOW_LIFT: f32 = 4.0;
const BOUNDS_HIGH_LIFT: f32 = 8.0;
/// Street names start at grid 256 and come every 56 units east-west (avenues) or 32 north-south (streets).
const NAME_ORIGIN: i32 = 256;
const AVENUE_SPACING: i32 = 56;
const STREET_SPACING: i32 = 32;
const AVENUES: [&str; 8] = ["Abrash Ave", "Bunten Ave", "Carmack Ave", "David Ave", "Eugene Ave", "Flinn Ave", "Garriott Ave", "Holland Ave"];
const STREETS: [&str; 9] = ["First Street", "Second Street", "Third Street", "Fourth Street", "Fifth Street", "Sixth Street", "Seventh Street", "Eighth Street", "Ninth Street"];
/// Avenue ids follow the nine street ids.
const FIRST_AVENUE_ID: i32 = 9;

/// A street intersection (level_data +0x64, 0x88 each).
#[derive(Clone, Debug, Default)]
pub struct StreetIntersection {
    /// Its city grid position (+0x00) and world position (+0x0c).
    pub pos: IVec3,
    pub world_pos: Vec3,
    /// The street leaving it east, south, west and north (+0x18, -1 for none).
    pub streets: [i32; 4],
    /// How far the streets around it reach in each direction (+0x28), in grid units.
    pub lanes: [i32; 4],
    /// The box the intersection's road covers (+0x70, +0x7c).
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

/// A lane of a street: which way it runs (0 from the first intersection, 1 back from the second) and its ends.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lane {
    pub reverse: i32,
    pub start: Vec3,
    pub end: Vec3,
}

/// A street between two intersections (level_data +0x22068, 0x630 each).
#[derive(Clone, Debug, Default)]
pub struct MapStreet {
    pub name: &'static str,
    pub id: i32,
    pub intersections: [usize; 2],
    /// 1 for streets running along z (+0x2c).
    pub north_south: i32,
    pub left_lanes: i32,
    pub right_lanes: i32,
    /// The left lanes from the far side in, then the right lanes from the centre out (+0x38 count, +0x3c).
    pub lanes: Vec<Lane>,
    /// The unit direction from the first intersection to the second (+0x1fc) and across it (+0x208).
    pub dir: Vec3,
    pub side: Vec3,
    /// The box the street's road covers (+0x214, +0x220).
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

/// A node of the street route graph (build_street_route_graph, 0x48 each): the middle of a street's last lane
/// (`end` 0, heading to its second intersection) or first lane (`end` 1, heading back), and the nodes it leads to.
#[derive(Clone, Debug, Default)]
pub struct RouteNode {
    pub pos: Vec3,
    pub neighbors: Vec<i32>,
    pub street: usize,
    pub end: i32,
    pub dead_end: i32,
}

/// The streets traffic drives on, built after the city's road blocks.
#[derive(Clone, Debug, Default)]
pub struct StreetMap {
    pub intersections: Vec<StreetIntersection>,
    pub streets: Vec<MapStreet>,
    pub route_nodes: Vec<RouteNode>,
    /// The grid extent of the intersections (level_data +0x44, +0x50; only x and z are set).
    pub min: IVec3,
    pub max: IVec3,
}

fn normalized(v: Vec3) -> Vec3 {
    let len = (v.z * v.z + (v.x * v.x + v.y * v.y)).sqrt();
    if len == 0.0 {
        return Vec3::ZERO;
    }
    let inv = 1.0 / len;
    Vec3::new(v.x * inv, v.y * inv, inv * v.z)
}

fn street_name(north_south: bool, i0: IVec3) -> (&'static str, i32) {
    if north_south {
        let k = (i0.x - NAME_ORIGIN) / AVENUE_SPACING;
        AVENUES.get(k as usize).filter(|_| k >= 0).map_or(("", 0), |&n| (n, FIRST_AVENUE_ID + k))
    } else {
        let k = (i0.z - NAME_ORIGIN) / STREET_SPACING + 1;
        STREETS.get((k - 1) as usize).filter(|_| k >= 1).map_or(("", 0), |&n| (n, k - 1))
    }
}

impl StreetMap {
    /// create_intersection_internal, create_road, intersection_compute_world_bounds, street_compute_lane_geometry
    /// and build_street_route_graph over the city's road network (after its lane widths are final).
    pub fn build(roads: &RoadNetwork) -> Self {
        let mut map = StreetMap { min: IVec3::new(i32::MAX, 0, i32::MAX), max: IVec3::new(i32::MIN, 0, i32::MIN), ..Default::default() };
        for (pos, lanes, streets) in roads.intersection_fields() {
            map.min = IVec3::new(map.min.x.min(pos.x), 0, map.min.z.min(pos.z));
            map.max = IVec3::new(map.max.x.max(pos.x), 0, map.max.z.max(pos.z));
            let world_pos = Vec3::new(pos.x as f32 * UNIT, (pos.y + 1) as f32 * UNIT, pos.z as f32 * UNIT);
            let bounds_min = Vec3::new((pos.x - lanes[2]) as f32 * UNIT, 0.0, (pos.z - lanes[3]) as f32 * UNIT);
            let bounds_max = Vec3::new((pos.x + lanes[0]) as f32 * UNIT, 0.0, (pos.z + lanes[1]) as f32 * UNIT);
            map.intersections.push(StreetIntersection { pos, world_pos, streets, lanes, bounds_min, bounds_max });
        }
        for (i0, i1, left, right) in roads.street_ends() {
            map.streets.push(map.street(i0, i1, left, right));
        }
        map.build_route_graph();
        map
    }

    fn street(&self, i0: usize, i1: usize, left: i32, right: i32) -> MapStreet {
        let (a, b) = (&self.intersections[i0], &self.intersections[i1]);
        let north_south = b.pos.x - a.pos.x <= b.pos.z - a.pos.z;
        let (name, id) = street_name(north_south, a.pos);
        let len = if north_south {
            (b.pos.z - b.lanes[3] - a.lanes[1] - a.pos.z) as f32 * UNIT
        } else {
            (b.pos.x - b.lanes[2] - (a.lanes[0] + a.pos.x)) as f32 * UNIT
        };
        let dir = normalized(Vec3::new((b.pos.x - a.pos.x) as f32, 0.0, (b.pos.z - a.pos.z) as f32));
        let side = Vec3::new(-dir.z, 0.0, dir.x);
        let (mut lo, mut hi);
        if north_south {
            lo = Vec3::new(a.pos.x as f32 * UNIT, 0.0, (a.lanes[1] + a.pos.z) as f32 * UNIT);
            let k = right as f32 * UNIT;
            lo = Vec3::new(side.x * k + lo.x, side.y * k + lo.y, k * side.z + lo.z);
            hi = Vec3::new(b.pos.x as f32 * UNIT, 0.0, (b.pos.z - b.lanes[3]) as f32 * UNIT);
        } else {
            lo = Vec3::new((a.lanes[0] + a.pos.x) as f32 * UNIT, 0.0, a.pos.z as f32 * UNIT);
            let k = -(left as f32 * UNIT);
            lo = Vec3::new(side.x * k + lo.x, side.y * k + lo.y, k * side.z + lo.z);
            hi = Vec3::new((b.pos.x - b.lanes[2]) as f32 * UNIT, 0.0, b.pos.z as f32 * UNIT);
        }
        let (ya, yb) = (a.pos.y as f32 * UNIT, b.pos.y as f32 * UNIT);
        if a.pos.y >= b.pos.y {
            lo.y = yb + BOUNDS_LOW_LIFT;
            hi.y = ya + BOUNDS_HIGH_LIFT;
        } else {
            lo.y = ya + BOUNDS_LOW_LIFT;
            hi.y = yb + BOUNDS_HIGH_LIFT;
        }
        let lane_y = (a.pos.y + 1) as f32 * UNIT + LANE_LIFT;
        let (lane_x, lane_z);
        if north_south {
            let k = -(left as f32 * UNIT);
            hi = Vec3::new(side.x * k + hi.x, side.y * k + hi.y, k * side.z + hi.z);
            lane_x = a.pos.x as f32 * UNIT;
            lane_z = (a.pos.z + a.lanes[1]) as f32 * UNIT;
        } else {
            let k = right as f32 * UNIT;
            hi = Vec3::new(side.x * k + hi.x, hi.y + side.y * k, k * side.z + hi.z);
            lane_x = (a.pos.x + a.lanes[0]) as f32 * UNIT;
            lane_z = a.pos.z as f32 * UNIT;
        }
        let lane = |reverse: i32, off: f32| {
            let start = Vec3::new(side.x * off + lane_x, side.y * off + lane_y, side.z * off + lane_z);
            Lane { reverse, start, end: Vec3::new(start.x + dir.x * len, start.y + dir.y * len, start.z + dir.z * len) }
        };
        let lanes = (1..=left).rev().map(|k| lane(0, -(k as f32 * UNIT - LANE_INSET))).chain((1..=right).map(|k| lane(1, k as f32 * UNIT - LANE_INSET))).collect();
        MapStreet { name, id, intersections: [i0, i1], north_south: north_south as i32, left_lanes: left, right_lanes: right, lanes, dir, side, bounds_min: lo, bounds_max: hi }
    }

    fn build_route_graph(&mut self) {
        let mid = |l: &Lane| Vec3::new((l.start.x + l.end.x) * 0.5, (l.start.y + l.end.y) * 0.5, (l.start.z + l.end.z) * 0.5);
        self.route_nodes = self
            .streets
            .iter()
            .enumerate()
            .flat_map(|(s, st)| {
                let (first, last) = (st.lanes.first().copied().unwrap_or_default(), st.lanes.last().copied().unwrap_or_default());
                [RouteNode { pos: mid(&last), street: s, end: 0, ..Default::default() }, RouteNode { pos: mid(&first), street: s, end: 1, ..Default::default() }]
            })
            .collect();
        for n in 0..self.route_nodes.len() {
            let (s, end) = (self.route_nodes[n].street, self.route_nodes[n].end);
            let at = self.streets[s].intersections[if end != 0 { 0 } else { 1 }];
            let neighbors: Vec<i32> = self.intersections[at]
                .streets
                .iter()
                .enumerate()
                .filter(|&(_, &o)| o != -1 && o != s as i32)
                .map(|(i, &o)| o * 2 + (i > 1) as i32)
                .collect();
            let node = &mut self.route_nodes[n];
            if neighbors.is_empty() {
                node.dead_end = 1;
                node.neighbors = vec![if end != 0 { s as i32 * 2 } else { s as i32 * 2 + 1 }];
            } else {
                node.neighbors = neighbors;
            }
        }
    }
}
