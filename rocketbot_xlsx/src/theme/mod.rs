#![cfg(feature = "theme")]
pub(crate) mod enums;
mod simple_types;


use sxd_document::dom::Element;

use crate::Error;
pub use crate::theme::enums::color::{
    ColorSchemeIndex, PresetColorValue, SchemeColorValue, SystemColorValue,
};
pub use crate::theme::enums::text::{
    FontCollectionIndex, TextAutoNumberScheme, TextCapsType, TextStrikeType, TextTabAlignType,
    TextUnderlineType,
};
pub use crate::theme::enums::three_d::{
    BevelPresetType, LightRigDirection, LightRigType, PresetCameraType, PresetMaterialType,
};
pub use crate::theme::enums::two_d::{BlackWhiteMode, PresetPatternValue, ShapeType, TextAlignType,
    TextAnchoringType, TextFontAlignType, TextHorizontalOverflowType, TextShapeType,
    TextVerticalOverflowType, TextVerticalType, TextWrappingType,
};
pub use crate::theme::simple_types::{
    Angle, Coordinate, Coordinate32, FixedAngle, FixedPercentage, FovAngle, LineWidth, Panose,
    Percentage, PitchFamily, PositiveCoordinate, PositiveCoordinate32, PositiveFixedAngle,
    PositiveFixedPercentage, PositivePercentage, TextBulletSizePercent, TextBulletStartAtNumber,
    TextColumnCount, TextFontSize, TextIndent, TextIndentLevelType, TextMargin,
    TextNonNegativePoint, TextPoint, TextSpacingPoint, TileFlipMode,
};
use crate::xml::{ElemExt, NS_DRAWINGML};


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Theme {
    // <{http://schemas.openxmlformats.org/drawingml/2006/main}theme>
    pub name: Option<String>, // @name? xsd:string
    pub elements: BaseStyles, // <themeElements> a_CT_BaseStyles
    pub object_defaults: Option<ObjectStyleDefaults>, // <objectDefaults>? a_CT_ObjectStyleDefaults
    pub extra_color_scheme_list: Option<Vec<ColorSchemeAndMapping>>, // <extraClrSchemeLst>? -> <extraClrScheme>* a_CT_ColorSchemeAndMapping
    pub custom_color_list: Option<Vec<CustomColor>>, // <custClrLst>? -> <custClr>* a_CT_CustomColor
}
impl Theme {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let name = elem
            .attribute_value("name")
            .map(|s| s.to_owned());
        let elements = elem.required_first_child_element_named_ns("themeElements", NS_DRAWINGML, path)?;
        let object_defaults = elem.first_child_element_named_ns("objectDefaults", NS_DRAWINGML)
            .and_then(|e| ObjectStyleDefaults::try_from_elem(e, path))
            .transpose()?;

        let extra_color_scheme_list = if let Some(extra_color_scheme_list_elem) = elem.first_child_element_named_ns("extraClrSchemeLst", NS_DRAWINGML) {
            let mut ecsl = Vec::new();
            let ecs_elems = extra_color_scheme_list_elem.child_elements_named_ns("extraClrScheme", NS_DRAWINGML);
            for ecs_elem in ecs_elems {
                let ecs = ColorSchemeAndMapping::try_from_elem(ecs_elem, path)?;
                ecsl.push(ecs);
            }
            Some(ecsl)
        } else {
            None
        };

        let custom_color_list = if let Some(custom_color_list_elem) = elem.first_child_element_named_ns("custClrLst", NS_DRAWINGML) {
            let mut ccl = Vec::new();
            let cc_elems = custom_color_list_elem.child_elements_named_ns("custClr", NS_DRAWINGML);
            for cc_elem in cc_elems {
                let cc = CustomColor::try_from_elem(cc_elem, path)?;
                ccl.push(cc);
            }
            Some(ccl)
        } else {
            None
        };

        Ok(Self {
            name,
            elements,
            object_defaults,
            extra_color_scheme_list,
            custom_color_list,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BaseStyles {
    pub color_scheme: ColorScheme, // <clrScheme> a_CT_ColorScheme
    pub font_scheme: FontScheme, // <fontScheme> a_CT_FontScheme
    pub format_scheme: StyleMatrix, // <fmtScheme> a_CT_StyleMatrix
}
impl BaseStyles {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let color_scheme_elem = elem
            .required_first_child_element_named_ns("clrScheme", NS_DRAWINGML, path)?;
        let color_scheme = ColorScheme::try_from_elem(color_scheme_elem, path);

        let font_scheme_elem = elem
            .required_first_child_element_named_ns("fontScheme", NS_DRAWINGML, path)?;
        let font_scheme = FontScheme::try_from_elem(font_scheme_elem, path);

        let format_scheme_elem = elem
            .required_first_child_element_named_ns("fmtScheme", NS_DRAWINGML, path)?;
        let format_scheme = StyleMatrix::try_from_elem(format_scheme_elem, path);

        Ok(Self {
            color_scheme,
            font_scheme,
            format_scheme,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ColorScheme {
    pub name: String, // @name xsd:string
    pub dark1: Color, // <dk1> a_CT_Color
    pub light1: Color, // <lt1> a_CT_Color
    pub dark2: Color, // <dk2> a_CT_Color
    pub light2: Color, // <lt2> a_CT_Color
    pub accent1: Color, // <accent1> a_CT_Color
    pub accent2: Color, // <accent2> a_CT_Color
    pub accent3: Color, // <accent3> a_CT_Color
    pub accent4: Color, // <accent4> a_CT_Color
    pub accent5: Color, // <accent5> a_CT_Color
    pub accent6: Color, // <accent6> a_CT_Color
    pub hyperlink: Color, // <hlink> a_CT_Color
    pub followed_hyperlink: Color, // <folHlink> a_CT_Color
}
impl ColorScheme {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let name = elem.attribute_value("name")
            .map(|s| s.to_owned());

        let dark1_elem = elem.required_first_child_element_named_ns("dk1", NS_DRAWINGML, path)?;
        let dark1 = Color::try_from_elem(dark1_elem, path)?;
        let light1_elem = elem.required_first_child_element_named_ns("lt1", NS_DRAWINGML, path)?;
        let light1 = Color::try_from_elem(light1_elem, path)?;
        let dark2_elem = elem.required_first_child_element_named_ns("dk2", NS_DRAWINGML, path)?;
        let dark2 = Color::try_from_elem(dark2_elem, path)?;
        let light2_elem = elem.required_first_child_element_named_ns("lt2", NS_DRAWINGML, path)?;
        let light2 = Color::try_from_elem(light2_elem, path)?;
        let accent1_elem = elem.required_first_child_element_named_ns("accent1", NS_DRAWINGML, path)?;
        let accent1 = Color::try_from_elem(accent1_elem, path)?;
        let accent2_elem = elem.required_first_child_element_named_ns("accent2", NS_DRAWINGML, path)?;
        let accent2 = Color::try_from_elem(accent2_elem, path)?;
        let accent3_elem = elem.required_first_child_element_named_ns("accent3", NS_DRAWINGML, path)?;
        let accent3 = Color::try_from_elem(accent3_elem, path)?;
        let accent4_elem = elem.required_first_child_element_named_ns("accent4", NS_DRAWINGML, path)?;
        let accent4 = Color::try_from_elem(accent4_elem, path)?;
        let accent5_elem = elem.required_first_child_element_named_ns("accent5", NS_DRAWINGML, path)?;
        let accent5 = Color::try_from_elem(accent5_elem, path)?;
        let accent6_elem = elem.required_first_child_element_named_ns("accent6", NS_DRAWINGML, path)?;
        let accent6 = Color::try_from_elem(accent6_elem, path)?;
        let hyperlink_elem = elem.required_first_child_element_named_ns("hlink", NS_DRAWINGML, path)?;
        let hyperlink = Color::try_from_elem(hyperlink_elem, path)?;
        let followed_hyperlink_elem = elem.required_first_child_element_named_ns("folHlink", NS_DRAWINGML, path)?;
        let followed_hyperlink = Color::try_from_elem(followed_hyperlink_elem, path)?;

        Ok(Self {
            dark1,
            light1,
            dark2,
            light2,
            accent1,
            accent2,
            accent3,
            accent4,
            accent5,
            accent6,
            hyperlink,
            followed_hyperlink,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Color {
    // a_CT_Color / a_EG_ColorChoice
    ScRgb(ScRgbColor), // <scrgbClr> a_CT_ScRgbColor
    SRgb(SRgbColor), // <srgbClr> a_CT_SRgbColor
    Hsl(HslColor), // <hslClr> a_CT_HslColor
    System(SystemColor), // <sysClr> a_CT_SystemColor
    Scheme(SchemeColor), // <schemeClr> a_CT_SchemeColor
    Preset(PresetColor), // <prstClr> a_CT_PresetColor
}
impl Color {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Option<Self>, Error> {
        if let Some(sc_rgb_elem) = elem.first_child_element_named_ns("scrgbClr", NS_DRAWINGML) {
            let sc_rgb = ScRgbColor::try_from_elem(sc_rgb_elem, path)?;
            Ok(Some(Self::ScRgb(sc_rgb)))
        } else if let Some(s_rgb_elem) = elem.first_child_element_named_ns("srgbClr", NS_DRAWINGML) {
            let s_rgb = SRgbColor::try_from_elem(s_rgb_elem, path)?;
            Ok(Some(Self::SRgb(s_rgb)))
        } else if let Some(hsl_elem) = elem.first_child_element_named_ns("hslClr", NS_DRAWINGML) {
            let hsl = HslColor::try_from_elem(hsl_elem, path)?;
            Ok(Some(Self::Hsl(hsl)))
        } else if let Some(sys_elem) = elem.first_child_element_named_ns("sysClr", NS_DRAWINGML) {
            let sys = HslColor::try_from_elem(sys_elem, path)?;
            Ok(Some(Self::System(sys)))
        } else if let Some(scheme_elem) = elem.first_child_element_named_ns("schemeClr", NS_DRAWINGML) {
            let scheme = SchemeColor::try_from_elem(scheme_elem, path)?;
            Ok(Some(Self::System(scheme)))
        } else if let Some(preset_elem) = elem.first_child_element_named_ns("prstClr", NS_DRAWINGML) {
            let preset = PresetColor::try_from_elem(preset_elem, path)?;
            Ok(Some(Self::Preset(preset)))
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScRgbColor {
    pub r: Percentage, // @r s_ST_Percentage
    pub g: Percentage, // @g s_ST_Percentage
    pub b: Percentage, // @b s_ST_Percentage
    pub transforms: Vec<ColorTransform>, // <...>* a_EG_ColorTransform
}
impl ScRgbColor {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Self, Error> {
        let r = elem.required_attribute_value_in_format(
            "r",
            path,
            |s| Percentage::try_from_str(s),
            "percentage",
        )?;
        let g = elem.required_attribute_value_in_format(
            "g",
            path,
            |s| Percentage::try_from_str(s),
            "percentage",
        )?;
        let b = elem.required_attribute_value_in_format(
            "b",
            path,
            |s| Percentage::try_from_str(s),
            "percentage",
        )?;

        let mut transforms = Vec::new();
        for child in elem.child_elements() {
            let transform_opt = ColorTransform::try_from_elem(elem, path)?;
            if let Some(transform) = transform_opt {
                transforms.push(transform);
            }
        }

        Ok(Self {
            r,
            g,
            b,
            transforms,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ColorTransform {
    Tint(PositiveFixedPercentage), // <tint val="a_ST_PositiveFixedPercentage" />
    Shade(PositiveFixedPercentage), // <shade val="a_ST_PositiveFixedPercentage" />
    Complement, // <comp /> empty
    Inverse, // <inv /> empty
    Grayscale, // <gray /> empty
    Alpha(PositiveFixedPercentage), // <alpha val="a_ST_PositiveFixedPercentage" />
    AlphaOff(FixedPercentage), // <alphaOff val="a_ST_FixedPercentage" />
    AlphaMod(PositivePercentage), // <alphaMod val="a_ST_PositivePercentage" />
    Hue(PositiveFixedAngle), // <hue val="a_ST_PositiveFixedAngle" />
    HueOff(Angle), // <hueOff val="a_ST_Angle" />
    HueMod(PositivePercentage), // <hueMod val="a_ST_PositivePercentage" />
    Sat(Percentage), // <sat val="s_ST_Percentage" />
    SatOff(Percentage), // <satOff val="s_ST_Percentage" />
    SatMod(Percentage), // <satMod val="s_ST_Percentage" />
    Lum(Percentage), // <lum val="s_ST_Percentage" />
    LumOff(Percentage), // <lumOff val="s_ST_Percentage" />
    LumMod(Percentage), // <lumMod val="s_ST_Percentage" />
    Red(Percentage), // <red val="s_ST_Percentage" />
    RedOff(Percentage), // <redOff val="s_ST_Percentage" />
    RedMod(Percentage), // <redMod val="s_ST_Percentage" />
    Green(Percentage), // <green val="s_ST_Percentage" />
    GreenOff(Percentage), // <greenOff val="s_ST_Percentage" />
    GreenMod(Percentage), // <greenMod val="s_ST_Percentage" />
    Blue(Percentage), // <blue val="s_ST_Percentage" />
    BlueOff(Percentage), // <blueOff val="s_ST_Percentage" />
    BlueMod(Percentage), // <blueMod val="s_ST_Percentage" />
    Gamma, // <gamma /> empty
    InverseGamma, // <invGamma /> empty
}
impl ColorTransform {
    pub fn try_from_elem<'d>(elem: Element<'d>, path: &str) -> Result<Option<Self>, Error> {
        if let Some(tint_elem) = elem.first_child_element_named_ns("tint", NS_DRAWINGML) {
            let tint = elem.required_attribute_value_in_format(
                "val",
                path,
                |s| PositiveFixedPercentage::try_from_str(s),
                "positive fixed percentage",
            )?;
            Ok(Self::Tint(tint))
        } else if let Some(shade_elem) = elem.first_child_element_named_ns("shade", NS_DRAWINGML) {
            let shade = elem.required_attribute_value_in_format(
                "val",
                path,
                |s| PositiveFixedPercentage::try_from_str(s),
                "positive fixed percentage",
            )?;
            Ok(Self::Shade(shade))
        } else if elem.first_child_element_named_ns("comp", NS_DRAWINGML).is_some() {
            Ok(Self::Complement)
        } else if elem.first_child_element_named_ns("inv", NS_DRAWINGML).is_some() {
            Ok(Self::Inverse)
        } else if elem.first_child_element_named_ns("gray", NS_DRAWINGML).is_some() {
            Ok(Self::Grayscale)
        } else if let Some(alpha_elem) = elem.first_child_element_named_ns("alpha", NS_DRAWINGML) {
            let alpha = elem.required_attribute_value_in_format(
                "val",
                path,
                |s| PositiveFixedPercentage::try_from_str(s),
                "positive fixed percentage",
            )?;
            Ok(Self::Alpha(alpha))
        } else if let Some(alpha_off_elem) = elem.first_child_element_named_ns("alphaOff", NS_DRAWINGML) {
            let alpha_off = elem.required_attribute_value_in_format(
                "val",
                path,
                |s| FixedPercentage::try_from_str(s),
                "fixed percentage",
            )?;
            Ok(Self::AlphaOff(alpha_off))
        } else if let Some(alpha_mod_elem) = elem.first_child_element_named_ns("alphaMod", NS_DRAWINGML) {
            let alpha_mod = elem.required_attribute_value_in_format(
                "val",
                path,
                |s| PositivePercentage::try_from_str(s),
                "positive percentage",
            )?;
            Ok(Self::AlphaMod(alpha_mod))
        } else if let Some(hue_elem) = elem.first_child_element_named_ns("hue", NS_DRAWINGML) {
            let hue = elem.required_attribute_value_in_format(
                "val",
                path,
                |s| PositiveFixedAngle::try_from_str(s),
                "positive fixed angle",
            )?;
            Ok(Self::Hue(hue))
        } else if let Some(hue_off_elem) = elem.first_child_element_named_ns("hueOff", NS_DRAWINGML) {
            let hue_off = elem.required_attribute_value_in_format(
                "val",
                path,
                |s| Angle::try_from_str(s),
                "angle",
            )?;
            Ok(Self::HueOff(hue_off))
        } else if let Some(hue_mod_elem) = elem.first_child_element_named_ns("hueMod", NS_DRAWINGML) {
            let hue_mod = elem.required_attribute_value_in_format(
                "val",
                path,
                |s| PositivePercentage::try_from_str(s),
                "positive percentage",
            )?;
            Ok(Self::HueMod(hue_mod))
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Rgb {
    // s_ST_HexColorRGB = "RRGGBB" as hex
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SRgbColor {
    pub rgb: Rgb, // @val s_ST_HexColorRGB
    pub transforms: Vec<ColorTransform>, // <...>* a_EG_ColorTransform
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HslColor {
    pub hue: PositiveFixedAngle, // @hue a_ST_PositiveFixedAngle
    pub saturation: Percentage, // @sat s_ST_Percentage
    pub luminosity: Percentage, // @lum s_ST_Percentage
    pub transforms: Vec<ColorTransform>, // <...>* a_EG_ColorTransform
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SystemColor {
    pub value: SystemColorValue,
    pub last_color: Option<Rgb>, // <lastClr>? s_ST_HexColorRGB
    pub transforms: Vec<ColorTransform>, // <...>* a_EG_ColorTransform
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemeColor {
    pub value: SchemeColorValue, // @val a_ST_SchemeColorVal
    pub transforms: Vec<ColorTransform>, // <...>* a_EG_ColorTransform
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresetColor {
    pub value: PresetColorValue, // @val a_ST_PresetColorVal
    pub transforms: Vec<ColorTransform>, // <...>* a_EG_ColorTransform
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontScheme {
    pub name: String, // @name xsd:string
    pub major_font: FontCollection,
    pub minor_font: FontCollection,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontCollection {
    pub latin: TextFont, // <latin> a_CT_TextFont
    pub ea: TextFont, // <ea> a_CT_TextFont
    pub cs: TextFont, // <cs> a_CT_TextFont
    pub fonts: Vec<SupplementalFont>, // <font>* a_CT_SupplementalFont
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextFont {
    pub typeface: String, // @typeface a_ST_TextTypeface
    pub panose: Option<Panose>, // @panose? s_ST_Panose
    pub pitch_family: Option<PitchFamily>, // @pitchFamily? a_ST_PitchFamily
    pub charset: Option<u8>, // @charset? xsd::byte
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SupplementalFont {
    pub script: String, // @script xsd:string
    pub typeface: String, // @typeface a_ST_TextTypeface
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StyleMatrix {
    pub name: Option<String>, // @name? xsd:string
    pub fill_style_list: Vec<FillProperties>, // <fillStyleLst> -> <...> a_EG_FillProperties+
    pub line_style_list: Vec<LineProperties>, // <lnStyleLst> -> <ln>+ -> a_CT_LineProperties
    pub effect_style_list: Vec<EffectStyleItem>, // <effectStyleLst> -> <effectStyle>+ -> a_CT_EffectStyleItem
    pub background_fill_style_list: Vec<FillProperties>, // <bgFillStyleLst> -> <...> a_EG_FillProperties
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FillProperties {
    // a_EG_FillProperties
    No, // <noFill /> empty
    Solid(Option<Color>), // <solidFill> a_EG_ColorChoice?
    Gradient(GradientFillProperties), // <gradFill> a_CT_GradientFillProperties
    Blip(BlipFillProperties), // <blipFill> a_CT_BlipFillProperties
    Pattern(PatternFillProperties), // <patFill> a_CT_PatternFillProperties
    GroupFill, // <grpFill /> empty
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GradientFillProperties {
    pub flip: Option<TileFlipMode>, // @flip a_ST_TileFlipMode
    pub rotate_with_shape: Option<bool>, // @rotWithShape xsd:boolean
    pub gradient_stop_list: Option<Vec<GradientStop>>, // <gsLst> a_CT_GradientStopList -> <gs>+ a_CT_GradientStop
    pub shade_properties: Option<ShadeProperties>, // <...> a_EG_ShadeProperties
    pub tile_rect: Option<RelativeRect>, // <tileRect> a_CT_RelativeRect
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GradientStop {
    pub position: PositiveFixedPercentage, // @pos
    pub color: Color, // <...> a_EG_ColorChoice
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShadeProperties {
    Linear(LinearShadeProperties), // <lin /> a_CT_LinearShadeProperties
    Path(PathShadeProperties), // <path> a_CT_PathShadeProperties
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinearShadeProperties {
    pub angle: Option<PositiveFixedAngle>, // @ang? a_ST_PositiveFixedAngle
    pub scaled: Option<bool>, // @scaled? xsd:boolean
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PathShadeProperties {
    pub path: Option<PathShadeType>, // @path? a_ST_PathShadeType
    pub flll_to_rect: Option<RelativeRect>, // <fillToRect>? a_CT_RelativeRect
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PathShadeType {
    Shape,
    Circle,
    Rect,
}
impl PathShadeType {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "shape" => Some(Self::Shape),
            "circle" => Some(Self::Circle),
            "rect" => Some(Self::Rect),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RelativeRect {
    pub left: Option<Percentage>, // @l? a_ST_Percentage
    pub top: Option<Percentage>, // @t? a_ST_Percentage
    pub right: Option<Percentage>, // @r? a_ST_Percentage
    pub bottom: Option<Percentage>, // @b? a_ST_Percentage
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlipFillProperties {
    pub dpi: Option<u64>, // @dpi? xsd:unsignedInt
    pub rotate_with_shape: Option<bool>, // @rotWithShape? xsd:boolean
    pub blip: Option<Blip>, // <blip>? a_CT_Blip
    pub source_rectangle: Option<RelativeRect>, // <srcRect>? a_CT_RelativeRect
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Blip {
    pub blob: Blob, // @r:embed, @r:link (see there)
    pub compression_state: Option<BlipCompression>, // @cstate? a_ST_BlipCompression
    pub blip_effects: Vec<BlipEffect>, // <...>*
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Blob {
    pub embed: Option<String>, // @r:embed r_ST_RelationshipId
    pub link: Option<String>, // @r:link r_ST_RelationshipId
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BlipCompression {
    Email,
    Screen,
    Print,
    HighQualityPrint,
    None,
}
impl BlipCompression {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "email" => Some(Self::Email),
            "screen" => Some(Self::Screen),
            "print" => Some(Self::Print),
            "hqprint" => Some(Self::HighQualityPrint),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BlipEffect {
    AlphaBiLevel(PositiveFixedPercentageThreshold), // <alphaBiLevel thresh="a_ST_PositiveFixedPercentage" />
    AlphaCeiling, // <alphaCeiling /> empty
    AlphaFloor, // <alphaFloor /> empty
    AlphaInverse(Option<Color>), // <alphaInv> a_EG_ColorChoice?
    AlphaModulate(EffectContainer), // <alphaMod> -> <cont> a_CT_EffectContainer
    AlphaModulateFixed { amount: PositivePercentage }, // <alphaModFix amt="a_ST_PositivePercentage" />
    AlphaReplace { new_alpha: PositiveFixedPercentage }, // <alphaRepl a="a_ST_PositiveFixedPercentage" />
    BiLevel(PositiveFixedPercentageThreshold), // <biLevel thresh="a_ST_PositiveFixedPercentage" />
    Blur(BlurEffect), // <blur /> a_CT_BlurEffect
    ColorChange(ColorChangeEffect), // <clrChange> a_CT_ColorChangeEffect
    ColorReplace(Color), // <clrRepl> -> <...> a_EG_ColorChoice
    Duotone(Vec<Color>), // <duotone> -> <...>+ a_EG_ColorChoice
    FillOverlay(FillOverlayEffect), // <fillOverlay> a_CT_FillOverlayEffect
    Grayscale, // <grayscl /> empty
    Hsl(HslEffect), // <hsl /> a_CT_HSLEffect
    Luminance(LuminanceEffect), // <lum /> a_CT_LuminanceEffect
    Tint(TintEffect), // <tint /> a_CT_TintEffect
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositiveFixedPercentageThreshold {
    // a_CT_AlphaBiLevelEffect, a_CT_BiLevelEffect
    pub threshold: PositiveFixedPercentage, // @thresh a_ST_PositiveFixedPercentage
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EffectContainer {
    pub container_type: Option<EffectContainerType>, // @type? a_ST_EffectContainerType
    pub name: Option<String>, // @name? xsd:token
    pub effects: Vec<Effect>, // <...>* a_EG_Effect
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EffectContainerType {
    Sib,
    Tree,
}
impl EffectContainerType {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "sib" => Some(Self::Sib),
            "tree" => Some(Self::Tree),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Effect {
    Container(EffectContainer), // <cont> a_CT_EffectContainer
    Effect(String), // <effect ref="xsd:token" />
    AlphaBiLevel(PositiveFixedPercentageThreshold), // <alphaBiLevel /> a_CT_AlphaBiLevelEffect
    AlphaCeiling, // <alphaCeiling /> empty
    AlphaFloor, // <alphaFloor /> empty
    AlphaInverse(Option<Color>), // <alphaInv> a_EG_ColorChoice?
    AlphaModulate(EffectContainer), // <alphaMod> -> <cont> a_CT_EffectContainer
    AlphaModulateFixed(PositivePercentageAmount), // <alphaModFix amt="a_ST_PositivePercentage" />
    AlphaOutset(Option<CoordinateRadius>), // <alphaOutset rad?="a_ST_Coordinate" />
    AlphaReplace(PositiveFixedPercentage), // <alphaRepl a="a_ST_PositiveFixedPercentage" />
    BiLevel(PositiveFixedPercentageThreshold), // <biLevel thresh="a_ST_PositiveFixedPercentage" />
    Blend(BlendEffect), // <blend> a_CT_BlendEffect
    Blur(BlurEffect), // <blur> a_CT_BlurEffect
    ColorChange(ColorChangeEffect), // <clrChange> a_CT_ColorChangeEffect
    ColorReplace(Color), // <clrRepl> -> <...> a_EG_ColorChoice
    Duotone(Vec<Color>), // <duotone> -> <...>+ a_EG_ColorChoice
    Fill(FillProperties), // <fill> -> <...> a_EG_FillProperties
    FillOverlay(FillOverlayEffect), // <fillOverlay> a_CT_FillOverlayEffect
    Glow(GlowEffect), // <glow> a_CT_GlowEffect
    Grayscale, // <grayscl /> empty
    Hsl(HslEffect), // <hsl> a_CT_HSLEffect
    InnerShadow(InnerShadowEffect), // <innerShdw> a_CT_InnerShadowEffect
    Luminance(LuminanceEffect), // <lum> a_CT_LuminanceEffect
    OuterShadow(OuterShadowEffect), // <outerShdw> a_CT_OuterShadowEffect
    PresetShadow(PresetShadowEffect), // <prstShdw> a_CT_PresetShadowEffect
    Reflection(ReflectionEffect), // <reflection> a_CT_ReflectionEffect
    RelativeOffset(RelativeOffsetEffect), // <relOff> a_CT_RelativeOffsetEffect
    SoftEdge(SoftEdgesEffect), // <softEdge> a_CT_SoftEdgesEffect
    Tint(TintEffect), // <tint> a_CT_TintEffect
    Transform(TransformEffect), // <xfrm> a_CT_TransformEffect
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositivePercentageAmount {
    pub amount: Option<PositivePercentage>, // @amt? a_ST_PositivePercentage
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CoordinateRadius {
    pub radius: Option<Coordinate>, // @rad? a_ST_Coordinate
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlendEffect {
    pub blend_mode: BlendMode, // @blend a_ST_BlendMode
    pub container: EffectContainer, // <cont> a_CT_EffectContainer
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BlendMode {
    Over,
    Multiply,
    Screen,
    Darken,
    Lighten,
}
impl BlendMode {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "over" => Some(Self::Over),
            "mult" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "darken" => Some(Self::Darken),
            "lighten" => Some(Self::Lighten),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlurEffect {
    pub radius: Option<PositiveCoordinate>, // @rad? a_ST_PositiveCoordinate
    pub grow: Option<bool>, // @grow? xsd:boolean
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ColorChangeEffect {
    pub use_alpha: Option<bool>, // @useA? xsd:boolean
    pub color_from: Color, // <clrFrom> a_CT_Color
    pub color_to: Color, // <clrTo> a_CT_Color
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FillOverlayEffect {
    pub blend: BlendMode, // @blend a_ST_BlendMode
    pub fill_properties: FillProperties, // <...> a_EG_FillProperties
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GlowEffect {
    pub radius: Option<PositiveCoordinate>, // @rad? a_ST_PositiveCoordinate
    pub color: Color, // <...> a_EG_ColorChoice
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HslEffect {
    pub hue: Option<PositiveFixedAngle>, // @hue? a_ST_PositiveFixedAngle
    pub saturation: Option<FixedPercentage>, // @sat? a_ST_FixedPercentage
    pub luminance: Option<FixedPercentage>, // @lum? a_ST_FixedPercentage
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InnerShadowEffect {
    pub blur_radius: Option<PositiveCoordinate>, // @blurRad? a_ST_PositiveCoordinate
    pub distance: Option<PositiveCoordinate>, // @dist? a_ST_PositiveCoordinate
    pub direction: Option<PositiveFixedAngle>, // @dir? a_ST_PositiveFixedAngle
    pub color: Color, // <...> a_EG_ColorChoice
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LuminanceEffect {
    pub brightness: FixedPercentage, // @bright? a_ST_FixedPercentage
    pub contrast: FixedPercentage, // @contrast? a_ST_FixedPercentage
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OuterShadowEffect {
    pub blur_radius: Option<PositiveCoordinate>, // @blurRad? a_ST_PositiveCoordinate
    pub distance: Option<PositiveCoordinate>, // @dist? a_ST_PositiveCoordinate
    pub direction: Option<PositiveFixedAngle>, // @dir? a_ST_PositiveFixedAngle
    pub sx: Option<Percentage>, // @sx? a_ST_Percentage
    pub sy: Option<Percentage>, // @sy? a_ST_Percentage
    pub kx: Option<FixedAngle>, // @kx? a_ST_FixedAngle
    pub ky: Option<FixedAngle>, // @ky? a_ST_FixedAngle
    pub alignment: Option<RectangleAlignment>, // @algn? a_ST_RectAlignment
    pub rotate_with_shape: Option<bool>, // @rotWithShape? xsd:boolean
    pub color: Color, // <...> a_EG_ColorChoice
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RectangleAlignment {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}
impl RectangleAlignment {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "tl" => Some(Self::TopLeft),
            "t" => Some(Self::Top),
            "tr" => Some(Self::TopRight),
            "l" => Some(Self::Left),
            "ctr" => Some(Self::Center),
            "r" => Some(Self::Right),
            "bl" => Some(Self::BottomLeft),
            "b" => Some(Self::Bottom),
            "br" => Some(Self::BottomRight),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresetShadowEffect {
    pub preset: ShadowPreset, // @prst a_ST_PresetShadowVal
    pub distance: Option<PositiveCoordinate>, // @dist? a_ST_PositiveCoordinate
    pub direction: Option<PositiveFixedAngle>, // @dir? a_ST_PositiveFixedAngle
    pub color: Color, // <...> a_EG_ColorChoice
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShadowPreset {
    Shadow1,
    Shadow2,
    Shadow3,
    Shadow4,
    Shadow5,
    Shadow6,
    Shadow7,
    Shadow8,
    Shadow9,
    Shadow10,
    Shadow11,
    Shadow12,
    Shadow13,
    Shadow14,
    Shadow15,
    Shadow16,
    Shadow17,
    Shadow18,
    Shadow19,
    Shadow20,
}
impl ShadowPreset {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "shdw1" => Some(Self::Shadow1),
            "shdw2" => Some(Self::Shadow2),
            "shdw3" => Some(Self::Shadow3),
            "shdw4" => Some(Self::Shadow4),
            "shdw5" => Some(Self::Shadow5),
            "shdw6" => Some(Self::Shadow6),
            "shdw7" => Some(Self::Shadow7),
            "shdw8" => Some(Self::Shadow8),
            "shdw9" => Some(Self::Shadow9),
            "shdw10" => Some(Self::Shadow10),
            "shdw11" => Some(Self::Shadow11),
            "shdw12" => Some(Self::Shadow12),
            "shdw13" => Some(Self::Shadow13),
            "shdw14" => Some(Self::Shadow14),
            "shdw15" => Some(Self::Shadow15),
            "shdw16" => Some(Self::Shadow16),
            "shdw17" => Some(Self::Shadow17),
            "shdw18" => Some(Self::Shadow18),
            "shdw19" => Some(Self::Shadow19),
            "shdw20" => Some(Self::Shadow20),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReflectionEffect {
    pub blur_radius: Option<PositiveCoordinate>, // @blurRad? a_ST_PositiveCoordinate
    pub start_a: Option<PositiveFixedPercentage>, // @stA? a_ST_PositiveFixedPercentage
    pub start_position: Option<PositiveFixedPercentage>, // @stPos@? a_ST_PositiveFixedPercentage
    pub end_a: Option<PositiveFixedPercentage>, // @endA? a_ST_PositiveFixedPercentage
    pub end_position: Option<PositiveFixedPercentage>, // @endPos@? a_ST_PositiveFixedPercentage
    pub distance: Option<PositiveCoordinate>, // @dist? a_ST_PositiveCoordinate
    pub direction: Option<PositiveFixedAngle>, // @dir? a_ST_PositiveFixedAngle
    pub fade_direction: Option<PositiveFixedAngle>, // @fadeDir? a_ST_PositiveFixedAngle
    pub sx: Option<Percentage>, // @sx? a_ST_Percentage
    pub sy: Option<Percentage>, // @sy? a_ST_Percentage
    pub kx: Option<FixedAngle>, // @kx? a_ST_FixedAngle
    pub ky: Option<FixedAngle>, // @ky? a_ST_FixedAngle
    pub alignment: Option<RectangleAlignment>, // @algn? a_ST_RectAlignment
    pub rotate_with_shape: Option<bool>, // @rotWithShape? xsd:boolean
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RelativeOffsetEffect {
    pub tx: Option<Percentage>, // @tx? a_ST_Percentage
    pub ty: Option<Percentage>, // @ty? a_ST_Percentage
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SoftEdgesEffect {
    pub radius: PositiveCoordinate, // @rad a_ST_PositiveCoordinate
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TintEffect {
    pub hue: Option<PositiveFixedAngle>, // @hue? a_ST_PositiveFixedAngle
    pub amount: Option<FixedPercentage>, // @amt? a_ST_FixedPercentage
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TransformEffect {
    pub sx: Option<Percentage>, // @sx? a_ST_Percentage
    pub sy: Option<Percentage>, // @sy? a_ST_Percentage
    pub kx: Option<FixedAngle>, // @kx? a_ST_FixedAngle
    pub ky: Option<FixedAngle>, // @ky? a_ST_FixedAngle
    pub tx: Option<Coordinate>, // @tx? a_ST_Coordinate
    pub ty: Option<Coordinate>, // @ty? a_ST_Coordinate
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatternFillProperties {
    pub preset: PresetPatternValue, // @prst? a_ST_PresetPatternVal
    pub foreground_color: Option<Color>, // <fgClr>? -> <...> a_CT_Color
    pub background_color: Option<Color>, // <bgClr>? -> <...> a_CT_Color
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineProperties {
    pub width: Option<LineWidth>, // @w? a_ST_LineWidth
    pub cap: Option<LineCap>, // @cap? a_ST_LineCap
    pub compound: Option<CompoundLine>, // @cmpd? a_ST_CompoundLine
    pub alignment: Option<PenAlignment>, // @algn? a_ST_PenAlignment
    pub fill: Option<LineFillProperties>, // <...>? a_EG_LineFillProperties
    pub dash: Option<LineDashProperties>, // <...>? a_EG_LineDashProperties
    pub join: Option<LineJoin>, // <...>? a_EG_LineJoinProperties
    pub head_end: Option<LineEndProperties>, // <headEnd>? a_CT_LineEndProperties
    pub tail_end: Option<LineEndProperties>, // <tailEnd>? a_CT_LineEndProperties
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineCap {
    Round,
    Square,
    Flat,
}
impl LineCap {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "rnd" => Some(Self::Round),
            "sq" => Some(Self::Square),
            "flat" => Some(Self::Flat),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompoundLine {
    Single,
    Double,
    ThickThin,
    ThinThick,
    Triple,
}
impl CompoundLine {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "sng" => Some(Self::Single),
            "dbl" => Some(Self::Double),
            "thickThin" => Some(Self::ThickThin),
            "thinThick" => Some(Self::ThinThick),
            "tri" => Some(Self::Triple),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PenAlignment {
    Center,
    Inset,
}
impl PenAlignment {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "ctr" => Some(Self::Center),
            "in" => Some(Self::Inset),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineFillProperties {
    // a_EG_LineFillProperties
    No, // <noFill /> empty
    Solid(Option<Color>), // <solidFill> a_EG_ColorChoice?
    Gradient(GradientFillProperties), // <gradFill> a_CT_GradientFillProperties
    Pattern(PatternFillProperties), // <patFill> a_CT_PatternFillProperties
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineDashProperties {
    // a_EG_LineDashProperties
    Preset(Option<PresetLineDashValue>), // <prstDash val?="a_ST_PresetLineDashVal" />
    Custom(Vec<DashStop>), // <ds>* a_CT_DashStop
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PresetLineDashValue {
    Solid,
    Dot,
    Dash,
    LgDash,
    DashDot,
    LgDashDot,
    LgDashDotDot,
    SysDash,
    SysDot,
    SysDashDot,
    SysDashDotDot,
}
impl PresetLineDashValue {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "solid" => Some(Self::Solid),
            "dot" => Some(Self::Dot),
            "dash" => Some(Self::Dash),
            "lgDash" => Some(Self::LgDash),
            "dashDot" => Some(Self::DashDot),
            "lgDashDot" => Some(Self::LgDashDot),
            "lgDashDotDot" => Some(Self::LgDashDotDot),
            "sysDash" => Some(Self::SysDash),
            "sysDot" => Some(Self::SysDot),
            "sysDashDot" => Some(Self::SysDashDot),
            "sysDashDotDot" => Some(Self::SysDashDotDot),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DashStop {
    pub d: PositivePercentage, // @d a_ST_PositivePercentage
    pub sp: PositivePercentage, // @sp a_ST_PositivePercentage
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineJoin {
    Round, // <round /> empty
    Bevel, // <bevel /> empty
    Miter(LineJoinMiterProperties), // <miter> a_CT_LineJoinMiterProperties
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineJoinMiterProperties {
    pub lim: Option<PositivePercentage>, // @lim? a_ST_PositivePercentage
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineEndProperties {
    pub end_type: Option<LineEndType>, // @type? a_ST_LineEndType
    pub width: Option<LineEndSize>, // @w? a_ST_LineEndWidth
    pub length: Option<LineEndSize>, // @len? a_ST_LineEndLength
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineEndType {
    None,
    Triangle,
    Stealth,
    Diamond,
    Oval,
    Arrow,
}
impl LineEndType {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "triangle" => Some(Self::Triangle),
            "stealth" => Some(Self::Stealth),
            "diamond" => Some(Self::Diamond),
            "oval" => Some(Self::Oval),
            "arrow" => Some(Self::Arrow),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineEndSize {
    Small,
    Medium,
    Large,
}
impl LineEndSize {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "sm" => Some(Self::Small),
            "med" => Some(Self::Medium),
            "lg" => Some(Self::Large),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EffectStyleItem {
    pub properties: EffectProperties, // <...> a_EG_EffectProperties
    pub scene_3d: Scene3d, // <scene3d>? a_CT_Scene3D
    pub shape_3d: Shape3d, // <sp3d>? a_CT_Shape3D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EffectProperties {
    List(EffectList), // <effectLst> a_CT_EffectList
    Dag(EffectContainer), // <effectDag> a_CT_EffectContainer
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EffectList {
    pub blur: Option<BlurEffect>, // <blur>? a_CT_BlurEffect
    pub fill_overlay: Option<FillOverlayEffect>, // <fillOverlay>? a_CT_FillOverlayEffect
    pub glow: Option<GlowEffect>, // <glow>? a_CT_GlowEffect
    pub inner_shadow: Option<InnerShadowEffect>, // <innerShdw>? a_CT_InnerShadowEffect
    pub outer_shadow: Option<OuterShadowEffect>, // <outerShdw>? a_CT_OuterShadowEffect
    pub preset_shadow: Option<PresetShadowEffect>, // <prstShdw>? a_CT_PresetShadowEffect
    pub reflection: Option<ReflectionEffect>, // <reflection>? a_CT_ReflectionEffect
    pub soft_edge: Option<SoftEdgesEffect>, // <softEdge>? a_CT_SoftEdgesEffect
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Scene3d {
    pub camera: Camera, // <camera> a_CT_Camera
    pub light_rig: LightRig, // <lightRig> a_CT_LightRig,
    pub backdrop: Option<Backdrop>, // <backdrop>? a_CT_Backdrop
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Camera {
    pub preset: PresetCameraType, // @prst a_ST_PresetCameraType
    pub field_of_view: Option<FovAngle>, // @fov? a_ST_FOVAngle
    pub zoom: Option<PositivePercentage>, // @zoom? a_ST_PositivePercentage
    pub rotation: Option<SphereCoordinates>, // <rot>? a_CT_SphereCoords
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SphereCoordinates {
    pub latitude: PositiveFixedAngle, // @lat a_ST_PositiveFixedAngle
    pub longitude: PositiveFixedAngle, // @lon a_ST_PositiveFixedAngle
    pub rev: PositiveFixedAngle, // @rev a_ST_PositiveFixedAngle
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LightRig {
    pub rig_type: LightRigType, // @rig a_ST_LightRigType
    pub direction: LightRigDirection, // @dir a_ST_LightRigDirection
    pub rotation: Option<SphereCoordinates>, // <rot>? a_CT_SphereCoords
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Backdrop {
    pub anchor: Point3d, // <anchor> a_CT_Point3D
    pub normal_vector: Vector3d, // <norm> a_CT_Vector3D
    pub up_vector: Vector3d, // <up> a_CT_Vector3D
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Point3d {
    pub x: Coordinate, // @x a_ST_Coordinate
    pub y: Coordinate, // @y a_ST_Coordinate
    pub z: Coordinate, // @z a_ST_Coordinate
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Vector3d {
    pub dx: Coordinate, // @dx a_ST_Coordinate
    pub dy: Coordinate, // @dy a_ST_Coordinate
    pub dz: Coordinate, // @dz a_ST_Coordinate
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Shape3d {
    pub z: Option<Coordinate>, // @z? a_ST_Coordinate
    pub extrusion_height: Option<PositiveCoordinate>, // @extrusionH? a_ST_PositiveCoordinate
    pub contour_width: Option<PositiveCoordinate>, // @contourW? a_ST_PositiveCoordinate
    pub preset_material: Option<PresetMaterialType>, // @prstMaterial? a_ST_PresetMaterialType
    pub bevel_top: Option<Bevel>, // <bevelT>? a_CT_Bevel
    pub bevel_bottom: Option<Bevel>, // <bevelB>? a_CT_Bevel
    pub extrusion_color: Option<Color>, // <extrusionClr>? a_CT_Color
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Bevel {
    pub width: Option<PositiveCoordinate>, // @w? a_ST_PositiveCoordinate
    pub height: Option<PositiveCoordinate>, // @h? a_ST_PositiveCoordinate
    pub preset: Option<BevelPresetType>, // @prst? a_ST_BevelPresetType
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectStyleDefaults {
    pub shape_definition: Option<DefaultShapeDefinition>, // <spDef>? a_CT_DefaultShapeDefinition
    pub line_definition: Option<DefaultShapeDefinition>, // <lnDef>? a_CT_DefaultShapeDefinition
    pub text_definition: Option<DefaultShapeDefinition>, // <txDef>? a_CT_DefaultShapeDefinition
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DefaultShapeDefinition {
    pub shape_properties: ShapeProperties, // <spPr> a_CT_ShapeProperties
    pub body_properties: TextBodyProperties, // <bodyPr> a_CT_TextBodyProperties
    pub list_style: TextListStyle, // <lstStyle> a_CT_TextListStyle
    pub style: Option<ShapeStyle>, // <style>? a_CT_ShapeStyle
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ShapeProperties {
    pub black_white_mode: Option<BlackWhiteMode>, // @bwMode? a_ST_BlackWhiteMode
    pub transform: Option<Transform2d>, // <xfrm>? a_CT_Transform2D
    pub geometry: Option<Geometry>, // a_EG_Geometry?
    pub fill_properties: Option<FillProperties>, // a_EG_FillProperties?
    pub line_properties: Option<LineProperties>, // <ln>? a_CT_LineProperties
    pub effect_properties: Option<EffectProperties>, // a_EG_EffectProperties?
    pub scene_3d: Option<Scene3d>, // <scene3d>? a_CT_Scene3D
    pub shape_3d: Option<Shape3d>, // <sp3d>? a_CT_Shape3D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Geometry {
    Custom(CustomGeometry2d), // <custGeom> a_CT_CustomGeometry2D
    Preset(PresetGeometry2d), // <prstGeom> a_CT_PresetGeometry2D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CustomGeometry2d {
    pub av_list: Option<Vec<GeomGuide>>, // <avLst>? -> <gd>* a_CT_GeomGuide
    pub guide_list: Option<Vec<GeomGuide>>, // <gdLst>? -> <gd>* a_CT_GeomGuide
    pub adjust_handle_list: Option<Vec<AdjustHandle>>, // <ahLst>? -> <...>* = a_CT_AdjustHandleList
    pub connection_list: Option<Vec<ConnectionSite>>, // <cxnLst>? -> <cxn>* a_CT_ConnectionSite
    pub rect: Option<GeomRect>, // <rect>? a_CT_GeomRect
    pub path_list: Vec<Path2d>, // <pathLst> -> <path>* a_CT_Path2D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeomGuide {
    pub name: String, // @name xsd:token
    pub formula: String, // @fmla xsd:string
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AdjustHandle {
    Xy(XyAdjustHandle), // <ahXY> a_CT_XYAdjustHandle
    Polar(PolarAdjustHandle), // <ahPolar> a_CT_PolarAdjustHandle
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct XyAdjustHandle {
    pub grid_reference_x: Option<String>, // @gdRefX? a_ST_GeomGuideName
    pub minimum_x: Option<AdjustCoordinate>, // @minX? a_ST_AdjCoordinate
    pub maximum_x: Option<AdjustCoordinate>, // @maxX? a_ST_AdjCoordinate
    pub grid_reference_y: Option<String>, // @gdRefY? a_ST_GeomGuideName
    pub minimum_y: Option<AdjustCoordinate>, // @minY? a_ST_AdjCoordinate
    pub maximum_y: Option<AdjustCoordinate>, // @maxY? a_ST_AdjCoordinate
    pub position: AdjustPoint2d, // <pos> a_CT_AdjPoint2D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AdjustCoordinate {
    Coordinate(Coordinate), // a_ST_Coordinate
    Guide(String), // a_ST_GeomGuideName
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AdjustPoint2d {
    pub x: AdjustCoordinate, // @x a_ST_AdjCoordinate
    pub y: AdjustCoordinate, // @y a_ST_AdjCoordinate
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PolarAdjustHandle {
    pub guide_reference_radius: Option<String>, // @gdRefR? a_ST_GeomGuideName
    pub minimum_radius: Option<AdjustCoordinate>, // @minR? a_ST_AdjCoordinate
    pub maximum_radius: Option<AdjustCoordinate>, // @maxR? a_ST_AdjCoordinate
    pub guide_reference_angle: Option<String>, // @gdRefAng? a_ST_GeomGuideName
    pub minimum_angle: Option<AdjustAngle>, // @minAng? a_ST_AdjAngle
    pub maximum_angle: Option<AdjustAngle>, // @maxAng? a_ST_AdjAngle
    pub position: AdjustPoint2d, // <pos> a_CT_AdjPoint2D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AdjustAngle {
    Angle(Angle), // a_ST_Angle
    Guide(String), // a_ST_GeomGuideName
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConnectionSite {
    pub angle: AdjustAngle, // @ang a_ST_AdjAngle
    pub position: AdjustPoint2d, // <pos> a_CT_AdjPoint2D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeomRect {
    pub left: AdjustCoordinate,
    pub top: AdjustCoordinate,
    pub right: AdjustCoordinate,
    pub bottom: AdjustCoordinate,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Path2d {
    pub width: Option<PositiveCoordinate>, // @w? a_ST_PositiveCoordinate
    pub height: Option<PositiveCoordinate>, // @h? a_ST_PositiveCoordinate
    pub fill: Option<PathFillMode>, // @fill? a_ST_PathFillMode
    pub stroke: Option<bool>, // @stroke? xsd:boolean
    pub extrusion_ok: Option<bool>, // @extrusionOk? xsd:boolean
    pub commands: Vec<Path2dCommand>, // <...>*
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PathFillMode {
    None,
    Norm,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}
impl PathFillMode {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "norm" => Some(Self::Norm),
            "lighten" => Some(Self::Lighten),
            "lightenLess" => Some(Self::LightenLess),
            "darken" => Some(Self::Darken),
            "darkenLess" => Some(Self::DarkenLess),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Path2dCommand {
    Close, // <close /> empty
    MoveTo(AdjustPoint2d), // <moveTo> -> <pt> a_CT_AdjPoint2D
    LineTo(AdjustPoint2d), // <lineTo> -> <pt> a_CT_AdjPoint2D
    ArcTo(Path2dArcTo), // <arcTo> a_CT_Path2DArcTo
    QuadraticBezierTo(Vec<AdjustPoint2d>), // <quadBezTo> -> <pt>+ a_CT_AdjPoint2D
    CubicBezierTo(Vec<AdjustPoint2d>), // <cubicBezTo> -> <pt>+ a_CT_AdjPoint2D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Path2dArcTo {
    pub width_radius: AdjustCoordinate, // @wR a_ST_AdjCoordinate
    pub height_radius: AdjustCoordinate, // @hR a_ST_AdjCoordinate
    pub st_angle: AdjustAngle, // @stAng a_ST_AdjAngle
    pub sw_angle: AdjustAngle, // @swAng a_ST_AdjAngle
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresetGeometry2d {
    pub preset: ShapeType, // @prst a_ST_ShapeType
    pub av_list: Option<Vec<GeomGuide>>, // <avLst>? -> <gd>* a_CT_GeomGuide
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Transform2d {
    pub rotation: Option<Angle>, // @rot? a_ST_Angle
    pub flip_horizontal: Option<bool>, // @flipH? xsd:boolean
    pub flip_vertical: Option<bool>, // @flipV? xsd:boolean
    pub offset: Option<Point2d>, // <off>? a_CT_Point2D
    pub extent: Option<PositiveSize2d>, // <ext>? a_CT_PositiveSize2D
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Point2d {
    pub x: Coordinate, // @x a_ST_Coordinate
    pub y: Coordinate, // @y a_ST_Coordinate
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositiveSize2d {
    pub cx: PositiveCoordinate, // @cx a_ST_PositiveCoordinate
    pub cy: PositiveCoordinate, // @cy a_ST_PositiveCoordinate
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextBodyProperties {
    pub rotation: Option<Angle>, // @rot? a_ST_Angle
    pub space_first_last_paragraph: Option<bool>, // @spcFirstLastPara? xsd:boolean
    pub vertical_overflow: Option<TextVerticalOverflowType>, // @vertOverflow? a_ST_TextVertOverflowType
    pub horizontal_overflow: Option<TextHorizontalOverflowType>, // @horzOverflow? a_ST_TextHorzOverflowType
    pub vertical: Option<TextVerticalType>, // @vert? a_ST_TextVerticalType
    pub wrap: Option<TextWrappingType>, // @wrap? a_ST_TextWrappingType
    pub left_inset: Option<Coordinate32>, // @lIns? a_ST_Coordinate32
    pub top_inset: Option<Coordinate32>, // @tIns? a_ST_Coordinate32
    pub right_inset: Option<Coordinate32>, // @rIns? a_ST_Coordinate32
    pub bottom_inset: Option<Coordinate32>, // @bIns? a_ST_Coordinate32
    pub number_columns: Option<TextColumnCount>, // @numCol? a_ST_TextColumnCount
    pub space_column: Option<PositiveCoordinate32>, // @spcCol? a_ST_PositiveCoordinate32
    pub right_to_left_column: Option<bool>, // @rtlCol? xsd:boolean
    pub from_word_art: Option<bool>, // @fromWordArt? xsd:boolean
    pub anchor: Option<TextAnchoringType>, // @anchor? a_ST_TextAnchoringType
    pub anchor_ctr: Option<bool>, // @anchorCtr? xsd:boolean
    pub force_antialiasing: Option<bool>, // @forceAA? xsd:boolean
    pub upright: Option<bool>, // @upright? xsd:boolean
    pub compatibility_line_spacing: Option<bool>, // @compatLnSpc? xsd:boolean
    pub preset_text_warp: Option<PresetTextShape>, // <prstTxWarp>? a_CT_PresetTextShape
    pub autofit: Option<TextAutofit>, // a_EG_TextAutofit?
    pub scene_3d: Option<Scene3d>, // <scene3d>? a_CT_Scene3D
    pub text_3d: Option<Text3d>, // <...>? a_EG_Text3D
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresetTextShape {
    pub preset: TextShapeType, // @prst a_ST_TextShapeType
    pub av_list: Option<Vec<GeomGuide>>, // <avLst>? -> <gd>* a_CT_GeomGuide
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextAutofit {
    No, // <noAutofit /> empty
    Normal(TextNormalAutofit), // <normAutofit> a_CT_TextNormalAutofit
    Shape, // <spAutoFit /> empty
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextNormalAutofit {
    pub font_scale: Option<Percentage>, // @fontScale? s_ST_Percentage
    pub line_spacing_reduction: Option<Percentage>, // @lnSpcReduction? s_ST_Percentage
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Text3d {
    Shape3d(Shape3d), // <sp3d> a_CT_Shape3D
    Flat(FlatText), // <flatTx> a_CT_FlatText
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FlatText {
    pub z: Option<Coordinate>, // @z? a_ST_Coordinate
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextListStyle {
    pub default_paragraph_properties: Option<TextParagraphProperties>, // <defPPr>? a_CT_TextParagraphProperties
    pub level_1_paragraph_properties: Option<TextParagraphProperties>, // <lvl1pPr>? a_CT_TextParagraphProperties
    pub level_2_paragraph_properties: Option<TextParagraphProperties>, // <lvl2pPr>? a_CT_TextParagraphProperties
    pub level_3_paragraph_properties: Option<TextParagraphProperties>, // <lvl3pPr>? a_CT_TextParagraphProperties
    pub level_4_paragraph_properties: Option<TextParagraphProperties>, // <lvl4pPr>? a_CT_TextParagraphProperties
    pub level_5_paragraph_properties: Option<TextParagraphProperties>, // <lvl5pPr>? a_CT_TextParagraphProperties
    pub level_6_paragraph_properties: Option<TextParagraphProperties>, // <lvl6pPr>? a_CT_TextParagraphProperties
    pub level_7_paragraph_properties: Option<TextParagraphProperties>, // <lvl7pPr>? a_CT_TextParagraphProperties
    pub level_8_paragraph_properties: Option<TextParagraphProperties>, // <lvl8pPr>? a_CT_TextParagraphProperties
    pub level_9_paragraph_properties: Option<TextParagraphProperties>, // <lvl9pPr>? a_CT_TextParagraphProperties
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextParagraphProperties {
    pub margin_left: Option<TextMargin>, // @marL? a_ST_TextMargin
    pub margin_right: Option<TextMargin>, // @marR? a_ST_TextMargin
    pub level: Option<TextIndentLevelType>, // @lvl? a_ST_TextIndentLevelType
    pub indent: Option<TextIndent>, // @indent? a_ST_TextIndent
    pub align: Option<TextAlignType>, // @algn? a_ST_TextAlignType
    pub default_tab_size: Option<Coordinate32>, // @defTabSz? a_ST_Coordinate32
    pub right_to_left: Option<bool>, // @rtl? xsd:boolean
    pub ea_line_break: Option<bool>, // @eaLnBrk? xsd:boolean
    pub font_align: Option<TextFontAlignType>, // @fontAlgn? a_ST_TextFontAlignType
    pub latin_line_break: Option<bool>, // @latinLnBrk? xsd:boolean
    pub hanging_punctuation: Option<bool>, // @hangingPunct? xsd:boolean
    pub line_spacing: Option<TextSpacing>, // <lnSpc>? a_CT_TextSpacing
    pub space_before: Option<TextSpacing>, // <spcBef>? a_CT_TextSpacing
    pub space_after: Option<TextSpacing>, // <spcAft>? a_CT_TextSpacing
    pub bullet_color: Option<TextBulletColor>, // a_EG_TextBulletColor?
    pub bullet_size: Option<TextBulletSize>, // a_EG_TextBulletSize?
    pub bullet_typeface: Option<TextBulletTypeface>, // a_EG_TextBulletTypeface?
    pub bullet: Option<TextBullet>, // a_EG_TextBullet?
    pub tab_list: Option<Vec<TabStop>>, // <tabLst>? -> <tab>* a_CT_TextTabStop
    pub text_character_properties: Option<TextCharacterProperties>, // <defRPr>? a_CT_TextCharacterProperties
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextSpacing {
    Percent(Percentage), // <spcPt val="s_ST_Percentage" />
    Points(TextSpacingPoint), // <spcPts val="a_ST_TextSpacingPoint" />
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextBulletColor {
    FollowText, // <buClrTx /> empty
    Color(Color), // <buClr> a_CT_Color
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextBulletSize {
    FollowText, // <buSzTx /> empty
    Percent(TextBulletSizePercent), // <buSzPct val="a_ST_TextBulletSizePercent" />
    Points(TextFontSize), // <buSzPts val="a_ST_TextFontSize" />
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextBulletTypeface {
    FollowText, // <buFontTx /> empty
    Font(TextFont), // <buFont> a_CT_TextFont
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextBullet {
    None, // <buNone /> empty
    AutoNumber(TextAutoNumberBullet), // <buAutoNum> a_CT_TextAutonumberBullet
    Char(String), // <buChar char="xsd:string" />
    Blip(Blip), // <buBlip> -> <blip>
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextAutoNumberBullet {
    pub scheme: TextAutoNumberScheme, // @type a_ST_TextAutonumberScheme
    pub start_at: Option<TextBulletStartAtNumber>, // @startAt? a_ST_TextBulletStartAtNum
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TabStop {
    pub position: Option<Coordinate32>, // @pos? a_ST_Coordinate32
    pub alignment: Option<TextTabAlignType>, // @algn? a_ST_TextTabAlignType
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextCharacterProperties {
    pub kumimoji: Option<bool>, // @kumimoji? xsd:boolean
    pub language: Option<String>, // @lang? xsd:string
    pub alternate_language: Option<String>, // @altLang? xsd:string
    pub size: Option<TextFontSize>, // @sz? a_ST_TextFontSize
    pub bold: Option<bool>, // @b? xsd:boolean
    pub italic: Option<bool>, // @i? xsd:boolean
    pub underline: Option<TextUnderlineType>, // @u? a_ST_TextUnderlineType
    pub strikethrough: Option<TextStrikeType>, // @strike? a_ST_TextStrikeType
    pub kern: Option<TextNonNegativePoint>, // @kern? a_ST_TextNonNegativePoint
    pub capitalize: Option<TextCapsType>, // @cap? a_ST_TextCapsType
    pub spacing: Option<TextPoint>, // @spc? a_ST_TextPoint
    pub normalize_horizontal: Option<bool>, // @normalizeH? xsd:boolean
    pub baseline: Option<Percentage>, // @baseline? a_ST_Percentage
    pub no_proofing: Option<bool>, // @noProof? xsd:boolean
    pub dirty: Option<bool>, // @dirty? xsd:boolean
    pub error: Option<bool>, // @err? xsd:boolean
    pub smt_clean: Option<bool>, // @smtClean? xsd:boolean
    pub smt_id: Option<u32>, // @smtId? xsd:unsignedInt
    pub bmk: Option<String>, // @bmk? xsd:string
    pub line: Option<LineProperties>, // <ln>? a_CT_LineProperties
    pub fill_properties: Option<FillProperties>, // <...>? a_EG_FillProperties
    pub effect_properties: Option<EffectProperties>, // <...>? a_EG_EffectProperties
    pub highlight: Option<Color>, // <highlight>? a_CT_Color
    pub text_underline_line: Option<TextUnderlineLine>, // <...>? a_EG_TextUnderlineLine
    pub text_underline_fill: Option<TextUnderlineFill>, // <...>? a_EG_TextUnderlineFill
    pub latin: Option<TextFont>, // <latin>? a_CT_TextFont
    pub ea: Option<TextFont>, // <ea>? a_CT_TextFont
    pub cs: Option<TextFont>, // <cs>? a_CT_TextFont
    pub sym: Option<TextFont>, // <sym>? a_CT_TextFont
    pub hyperlink_click: Option<Hyperlink>, // <hlinkClick>? a_CT_Hyperlink
    pub hyperlink_mouse_over: Option<Hyperlink>, // <hlinkMouseOver>? a_CT_Hyperlink
    pub right_to_left: Option<bool>, // <rtl val="xsd:boolean" />?
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextUnderlineLine {
    FollowText, // <uLnTx /> empty
    Line(LineProperties), // <uLn> a_CT_LineProperties
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextUnderlineFill {
    FollowText, // <uFillTx /> empty
    Fill(FillProperties), // <uFill> a_EG_FillProperties
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Hyperlink {
    pub relationship_id: Option<String>, // @r:id? r_ST_RelationshipId
    pub invalid_url: Option<String>, // @invalidUrl? xsd:string
    pub action: Option<String>, // @action? xsd:string
    pub target_frame: Option<String>, // @tgtFrame? xsd:string
    pub tooltip: Option<String>, // @tooltip? xsd:string
    pub history: Option<bool>, // @history? xsd:boolean
    pub highlight_click: Option<bool>, // @highlightClick? xsd:boolean
    pub end_sound: Option<bool>, // @endSnd? xsd:boolean
    pub sound: Option<EmbeddedWavAudioFile>, // <snd>? a_CT_EmbeddedWAVAudioFile
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EmbeddedWavAudioFile {
    pub relationship_embed: String, // @r:embed xsd:string
    pub name: Option<String>, // @name? xsd:string
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ShapeStyle {
    pub line_reference: StyleMatrixReference, // <lnRef> a_CT_StyleMatrixReference
    pub fill_reference: StyleMatrixReference, // <fillRef> a_CT_StyleMatrixReference
    pub effect_reference: StyleMatrixReference, // <effectRef> a_CT_StyleMatrixReference
    pub font_reference: FontReference, // <fontRef> a_CT_FontReference
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StyleMatrixReference {
    pub index: usize, // @idx xsd:unsignedInt
    pub color: Option<Color>, // <...>? a_EG_ColorChoice
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontReference {
    pub index: FontCollectionIndex, // @idx a_ST_FontCollectionIndex
    pub color: Option<Color>, // <...>? a_EG_ColorChoice
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ColorSchemeAndMapping {
    pub color_scheme: ColorScheme, // <clrScheme> a_CT_ColorScheme
    pub mapping: ColorMapping, // <clrMap>? a_CT_ColorMapping
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ColorMapping {
    pub background1: ColorSchemeIndex, // @bg1 a_ST_ColorSchemeIndex
    pub text1: ColorSchemeIndex, // @tx1 a_ST_ColorSchemeIndex
    pub background2: ColorSchemeIndex, // @bg2 a_ST_ColorSchemeIndex
    pub text2: ColorSchemeIndex, // @tx2 a_ST_ColorSchemeIndex
    pub accent1: ColorSchemeIndex, // @accent1 a_ST_ColorSchemeIndex
    pub accent2: ColorSchemeIndex, // @accent2 a_ST_ColorSchemeIndex
    pub accent3: ColorSchemeIndex, // @accent3 a_ST_ColorSchemeIndex
    pub accent4: ColorSchemeIndex, // @accent4 a_ST_ColorSchemeIndex
    pub accent5: ColorSchemeIndex, // @accent5 a_ST_ColorSchemeIndex
    pub accent6: ColorSchemeIndex, // @accent6 a_ST_ColorSchemeIndex
    pub hyperlink: ColorSchemeIndex, // @hlink a_ST_ColorSchemeIndex
    pub followed_hyperlink: ColorSchemeIndex, // @folHlink a_ST_ColorSchemeIndex
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CustomColor {
    pub name: Option<String>, // @name? xsd:string
    pub color: Color, // <...> a_EG_ColorChoice
}
