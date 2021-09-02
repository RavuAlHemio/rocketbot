#[cfg(feature = "confusion")]
mod confusion;


use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::num::ParseFloatError;
use std::path::Path;

use bytes::Buf;
use chrono::{DateTime, TimeZone, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::IntoUrl;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::sync::Mutex;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json;
use url::Url;


static POST_CODE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    "^(?P<country>[A-Z]{1,3})-(?P<postcode>[A-Z0-9- ]+)$"
).expect("failed to compile regex"));

static DATE_TIME_FORMAT: &'static str = "%Y-%m-%d %H:%M";


#[derive(Debug)]
pub enum GeoNamesError {
    Http(String, reqwest::Error),
    ResponseCode(String, reqwest::Response),
    Bytes(String, reqwest::Error),
    JsonParsing(String, serde_json::Error),
    NotPostCode,
    OpeningFile(io::Error),
    InvalidCountryCode,
    NoResult,
    ReadingFile(io::Error),
    CountryCodeParsing(serde_json::Error),
    CountryCodesNotList,
}
impl fmt::Display for GeoNamesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeoNamesError::Http(uri, e)
                => write!(f, "error requesting {}: {}", uri, e),
            GeoNamesError::ResponseCode(uri, resp)
                => write!(f, "HTTP request to {} returned status code {}", uri, resp.status()),
            GeoNamesError::Bytes(uri, e)
                => write!(f, "failed to convert response of {} to bytes: {}", uri, e),
            GeoNamesError::JsonParsing(uri, e)
                => write!(f, "failed to parse response of {} as JSON: {}", uri, e),
            GeoNamesError::NotPostCode
                => write!(f, "invalid post code"),
            GeoNamesError::OpeningFile(e)
                => write!(f, "error opening file: {}", e),
            GeoNamesError::InvalidCountryCode
                => write!(f, "invalid country code"),
            GeoNamesError::NoResult
                => write!(f, "no result found"),
            GeoNamesError::ReadingFile(e)
                => write!(f, "error reading file: {}", e),
            GeoNamesError::CountryCodeParsing(e)
                => write!(f, "error parsing country code file: {}", e),
            GeoNamesError::CountryCodesNotList
                => write!(f, "country code structure is not a list"),
        }
    }
}
impl std::error::Error for GeoNamesError {
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoName {
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

    #[serde(rename = "lat")]
    pub latitude_string: String,

    #[serde(rename = "lng")]
    pub longitude_string: String,

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

    pub fn latitude(&self) -> Result<f64, ParseFloatError> {
        self.latitude_string.parse()
    }

    pub fn longitude(&self) -> Result<f64, ParseFloatError> {
        self.longitude_string.parse()
    }
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoSearchResponse {
    #[serde(rename = "geonames")]
    pub geo_names: Vec<GeoName>,
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeoTimeZoneResponse {
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

    #[serde(rename = "sunrise")]
    pub sunrise_string: String,

    #[serde(rename = "sunset")]
    pub sunset_string: String,

    #[serde(rename = "time")]
    pub time_string: String,

    #[serde(rename = "timezoneId")]
    pub timezone_id: String,
}
impl GeoTimeZoneResponse {
    pub fn sunrise(&self) -> DateTime<Utc> {
        Utc.datetime_from_str(&self.sunrise_string, DATE_TIME_FORMAT)
            .expect("parsing failed")
    }

    pub fn set_sunrise(&mut self, sunrise: &DateTime<Utc>) {
        self.sunrise_string = sunrise.format(DATE_TIME_FORMAT).to_string();
    }

    pub fn sunset(&self) -> DateTime<Utc> {
        Utc.datetime_from_str(&self.sunset_string, DATE_TIME_FORMAT)
            .expect("parsing failed")
    }

    pub fn set_sunset(&mut self, sunset: &DateTime<Utc>) {
        self.sunset_string = sunset.format(DATE_TIME_FORMAT).to_string();
    }

    pub fn time(&self) -> DateTime<Utc> {
        Utc.datetime_from_str(&self.time_string, DATE_TIME_FORMAT)
            .expect("parsing failed")
    }

    pub fn set_time(&mut self, time: &DateTime<Utc>) {
        self.time_string = time.format(DATE_TIME_FORMAT).to_string();
    }
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PostCodeSearchResponse {
    #[serde(rename = "postalCodes")]
    pub post_code_entries: Vec<GeoName>,
}


#[derive(Clone, Debug, PartialEq)]
pub struct CountryCodeMapping {
    alpha2: HashSet<String>,
    license_plate_to_alpha2: HashMap<String, String>,
    alpha3_to_alpha2: HashMap<String, String>,
}
impl CountryCodeMapping {
    pub fn load_from_file(path: &Path) -> Result<CountryCodeMapping, GeoNamesError> {
        let mut file = File::open(path)
            .map_err(|e| GeoNamesError::OpeningFile(e))?;

        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|e| GeoNamesError::OpeningFile(e))?;

        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| GeoNamesError::CountryCodeParsing(e))?;

        let mut alpha2 = HashSet::new();
        let mut license_plate_to_alpha2 = HashMap::new();
        let mut alpha3_to_alpha2 = HashMap::new();

        for item in json.members().ok_or(GeoNamesError::CountryCodesNotList)? {
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


pub struct GeoNamesClient {
    username: String,
    http_client: Mutex<reqwest::Client>,
    country_codes: CountryCodeMapping,

    #[cfg(feature = "confusion")]
    confuser: crate::confusion::Confuser,
}
impl GeoNamesClient {
    pub fn new(config: &serde_json::Value) -> GeoNamesClient {
        let username = config["username"]
            .as_str().expect("username missing or not representable as string")
            .to_owned();
        let http_client = Mutex::new(
            "GeoNamesClient::http_client",
            reqwest::Client::new(),
        );
        let country_codes = CountryCodeMapping::load_from_file(Path::new("CountryCodes.json"))
            .expect("failed to load country code mappings");

        Self::finish_config(username, http_client, country_codes, config)
    }

    #[cfg(feature = "confusion")]
    fn finish_config(
        username: String,
        http_client: Mutex<reqwest::Client>,
        country_codes: CountryCodeMapping,
        config: &serde_json::Value,
    ) -> Self {
        let confuser = crate::confusion::Confuser::new(config);
        Self {
            username,
            http_client,
            country_codes,
            confuser,
        }
    }

    #[cfg(not(feature = "confusion"))]
    fn finish_config(
        username: String,
        http_client: Mutex<reqwest::Client>,
        country_codes: CountryCodeMapping,
        _config: &serde_json::Value,
    ) -> Self {
        Self {
            username,
            http_client,
            country_codes,
        }
    }

    #[cfg(feature = "confusion")]
    fn confuse_location(&self, location: &str) -> String {
        self.confuser.confuse(location)
    }

    #[cfg(not(feature = "confusion"))]
    fn confuse_location(&self, location: &str) -> String {
        location.to_owned()
    }

    async fn get_and_populate_json<T: DeserializeOwned, U: Clone + fmt::Display + IntoUrl>(&self, uri: U) -> Result<T, GeoNamesError> {
        let client_guard = self.http_client
            .lock().await;
        let response = client_guard
            .get(uri.clone())
            .send().await.map_err(|e| GeoNamesError::Http(uri.to_string(), e))?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(GeoNamesError::ResponseCode(uri.to_string(), response));
        }

        let bytes = response
            .bytes().await.map_err(|e| GeoNamesError::Bytes(uri.to_string(), e))?;
        let bytes_reader = bytes.reader();
        let deserialized: T = serde_json::from_reader(bytes_reader)
            .map_err(|e| GeoNamesError::JsonParsing(uri.to_string(), e))?;

        Ok(deserialized)
    }

    pub async fn search_for_location(&self, location: &str) -> Result<GeoSearchResponse, GeoNamesError> {
        let actual_location = self.confuse_location(location);

        let mut url = Url::parse("http://api.geonames.org/searchJSON?maxRows=1")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("q", &actual_location)
            .append_pair("username", &self.username);

        self.get_and_populate_json(url).await
    }

    pub async fn get_timezone(&self, latitude: f64, longitude: f64) -> Result<GeoTimeZoneResponse, GeoNamesError> {
        let mut url = Url::parse("http://api.geonames.org/timezoneJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("lat", &latitude.to_string())
            .append_pair("lng", &longitude.to_string())
            .append_pair("username", &self.username);

        self.get_and_populate_json(url).await
    }

    pub async fn reverse_geocode(&self, latitude: f64, longitude: f64) -> Result<GeoSearchResponse, GeoNamesError> {
        let mut url = Url::parse("http://api.geonames.org/findNearbyJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("lat", &latitude.to_string())
            .append_pair("lng", &longitude.to_string())
            .append_pair("fclass", "P")
            .append_pair("fcode", "PPLA")
            .append_pair("fcode", "PPL")
            .append_pair("fcode", "PPLC")
            .append_pair("username", &self.username);

        self.get_and_populate_json(url).await
    }

    async fn country_code_to_alpha2(&self, country_code: &str) -> Option<String> {
        if self.country_codes.alpha2.contains(country_code) {
            Some(country_code.to_owned())
        } else if let Some(alpha2) = self.country_codes.alpha3_to_alpha2.get(country_code) {
            Some(alpha2.to_owned())
        } else if let Some(alpha2) = self.country_codes.license_plate_to_alpha2.get(country_code) {
            Some(alpha2.to_owned())
        } else {
            None
        }
    }

    pub async fn search_for_post_code(&self, post_code_string: &str) -> Result<PostCodeSearchResponse, GeoNamesError> {
        let post_code_match = match POST_CODE_REGEX.captures(post_code_string) {
            Some(c) => c,
            None => return Err(GeoNamesError::NotPostCode),
        };

        let country = post_code_match.name("country")
            .expect("failed to capture country")
            .as_str();
        let country_alpha2 = self.country_code_to_alpha2(&country).await
            .ok_or(GeoNamesError::InvalidCountryCode)?;

        let post_code = post_code_match.name("postcode")
            .expect("failed to capture postcode")
            .as_str().to_owned();

        let mut url = Url::parse("http://api.geonames.org/postalCodeSearchJSON")
            .expect("parsing URL failed");
        url.query_pairs_mut()
            .append_pair("postalcode", &post_code)
            .append_pair("country", &country_alpha2)
            .append_pair("maxRows", "1")
            .append_pair("username", &self.username);

        self.get_and_populate_json(url).await
    }

    pub async fn get_first_geo_name(&self, query: &str) -> Result<GeoName, GeoNamesError> {
        if let Ok(post_code_response) = self.search_for_post_code(query).await {
            return if let Some(pce) = post_code_response.post_code_entries.get(0) {
                Ok(pce.clone())
            } else {
                Err(GeoNamesError::NoResult)
            };
        }

        let response = self.search_for_location(query).await?;
        if let Some(gn) = response.geo_names.get(0) {
            Ok(gn.clone())
        } else {
            Err(GeoNamesError::NoResult)
        }
    }

    pub async fn get_first_reverse_geo(&self, latitude: f64, longitude: f64) -> Result<String, GeoNamesError> {
        let response = self.reverse_geocode(latitude, longitude).await?;

        if let Some(name) = response.geo_names.get(0) {
            Ok(name.name_and_country_name())
        } else {
            Err(GeoNamesError::NoResult)
        }
    }
}
