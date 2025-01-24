#![cfg(feature = "theme")]
mod color_enums;
mod simple_types;


pub use crate::theme::color_enums::{PresetColorValue, SchemeColorValue, SystemColorValue};
pub use crate::theme::simple_types::{
    Angle, Coordinate, FixedAngle, FixedPercentage, LineWidth, Panose, Percentage, PitchFamily,
    PositiveCoordinate, PositiveFixedAngle, PositiveFixedPercentage, PositivePercentage,
    TileFlipMode,
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
    // a_CT_Color / a_EG_ColorChoice
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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PresetPatternValue {
    Pct5,
    Pct10,
    Pct20,
    Pct25,
    Pct30,
    Pct40,
    Pct50,
    Pct60,
    Pct70,
    Pct75,
    Pct80,
    Pct90,
    Horz,
    Vert,
    LtHorz,
    LtVert,
    DkHorz,
    DkVert,
    NarHorz,
    NarVert,
    DashHorz,
    DashVert,
    Cross,
    DnDiag,
    UpDiag,
    LtDnDiag,
    LtUpDiag,
    DkDnDiag,
    DkUpDiag,
    WdDnDiag,
    WdUpDiag,
    DashDnDiag,
    DashUpDiag,
    DiagCross,
    SmCheck,
    LgCheck,
    SmGrid,
    LgGrid,
    DotGrid,
    SmConfetti,
    LgConfetti,
    HorzBrick,
    DiagBrick,
    SolidDmnd,
    OpenDmnd,
    DotDmnd,
    Plaid,
    Sphere,
    Weave,
    Divot,
    Shingle,
    Wave,
    Trellis,
    ZigZag,
}
impl PresetPatternValue {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "pct5" => Some(Self::Pct5),
            "pct10" => Some(Self::Pct10),
            "pct20" => Some(Self::Pct20),
            "pct25" => Some(Self::Pct25),
            "pct30" => Some(Self::Pct30),
            "pct40" => Some(Self::Pct40),
            "pct50" => Some(Self::Pct50),
            "pct60" => Some(Self::Pct60),
            "pct70" => Some(Self::Pct70),
            "pct75" => Some(Self::Pct75),
            "pct80" => Some(Self::Pct80),
            "pct90" => Some(Self::Pct90),
            "horz" => Some(Self::Horz),
            "vert" => Some(Self::Vert),
            "ltHorz" => Some(Self::LtHorz),
            "ltVert" => Some(Self::LtVert),
            "dkHorz" => Some(Self::DkHorz),
            "dkVert" => Some(Self::DkVert),
            "narHorz" => Some(Self::NarHorz),
            "narVert" => Some(Self::NarVert),
            "dashHorz" => Some(Self::DashHorz),
            "dashVert" => Some(Self::DashVert),
            "cross" => Some(Self::Cross),
            "dnDiag" => Some(Self::DnDiag),
            "upDiag" => Some(Self::UpDiag),
            "ltDnDiag" => Some(Self::LtDnDiag),
            "ltUpDiag" => Some(Self::LtUpDiag),
            "dkDnDiag" => Some(Self::DkDnDiag),
            "dkUpDiag" => Some(Self::DkUpDiag),
            "wdDnDiag" => Some(Self::WdDnDiag),
            "wdUpDiag" => Some(Self::WdUpDiag),
            "dashDnDiag" => Some(Self::DashDnDiag),
            "dashUpDiag" => Some(Self::DashUpDiag),
            "diagCross" => Some(Self::DiagCross),
            "smCheck" => Some(Self::SmCheck),
            "lgCheck" => Some(Self::LgCheck),
            "smGrid" => Some(Self::SmGrid),
            "lgGrid" => Some(Self::LgGrid),
            "dotGrid" => Some(Self::DotGrid),
            "smConfetti" => Some(Self::SmConfetti),
            "lgConfetti" => Some(Self::LgConfetti),
            "horzBrick" => Some(Self::HorzBrick),
            "diagBrick" => Some(Self::DiagBrick),
            "solidDmnd" => Some(Self::SolidDmnd),
            "openDmnd" => Some(Self::OpenDmnd),
            "dotDmnd" => Some(Self::DotDmnd),
            "plaid" => Some(Self::Plaid),
            "sphere" => Some(Self::Sphere),
            "weave" => Some(Self::Weave),
            "divot" => Some(Self::Divot),
            "shingle" => Some(Self::Shingle),
            "wave" => Some(Self::Wave),
            "trellis" => Some(Self::Trellis),
            "zigZag" => Some(Self::ZigZag),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineProperties {
    pub width: Option<LineWidth>, // @w? a_ST_LineWidth
    pub cap: Option<LineCap>, // @cap? a_ST_LineCap
    pub compound: Option<CompoundLine>, // @cmpd? a_ST_CompoundLine
    pub alignment: Option<PenAlignment>, // @algn? a_ST_PenAlignment
    pub fill: Option<LineFillProperties>, // <...>? a_EG_LineFillProperties
    pub dash: Option<LineDashProperties>, // <...>? a_EG_LineDashProperties
    pub join: Option<LineJoinProperties>, // <...>? a_EG_LineJoinProperties
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
