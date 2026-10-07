use std::path::Path;

use glam::IVec3;
use rosa_map::file_types::{LoaderError, csx::CityFileCSX, sbc::CityFileSBC};

use crate::world::{grid::AreaGrid, roads::RoadNetwork};

pub struct Map {
    pub map_name: String,
    pub city: CityFileSBC,
    pub city_data: CityFileCSX,
    pub grid: AreaGrid
}

impl Map {
    pub fn load_map(map_name: String) -> Result<Self, LoaderError> {
        let name = format!("data/{}", map_name);
        let dir = Path::new(&name);

        if !dir.is_dir() || !dir.exists() {
            return Err(LoaderError::NotFound)
        }

        println!("[Map] Attempting to load {}/city2.sbc", map_name);
        let city = CityFileSBC::load(dir)?;

        println!("[Map] Map file version: {}", city.version);

        println!("[Map] Attempting to load {}/test.csx", map_name);
        let city_data = CityFileCSX::load(dir)?;

        println!("[Map] Attempting to build grid...");
        let mut grid = AreaGrid::build(&city, &city_data);

        let city_inters: Vec<IVec3> = city.intersections.iter().map(|i| i.0.as_ivec3()).collect();
        let city_streets: Vec<(usize, usize, i32, i32)> = city.streets.iter().map(|s| (s.intersection_indices[0] as usize, s.intersection_indices[1] as usize, s.left_lane as i32, s.right_lane as i32)).collect();
        RoadNetwork::build(&city_inters, &city_streets).stamp(&mut grid);

        let _ = grid.write_heightmap_ppm("map.ppm", 1);

        Ok(Self {
            map_name,
            city,
            city_data,
            grid
        })
    }
}