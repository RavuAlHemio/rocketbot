pub mod achievements;
pub mod ride_table;


use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, TimeZone};
use indexmap::IndexSet;
use rocketbot_string::NatSortedString;
use serde::{Deserialize, Serialize};


pub type VehicleNumber = NatSortedString;


#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VehicleClass {
    Tram,
    Metro,
    PreMetro,
    Bus,
    ElectricBus,
    Trolleybus,
    BatteryTrolleybus,
    TramTrain,
    RegionalTrain,
    LongDistanceTrain,
    HorseDrawnCarriage,
    Funicular,
    AerialTramway,
    JBarLift,
    TBarLift,
    SeatLift,
    GondolaLift,
    SeatAndGondolaLift,
    Ship,
    Hovercraft,
}
impl fmt::Display for VehicleClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tram => write!(f, "tram"),
            Self::Metro => write!(f, "metro"),
            Self::PreMetro => write!(f, "premetro"),
            Self::Bus => write!(f, "bus"),
            Self::ElectricBus => write!(f, "electric bus"),
            Self::Trolleybus => write!(f, "trolleybus"),
            Self::BatteryTrolleybus => write!(f, "battery trolleybus"),
            Self::TramTrain => write!(f, "tram-train"),
            Self::RegionalTrain => write!(f, "train (regional)"),
            Self::LongDistanceTrain => write!(f, "train (long-distance)"),
            Self::HorseDrawnCarriage => write!(f, "horse-drawn carriage"),
            Self::Funicular => write!(f, "funicular"),
            Self::AerialTramway => write!(f, "aerial tramway"),
            Self::JBarLift => write!(f, "J-bar lift"),
            Self::TBarLift => write!(f, "T-bar lift"),
            Self::SeatLift => write!(f, "seat lift"),
            Self::GondolaLift => write!(f, "gondola lift"),
            Self::SeatAndGondolaLift => write!(f, "seat and gondola lift"),
            Self::Ship => write!(f, "ship"),
            Self::Hovercraft => write!(f, "hovercraft"),
        }
    }
}


/// Specifies whether a vehicle has actually been ridden or was simply coupled to one that was ridden.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CouplingMode {
    /// Explicitly specified and actually ridden.
    Ridden,

    /// Explicitly specified but only coupled to the vehicle actually ridden.
    Explicit,

    /// Not specified, but fixed-coupled to the vehicle actually ridden.
    FixedCoupling,
}
impl CouplingMode {
    pub fn try_from_db_str(db_str: &str) -> Option<Self> {
        match db_str {
            "R" => Some(Self::Ridden),
            "E" => Some(Self::Explicit),
            "F" => Some(Self::FixedCoupling),
            _ => None,
        }
    }

    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Ridden => "R",
            Self::Explicit => "E",
            Self::FixedCoupling => "F",
        }
    }

    pub fn is_explicit(&self) -> bool {
        match self {
            Self::Ridden|Self::Explicit => true,
            _ => false,
        }
    }
}
impl fmt::Display for CouplingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ridden => write!(f, "ridden"),
            Self::Explicit => write!(f, "explicit"),
            Self::FixedCoupling => write!(f, "coupled"),
        }
    }
}


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VehicleInfo {
    pub number: VehicleNumber,
    pub vehicle_class: VehicleClass,
    pub type_code: String,
    pub in_service_since: Option<String>,
    pub out_of_service_since: Option<String>,
    pub manufacturer: Option<String>,
    pub other_data: BTreeMap<String, String>,
    pub fixed_coupling: IndexSet<VehicleNumber>,
}
impl VehicleInfo {
    pub fn new(number: VehicleNumber, vehicle_class: VehicleClass, type_code: String) -> Self {
        Self {
            number,
            vehicle_class,
            type_code,
            in_service_since: None,
            out_of_service_since: None,
            manufacturer: None,
            other_data: BTreeMap::new(),
            fixed_coupling: IndexSet::new(),
        }
    }
}


/// The last rider of a vehicle.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum LastRider<'a> {
    #[default]
    Nobody,

    Me,

    SomebodyElse(&'a str),
}
impl<'a> LastRider<'a> {
    #[inline]
    pub const fn is_somebody_else(&self) -> bool {
        match self {
            Self::SomebodyElse(_) => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_specific_somebody_else(&self, who: &str) -> bool {
        match self {
            Self::SomebodyElse(se) => *se == who,
            _ => false,
        }
    }
}


/// Formats a timestamp, possibly relative to another timestamp.
///
/// If an anchor timestamp is provided and the timestamp has happened less than 24h before the
/// anchor timestamp, the timestamp is formatted as time-only. Otherwise, it is formatted as date
/// and time.
fn format_timestamp<Tz1: TimeZone, Tz2: TimeZone>(
    timestamp: DateTime<Tz1>,
    anchor_timestamp: Option<DateTime<Tz2>>,
) -> String
    where
        Tz1::Offset: fmt::Display {
    if let Some(anchor) = anchor_timestamp {
        if anchor >= timestamp && anchor.signed_duration_since(timestamp.clone()).num_hours() < 24 {
            return timestamp.format("%H:%M").to_string();
        }
    }
    timestamp.format("%d.%m.%Y %H:%M").to_string()
}
