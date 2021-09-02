use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::io::Cursor;
use std::sync::Weak;

use async_trait::async_trait;
use exif;
use hyper::StatusCode;
use log::error;
use num_rational::Rational64;
use once_cell::unsync::Lazy;
use rocketbot_geonames::GeoNamesClient;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


#[derive(Debug)]
struct InvalidDirection(char);
impl fmt::Display for InvalidDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid direction {:?}", self.0)
    }
}
impl std::error::Error for InvalidDirection {
}


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LatitudeDirection {
    North,
    South,
}
impl From<LatitudeDirection> for Rational64 {
    fn from(d: LatitudeDirection) -> Self {
        match d {
            LatitudeDirection::North => Rational64::new(1, 1),
            LatitudeDirection::South => Rational64::new(-1, 1),
        }
    }
}
impl TryFrom<char> for LatitudeDirection {
    type Error = InvalidDirection;

    fn try_from(value: char) -> Result<Self, Self::Error> {
        match value {
            'N' => Ok(Self::North),
            'S' => Ok(Self::South),
            other => Err(InvalidDirection(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LongitudeDirection {
    East,
    West,
}
impl From<LongitudeDirection> for Rational64 {
    fn from(d: LongitudeDirection) -> Self {
        match d {
            LongitudeDirection::East => Rational64::new(1, 1),
            LongitudeDirection::West => Rational64::new(-1, 1),
        }
    }
}
impl TryFrom<char> for LongitudeDirection {
    type Error = InvalidDirection;

    fn try_from(value: char) -> Result<Self, Self::Error> {
        match value {
            'E' => Ok(Self::East),
            'W' => Ok(Self::West),
            other => Err(InvalidDirection(other)),
        }
    }
}


fn to_rationals<T, F>(vals: &Vec<T>, transform: F) -> Vec<Rational64>
    where
        F: FnMut(&T) -> Rational64,
{
    vals.iter().map(transform).collect()
}


fn to_rationals_opt<T, F>(vals: &Vec<T>, mut transform: F) -> Option<Vec<Rational64>>
    where
        F: FnMut(&T) -> Option<Rational64>,
{
    let mut rats = Vec::with_capacity(vals.len());
    for val in vals {
        let rat = match transform(val) {
            Some(r) => r,
            None => return None,
        };
        rats.push(rat)
    }
    Some(rats)
}


fn decode_exif_gps_position(value: &exif::Value) -> Option<Rational64> {
    let rationals: Vec<Rational64> = match value {
        exif::Value::Byte(vals)
            => to_rationals(&vals, |b| Rational64::new((*b).into(), 1)),
        exif::Value::Double(vals)
            => to_rationals_opt(&vals, |f| Rational64::approximate_float(*f))?,
        exif::Value::Float(vals)
            => to_rationals_opt(&vals, |f| Rational64::approximate_float(*f as f64))?,
        exif::Value::Long(vals)
            => to_rationals(&vals, |l| Rational64::new((*l).into(), 1)),
        exif::Value::Rational(vals)
            => to_rationals(&vals, |r| Rational64::new(r.num.into(), r.denom.into())),
        exif::Value::SByte(vals)
            => to_rationals(&vals, |b| Rational64::new((*b).into(), 1)),
        exif::Value::SLong(vals)
            => to_rationals(&vals, |l| Rational64::new((*l).into(), 1)),
        exif::Value::SRational(vals)
            => to_rationals(&vals, |r| Rational64::new(r.num.into(), r.denom.into())),
        exif::Value::SShort(vals)
            => to_rationals(&vals, |s| Rational64::new((*s).into(), 1)),
        exif::Value::Short(vals)
            => to_rationals(&vals, |s| Rational64::new((*s).into(), 1)),
        exif::Value::Ascii(_) => return None,
        exif::Value::Undefined(_, _) => return None,
        exif::Value::Unknown(_, _, _) => return None,
    };
    match rationals.len() {
        0 => None,
        1 => {
            // (decimal) degree
            Some(rationals[0])
        },
        2 => {
            // degree and (decimal) minute
            let decimal_degree = rationals[0]
                + rationals[1] * Rational64::new(1, 60)
            ;
            Some(decimal_degree)
        },
        3 => {
            // degree, minute and (decimal) second
            let decimal_degree = rationals[0]
                + rationals[1] * Rational64::new(1, 60)
                + rationals[2] * Rational64::new(1, 60*60)
            ;
            Some(decimal_degree)
        },
        _ => None
    }
}

fn decode_exif_gps_reference<T, F>(value: &exif::Value, mut transform: F) -> Option<T>
    where
        F: FnMut(char) -> Option<T>,
{
    if let exif::Value::Ascii(v) = value {
        if v.len() != 1 {
            // need exactly one value
            return None;
        }
        if v[0].len() != 1 {
            // need exactly one character
            return None;
        }
        let char_ref = char::from_u32(v[0][0].into())?;
        transform(char_ref)
    } else {
        None
    }
}

fn get_location_from_values(gps_lat: &exif::Field, gps_lat_ref: &exif::Field, gps_lon: &exif::Field, gps_lon_ref: &exif::Field) -> Option<(Rational64, Rational64)> {
    // convertible to our values?
    let lat = decode_exif_gps_position(&gps_lat.value)?;
    let lat_ref: LatitudeDirection = decode_exif_gps_reference(&gps_lat_ref.value, |r| r.try_into().ok())?;
    let lon = decode_exif_gps_position(&gps_lon.value)?;
    let lon_ref: LongitudeDirection = decode_exif_gps_reference(&gps_lon_ref.value, |r| r.try_into().ok())?;

    // possibly minus
    let final_lat: Rational64 = lat * Rational64::from(lat_ref);
    let final_lon: Rational64 = lon * Rational64::from(lon_ref);

    Some((final_lat, final_lon))
}


pub struct ExifTellPlugin {
    interface: Weak<dyn RocketBotInterface>,
    max_image_bytes: usize,
    max_messages_per_image: Option<usize>,
    geonames_client: GeoNamesClient,
}
#[async_trait]
impl RocketBotPlugin for ExifTellPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let max_image_bytes = config["max_image_bytes"].as_usize()
            .expect("max_image_bytes not representable as a usize");
        let max_messages_per_image = if config["max_messages_per_image"].is_null() {
            None
        } else {
            Some(
                config["max_image_bytes"].as_usize()
                    .expect("max_image_bytes not representable as a usize")
            )
        };
        let geonames_client = GeoNamesClient::new(&config["geonames"]);

        Self {
            interface,
            max_image_bytes,
            max_messages_per_image,
            geonames_client,
        }
    }

    async fn plugin_name(&self) -> String {
        "exiftell".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        // lazy EXIF reader for when there is an attachment
        let exif_reader: Lazy<exif::Reader> = Lazy::new(|| exif::Reader::new());

        let mut sent_messages = 0;
        for attachment in &channel_message.message.attachments {
            match attachment.image_size_bytes {
                None => continue,
                Some(isb) => if isb > self.max_image_bytes { continue },
            };

            if !attachment.title_link.starts_with("/") {
                continue;
            }

            // download image
            let download_response = match interface.obtain_http_resource(&attachment.title_link).await {
                Ok(r) => r,
                Err(_) => continue,
            };
            let (parts, mut body) = download_response.into_parts();
            if parts.status != StatusCode::OK {
                error!("obtaining attachment {:?} led to error code {}", attachment.title_link, parts.status);
                continue;
            }
            let attachment_bytes = match hyper::body::to_bytes(&mut body).await {
                Ok(b) => b.to_vec(),
                Err(e) => {
                    error!("error obtaining bytes from response for attachment {:?}: {}", attachment.title_link, e);
                    continue;
                },
            };
            let mut attachment_cursor = Cursor::new(attachment_bytes);

            // exif?
            let exif_data = match exif_reader.read_from_container(&mut attachment_cursor) {
                Ok(e) => e,
                Err(e) => {
                    error!("failed to read EXIF data: {}", e);
                    continue;
                },
            };

            // lat, lon? refs?
            let gps_lat = match exif_data.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY) {
                Some(l) => l,
                None => continue,
            };
            let gps_lat_ref = match exif_data.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY) {
                Some(lr) => lr,
                None => continue,
            };
            let gps_lon = match exif_data.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY) {
                Some(l) => l,
                None => continue,
            };
            let gps_lon_ref = match exif_data.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY) {
                Some(lr) => lr,
                None => continue,
            };

            let (final_lat, final_lon) = match get_location_from_values(gps_lat, gps_lat_ref, gps_lon, gps_lon_ref) {
                Some(flfl) => flfl,
                None => continue,
            };

            let final_lat_f64 = (*final_lat.numer() as f64) / (*final_lat.denom() as f64);
            let final_lon_f64 = (*final_lon.numer() as f64) / (*final_lon.denom() as f64);

            // try to reverse-geocode
            let geonames_location = match self.geonames_client.get_first_reverse_geo(final_lat_f64, final_lon_f64).await {
                Ok(loc) => loc,
                Err(e) => {
                    error!("GeoNames failed to reverse-geocode {} {}: {}", final_lat_f64, final_lon_f64, e);
                    format!("{} {}", final_lat_f64, final_lon_f64)
                },
            };

            let response_body = format!("EXIF says: {}", geonames_location);
            interface.send_channel_message(
                &channel_message.channel.name,
                &response_body,
            ).await;

            sent_messages += 1;
            if let Some(mmpi) = self.max_messages_per_image {
                if sent_messages >= mmpi {
                    break;
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::Signed;

    #[test]
    fn test_load_from_jpeg() {
        let jpeg_bytes = include_bytes!("../testing/exifgpstest.jpeg");
        let mut jpeg_cursor = std::io::Cursor::new(jpeg_bytes);

        let exif_reader = exif::Reader::new();
        let exif = exif_reader.read_from_container(&mut jpeg_cursor).unwrap();

        let gps_lat = exif.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY).unwrap();
        let gps_lat_ref = exif.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY).unwrap();
        let gps_lon = exif.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY).unwrap();
        let gps_lon_ref = exif.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY).unwrap();

        let (final_lat, final_lon) = get_location_from_values(gps_lat, gps_lat_ref, gps_lon, gps_lon_ref).unwrap();

        let epsilon = Rational64::new(1, 1_000_000_000);

        // the photo encodes 36.9780234, 48.6996499
        assert!((Rational64::new(369780234, 10_000_000) - final_lat).abs() < epsilon);
        assert!((Rational64::new(486996499, 10_000_000) - final_lon).abs() < epsilon);
    }
}
