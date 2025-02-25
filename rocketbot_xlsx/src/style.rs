use std::str::FromStr;

use from_to_repr::from_to_other;
use strict_num::FiniteF64;
use sxd_document::dom::Element;

use crate::{Error, QualifiedName};
use crate::xml::{ElemExt, NS_SPRSH, StrExt};


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

    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        elem.required_attribute_value_in_format(
            "rgb",
            path,
            |s| s.parse().ok(),
            "\"AARRGGBB\" as hex",
        )
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
impl Color {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let auto = elem.attribute_value("auto")
            .as_xsd_boolean();
        let indexed = elem.attribute_value("indexed")
            .as_usize();
        let rgb = elem.attribute_value("rgb")
            .and_then(|hex_str| hex_str.parse().ok());
        let theme = elem.attribute_value("theme")
            .as_usize();
        let tint = elem.attribute_value("tint")
            .as_finite_f64();
        Self {
            auto,
            indexed,
            rgb,
            theme,
            tint,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Fill {
    Pattern(PatternFill),
    Gradient(GradientFill),
}
impl Fill {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let pattern_fill_child_opt = elem
            .first_child_element_named_ns("patternFill", NS_SPRSH);
        let gradient_fill_child_opt = elem
            .first_child_element_named_ns("gradientFill", NS_SPRSH);
        if let Some(pattern_fill_child) = pattern_fill_child_opt {
            if let Some(gradient_fill_child) = gradient_fill_child_opt {
                return Err(Error::MultiChoiceChildElements {
                    path: path.to_owned(),
                    parent_name: elem.name().into(),
                    one_child_name: pattern_fill_child.name().into(),
                    other_child_name: gradient_fill_child.name().into(),
                });
            }
            let pattern_fill = PatternFill::from_elem(elem);
            Ok(Self::Pattern(pattern_fill))
        } else if let Some(gradient_fill_child) = gradient_fill_child_opt {
            let gradient_fill = GradientFill::from_elem(gradient_fill_child);
            Ok(Self::Gradient(gradient_fill))
        } else {
            Err(Error::MissingChildElement {
                path: path.to_owned(),
                childless_parent_name: elem.name().into(),
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatternFill {
    pub foreground_color: Option<Color>,
    pub background_color: Option<Color>,
    pub pattern_type: Option<PatternType>,
}
impl PatternFill {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let foreground_color = elem
            .first_child_element_named_ns("fgColor", NS_SPRSH)
            .map(|fc| Color::from_elem(fc));
        let background_color = elem
            .first_child_element_named_ns("bgColor", NS_SPRSH)
            .map(|fc| Color::from_elem(fc));
        let pattern_type = elem
            .attribute_value("patternType")
            .and_then(|pt| PatternType::try_from_str(pt));
        Self {
            foreground_color,
            background_color,
            pattern_type,
        }
    }
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
impl GradientFill {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let gradient_type = elem.attribute_value("type")
            .and_then(|gt_str| GradientType::try_from_str(gt_str));
        let degree = elem.attribute_value("degree")
            .as_finite_f64();
        let left = elem.attribute_value("left")
            .as_finite_f64();
        let right = elem.attribute_value("right")
            .as_finite_f64();
        let top = elem.attribute_value("top")
            .as_finite_f64();
        let bottom = elem.attribute_value("bottom")
            .as_finite_f64();

        let mut stops = Vec::new();
        let stop_elems = elem
            .child_elements_named_ns("stop", NS_SPRSH);
        for stop_elem in stop_elems {
            let Some(position) = stop_elem.attribute_value("position")
                .as_finite_f64()
                else { continue };
            let Some(color_elem) = stop_elem
                .first_child_element_named_ns("color", NS_SPRSH)
                else { continue };
            let color = Color::from_elem(color_elem);
            stops.push(GradientStop {
                position,
                color,
            });
        }
        Self {
            stops,
            gradient_type,
            degree,
            left,
            right,
            top,
            bottom,
        }
    }
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
pub enum FontVerticalAlign {
    #[default] Baseline,
    Subscript,
    Superscript,
}
impl FontVerticalAlign {
    pub fn try_from_str(vertical_align_str: &str) -> Option<Self> {
        match vertical_align_str {
            "baseline" => Some(Self::Baseline),
            "subscript" => Some(Self::Subscript),
            "superscript" => Some(Self::Superscript),
            _ => None,
        }
    }
}
impl FromStr for FontVerticalAlign {
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
    pub vertical_align: Option<FontVerticalAlign>,
    pub scheme: Option<FontScheme>,
}
impl Font {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let name = elem.first_child_element_named_ns("name", NS_SPRSH)
            .and_then(|e| e.attribute_value("val"))
            .map(|n| n.to_owned());
        let charset: Option<u8> = elem.first_child_element_named_ns("charset", NS_SPRSH)
            .and_then(|e| e.attribute_value("val"))
            .and_then(|s| s.parse().ok());
        let family = elem.first_child_element_named_ns("family", NS_SPRSH)
            .and_then(|e| e.attribute_value("val"))
            .and_then(|s| u8::from_str(s).ok())
            .map(|v| FontFamily::from_base_type(v));
        // boolean elements: if the element exists but does not have a (valid) "val" attribute, assume true
        let bold = elem.first_child_element_ns_boolean_property_assuming("b", NS_SPRSH, true);
        let italic = elem.first_child_element_ns_boolean_property_assuming("i", NS_SPRSH, true);
        let strikethrough = elem.first_child_element_ns_boolean_property_assuming("strike", NS_SPRSH, true);
        let outline = elem.first_child_element_ns_boolean_property_assuming("outline", NS_SPRSH, true);
        let shadow = elem.first_child_element_ns_boolean_property_assuming("shadow", NS_SPRSH, true);
        let condense = elem.first_child_element_ns_boolean_property_assuming("condense", NS_SPRSH, true);
        let extend = elem.first_child_element_ns_boolean_property_assuming("extend", NS_SPRSH, true);
        let color = elem.first_child_element_named_ns("color", NS_SPRSH)
            .map(|e| Color::from_elem(e));
        let size = elem.first_child_element_named_ns("sz", NS_SPRSH)
            .and_then(|e| e.attribute_value("val"))
            .and_then(|s| f64::from_str(s).ok())
            .and_then(|f| FiniteF64::new(f));
        // similar to boolean elements
        let underline = elem.first_child_element_named_ns("u", NS_SPRSH)
            .map(|underline_elem| underline_elem
                .attribute_value("val")
                .and_then(|v| Underline::try_from_str(v))
                .unwrap_or(Underline::Single)
            );
        let vertical_align = elem.first_child_element_named_ns("vertAlign", NS_SPRSH)
            .and_then(|e| e.attribute_value("val"))
            .and_then(|s| FontVerticalAlign::try_from_str(s));
        let scheme = elem.first_child_element_named_ns("scheme", NS_SPRSH)
            .map(|e| e.collect_text())
            .and_then(|s| FontScheme::try_from_str(&s));
        Self {
            name,
            charset,
            family,
            bold,
            italic,
            strikethrough,
            outline,
            shadow,
            condense,
            extend,
            color,
            size,
            underline,
            vertical_align,
            scheme,
        }
    }
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
impl Border {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let diagonal_up = elem.attribute_value("diagonalUp")
            .and_then(|s| s.as_xsd_boolean());
        let diagonal_down = elem.attribute_value("diagonalDown")
            .and_then(|s| s.as_xsd_boolean());
        let outline = elem.attribute_value("outline")
            .and_then(|s| s.as_xsd_boolean());
        let start = elem.first_child_element_named_ns("start", NS_SPRSH)
            .map(|e| BorderProperties::from_elem(e));
        let end = elem.first_child_element_named_ns("end", NS_SPRSH)
            .map(|e| BorderProperties::from_elem(e));
        let top = elem.first_child_element_named_ns("top", NS_SPRSH)
            .map(|e| BorderProperties::from_elem(e));
        let bottom = elem.first_child_element_named_ns("bottom", NS_SPRSH)
            .map(|e| BorderProperties::from_elem(e));
        let diagonal = elem.first_child_element_named_ns("diagonal", NS_SPRSH)
            .map(|e| BorderProperties::from_elem(e));
        let vertical = elem.first_child_element_named_ns("vertical", NS_SPRSH)
            .map(|e| BorderProperties::from_elem(e));
        let horizontal = elem.first_child_element_named_ns("horizontal", NS_SPRSH)
            .map(|e| BorderProperties::from_elem(e));
        Self {
            start,
            end,
            top,
            bottom,
            diagonal,
            vertical,
            horizontal,
            diagonal_up,
            diagonal_down,
            outline,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BorderProperties {
    pub color: Option<Color>,
    pub style: Option<BorderStyle>,
}
impl BorderProperties {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let color = elem.first_child_element_named_ns("color", NS_SPRSH)
            .map(|e| Color::from_elem(e));
        let style = elem.attribute_value("style")
            .and_then(|s| BorderStyle::try_from_str(s));
        Self {
            color,
            style,
        }
    }
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
impl FormattingRecord {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let number_format_id = elem.attribute_value("numFmtId")
            .and_then(|s| usize::from_str(s).ok());
        let font_id = elem.attribute_value("fontId")
            .and_then(|s| usize::from_str(s).ok());
        let fill_id = elem.attribute_value("fillId")
            .and_then(|s| usize::from_str(s).ok());
        let border_id = elem.attribute_value("borderId")
            .and_then(|s| usize::from_str(s).ok());
        let cell_formatting_record_id = elem.attribute_value("xfId")
            .and_then(|s| usize::from_str(s).ok());
        let quote_prefix = elem.attribute_value("quotePrefix")
            .and_then(|s| s.as_xsd_boolean());
        let pivot_button = elem.attribute_value("pivotButton")
            .and_then(|s| s.as_xsd_boolean());
        let apply_number_format = elem.attribute_value("applyNumberFormat")
            .and_then(|s| s.as_xsd_boolean());
        let apply_font = elem.attribute_value("applyFont")
            .and_then(|s| s.as_xsd_boolean());
        let apply_fill = elem.attribute_value("applyFill")
            .and_then(|s| s.as_xsd_boolean());
        let apply_border = elem.attribute_value("applyBorder")
            .and_then(|s| s.as_xsd_boolean());
        let apply_alignment = elem.attribute_value("applyAlignment")
            .and_then(|s| s.as_xsd_boolean());
        let apply_protection = elem.attribute_value("applyProtection")
            .and_then(|s| s.as_xsd_boolean());

        let alignment = elem.first_child_element_named_ns("alignment", NS_SPRSH)
            .map(|e| CellAlignment::from_elem(e));
        let protection = elem.first_child_element_named_ns("protection", NS_SPRSH)
            .map(|e| CellProtection::from_elem(e));

        Self {
            alignment,
            protection,
            number_format_id,
            font_id,
            fill_id,
            border_id,
            cell_formatting_record_id,
            quote_prefix,
            pivot_button,
            apply_number_format,
            apply_font,
            apply_fill,
            apply_border,
            apply_alignment,
            apply_protection,
        }
    }
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
impl CellAlignment {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let horizontal = elem.attribute_value("horizontal")
            .and_then(|s| HorizontalAlignment::try_from_str(s));
        let vertical = elem.attribute_value("vertical")
            .and_then(|s| VerticalAlignment::try_from_str(s));
        let text_rotation = elem.attribute_value("textRotation")
            .and_then(|s| u8::from_str(s).ok());
        let wrap_text = elem.attribute_value("wrapText")
            .and_then(|s| s.as_xsd_boolean());
        let indent = elem.attribute_value("indent")
            .and_then(|s| u64::from_str(s).ok());
        let relative_indent = elem.attribute_value("relativeIndent")
            .and_then(|s| i64::from_str(s).ok());
        let justify_last_line = elem.attribute_value("justifyLastLine")
            .and_then(|s| s.as_xsd_boolean());
        let shrink_to_fit = elem.attribute_value("shrinkToFit")
            .and_then(|s| s.as_xsd_boolean());
        let reading_order = elem.attribute_value("readingOrder")
            .and_then(|s| u64::from_str(s).ok())
            .map(|i| ReadingOrder::from_base_type(i));

        Self {
            horizontal,
            vertical,
            text_rotation,
            wrap_text,
            indent,
            relative_indent,
            justify_last_line,
            shrink_to_fit,
            reading_order,
        }
    }
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
#[from_to_other(base_type = u64, derive_compare = "as_int")]
pub enum ReadingOrder {
    ContextDependent = 0,
    LeftToRight = 1,
    RightToLeft = 2,
    Other(u64),
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellProtection {
    pub locked: Option<bool>,
    pub hidden: Option<bool>,
}
impl CellProtection {
    pub fn from_elem<'d>(elem: Element<'d>) -> Self {
        let locked = elem.attribute_value("locked")
            .and_then(|s| s.as_xsd_boolean());
        let hidden = elem.attribute_value("hidden")
            .and_then(|s| s.as_xsd_boolean());

        Self {
            locked,
            hidden,
        }
    }
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
    pub default_table_style: Option<String>,
    pub default_pivot_style: Option<String>,
    pub table_styles: Vec<TableStyle>,
    pub indexed_colors: Option<Vec<Rgba>>,
    pub mru_colors: Option<Vec<Color>>,
}
impl Stylesheet {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let elem = elem
            .ensure_name_ns_for_path("styleSheet", NS_SPRSH, path)?;

        let mut number_formats = Vec::new();
        let num_fmt_elems = elem
            .grandchild_elements_named_ns(
                "numFmts", NS_SPRSH,
                "numFmt", NS_SPRSH,
            );
        for num_fmt_elem in num_fmt_elems {
            let number_format = NumberFormat::try_from_elem(num_fmt_elem, path)?;
            number_formats.push(number_format);
        }

        let mut fonts = Vec::new();
        let font_elems = elem
            .grandchild_elements_named_ns(
                "fonts", NS_SPRSH,
                "font", NS_SPRSH,
            );
        for font_elem in font_elems {
            let font = Font::from_elem(font_elem);
            fonts.push(font);
        }

        let mut fills = Vec::new();
        let fill_elems = elem
            .grandchild_elements_named_ns(
                "fills", NS_SPRSH,
                "fill", NS_SPRSH,
            );
        for fill_elem in fill_elems {
            let fill = Fill::try_from_elem(fill_elem, path)?;
            fills.push(fill);
        }

        let mut borders = Vec::new();
        let border_elems = elem
            .grandchild_elements_named_ns(
                "borders", NS_SPRSH,
                "border", NS_SPRSH,
            );
        for border_elem in border_elems {
            let border = Border::from_elem(border_elem);
            borders.push(border);
        }

        let mut cell_style_formatting_records = Vec::new();
        let csfr_elems = elem
            .grandchild_elements_named_ns(
                "cellStyleXfs", NS_SPRSH,
                "xf", NS_SPRSH,
            );
        for csfr_elem in csfr_elems {
            let formatting_record = FormattingRecord::from_elem(csfr_elem);
            cell_style_formatting_records.push(formatting_record);
        }

        let mut cell_formatting_records = Vec::new();
        let cfr_elems = elem
            .grandchild_elements_named_ns(
                "cellXfs", NS_SPRSH,
                "xf", NS_SPRSH,
            );
        for cfr_elem in cfr_elems {
            let formatting_record = FormattingRecord::from_elem(cfr_elem);
            cell_formatting_records.push(formatting_record);
        }

        let mut cell_styles = Vec::new();
        let cell_style_elems = elem
            .grandchild_elements_named_ns(
                "cellStyles", NS_SPRSH,
                "cellStyle", NS_SPRSH,
            );
        for cell_style_elem in cell_style_elems {
            let cell_style = CellStyle::try_from_elem(cell_style_elem, path)?;
            cell_styles.push(cell_style);
        }

        let mut differential_formatting_records = Vec::new();
        let dfr_elems = elem
            .grandchild_elements_named_ns(
                "dxfs", NS_SPRSH,
                "dxf", NS_SPRSH,
            );
        for dfr_elem in dfr_elems {
            let dfr = DifferentialFormattingRecord::try_from_elem(dfr_elem, path)?;
            differential_formatting_records.push(dfr);
        }

        let mut default_table_style = None;
        let mut default_pivot_style = None;
        let mut table_styles = Vec::new();
        let table_styles_elem_opt = elem
            .first_child_element_named_ns("tableStyles", NS_SPRSH);
        if let Some(table_styles_elem) = table_styles_elem_opt {
            default_table_style = table_styles_elem.attribute_value("defaultTableStyle")
                .map(|ts| ts.to_owned());
            default_pivot_style = table_styles_elem.attribute_value("defaultPivotStyle")
                .map(|ps| ps.to_owned());
            let table_style_elems = table_styles_elem
                .child_elements_named_ns(
                    "tableStyle", NS_SPRSH,
                );
            for table_style_elem in table_style_elems {
                let table_style = TableStyle::try_from_elem(table_style_elem, path)?;
                table_styles.push(table_style);
            }
        }

        let mut indexed_colors = None;
        let mut mru_colors = None;
        let color_elem_opt = elem
            .first_child_element_named_ns("colors", NS_SPRSH);
        if let Some(color_elem) = color_elem_opt {
            let indexed_colors_elem_opt = color_elem
                .first_child_element_named_ns("indexedColors", NS_SPRSH);
            if let Some(indexed_colors_elem) = indexed_colors_elem_opt {
                let mut indexed_colors_vec = Vec::new();
                let indexed_color_elems = indexed_colors_elem
                    .child_elements_named_ns("rgbColor", NS_SPRSH);
                for indexed_color_elem in indexed_color_elems {
                    let rgb_color = Rgba::try_from_elem(indexed_color_elem, path)?;
                    indexed_colors_vec.push(rgb_color);
                }
                indexed_colors = Some(indexed_colors_vec);
            }
            let mru_colors_elem_opt = color_elem
                .first_child_element_named_ns("mruColors", NS_SPRSH);
            if let Some(mru_colors_elem) = mru_colors_elem_opt {
                let mut mru_colors_vec = Vec::new();
                let mru_color_elems = mru_colors_elem
                    .child_elements_named_ns("color", NS_SPRSH);
                for mru_color_elem in mru_color_elems {
                    let color = Color::from_elem(mru_color_elem);
                    mru_colors_vec.push(color);
                }
                mru_colors = Some(mru_colors_vec);
            }
        }

        Ok(Self {
            number_formats,
            fonts,
            fills,
            borders,
            cell_style_formatting_records,
            cell_formatting_records,
            cell_styles,
            differential_formatting_records,
            default_table_style,
            default_pivot_style,
            table_styles,
            indexed_colors,
            mru_colors,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NumberFormat {
    pub format_id: usize,
    pub format_code: String,
}
impl NumberFormat {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let format_id: usize = elem.required_attribute_value_in_format(
            "numFmtId",
            path,
            |s| s.parse().ok(),
            "unsigned int",
        )?;
        let format_code = elem.required_attribute_value("formatCode", path)?
            .to_owned();
        Ok(Self {
            format_id,
            format_code,
        })
    }
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
impl CellStyle {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let name = elem.attribute_value("name")
            .map(|s| s.to_owned());
        let formatting_record_id: usize = elem.required_attribute_value_in_format(
            "xfId",
            path,
            |s| s.parse().ok(),
            "unsigned int",
        )?;
        let builtin_id = elem.attribute_value("builtinId")
            .and_then(|s| usize::from_str(s).ok());
        let i_level = elem.attribute_value("iLevel")
            .and_then(|s| u64::from_str(s).ok());
        let hidden = elem.attribute_value("hidden")
            .and_then(|s| s.as_xsd_boolean());
        let custom_builtin = elem.attribute_value("customBuiltin")
            .and_then(|s| s.as_xsd_boolean());
            Ok(Self {
                name,
                formatting_record_id,
                builtin_id,
                i_level,
                hidden,
                custom_builtin,
            })
    }
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
impl DifferentialFormattingRecord {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let font = elem.first_child_element_named_ns("font", NS_SPRSH)
            .map(|e| Font::from_elem(e));
        let number_format = elem.first_child_element_named_ns("numFmt", NS_SPRSH)
            .map(|e| NumberFormat::try_from_elem(e, path))
            .transpose()?;
        let fill = elem.first_child_element_named_ns("fill", NS_SPRSH)
            .map(|e| Fill::try_from_elem(e, path))
            .transpose()?;
        let alignment = elem.first_child_element_named_ns("alignment", NS_SPRSH)
            .map(|e| CellAlignment::from_elem(e));
        let border = elem.first_child_element_named_ns("border", NS_SPRSH)
            .map(|e| Border::from_elem(e));
        let protection = elem.first_child_element_named_ns("protection", NS_SPRSH)
            .map(|e| CellProtection::from_elem(e));
        Ok(Self {
            font,
            number_format,
            fill,
            alignment,
            border,
            protection,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TableStyle {
    pub name: String,
    pub pivot: Option<bool>,
    pub table: Option<bool>,
    pub elements: Vec<TableStyleElement>,
}
impl TableStyle {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let name = elem.attribute_value("name")
            .ok_or_else(|| Error::MissingRequiredAttribute {
                path: path.to_owned(),
                element_name: elem.name().into(),
                attribute_name: QualifiedName::new_bare("name"),
            })?
            .to_owned();
        let pivot = elem.attribute_value("pivot")
            .as_xsd_boolean();
        let table = elem.attribute_value("table")
            .as_xsd_boolean();

        let mut elements = Vec::new();
        let table_style_elems = elem
            .child_elements_named_ns("tableStyleElement", NS_SPRSH);
        for table_style_elem in table_style_elems {
            let element = TableStyleElement::try_from_elem(table_style_elem, path)?;
            elements.push(element);
        }
        Ok(Self {
            name,
            pivot,
            table,
            elements,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TableStyleElement {
    pub style_type: TableStyleType,
    pub size: Option<u64>,
    pub differential_formatting_record_id: Option<usize>,
}
impl TableStyleElement {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let style_type: TableStyleType = elem.required_attribute_value_in_format(
            "type",
            path,
            |s| TableStyleType::try_from_str(s),
            "ST_TableStyleType enum",
        )?;
        let size: Option<u64> = elem.attribute_value("size")
            .and_then(|s| s.parse().ok());
        let differential_formatting_record_id: Option<usize> = elem.attribute_value("dxfId")
            .and_then(|s| s.parse().ok());
        Ok(Self {
            style_type,
            size,
            differential_formatting_record_id,
        })
    }
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
