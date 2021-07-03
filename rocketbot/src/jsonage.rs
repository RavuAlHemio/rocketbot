use std::convert::TryFrom;
use std::fmt;

use json::JsonValue;
use rocketbot_interface::model::{Checkbox, InlineFragment, ListItem, MessageFragment};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Visitor;
use serde::ser::{SerializeMap, SerializeSeq};

use crate::errors::MessageParsingError;


#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RocketBotJsonValue(JsonValue);
impl From<JsonValue> for RocketBotJsonValue {
    fn from(val: JsonValue) -> Self {
        RocketBotJsonValue(val)
    }
}
impl Serialize for RocketBotJsonValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            JsonValue::Array(arr) => {
                let mut seq = serializer.serialize_seq(Some(arr.len()))?;
                for elem in arr {
                    let rbjv: RocketBotJsonValue = elem.clone().into();
                    seq.serialize_element(&rbjv)?;
                }
                seq.end()
            },
            JsonValue::Boolean(b) => {
                serializer.serialize_bool(*b)
            },
            JsonValue::Null => {
                serializer.serialize_none()
            },
            JsonValue::Number(num) => {
                if let Ok(u) = u64::try_from(*num) {
                    serializer.serialize_u64(u)
                } else if let Ok(i) = i64::try_from(*num) {
                    serializer.serialize_i64(i)
                } else {
                    serializer.serialize_f64(f64::from(*num))
                }
            },
            JsonValue::Object(obj) => {
                let mut map = serializer.serialize_map(Some(obj.len()))?;
                for (k, v) in obj.iter() {
                    let rbjv: RocketBotJsonValue = v.clone().into();
                    map.serialize_entry(k, &rbjv)?;
                }
                map.end()
            },
            JsonValue::Short(s) => {
                serializer.serialize_str(s.as_str())
            },
            JsonValue::String(s) => {
                serializer.serialize_str(s.as_str())
            },
        }
    }
}
impl<'de> Deserialize<'de> for RocketBotJsonValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsonValueVisitor)
    }
}
impl From<RocketBotJsonValue> for JsonValue {
    fn from(v: RocketBotJsonValue) -> Self {
        v.0
    }
}


struct JsonValueVisitor;

impl<'de> Visitor<'de> for JsonValueVisitor {
    type Value = RocketBotJsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a value representable in JSON")
    }

    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(RocketBotJsonValue(JsonValue::Boolean(v)))
    }

    fn visit_i8<E: serde::de::Error>(self, v: i8) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_i16<E: serde::de::Error>(self, v: i16) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_i32<E: serde::de::Error>(self, v: i32) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_u8<E: serde::de::Error>(self, v: u8) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_u16<E: serde::de::Error>(self, v: u16) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_u32<E: serde::de::Error>(self, v: u32) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_f32<E: serde::de::Error>(self, v: f32) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
        let number = json::number::Number::from(v);
        Ok(RocketBotJsonValue(JsonValue::Number(number)))
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(RocketBotJsonValue(JsonValue::String(v.into())))
    }

    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(RocketBotJsonValue(JsonValue::String(v)))
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut vals = match seq.size_hint() {
            Some(n) => Vec::with_capacity(n),
            None => Vec::new(),
        };

        while let Some(v) = seq.next_element()? {
            let v2: RocketBotJsonValue = v;
            vals.push(v2.0);
        }

        Ok(RocketBotJsonValue(JsonValue::Array(vals)))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut obj = match map.size_hint() {
            Some(n) => json::object::Object::with_capacity(n),
            None => json::object::Object::new(),
        };

        while let Some((k, v)) = map.next_entry()? {
            let k2: String = k;
            let v2: RocketBotJsonValue = v;
            obj.insert(&k2, v2.0);
        }

        Ok(RocketBotJsonValue(JsonValue::Object(obj)))
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(RocketBotJsonValue(JsonValue::Null))
    }
}

fn parse_inline_fragment(inline: &JsonValue) -> Result<InlineFragment, MessageParsingError> {
    let inline_type = inline["type"].as_str()
        .ok_or(MessageParsingError::TypeNotString)?;
    match inline_type {
        "PLAIN_TEXT" => {
            let value = inline["value"].as_str()
                .ok_or(MessageParsingError::PlainTextValueNotString)?
                .to_owned();
            Ok(InlineFragment::PlainText(value))
        },
        "BOLD"|"STRIKE"|"ITALIC" => {
            let content_box = Box::new(parse_inline_fragment(&inline["value"])?);
            let result = match inline_type {
                "BOLD" => InlineFragment::Bold(content_box),
                "STRIKE" => InlineFragment::Strike(content_box),
                "ITALIC" => InlineFragment::Italic(content_box),
                _ => panic!("type does not match pre-filtered types; assume bug"),
            };
            Ok(result)
        },
        "LINK" => {
            let value_type = inline["value"]["src"]["type"].as_str()
                .ok_or(MessageParsingError::LinkValueNotSinglePlainText)?;
            if value_type != "PLAIN_TEXT" {
                return Err(MessageParsingError::LinkValueNotSinglePlainText);
            }
            let url = inline["value"]["src"]["value"].as_str()
                .ok_or(MessageParsingError::LinkValuePlainTextNotString)?
                .to_owned();

            let label = parse_inline_fragment(&inline["value"]["label"])?;

            Ok(InlineFragment::Link(url, Box::new(label)))
        },
        "MENTION_CHANNEL"|"MENTION_USER"|"EMOJI"|"INLINE_CODE" => {
            let value_type = inline["value"]["type"].as_str()
                .ok_or(MessageParsingError::TargetValueNotSinglePlainText(inline_type.into()))?;
            if value_type != "PLAIN_TEXT" {
                return Err(MessageParsingError::TargetValueNotSinglePlainText(inline_type.into()));
            }
            let target = inline["value"]["value"].as_str()
                .ok_or(MessageParsingError::TargetValueNotSinglePlainText(inline_type.into()))?
                .to_owned();
            let result = match inline_type {
                "MENTION_CHANNEL" => InlineFragment::MentionChannel(target),
                "MENTION_USER" => InlineFragment::MentionUser(target),
                "EMOJI" => InlineFragment::Emoji(target),
                "INLINE_CODE" => InlineFragment::InlineCode(target),
                _ => panic!("type does not match pre-filtered types; assume bug"),
            };
            Ok(result)
        },
        other => {
            Err(MessageParsingError::UnexpectedFragment(other.into(), "inline fragment".into()))
        },
    }
}

fn parse_list_item(item: &JsonValue) -> Result<ListItem, MessageParsingError> {
    match item["type"].as_str().ok_or(MessageParsingError::TypeNotString)? {
        "LIST_ITEM" => {
            let mut fragments: Vec<InlineFragment> = Vec::new();
            for fragment in item["value"].members() {
                fragments.push(parse_inline_fragment(fragment)?);
            }
            Ok(ListItem {
                label: fragments,
            })
        },
        other => {
            Err(MessageParsingError::UnexpectedFragment(other.into(), "list item".into()))
        },
    }
}

fn parse_checkbox(item: &JsonValue) -> Result<Checkbox, MessageParsingError> {
    match item["type"].as_str().ok_or(MessageParsingError::TypeNotString)? {
        "TASK" => {
            let checked = item["status"].as_bool()
                .ok_or(MessageParsingError::TaskStatusNotBool)?;

            let mut fragments: Vec<InlineFragment> = Vec::new();
            for fragment in item["value"].members() {
                fragments.push(parse_inline_fragment(fragment)?);
            }
            Ok(Checkbox {
                checked,
                label: fragments,
            })
        },
        other => {
            Err(MessageParsingError::UnexpectedFragment(other.into(), "task".into()))
        },
    }
}

fn parse_code_line(item: &JsonValue) -> Result<InlineFragment, MessageParsingError> {
    match item["type"].as_str().ok_or(MessageParsingError::TypeNotString)? {
        "CODE_LINE" => {
            parse_inline_fragment(&item["value"])
        },
        other => {
            Err(MessageParsingError::UnexpectedFragment(other.into(), "code line".into()))
        },
    }
}

fn parse_paragraph_fragment(paragraph: &JsonValue) -> Result<MessageFragment, MessageParsingError> {
    match paragraph["type"].as_str().ok_or(MessageParsingError::TypeNotString)? {
        "BIG_EMOJI" => {
            let mut emoji: Vec<String> = Vec::new();
            for big_emoji in paragraph["value"].members() {
                let emoji_string = big_emoji["value"]["value"].as_str()
                    .ok_or(MessageParsingError::BigEmojiValueNotString)?
                    .to_owned();
                emoji.push(emoji_string);
            }
            Ok(MessageFragment::BigEmoji(emoji))
        },
        "UNORDERED_LIST" => {
            let mut items: Vec<ListItem> = Vec::new();
            for item in paragraph["value"].members() {
                let list_item = parse_list_item(item)?;
                items.push(list_item);
            }
            Ok(MessageFragment::UnorderedList(items))
        },
        "QUOTE" => {
            let mut items: Vec<MessageFragment> = Vec::new();
            for item in paragraph["value"].members() {
                let fragment = parse_paragraph_fragment(item)?;
                items.push(fragment);
            }
            Ok(MessageFragment::Quote(items))
        },
        "TASKS" => {
            let mut tasks: Vec<Checkbox> = Vec::new();
            for item in paragraph["value"].members() {
                let task = parse_checkbox(item)?;
                tasks.push(task);
            }
            Ok(MessageFragment::Tasks(tasks))
        },
        "ORDERED_LIST" => {
            let mut items: Vec<ListItem> = Vec::new();
            for item in paragraph["value"].members() {
                let list_item = parse_list_item(item)?;
                items.push(list_item);
            }
            Ok(MessageFragment::OrderedList(items))
        },
        "PARAGRAPH" => {
            let mut fragments: Vec<InlineFragment> = Vec::new();
            for frag in paragraph["value"].members() {
                let fragment = parse_inline_fragment(frag)?;
                fragments.push(fragment);
            }
            Ok(MessageFragment::Paragraph(fragments))
        },
        "CODE" => {
            let language = paragraph["language"].as_str()
                .ok_or(MessageParsingError::CodeLanguageNotString)?
                .to_owned();

            let mut lines: Vec<InlineFragment> = Vec::new();
            for line in paragraph["value"].members() {
                let parsed_line = parse_code_line(line)?;
                lines.push(parsed_line);
            }
            Ok(MessageFragment::Code(language, lines))
        },
        "HEADING" => {
            let level = paragraph["level"].as_u32()
                .ok_or(MessageParsingError::HeadingLevelNotU32)?;

            let mut fragments: Vec<InlineFragment> = Vec::new();
            for fragment in paragraph["value"].members() {
                let parsed_line = parse_inline_fragment(fragment)?;
                fragments.push(parsed_line);
            }
            Ok(MessageFragment::Heading(level, fragments))
        },
        other => {
            Err(MessageParsingError::UnexpectedFragment(other.into(), "message fragment".into()))
        },
    }
}

pub(crate) fn parse_message(paragraphs: &JsonValue) -> Result<Vec<MessageFragment>, MessageParsingError> {
    let mut ret: Vec<MessageFragment> = Vec::new();
    for pm in paragraphs.members() {
        let fragment = parse_paragraph_fragment(pm)?;
        ret.push(fragment);
    }
    Ok(ret)
}
