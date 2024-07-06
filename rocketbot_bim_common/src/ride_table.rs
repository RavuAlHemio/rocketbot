use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::{CouplingMode, LastRider, format_timestamp};


#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RideTableData {
    /// The number of the ride.
    pub ride_id: i64,

    /// The line on which the vehicles have been ridden.
    pub line: Option<String>,

    /// The name of the rider. Used as a column header.
    pub rider_username: String,

    /// The vehicles ridden.
    pub vehicles: Vec<RideTableVehicle>,

    /// The time relative to which to show timestamps. This shortens timestamps to only the time if
    /// the ride happened within the past 24 hours. `None` causes the full timestamp to always be
    /// shown.
    pub relative_time: Option<DateTime<Local>>,
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RideTableVehicle {
    /// The number of this vehicle.
    pub vehicle_number: String,

    /// The type of this vehicle, if known.
    pub vehicle_type: Option<String>,

    /// The number of times this vehicle (exactly) was ridden in a row by the rider registering the
    /// ride.
    #[serde(default)]
    pub my_same_count_streak: i64,

    /// The number of times this vehicle (exactly) was ridden by the rider registering the ride.
    #[serde(default)]
    pub my_same_count: i64,

    /// The last time this vehicle (exactly) was ridden by the rider registering the ride.
    pub my_same_last: Option<Ride>,

    /// The number of times a vehicle coupled with this one was ridden in a row by the rider
    /// registering the ride.
    #[serde(default)]
    pub my_coupled_count_streak: i64,

    /// The number of times a vehicle coupled with this one was ridden by the rider registering the
    /// ride.
    #[serde(default)]
    pub my_coupled_count: i64,

    /// The timestamp and line of the last time a vehicle coupled with this one was ridden by the
    /// rider registering the ride.
    pub my_coupled_last: Option<Ride>,

    /// The number of times this vehicle (exactly) was ridden by a different rider in a row.
    #[serde(default)]
    pub other_same_count_streak: i64,

    /// The number of times this vehicle (exactly) was ridden by a different rider.
    #[serde(default)]
    pub other_same_count: i64,

    /// The rider's username and timestamp of the last time this vehicle (exactly) was ridden by a
    /// different rider.
    pub other_same_last: Option<UserRide>,

    /// The number of times a vehicle coupled with this one was ridden by a different rider in a
    /// row.
    #[serde(default)]
    pub other_coupled_count_streak: i64,

    /// The number of times a vehicle coupled with this one was ridden by a different rider.
    #[serde(default)]
    pub other_coupled_count: i64,

    /// The rider's username and timestamp of the last time a vehicle coupled with this one was
    /// ridden by a different rider.
    pub other_coupled_last: Option<UserRide>,

    /// Whether to highlight coupled rides if they are the latest.
    ///
    /// If false, a coupled ride is not highlighted even if it is the latest.
    pub highlight_coupled_rides: bool,

    /// The coupling mode of this vehicle.
    pub coupling_mode: CouplingMode,
}


#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Ride {
    /// The timestamp at which the vehicle was ridden.
    pub timestamp: DateTime<Local>,

    /// The line in which the vehicle was ridden.
    pub line: Option<String>,
}
impl Ride {
    pub fn stringify<Tz: TimeZone>(&self, anchor_timestamp: Option<DateTime<Tz>>) -> String {
        if let Some(line) = &self.line {
            format!("{}/{}", format_timestamp(self.timestamp, anchor_timestamp), line)
        } else {
            format_timestamp(self.timestamp, anchor_timestamp)
        }
    }

    /// Returns the timestamp of this ride. Provided for structural equivalence with [`UserRide`].
    #[inline]
    pub fn timestamp(&self) -> &DateTime<Local> { &self.timestamp }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct UserRide {
    /// The username of the rider who rode this vehicle.
    pub rider_username: String,

    /// All the other ride data.
    pub ride: Ride,
}
impl UserRide {
    /// Returns the timestamp of this ride. Provided for structural equivalence with [`Ride`].
    #[inline]
    pub fn timestamp(&self) -> &DateTime<Local> { &self.ride.timestamp }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct HighlightedRide {
    pub timestamp: DateTime<Local>,
    pub is_other: bool,
    pub is_coupled: bool,
}


macro_rules! define_is_most_recent {
    ($name:ident, $expect_other:expr, $expect_coupled:expr) => {
        #[inline]
        pub fn $name(&self) -> bool {
            let highlighted_rides = self.highlighted_rides_most_recent_first();
            highlighted_rides.get(0)
                .map(|r| r.is_other == $expect_other && r.is_coupled == $expect_coupled)
                .unwrap_or(false)
        }
    };
}


impl RideTableVehicle {
    /// Returns all the rides worth highlighting.
    ///
    /// A ride is considered worth highlighting if it is a same-vehicle ride, or if it is a
    /// coupled-vehicle ride and `highlight_coupled_rides` is true.
    ///
    /// The rides are returned sorted in descending order by timestamp, i.e. latest first.
    fn highlighted_rides_most_recent_first(&self) -> SmallVec<[HighlightedRide; 4]> {
        let mut ret = SmallVec::new();
        if let Some(my_same_last) = self.my_same_last.as_ref() {
            ret.push(HighlightedRide {
                timestamp: my_same_last.timestamp,
                is_coupled: false,
                is_other: false,
            });
        }
        if let Some(other_same_last) = self.other_same_last.as_ref() {
            ret.push(HighlightedRide {
                timestamp: other_same_last.ride.timestamp,
                is_coupled: false,
                is_other: true,
            });
        }
        if self.highlight_coupled_rides {
            if let Some(my_coupled_last) = self.my_coupled_last.as_ref() {
                ret.push(HighlightedRide {
                    timestamp: my_coupled_last.timestamp,
                    is_coupled: true,
                    is_other: false,
                });
            }
            if let Some(other_coupled_last) = self.other_coupled_last.as_ref() {
                ret.push(HighlightedRide {
                    timestamp: other_coupled_last.ride.timestamp,
                    is_coupled: true,
                    is_other: true,
                });
            }
        }
        ret.sort_unstable();
        ret.reverse();
        ret
    }

    define_is_most_recent!(is_my_same_most_recent, false, false);
    define_is_most_recent!(is_my_coupled_most_recent, false, true);
    define_is_most_recent!(is_other_same_most_recent, true, false);
    define_is_most_recent!(is_other_coupled_most_recent, true, true);

    /// Whether this vehicle has, in terms of highlighting, never been ridden before.
    ///
    /// See the documentation for `highlighted_rides_most_recent_first` for an explanation of
    /// highlighting rules.
    #[inline]
    pub fn is_first_highlighted_ride_overall(&self) -> bool {
        self.highlighted_rides_most_recent_first().len() == 0
    }

    /// Whether this vehicle has previously belonged to the same rider in terms of highlighting.
    ///
    /// See the documentation for `highlighted_rides_most_recent_first` for an explanation of
    /// highlighting rules.
    #[inline]
    pub fn belongs_to_rider_highlighted(&self) -> bool {
        self.highlighted_rides_most_recent_first().get(0)
            .map(|hr| !hr.is_other)
            .unwrap_or(false)
    }

    /// Whether this vehicle has changed hands in terms of highlighting.
    ///
    /// See the documentation for `highlighted_rides_most_recent_first` for an explanation of
    /// highlighting rules.
    #[inline]
    pub fn has_changed_hands_highlighted(&self) -> bool {
        self.highlighted_rides_most_recent_first().get(0)
            .map(|hr| hr.is_other)
            .unwrap_or(false)
    }

    /// Returns the last highlighted rider of this vehicle.
    pub fn last_highlighted_rider(&self) -> LastRider<'_> {
        let last_rides_by_category = self.highlighted_rides_most_recent_first();

        if let Some(last_highlighted_ride) = last_rides_by_category.get(0) {
            if last_highlighted_ride.is_other {
                if last_highlighted_ride.is_coupled {
                    LastRider::SomebodyElse(self.other_coupled_last.as_ref().unwrap().rider_username.as_str())
                } else {
                    LastRider::SomebodyElse(self.other_same_last.as_ref().unwrap().rider_username.as_str())
                }
            } else {
                LastRider::Me
            }
        } else {
            LastRider::Nobody
        }
    }

    /// The latest highlighted ride of this vehicle by the rider registering the ride, if any.
    ///
    /// If `highlight_coupled_rides` is true, picks the later ride of `my_same_last` and
    /// `my_coupled_last`. If `highlight_coupled_rides` is false, returns `my_same_last`.
    pub fn my_highlighted_last(&self) -> Option<&Ride> {
        if self.highlight_coupled_rides {
            match (self.my_same_last.as_ref(), self.my_coupled_last.as_ref()) {
                (Some(s), Some(c)) => Some(if c.timestamp() > s.timestamp() { c } else { s }),
                (Some(s), None) => Some(s),
                (None, Some(c)) => Some(c),
                (None, None) => None,
            }
        } else {
            self.my_same_last.as_ref()
        }
    }
}
