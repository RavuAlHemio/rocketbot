use std::convert::TryInto;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Utc;
use json::JsonValue;
use log::{error, info};
use tokio_postgres::{self, NoTls};

use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, InlineFragment, MessageFragment};


fn find_user_mention_in_inline_fragment(fragment: &InlineFragment) -> Option<String> {
    match fragment {
        InlineFragment::Bold(inner) => find_user_mention_in_inline_fragment(inner),
        InlineFragment::Emoji(_name) => None,
        InlineFragment::InlineCode(_code) => None,
        InlineFragment::Italic(inner) => find_user_mention_in_inline_fragment(inner),
        InlineFragment::Link(_url, label) => find_user_mention_in_inline_fragment(label),
        InlineFragment::MentionChannel(_channel_name) => None,
        InlineFragment::MentionUser(user_name) => Some(user_name.clone()),
        InlineFragment::PlainText(_text) => None,
        InlineFragment::Strike(inner) => find_user_mention_in_inline_fragment(inner),
    }
}

fn find_user_mention_in_inline_fragments<'a, I: Iterator<Item = &'a InlineFragment>>(fragments: I) -> Option<String> {
    for frag in fragments {
        if let Some(mention) = find_user_mention_in_inline_fragment(frag) {
            return Some(mention);
        }
    }
    None
}

fn find_user_mention_in_message_fragment(fragment: &MessageFragment) -> Option<String> {
    match fragment {
        MessageFragment::BigEmoji(_emoji_codes) => None,
        MessageFragment::Code(_language, _lines) => None,
        MessageFragment::Heading(_level, fragments)
            => find_user_mention_in_inline_fragments(fragments.iter()),
        MessageFragment::OrderedList(items)
            => find_user_mention_in_inline_fragments(
                items.iter()
                    .flat_map(|item| item.label.iter())
            ),
        MessageFragment::Paragraph(fragments)
            => find_user_mention_in_inline_fragments(fragments.iter()),
        MessageFragment::Quote(fragments)
            => fragments.iter()
                .filter_map(|frag| find_user_mention_in_message_fragment(frag))
                .nth(0),
        MessageFragment::Tasks(tasks)
            => tasks.iter()
                .map(|task| find_user_mention_in_inline_fragments(task.label.iter()))
                .filter_map(|mention| mention)
                .nth(0),
        MessageFragment::UnorderedList(items)
            => find_user_mention_in_inline_fragments(
                items.iter()
                    .flat_map(|item| item.label.iter())
            ),
    }
}

fn find_user_mention_in_message_fragments<'a, I: Iterator<Item = &'a MessageFragment>>(fragments: I) -> Option<String> {
    for frag in fragments {
        if let Some(mention) = find_user_mention_in_message_fragment(frag) {
            return Some(mention);
        }
    }
    None
}


pub struct ThanksPlugin {
    interface: Weak<dyn RocketBotInterface>,
    db_conn_string: String,
    most_grateful_count: usize,
}
impl ThanksPlugin {
    async fn find_user_mention(&self, channel_message: &ChannelMessage) -> Option<String> {
        let interface = match self.interface.upgrade() {
            None => return None,
            Some(i) => i,
        };

        let mention_opt = find_user_mention_in_message_fragments(channel_message.message.parsed.iter());
        match mention_opt {
            Some(m) => Some(m),
            None => {
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!("@{}: please use the mention syntax (`@username`) to target a user", channel_message.message.sender.username),
                ).await;
                None
            },
        }
    }

    async fn connect_db(&self) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
        let (client, connection) = match tokio_postgres::connect(&self.db_conn_string, NoTls).await {
            Ok(cc) => cc,
            Err(e) => {
                error!("error connecting to database: {}", e);
                return Err(e);
            },
        };
        tokio::spawn(async move {
            connection.await
        });
        Ok(client)
    }

    async fn handle_thank(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let target = match self.find_user_mention(channel_message).await {
            Some(tw) => tw,
            None => return,
        };
        let db_client = match self.connect_db().await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let now = Utc::now();
        let thanker_lower = channel_message.message.sender.username.to_lowercase();
        let thankee_lower = target.to_lowercase();
        let channel = channel_message.channel.name.clone();
        let reason = channel_message.message.raw.clone();

        if thanker_lower == thankee_lower {
            interface.send_channel_message(
                &channel_message.channel.name,
                &format!(
                    "You are so full of yourself, @{}",
                    channel_message.message.sender.username,
                ),
            ).await;
            return;
        }

        let exec_res = db_client.execute(
            r#"
                INSERT INTO thanks.thanks (thanks_id, "timestamp", thanker_lowercase, thankee_lowercase, channel, reason, deleted)
                VALUES (DEFAULT, $1, $2, $3, $4, $5, FALSE)
            "#,
            &[&now, &thanker_lower, &thankee_lower, &channel, &reason],
        ).await;
        if let Err(e) = exec_res {
            error!("error inserting thanks: {}", e);
            interface.send_channel_message(
                &channel_message.channel.name,
                &format!(
                    "@{}: something broke, sorry!",
                    channel_message.message.sender.username,
                ),
            ).await;
            return;
        }

        info!("{} thanks {} in {}: {}", thanker_lower, thankee_lower, channel, reason);

        let count_row_res = db_client.query_one(
            r#"
                SELECT COUNT(*) count FROM thanks.thanks WHERE thankee_lowercase = $1 AND deleted = FALSE
            "#,
            &[&thankee_lower],
        ).await;
        let count_row = match count_row_res {
            Ok(cr) => cr,
            Err(e) => {
                error!("error querying thanks count: {}", e);
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!(
                        "@{} Alright! {} has been thanked.",
                        channel_message.message.sender.username,
                        target,
                    ),
                ).await;
                return;
            },
        };
        let count: i64 = count_row.get(0);

        interface.send_channel_message(
            &channel_message.channel.name,
            &format!(
                "@{} Alright! By the way, {} has been thanked {} until now.",
                channel_message.message.sender.username,
                target,
                if count == 1 { "once".into() } else { format!("{} times", count) },
            ),
        ).await;
    }

    async fn handle_thanked(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let target = match self.find_user_mention(channel_message).await {
            Some(tw) => tw,
            None => return,
        };
        let db_client = match self.connect_db().await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let target_lower = target.to_lowercase();
        let count_row_res = db_client.query_one(
            r#"
                SELECT COUNT(*) count FROM thanks.thanks WHERE thankee_lowercase = $1 AND deleted = FALSE
            "#,
            &[&target_lower],
        ).await;
        let count_row = match count_row_res {
            Ok(cr) => cr,
            Err(e) => {
                error!("error querying thanks count: {}", e);
                return;
            },
        };
        let count: i64 = count_row.get(0);

        let count_phrase: String = match count {
            0 => "not been thanked".into(),
            1 => "been thanked once".into(),
            other => format!("been thanked {} times", other),
        };

        let most_grateful_suffix = if count == 0 {
            String::new()
        } else {
            // also show stats
            let count_plus_one: i64 = (self.most_grateful_count + 1).try_into()
                .expect("most grateful count not representable as i64");
            let most_grateful_res = db_client.query(
                r#"
                    SELECT thanker_lowercase, COUNT(*) count
                    FROM thanks.thanks
                    WHERE thankee_lowercase = $1 AND deleted = FALSE
                    GROUP BY thanker_lowercase
                    ORDER BY count DESC
                    LIMIT $2
                "#,
                &[&target_lower, &count_plus_one],
            ).await;
            match most_grateful_res {
                Err(e) => {
                    error!("error querying most grateful: {}", e);
                    String::new()
                },
                Ok(mg) => {
                    let mut entries = Vec::new();
                    for row in mg.iter().take(self.most_grateful_count) {
                        let thanker: String = row.get(0);
                        let count: i64 = row.get(1);
                        entries.push(format!("{}: {}\u{D7}", thanker, count));
                    }
                    let entries_string = entries.join(", ");
                    if mg.len() > self.most_grateful_count {
                        // additional people have been grateful
                        format!(" (Most grateful {}: {})", self.most_grateful_count, entries_string)
                    } else {
                        // nobody else has been
                        format!(" ({})", entries_string)
                    }
                },
            }
        };

        interface.send_channel_message(
            &channel_message.channel.name,
            &format!(
                "@{} {} has {}{}.",
                channel_message.message.sender.username,
                target,
                count_phrase,
                most_grateful_suffix,
            ),
        ).await;
    }

    async fn handle_grateful(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let target = match self.find_user_mention(channel_message).await {
            Some(tw) => tw,
            None => return,
        };
        let db_client = match self.connect_db().await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let target_lower = target.to_lowercase();
        let count_row_res = db_client.query_one(
            r#"
                SELECT COUNT(*) count FROM thanks.thanks WHERE thanker_lowercase = $1 AND deleted = FALSE
            "#,
            &[&target_lower],
        ).await;
        let count_row = match count_row_res {
            Ok(cr) => cr,
            Err(e) => {
                error!("error querying thanks count: {}", e);
                return;
            },
        };
        let count: i64 = count_row.get(0);

        let count_phrase: String = match count {
            0 => "thanked nobody".into(),
            1 => "given thanks once".into(),
            other => format!("given thanks {} times", other),
        };

        let most_thanked_suffix = if count == 0 {
            String::new()
        } else {
            // also show stats
            let count_plus_one: i64 = (self.most_grateful_count + 1).try_into()
                .expect("most grateful count not representable as i64");
            let most_thanked_res = db_client.query(
                r#"
                    SELECT thankee_lowercase, COUNT(*) count
                    FROM thanks.thanks
                    WHERE thanker_lowercase = $1 AND deleted = FALSE
                    GROUP BY thankee_lowercase
                    ORDER BY count DESC
                    LIMIT $2
                "#,
                &[&target_lower, &count_plus_one],
            ).await;
            match most_thanked_res {
                Err(e) => {
                    error!("error querying most thanked: {}", e);
                    String::new()
                },
                Ok(mg) => {
                    let mut entries = Vec::new();
                    for row in mg.iter().take(self.most_grateful_count) {
                        let thankee: String = row.get(0);
                        let count: i64 = row.get(1);
                        entries.push(format!("{}: {}\u{D7}", thankee, count));
                    }
                    let entries_string = entries.join(", ");
                    if mg.len() > self.most_grateful_count {
                        // user thanked additional people
                        format!(" (Most thanked {}: {})", self.most_grateful_count, entries_string)
                    } else {
                        // user thanked nobody else
                        format!(" ({})", entries_string)
                    }
                },
            }
        };

        interface.send_channel_message(
            &channel_message.channel.name,
            &format!(
                "@{} {} has {}{}.",
                channel_message.message.sender.username,
                target,
                count_phrase,
                most_thanked_suffix,
            ),
        ).await;
    }

    async fn handle_topthanked(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let db_client = match self.connect_db().await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let count: i64 = self.most_grateful_count.try_into()
            .expect("most grateful count not representable as i64");
        let most_thanked_res = db_client.query(
            r#"
                SELECT thankee_lowercase, COUNT(*) count
                FROM thanks.thanks
                WHERE deleted = FALSE
                GROUP BY thankee_lowercase
                ORDER BY count DESC
                LIMIT $1
            "#,
            &[&count],
        ).await;
        match most_thanked_res {
            Err(e) => {
                error!("error querying most thanked: {}", e);
            },
            Ok(mg) => {
                let mut entries = Vec::new();
                for row in mg.iter().take(self.most_grateful_count) {
                    let thankee: String = row.get(0);
                    let count: i64 = row.get(1);
                    entries.push(format!("{}: {}\u{D7}", thankee, count));
                }
                let entries_string = entries.join(", ");
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!(
                        "@{} {}",
                        channel_message.message.sender.username,
                        entries_string,
                    ),
                ).await;
            },
        }
    }

    async fn handle_topgrateful(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let db_client = match self.connect_db().await {
            Ok(c) => c,
            Err(_e) => return,
        };

        let count: i64 = self.most_grateful_count.try_into()
            .expect("most grateful count not representable as i64");
        let most_thanked_res = db_client.query(
            r#"
                SELECT thanker_lowercase, COUNT(*) count
                FROM thanks.thanks
                WHERE deleted = FALSE
                GROUP BY thanker_lowercase
                ORDER BY count DESC
                LIMIT $1
            "#,
            &[&count],
        ).await;
        match most_thanked_res {
            Err(e) => {
                error!("error querying most grateful: {}", e);
            },
            Ok(mg) => {
                let mut entries = Vec::new();
                for row in mg.iter().take(self.most_grateful_count) {
                    let thanker: String = row.get(0);
                    let count: i64 = row.get(1);
                    entries.push(format!("{}: {}\u{D7}", thanker, count));
                }
                let entries_string = entries.join(", ");
                interface.send_channel_message(
                    &channel_message.channel.name,
                    &format!(
                        "@{} {}",
                        channel_message.message.sender.username,
                        entries_string,
                    ),
                ).await;
            },
        }
    }
}
#[async_trait]
impl RocketBotPlugin for ThanksPlugin {
    fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> Self {
        ThanksPlugin {
            interface,
            db_conn_string: config["db_conn_string"]
                .as_str().expect("db_conn_string is not a string")
                .to_owned(),
            most_grateful_count: config["most_grateful_count"]
                .as_usize().expect("most_grateful_count is not a usize"),
        }
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        if !channel_message.message.raw.starts_with("!") {
            return;
        }

        if channel_message.message.raw == "!topthanked" {
            self.handle_topthanked(channel_message).await
        } else if channel_message.message.raw == "!topgrateful" {
            self.handle_topgrateful(channel_message).await
        } else if let Some(frag) = channel_message.message.parsed.get(0) {
            if let MessageFragment::Paragraph(pieces) = frag {
                if let Some(cmd) = pieces.get(0) {
                    if let InlineFragment::PlainText(cmd) = cmd {
                        if !cmd.starts_with("!") {
                            return;
                        }

                        if cmd.starts_with("!thank ") || cmd.starts_with("!thanks ") || cmd.starts_with("!thx ") {
                            self.handle_thank(channel_message).await
                        } else if cmd.starts_with("!thanked ") {
                            self.handle_thanked(channel_message).await
                        } else if cmd.starts_with("!grateful ") {
                            self.handle_grateful(channel_message).await
                        }
                    }
                }
            }
        }
    }
}
