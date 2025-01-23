use strict_num::FiniteF64;


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Percentage {
    // s_ST_Percentage, xsd:string matches "-?[0-9]+(\.[0-9]+)?%"
    pub percentage: FiniteF64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FixedPercentage {
    // s_ST_FixedPercentage, xsd:string matches "-?((100)|([0-9][0-9]?))(\.[0-9][0-9]?)?%"
    // de facto Percentage clamped to -100.0..=100.0
    pub percentage: FiniteF64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositivePercentage {
    // s_ST_PositivePercentage, xsd:string matches "[0-9]+(\.[0-9]+)?%"
    // de facto Percentage clamped to 0.0..
    pub percentage: FiniteF64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositiveFixedPercentage {
    // s_ST_PositiveFixedPercentage, xsd:string matches "((100)|([0-9][0-9]?))(\.[0-9][0-9]?)?%"
    // de facto Percentage clamped to 0.0..=100.0
    pub percentage: FiniteF64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Angle {
    // a_ST_Angle, xsd:int
    pub angle: i64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositiveFixedAngle {
    // a_ST_PositiveFixedAngle, xsd:int in 0..21600000
    pub angle: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Panose {
    // s_ST_Panose, xsd::hexBinary of 10 bytes
    pub bytes: [u8; 10],
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum PitchFamily {
    // a_ST_PitchFamily
    Family00 = 0x00,
    Family01 = 0x01,
    Family02 = 0x02,
    Family16 = 0x16,
    Family17 = 0x17,
    Family18 = 0x18,
    Family32 = 0x32,
    Family33 = 0x33,
    Family34 = 0x34,
    Family48 = 0x48,
    Family49 = 0x49,
    Family50 = 0x50,
    Family64 = 0x64,
    Family65 = 0x65,
    Family66 = 0x66,
    Family80 = 0x80,
    Family81 = 0x81,
    Family82 = 0x82,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TileFlipMode {
    None,
    X,
    Y,
    XY,
}
impl TileFlipMode {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "xy" => Some(Self::XY),
            _ => None,
        }
    }
}
