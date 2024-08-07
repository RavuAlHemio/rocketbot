use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;

use askama::Template;
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::{CONFIG, connect_to_db, get_query_pairs, render_response, return_405, return_500};


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "topquotes.html")]
struct TopQuotesTemplate {
    pub quotes: Vec<TopQuotePart>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct TopQuotePart {
    pub score: i64,
    pub score_changed: bool,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "quotesvotes.html")]
struct QuotesVotesPart {
    pub quotes: Vec<QuoteVotesPart>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct QuoteVotesPart {
    pub id: i64,
    pub body: String,
    pub score: i64,
    pub votes: Vec<QuoteVotePart>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct QuoteVotePart {
    pub voter: String,
    pub value: i64,
}


async fn get_table_prefix<'a>(query_pairs: &'a HashMap<Cow<'a, str>, Cow<'a, str>>) -> &'a str {
    let table_prefix = match query_pairs.get("table-prefix") {
        Some(tp) => tp,
        None => return "",
    };

    {
        let config_guard = CONFIG
            .get().expect("CONFIG not set?!")
            .read().await;
        if config_guard.quotes_table_prefixes.len() == 0 {
            // no prefixes configured => don't allow any prefixes
            return "";
        }
        if config_guard.quotes_table_prefixes.contains(table_prefix.as_ref()) {
            table_prefix.as_ref()
        } else {
            ""
        }
    }
}


pub(crate) async fn handle_top_quotes(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    let table_prefix = get_table_prefix(&query_pairs).await;

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut quotes: Vec<TopQuotePart> = Vec::new();
    let query_res = db_conn.query(
        &format!(
            "
                SELECT
                    q.quote_id, q.author, q.message_type, q.body, CAST(COALESCE(SUM(CAST(v.points AS bigint)), 0) AS bigint) vote_sum
                FROM
                    quotes.{}quotes q
                    LEFT OUTER JOIN quotes.{}quote_votes v ON v.quote_id = q.quote_id
                GROUP BY
                    q.quote_id, q.author, q.message_type, q.body
                ORDER BY
                    -- on vote tie, prefer newer quotes
                    vote_sum DESC, quote_id DESC
            ",
            table_prefix,
            table_prefix,
        ),
        &[],
    ).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query top quotes: {}", e);
            return return_500();
        },
    };
    let mut last_score = None;
    for row in rows {
        //let quote_id: i64 = row.get(0);
        let author: String = row.get(1);
        let message_type: String = row.get(2);
        let body_in_db: String = row.get(3);
        let vote_sum_opt: Option<i64> = row.get(4);

        let vote_sum = vote_sum_opt.unwrap_or(0);

        let score_changed = if last_score != Some(vote_sum) {
            last_score = Some(vote_sum);
            true
        } else {
            false
        };

        // render the quote
        let body = match message_type.as_str() {
            "F" => body_in_db,
            "M" => format!("<{}> {}", author, body_in_db),
            "A" => format!("* {} {}", author, body_in_db),
            other => format!("{}? <{}> {}", other, author, body_in_db),
        };
        quotes.push(TopQuotePart {
            score: vote_sum,
            score_changed,
            body,
        });
    }

    let template = TopQuotesTemplate {
        quotes,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}


pub(crate) async fn handle_quotes_votes(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);
    let table_prefix = get_table_prefix(&query_pairs).await;

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut quotes: Vec<QuoteVotesPart> = Vec::new();
    let query_res = db_conn.query(
        &format!(
            "
                SELECT q.quote_id, q.author, q.message_type, q.body
                FROM quotes.{}quotes q
                ORDER BY q.quote_id DESC
            ",
            table_prefix,
        ),
        &[],
    ).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query top quotes: {}", e);
            return return_500();
        },
    };
    for row in rows {
        let quote_id: i64 = row.get(0);
        let author: String = row.get(1);
        let message_type: String = row.get(2);
        let body_in_db: String = row.get(3);

        // render the quote
        let body = match message_type.as_str() {
            "F" => body_in_db,
            "M" => format!("<{}> {}", author, body_in_db),
            "A" => format!("* {} {}", author, body_in_db),
            other => format!("{}? <{}> {}", other, author, body_in_db),
        };
        quotes.push(QuoteVotesPart {
            id: quote_id,
            body,
            score: 0,
            votes: vec![],
        });
    }

    // add votes
    let vote_statement_res = db_conn.prepare(&format!(
        "
            SELECT
                v.voter_lowercase,
                CAST(v.points AS bigint)
            FROM
                quotes.{}quote_votes v
            WHERE
                v.quote_id = $1
            ORDER BY
                v.vote_id
        ",
        table_prefix,
    )).await;
    let vote_statement = match vote_statement_res {
        Ok(s) => s,
        Err(e) => {
            error!("failed to prepare vote statement: {}", e);
            return return_500();
        },
    };
    for quote in &mut quotes {
        let rows = match db_conn.query(&vote_statement, &[&quote.id]).await {
            Ok(r) => r,
            Err(e) => {
                error!("failed to obtain votes for quote {}: {}", quote.id, e);
                return return_500();
            },
        };
        let mut votes = Vec::new();
        let mut total_points: i64 = 0;
        for row in &rows {
            let voter: String = row.get(0);
            let points: i64 = row.get(1);
            total_points += points;
            votes.push(QuoteVotePart {
                voter,
                value: points,
            });
        }
        quote.score = total_points;
        quote.votes = votes;
    }

    let template = QuotesVotesPart {
        quotes,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
