#![cfg(feature = "theme")]


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Theme {
    // <{http://schemas.openxmlformats.org/drawingml/2006/main}theme>
    name: Option<String>, // @name? xsd:string
    elements: BaseStyles, // <themeElements> a_CT_BaseStyles
    object_defaults: Option<ObjectStyleDefaults>, // <objectDefaults>? a_CT_ObjectStyleDefaults
    extra_color_scheme_list: Option<ColorSchemeList>, // <extraClrSchemeLst>? a_CT_ColorSchemeList
    custom_color_list: Option<CustomColorList>, // <custClrLst>? a_CT_CustomColorList
    extension_list: Option<OfficeArtExtensionList>, // <extLst>? a_CT_OfficeArtExtensionList
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BaseStyles {
    pub color_scheme: ColorScheme, // <clrScheme> a_CT_ColorScheme
    pub font_scheme: FontScheme, // <fontScheme> a_CT_FontScheme
    pub format_scheme: StyleMatrix, // <fmtScheme> a_CT_StyleMatrix
    pub extension_list: Option<OfficeArtExtensionList>, // <extLst>? a_CT_OfficeArtExtensionList
}
