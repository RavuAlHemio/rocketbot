mod xlsx;


use std::collections::{BTreeMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::str::FromStr;

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error;
use serde_json;


#[derive(Clone, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
enum ColumnSpec {
    Index(usize),
    Name(String),
}
impl ColumnSpec {
    pub fn matches_cell(&self, column_index: usize, column_name: &str) -> bool {
        match self {
            Self::Index(i) => *i == column_index,
            Self::Name(n) => n == column_name,
        }
    }
}
impl<'de> Deserialize<'de> for ColumnSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let string_repr = String::deserialize(deserializer)?;
        if let Some(rest) = string_repr.strip_prefix("#") {
            let index = rest.parse()
                .map_err(|_| D::Error::custom(format_args!("invalid usize (column index) following #")))?;
            Ok(Self::Index(index))
        } else if let Some(rest) = string_repr.strip_prefix("$") {
            Ok(Self::Name(String::from(rest)))
        } else {
            Err(D::Error::custom(format_args!("value is prefixed by neither '#' nor '$'")))
        }
    }
}
impl Serialize for ColumnSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let string = match self {
            Self::Index(i) => format!("#{}", i),
            Self::Name(s) => format!("${}", s),
        };
        string.serialize(serializer)
    }
}


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl RgbaColor {
    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    #[inline]
    pub const fn none() -> Self {
        Self::rgba(0, 0, 0, 0)
    }

    #[inline]
    pub const fn gray125() -> Self {
        // 0.125 * 255 =~ 32
        Self::rgb(32, 32, 32)
    }
}
impl fmt::Display for RgbaColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b)
    }
}
impl FromStr for RgbaColor {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 8 {
            return Err("wrong length");
        }
        if !s.chars().all(|c| (c >= '0' && c <= '9') || (c >= 'A' && c <= 'F') || (c >= 'a' && c <= 'f')) {
            return Err("wrong character");
        }
        let argb = u32::from_str_radix(s, 16).unwrap();
        let a = ((argb >> 24) & 0xFF) as u8;
        let r = ((argb >> 16) & 0xFF) as u8;
        let g = ((argb >>  8) & 0xFF) as u8;
        let b = ((argb >>  0) & 0xFF) as u8;
        Ok(Self::rgba(r, g, b, a))
    }
}
impl<'de> Deserialize<'de> for RgbaColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let string = String::deserialize(deserializer)?;
        string.parse().map_err(D::Error::custom)
    }
}
impl Serialize for RgbaColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}


#[derive(Clone, Debug, Deserialize, Serialize)]
struct CodeConversion {
    #[serde(with = "rocketbot_interface::serde::serde_regex")]
    pub code_extractor_regex: Regex,
    pub code_replacements: Vec<String>,
    pub vehicle_class: String,
    pub overridden_type: Option<String>,
    pub common_props: BTreeMap<String, String>,
}


#[derive(Clone, Debug, Deserialize, Serialize)]
struct Config {
    pub xlsx_url: String,
    pub output_path: String,

    #[serde(with = "rocketbot_interface::serde::serde_vec_regex")]
    pub ignore_sheet_names: Vec<Regex>,
    #[serde(with = "rocketbot_interface::serde::serde_vec_regex")]
    pub grouped_train_sheet_names: Vec<Regex>,

    pub state_color_column: ColumnSpec,
    pub type_column: ColumnSpec,
    pub code_column: ColumnSpec,
    pub grouped_type_column: ColumnSpec,
    pub grouped_code_column: ColumnSpec,
    #[serde(with = "rocketbot_interface::serde::serde_vec_regex")]
    pub ignore_column_names: Vec<Regex>,
    pub full_code_additional_field: Option<String>,
    pub original_type_additional_field: Option<String>,

    pub background_colors: HashSet<RgbaColor>,
    pub header_colors: HashSet<RgbaColor>,
    pub rebuilt_color: RgbaColor,
    pub out_of_service_color: RgbaColor,

    pub code_conversions: Vec<CodeConversion>,
}


#[tokio::main]
async fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() != 2 {
        panic!("Usage: obtain_bim_xlsx CONFIG");
    }

    let config: Config = {
        let mut f = File::open(&args[1])
            .expect("failed to open config file");
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .expect("failed to read config file");
        let string = String::from_utf8(buf)
            .expect("failed to decode config file");
        serde_json::from_str(&string)
            .expect("failed to parse config file")
    };

    // obtain XLSX
    let mut xlsx_bytes: Vec<u8> = Vec::new();
    if let Some(local_path) = config.xlsx_url.strip_prefix("file://") {
        let mut f = File::open(local_path)
            .expect("failed to open local .xlsx file");
        f.read_to_end(&mut xlsx_bytes)
            .expect("failed to read local .xlsx file");
    } else {
        let resp_bytes = reqwest::get(&config.xlsx_url)
            .await.expect("failed to perform GET request on .xlsx URL")
            .bytes().await.expect("failed to obtain response bytes");
        xlsx_bytes.extend(&resp_bytes);
    }

    let vehicles = crate::xlsx::extract_xlsx_vehicles(&config, &xlsx_bytes);

    {
        let output = File::create(&config.output_path)
            .expect("failed to open output file");
        serde_json::to_writer_pretty(output, &vehicles)
            .expect("failed to write output file");
    }
}
