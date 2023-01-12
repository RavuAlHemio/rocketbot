use std::collections::HashMap;
use std::io::{Cursor, Read};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use flate2::Compression;
use flate2::read::{ZlibEncoder, ZlibDecoder};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error;


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PdfDescription {
    pub title: String,
    pub pages: Vec<PdfPageDescription>,
    pub fonts: HashMap<String, PdfBinaryDataDescription>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PdfPageDescription {
    pub width_mm: f64,
    pub height_mm: f64,
    pub elements: Vec<PdfElementDescription>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PdfElementDescription {
    Path(PdfPathDescription),
    Text(PdfTextDescription),
    Image(PdfImageDescription),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PdfPathDescription {
    pub stroke: Option<PdfColorDescription>,
    pub stroke_width: Option<f64>,
    pub fill: Option<PdfColorDescription>,
    pub close: bool,
    pub commands_mm: Vec<PdfPathCommandDescription>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PdfColorDescription {
    Rgb { red: f64, green: f64, blue: f64 },
    Cmyk { cyan: f64, magenta: f64, yellow: f64, black: f64 },
    Grayscale { white: f64 },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum PdfPathCommandDescription {
    #[serde(rename = "m")]
    MoveTo { x: f64, y: f64 },

    #[serde(rename = "l")]
    LineTo { x: f64, y: f64 },

    #[serde(rename = "c")]
    CubicBezierTo { c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64 },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignmentDescription {
    Left,
    Center,
    Right,
}
impl Default for TextAlignmentDescription {
    fn default() -> Self {
        Self::Left
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PdfTextDescription {
    pub x: f64,
    pub y: f64,
    pub font: String,
    pub size_pt: f64,
    pub text: String,
    #[serde(default)]
    pub alignment: TextAlignmentDescription,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PdfImageDescription {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub mime_type: String,
    pub data: PdfBinaryDataDescription,
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PdfBinaryDataDescription(pub Vec<u8>);
impl PdfBinaryDataDescription {
}
impl Serialize for PdfBinaryDataDescription {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let zlibbed = {
            let cursor = Cursor::new(&self.0);
            let mut zlibber = ZlibEncoder::new(cursor, Compression::best());
            let mut zlibbed = Vec::new();
            zlibber.read_to_end(&mut zlibbed).expect("failed to ZLIB-compress");
            zlibbed
        };

        let b64 = BASE64_STANDARD.encode(&zlibbed);
        b64.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for PdfBinaryDataDescription {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let b64 = String::deserialize(deserializer)?;
        let zlibbed = BASE64_STANDARD.decode(b64)
            .map_err(|e| D::Error::custom(e))?;

        let bs = {
            let cursor = Cursor::new(zlibbed);
            let mut dezlibber = ZlibDecoder::new(cursor);
            let mut bs = Vec::new();
            dezlibber.read_to_end(&mut bs)
                .map_err(|e| D::Error::custom(e))?;
            bs
        };

        Ok(PdfBinaryDataDescription(bs))
    }
}
