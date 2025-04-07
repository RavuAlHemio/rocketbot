use std::fmt;

use chrono::{DateTime, Local, TimeZone};
use rocketbot_bim_common::VehicleNumber;
use tokio_postgres::Client;
use tracing::error;

use crate::date_time::canonical_date_format_relative;


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Status {
    pub my: Option<SingleInfo>,
    pub other: Option<SingleInfo>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SingleInfo {
    pub rider: Rider,
    pub timestamp: DateTime<Local>,
    pub line: Option<String>,
}
impl SingleInfo {
    pub fn write_relative_date<Tz: TimeZone, W: fmt::Write>(&self, mut writer: W, relative_to: &DateTime<Tz>) -> fmt::Result
            where Tz::Offset : fmt::Display {
        write!(&mut writer, "{} last rode it ", self.rider)?;
        canonical_date_format_relative(&mut writer, &self.timestamp, relative_to, true, true)?;
        if let Some(line) = self.line.as_ref() {
            write!(&mut writer, " on line {}", line)?;
        }
        write!(&mut writer, ".")
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Rider {
    Me,
    Other { username: String },
}
impl fmt::Display for Rider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Me => write!(f, "You"),
            Self::Other { username } => write!(f, "{}", username),
        }
    }
}

pub async fn get(
    ride_conn: &Client,
    company: &str,
    vehicle_number: &VehicleNumber,
    rider_username: &str,
    highlight_coupled_rides: bool,
) -> Option<Status> {
    let mut my = None;
    let mut other = None;
    for (is_you, operator) in &[(true, "="), (false, "<>")] {
        let ride_row_opt_res = ride_conn.query_opt(
            &format!(
                "
                    SELECT
                        rav.rider_username,
                        rav.\"timestamp\",
                        rav.line
                    FROM
                        bim.rides_and_vehicles rav
                    WHERE
                        rav.company = $1
                        AND rav.vehicle_number = $2
                        AND rav.rider_username {} $3
                        {}
                    ORDER BY
                        rav.\"timestamp\" DESC,
                        rav.id DESC
                    LIMIT 1
                ",
                operator,
                if highlight_coupled_rides { "" } else { "AND rav.coupling_mode = 'R'" },
            ),
            &[&company, &vehicle_number.as_str(), &rider_username],
        ).await;
        match ride_row_opt_res {
            Ok(Some(lrr)) => {
                let last_rider_username: String = lrr.get(0);
                let last_ride: DateTime<Local> = lrr.get(1);
                let last_line: Option<String> = lrr.get(2);

                let rider = if *is_you {
                    Rider::Me
                } else {
                    Rider::Other { username: last_rider_username }
                };
                let info = SingleInfo {
                    rider,
                    timestamp: last_ride,
                    line: last_line,
                };
                if *is_you {
                    my = Some(info);
                } else {
                    other = Some(info);
                }
            },
            Ok(None) => {},
            Err(e) => {
                error!("failed to obtain last rider (is_you={:?}): {}", is_you, e);
                return None;
            },
        };
    }
    Some(Status {
        my,
        other,
    })
}
