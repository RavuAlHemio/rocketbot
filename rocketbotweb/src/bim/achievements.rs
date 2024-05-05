use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;

use askama::Template;
use chrono::{DateTime, Local};
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use log::error;
use rocketbot_bim_common::achievements::{AchievementDef, ACHIEVEMENT_DEFINITIONS};
use rocketbot_date_time::DateTimeLocalWithWeekday;
use serde::Serialize;

use crate::{get_query_pairs, render_response, return_405, return_500};
use crate::bim::connect_to_db;


#[derive(Clone, Debug, Eq, PartialEq, Serialize, Template)]
#[template(path = "bimachievements.html")]
struct BimAchievementsTemplate {
    pub achievement_to_rider_to_timestamp: HashMap<i64, HashMap<String, DateTimeLocalWithWeekday>>,
    pub all_achievements: Vec<AchievementDef>,
    pub all_riders: BTreeSet<String>,
}


pub(crate) async fn handle_bim_achievements(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    // query achievements
    let ach_rows_res = db_conn.query(
        "
            SELECT ra.rider_username, ra.achievement_id, ra.achieved_on
            FROM
                bim.rider_achievements ra
            ORDER BY
                ra.rider_username, ra.achievement_id
        ",
        &[],
    ).await;
    let ach_rows = match ach_rows_res {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying vehicle rides: {}", e);
            return return_500();
        },
    };

    let all_achievements = ACHIEVEMENT_DEFINITIONS.iter()
        .map(|ad| ad.clone())
        .collect();

    let mut all_riders = BTreeSet::new();
    let mut achievement_to_rider_to_timestamp = HashMap::new();
    for ach in ach_rows {
        let rider: String = ach.get(0);
        let achievement_id: i64 = ach.get(1);
        let achieved_on_odtl: Option<DateTime<Local>> = ach.get(2);

        let achieved_on = match achieved_on_odtl {
            Some(dtl) => DateTimeLocalWithWeekday(dtl),
            None => continue,
        };

        all_riders.insert(rider.clone());
        achievement_to_rider_to_timestamp
            .entry(achievement_id)
            .or_insert_with(|| HashMap::new())
            .insert(rider, achieved_on);
    }

    let template = BimAchievementsTemplate {
        achievement_to_rider_to_timestamp,
        all_riders,
        all_achievements,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
