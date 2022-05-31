use std::fmt;

use chrono::{Datelike, DateTime, Local, Weekday};
use serde::{Serialize, Serializer};


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct DateTimeLocalWithWeekday(pub DateTime<Local>);
impl fmt::Display for DateTimeLocalWithWeekday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let weekday = match self.0.weekday() {
            Weekday::Mon => "Mo",
            Weekday::Tue => "Tu",
            Weekday::Wed => "We",
            Weekday::Thu => "Th",
            Weekday::Fri => "Fr",
            Weekday::Sat => "Sa",
            Weekday::Sun => "Su",
        };
        write!(
            f,
            "{} {}",
            weekday, self.0.format("%Y-%m-%d %H:%M:%S"),
        )
    }
}
impl From<DateTime<Local>> for DateTimeLocalWithWeekday {
    fn from(dt: DateTime<Local>) -> Self {
        Self(dt)
    }
}
impl Serialize for DateTimeLocalWithWeekday {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let timestamp_string = self.to_string();
        timestamp_string.serialize(serializer)
    }
}
