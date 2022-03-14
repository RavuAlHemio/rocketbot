use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;

use askama::Template;
use hyper::{Body, Method, Request, Response};
use log::error;
use serde::{Deserialize, Serialize};

use crate::{connect_to_db, get_query_pairs, render_response, return_405, return_500};
use crate::templating::filters;


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Template)]
#[template(path = "thanks.html")]
struct ThanksTemplate {
    pub users: Vec<String>,
    pub thanks_from_to: Vec<Vec<i64>>,
    pub total_given: Vec<i64>,
    pub total_received: Vec<i64>,
    pub total_count: i64,
}


pub(crate) async fn handle_thanks(request: &Request<Body>) -> Result<Response<Body>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut user_name_set = BTreeSet::new();
    let mut thanks_counts: HashMap<(String, String), i64> = HashMap::new();
    let query_res = db_conn.query("
        SELECT t.thanker_lowercase, t.thankee_lowercase, COUNT(*) thanks_count
        FROM thanks.thanks t
        WHERE t.deleted = FALSE
        GROUP BY t.thanker_lowercase, t.thankee_lowercase
    ", &[]).await;
    let rows = match query_res {
        Ok(r) => r,
        Err(e) => {
            error!("failed to query thanks: {}", e);
            return return_500();
        },
    };
    for row in rows {
        let thanker: String = row.get(0);
        let thankee: String = row.get(1);
        let count: i64 = row.get(2);

        user_name_set.insert(thanker.clone());
        user_name_set.insert(thankee.clone());

        thanks_counts.insert((thanker, thankee), count);
    }

    let users: Vec<String> = user_name_set.iter()
        .map(|un| un.clone())
        .collect();

    // complete the values
    for thanker in &users {
        for thankee in &users {
            thanks_counts.entry((thanker.clone(), thankee.clone()))
                .or_insert(0);
        }
    }

    let mut total_given: Vec<i64> = vec![0; users.len()];
    let mut total_received: Vec<i64> = vec![0; users.len()];

    let mut thanks_from_to: Vec<Vec<i64>> = Vec::with_capacity(users.len());
    for _ in 0..users.len() {
        thanks_from_to.push(vec![0; users.len()]);
    }
    let mut total_count = 0;

    for (r, thanker) in users.iter().enumerate() {
        for (e, thankee) in users.iter().enumerate() {
            let pair_count = *thanks_counts.get(&(thanker.clone(), thankee.clone())).unwrap();

            total_count += pair_count;
            total_given[r] += pair_count;
            total_received[e] += pair_count;
            thanks_from_to[r][e] = pair_count;
        }
    }

    let template = ThanksTemplate {
        users,
        thanks_from_to,
        total_given,
        total_received,
        total_count,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
