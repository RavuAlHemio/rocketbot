use sxd_document::QName;
use sxd_document::dom::{ChildOfElement, Document, Element};

use crate::Error;


pub(crate) const NS_OFFDOC_RELS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(crate) const NS_PKG_RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
pub(crate) const NS_SPRSH: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub(crate) const REL_TYPE_OFFDOC: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub(crate) const REL_TYPE_SHARED_STR: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
pub(crate) const REL_TYPE_SHEET: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Relationship {
    pub id: String,
    pub rel_type: String,
    pub target: String,
}

pub(crate) trait DocExt<'d> {
    fn root_element(&self) -> Option<Element<'d>>;
}
impl<'d> DocExt<'d> for Document<'d> {
    fn root_element(&self) -> Option<Element<'d>> {
        self
            .root()
            .children().into_iter()
            .filter_map(|c| c.element())
            .nth(0)
    }
}

pub(crate) trait ElemExt<'d> {
    fn ensure_name_ns_for_path(self, name: &str, namespace: &str, path: &str) -> Result<Self, Error> where Self: Sized;
    fn child_elements_named_ns(&self, name: &str, namespace: &str) -> Vec<Element<'d>>;
    fn attribute_value_ns(&self, name: &str, namespace: &str) -> Option<&str>;
    fn collect_text_into(&self, buf: &mut String);

    fn collect_text(&self) -> String {
        let mut buf = String::new();
        self.collect_text_into(&mut buf);
        buf
    }
}
impl<'d> ElemExt<'d> for Element<'d> {
    fn ensure_name_ns_for_path(self, name: &str, namespace: &str, path: &str) -> Result<Self, Error> {
        let expected_name = QName::with_namespace_uri(Some(namespace), name);
        let my_name = self.name();
        if my_name == expected_name {
            Ok(self)
        } else {
            Err(Error::UnexpectedRootElement {
                path: path.to_owned(),
                expected: expected_name.into(),
                obtained: my_name.into(),
            })
        }
    }

    fn child_elements_named_ns(&self, name: &str, namespace: &str) -> Vec<Element<'d>> {
        let requested_name = QName::with_namespace_uri(Some(namespace), name);
        self.children()
            .into_iter()
            .filter_map(|c| c.element())
            .filter(|e| e.name() == requested_name)
            .collect()
    }

    fn attribute_value_ns(&self, name: &str, namespace: &str) -> Option<&str> {
        let requested_name = QName::with_namespace_uri(Some(namespace), name);
        self.attribute_value(requested_name)
    }

    fn collect_text_into(&self, buf: &mut String) {
        for child in self.children() {
            match child {
                ChildOfElement::Element(element) => {
                    element.collect_text_into(buf);
                },
                ChildOfElement::Text(text) => {
                    buf.push_str(text.text());
                },

                // the following node types have neither children nor text
                ChildOfElement::Comment(_) => {},
                ChildOfElement::ProcessingInstruction(_) => {},
            }
        }
    }
}
