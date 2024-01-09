use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Buf;
use chrono::{DateTime, Utc};
use log::debug;
use reqwest::{self, IntoUrl};
use rocketbot_interface::sync::Mutex;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use url::Url;

use crate::{GeocodingError, GeocodingProvider, GeoCoordinates, GeoLocation};
use crate::country_codes::CountryCodeMapping;


static DATE_TIME_FORMAT: &'static str = "%Y-%m-%d %H:%M";


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct GeoName {
    #[serde(rename = "adminCode1")]
    pub admin_code_1: String,

    #[serde(rename = "adminName1")]
    pub admin_name_1: String,

    #[serde(rename = "countryCode")]
    pub country_code: String,

    #[serde(rename = "countryId")]
    pub country_id: String,

    #[serde(rename = "countryName")]
    pub country_name: String,

    #[serde(rename = "fcl")]
    pub fcl: String,

    #[serde(rename = "fclName")]
    pub fcl_name: String,

    #[serde(rename = "fcode")]
    pub fcode: String,

    #[serde(rename = "fcodeName")]
    pub fcode_name: String,

    #[serde(rename = "geonameId")]
    pub geoname_id: u64,

    #[serde(rename = "lat", with = "crate::s11n::serde_f64_as_string")]
    pub latitude: f64,

    #[serde(rename = "lng", with = "crate::s11n::serde_f64_as_string")]
    pub longitude: f64,

    #[serde(rename = "name")]
    pub name: String,

    #[serde(rename = "population")]
    pub population: u64,

    #[serde(rename = "toponymName")]
    pub toponym_name: String,
}
impl GeoName {
    pub fn name_and_country_name(&self) -> String {
        format!("{}, {}", self.name, self.country_name)
    }
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct GeoSearchResponse {
    #[serde(rename = "geonames")]
    pub geo_names: Vec<GeoName>,
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct GeoTimeZoneResponse {
    #[serde(rename = "countryCode")]
    pub country_code: String,

    #[serde(rename = "countryName")]
    pub country_name: String,

    #[serde(rename = "dstOffset")]
    pub dst_offset: f64,

    #[serde(rename = "gmtOffset")]
    pub gmt_offset: f64,

    #[serde(rename = "lat")]
    pub latitude: f64,

    #[serde(rename = "lng")]
    pub longitude: f64,

    #[serde(rename = "rawOffset")]
    pub raw_offset: f64,

    #[serde(with = "serde_datetime")]
    pub sunrise: DateTime<Utc>,

    #[serde(with = "serde_datetime")]
    pub sunset: DateTime<Utc>,

    #[serde(with = "serde_datetime")]
    pub time: DateTime<Utc>,

    #[serde(rename = "timezoneId")]
    pub timezone_id: String,
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct PostCodeSearchResponse {
    #[serde(rename = "postalCodes")]
    pub post_code_entries: Vec<GeoName>,
}


pub struct GeonamesGeocodingProvider {
    username: String,
    http_client: Mutex<reqwest::Client>,
}
impl GeonamesGeocodingProvider {
    async fn get_and_populate_json<T: DeserializeOwned, U: Clone + fmt::Display + IntoUrl>(&self, uri: U) -> Result<T, GeocodingError> {
        let client_guard = self.http_client
            .lock().await;
        let response = client_guard
            .get(uri.clone())
            .send().await.map_err(|e| {
                debug!("Geonames request to {} failed: {}", uri, e);
                GeocodingError::Http(uri.to_string(), e)
            })?;
        if response.status() != reqwest::StatusCode::OK {
            debug!("Geonames request to {} returned status code {}", uri, response.status());
            return Err(GeocodingError::ResponseCode(uri.to_string(), response));
        }

        let bytes = response
            .bytes().await.map_err(|e| {
                debug!("failed to convert Geonames response from {} to bytes: {}", uri, e);
                GeocodingError::Bytes(uri.to_string(), e)
            })?;
        let bytes_reader = bytes.reader();
        let deserialized: T = serde_json::from_reader(bytes_reader)
            .map_err(|e| {
                debug!("failed to deserialize Geonames response from {} to JSON: {}", uri, e);
                GeocodingError::JsonParsing(uri.to_string(), e)
            })?;

        Ok(deserialized)
    }
}
#[async_trait]
impl GeocodingProvider for GeonamesGeocodingProvider {
    async fn new(config: &serde_json::Value, _country_code_mapping: Arc<CountryCodeMapping>) -> Result<Self, &'static str> {
        let username = config["username"]
            .as_str().ok_or("\"username\" not a string")?
            .to_owned();
        let http_client = Mutex::new(
            "GeonamesGeocodingProvider::http_client",
            reqwest::Client::new(),
        );

        Ok(GeonamesGeocodingProvider {
            username,
            http_client,
        })
    }

    async fn geocode(&self, place: &str) -> Result<GeoLocation, GeocodingError> {
        let mut url = Url::parse("http://api.geonames.org/searchJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("maxRows", "1")
            .append_pair("q", &place)
            .append_pair("username", &self.username);

        let search_response: GeoSearchResponse = self
            .get_and_populate_json(url).await?;
        search_response.geo_names
            .get(0)
            .map(|gn| GeoLocation::new(
                GeoCoordinates::new(gn.latitude, gn.longitude),
                gn.name_and_country_name(),
            ))
            .ok_or(GeocodingError::NoResult)
    }

    async fn geocode_advanced(&self, place: &str) -> Result<serde_json::Value, GeocodingError> {
        let mut url = Url::parse("http://api.geonames.org/searchJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("q", &place)
            .append_pair("username", &self.username);

        let search_response: GeoSearchResponse = self.get_and_populate_json(url).await?;
        serde_json::to_value(search_response)
            .map_err(|e| GeocodingError::JsonSerialization(e))
    }

    async fn geocode_postcode(&self, country_alpha2: &str, post_code: &str) -> Result<GeoLocation, GeocodingError> {
        let mut url = Url::parse("http://api.geonames.org/postalCodeSearchJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("maxRows", "1")
            .append_pair("postalcode", &post_code)
            .append_pair("country", &country_alpha2)
            .append_pair("username", &self.username);

        let search_response: PostCodeSearchResponse = self.get_and_populate_json(url).await?;
        search_response.post_code_entries
            .get(0)
            .map(|gn| GeoLocation::new(
                GeoCoordinates::new(gn.latitude, gn.longitude),
                gn.name_and_country_name(),
            ))
            .ok_or(GeocodingError::NoResult)
    }

    async fn geocode_postcode_advanced(&self, country_alpha2: &str, post_code: &str) -> Result<serde_json::Value, GeocodingError> {
        let mut url = Url::parse("http://api.geonames.org/postalCodeSearchJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("postalcode", &post_code)
            .append_pair("country", &country_alpha2)
            .append_pair("username", &self.username);

        let search_response: PostCodeSearchResponse = self.get_and_populate_json(url).await?;
        serde_json::to_value(search_response)
            .map_err(|e| GeocodingError::JsonSerialization(e))
    }

    async fn reverse_geocode(&self, coordinates: GeoCoordinates) -> Result<String, GeocodingError> {
        let mut url = Url::parse("http://api.geonames.org/findNearbyJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("maxRows", "1")
            .append_pair("lat", &coordinates.latitude_deg.to_string())
            .append_pair("lng", &coordinates.longitude_deg.to_string())
            .append_pair("fclass", "P")
            .append_pair("fcode", "PPLA")
            .append_pair("fcode", "PPL")
            .append_pair("fcode", "PPLC")
            .append_pair("username", &self.username);

        let search_response: GeoSearchResponse = self.get_and_populate_json(url).await?;

        if let Some(gn) = search_response.geo_names.get(0) {
            Ok(gn.name_and_country_name())
        } else {
            Err(GeocodingError::NoResult)
        }
    }

    async fn reverse_geocode_advanced(&self, coordinates: GeoCoordinates) -> Result<serde_json::Value, GeocodingError> {
        let mut url = Url::parse("http://api.geonames.org/findNearbyJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("lat", &coordinates.latitude_deg.to_string())
            .append_pair("lng", &coordinates.longitude_deg.to_string())
            .append_pair("fclass", "P")
            .append_pair("fcode", "PPLA")
            .append_pair("fcode", "PPL")
            .append_pair("fcode", "PPLC")
            .append_pair("username", &self.username);

        let search_response: GeoSearchResponse = self.get_and_populate_json(url).await?;
        serde_json::to_value(search_response)
            .map_err(|e| GeocodingError::JsonSerialization(e))
    }

    async fn supports_timezones(&self) -> bool {
        true
    }

    async fn reverse_geocode_timezone(&self, coordinates: GeoCoordinates) -> Result<String, GeocodingError> {
        let mut url = Url::parse("http://api.geonames.org/timezoneJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("lat", &coordinates.latitude_deg.to_string())
            .append_pair("lng", &coordinates.longitude_deg.to_string())
            .append_pair("username", &self.username);

        let timezone_response: GeoTimeZoneResponse = self.get_and_populate_json(url).await?;
        Ok(timezone_response.timezone_id)
    }
}

mod serde_datetime {
    use chrono::{DateTime, NaiveDateTime, Utc};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::de::Error;

    pub(crate) fn serialize<S: Serializer>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
        value.format(super::DATE_TIME_FORMAT)
            .to_string()
            .serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
        let date_time_string = String::deserialize(deserializer)?;
        NaiveDateTime::parse_from_str(&date_time_string, super::DATE_TIME_FORMAT)
            .map(|ndt| ndt.and_utc())
            .map_err(|_| D::Error::custom("parsing failed"))
    }
}
