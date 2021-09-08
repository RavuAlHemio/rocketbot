use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Buf;
use log::debug;
use reqwest::{self, IntoUrl};
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::sync::Mutex;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json;
use url::Url;

use crate::{GeoCoordinates, GeocodingError, GeocodingProvider, GeoLocation};
use crate::country_codes::CountryCodeMapping;


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct NominatimPlace {
    pub place_id: u64,

    pub licence: String,

    pub osm_type: Option<String>,

    pub osm_id: Option<u64>,

    #[serde(rename = "boundingbox")]
    pub bounding_box: Vec<String>,

    #[serde(with = "crate::s11n::serde_f64_as_string")]
    pub lat: f64,

    #[serde(with = "crate::s11n::serde_f64_as_string")]
    pub lon: f64,

    pub display_name: String,

    pub category: String,

    #[serde(rename = "type")]
    pub place_type: String,

    pub importance: f64,

    pub icon: Option<String>,

    pub address: Option<NominatimAddress>,

    #[serde(rename = "extratags")]
    pub extra_tags: Option<HashMap<String, String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct NominatimAddress {
    pub postcode: Option<String>,

    pub house_number: Option<String>,
    pub house_name: Option<String>,

    pub road: Option<String>,

    pub city_block: Option<String>,
    pub residential: Option<String>,
    pub farm: Option<String>,
    pub farmyard: Option<String>,
    pub industrial: Option<String>,
    pub commercial: Option<String>,
    pub retail: Option<String>,

    pub neighbourhood: Option<String>,
    pub allotments: Option<String>,
    pub quarter: Option<String>,

    pub hamlet: Option<String>,
    pub croft: Option<String>,
    pub isolated_dwelling: Option<String>,

    pub city_district: Option<String>,
    pub district: Option<String>,
    pub borough: Option<String>,
    pub suburb: Option<String>,
    pub subdivision: Option<String>,

    pub municipality: Option<String>,
    pub city: Option<String>,
    pub town: Option<String>,
    pub village: Option<String>,

    pub region: Option<String>,
    pub state: Option<String>,
    pub state_district: Option<String>,
    pub county: Option<String>,

    pub country: Option<String>,
    pub country_code: Option<String>,

    pub continent: Option<String>,
}
impl NominatimAddress {
    pub fn name_and_country_name(&self) -> Option<String> {
        let town = self.municipality.as_ref()
            .or(self.city.as_ref())
            .or(self.town.as_ref())
            .or(self.village.as_ref())?;
        let country = self.country.as_ref()?;
        Some(format!("{}, {}", town, country))
    }
}


pub struct NominatimGeocodingProvider {
    base_url: Url,
    user_agent: String,
    additional_headers: HashMap<String, String>,
    http_client: Mutex<reqwest::Client>,
}
impl NominatimGeocodingProvider {
    fn get_url(&self, verb: &str) -> Result<Url, GeocodingError> {
        let mut url = self.base_url
            .join(verb).map_err(|e| GeocodingError::ConstructingUrl(e))?;

        // shunt over query parameters from base URL (might be API keys etc.)
        url.set_query(self.base_url.query());

        Ok(url)
    }

    async fn get_and_populate_json<T: DeserializeOwned, U: Clone + fmt::Display + IntoUrl>(&self, uri: U) -> Result<T, GeocodingError> {
        let client_guard = self.http_client
            .lock().await;
        let mut builder = client_guard
            .get(uri.clone())
            .header("User-Agent", &self.user_agent);
        for (header_key, header_value) in &self.additional_headers {
            builder = builder
                .header(header_key, header_value);
        }
        let response = builder
            .send().await.map_err(|e| {
                debug!("Nominatim request to {} failed: {}", uri, e);
                GeocodingError::Http(uri.to_string(), e)
            })?;
        if response.status() != reqwest::StatusCode::OK {
            debug!("Nominatim request to {} returned status code {}", uri, response.status());
            return Err(GeocodingError::ResponseCode(uri.to_string(), response));
        }

        let bytes = response
            .bytes().await.map_err(|e| {
                debug!("failed to convert Nominatim response from {} to bytes: {}", uri, e);
                GeocodingError::Bytes(uri.to_string(), e)
            })?;
        let bytes_reader = bytes.reader();
        let deserialized: T = serde_json::from_reader(bytes_reader)
            .map_err(|e| {
                debug!("failed to deserialize Nominatim response from {} to JSON: {}", uri, e);
                GeocodingError::JsonParsing(uri.to_string(), e)
            })?;

        Ok(deserialized)
    }
}
#[async_trait]
impl GeocodingProvider for NominatimGeocodingProvider {
    async fn new(config: &serde_json::Value, _country_code_mapping: Arc<CountryCodeMapping>) -> Self {
        let base_url = if config["base_url"].is_null() {
            Url::parse("https://nominatim.openstreetmap.org/")
                .expect("failed to parse default Nominatim URL")
        } else {
            config["base_url"]
                .as_str().expect("base_url missing or not a string")
                .parse().expect("failed to parse base_url as a string")
        };
        let user_agent: String = config["user_agent"]
            .as_str().expect("user_agent missing or not a string")
            .to_owned();
        let additional_headers = if config["additional_headers"].is_null() {
            HashMap::new()
        } else {
            let mut headers = HashMap::new();
            let header_map = config["additional_headers"].entries()
                .expect("additional_headers not a map");
            for (key, value) in header_map {
                let value_str = value
                    .as_str().expect("additional_headers value not a string");
                headers.insert(key.clone(), value_str.to_owned());
            }
            headers
        };
        let http_client = Mutex::new(
            "NominatimGeocodingProvider::http_client",
            reqwest::Client::new(),
        );

        Self {
            base_url,
            user_agent,
            additional_headers,
            http_client,
        }
    }

    async fn geocode(&self, place: &str) -> Result<GeoLocation, GeocodingError> {
        let mut url = self.get_url("search")?;

        {
            url.query_pairs_mut()
                .append_pair("format", "jsonv2")
                .append_pair("q", place);
        }

        let place_obj: NominatimPlace = self.get_and_populate_json(url).await?;
        debug!("nominatim geocode({:?}) -> {:?}", place, place_obj);
        if let Some(addr) = place_obj.address.and_then(|a| a.name_and_country_name()) {
            Ok(GeoLocation::new(
                GeoCoordinates::new(place_obj.lat, place_obj.lon),
                addr,
            ))
        } else {
            Err(GeocodingError::MissingAddressInfo)
        }
    }

    async fn geocode_advanced(&self, place: &str) -> Result<serde_json::Value, GeocodingError> {
        let mut url = self.get_url("search")?;

        {
            url.query_pairs_mut()
                .append_pair("format", "jsonv2")
                .append_pair("q", place);
        }

        let place_obj: NominatimPlace = self.get_and_populate_json(url).await?;
        debug!("nominatim geocode_advanced({:?}) -> {:?}", place, place_obj);
        serde_json::to_value(place_obj)
            .map_err(|e| GeocodingError::JsonSerialization(e))
    }

    async fn geocode_postcode(&self, country_alpha2: &str, post_code: &str) -> Result<GeoLocation, GeocodingError> {
        let mut url = self.get_url("search")?;

        {
            url.query_pairs_mut()
                .append_pair("format", "jsonv2")
                .append_pair("country", country_alpha2)
                .append_pair("postalcode", post_code);
        }

        let place: NominatimPlace = self.get_and_populate_json(url).await?;
        debug!("nominatim geocode_postcode({:?}, {:?}) -> {:?}", country_alpha2, post_code, place);
        if let Some(addr) = place.address.and_then(|a| a.name_and_country_name()) {
            Ok(GeoLocation::new(
                GeoCoordinates::new(place.lat, place.lon),
                addr,
            ))
        } else {
            Err(GeocodingError::MissingAddressInfo)
        }
    }

    async fn geocode_postcode_advanced(&self, country_alpha2: &str, post_code: &str) -> Result<serde_json::Value, GeocodingError> {
        let mut url = self.get_url("search")?;

        {
            url.query_pairs_mut()
                .append_pair("format", "jsonv2")
                .append_pair("country", country_alpha2)
                .append_pair("postalcode", post_code);
        }

        let place: NominatimPlace = self.get_and_populate_json(url).await?;
        debug!("nominatim geocode_postcode({:?}, {:?}) -> {:?}", country_alpha2, post_code, place);
        serde_json::to_value(place)
            .map_err(|e| GeocodingError::JsonSerialization(e))
    }

    async fn reverse_geocode(&self, coordinates: GeoCoordinates) -> Result<String, GeocodingError> {
        let mut url = self.get_url("reverse")?;

        {
            url.query_pairs_mut()
                .append_pair("format", "jsonv2")
                .append_pair("lat", &coordinates.latitude_deg.to_string())
                .append_pair("lon", &coordinates.longitude_deg.to_string())
                .append_pair("addressdetails", "1");
        }

        let place: NominatimPlace = self.get_and_populate_json(url).await?;
        debug!("nominatim reverse_geocode({:?}) -> {:?}", coordinates, place);
        if let Some(addr) = place.address.and_then(|a| a.name_and_country_name()) {
            Ok(addr)
        } else {
            Err(GeocodingError::NoResult)
        }
    }

    async fn reverse_geocode_advanced(&self, coordinates: GeoCoordinates) -> Result<serde_json::Value, GeocodingError> {
        let mut url = self.get_url("reverse")?;

        {
            url.query_pairs_mut()
                .append_pair("format", "jsonv2")
                .append_pair("lat", &coordinates.latitude_deg.to_string())
                .append_pair("lon", &coordinates.longitude_deg.to_string())
                .append_pair("addressdetails", "1");
        }

        let place: NominatimPlace = self.get_and_populate_json(url).await?;
        debug!("nominatim reverse_geocode_advanced({:?}) -> {:?}", coordinates, place);
        serde_json::to_value(place)
            .map_err(|e| GeocodingError::JsonSerialization(e))
    }
}
