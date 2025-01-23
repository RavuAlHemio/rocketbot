#![cfg(feature = "theme")]
mod color_enums;
mod simple_types;


pub use crate::theme::color_enums::{PresetColorValue, SchemeColorValue, SystemColorValue};
pub use crate::theme::simple_types::{
    Angle, FixedPercentage, Panose, Percentage, PitchFamily, PositiveFixedAngle,
    PositiveFixedPercentage, PositivePercentage, TileFlipMode,
};


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Theme {
    // <{http://schemas.openxmlformats.org/drawingml/2006/main}theme>
    pub name: Option<String>, // @name? xsd:string
    pub elements: BaseStyles, // <themeElements> a_CT_BaseStyles
    pub object_defaults: Option<ObjectStyleDefaults>, // <objectDefaults>? a_CT_ObjectStyleDefaults
    pub extra_color_scheme_list: Option<ColorSchemeList>, // <extraClrSchemeLst>? a_CT_ColorSchemeList
    pub custom_color_list: Option<CustomColorList>, // <custClrLst>? a_CT_CustomColorList
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BaseStyles {
    pub color_scheme: ColorScheme, // <clrScheme> a_CT_ColorScheme
    pub font_scheme: FontScheme, // <fontScheme> a_CT_FontScheme
    pub format_scheme: StyleMatrix, // <fmtScheme> a_CT_StyleMatrix
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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Color {
    // a_EG_ColorChoice
    ScRgb(ScRgbColor), // <scrgbClr> a_CT_ScRgbColor
    SRgb(SRgbColor), // <srgbClr> a_CT_SRgbColor
    Hsl(HslColor), // <hslClr> a_CT_HslColor
    System(SystemColor), // <sysClr> a_CT_SystemColor
    Scheme(SchemeColor), // <schemeClr> a_CT_SchemeColor
    Preset(PresetColor), // <prstClr> a_CT_PresetColor
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScRgbColor {
    pub r: Percentage, // @r s_ST_Percentage
    pub g: Percentage, // @g s_ST_Percentage
    pub b: Percentage, // @b s_ST_Percentage
    pub transforms: Vec<ColorTransform>, // <...>* a_EG_ColorTransform
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ColorTransform {
    Tint(PositiveFixedPercentage), // <tint val="a_ST_PositiveFixedPercentage" />
    Shade(PositiveFixedPercentage), // <shade> a_CT_PositiveFixedPercentage
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
    pub line_style_list: Vec<LineStyle>, // <lnStyleLst> -> <ln>+ -> a_CT_LineProperties
    pub effect_style_list: Vec<EffectStyleItem>, // <effectStyleLst> -> <effectStyle>+ -> a_CT_EffectStyleItem
    pub background_fill_style_list: Vec<FillProperties>, // <bgFillStyleLst> -> <...> a_EG_FillProperties
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FillProperties {
    No, // <noFill /> empty
    Solid(Option<Color>), // <solidFill> a_EG_ColorChoice?
    Gradient(GradientFillProperties), // <gradFill> a_CT_GradientFillProperties
    Blip(BlipFillProperties), // <blipFill> a_CT_BlipFillProperties
    Pattern(PatternFillProperties), // <patFill> a_CT_PatternFillProperties
    GroupFill(GroupFillProperties), // <grpFill> a_CT_GroupFillProperties
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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShadeProperties {
    Linear(LinearShadeProperties), // <lin /> a_CT_LinearShadeProperties
    Path(PathShadeProperties), // <path> a_CT_PathShadeProperties
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinearShadeProperties {
    pub angle: Option<PositiveFixedAngle>, // @ang? a_ST_PositiveFixedAngle
    pub scaled: Option<bool>, // @scaled? xsd:boolean
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BlipEffect {
    AlphaBiLevel { threshold: PositiveFixedPercentage }, // <alphaBiLevel thresh="a_ST_PositiveFixedPercentage" />
    AlphaCeiling, // <alphaCeiling /> empty
    AlphaFloor, // <alphaFloor /> empty
    AlphaInverse(Option<Color>), // <alphaInv> a_EG_ColorChoice?
    AlphaModulate(EffectContainer), // <alphaMod> -> <cont> a_CT_EffectContainer
    AlphaModulateFixed { amount: PositivePercentage }, // <alphaModFix amt="a_ST_PositivePercentage" />
    AlphaReplace { new_alpha: PositiveFixedPercentage }, // <alphaRepl a="a_ST_PositiveFixedPercentage" />
    BiLevel { threshold: PositiveFixedPercentage }, // <biLevel thresh="a_ST_PositiveFixedPercentage" />
    Blur(BlurEffect), // <blur /> a_CT_BlurEffect
    ColorChange(ColorChangeEffect), // <clrChange> a_CT_ColorChangeEffect
    ColorReplace(Color), // <clrRepl> -> <...> a_EG_ColorChoice
    Duotone(Vec<Color>), // <duotone> -> <...>+ a_EG_ColorChoice+
    FillOverlay(FillOverlayEffect), // <fillOverlay> a_CT_FillOverlayEffect
    Grayscale, // <grayscl /> empty
    Hsl(HslEffect), // <hsl /> a_CT_HSLEffect
    Luminance(LuminanceEffect), // <lum /> a_CT_LuminanceEffect
    Tint(TintEffect), // <tint /> a_CT_TintEffect
}

/*
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineStyle {
    pub properties: Vec<LineProperties>, // <...>+ a_CT_LineProperties
}
*/
