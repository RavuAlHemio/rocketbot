use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use rocketbot_interface::JsonValueExtensions;

use crate::GeocodingError;


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountryCodeMapping {
    pub alpha2: HashSet<String>,
    pub license_plate_to_alpha2: HashMap<String, String>,
    pub alpha3_to_alpha2: HashMap<String, String>,
}
impl CountryCodeMapping {
    pub fn load_from_file(path: &Path) -> Result<CountryCodeMapping, GeocodingError> {
        let mut file = File::open(path)
            .map_err(|e| GeocodingError::OpeningFile(e))?;

        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|e| GeocodingError::OpeningFile(e))?;

        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| GeocodingError::CountryCodeParsing(e))?;

        let mut alpha2 = HashSet::new();
        let mut license_plate_to_alpha2 = HashMap::new();
        let mut alpha3_to_alpha2 = HashMap::new();

        for item in json.members().ok_or(GeocodingError::CountryCodesNotList)? {
            let my_alpha2: String = item["alpha2"]
                .as_str().expect("alpha2 is missing or not a string")
                .to_owned();

            let my_alpha3: Option<String> = if item["alpha3"].is_null() {
                None
            } else {
                Some(
                    item["alpha3"]
                        .as_str().expect("alpha3 is missing or not a string")
                        .to_owned()
                )
            };

            let my_license_plate: Option<String> = if item["plate"].is_null() {
                None
            } else {
                Some(
                    item["plate"]
                        .as_str().expect("plate is missing or not a string")
                        .to_owned()
                )
            };

            alpha2.insert(my_alpha2.clone());
            if let Some(ma3) = my_alpha3 {
                alpha3_to_alpha2.insert(ma3.clone(), my_alpha2.clone());
            }
            if let Some(mlp) = my_license_plate {
                license_plate_to_alpha2.insert(mlp.clone(), my_alpha2.clone());
            }
        }

        Ok(CountryCodeMapping {
            alpha2,
            license_plate_to_alpha2,
            alpha3_to_alpha2,
        })
    }
}
