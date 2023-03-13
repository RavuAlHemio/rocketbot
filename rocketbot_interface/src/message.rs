use std::fmt;


fn write_joined_mapped<T, I, F>(f: &mut fmt::Formatter<'_>, pieces: I, glue: &str, mut format_item: F) -> fmt::Result
    where
        T: fmt::Display,
        I: IntoIterator<Item = T>,
        F: FnMut(&mut fmt::Formatter<'_>, &T) -> fmt::Result {
    let mut first = true;
    for piece in pieces.into_iter() {
        if first {
            first = false;
        } else {
            write!(f, "{}", glue)?;
        }
        format_item(f, &piece)?;
    }
    Ok(())
}
fn write_joined<T: fmt::Display, I: IntoIterator<Item = T>>(f: &mut fmt::Formatter<'_>, pieces: I, glue: &str) -> fmt::Result {
    write_joined_mapped(f, pieces, glue, |f, piece| write!(f, "{}", piece))
}
fn write_concatenated<T: fmt::Display, I: IntoIterator<Item = T>>(f: &mut fmt::Formatter<'_>, pieces: I) -> fmt::Result {
    write_joined(f, pieces, "")
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum InlineFragment {
    PlainText(String),
    Bold(Vec<InlineFragment>),
    Strike(Vec<InlineFragment>),
    Italic(Vec<InlineFragment>),
    Link(String, Vec<InlineFragment>),
    MentionChannel(String),
    MentionUser(String),
    Emoji(Emoji),
    InlineCode(String),
}
impl fmt::Display for InlineFragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InlineFragment::PlainText(pt)
                => write!(f, "{}", pt),
            InlineFragment::Bold(fragments) => {
                write!(f, "*")?;
                write_concatenated(f, fragments)?;
                write!(f, "*")
            },
            InlineFragment::Strike(fragments) => {
                write!(f, "~")?;
                write_concatenated(f, fragments)?;
                write!(f, "~")
            },
            InlineFragment::Italic(fragments) => {
                write!(f, "_")?;
                write_concatenated(f, fragments)?;
                write!(f, "_")
            },
            InlineFragment::Link(target, label_fragments) => {
                write!(f, "[")?;
                write_concatenated(f, label_fragments)?;
                write!(f, "]({})", target)
            },
            InlineFragment::MentionChannel(tgt)
                => write!(f, "#{}", tgt),
            InlineFragment::MentionUser(tgt)
                => write!(f, "@{}", tgt),
            InlineFragment::Emoji(tgt)
                => write!(f, "{}", tgt),
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
        write_concatenated(f, &self.label)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ListItem {
    pub label: Vec<InlineFragment>,
}
impl fmt::Display for ListItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_concatenated(f, &self.label)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MessageFragment {
    BigEmoji(Vec<Emoji>),
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
            MessageFragment::BigEmoji(emoji)
                => write_joined(f, emoji, " "),
            MessageFragment::UnorderedList(items)
                => write_joined(f, items, "\n"),
            MessageFragment::Quote(paragraphs)
                => write_joined_mapped(
                    f,
                    paragraphs,
                    "\n",
                    |f, para| write!(f, ">{}", para),
                ),
            MessageFragment::Tasks(tasks)
                => write_joined(f, tasks, "\n"),
            MessageFragment::OrderedList(items) => {
                let mut i = 0usize;
                write_joined_mapped(
                    f,
                    items,
                    "\n",
                    |f, item| {
                        i += 1;
                        write!(f, "{}. {}", i, item)
                    },
                )
            },
            MessageFragment::Paragraph(pieces)
                => write_concatenated(f, pieces),
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
                write_concatenated(f, pieces)?;
                write!(f, "\n")
            },
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Emoji {
    Code(String),
    Unicode(String),
}
impl fmt::Display for Emoji {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(c) => write!(f, ":{}:", c),
            Self::Unicode(s) => f.write_str(s),
        }
    }
}


pub fn collect_inline_urls<'a, I: Iterator<Item = &'a InlineFragment>>(fragments: I) -> Vec<String> {
    let mut urls = Vec::new();
    for fragment in fragments {
        match fragment {
            InlineFragment::Bold(ilfs) => {
                let mut inline_urls = collect_inline_urls(ilfs.iter());
                urls.append(&mut inline_urls);
            },
            InlineFragment::Emoji(_emoji) => {},
            InlineFragment::InlineCode(_code) => {},
            InlineFragment::Italic(ilfs) => {
                let mut inline_urls = collect_inline_urls(ilfs.iter());
                urls.append(&mut inline_urls);
            },
            InlineFragment::Link(url, label_ilfs) => {
                // this is where the magic happens
                urls.push(url.clone());

                // you never know
                let mut inline_urls = collect_inline_urls(label_ilfs.iter());
                urls.append(&mut inline_urls);
            },
            InlineFragment::MentionChannel(_channel) => {},
            InlineFragment::MentionUser(_user) => {},
            InlineFragment::PlainText(_pt) => {},
            InlineFragment::Strike(ilfs) => {
                let mut inline_urls = collect_inline_urls(ilfs.iter());
                urls.append(&mut inline_urls);
            },
        }
    }
    urls
}

pub fn collect_urls<'a, I: Iterator<Item = &'a MessageFragment>>(fragments: I) -> Vec<String> {
    let mut urls = Vec::new();
    for fragment in fragments {
        match fragment {
            MessageFragment::BigEmoji(_emoji) => {},
            MessageFragment::Code(_lang, _ilfs) => {},
            MessageFragment::Heading(_level, ilfs) => {
                let mut inline_urls = collect_inline_urls(ilfs.iter());
                urls.append(&mut inline_urls);
            },
            MessageFragment::OrderedList(items) => {
                for item in items {
                    let mut inline_urls = collect_inline_urls(item.label.iter());
                    urls.append(&mut inline_urls);
                }
            },
            MessageFragment::Paragraph(ilfs) => {
                let mut inline_urls = collect_inline_urls(ilfs.iter());
                urls.append(&mut inline_urls);
            },
            MessageFragment::Quote(frags) => {
                let mut frag_urls = collect_urls(frags.iter());
                urls.append(&mut frag_urls);
            },
            MessageFragment::Tasks(cbs) => {
                for cb in cbs {
                    let mut inline_urls = collect_inline_urls(cb.label.iter());
                    urls.append(&mut inline_urls);
                }
            },
            MessageFragment::UnorderedList(items) => {
                for item in items {
                    let mut inline_urls = collect_inline_urls(item.label.iter());
                    urls.append(&mut inline_urls);
                }
            },
        }
    }
    urls
}
