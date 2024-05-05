#[cfg(feature = "confusion")]
pub mod confusion;
pub mod country_codes;
pub mod providers;
mod s11n;


use std::fmt;
use std::io;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_interface::ResultExtensions;
use serde::{Deserialize, Serialize};
use serde_json;
use tracing::error;

use crate::country_codes::CountryCodeMapping;
#[cfg(feature = "confusion")]
use crate::confusion::Confuser;


static POST_CODE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^(?P<country>[A-Z]{1,3})-(?P<postcode>[A-Z0-9- ]+)$"
).expect("failed to compile regex"));


#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoCoordinates {
    /// The latitude (north-south value) of the location described by this coordinate pair, in
    /// degrees. A valid value is at least -90.0 and at most 90.0. By convention, positive values
    /// denote latitudes north of the equator and negative values denote latitudes to its south.
    pub latitude_deg: f64,

    /// The longitude (west-east value) of the location described by this coordinate pair, in
    /// degrees. A valid value is at least -180.0 and at most 180.0. By convention, positive values
    /// denote longitudes east of meridian zero and negative values denote longitudes to its west.
    pub longitude_deg: f64,
}
impl GeoCoordinates {
    pub fn new(
        latitude_deg: f64,
        longitude_deg: f64,
    ) -> Self {
        Self {
            latitude_deg,
            longitude_deg,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoLocation {
    pub coordinates: GeoCoordinates,
    pub place: String,
}
impl GeoLocation {
    pub fn new(
        coordinates: GeoCoordinates,
        place: String,
    ) -> Self {
        Self {
            coordinates,
            place,
        }
    }
}


#[async_trait]
pub trait GeocodingProvider : Send + Sync {
    /// Creates a new instance of this geocoding provider.
    async fn new(config: &serde_json::Value, country_code_mapping: Arc<CountryCodeMapping>) -> Result<Self, &'static str> where Self : Sized;

    /// Attempts to convert a place name to its geographical coordinates.
    async fn geocode(&self, place: &str) -> Result<GeoLocation, GeocodingError>;

    /// Attempts to convert a place name to its geographical coordinates. Returns the coordinates
    /// as well as additional information in a provider-specific format.
    async fn geocode_advanced(&self, place: &str) -> Result<serde_json::Value, GeocodingError>;

    /// Attempts to convert a country code and country-specific post code to geographical
    /// coordinates.
    async fn geocode_postcode(&self, country_alpha2: &str, post_code: &str) -> Result<GeoLocation, GeocodingError>;

    /// Attempts to convert a country code and country-specific post code to geographical
    /// coordinates. Returns the coordinates as well as additional information in a
    /// provider-specific format.
    async fn geocode_postcode_advanced(&self, country_alpha2: &str, post_code: &str) -> Result<serde_json::Value, GeocodingError>;

    /// Attempts to convert a place's geographical coordinates to its name.
    async fn reverse_geocode(&self, coordinates: GeoCoordinates) -> Result<String, GeocodingError>;

    /// Attempts to convert a place's geographical coordinates to its name. Returns the coordinates
    /// as well as additional information in a provider-specific format.
    async fn reverse_geocode_advanced(&self, coordinates: GeoCoordinates) -> Result<serde_json::Value, GeocodingError>;

    /// Returns whether this geocoding provider supports geocoding timezones.
    async fn supports_timezones(&self) -> bool {
        false
    }

    /// Attempts to convert a place name to its timezone. Returns the IANA timezone ID. If the
    /// provider does not support timezones, returns `Err(GeocodingError::UnsupportedFeature)`.
    async fn reverse_geocode_timezone(&self, _coordinates: GeoCoordinates) -> Result<String, GeocodingError> {
        Err(GeocodingError::UnsupportedFeature)
    }
}


#[derive(Debug)]
pub enum GeocodingError {
    Http(String, reqwest::Error),
    ResponseCode(String, reqwest::Response),
    Bytes(String, reqwest::Error),
    JsonParsing(String, serde_json::Error),
    JsonSerialization(serde_json::Error),
    NotPostCode,
    OpeningFile(io::Error),
    InvalidCountryCode,
    NoResult,
    ReadingFile(io::Error),
    CountryCodeParsing(serde_json::Error),
    CountryCodesNotList,
    ConstructingUrl(url::ParseError),
    MissingAddressInfo,
    UnsupportedFeature,
}
impl fmt::Display for GeocodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeocodingError::Http(uri, e)
                => write!(f, "error requesting {}: {}", uri, e),
            GeocodingError::ResponseCode(uri, resp)
                => write!(f, "HTTP request to {} returned status code {}", uri, resp.status()),
            GeocodingError::Bytes(uri, e)
                => write!(f, "failed to convert response of {} to bytes: {}", uri, e),
            GeocodingError::JsonParsing(uri, e)
                => write!(f, "failed to parse response of {} as JSON: {}", uri, e),
            GeocodingError::JsonSerialization(e)
                => write!(f, "failed to serialize object as JSON: {}", e),
            GeocodingError::NotPostCode
                => write!(f, "invalid post code"),
            GeocodingError::OpeningFile(e)
                => write!(f, "error opening file: {}", e),
            GeocodingError::InvalidCountryCode
                => write!(f, "invalid country code"),
            GeocodingError::NoResult
                => write!(f, "no result found"),
            GeocodingError::ReadingFile(e)
                => write!(f, "error reading file: {}", e),
            GeocodingError::CountryCodeParsing(e)
                => write!(f, "error parsing country code file: {}", e),
            GeocodingError::CountryCodesNotList
                => write!(f, "country code structure is not a list"),
            GeocodingError::ConstructingUrl(e)
                => write!(f, "error constructing URL: {}", e),
            GeocodingError::MissingAddressInfo
                => write!(f, "address information is missing"),
            GeocodingError::UnsupportedFeature
                => write!(f, "feature not supported by this geocoding provider"),
        }
    }
}
impl std::error::Error for GeocodingError {
}

pub struct Geocoder {
    providers: Vec<Box<dyn GeocodingProvider>>,
    country_code_mapping: Arc<CountryCodeMapping>,
    #[cfg(feature = "confusion")]
    confuser: Confuser,
}
impl Geocoder {
    pub async fn new(config: &serde_json::Value) -> Result<Self, &'static str> {
        // load country codes
        let country_code_mapping_inner = CountryCodeMapping::load_from_file(Path::new("CountryCodes.json"))
            .or_msg("failed to load country code mappings")?;
        let country_code_mapping = Arc::new(country_code_mapping_inner);

        let mut providers = Vec::new();
        let provider_objects = config["providers"]
            .as_array().ok_or("providers is not a list")?;
        for provider_object in provider_objects {
            let name = provider_object["name"]
                .as_str().ok_or("name of provider entry is not a string")?;
            let config = &provider_object["config"];

            let provider: Box<dyn GeocodingProvider> = if name == "geonames" {
                Box::new(crate::providers::geonames::GeonamesGeocodingProvider::new(config, Arc::clone(&country_code_mapping)).await?)
            } else if name == "nominatim" {
                Box::new(crate::providers::nominatim::NominatimGeocodingProvider::new(config, Arc::clone(&country_code_mapping)).await?)
            } else {
                error!("unknown geocoding provider {:?}", name);
                return Err("unknown geocoding provider");
            };

            providers.push(provider);
        }

        Self::finish_config(
            config,
            providers,
            country_code_mapping,
        )
    }

    #[cfg(feature = "confusion")]
    fn finish_config(
        config: &serde_json::Value,
        providers: Vec<Box<dyn GeocodingProvider>>,
        country_code_mapping: Arc<CountryCodeMapping>,
    ) -> Result<Self, &'static str> {
        let confuser = Confuser::new(config)?;
        Ok(Self {
            providers,
            country_code_mapping,
            confuser,
        })
    }

    #[cfg(not(feature = "confusion"))]
    fn finish_config(
        _config: &serde_json::Value,
        providers: Vec<Box<dyn GeocodingProvider>>,
        country_code_mapping: Arc<CountryCodeMapping>,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            providers,
            country_code_mapping,
        })
    }

    #[cfg(feature = "confusion")]
    fn confuse(&self, place: &str) -> String {
        self.confuser.confuse(place)
    }

    #[cfg(not(feature = "confusion"))]
    fn confuse(&self, place: &str) -> String {
        place.to_owned()
    }

    fn country_code_to_alpha2(&self, code_to_find: &str) -> Option<String> {
        if self.country_code_mapping.alpha2.contains(code_to_find) {
            return Some(code_to_find.to_owned());
        }

        if let Some(a2) = self.country_code_mapping.alpha3_to_alpha2.get(code_to_find) {
            return Some(a2.clone());
        }

        if let Some(a2) = self.country_code_mapping.license_plate_to_alpha2.get(code_to_find) {
            return Some(a2.clone());
        }

        None
    }

    pub async fn geocode(&self, place: &str) -> Result<GeoLocation, Vec<GeocodingError>> {
        if let Some(caps) = POST_CODE_REGEX.captures(place) {
            // try geocoding as postcode first
            let country = caps
                .name("country").expect("country not matched")
                .as_str();
            let postcode = caps
                .name("postcode").expect("postcode not matched")
                .as_str();

            if let Some(alpha2) = self.country_code_to_alpha2(country) {
                for provider in &self.providers {
                    if let Ok(res) = provider.geocode_postcode(&alpha2, postcode).await {
                        return Ok(res);
                    }
                    // ignore errors; they will show up during regular geocoding
                }
            }
        }

        let confused_place = self.confuse(place);

        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.geocode(&confused_place).await {
                Ok(res) => return Ok(res),
                Err(e) => errors.push(e),
            }
        }
        Err(errors)
    }

    pub async fn geocode_advanced(&self, place: &str) -> Vec<Result<serde_json::Value, GeocodingError>> {
        let mut results = Vec::new();

        if let Some(caps) = POST_CODE_REGEX.captures(place) {
            let country = caps
                .name("country").expect("country not matched")
                .as_str();
            let postcode = caps
                .name("postcode").expect("postcode not matched")
                .as_str();

            if let Some(alpha2) = self.country_code_to_alpha2(country) {
                for provider in &self.providers {
                    results.push(provider.geocode_postcode_advanced(&alpha2, postcode).await);
                }
            }
        }

        let confused_place = self.confuse(place);

        for provider in &self.providers {
            results.push(provider.geocode_advanced(&confused_place).await);
        }

        results
    }

    pub async fn reverse_geocode(&self, location: GeoCoordinates) -> Result<String, Vec<GeocodingError>> {
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.reverse_geocode(location).await {
                Ok(res) => return Ok(res),
                Err(e) => errors.push(e),
            }
        }
        Err(errors)
    }

    pub async fn reverse_geocode_advanced(&self, location: GeoCoordinates) -> Vec<Result<serde_json::Value, GeocodingError>> {
        let mut results = Vec::new();
        for provider in &self.providers {
            results.push(provider.reverse_geocode_advanced(location).await);
        }
        results
    }

    pub async fn supports_timezones(&self) -> bool {
        for provider in &self.providers {
            if provider.supports_timezones().await {
                return true;
            }
        }
        false
    }

    pub async fn reverse_geocode_timezone(&self, location: GeoCoordinates) -> Result<String, Vec<GeocodingError>> {
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.reverse_geocode_timezone(location).await {
                Ok(res) => return Ok(res),
                Err(e) => errors.push(e),
            }
        }
        Err(errors)
    }
}
