mod coordinate;
pub mod style;
mod xml;


use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::io::{self, Read, Seek};

use sxd_document::{Package, QName};
use zip::read::ZipArchive;
use zip::result::ZipError;

pub use crate::coordinate::{CoordinateError, ExcelCoordinate};
use crate::style::Stylesheet;
use crate::xml::{
    DocExt, ElemExt, NS_OFFDOC_RELS, NS_PKG_RELS, NS_SPRSH, Relationship, REL_TYPE_OFFDOC,
    REL_TYPE_SHARED_STR, REL_TYPE_SHEET, REL_TYPE_STYLES,
};


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QualifiedName {
    pub local_name: String,
    pub namespace_uri: Option<String>,
}
impl QualifiedName {
    pub fn new<S: Into<String>, N: Into<String>>(local_name: S, namespace_uri: Option<N>) -> Self {
        Self { local_name: local_name.into(), namespace_uri: namespace_uri.map(|v| v.into()) }
    }

    pub fn new_bare<S: Into<String>>(local_name: S) -> Self {
        let namespace_uri: Option<&'static str> = None;
        Self::new(local_name.into(), namespace_uri)
    }

    pub fn new_ns<S: Into<String>, N: Into<String>>(local_name: S, namespace_uri: N) -> Self {
        Self::new(local_name.into(), Some(namespace_uri.into()))
    }
}
impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(namespace_uri) = self.namespace_uri.as_ref() {
            write!(f, "{}{}{}", '{', namespace_uri, '}')?;
        }
        write!(f, "{}", self.local_name)
    }
}
impl<'d> From<QName<'d>> for QualifiedName {
    fn from(value: QName<'d>) -> Self {
        Self {
            local_name: value.local_part().to_owned(),
            namespace_uri: value.namespace_uri().map(|ns| ns.to_owned()),
        }
    }
}


#[derive(Debug)]
pub enum Error {
    OpeningXlsx { zip_error: ZipError },
    OpeningFileWithinXlsx { path: String, zip_error: ZipError },
    ReadingFileWithinXlsx { path: String, io_error: io::Error },
    XmlParsingFailed { path: String, parse_error: sxd_document::parser::Error },
    MissingRootElement { path: String },
    UnexpectedElement { path: String, expected: QualifiedName, obtained: QualifiedName },
    MissingWorkbookRelationship,
    MissingWorksheetRelationship { name: String, relationship_id: String },
    MissingSharedStringsRelationship,
    MissingStylesheetRelationship,
    MissingCoordinate { path: String },
    InvalidCoordinate { path: String, coordinate_string: String, coordinate_error: CoordinateError },
    MissingRequiredAttribute { path: String, element_name: QualifiedName, attribute_name: QualifiedName },
    RequiredAttributeWrongFormat { path: String, element_name: QualifiedName, attribute_name: QualifiedName, value: String, format_hint: Cow<'static, str> },
    MultiChoiceChildElements { path: String, parent_name: QualifiedName, one_child_name: QualifiedName, other_child_name: QualifiedName },
    MissingChildElement { path: String, childless_parent_name: QualifiedName },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpeningXlsx { zip_error }
                => write!(f, "ZIP format error while opening XLSX file: {}", zip_error),
            Self::OpeningFileWithinXlsx { path, zip_error }
                => write!(f, "ZIP error while opening path {:?} from XLSX file: {}", path, zip_error),
            Self::ReadingFileWithinXlsx { path, io_error }
                => write!(f, "input/output error while reading path {:?} from XLSX file: {}", path, io_error),
            Self::XmlParsingFailed { path, parse_error }
                => write!(f, "failed to parse file {:?} as XML: {}", path, parse_error),
            Self::MissingRootElement { path }
                => write!(f, "XML file {:?} is missing a root element", path),
            Self::UnexpectedElement { path, expected, obtained }
                => write!(f, "XML file {:?} has unexpected element {}; expected {}", path, obtained, expected),
            Self::MissingWorkbookRelationship
                => write!(f, "no top-level workbook relationship found"),
            Self::MissingWorksheetRelationship { name, relationship_id }
                => write!(f, "failed to find relationship to worksheet {:?} (workbook relationship ID {:?})", name, relationship_id),
            Self::MissingSharedStringsRelationship
                => write!(f, "no relationship to shared strings found"),
            Self::MissingStylesheetRelationship
                => write!(f, "no relationship to stylesheet found"),
            Self::MissingCoordinate { path }
                => write!(f, "file {:?} has a cell with a missing coordinate string", path),
            Self::InvalidCoordinate { path, coordinate_string, coordinate_error }
                => write!(f, "file {:?} has invalid coordinate string {:?}: {}", path, coordinate_string, coordinate_error),
            Self::MissingRequiredAttribute { path, element_name, attribute_name }
                => write!(f, "file {:?} element {} is missing a required attribute named {}", path, element_name, attribute_name),
            Self::RequiredAttributeWrongFormat { path, element_name, attribute_name, value, format_hint }
                => write!(f, "file {path:?} element {element_name} required attribute {attribute_name} has value {value:?} which doesn't abide by the expected format {format_hint:?}"),
            Self::MultiChoiceChildElements { path, parent_name, one_child_name, other_child_name  }
                => write!(f, "file {path:?} element {parent_name} contains both a {one_child_name} and a {other_child_name} child but may only contain one of them"),
            Self::MissingChildElement { path, childless_parent_name }
                => write!(f, "file {path:?} element {childless_parent_name} lacks children"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OpeningXlsx { zip_error } => Some(zip_error),
            Self::OpeningFileWithinXlsx { zip_error, .. } => Some(zip_error),
            Self::ReadingFileWithinXlsx { io_error, .. } => Some(io_error),
            Self::XmlParsingFailed { parse_error, .. } => Some(parse_error),
            Self::MissingRootElement { .. } => None,
            Self::UnexpectedElement { .. } => None,
            Self::MissingWorkbookRelationship => None,
            Self::MissingWorksheetRelationship { .. } => None,
            Self::MissingSharedStringsRelationship => None,
            Self::MissingStylesheetRelationship => None,
            Self::MissingCoordinate { .. } => None,
            Self::InvalidCoordinate { coordinate_error, .. } => Some(coordinate_error),
            Self::MissingRequiredAttribute { .. } => None,
            Self::RequiredAttributeWrongFormat { .. } => None,
            Self::MultiChoiceChildElements { .. } => None,
            Self::MissingChildElement { .. } => None,
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Worksheet {
    pub name: String,
    pub xlsx_path: String,
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Cell {
    pub coordinate: ExcelCoordinate,
    pub style_index: Option<usize>,
    pub value_index: Option<usize>,
}


#[derive(Clone, Debug)]
pub struct XlsxFile<R> {
    zip_archive: ZipArchive<R>,
    workbook_path: String,
}
impl<R: Read + Seek> XlsxFile<R> {
    pub fn new(reader: R) -> Result<Self, Error> {
        let mut zip_archive = ZipArchive::new(reader)
            .map_err(|zip_error| Error::OpeningXlsx { zip_error })?;

        // attempt to extract the workbook definition path from the top-level relationship file
        let top_level_rels = Self::read_relationships(&mut zip_archive, "_rels/.rels")?;
        let workbook_rel = top_level_rels.into_iter()
            .filter(|rel| rel.rel_type == REL_TYPE_OFFDOC)
            .nth(0).ok_or(Error::MissingWorkbookRelationship)?;
        let workbook_path = workbook_rel.target;

        Ok(Self {
            zip_archive,
            workbook_path,
        })
    }

    pub fn get_worksheets(&mut self) -> Result<Vec<Worksheet>, Error> {
        // obtain worksheet references from workbook
        let workbook = Self::read_xml(&mut self.zip_archive, &self.workbook_path)?;
        let workbook_root = workbook.as_document()
            .root_element().ok_or_else(|| Error::MissingRootElement { path: self.workbook_path.clone() })?
            .ensure_name_ns_for_path("workbook", NS_SPRSH, &self.workbook_path)?;
        let sheets = workbook_root
            .child_elements_named_ns("sheets", NS_SPRSH)
            .into_iter()
            .flat_map(|sheets| sheets.child_elements_named_ns("sheet", NS_SPRSH));
        let mut sheet_name_and_rel = Vec::new();
        for sheet in sheets {
            let Some(name) = sheet.attribute_value("name") else { continue };
            let Some(rel) = sheet.attribute_value_ns("id", NS_OFFDOC_RELS) else { continue };
            sheet_name_and_rel.push((name.to_owned(), rel.to_owned()));
        }

        // obtain worksheet paths from workbook relationship file
        let workbook_rels_path = Self::convert_file_path_to_rel_file_path(&self.workbook_path);
        let workbook_rels = Self::read_relationships(&mut self.zip_archive, &workbook_rels_path)?;
        let mut worksheets = Vec::with_capacity(sheet_name_and_rel.len());
        for (sheet_name, sheet_rel) in sheet_name_and_rel {
            let sheet_path = workbook_rels.iter()
                .filter(|rel| rel.rel_type == REL_TYPE_SHEET)
                .filter(|rel| rel.id == sheet_rel)
                .nth(0).ok_or_else(|| Error::MissingWorksheetRelationship { name: sheet_name.clone(), relationship_id: sheet_rel })?
                .target.clone();
            worksheets.push(Worksheet {
                name: sheet_name,
                xlsx_path: sheet_path,
            });
        }
        Ok(worksheets)
    }

    pub fn get_shared_strings(&mut self) -> Result<HashMap<usize, String>, Error> {
        // obtain shared strings reference from workbook relationship file
        let workbook_rels_path = Self::convert_file_path_to_rel_file_path(&self.workbook_path);
        let workbook_rels = Self::read_relationships(&mut self.zip_archive, &workbook_rels_path)?;
        let shared_str_rel = workbook_rels.into_iter()
            .filter(|rel| rel.rel_type == REL_TYPE_SHARED_STR)
            .nth(0).ok_or(Error::MissingSharedStringsRelationship)?;

        let shared_strings = Self::read_xml(&mut self.zip_archive, &shared_str_rel.target)?;
        let shared_strings_root = shared_strings.as_document()
            .root_element().ok_or_else(|| Error::MissingRootElement { path: shared_str_rel.target.clone() })?
            .ensure_name_ns_for_path("sst", NS_SPRSH, &shared_str_rel.target)?;
        let shared_string_elems = shared_strings_root
            .child_elements_named_ns("si", NS_SPRSH);
        let mut map = HashMap::with_capacity(shared_string_elems.len());
        for (i, shared_string_elem) in shared_string_elems.into_iter().enumerate() {
            let text = shared_string_elem.collect_text();
            map.insert(i, text);
        }
        Ok(map)
    }

    pub fn get_worksheet_contents(&mut self, worksheet_path: &str) -> Result<HashMap<ExcelCoordinate, Cell>, Error> {
        let worksheet = Self::read_xml(&mut self.zip_archive, worksheet_path)?;
        let worksheet_root = worksheet.as_document()
            .root_element().ok_or_else(|| Error::MissingRootElement { path: worksheet_path.to_owned() })?
            .ensure_name_ns_for_path("worksheet", NS_SPRSH, worksheet_path)?;
        let cells = worksheet_root
            .child_elements_named_ns("sheetData", NS_SPRSH)
            .into_iter()
            .flat_map(|sheets| sheets.child_elements_named_ns("row", NS_SPRSH))
            .flat_map(|sheets| sheets.child_elements_named_ns("c", NS_SPRSH));
        let mut map = HashMap::new();
        for cell in cells {
            let coordinate_str = cell.attribute_value("r")
                .ok_or_else(|| Error::MissingCoordinate { path: worksheet_path.to_owned() })?;
            let coordinate: ExcelCoordinate = coordinate_str
                .parse()
                .map_err(|coordinate_error| Error::InvalidCoordinate {
                    path: worksheet_path.to_owned(),
                    coordinate_string: coordinate_str.to_owned(),
                    coordinate_error,
                })?;
            let style_index: Option<usize> = cell.attribute_value("s")
                .and_then(|style_index_str| style_index_str.parse().ok());
            let value_index: Option<usize> = cell
                .child_elements_named_ns("v", NS_SPRSH)
                .into_iter()
                .nth(0)
                .and_then(|v_cell| v_cell.collect_text().parse().ok());
            map.insert(
                coordinate,
                Cell {
                    coordinate,
                    style_index,
                    value_index,
                }
            );
        }
        Ok(map)
    }

    pub fn get_stylesheet(&mut self) -> Result<Stylesheet, Error> {
        // obtain shared strings reference from workbook relationship file
        let workbook_rels_path = Self::convert_file_path_to_rel_file_path(&self.workbook_path);
        let workbook_rels = Self::read_relationships(&mut self.zip_archive, &workbook_rels_path)?;
        let stylesheet_rel = workbook_rels.into_iter()
            .filter(|rel| rel.rel_type == REL_TYPE_STYLES)
            .nth(0).ok_or(Error::MissingSharedStringsRelationship)?;

        let stylesheet = Self::read_xml(&mut self.zip_archive, &stylesheet_rel.target)?;
        let stylesheet_root = stylesheet.as_document()
            .root_element().ok_or_else(|| Error::MissingRootElement { path: stylesheet_rel.target.clone() })?
            .ensure_name_ns_for_path("styleSheet", NS_SPRSH, &stylesheet_rel.target)?;
        Stylesheet::try_from_elem(stylesheet_root, &workbook_rels_path)
    }

    fn convert_file_path_to_rel_file_path(path: &str) -> String {
        // one/two/three/something.xml -> one/two/three/_rels/something.xml.rels
        let insert_rels_point = match path.rfind('/') {
            Some(last_slash) => last_slash + 1,
            None => 0,
        };
        let (before, after) = path.split_at(insert_rels_point);
        format!("{}/_rels{}.rels", before, after)
    }

    fn read_xml(zip_archive: &mut ZipArchive<R>, xml_path: &str) -> Result<Package, Error> {
        // open file
        let mut xml_file = zip_archive.by_name(xml_path)
            .map_err(|zip_error| Error::OpeningFileWithinXlsx { path: xml_path.to_owned(), zip_error })?;
        let mut xml_string = String::new();
        xml_file.read_to_string(&mut xml_string)
            .map_err(|io_error| Error::ReadingFileWithinXlsx { path: xml_path.to_owned(), io_error })?;
        sxd_document::parser::parse(&xml_string)
            .map_err(|parse_error| Error::XmlParsingFailed { path: xml_path.to_owned(), parse_error })
    }

    fn read_relationships(zip_archive: &mut ZipArchive<R>, rel_file_path: &str) -> Result<Vec<Relationship>, Error> {
        // parse the relationships file
        let rel_package = Self::read_xml(zip_archive, rel_file_path)?;
        let rel = rel_package.as_document();
        let rel_root = rel
            .root_element().ok_or_else(|| Error::MissingRootElement { path: rel_file_path.to_owned() })?
            .ensure_name_ns_for_path("Relationships", NS_PKG_RELS, rel_file_path)?;
        let rel_elems = rel_root.child_elements_named_ns("Relationship", NS_PKG_RELS);
        let mut relationships = Vec::with_capacity(rel_elems.len());
        for rel_elem in rel_elems {
            let Some(id) = rel_elem.attribute_value("Id") else { continue };
            let Some(rel_type) = rel_elem.attribute_value("Type") else { continue };
            let Some(target) = rel_elem.attribute_value("Target") else { continue };
            relationships.push(Relationship {
                id: id.to_owned(),
                rel_type: rel_type.to_owned(),
                target: target.to_owned(),
            });
        }
        Ok(relationships)
    }
}
