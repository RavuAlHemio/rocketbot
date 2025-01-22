use std::str::FromStr;

use from_to_repr::from_to_other;
use strict_num::FiniteF64;


#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_argb_u32(argb: u32) -> Self {
        let a = ((argb >> 24) & 0xFF) as u8;
        let r = ((argb >> 16) & 0xFF) as u8;
        let g = ((argb >>  8) & 0xFF) as u8;
        let b = ((argb >>  0) & 0xFF) as u8;
        Self::new(r, g, b, a)
    }
}
impl FromStr for Rgba {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // assume AARRGGBB as hex
        if s.len() != 8 {
            return Err(());
        }
        let argb_u32 = u32::from_str_radix(s, 16)
            .map_err(|_| ())?;
        Ok(Self::from_argb_u32(argb_u32))
    }
}


#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Color {
    pub auto: Option<bool>,
    pub indexed: Option<usize>,
    pub rgb: Option<Rgba>,
    pub theme: Option<usize>,
    pub tint: Option<FiniteF64>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Fill {
    Pattern(PatternFill),
    Gradient(GradientFill),
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatternFill {
    pub foreground_color: Option<Color>,
    pub background_color: Option<Color>,
    pub pattern_type: Option<PatternType>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PatternType {
    None,
    Solid,
    MediumGray,
    DarkGray,
    LightGray,
    DarkHorizontal,
    DarkVertical,
    DarkDown,
    DarkUp,
    DarkGrid,
    DarkTrellis,
    LightHorizontal,
    LightVertical,
    LightDown,
    LightUp,
    LightGrid,
    LightTrellis,
    Gray125,
    Gray0625,
}
impl PatternType {
    pub fn try_from_str(type_str: &str) -> Option<Self> {
        match type_str {
            "none" => Some(Self::None),
            "solid" => Some(Self::Solid),
            "mediumGray" => Some(Self::MediumGray),
            "darkGray" => Some(Self::DarkGray),
            "lightGray" => Some(Self::LightGray),
            "darkHorizontal" => Some(Self::DarkHorizontal),
            "darkVertical" => Some(Self::DarkVertical),
            "darkDown" => Some(Self::DarkDown),
            "darkUp" => Some(Self::DarkUp),
            "darkGrid" => Some(Self::DarkGrid),
            "darkTrellis" => Some(Self::DarkTrellis),
            "lightHorizontal" => Some(Self::LightHorizontal),
            "lightVertical" => Some(Self::LightVertical),
            "lightDown" => Some(Self::LightDown),
            "lightUp" => Some(Self::LightUp),
            "lightGrid" => Some(Self::LightGrid),
            "lightTrellis" => Some(Self::LightTrellis),
            "gray125" => Some(Self::Gray125),
            "gray0625" => Some(Self::Gray0625),
            _ => None,
        }
    }
}
impl FromStr for PatternType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GradientFill {
    pub stops: Vec<GradientStop>,
    pub gradient_type: Option<GradientType>,
    pub degree: Option<FiniteF64>,
    pub left: Option<FiniteF64>,
    pub right: Option<FiniteF64>,
    pub top: Option<FiniteF64>,
    pub bottom: Option<FiniteF64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GradientStop {
    pub color: Color,
    pub position: FiniteF64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GradientType {
    Linear,
    Path,
}
impl GradientType {
    pub fn try_from_str(type_str: &str) -> Option<Self> {
        match type_str {
            "linear" => Some(Self::Linear),
            "path" => Some(Self::Path),
            _ => None,
        }
    }
}
impl FromStr for GradientType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}


#[derive(Clone, Copy, Debug)]
#[from_to_other(base_type = u8, derive_compare = "as_int")]
pub enum FontFamily {
    NotApplicable = 0,
    Roman = 1,
    Swiss = 2,
    Modern = 3,
    Script = 4,
    Decorative = 5,
    Other(u8),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Underline {
    Single,
    Double,
    SingleAccounting,
    DoubleAccounting,
    None,
}
impl Underline {
    pub fn try_from_str(underline_str: &str) -> Option<Self> {
        match underline_str {
            "single" => Some(Self::Single),
            "double" => Some(Self::Double),
            "singleAccounting" => Some(Self::SingleAccounting),
            "doubleAccounting" => Some(Self::DoubleAccounting),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}
impl FromStr for Underline {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum VerticalAlignRun {
    #[default] Baseline,
    Subscript,
    Superscript,
}
impl VerticalAlignRun {
    pub fn try_from_str(vertical_align_str: &str) -> Option<Self> {
        match vertical_align_str {
            "baseline" => Some(Self::Baseline),
            "subscript" => Some(Self::Subscript),
            "superscript" => Some(Self::Superscript),
            _ => None,
        }
    }
}
impl FromStr for VerticalAlignRun {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FontScheme {
    Major,
    Minor,
    None,
}
impl FontScheme {
    pub fn try_from_str(scheme_str: &str) -> Option<Self> {
        match scheme_str {
            "major" => Some(Self::Major),
            "minor" => Some(Self::Minor),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}
impl FromStr for FontScheme {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Font {
    pub name: Option<String>,
    pub charset: Option<u8>,
    pub family: Option<FontFamily>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strikethrough: Option<bool>,
    pub outline: Option<bool>,
    pub shadow: Option<bool>,
    pub condense: Option<bool>,
    pub extend: Option<bool>,
    pub color: Option<Color>,
    pub size: Option<FiniteF64>,
    pub underline: Option<Underline>,
    pub vertical_align: Option<VerticalAlignRun>,
    pub scheme: Option<FontScheme>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Border {
    pub start: Option<BorderProperties>,
    pub end: Option<BorderProperties>,
    pub top: Option<BorderProperties>,
    pub bottom: Option<BorderProperties>,
    pub diagonal: Option<BorderProperties>,
    pub vertical: Option<BorderProperties>,
    pub horizontal: Option<BorderProperties>,
    pub diagonal_up: Option<bool>,
    pub diagonal_down: Option<bool>,
    pub outline: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BorderProperties {
    pub color: Option<Color>,
    pub border_style: Option<BorderStyle>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BorderStyle {
    None,
    Thin,
    Medium,
    Dashed,
    Dotted,
    Thick,
    Double,
    Hair,
    MediumDashed,
    DashDot,
    MediumDashDot,
    DashDotDot,
    MediumDashDotDot,
    SlantDashDot,
}
impl BorderStyle {
    pub fn try_from_str(style_str: &str) -> Option<Self> {
        match style_str {
            "none" => Some(Self::None),
            "thin" => Some(Self::Thin),
            "medium" => Some(Self::Medium),
            "dashed" => Some(Self::Dashed),
            "dotted" => Some(Self::Dotted),
            "thick" => Some(Self::Thick),
            "double" => Some(Self::Double),
            "hair" => Some(Self::Hair),
            "mediumDashed" => Some(Self::MediumDashed),
            "dashDot" => Some(Self::DashDot),
            "mediumDashDot" => Some(Self::MediumDashDot),
            "dashDotDot" => Some(Self::DashDotDot),
            "mediumDashDotDot" => Some(Self::MediumDashDotDot),
            "slantDashDot" => Some(Self::SlantDashDot),
            _ => None,
        }
    }
}
impl FromStr for BorderStyle {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FormattingRecord {
    pub alignment: Option<CellAlignment>,
    pub protection: Option<CellProtection>,
    pub number_format_id: Option<usize>,
    pub font_id: Option<usize>,
    pub fill_id: Option<usize>,
    pub border_id: Option<usize>,
    pub cell_formatting_record_id: Option<usize>,
    pub quote_prefix: Option<bool>,
    pub pivot_button: Option<bool>,
    pub apply_number_format: Option<bool>,
    pub apply_font: Option<bool>,
    pub apply_fill: Option<bool>,
    pub apply_border: Option<bool>,
    pub apply_alignment: Option<bool>,
    pub apply_protection: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellAlignment {
    pub horizontal: Option<HorizontalAlignment>,
    pub vertical: Option<VerticalAlignment>,
    pub text_rotation: Option<u8>,
    pub wrap_text: Option<bool>,
    pub indent: Option<u64>,
    pub relative_indent: Option<i64>,
    pub justify_last_line: Option<bool>,
    pub shrink_to_fit: Option<bool>,
    pub reading_order: Option<ReadingOrder>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HorizontalAlignment {
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterContinuous,
    Distributed,
}
impl HorizontalAlignment {
    pub fn try_from_str(style_str: &str) -> Option<Self> {
        match style_str {
            "general" => Some(Self::General),
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            "fill" => Some(Self::Fill),
            "justify" => Some(Self::Justify),
            "centerContinuous" => Some(Self::CenterContinuous),
            "distributed" => Some(Self::Distributed),
            _ => None,
        }
    }
}
impl FromStr for HorizontalAlignment {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
    Justify,
    Distributed,
}
impl VerticalAlignment {
    pub fn try_from_str(style_str: &str) -> Option<Self> {
        match style_str {
            "top" => Some(Self::Top),
            "center" => Some(Self::Center),
            "bottom" => Some(Self::Bottom),
            "justify" => Some(Self::Justify),
            "distributed" => Some(Self::Distributed),
            _ => None,
        }
    }
}
impl FromStr for VerticalAlignment {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}

#[derive(Clone, Copy, Debug)]
#[from_to_other(base_type = u32, derive_compare = "as_int")]
pub enum ReadingOrder {
    ContextDependent = 0,
    LeftToRight = 1,
    RightToLeft = 2,
    Other(u32),
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellProtection {
    pub locked: Option<bool>,
    pub hidden: Option<bool>,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Stylesheet {
    pub number_formats: Vec<NumberFormat>,
    pub fonts: Vec<Font>,
    pub fills: Vec<Fill>,
    pub borders: Vec<Border>,
    pub cell_style_formatting_records: Vec<FormattingRecord>,
    pub cell_formatting_records: Vec<FormattingRecord>,
    pub cell_styles: Vec<CellStyle>,
    pub differential_formatting_records: Vec<DifferentialFormattingRecord>,
    pub table_styles: Vec<TableStyle>,
    pub colors: Vec<Color>,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NumberFormat {
    pub format_id: usize,
    pub format_code: String,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellStyle {
    pub name: Option<String>,
    pub formatting_record_id: usize,
    pub builtin_id: Option<usize>,
    pub i_level: Option<u64>,
    pub hidden: Option<bool>,
    pub custom_builtin: Option<bool>,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DifferentialFormattingRecord {
    pub font: Option<Font>,
    pub number_format: Option<NumberFormat>,
    pub fill: Option<Fill>,
    pub alignment: Option<CellAlignment>,
    pub border: Option<Border>,
    pub protection: Option<CellProtection>,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TableStyle {
    pub name: String,
    pub pivot: Option<bool>,
    pub table: Option<bool>,
    pub elements: Vec<TableStyleElement>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TableStyleElement {
    pub style_type: TableStyleType,
    pub size: Option<u64>,
    pub differential_formatting_record_id: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TableStyleType {
    WholeTable,
    HeaderRow,
    TotalRow,
    FirstColumn,
    LastColumn,
    FirstRowStripe,
    SecondRowStripe,
    FirstColumnStripe,
    SecondColumnStripe,
    FirstHeaderCell,
    LastHeaderCell,
    FirstTotalCell,
    LastTotalCell,
    FirstSubtotalColumn,
    SecondSubtotalColumn,
    ThirdSubtotalColumn,
    FirstSubtotalRow,
    SecondSubtotalRow,
    ThirdSubtotalRow,
    BlankRow,
    FirstColumnSubheading,
    SecondColumnSubheading,
    ThirdColumnSubheading,
    FirstRowSubheading,
    SecondRowSubheading,
    ThirdRowSubheading,
    PageFieldLabels,
    PageFieldValues,
}
impl TableStyleType {
    pub fn try_from_str(style_str: &str) -> Option<Self> {
        match style_str {
            "wholeTable" => Some(Self::WholeTable),
            "headerRow" => Some(Self::HeaderRow),
            "totalRow" => Some(Self::TotalRow),
            "firstColumn" => Some(Self::FirstColumn),
            "lastColumn" => Some(Self::LastColumn),
            "firstRowStripe" => Some(Self::FirstRowStripe),
            "secondRowStripe" => Some(Self::SecondRowStripe),
            "firstColumnStripe" => Some(Self::FirstColumnStripe),
            "secondColumnStripe" => Some(Self::SecondColumnStripe),
            "firstHeaderCell" => Some(Self::FirstHeaderCell),
            "lastHeaderCell" => Some(Self::LastHeaderCell),
            "firstTotalCell" => Some(Self::FirstTotalCell),
            "lastTotalCell" => Some(Self::LastTotalCell),
            "firstSubtotalColumn" => Some(Self::FirstSubtotalColumn),
            "secondSubtotalColumn" => Some(Self::SecondSubtotalColumn),
            "thirdSubtotalColumn" => Some(Self::ThirdSubtotalColumn),
            "firstSubtotalRow" => Some(Self::FirstSubtotalRow),
            "secondSubtotalRow" => Some(Self::SecondSubtotalRow),
            "thirdSubtotalRow" => Some(Self::ThirdSubtotalRow),
            "blankRow" => Some(Self::BlankRow),
            "firstColumnSubheading" => Some(Self::FirstColumnSubheading),
            "secondColumnSubheading" => Some(Self::SecondColumnSubheading),
            "thirdColumnSubheading" => Some(Self::ThirdColumnSubheading),
            "firstRowSubheading" => Some(Self::FirstRowSubheading),
            "secondRowSubheading" => Some(Self::SecondRowSubheading),
            "thirdRowSubheading" => Some(Self::ThirdRowSubheading),
            "pageFieldLabels" => Some(Self::PageFieldLabels),
            "pageFieldValues" => Some(Self::PageFieldValues),
            _ => None,
        }
    }
}
impl FromStr for TableStyleType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
            .ok_or(())
    }
}
