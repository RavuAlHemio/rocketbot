use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::message::{Checkbox, Emoji, InlineFragment, ListItem, MessageFragment};
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

            let mut label_fragments: Vec<InlineFragment> = Vec::new();
            for fragment in inline["value"]["label"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                label_fragments.push(parse_inline_fragment(fragment)?);
            }

            Ok(InlineFragment::Link(url, label_fragments))
        },
        "MENTION_CHANNEL"|"MENTION_USER"|"EMOJI"|"INLINE_CODE" => {
            if inline_type == "EMOJI" && inline["unicode"].is_string() {
                // special case: Unicode emoji
                return Ok(InlineFragment::Emoji(Emoji::Unicode(
                    inline["unicode"].as_str().unwrap().to_owned()
                )));
            }
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
                "EMOJI" => InlineFragment::Emoji(Emoji::Code(target)),
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
            let mut emoji: Vec<Emoji> = Vec::new();
            for big_emoji in paragraph["value"].members().ok_or(MessageParsingError::InnerValueNotList)? {
                let inline_emoji = parse_inline_fragment(big_emoji)?;
                if let InlineFragment::Emoji(e) = inline_emoji {
                    emoji.push(e);
                } else {
                    return Err(MessageParsingError::BigEmojiValueNotEmoji);
                }
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
        "LINE_BREAK" => Ok(MessageFragment::LineBreak),
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


#[cfg(test)]
mod tests {
    use super::parse_message;
    use rocketbot_interface::message::{Emoji, InlineFragment, MessageFragment};
    use serde_json::json;

    #[test]
    fn parse_plain() {
        let msg = json!([
            {
                "type": "PARAGRAPH",
                "value": [
                    {
                        "type": "PLAIN_TEXT",
                        "value": "auch irgendwie zu subtil um einen Unterschied zu machen"
                    }
                ]
            }
        ]);
        let mut parsed = parse_message(&msg).unwrap();
        assert_eq!(parsed.len(), 1);
        let mut inlines = match parsed.remove(0) {
            MessageFragment::Paragraph(inlines) => inlines,
            _ => panic!("not a paragraph"),
        };
        assert_eq!(inlines.len(), 1);
        let plain_text = match inlines.remove(0) {
            InlineFragment::PlainText(plain) => plain,
            _ => panic!("not plain"),
        };
        assert_eq!(plain_text, "auch irgendwie zu subtil um einen Unterschied zu machen");
    }

    #[test]
    fn parse_link() {
        let msg = json!([
            {
                "type": "PARAGRAPH",
                "value": [
                    {
                        "type": "PLAIN_TEXT",
                        "value": "frag mal auf "
                    },
                    {
                        "type": "LINK",
                        "value": {
                            "src": {
                                "type": "PLAIN_TEXT",
                                "value": "//english.stackexchange.com..."
                            },
                            "label": [
                                {
                                    "type": "PLAIN_TEXT",
                                    "value": "english.stackexchange.com..."
                                }
                            ]
                        }
                    }
                ]
            }
        ]);
        let mut parsed = parse_message(&msg).unwrap();
        assert_eq!(parsed.len(), 1);
        let mut inlines = match parsed.remove(0) {
            MessageFragment::Paragraph(inlines) => inlines,
            _ => panic!("not a paragraph"),
        };
        assert_eq!(inlines.len(), 2);

        let plain_text = match inlines.remove(0) {
            InlineFragment::PlainText(plain) => plain,
            _ => panic!("not plain"),
        };
        assert_eq!(plain_text, "frag mal auf ");

        let (url, link_body) = match inlines.remove(0) {
            InlineFragment::Link(url, link_body) => (url, link_body),
            _ => panic!("not link"),
        };
        assert_eq!(url, "//english.stackexchange.com...");
        assert_eq!(link_body.len(), 1);
        let link_text = match &link_body[0] {
            InlineFragment::PlainText(plain) => plain,
            _ => panic!("link body not plain"),
        };
        assert_eq!(link_text, "english.stackexchange.com...");
    }

    #[test]
    fn parse_big_emoji_code() {
        let msg = json!([
            {
                "type": "BIG_EMOJI",
                "value": [
                    {
                        "type": "EMOJI",
                        "value": {
                            "type": "PLAIN_TEXT",
                            "value": "eggplant"
                        },
                        "shortCode": "eggplant"
                    }
                ]
            }
        ]);
        let mut parsed = parse_message(&msg).unwrap();
        assert_eq!(parsed.len(), 1);
        let mut emoji = match parsed.remove(0) {
            MessageFragment::BigEmoji(emoji) => emoji,
            _ => panic!("not a paragraph"),
        };
        assert_eq!(emoji.len(), 1);
        let em = match emoji.remove(0) {
            Emoji::Code(em) => em,
            _ => panic!("not plain"),
        };
        assert_eq!(em, "eggplant");
    }

    #[test]
    fn parse_big_emoji_unicode() {
        let msg = json!([
            {
                "type": "BIG_EMOJI",
                "value": [
                    {
                        "type": "EMOJI",
                        "unicode": "\u{1F346}"
                    }
                ]
            }
        ]);
        let mut parsed = parse_message(&msg).unwrap();
        assert_eq!(parsed.len(), 1);
        let mut emoji = match parsed.remove(0) {
            MessageFragment::BigEmoji(emoji) => emoji,
            _ => panic!("not a paragraph"),
        };
        assert_eq!(emoji.len(), 1);
        let em = match emoji.remove(0) {
            Emoji::Unicode(em) => em,
            _ => panic!("not plain"),
        };
        assert_eq!(em.chars().count(), 1);
        assert_eq!(em.chars().nth(0).unwrap() as u32, 0x1F346);
    }

    #[test]
    fn parse_embedded_link() {
        let msg = json!([
            {
                "type": "PARAGRAPH",
                "value": [
                    {
                        "type": "PLAIN_TEXT",
                        "value": "!fixurls ",
                    },
                    {
                        "type": "LINK",
                        "value": {
                            "label": [
                                {
                                    "type": "PLAIN_TEXT",
                                    "value": "https://xover.mud.at/~tramway/stvkr-a-wiki/index.php?title=Type_500_(WLB",
                                },
                            ],
                            "src": {
                                "type": "PLAIN_TEXT",
                                "value": "https://xover.mud.at/~tramway/stvkr-a-wiki/index.php?title=Type_500_(WLB",
                            },
                        }
                    },
                    {
                        "type": "PLAIN_TEXT",
                        "value": ")&action=history",
                    },
                ],
            },
        ]);
        let mut parsed = parse_message(&msg).unwrap();
        assert_eq!(parsed.len(), 1);
        let mut fragments = match parsed.remove(0) {
            MessageFragment::Paragraph(frags) => frags,
            _ => panic!("not a paragraph"),
        };
        assert_eq!(fragments.len(), 3);
        let plain_prefix = match fragments.remove(0) {
            InlineFragment::PlainText(pt) => pt,
            _ => panic!("prefix not plain text"),
        };
        assert_eq!(plain_prefix, "!fixurls ");
        let (link_target, mut link_label) = match fragments.remove(0) {
            InlineFragment::Link(target, label) => (target, label),
            _ => panic!("not a link"),
        };
        assert_eq!(link_target, "https://xover.mud.at/~tramway/stvkr-a-wiki/index.php?title=Type_500_(WLB");
        assert_eq!(link_label.len(), 1);
        let link_label_plain = match link_label.remove(0) {
            InlineFragment::PlainText(lbl) => lbl,
            _ => panic!("link label not plain"),
        };
        assert_eq!(link_label_plain, "https://xover.mud.at/~tramway/stvkr-a-wiki/index.php?title=Type_500_(WLB");
        let suffix = match fragments.remove(0) {
            InlineFragment::PlainText(pt) => pt,
            _ => panic!("suffix not plain text"),
        };
        assert_eq!(suffix, ")&action=history");
    }
}
