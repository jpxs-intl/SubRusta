use std::path::Path;

use rosa_map::file_types::{LoaderError, csx::CityFileCSX, sbc::CityFileSBC};

use crate::world::{grid::AreaGrid, ground::{Ground, generate_grass}, roads::RoadNetwork};

pub struct Map {
    pub map_name: String,
    pub city: CityFileSBC,
    pub city_data: CityFileCSX,
    pub grid: AreaGrid,
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

        // reset_game: hack_roundcity_traffic = 1 only for the "round" map
        let roundcity = map_name == "round";

        // road graph first: its intersection bbox gates terrain-mask clearing (load_map order)
        let mut roads = RoadNetwork::from_city(&city);

        let mut ground = Ground::new(generate_grass(!roundcity), roundcity); // border applies on non-round maps
        if let Some(bounds) = roads.bounds() {
            ground.set_street_bounds(bounds);
        }

        println!("[Map] Attempting to build grid...");
        let grid = AreaGrid::build(&city, &city_data, &mut ground, &roads);

        // intersection_compute_world_bounds widens the lanes after the roads are placed...
        roads.compute_world_bounds();
        // ...then build_traffic_navmap runs last: rewrites the hardcoded city blocks from the roadmap
        ground.build_city_blocks(&roads);

        let _ = grid.write_world_ppm(&ground, "map.ppm", 1);

        println!("[Map] Loading complete!");

        Ok(Self {
            map_name,
            city,
            city_data,
            grid,
            ground
        })
    }
}