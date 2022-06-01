use rocketbot_date_time::DateTimeLocalWithWeekday;
use serde::Serialize;


pub static ACHIEVEMENT_DEFINITIONS: [AchievementDef; 31] = [
    // special vehicle numbers
    AchievementDef::new(1, "Beastly", "Ride a vehicle (of any company) with number 666."),
    AchievementDef::new(22, "Kinda Beastly", "Ride a vehicle (of any company) whose number contains \"666\" (but isn't 666)."),
    AchievementDef::new(2, "Nice", "Ride a vehicle (of any company) with number 69."),
    AchievementDef::new(23, "Rather Nice", "Ride a vehicle (of any company) whose number contains \"69\" (but isn't 69)."),
    AchievementDef::new(4, "Two of a Kind", "Ride a vehicle (of any company) whose number consists of one digit repeated at least twice."),
    AchievementDef::new(5, "Three of a Kind", "Ride a vehicle (of any company) whose number consists of one digit repeated at least three times."),
    AchievementDef::new(6, "Four of a Kind", "Ride a vehicle (of any company) whose number consists of one digit repeated at least four times."),
    AchievementDef::new(7, "Palindrome", "Ride a vehicle (of any company) whose number is a palindrome while not being all the same digit."),
    AchievementDef::new(9, "Boeing", "Ride a vehicle (of any company) whose number has the pattern \"7x7\"."),
    AchievementDef::new(26, "Priming the Pump", "Ride a vehicle (of any company) whose vehicle number is a four-digit prime."),
    AchievementDef::new(27, "Prim and Proper", "Ride a vehicle (of any company) whose vehicle number is a three-digit prime."),
    AchievementDef::new(28, "Primate Representative", "Ride a vehicle (of any company) whose vehicle number is a two-digit prime."),
    AchievementDef::new(29, "Primus Inter Pares", "Ride a vehicle (of any company) whose vehicle number is a single-digit prime."),
    AchievementDef::new(30, "It Gets Better", "Ride a vehicle (of any company) whose at least three-digit number's decimal digits are in ascending order."),
    AchievementDef::new(31, "Downward Spiral", "Ride a vehicle (of any company) whose at least three-digit number's decimal digits are in descending order."),

    // vehicle numbers in relation to line numbers
    AchievementDef::new(3, "Home Line", "Ride a vehicle (of any company) where the vehicle number and the line are the same."),
    AchievementDef::new(8, "Mirror Home Line", "Ride a vehicle (of any company) where the vehicle number is the reverse of the line."),
    AchievementDef::new(24, "Indivisibiliter", "Ride a vehicle (of any company) whose vehicle number is divisible by (but not equal to) its line number."),
    AchievementDef::new(25, "Inseparabiliter", "Ride a vehicle (of any company) on a line whose number is divisible by (but not equal to) the vehicle's number."),

    // vehicle numbers in relation to companies
    AchievementDef::new(10, "Elsewhere", "Ride two vehicles with the same vehicle number but different companies."),

    // same vehicle after some time
    AchievementDef::new(11, "Monthiversary", "Ride the same vehicle on the same day of two consecutive months."),
    AchievementDef::new(12, "Anniversary", "Ride the same vehicle on the same day of two consecutive years."),
    AchievementDef::new(13, "Same Time Next Week", "Ride the same vehicle on the same weekday of two consecutive weeks."),

    // consecutive numbers
    AchievementDef::new(14, "Five Sweep", "Collect rides with five vehicles of the same company with consecutive numbers."),
    AchievementDef::new(15, "Ten Sweep", "Collect rides with ten vehicles of the same company with consecutive numbers."),
    AchievementDef::new(16, "Twenty Sweep", "Collect rides with twenty vehicles of the same company with consecutive numbers."),
    AchievementDef::new(17, "Thirty Sweep", "Collect rides with thirty vehicles of the same company with consecutive numbers."),
    AchievementDef::new(18, "Forty Sweep", "Collect rides with forty vehicles of the same company with consecutive numbers."),
    AchievementDef::new(19, "Half-Century Sweep", "Collect rides with fifty vehicles of the same company with consecutive numbers."),
    AchievementDef::new(20, "Nice Sweep", "Collect rides with sixty-nine vehicles of the same company with consecutive numbers."),
    AchievementDef::new(21, "Century Sweep", "Collect rides with one hundred vehicles of the same company with consecutive numbers."),
];


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
