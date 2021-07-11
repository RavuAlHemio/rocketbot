mod model;


use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::TryInto;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use log::error;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use tokio::sync::Mutex;
use tokio_postgres::NoTls;
use tokio_postgres::types::ToSql;

use crate::model::{MessageType, Quote, QuoteAndVoteSum};


struct QuotesState {
    pub potential_quotes_per_channel_name: HashMap<String, VecDeque<Quote>>,
    pub last_quote_id_per_channel_name: HashMap<String, i64>,
    pub rng: StdRng,
    pub shuffled_good_quotes: Option<Vec<QuoteAndVoteSum>>,
    pub shuffled_any_quotes: Option<Vec<QuoteAndVoteSum>>,
    pub shuffled_bad_quotes: Option<Vec<QuoteAndVoteSum>>,
    pub shuffled_good_quotes_index: usize,
    pub shuffled_any_quotes_index: usize,
    pub shuffled_bad_quotes_index: usize,
}
impl QuotesState {
    pub fn new(
        potential_quotes_per_channel_name: HashMap<String, VecDeque<Quote>>,
        last_quote_id_per_channel_name: HashMap<String, i64>,
        rng: StdRng,
        shuffled_good_quotes: Option<Vec<QuoteAndVoteSum>>,
        shuffled_any_quotes: Option<Vec<QuoteAndVoteSum>>,
        shuffled_bad_quotes: Option<Vec<QuoteAndVoteSum>>,
        shuffled_good_quotes_index: usize,
        shuffled_any_quotes_index: usize,
        shuffled_bad_quotes_index: usize,
    ) -> QuotesState {
        QuotesState {
            potential_quotes_per_channel_name,
            last_quote_id_per_channel_name,
            rng,
            shuffled_good_quotes,
            shuffled_any_quotes,
            shuffled_bad_quotes,
            shuffled_good_quotes_index,
            shuffled_any_quotes_index,
            shuffled_bad_quotes_index,
        }
    }
}


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum QuoteRating {
    Low,
    Any,
    High,
}


fn substring_to_like(substring: &str, escape_char: char) -> String {
    let mut ret = String::with_capacity(substring.len() + 2);
    ret.push('%');
    for c in substring.chars() {
        if c == escape_char || c == '%' || c == '_' {
            ret.push(escape_char);
        }
        ret.push(c);
    }
    ret.push('%');
    ret
}


pub struct QuotesPlugin {
    interface: Weak<dyn RocketBotInterface>,
    db_conn_string: String,
    remember_posts_for_quotes: usize,
    vote_threshold: i64,
    quotes_state: Mutex<QuotesState>,
}
impl QuotesPlugin {
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

    async fn get_filtered_quotes(
        &self,
        requested_rating: QuoteRating,
        author_username: Option<&str>,
        body_substring: Option<&str>,
    ) -> Result<Vec<QuoteAndVoteSum>, tokio_postgres::Error> {
        let quote_query_format = "
SELECT q.quote_id, q.timestamp, q.channel, q.author, q.message_type, q.body, COALESCE(SUM(qv.points), 0) vote_sum
FROM quotes.quotes q
LEFT OUTER JOIN quotes.quote_votes qv ON qv.quote_id = q.quote_id
{{where_filter}}
GROUP BY q.quote_id, q.timestamp, q.channel, q.author, q.message_type, q.body
{{having_filter}}
";

        let mut having_filter_pieces: Vec<String> = Vec::new();
        let mut having_filter_values: Vec<&(dyn ToSql + Sync)> = Vec::new();
        let mut where_filter_pieces: Vec<String> = Vec::new();
        let mut where_filter_values: Vec<&(dyn ToSql + Sync)> = Vec::new();

        let mut next_filter_index: usize = 1;
        if requested_rating == QuoteRating::High {
            having_filter_pieces.push(format!("COALESCE(SUM(qv.points), 0) >= ${}", next_filter_index));
            next_filter_index += 1;
            having_filter_values.push(&self.vote_threshold);
        } else if requested_rating == QuoteRating::Low {
            having_filter_pieces.push(format!("COALESCE(SUM(qv.points), 0) < ${}", next_filter_index));
            next_filter_index += 1;
            having_filter_values.push(&self.vote_threshold);
        }

        if author_username.is_some() {
            where_filter_pieces.push(format!("q.author = ${}", next_filter_index));
            next_filter_index += 1;
            where_filter_values.push(&author_username);
        }

        let mut body_substring_like = String::new();
        if body_substring.is_some() {
            body_substring_like = substring_to_like(body_substring.unwrap(), '\\');
            where_filter_pieces.push(format!("q.body LIKE ${} ESCAPE '\\'", next_filter_index));
            next_filter_index += 1;
            where_filter_values.push(&body_substring_like);
        }

        let mut all_filter_pieces: Vec<String> = Vec::new();
        all_filter_pieces.append(&mut having_filter_pieces);
        all_filter_pieces.append(&mut where_filter_pieces);

        let mut all_filter_values: Vec<&(dyn ToSql + Sync)> = Vec::new();
        all_filter_values.append(&mut having_filter_values);
        all_filter_values.append(&mut where_filter_values);

        let having_filter = if having_filter_pieces.len() > 0 {
            String::new()
        } else {
            format!("HAVING {}", having_filter_pieces.join(", "))
        };

        let where_filter = if where_filter_pieces.len() > 0 {
            String::new()
        } else {
            format!("WHERE {}", where_filter_pieces.join(", "))
        };

        let quote_query = quote_query_format
            .replace(
                "{{having_filter}}",
                &having_filter,
            )
            .replace(
                "{{where_filter}}",
                &where_filter,
            );

        let db_client = self.connect_db().await?;
        let rows = db_client.query(quote_query.as_str(), &all_filter_values).await?;

        let mut ret = Vec::new();
        for row in &rows {
            let quote_id: i64 = row.get(0);
            let timestamp: DateTime<Utc> = row.get(1);
            let channel: String = row.get(2);
            let author: String = row.get(3);
            let message_type_text: String = row.get(4);
            let body: String = row.get(5);
            let vote_sum_opt: Option<i64> = row.get(6);

            let message_type: MessageType = message_type_text
                .chars()
                .nth(0).expect("message type is too short")
                .try_into().expect("message type is invalid");
            let vote_sum = vote_sum_opt.unwrap_or(0);

            let quote_and_vote_sum = QuoteAndVoteSum::new(
                Quote::new(
                    quote_id,
                    timestamp,
                    channel,
                    author,
                    message_type,
                    body,
                ),
                vote_sum,
            );
            ret.push(quote_and_vote_sum);
        }

        Ok(ret)
    }

    async fn post_quote(
        &self,
        quote_and_vote_sum: &QuoteAndVoteSum,
        requestor_username: &str,
        channel_name: &str,
        add_my_rating: bool,
    ) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let mut state_guard = self.quotes_state
            .lock().await;

        // remember last ID (for upquote/downquote commands)
        state_guard.last_quote_id_per_channel_name.insert(
            channel_name.to_owned(),
            quote_and_vote_sum.quote.id,
        );
        let requestor_vote: &str = if add_my_rating {
            // find the rating
            let db_client = match self.connect_db().await {
                Ok(dbc) => dbc,
                Err(e) => {
                    error!("failed to connect to database: {}", e);
                    return;
                },
            };
            let rows_res = db_client.query_opt(
                "SELECT points FROM quotes.quote_votes WHERE quote_id = $1 AND voter_lowercase = $2",
                &[&quote_and_vote_sum.quote.id, &requestor_username],
            ).await;
            match rows_res {
                Ok(Some(r)) => {
                    let vote: i16 = r.get(0);
                    match vote {
                        -1 => "-",
                        0 => " ",
                        1 => "+",
                        _ => {
                            error!("invalid vote points: {}", vote);
                            return;
                        },
                    }
                },
                Ok(None) => {
                    " "
                },
                Err(e) => {
                    error!("failed to obtain rating: {}", e);
                    return;
                },
            }
        } else {
            ""
        };

        let quote_text = quote_and_vote_sum.format_output(requestor_vote);
        interface.send_channel_message(
            channel_name,
            &quote_text,
        ).await;
    }

    async fn post_random_quote(
        &self,
        requestor_username: &str,
        channel_name: &str,
        quotes_and_vote_sums: &[QuoteAndVoteSum],
        add_my_rating: bool,
    ) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let quote_count = quotes_and_vote_sums.len();
        if quote_count > 0 {
            let index: usize = self.quotes_state
                .lock().await
                .rng.gen_range(0..quote_count);
            let quote_and_vote_sum = &quotes_and_vote_sums[index];
            self.post_quote(
                quote_and_vote_sum,
                requestor_username,
                channel_name,
                add_my_rating,
            ).await;
        } else {
            interface.send_channel_message(
                channel_name,
                &format!("@{} Sorry, I don't have any matching quotes.", requestor_username),
            ).await;
        }
    }

    async fn insert_quote(&self, new_quote: &Quote) -> Result<(), tokio_postgres::Error> {
        let db_client = self.connect_db().await?;

        let message_type_char: char = new_quote.message_type.into();
        let mut message_type_string = String::with_capacity(1);
        message_type_string.push(message_type_char);

        db_client.execute(
            r#"
INSERT INTO quotes.quotes ("timestamp", channel, author, message_type, body)
VALUES ($1, $2, $3, $4, $5)
            "#,
            &[&new_quote.timestamp, &new_quote.channel, &new_quote.author, &message_type_string, &new_quote.body],
        ).await?;
        Ok(())
    }

    async fn handle_addquote(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let new_quote = Quote::new(
            -1,
            Utc::now(),
            channel_message.channel.name.clone(),
            channel_message.message.sender.username.clone(),
            MessageType::FreeForm,
            command.rest.clone(),
        );
        if let Err(e) = self.insert_quote(&new_quote).await {
            error!("failed to insert quote: {}", e);
            interface.send_channel_message(
                &channel_message.channel.name,
                &format!(
                    "@{} Failed to store the quote, sorry!",
                    channel_message.message.sender.username,
                ),
            ).await;
        }
    }

    async fn handle_remember(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let mut quotes_lock = self.quotes_state.lock().await;
        let mut quotes_state = &mut *quotes_lock;

        let channel_name = &channel_message.channel.name;
        let sender_username = &channel_message.message.sender.username;
        let quote_username = &command.args[0];
        let substring = &command.rest;

        let pot_quotes_opt = quotes_state.potential_quotes_per_channel_name
            .get_mut(channel_name);
        let pot_quotes = match pot_quotes_opt {
            None => return,
            Some(pq) => pq,
        };

        let mut remove_index: Option<usize> = None;
        for (i, pot_quote) in pot_quotes.iter_mut().enumerate() {
            if &pot_quote.author != quote_username || !pot_quote.body.contains(substring) {
                continue;
            }

            if quote_username == sender_username {
                interface.send_channel_message(
                    channel_name,
                    &format!(
                        "@{} Sorry, someone else has to remember your quotes!",
                        channel_message.message.sender.username,
                    ),
                ).await;
                return;
            }

            if let Err(e) = self.insert_quote(&pot_quote).await {
                error!("failed to insert new quote: {}", e);
                interface.send_channel_message(
                    channel_name,
                    &format!(
                        "@{} Failed to store the quote, sorry!",
                        channel_message.message.sender.username,
                    ),
                ).await;
                return;
            }

            interface.send_channel_message(
                channel_name,
                &format!("Remembering {}", pot_quote),
            ).await;

            remove_index = Some(i);

            // invalidate
            quotes_state.shuffled_any_quotes = None;
            quotes_state.shuffled_bad_quotes = None;
            quotes_state.shuffled_good_quotes = None;
            break;
        }

        if let Some(ri) = remove_index {
            // successfully stored; remove from potential quotes
            pot_quotes.remove(ri);
            return;
        }

        interface.send_channel_message(
            channel_name,
            &format!(
                "@{} Sorry, I don't remember what {} said about {:?}.",
                sender_username, quote_username, substring,
            ),
        ).await;
    }

    async fn quote_rating_from_command(&self, command: &CommandInstance, channel_name: &str, sender_username: &str) -> Option<QuoteRating> {
        let interface = match self.interface.upgrade() {
            None => return None,
            Some(i) => i,
        };

        let quote_rating = if command.flags.contains("any") {
            if command.flags.contains("bad") {
                interface.send_channel_message(
                    channel_name,
                    &format!(
                        "@{} Options `--any` and `--bad` cannot be used simultaneously.",
                        sender_username,
                    ),
                ).await;
                return None;
            }
            QuoteRating::Any
        } else if command.flags.contains("bad") {
            QuoteRating::Low
        } else {
            QuoteRating::High
        };

        Some(quote_rating)
    }

    async fn handle_quote_quoteuser(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let channel_name = &channel_message.channel.name;
        let sender_username = &channel_message.message.sender.username;

        let author_username = if command.name == "quoteuser" {
            Some(command.args[0].as_str())
        } else {
            None
        };
        let substring = if command.rest.len() > 0 {
            Some(command.rest.as_str())
        } else {
            None
        };

        let quote_rating = match self.quote_rating_from_command(command, channel_name, sender_username).await {
            Some(qr) => qr,
            None => return,
        };
        let show_rating = command.flags.contains("r");

        let relevant_quotes_res = self.get_filtered_quotes(
            quote_rating,
            author_username,
            substring,
        ).await;
        let relevant_quotes = match relevant_quotes_res {
            Ok(rq) => rq,
            Err(e) => {
                error!("failed to fetch quotes: {}", e);
                return;
            },
        };

        self.post_random_quote(
            sender_username,
            channel_name,
            &relevant_quotes,
            show_rating,
        ).await;
    }

    async fn handle_nextquote(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let channel_name = &channel_message.channel.name;
        let sender_username = &channel_message.message.sender.username;

        let quote_rating = match self.quote_rating_from_command(command, channel_name, sender_username).await {
            Some(qr) => qr,
            None => return,
        };
        let show_rating = command.flags.contains("r");

        {
            let mut quotes_guard = self.quotes_state.lock().await;
            let quotes_state = &mut *quotes_guard;

            let mut rng = &mut quotes_state.rng;
            let (shuffled_quotes, shuffled_index) = match quote_rating {
                QuoteRating::Low => (&mut quotes_state.shuffled_bad_quotes, &mut quotes_state.shuffled_bad_quotes_index),
                QuoteRating::Any => (&mut quotes_state.shuffled_any_quotes, &mut quotes_state.shuffled_any_quotes_index),
                QuoteRating::High => (&mut quotes_state.shuffled_good_quotes, &mut quotes_state.shuffled_good_quotes_index),
            };

            match shuffled_quotes {
                Some(sq) => {
                    if sq.len() == 0 {
                        interface.send_channel_message(
                            channel_name,
                            &format!("@{} Sorry, I don't have any matching quotes.", sender_username),
                        ).await;
                        return;
                    }

                    {
                        let quote_and_vote_sum = &sq[*shuffled_index];
                        self.post_quote(
                            quote_and_vote_sum,
                            sender_username,
                            channel_name,
                            show_rating,
                        ).await;
                    }

                    *shuffled_index += 1;
                    if *shuffled_index >= sq.len() {
                        // re-shuffle quotes
                        sq.shuffle(&mut rng);
                        *shuffled_index = 0;
                    }
                },
                None => {
                    let fresh_quotes_res = self.get_filtered_quotes(
                        quote_rating,
                        None,
                        None,
                    ).await;
                    let mut fresh_quotes = match fresh_quotes_res {
                        Ok(fq) => fq,
                        Err(e) => {
                            error!("failed to update nextquote list for rating {:?}: {}", quote_rating, e);
                            return;
                        },
                    };
                    if fresh_quotes.len() == 0 {
                        interface.send_channel_message(
                            channel_name,
                            &format!("@{} Sorry, I don't have any matching quotes.", sender_username),
                        ).await;
                        return;
                    } else {
                        fresh_quotes.shuffle(&mut rng);

                        let quote_and_vote_sum = &fresh_quotes[0];
                        self.post_quote(
                            quote_and_vote_sum,
                            sender_username,
                            channel_name,
                            show_rating,
                        ).await;
                    }

                    *shuffled_quotes = Some(fresh_quotes);
                    *shuffled_index = 1;
                },
            }
        }
    }

    async fn upsert_vote(&self, quote_id: i64, voter_username: &str, vote_points: i16) -> Result<(), tokio_postgres::Error> {
        let db_client = self.connect_db().await?;
        db_client.execute(
            r#"
INSERT INTO quotes.quote_votes (quote_id, voter_lowercase, points) VALUES ($1, $2, $3)
ON CONFLICT (quote_id, voter_lowercase) DO UPDATE SET points = excluded.points
            "#,
            &[&quote_id, &voter_username, &vote_points],
        ).await?;
        Ok(())
    }

    async fn handle_vote(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let vote_points: i16 = if command.name == "upquote" || command.name == "uq" {
            1
        } else if command.name == "downquote" || command.name == "dq" {
            -1
        } else {
            panic!("unexpected command {:?} in handle_vote", command.name);
        };

        let voter_username = &channel_message.message.sender.username;
        let channel_name = &channel_message.channel.name;
        let state_guard = self.quotes_state.lock().await;
        let quote_id = match state_guard.last_quote_id_per_channel_name.get(channel_name) {
            Some(qid) => *qid,
            None => {
                interface.send_channel_message(
                    channel_name,
                    &format!(
                        "@{} You'll have to get a quote first...",
                        voter_username,
                    ),
                ).await;
                return;
            },
        };

        if let Err(e) = self.upsert_vote(quote_id, voter_username, vote_points).await {
            error!("failed to upsert vote: {}", e);
            interface.send_channel_message(
                &channel_message.channel.name,
                &format!(
                    "@{} Failed to upsert your vote, sorry!",
                    channel_message.message.sender.username,
                ),
            ).await;
        }
    }
}
#[async_trait]
impl RocketBotPlugin for QuotesPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: json::JsonValue) -> Self where Self: Sized {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let db_conn_string = config["db_conn_string"]
            .as_str().expect("db_conn_string missing or not a string")
            .to_owned();
        let remember_posts_for_quotes = config["remember_posts_for_quotes"]
            .as_usize().unwrap_or(30);
        let vote_threshold = config["vote_threshold"]
            .as_i64().unwrap_or(-3);

        let quotes_state = Mutex::new(QuotesState::new(
            HashMap::new(),
            HashMap::new(),
            StdRng::from_entropy(),
            None,
            None,
            None,
            0,
            0,
            0,
        ));

        let addquote_command = CommandDefinition::new(
            "addquote".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
        );
        my_interface.register_channel_command(&addquote_command).await;

        let remember_command = CommandDefinition::new(
            "remember".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            1,
        );
        my_interface.register_channel_command(&remember_command).await;

        let mut quote_flags = HashSet::new();
        quote_flags.insert("any".to_owned());
        quote_flags.insert("bad".to_owned());
        quote_flags.insert("r".to_owned());

        let quote_command = CommandDefinition::new(
            "quote".to_owned(),
            Some(quote_flags.clone()),
            HashMap::new(),
            0,
        );
        my_interface.register_channel_command(&quote_command).await;

        let quoteuser_command = CommandDefinition::new(
            "quoteuser".to_owned(),
            Some(quote_flags.clone()),
            HashMap::new(),
            1,
        );
        my_interface.register_channel_command(&quoteuser_command).await;

        let nextquote_command = CommandDefinition::new(
            "nextquote".to_owned(),
            Some(quote_flags.clone()),
            HashMap::new(),
            0,
        );
        my_interface.register_channel_command(&nextquote_command).await;

        let upquote_command = CommandDefinition::new(
            "upquote".to_owned(),
            Some(quote_flags.clone()),
            HashMap::new(),
            0,
        );
        let uq_command = upquote_command.copy_named("uq");
        let downquote_command = upquote_command.copy_named("downquote");
        let dq_command = upquote_command.copy_named("dq");
        my_interface.register_channel_command(&upquote_command).await;
        my_interface.register_channel_command(&uq_command).await;
        my_interface.register_channel_command(&downquote_command).await;
        my_interface.register_channel_command(&dq_command).await;

        QuotesPlugin {
            interface,
            db_conn_string,
            remember_posts_for_quotes,
            vote_threshold,
            quotes_state,
        }
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let potential_quote = Quote::new(
            -1,
            Utc::now(),
            channel_message.channel.name.clone(),
            channel_message.message.sender.username.clone(),
            MessageType::Message,
            channel_message.message.raw.clone(),
        );

        {
            let mut state_guard = self.quotes_state.lock().await;
            let pot_quotes = state_guard.potential_quotes_per_channel_name
                .entry(channel_message.channel.name.clone())
                .or_insert_with(|| VecDeque::with_capacity(self.remember_posts_for_quotes + 1));

            // add potential quote
            pot_quotes.push_front(potential_quote);

            // clear out supernumerary ones
            while pot_quotes.len() > self.remember_posts_for_quotes {
                pot_quotes.pop_back();
            }
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "addquote" {
            self.handle_addquote(channel_message, command).await
        } else if command.name == "remember" {
            self.handle_remember(channel_message, command).await
        } else if command.name == "quote" {
            self.handle_quote_quoteuser(channel_message, command).await
        } else if command.name == "quoteuser" {
            self.handle_quote_quoteuser(channel_message, command).await
        } else if command.name == "nextquote" {
            self.handle_nextquote(channel_message, command).await
        } else if command.name == "upquote" || command.name == "uq" || command.name == "downquote" || command.name == "dq" {
            self.handle_vote(channel_message, command).await
        }
    }
}
