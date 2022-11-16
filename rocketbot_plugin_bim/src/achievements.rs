use std::collections::BTreeMap;

use chrono::{DateTime, Local};
use rocketbot_bim_achievements::AchievementState;
use tokio_postgres;


pub(crate) async fn get_all_achievements(db_conn: &tokio_postgres::Client) -> Result<BTreeMap<String, BTreeMap<i64, AchievementState>>, tokio_postgres::Error> {
    let rows = db_conn.query(
        "SELECT ra.rider_username, ra.achievement_id, ra.achieved_on FROM bim.rider_achievements ra",
        &[],
    ).await?;
    let mut achievements = BTreeMap::new();
    for row in rows {
        let username: String = row.get(0);
        let id: i64 = row.get(1);
        let timestamp: Option<DateTime<Local>> = row.get(2);

        achievements
            .entry(username)
            .or_insert_with(|| BTreeMap::new())
            .insert(
                id,
                AchievementState::new(
                    id,
                    timestamp.map(|ts| ts.into()),
                ),
            );
    }
    Ok(achievements)
}


pub(crate) async fn get_achievements_for(db_conn: &tokio_postgres::Client, rider_username: &str) -> Result<BTreeMap<i64, AchievementState>, tokio_postgres::Error> {
    let rows = db_conn.query(
        "SELECT ra.achievement_id, ra.achieved_on FROM bim.rider_achievements ra WHERE ra.rider_username = $1",
        &[&rider_username],
    ).await?;
    let mut achievements = BTreeMap::new();
    for row in rows {
        let id: i64 = row.get(0);
        let timestamp: Option<DateTime<Local>> = row.get(1);

        achievements.insert(
            id,
            AchievementState::new(
                id,
                timestamp.map(|ts| ts.into()),
            ),
        );
    }
    Ok(achievements)
}

pub(crate) async fn recalculate_achievements(db_conn: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    db_conn.execute("REFRESH MATERIALIZED VIEW bim.rider_achievements WITH DATA", &[]).await?;
    Ok(())
}
