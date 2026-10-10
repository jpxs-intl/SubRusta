use super::{MAX_ROUTE, TrafficCar};
use crate::world::streets::StreetMap;

const SEARCH_STEPS: usize = 4096;
const NO_BEST: f32 = 65536.0;
const HEURISTIC: f32 = 1.0 / 128.0;
const STEP_COST: f32 = 1.0;
const DEAD_END_COST: f32 = 256.0;
const OPEN: i32 = 1;
const CLOSED: i32 = 2;

#[derive(Clone, Copy, Default)]
struct SearchNode {
    state: i32,
    cost: f32,
    parent: usize,
}

/// One step of the route search (0x4081a0): the open node with the lowest cost plus straight distance to the goal is
/// closed and its neighbours opened; reaching the goal fills `path` from the goal back to the start.
fn search_step(map: &StreetMap, nodes: &mut [SearchNode], from: usize, to: usize, path: &mut Vec<usize>) {
    let goal = map.route_nodes[to].pos;
    let mut best = NO_BEST;
    let mut sel = 0;
    for (i, n) in map.route_nodes.iter().enumerate() {
        if nodes[i].state != OPEN {
            continue;
        }
        let (dx, dy, dz) = (goal.x - n.pos.x, goal.y - n.pos.y, goal.z - n.pos.z);
        let f = (dz * dz + (dx * dx + dy * dy)).sqrt() * HEURISTIC + nodes[i].cost;
        if best > f {
            sel = i;
            best = f;
        }
    }
    if nodes[sel].state != OPEN {
        return;
    }
    nodes[sel].state = CLOSED;
    for &j in &map.route_nodes[sel].neighbors {
        let j = j as usize;
        let cost = nodes[sel].cost + STEP_COST;
        let dead_end = map.route_nodes[j].dead_end != 0;
        match nodes[j].state {
            CLOSED => continue,
            0 => {
                nodes[j] = SearchNode { state: OPEN, cost: if dead_end { cost + DEAD_END_COST } else { cost }, parent: sel };
            }
            _ => {
                if !(nodes[j].cost > cost) {
                    continue;
                }
                nodes[j].parent = sel;
                nodes[j].cost = if dead_end { cost + DEAD_END_COST } else { cost };
            }
        }
        if j == to {
            path.push(j);
            let mut cur = j;
            while cur != from {
                cur = nodes[cur].parent;
                path.push(cur);
            }
            return;
        }
    }
}

/// traffic_car_plan_route: the route from one end of a street to one end of another, as the steps the car drives and
/// the turn it takes onto each.
pub fn plan_route(car: &mut TrafficCar, map: &StreetMap, from_street: i32, from_slot: i32, to_street: i32, to_slot: i32) {
    let count = map.route_nodes.len();
    let (from, to) = ((from_street * 2 + from_slot) as usize, (to_street * 2 + to_slot) as usize);
    let mut path = Vec::new();
    if from < count && to < count {
        let mut nodes = vec![SearchNode::default(); count];
        nodes[from].state = OPEN;
        for _ in 0..SEARCH_STEPS {
            search_step(map, &mut nodes, from, to, &mut path);
            if !path.is_empty() {
                break;
            }
        }
    }
    // TODO: the binary searches garbage for a street past the map's last (the Round City loop streets elsewhere)
    car.route_index = 0;
    car.route_len = 0;
    car.route[0].turn = 0;
    let n = path.len();
    if n == 0 {
        return;
    }
    let (mut from_way, mut to_way) = (0, 0);
    let mut street = map.route_nodes[path[n - 1]].street as i32;
    // TODO: a path longer than the record's 63 steps runs over the fields after them in the binary
    for i in 1..=n.min(MAX_ROUTE) {
        let end = map.route_nodes[path[n - i]].end;
        car.route[i - 1].street = street;
        car.route[i - 1].end = end;
        car.route_len = i as i32;
        if i == n || i == MAX_ROUTE {
            break;
        }
        let st = &map.streets[street as usize];
        let at = &map.intersections[st.intersections[if end != 0 { 0 } else { 1 }]];
        let next = map.route_nodes[path[n - 1 - i]].street as i32;
        for (k, &s) in at.streets.iter().enumerate() {
            if s == street {
                from_way = k as i32;
            }
            if s == next {
                to_way = k as i32;
            }
        }
        let mut diff = from_way - to_way;
        if diff < -1 {
            diff += 4;
        } else if diff > 1 {
            diff -= 4;
        }
        car.route[i].turn = if diff == -1 { -1 } else { (diff == 1) as i32 };
        street = next;
    }
}
