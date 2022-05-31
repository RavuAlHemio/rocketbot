use std::collections::BTreeMap;

use chrono::{DateTime, Local};
use tokio_postgres;


pub(crate) static ACHIEVEMENT_DEFINITIONS: [AchievementDef; 21] = [
    AchievementDef::new(1, "Beastly", "Ride a vehicle (of any company) with number 666."),
    AchievementDef::new(2, "Nice", "Ride a vehicle (of any company) with number 69."),
    AchievementDef::new(3, "Home Line", "Ride a vehicle (of any company) where the vehicle number and the line are the same."),
    AchievementDef::new(4, "Two of a Kind", "Ride a vehicle (of any company) whose number consists of one digit repeated at least twice."),
    AchievementDef::new(5, "Three of a Kind", "Ride a vehicle (of any company) whose number consists of one digit repeated at least three times."),
    AchievementDef::new(6, "Four of a Kind", "Ride a vehicle (of any company) whose number consists of one digit repeated at least four times."),
    AchievementDef::new(7, "Palindrome", "Ride a vehicle (of any company) whose number is a palindrome while not being all the same digit."),
    AchievementDef::new(8, "Mirror Home Line", "Ride a vehicle (of any company) where the vehicle number is the reverse of the line."),
    AchievementDef::new(9, "Boeing", "Ride a vehicle (of any company) whose number has the pattern \"7x7\"."),
    AchievementDef::new(10, "Elsewhere", "Ride two vehicles with the same vehicle number but different companies."),
    AchievementDef::new(11, "Monthiversary", "Ride the same vehicle on the same day of two consecutive months."),
    AchievementDef::new(12, "Anniversary", "Ride the same vehicle on the same day of two consecutive months."),
    AchievementDef::new(13, "Same Time Next Week", "Ride the same vehicle on the same weekday of two consecutive weeks."),
    AchievementDef::new(14, "Five Sweep", "Collect rides with five vehicles of the same company with consecutive numbers."),
    AchievementDef::new(15, "Ten Sweep", "Collect rides with ten vehicles of the same company with consecutive numbers."),
    AchievementDef::new(16, "Twenty Sweep", "Collect rides with twenty vehicles of the same company with consecutive numbers."),
    AchievementDef::new(17, "Thirty Sweep", "Collect rides with thirty vehicles of the same company with consecutive numbers."),
    AchievementDef::new(18, "Forty Sweep", "Collect rides with forty vehicles of the same company with consecutive numbers."),
    AchievementDef::new(19, "Half-Century Sweep", "Collect rides with fifty vehicles of the same company with consecutive numbers."),
    AchievementDef::new(20, "Nice Sweep", "Collect rides with sixty-nine vehicles of the same company with consecutive numbers."),
    AchievementDef::new(21, "Century Sweep", "Collect rides with one hundred vehicles of the same company with consecutive numbers."),
];


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct AchievementDef {
    pub id: i64,
    pub name: &'static str,
    pub description: &'static str,
}
impl AchievementDef {
    pub const fn new(id: i64, name: &'static str, description: &'static str) -> Self {
        Self { id, name, description }
    }
}


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct AchievementState {
    pub id: i64,
    pub timestamp: Option<DateTime<Local>>,
}
impl AchievementState {
    pub const fn new(id: i64, timestamp: Option<DateTime<Local>>) -> Self {
        Self { id, timestamp }
    }
}


pub(crate) async fn get_achievements_for(db_conn: &tokio_postgres::Client, rider_username: &str) -> Result<BTreeMap<i64, AchievementState>, tokio_postgres::Error> {
    let rows = db_conn.query(
        "SELECT achievement_id, achieved_on FROM bim.achievements_of($1)",
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
                timestamp,
            ),
        );
    }
    Ok(achievements)
}
