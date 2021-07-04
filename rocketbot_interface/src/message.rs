use std::fmt;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum InlineFragment {
    PlainText(String),
    Bold(Box<InlineFragment>),
    Strike(Box<InlineFragment>),
    Italic(Box<InlineFragment>),
    Link(String, Box<InlineFragment>),
    MentionChannel(String),
    MentionUser(String),
    Emoji(String),
    InlineCode(String),
}
impl fmt::Display for InlineFragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InlineFragment::PlainText(pt)
                => write!(f, "{}", pt),
            InlineFragment::Bold(b)
                => write!(f, "*{}*", b),
            InlineFragment::Strike(s)
                => write!(f, "~{}~", s),
            InlineFragment::Italic(i)
                => write!(f, "_{}_", i),
            InlineFragment::Link(target, label)
                => write!(f, "[{}]({})", label, target),
            InlineFragment::MentionChannel(tgt)
                => write!(f, "#{}", tgt),
            InlineFragment::MentionUser(tgt)
                => write!(f, "@{}", tgt),
            InlineFragment::Emoji(tgt)
                => write!(f, ":{}:", tgt),
            InlineFragment::InlineCode(tgt)
                => write!(f, "`{}`", tgt),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Checkbox {
    pub checked: bool,
    pub label: Vec<InlineFragment>,
}
impl fmt::Display for Checkbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let checkmark = if self.checked { 'x' } else { ' ' };
        write!(f, "- [{}] ", checkmark)?;
        for part in &self.label {
            write!(f, "{}", part)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ListItem {
    pub label: Vec<InlineFragment>,
}
impl fmt::Display for ListItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in &self.label {
            write!(f, "{}", part)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MessageFragment {
    BigEmoji(Vec<String>),
    UnorderedList(Vec<ListItem>),
    Quote(Vec<MessageFragment>),
    Tasks(Vec<Checkbox>),
    OrderedList(Vec<ListItem>),
    Paragraph(Vec<InlineFragment>),
    Code(String, Vec<InlineFragment>),
    Heading(u32, Vec<InlineFragment>),
}
impl fmt::Display for MessageFragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageFragment::BigEmoji(emoji) => {
                let wrapped_emoji: Vec<String> = emoji.iter()
                    .map(|moji| format!(":{}:", moji))
                    .collect();
                let emoji_string = wrapped_emoji.join(" ");
                write!(f, "{}", emoji_string)
            },
            MessageFragment::UnorderedList(items) => {
                let item_strings: Vec<String> = items.iter()
                    .map(|item| item.to_string())
                    .collect();
                write!(f, "{}", item_strings.join("\n"))
            },
            MessageFragment::Quote(paragraphs) => {
                let para_strings: Vec<String> = paragraphs.iter()
                    .map(|para| format!(">{}", para))
                    .collect();
                write!(f, "{}", para_strings.join("\n"))
            },
            MessageFragment::Tasks(tasks) => {
                let task_strings: Vec<String> = tasks.iter()
                    .map(|task| task.to_string())
                    .collect();
                write!(f, "{}", task_strings.join("\n"))
            },
            MessageFragment::OrderedList(items) => {
                let item_strings: Vec<String> = items.iter()
                    .enumerate()
                    .map(|(i, item)| format!("{}. {}", i, item))
                    .collect();
                write!(f, "{}", item_strings.join("\n"))
            },
            MessageFragment::Paragraph(pieces) => {
                for piece in pieces {
                    write!(f, "{}", piece)?;
                }
                Ok(())
            },
            MessageFragment::Code(language, lines) => {
                write!(f, "```{}\n", language)?;
                for line in lines {
                    write!(f, "{}\n", line)?;
                }
                write!(f, "```\n")
            },
            MessageFragment::Heading(level, pieces) => {
                for _ in 0..*level {
                    write!(f, "#")?;
                }
                write!(f, " ")?;
                for piece in pieces {
                    write!(f, "{}", piece)?;
                }
                write!(f, "\n")
            },
        }
    }
}
