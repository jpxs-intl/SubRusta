use crate::world::{area::AreaGrid, ground::Ground};

pub fn write_world_ppm(area: &AreaGrid, ground: &Ground, path: &str, step: u32) -> std::io::Result<()> {
        use std::io::Write;
        let (dx, dz) = (area.chunks.x as u32, area.chunks.z as u32);

        let (mut minx, mut minz, mut maxx, mut maxz) = (u32::MAX, u32::MAX, 0u32, 0u32);
        let mut cells: Vec<(u32, u32, u32)> = Vec::new();
        for r in area.records() {
            for c in 0..crate::world::area::CELLS {
                if r.layer0[c] == 0 && r.layer1[c] == 0 { continue; }
                let (lx, lz, ly) = ((c & 7) as u32, ((c >> 3) & 7) as u32, (c >> 6) as u32);
                let cx = r.pos.x as u32 * 8 + lx;
                let cy = r.pos.y as u32 * 8 + ly;
                let cz = r.pos.z as u32 * 8 + lz;
                minx = minx.min(cx); maxx = maxx.max(cx);
                minz = minz.min(cz); maxz = maxz.max(cz);
                cells.push((cx, cz, cy + 1));
            }
        }
        if cells.is_empty() { eprintln!("write_world_ppm: empty grid"); return Ok(()); }

        let margin = 24;
        let ox = minx.saturating_sub(margin);
        let oz = minz.saturating_sub(margin);
        let ex = (maxx + margin).min(dx * 64 - 1);
        let ez = (maxz + margin).min(dz * 64 - 1);
        let w = ((ex - ox) / step + 1) as usize;
        let h = ((ez - oz) / step + 1) as usize;

        let mut block_top = vec![0u32; w * h];
        for (cx, cz, top) in &cells {
            if *cx < ox || *cz < oz || *cx > ex || *cz > ez { continue; }
            let idx = ((cz - oz) / step) as usize * w + ((cx - ox) / step) as usize;
            if *top > block_top[idx] { block_top[idx] = *top; }
        }

        let (mut tmin, mut tmax) = (f32::MAX, f32::MIN);
        for pz in 0..h { for px in 0..w {
            let (cx, cz) = (ox + px as u32 * step, oz + pz as u32 * step);
            if let Some(ty) = ground.height_at(cx as f32 * 4.0 + 2.0, cz as f32 * 4.0 + 2.0) {
                tmin = tmin.min(ty); tmax = tmax.max(ty);
            }
        }}
        if tmax <= tmin { tmax = tmin + 1.0; }

        let (mut bmin, mut bmax) = (f32::MAX, f32::MIN);
        for &t in block_top.iter().filter(|&&t| t > 0) {
            let bh = t as f32 * 4.0;
            bmin = bmin.min(bh); bmax = bmax.max(bh);
        }
        if bmax <= bmin { bmax = bmin + 1.0; }

        let mut buf = vec![0u8; w * h * 3];
        for pz in 0..h { for px in 0..w {
            let i = (pz * w + px) * 3;
            let (cx, cz) = (ox + px as u32 * step, oz + pz as u32 * step);

            if let Some(ty) = ground.height_at(cx as f32 * 4.0 + 2.0, cz as f32 * 4.0 + 2.0) {
                let t = ((ty - tmin) / (tmax - tmin)).clamp(0.0, 1.0);
                buf[i]     = ((1.0 - t) * 255.0) as u8;
                buf[i + 1] = 0;
                buf[i + 2] = (t * 255.0) as u8;
            }

            let top = block_top[pz * w + px];
            if top > 0 {
                let s = ((top as f32 * 4.0 - bmin) / (bmax - bmin)).clamp(0.0, 1.0);
                let v = (60.0 + s * 195.0) as u8;
                buf[i] = v; buf[i + 1] = v; buf[i + 2] = v;
            }
        }}

        let mut f = std::fs::File::create(path)?;
        write!(f, "P6\n{w} {h}\n255\n")?;
        f.write_all(&buf)
}

pub fn terrain_height_world(base: &[f32], wx: f32, wz: f32) -> f32 {
    let fx = ((wx + 4096.0) / 8.0).clamp(0.0, (2049 - 2) as f32);
    let fz = ((wz + 4096.0) / 8.0).clamp(0.0, (2049 - 2) as f32);
    let (x0, z0) = (fx.floor() as usize, fz.floor() as usize);
    let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);

    let h00 = base[z0 * 2049 + x0];
    let h10 = base[z0 * 2049 + x0 + 1];
    let h01 = base[(z0 + 1) * 2049 + x0];
    let h11 = base[(z0 + 1) * 2049 + x0 + 1];

    (1.0 - tx) * (h00 * (1.0 - tz) + h01 * tz)
        + tx * (tz * h11 + (1.0 - tz) * h10)
}