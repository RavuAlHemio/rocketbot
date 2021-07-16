use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::message::{Checkbox, InlineFragment, ListItem, MessageFragment};
use serde_json;

use crate::errors::MessageParsingError;


fn parse_inline_fragment(inline: &serde_json::Value) -> Result<InlineFragment, MessageParsingError> {
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
            let mut fragments: Vec<InlineFragment> = Vec::new();
            for fragment in inline["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                fragments.push(parse_inline_fragment(fragment)?);
            }
            let result = match inline_type {
                "BOLD" => InlineFragment::Bold(fragments),
                "STRIKE" => InlineFragment::Strike(fragments),
                "ITALIC" => InlineFragment::Italic(fragments),
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

fn parse_list_item(item: &serde_json::Value) -> Result<ListItem, MessageParsingError> {
    match item["type"].as_str().ok_or(MessageParsingError::TypeNotString)? {
        "LIST_ITEM" => {
            let mut fragments: Vec<InlineFragment> = Vec::new();
            for fragment in item["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
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

fn parse_checkbox(item: &serde_json::Value) -> Result<Checkbox, MessageParsingError> {
    match item["type"].as_str().ok_or(MessageParsingError::TypeNotString)? {
        "TASK" => {
            let checked = item["status"].as_bool()
                .ok_or(MessageParsingError::TaskStatusNotBool)?;

            let mut fragments: Vec<InlineFragment> = Vec::new();
            for fragment in item["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
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

fn parse_code_line(item: &serde_json::Value) -> Result<InlineFragment, MessageParsingError> {
    match item["type"].as_str().ok_or(MessageParsingError::TypeNotString)? {
        "CODE_LINE" => {
            parse_inline_fragment(&item["value"])
        },
        other => {
            Err(MessageParsingError::UnexpectedFragment(other.into(), "code line".into()))
        },
    }
}

fn parse_paragraph_fragment(paragraph: &serde_json::Value) -> Result<MessageFragment, MessageParsingError> {
    match paragraph["type"].as_str().ok_or(MessageParsingError::TypeNotString)? {
        "BIG_EMOJI" => {
            let mut emoji: Vec<String> = Vec::new();
            for big_emoji in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                let emoji_string = big_emoji["value"]["value"].as_str()
                    .ok_or(MessageParsingError::BigEmojiValueNotString)?
                    .to_owned();
                emoji.push(emoji_string);
            }
            Ok(MessageFragment::BigEmoji(emoji))
        },
        "UNORDERED_LIST" => {
            let mut items: Vec<ListItem> = Vec::new();
            for item in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                let list_item = parse_list_item(item)?;
                items.push(list_item);
            }
            Ok(MessageFragment::UnorderedList(items))
        },
        "QUOTE" => {
            let mut items: Vec<MessageFragment> = Vec::new();
            for item in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                let fragment = parse_paragraph_fragment(item)?;
                items.push(fragment);
            }
            Ok(MessageFragment::Quote(items))
        },
        "TASKS" => {
            let mut tasks: Vec<Checkbox> = Vec::new();
            for item in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                let task = parse_checkbox(item)?;
                tasks.push(task);
            }
            Ok(MessageFragment::Tasks(tasks))
        },
        "ORDERED_LIST" => {
            let mut items: Vec<ListItem> = Vec::new();
            for item in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                let list_item = parse_list_item(item)?;
                items.push(list_item);
            }
            Ok(MessageFragment::OrderedList(items))
        },
        "PARAGRAPH" => {
            let mut fragments: Vec<InlineFragment> = Vec::new();
            for frag in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
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
            for line in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                let parsed_line = parse_code_line(line)?;
                lines.push(parsed_line);
            }
            Ok(MessageFragment::Code(language, lines))
        },
        "HEADING" => {
            let level = paragraph["level"].as_u32()
                .ok_or(MessageParsingError::HeadingLevelNotU32)?;

            let mut fragments: Vec<InlineFragment> = Vec::new();
            for fragment in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
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

pub(crate) fn parse_message(paragraphs: &serde_json::Value) -> Result<Vec<MessageFragment>, MessageParsingError> {
    let mut ret: Vec<MessageFragment> = Vec::new();
    for pm in paragraphs.members().ok_or(MessageParsingError::InnerValueNotList)? {
        let fragment = parse_paragraph_fragment(pm)?;
        ret.push(fragment);
    }
    Ok(ret)
}
