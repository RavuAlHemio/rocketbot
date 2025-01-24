use strict_num::FiniteF64;


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Percentage {
    // s_ST_Percentage, xsd:string matches "-?[0-9]+(\.[0-9]+)?%"
    // de facto a float with "%" tacked onto the end
    percentage: FiniteF64,
}
impl Percentage {
    pub const fn get(&self) -> FiniteF64 { self.percentage }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let stripped_percent = s.strip_suffix('%')?;
        let value: f64 = stripped_percent.parse().ok()?;
        let finite_value = FiniteF64::new(value)?;
        Some(Self {
            percentage: finite_value,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FixedPercentage {
    // s_ST_FixedPercentage, xsd:string matches "-?((100)|([0-9][0-9]?))(\.[0-9][0-9]?)?%"
    // de facto Percentage clamped to -100.0..=100.0
    percentage: FiniteF64,
}
impl FixedPercentage {
    pub const fn get(&self) -> FiniteF64 { self.percentage }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let unfettered = Percentage::try_from_str(s)?;
        if unfettered.percentage.get() >= -100.0 && unfettered.percentage.get() <= 100.0 {
            Some(Self {
                percentage: unfettered.percentage,
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositivePercentage {
    // s_ST_PositivePercentage, xsd:string matches "[0-9]+(\.[0-9]+)?%"
    // de facto Percentage clamped to 0.0..
    percentage: FiniteF64,
}
impl PositivePercentage {
    pub const fn get(&self) -> FiniteF64 { self.percentage }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let unfettered = Percentage::try_from_str(s)?;
        if unfettered.percentage.get() >= 0.0 {
            Some(Self {
                percentage: unfettered.percentage,
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositiveFixedPercentage {
    // s_ST_PositiveFixedPercentage, xsd:string matches "((100)|([0-9][0-9]?))(\.[0-9][0-9]?)?%"
    // de facto Percentage clamped to 0.0..=100.0
    percentage: FiniteF64,
}
impl PositiveFixedPercentage {
    pub const fn get(&self) -> FiniteF64 { self.percentage }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let unfettered = Percentage::try_from_str(s)?;
        if unfettered.percentage.get() >= 0.0 && unfettered.percentage.get() <= 100.0 {
            Some(Self {
                percentage: unfettered.percentage,
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Angle {
    // a_ST_Angle, xsd:int
    angle: i64,
}
impl Angle {
    pub const fn get(&self) -> i64 { self.angle }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let angle: i64 = s.parse().ok()?;
        Some(Self {
            angle,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FixedAngle {
    // a_ST_FixedAngle, xsd:int in -5400000..5400000
    angle: i32,
}
impl FixedAngle {
    pub const fn get(&self) -> i32 { self.angle }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let unfettered = Angle::try_from_str(s)?;
        if unfettered.angle >= -5400000 && unfettered.angle <= 5400000 {
            Some(Self {
                angle: unfettered.angle.try_into().unwrap(),
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositiveFixedAngle {
    // a_ST_PositiveFixedAngle, xsd:int in 0..21600000
    angle: u32,
}
impl PositiveFixedAngle {
    pub const fn get(&self) -> u32 { self.angle }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let unfettered = Angle::try_from_str(s)?;
        if unfettered.angle >= 0 && unfettered.angle <= 21600000 {
            Some(Self {
                angle: unfettered.angle.try_into().unwrap(),
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Panose {
    // s_ST_Panose, xsd::hexBinary of 10 bytes
    bytes: [u8; 10],
}
impl Panose {
    pub const fn get(&self) -> [u8; 10] { self.bytes }

    pub fn try_from_str(s: &str) -> Option<Self> {
        if s.len() != 20 {
            return None;
        }
        if !s.chars().all(|c| (c >= '0' && c <= '9') || (c >= 'A' && c <= 'F')) {
            return None;
        }
        let mut bytes = [0u8; 10];
        for i in 0..s.len()/2 {
            let hex_byte = &s[2*i..2*i+2];
            bytes[i] = hex_byte.parse().ok()?;
        }
        Some(Self {
            bytes,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum PitchFamily {
    // a_ST_PitchFamily = xsd:byte
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
impl PitchFamily {
    pub fn try_from_str(s: &str) -> Option<Self> {
        if s.len() != 2 {
            return None;
        }
        if !s.chars().all(|c| (c >= '0' && c <= '9') || (c >= 'A' && c <= 'F')) {
            return None;
        }
        let byte: u8 = s.parse().ok()?;
        match byte {
            0x00 => Some(Self::Family00),
            0x01 => Some(Self::Family01),
            0x02 => Some(Self::Family02),
            0x16 => Some(Self::Family16),
            0x17 => Some(Self::Family17),
            0x18 => Some(Self::Family18),
            0x32 => Some(Self::Family32),
            0x33 => Some(Self::Family33),
            0x34 => Some(Self::Family34),
            0x48 => Some(Self::Family48),
            0x49 => Some(Self::Family49),
            0x50 => Some(Self::Family50),
            0x64 => Some(Self::Family64),
            0x65 => Some(Self::Family65),
            0x66 => Some(Self::Family66),
            0x80 => Some(Self::Family80),
            0x81 => Some(Self::Family81),
            0x82 => Some(Self::Family82),
            _ => None,
        }
    }
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Coordinate {
    // a_ST_Coordinate
    Unqualified(CoordinateUnqualified), // a_ST_CoordinateUnqualified
    UniversalMeasure(UniversalMeasure), // s_ST_UniversalMeasure
}
impl Coordinate {
    pub fn try_from_str(s: &str) -> Option<Self> {
        // try to parse as universal measure first, then as a bare value
        if let Some(um) = UniversalMeasure::try_from_str(s) {
            Some(Self::UniversalMeasure(um))
        } else if let Some(cu) = CoordinateUnqualified::try_from_str(s) {
            Some(Self::Unqualified(cu))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Coordinate32 {
    Unqualified(Coordinate32Unqualified),
    UniversalMeasure(UniversalMeasure),
}
impl Coordinate32 {
    pub fn try_from_str(s: &str) -> Option<Self> {
        // try to parse as universal measure first, then as a bare value
        if let Some(um) = UniversalMeasure::try_from_str(s) {
            Some(Self::UniversalMeasure(um))
        } else if let Some(cu) = Coordinate32Unqualified::try_from_str(s) {
            Some(Self::Unqualified(cu))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextPoint {
    Unqualified(TextPointUnqualified),
    UniversalMeasure(UniversalMeasure),
}
impl TextPoint {
    pub fn try_from_str(s: &str) -> Option<Self> {
        // try to parse as universal measure first, then as a bare value
        if let Some(um) = UniversalMeasure::try_from_str(s) {
            Some(Self::UniversalMeasure(um))
        } else if let Some(cu) = TextPointUnqualified::try_from_str(s) {
            Some(Self::Unqualified(cu))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CoordinateUnqualified {
    // xsd:long in -27273042329600..=27273042316900
    value: i64,
}
impl CoordinateUnqualified {
    pub const fn get(&self) -> i64 { self.value }

    pub const fn try_from_i64(value: i64) -> Option<Self> {
        if value >= -27273042329600 && value <= 27273042316900 {
            Some(Self {
                value,
            })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: i64 = s.parse().ok()?;
        Self::try_from_i64(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Coordinate32Unqualified {
    // xsd:int
    value: i32,
}
impl Coordinate32Unqualified {
    pub const fn get(&self) -> i32 { self.value }

    pub const fn from_i32(value: i32) -> Self {
        Self { value }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: i32 = s.parse().ok()?;
        Some(Self {
            value,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextPointUnqualified {
    // xsd:int in -400000..=400000
    value: i32,
}
impl TextPointUnqualified {
    pub const fn get(&self) -> i32 { self.value }

    pub const fn try_from_i32(value: i32) -> Option<Self> {
        if value >= -400000 && value <= 400000 {
            Some(Self {
                value,
            })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: i32 = s.parse().ok()?;
        Self::try_from_i32(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UniversalMeasure {
    // s_ST_UniversalMeasure = xsd:string matching "-?[0-9]+(\.[0-9]+)?(mm|cm|in|pt|pc|pi)"
    value: FiniteF64,
    unit: UniversalMeasureUnit,
}
impl UniversalMeasure {
    pub const fn get_value(&self) -> FiniteF64 { self.value }
    pub const fn get_unit(&self) -> UniversalMeasureUnit { self.unit }

    pub fn try_from_str(s: &str) -> Option<Self> {
        // at least one digit and two letters of unit
        if s.len() < 3 {
            return None;
        }

        // slice right before the last two characters
        // if that isn't at a Unicode boundary, the value is invalid
        let (number_str, unit_str) = s.split_at_checked(s.len() - 2)?;
        let unit = UniversalMeasureUnit::try_from_str(unit_str)?;
        let value_f64: f64 = number_str.parse().ok()?;
        let value = FiniteF64::new(value_f64)?;
        Some(Self {
            value,
            unit,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum UniversalMeasureUnit {
    Mm,
    Cm,
    In,
    Pt,
    Pc,
    Pi,
}
impl UniversalMeasureUnit {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "mm" => Some(Self::Mm),
            "cm" => Some(Self::Cm),
            "in" => Some(Self::In),
            "pt" => Some(Self::Pt),
            "pc" => Some(Self::Pc),
            "pi" => Some(Self::Pi),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositiveCoordinate {
    // xsd:long in 0..=27273042316900
    coordinate: u64,
}
impl PositiveCoordinate {
    pub const fn get(&self) -> u64 { self.coordinate }

    pub const fn try_from_u64(coordinate: u64) -> Option<Self> {
        if coordinate <= 27273042316900 {
            Some(Self { coordinate })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u64 = s.parse().ok()?;
        Self::try_from_u64(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositiveCoordinate32 {
    // xsd:int in 0..
    coordinate: u32,
}
impl PositiveCoordinate32 {
    pub const fn get(&self) -> u32 { self.coordinate }

    pub const fn from_u32(coordinate: u32) -> Self {
        Self { coordinate }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u32 = s.parse().ok()?;
        Some(Self::from_u32(value))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineWidth {
    // xsd:int in 0..=20116800
    width: u32,
}
impl LineWidth {
    pub const fn get(&self) -> u32 { self.width }

    pub const fn try_from_u32(width: u32) -> Option<Self> {
        if width <= 20116800 {
            Some(Self { width })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u32 = s.parse().ok()?;
        Self::try_from_u32(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FovAngle {
    // xsd:int in 0..=10800000
    angle: u32,
}
impl FovAngle {
    pub const fn get(&self) -> u32 { self.angle }

    pub const fn try_from_u32(angle: u32) -> Option<Self> {
        if angle <= 10800000 {
            Some(Self { angle })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u32 = s.parse().ok()?;
        Self::try_from_u32(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextColumnCount {
    // xsd:int in 1..=16
    count: u8,
}
impl TextColumnCount {
    pub const fn get(&self) -> u8 { self.count }

    pub const fn try_from_u8(count: u8) -> Option<Self> {
        if count >= 1 && count <= 16 {
            Some(Self { count })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let count: u8 = s.parse().ok()?;
        Self::try_from_u8(count)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextMargin {
    // xsd:int in 0..=51206400
    margin: u32,
}
impl TextMargin {
    pub const fn get(&self) -> u32 { self.margin }

    pub const fn try_from_u32(margin: u32) -> Option<Self> {
        if margin <= 51206400 {
            Some(Self { margin })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u32 = s.parse().ok()?;
        Self::try_from_u32(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextIndentLevelType {
    // xsd:int in 0..=8
    level: u8,
}
impl TextIndentLevelType {
    pub const fn get(&self) -> u8 { self.level }

    pub const fn try_from_u8(level: u8) -> Option<Self> {
        if level <= 8 {
            Some(Self { level })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u8 = s.parse().ok()?;
        Self::try_from_u8(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextIndent {
    // xsd:int in -51206400..=51206400
    level: i32,
}
impl TextIndent {
    pub const fn get(&self) -> i32 { self.level }

    pub const fn try_from_i32(level: i32) -> Option<Self> {
        if level >= -51206400 && level <= 51206400 {
            Some(Self { level })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: i32 = s.parse().ok()?;
        Self::try_from_i32(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextSpacingPoint {
    // xsd:int in 0..=158400
    level: u32,
}
impl TextSpacingPoint {
    pub const fn get(&self) -> u32 { self.level }

    pub const fn try_from_u32(level: u32) -> Option<Self> {
        if level <= 158400 {
            Some(Self { level })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u32 = s.parse().ok()?;
        Self::try_from_u32(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextBulletSizePercent {
    // xsd:int in 25..=400
    size: u16,
}
impl TextBulletSizePercent {
    pub const fn get(&self) -> u16 { self.size }

    pub const fn try_from_u16(size: u16) -> Option<Self> {
        if size >= 25 && size <= 400 {
            Some(Self { size })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u16 = s.parse().ok()?;
        Self::try_from_u16(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextNonNegativePoint {
    // xsd:int in 0..=400000
    size: u32,
}
impl TextNonNegativePoint {
    pub const fn get(&self) -> u32 { self.size }

    pub const fn try_from_u32(size: u32) -> Option<Self> {
        if size <= 400000 {
            Some(Self { size })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u32 = s.parse().ok()?;
        Self::try_from_u32(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextFontSize {
    // xsd:int in 100..=400000
    size: u32,
}
impl TextFontSize {
    pub const fn get(&self) -> u32 { self.size }

    pub const fn try_from_u32(size: u32) -> Option<Self> {
        if size >= 100 && size <= 400000 {
            Some(Self { size })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u32 = s.parse().ok()?;
        Self::try_from_u32(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextBulletStartAtNumber {
    // xsd:int in 1..=32767
    start_at: u16,
}
impl TextBulletStartAtNumber {
    pub const fn get(&self) -> u16 { self.start_at }

    pub const fn try_from_u16(start_at: u16) -> Option<Self> {
        if start_at >= 1 && start_at <= 32767 {
            Some(Self { start_at })
        } else {
            None
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        let value: u16 = s.parse().ok()?;
        Self::try_from_u16(value)
    }
}
