use std::borrow::Cow;

use strict_num::FiniteF64;
use sxd_document::QName;
use sxd_document::dom::{ChildOfElement, Document, Element};

use crate::{Error, QualifiedName};


pub(crate) const NS_OFFDOC_RELS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(crate) const NS_PKG_RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
pub(crate) const NS_SPRSH: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub(crate) const REL_TYPE_OFFDOC: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub(crate) const REL_TYPE_SHARED_STR: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
pub(crate) const REL_TYPE_SHEET: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
pub(crate) const REL_TYPE_STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";


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
    fn required_attribute_value(&self, name: &str, path: &str) -> Result<&str, Error>;
    fn required_attribute_value_in_format<T, F: FnOnce(&str) -> Option<T>>(&self, name: &str, path: &str, parse_func: F, format_hint: &'static str) -> Result<T, Error>;

    fn collect_text(&self) -> String {
        let mut buf = String::new();
        self.collect_text_into(&mut buf);
        buf
    }

    fn grandchild_elements_named_ns(&self, child_name: &str, child_namespace: &str, grandchild_name: &str, grandchild_namespace: &str) -> Vec<Element<'d>> {
        self
            .child_elements_named_ns(child_name, child_namespace)
            .into_iter()
            .flat_map(|child| child.child_elements_named_ns(grandchild_name, grandchild_namespace))
            .collect()
    }

    fn first_child_element_named_ns(&self, name: &str, namespace: &str) -> Option<Element<'d>> {
        self
            .child_elements_named_ns(name, namespace)
            .into_iter()
            .nth(0)
    }

    /// Returns the boolean property value of the child with the given name.
    ///
    /// If no child element with the given name is found, returns `None`.
    ///
    /// If the first child element with the given name has an attribute "val" with a valid xsd:boolean value
    /// ("0"/"false"/"1"/"true"), returns `Some(_)` with that value.
    ///
    /// If the first child element with the given name does not have a "val" attribute, returns `Some(assumption)`.
    ///
    /// If the first child element with the given name has a "val" attribute with an invalid xsd:boolean value, returns
    /// `Some(assumption)`.
    fn first_child_element_ns_boolean_property_assuming(&self, name: &str, namespace: &str, assumption: bool) -> Option<bool> {
        let child = match self.first_child_element_named_ns(name, namespace) {
            Some(c) => c,
            None => return None,
        };
        let boolean_value = child.attribute_value("val")
            .and_then(|s| s.as_xsd_boolean())
            .unwrap_or(assumption);
        Some(boolean_value)
    }
}
impl<'d> ElemExt<'d> for Element<'d> {
    fn ensure_name_ns_for_path(self, name: &str, namespace: &str, path: &str) -> Result<Self, Error> {
        let expected_name = QName::with_namespace_uri(Some(namespace), name);
        let my_name = self.name();
        if my_name == expected_name {
            Ok(self)
        } else {
            Err(Error::UnexpectedElement {
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

    fn required_attribute_value(&self, name: &str, path: &str) -> Result<&str, Error> {
        self.attribute_value(name)
            .ok_or_else(|| Error::MissingRequiredAttribute {
                path: path.to_string(),
                element_name: self.name().into(),
                attribute_name: QualifiedName::new_bare(name),
            })
    }

    fn required_attribute_value_in_format<T, F: FnOnce(&str) -> Option<T>>(&self, name: &str, path: &str, parse_func: F, format_hint: &'static str) -> Result<T, Error> {
        let str_val = self.required_attribute_value(name, path)?;
        parse_func(str_val)
            .ok_or_else(|| Error::RequiredAttributeWrongFormat {
                path: path.to_owned(),
                element_name: self.name().into(),
                attribute_name: QualifiedName::new_bare(name),
                value: str_val.to_owned(),
                format_hint: Cow::Borrowed(format_hint),
            })
    }
}

pub(crate) trait StrExt {
    fn as_xsd_boolean(&self) -> Option<bool>;
    fn as_usize(&self) -> Option<usize>;
    fn as_finite_f64(&self) -> Option<FiniteF64>;
}
impl StrExt for str {
    fn as_xsd_boolean(&self) -> Option<bool> {
        // https://www.w3.org/TR/xmlschema-2/ § 3.2.2 boolean
        match self {
            "0"|"false" => Some(false),
            "1"|"true" => Some(true),
            _ => None,
        }
    }

    fn as_usize(&self) -> Option<usize> {
        self.parse().ok()
    }

    fn as_finite_f64(&self) -> Option<FiniteF64> {
        let value: f64 = self.parse().ok()?;
        FiniteF64::new(value)
    }
}
impl StrExt for Option<&str> {
    fn as_xsd_boolean(&self) -> Option<bool> {
        self.and_then(|s| s.as_xsd_boolean())
    }

    fn as_usize(&self) -> Option<usize> {
        self.and_then(|s| s.as_usize())
    }

    fn as_finite_f64(&self) -> Option<FiniteF64> {
        self.and_then(|s| s.as_finite_f64())
    }
}
