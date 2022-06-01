mod definitions;


use rocketbot_date_time::DateTimeLocalWithWeekday;
use serde::Serialize;

pub use crate::definitions::ACHIEVEMENT_DEFINITIONS;


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AchievementDef {
    pub id: i64,
    pub name: &'static str,
    pub description: &'static str,
}
impl AchievementDef {
    pub const fn new(id: i64, name: &'static str, description: &'static str) -> Self {
        Self { id, name, description }
    }
}


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AchievementState {
    pub id: i64,
    pub timestamp: Option<DateTimeLocalWithWeekday>,
}
impl AchievementState {
    pub const fn new(id: i64, timestamp: Option<DateTimeLocalWithWeekday>) -> Self {
        Self { id, timestamp }
    }
}
