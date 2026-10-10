use glam::Vec3;

pub type RotMatrix = [Vec3; 3];

pub const IDENTITY: RotMatrix = [Vec3::X, Vec3::Y, Vec3::Z];

pub fn rotate_orientation(rot: &mut RotMatrix, axis: Vec3, angle: f32) {
    let (s, c) = (angle as f64).sin_cos();
    let (s, c) = (s as f32, c as f32);
    let (ax, ay, az) = (axis.x, axis.y, axis.z);
    for v in rot.iter_mut() {
        let (vx, vy, vz) = (v.x, v.y, v.z);
        let cy = ax * vz - az * vx;
        let cz = ay * vx - ax * vy;
        let cx = az * vy - ay * vz;
        let dot = (vy * ay + vx * ax) + vz * az;
        let nx = ((ay * cz - az * cy) * c + ax * dot) + s * cx;
        let ny = ((az * cx - ax * cz) * c + ay * dot) + s * cy;
        let nz = ((cy * ax - cx * ay) * c + dot * az) + cz * s;
        let len = (nz * nz + (nx * nx + ny * ny)).sqrt();
        *v = if len == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len;
            Vec3::new(nx * inv, ny * inv, inv * nz)
        };
    }
}

/// rotate_vector_about_axis: `v` turned by `angle` about the unit `axis`.
pub fn rotate_vector_about_axis(v: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    let (s, c) = (angle as f64).sin_cos();
    let (s, c) = (s as f32, c as f32);
    let a = axis;
    let cx = v.y * a.z - v.z * a.y;
    let cy = v.z * a.x - v.x * a.z;
    let cz = v.x * a.y - v.y * a.x;
    let d = (v.y * a.y + v.x * a.x) + v.z * a.z;
    Vec3::new(
        ((cz * a.y - cy * a.z) * c + d * a.x) + s * cx,
        ((cx * a.z - cz * a.x) * c + d * a.y) + s * cy,
        ((cy * a.x - cx * a.y) * c + d * a.z) + s * cz,
    )
}

pub fn angular_velocity(rot: &RotMatrix, momentum: Vec3, inv_inertia: Vec3) -> Vec3 {
    let [r0, r1, r2] = *rot;
    let l = momentum;
    let a = ((l.x * r0.x + l.y * r0.y) + l.z * r0.z) * inv_inertia.x;
    let b = ((l.x * r1.x + l.y * r1.y) + l.z * r1.z) * inv_inertia.y;
    let c = ((l.y * r2.y + l.x * r2.x) + l.z * r2.z) * inv_inertia.z;
    Vec3::new(
        ((r0.x * a + 0.0) + r1.x * b) + r2.x * c,
        ((r0.y * a + 0.0) + r1.y * b) + r2.y * c,
        ((r0.z * a + 0.0) + r1.z * b) + r2.z * c,
    )
}

fn length(v: Vec3) -> f32 {
    (v.z * v.z + (v.x * v.x + v.y * v.y)).sqrt()
}

fn turned(rot: &RotMatrix, w: Vec3, half: bool) -> RotMatrix {
    let mut out = *rot;
    let n = length(w);
    if n > (1.0/65536.0) {
        let inv = 1.0 / n;
        rotate_orientation(&mut out, Vec3::new(w.x * inv, w.y * inv, inv * w.z), if half { n * 0.5 } else { n });
    }
    out
}

pub fn rk4_rotation(rot: &mut RotMatrix, momentum: Vec3, inv_inertia: Vec3) -> Vec3 {
    let k1 = angular_velocity(rot, momentum, inv_inertia);
    let k2 = angular_velocity(&turned(rot, k1, true), momentum, inv_inertia);
    let k3 = angular_velocity(&turned(rot, k2, true), momentum, inv_inertia);
    let k4 = angular_velocity(&turned(rot, k3, false), momentum, inv_inertia);

    let avg = Vec3::new(
        (((k2.x + k2.x) + k1.x) + (k3.x + k3.x) + k4.x) * (1.0 / 6.0),
        (((k1.y + (k2.y + k2.y)) + (k3.y + k3.y)) + k4.y) * (1.0 / 6.0),
        (1.0 / 6.0) * (k4.z + ((k3.z + k3.z) + ((k2.z + k2.z) + k1.z))),
    );

    let n = (avg.z * avg.z + (avg.x * avg.x + avg.y * avg.y)).sqrt();

    if n > (1.0/65536.0) {
        let inv = 1.0 / n;
        rotate_orientation(rot, Vec3::new(avg.x * inv, avg.y * inv, inv * avg.z), n);
    }

    avg
}

pub fn quaternion_multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        ((aw * bx + ax * bw) + ay * bz) - az * by,
        ((aw * by - ax * bz) + bw * ay) + bx * az,
        ((aw * bz + ax * by) - bx * ay) + bw * az,
        ((aw * bw - bx * ax) - ay * by) - bz * az,
    ]
}

fn unit_axis(q: [f32; 4]) -> Vec3 {
    let len = ((q[0] * q[0] + q[1] * q[1]) + q[2] * q[2]).sqrt();
    if len == 0.0 {
        return Vec3::ZERO;
    }
    let inv = 1.0 / len;
    Vec3::new(q[0] * inv, q[1] * inv, inv * q[2])
}

fn half_angle_acos(w: f32) -> f32 {
    let c = if -1.0 > w {
        -1.0
    } else if w <= 1.0 {
        w as f64
    } else {
        1.0
    };

    let a = c.acos();
    (a + a) as f32
}

/// quaternion_to_axis_angle: the unit rotation axis and the angle in radians.
pub fn quaternion_to_axis_angle(q: [f32; 4]) -> (Vec3, f32) {
    (unit_axis(q), half_angle_acos(q[3]))
}

/// quaternion_to_rotation_vector: the rotation axis scaled by the angle.
pub fn quaternion_to_rotation_vector(q: [f32; 4]) -> Vec3 {
    let axis = unit_axis(q);
    let angle = half_angle_acos(q[3]);
    Vec3::new(axis.x * angle, axis.y * angle, angle * axis.z)
}

/// multiply_matrixes: out[c] = (a[0]·b[c], a[1]·b[c], a[2]·b[c]).
pub fn multiply_matrixes(a: &RotMatrix, b: &RotMatrix) -> RotMatrix {
    let dot = |r: Vec3, c: Vec3| (r.y * c.y + r.x * c.x) + r.z * c.z;
    b.map(|c| Vec3::new(dot(a[0], c), dot(a[1], c), dot(a[2], c)))
}

/// vector_clamp_horizontal_and_vertical: limits the XZ length to `horizontal` and Y to ±`vertical`.
pub fn clamp_horizontal_vertical(v: &mut Vec3, horizontal: f32, vertical: f32) {
    let len = ((v.x * v.x + 0.0) + v.z * v.z).sqrt();
    if len > horizontal {
        let (x, z) = if len == 0.0 {
            (0.0, 0.0)
        } else {
            let inv = 1.0 / len;
            (v.x * inv, v.z * inv)
        };
        v.x = x * horizontal;
        v.z = z * horizontal;
    }
    let neg = -vertical;
    let lo = if neg > v.y { neg } else { v.y };
    v.y = if vertical < lo { vertical } else { lo };
}

pub fn quaternion_normalize(q: [f32; 4]) -> [f32; 4] {
    let [x, y, z, w] = q;
    let len = ((z * z + (x * x + y * y)) + w * w) as f64;
    let len = len.sqrt();
    if len == 0.0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let inv = 1.0 / len;
    q.map(|c| (c as f64 * inv) as f32)
}

pub fn quaternion_to_rot_matrix(q: [f32; 4]) -> RotMatrix {
    let [x, y, z, w] = q;
    let d = |a: f32, b: f32| {
        let p = a * b;
        p + p
    };
    let (wx, xx, wz, xz, wy, xy, yy, yz, zz) = (d(w, x), d(x, x), d(w, z), d(x, z), d(w, y), d(x, y), d(y, y), d(y, z), d(z, z));
    [
        Vec3::new(1.0 - (yy + zz), xy - wz, wy + xz),
        Vec3::new(wz + xy, 1.0 - (zz + xx), yz - wx),
        Vec3::new(xz - wy, yz + wx, 1.0 - (xx + yy)),
    ]
}

pub fn rot_matrix_to_quaternion(rot: &RotMatrix) -> [f32; 4] {
    let [r0, r1, r2] = *rot;
    let (x1, y1, z1, x2, y2, z2, x3, y3, z3) = (r0.x, r0.y, r0.z, r1.x, r1.y, r1.z, r2.x, r2.y, r2.z);
    let scale = |u: f32| -> (f32, f64) {
        let s = (2.0 * ((u + 1.0) as f64).sqrt()) as f32;
        (s, if s > 1.0 / 65536.0 { (1.0 / s) as f64 } else { 0.0 })
    };
    let diag = |u: f32| -> (f32, f64) { if -1.0 > u { (0.0, 0.0) } else { let (s, inv) = scale(u); (0.25 * s, inv) } };
    let mul = |a: f32, inv: f64| (a as f64 * inv) as f32;
    let t = (x1 + y2) + z3;
    if t >= 0.0 {
        let (s, inv) = scale(t);
        [mul(y3 - z2, inv), mul(z1 - x3, inv), mul(x2 - y1, inv), s * 0.25]
    } else if x1 > y2 && x1 > z3 {
        let (x, inv) = diag((x1 - y2) - z3);
        [x, mul(y1 + x2, inv), mul(z1 + x3, inv), mul(y3 - z2, inv)]
    } else if y2 > z3 {
        let (y, inv) = diag((y2 - x1) - z3);
        [mul(y1 + x2, inv), y, mul(z2 + y3, inv), mul(z1 - x3, inv)]
    } else {
        let (z, inv) = diag((z3 - x1) - y2);
        [mul(z1 + x3, inv), mul(z2 + y3, inv), z, mul(x2 - y1, inv)]
    }
}
