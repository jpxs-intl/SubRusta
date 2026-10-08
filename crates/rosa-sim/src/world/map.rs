use std::path::Path;

use rosa_map::file_types::{LoaderError, csx::CityFileCSX, sbc::CityFileSBC};

use crate::world::{
    grid::write_world_ppm,
    ground::{Ground, generate_grass},
    level::{Level, build_level},
    roads::RoadNetwork,
};

pub struct Map {
    pub map_name: String,
    pub city: CityFileSBC,
    pub city_data: CityFileCSX,
    pub level: Level,
    pub ground: Ground
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

        let roundcity = map_name == "round";

        let mut roads = RoadNetwork::from_city(&city);

        let mut ground = Ground::new(generate_grass(!roundcity), roundcity);
        if let Some(bounds) = roads.bounds() {
            ground.set_street_bounds(bounds);
        }

        println!("[Map] Attempting to build grid...");
        let level = build_level(&city, &city_data, &mut ground, &roads, dir, Path::new("data"), &map_name, 0);

        roads.compute_world_bounds();
        ground.build_city_blocks(&roads);

        let _ = write_world_ppm(&level.area, &ground, "map.ppm", 1);

        println!("[Map] Loading complete!");

        Ok(Self {
            map_name,
            city,
            city_data,
            level,
            ground
        })
    }
}