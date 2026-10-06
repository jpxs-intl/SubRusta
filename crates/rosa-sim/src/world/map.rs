use std::path::Path;

use rosa_map::file_types::{LoaderError, sbc::CityFileSBC};

pub struct Map {
    pub map_name: String,
    pub city: CityFileSBC
}

impl Map {
    pub fn load_map(map_name: String) -> Result<Self, LoaderError> {
        let name = format!("data/{}", map_name);
        let dir = Path::new(&name);

        if !dir.is_dir() || !dir.exists() {
            return Err(LoaderError::NotFound)
        }

        let city_file = CityFileSBC::load(dir)?;

        println!("[Map] Successfully loaded {}/city2.sbc", map_name);

        Ok(Self {
            map_name,
            city: city_file
        })
    }
}