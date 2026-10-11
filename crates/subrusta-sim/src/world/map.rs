use std::path::Path;

use subrusta_map::file_types::{LoaderError, csx::CityFileCSX, sbc::CityFileSBC};

use crate::world::{
    ground::{Ground, generate_grass},
    level::{Level, build_level},
    roads::RoadNetwork,
    streets::StreetMap,
};

pub struct Map {
    pub map_name: String,
    pub level: Level,
    pub ground: Ground,
    /// The streets, lanes and route graph traffic drives on.
    pub streets: StreetMap,
}

impl Map {
    pub(crate) fn load_map(map_name: String) -> Result<Self, LoaderError> {
        let name = format!("data/{}", map_name);
        let dir = Path::new(&name);

        if !dir.is_dir() || !dir.exists() {
            return Err(LoaderError::NotFound);
        }

        println!("[Map] Attempting to load map files for {}.", map_name);
        let (city, city_data) = rayon::join(|| CityFileSBC::load(dir), || CityFileCSX::load(dir));
        let (city, city_data) = (city?, city_data?);

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
        let streets = StreetMap::build(&roads);
        let mut level = level;
        crate::world::building::assign_streets(&mut level.buildings, &streets);

        println!("[Map] Loading complete!");

        Ok(Self { map_name, level, ground, streets })
    }
}
