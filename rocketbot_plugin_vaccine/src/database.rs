use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, NaiveDate, Utc};
use num_bigint::BigUint;
use reqwest;


#[derive(Debug)]
pub(crate) enum FetchingError {
    Reqwest(reqwest::Error),
    DateTimeSplit(String),
    DateParsing(String, chrono::ParseError),
    ShortRow(usize, usize, usize),
    StateIdParsing(usize, String, std::num::ParseIntError),
    PopulationParsing(usize, String, num_bigint::ParseBigIntError),
    StatisticParsing(usize, usize, String, num_bigint::ParseBigIntError),
}
impl fmt::Display for FetchingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchingError::Reqwest(e)
                => write!(f, "HTTP fetching error: {}", e),
            FetchingError::DateTimeSplit(s)
                => write!(f, "failed to split date and time {:?}", s),
            FetchingError::DateParsing(s, e)
                => write!(f, "failed to parse date {:?}: {}", s, e),
            FetchingError::ShortRow(line, expect_cols, have_cols)
                => write!(f, "line {} too short (expected {} columns, got {})", line, expect_cols, have_cols),
            FetchingError::StateIdParsing(line, s, e)
                => write!(f, "line {}: failed to parse state ID {:?}: {}", line, s, e),
            FetchingError::PopulationParsing(line, s, e)
                => write!(f, "line {}: failed to parse population {:?}: {}", line, s, e),
            FetchingError::StatisticParsing(line, col, s, e)
                => write!(f, "line {}: failed to parse statistic {:?} in column {}: {}", line, s, col, e),
        }
    }
}
impl std::error::Error for FetchingError {
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VaccinationStats {
    pub vaccinations: BigUint,
    pub partially_immune: BigUint,
    pub fully_immune: BigUint,
}
impl std::ops::Add for VaccinationStats {
    type Output = VaccinationStats;

    fn add(self, rhs: Self) -> Self::Output {
        VaccinationStats {
            vaccinations: self.vaccinations + rhs.vaccinations,
            partially_immune: self.partially_immune + rhs.partially_immune,
            fully_immune: self.fully_immune + rhs.fully_immune,
        }
    }
}
impl std::ops::Sub for VaccinationStats {
    type Output = Option<VaccinationStats>;

    fn sub(self, rhs: Self) -> Self::Output {
        if
            self.vaccinations < rhs.vaccinations
            || self.partially_immune < rhs.partially_immune
            || self.fully_immune < rhs.fully_immune
        {
            None
        } else {
            Some(VaccinationStats {
                vaccinations: self.vaccinations - rhs.vaccinations,
                partially_immune: self.partially_immune - rhs.partially_immune,
                fully_immune: self.fully_immune - rhs.fully_immune,
            })
        }
    }
}


#[derive(Debug)]
pub(crate) struct VaccineDatabase {
    pub state_id_to_name: HashMap<u32, String>,
    pub lower_name_to_state_id: HashMap<String, u32>,
    pub state_id_to_pop: HashMap<u32, BigUint>,
    pub state_id_and_date_to_fields: HashMap<(u32, NaiveDate), VaccinationStats>,
    pub corona_timestamp: DateTime<Utc>,
}
impl VaccineDatabase {
    pub async fn new_from_url(url: &str) -> Result<VaccineDatabase, FetchingError> {
        let response = reqwest::get(url)
            .await.map_err(|e| FetchingError::Reqwest(e))?;
        let response_bytes = response.bytes()
            .await.map_err(|e| FetchingError::Reqwest(e))?
            .to_vec();

        // try decoding as UTF-8, fall back to Windows-1252
        let response_string = if let Ok(s) = String::from_utf8(response_bytes.clone()) {
            s
        } else {
            encoding_rs::WINDOWS_1252.decode(&response_bytes).0.into()
        };

        // parse as CSV
        let mut state_id_to_name = HashMap::new();
        let mut lower_name_to_state_id = HashMap::new();
        let mut state_id_to_pop = HashMap::new();
        let mut state_id_and_date_to_fields = HashMap::new();
        let mut header_row = true;
        for (i, line) in response_string.split('\n').enumerate() {
            if header_row {
                header_row = false;
                continue;
            }

            let newlines: &[char] = &['\r', '\n'];
            let pieces: Vec<String> = line.trim_end_matches(newlines)
                .split(';')
                .map(|field| field.to_owned())
                .collect();

            let expected_cols: usize = 9;
            if pieces.len() < expected_cols {
                return Err(FetchingError::ShortRow(i, pieces.len(), expected_cols));
            }

            let date_string = pieces[0]
                .split('T')
                .nth(0)
                .ok_or_else(|| FetchingError::DateTimeSplit(pieces[0].to_owned()))?;
            let date = NaiveDate::parse_from_str(date_string, "%Y-%m-%d")
                .map_err(|e| FetchingError::DateParsing(date_string.to_owned(), e))?;

            let state_id: u32 = pieces[1]
                .parse()
                .map_err(|e| FetchingError::StateIdParsing(i, pieces[1].clone(), e))?;

            if pieces[2].len() > 0 {
                let population: BigUint = pieces[2].parse()
                    .map_err(|e| FetchingError::PopulationParsing(i, pieces[2].clone(), e))?;
                state_id_to_pop.insert(state_id, population);
            }

            let state_name = pieces[3].clone();
            state_id_to_name.insert(state_id, state_name.clone());
            lower_name_to_state_id.insert(state_name.to_lowercase(), state_id);

            let vaccinations: BigUint = pieces[4]
                .parse()
                .map_err(|e| FetchingError::StatisticParsing(i, 4, pieces[4].clone(), e))?;
            let partially_immune: BigUint = pieces[6]
                .parse()
                .map_err(|e| FetchingError::StatisticParsing(i, 6, pieces[6].clone(), e))?;
            let fully_immune: BigUint = pieces[8]
                .parse()
                .map_err(|e| FetchingError::StatisticParsing(i, 8, pieces[8].clone(), e))?;

            let fields = VaccinationStats {
                vaccinations,
                partially_immune,
                fully_immune,
            };

            state_id_and_date_to_fields.insert(
                (state_id, date),
                fields,
            );
        }

        let corona_timestamp = Utc::now();

        Ok(VaccineDatabase {
            state_id_to_name,
            lower_name_to_state_id,
            state_id_to_pop,
            state_id_and_date_to_fields,
            corona_timestamp,
        })
    }
}
