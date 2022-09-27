use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::Read;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use num_bigint::BigUint;
use reqwest;


#[derive(Debug)]
pub(crate) enum FetchingError {
    Reqwest(reqwest::Error),
    ReadingFile(std::io::Error),
    DateTimeSplit(String),
    DateParsing(String, chrono::ParseError),
    MissingField(usize, String),
    StateIdParsing(usize, String, std::num::ParseIntError),
    PopulationParsing(usize, String, num_bigint::ParseBigIntError),
    DoseCountParsing(usize, String, num_bigint::ParseBigIntError),
    CertificateCountParsing(usize, String, num_bigint::ParseBigIntError),
}
impl fmt::Display for FetchingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchingError::Reqwest(e)
                => write!(f, "HTTP fetching error: {}", e),
            FetchingError::ReadingFile(e)
                => write!(f, "error reading file: {}", e),
            FetchingError::DateTimeSplit(s)
                => write!(f, "failed to split date and time {:?}", s),
            FetchingError::DateParsing(s, e)
                => write!(f, "failed to parse date {:?}: {}", s, e),
            FetchingError::MissingField(entry, field_name)
                => write!(f, "entry {}: missing field {:?}", entry, field_name),
            FetchingError::StateIdParsing(entry, s, e)
                => write!(f, "entry {}: failed to parse state ID {:?}: {}", entry, s, e),
            FetchingError::PopulationParsing(entry, s, e)
                => write!(f, "entry {}: failed to parse population {:?}: {}", entry, s, e),
            FetchingError::DoseCountParsing(entry, s, e)
                => write!(f, "entry {}: failed to parse dose count {:?}: {}", entry, s, e),
            FetchingError::CertificateCountParsing(entry, s, e)
                => write!(f, "entry {}: failed to parse certificate count {:?}: {}", entry, s, e),
        }
    }
}
impl std::error::Error for FetchingError {
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VaccinationStats {
    pub dose_to_count: HashMap<String, BigUint>,
}
impl VaccinationStats {
    pub fn new() -> Self {
        let dose_to_count = HashMap::new();
        Self {
            dose_to_count,
        }
    }
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VaccineCertificateDatabase {
    pub state_id_to_name: HashMap<u32, String>,
    pub lower_name_to_state_id: HashMap<String, u32>,
    pub state_id_to_pop: HashMap<u32, BigUint>,
    pub state_id_and_date_to_vaccinated: HashMap<(u32, NaiveDate), BigUint>,
}
impl VaccineCertificateDatabase {
    pub fn new() -> Self {
        let state_id_to_name = HashMap::new();
        let lower_name_to_state_id = HashMap::new();
        let state_id_to_pop = HashMap::new();
        let state_id_and_date_to_vaccinated = HashMap::new();
        Self {
            state_id_to_name,
            lower_name_to_state_id,
            state_id_to_pop,
            state_id_and_date_to_vaccinated,
        }
    }
}


#[derive(Debug)]
pub(crate) struct VaccineDatabase {
    pub cert_database: VaccineCertificateDatabase,
    pub prev_cert_database: VaccineCertificateDatabase,
    pub state_id_and_date_to_fields: HashMap<(u32, NaiveDate), VaccinationStats>,
    pub corona_timestamp: DateTime<Utc>,
}
impl VaccineDatabase {
    async fn string_from_url(url: &str) -> Result<String, FetchingError> {
        let response_bytes = if let Some(file_path) = url.strip_prefix("file://") {
            let mut file = File::open(file_path)
                .map_err(|e| FetchingError::ReadingFile(e))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|e| FetchingError::ReadingFile(e))?;
            bytes
        } else {
            let response = reqwest::get(url)
                .await.map_err(|e| FetchingError::Reqwest(e))?;
            response.bytes()
                .await.map_err(|e| FetchingError::Reqwest(e))?
                .to_vec()
        };

        // try decoding as UTF-8, fall back to Windows-1252
        let decoded_string = if let Ok(s) = String::from_utf8(response_bytes.clone()) {
            s
        } else {
            encoding_rs::WINDOWS_1252.decode(&response_bytes).0.into()
        };
        Ok(decoded_string)
    }

    fn parse_csv(csv_str: &str) -> Vec<HashMap<String, String>> {
        let trimmed_csv_str = csv_str.trim_start_matches("\u{feff}");
        let mut headers: Option<Vec<String>> = None;
        let mut rows = Vec::new();
        for line in trimmed_csv_str.split("\n") {
            let trimmed_line = line.trim_end_matches('\r');
            let line_vec: Vec<String> = trimmed_line.split(';').map(|s| s.to_owned()).collect();
            if let Some(hs) = &headers {
                // data row
                let row: HashMap<String, String> = hs.iter().zip(line_vec.iter())
                    .map(|(a, b)| (a.clone(), b.clone()))
                    .collect();
                rows.push(row);
            } else {
                // header row
                headers = Some(line_vec);
            }
        }
        rows
    }

    fn parse_date(timestamp_str: &str) -> Result<NaiveDate, FetchingError> {
        let timestamp_t_index = timestamp_str.find('T')
            .ok_or_else(|| FetchingError::DateTimeSplit(timestamp_str.to_owned()))?;
        let date_str = &timestamp_str[0..timestamp_t_index];
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| FetchingError::DateParsing(timestamp_str.to_owned(), e))
    }

    async fn vaxxed_from_url(url: &str) -> Result<VaccineCertificateDatabase, FetchingError> {
        let vaxxed_string = Self::string_from_url(url).await?;
        let vaxxed_csv = Self::parse_csv(&vaxxed_string);

        let mut vcdb = VaccineCertificateDatabase::new();

        // get the state names and populations from the certs file, as that only contains one day
        for (entry_num, entry) in vaxxed_csv.iter().enumerate() {
            if entry.get("age_group").map(|v| v != "All").unwrap_or(true) {
                continue;
            }
            if entry.get("gender").map(|v| v != "All").unwrap_or(true) {
                continue;
            }

            let timestamp_str = entry.get("date")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "date".to_owned()))?;
            let date = Self::parse_date(timestamp_str)?;

            let state_id_str = entry.get("state_id")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "state_id".to_owned()))?;
            let state_id: u32 = state_id_str
                .parse()
                .map_err(|e| FetchingError::StateIdParsing(entry_num, state_id_str.clone(), e))?;

            let state_name = entry.get("state_name")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "state_name".to_owned()))?;

            let vaccinated_str = entry.get("vaccinated_according_to_recommendation")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "vaccinated_according_to_recommendation".to_owned()))?;
            let vaccinated: BigUint = vaccinated_str
                .parse()
                .map_err(|e| FetchingError::CertificateCountParsing(entry_num, "vaccinated_according_to_recommendation".to_owned(), e))?;

            let pop_str = entry.get("population")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "population".to_owned()))?;
            let pop: BigUint = pop_str
                .parse()
                .map_err(|e| FetchingError::PopulationParsing(entry_num, "population".to_owned(), e))?;

            vcdb.state_id_to_name.insert(state_id, state_name.clone());
            vcdb.lower_name_to_state_id.insert(state_name.to_lowercase(), state_id);
            vcdb.state_id_to_pop.insert(state_id, pop);
            vcdb.state_id_and_date_to_vaccinated.insert((state_id, date), vaccinated);
        }

        Ok(vcdb)
    }

    fn new() -> Self {
        Self {
            cert_database: VaccineCertificateDatabase::new(),
            prev_cert_database: VaccineCertificateDatabase::new(),
            state_id_and_date_to_fields: HashMap::new(),
            corona_timestamp: Utc.timestamp(0, 0),
        }
    }

    pub async fn new_from_urls(doses_timeline_url: &str, vaxxed_url: &str, prev_vaxxed_url_format: &str) -> Result<VaccineDatabase, FetchingError> {
        // get state, population and vaccine cert stats
        let vaxxed_stats = Self::vaxxed_from_url(vaxxed_url).await?;

        // get vax cert stats date
        let vcs_date_opt = vaxxed_stats.state_id_and_date_to_vaccinated
            .keys()
            .map(|(_state_id, date)| *date)
            .max();
        let vcs_date = match vcs_date_opt {
            Some(d) => d,
            None => {
                // no date found; return an empty database
                return Ok(VaccineDatabase::new());
            },
        };

        // get vax cert stats for the previous day (for deltas)
        let prev_date = vcs_date.pred();
        let prev_vaxxed_url = prev_vaxxed_url_format
            .replace("{date}", &prev_date.format("%Y%m%d").to_string());
        let prev_vaxxed_stats = Self::vaxxed_from_url(&prev_vaxxed_url).await?;

        // obtain doses
        let doses_timeline_string = Self::string_from_url(doses_timeline_url).await?;
        let doses_timeline_csv = Self::parse_csv(&doses_timeline_string);

        let mut state_id_and_date_to_fields = HashMap::new();

        // get the vaccine stats
        let mut cur_state_date = None;
        let mut cur_stats = VaccinationStats::new();
        for (entry_num, entry) in doses_timeline_csv.iter().enumerate() {
            let timestamp_str = entry.get("date")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "date".to_owned()))?;
            let date = Self::parse_date(timestamp_str)?;

            let state_id_str = entry.get("state_id")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "state_id".to_owned()))?;
            let state_id: u32 = state_id_str
                .parse()
                .map_err(|e| FetchingError::StateIdParsing(entry_num, state_id_str.clone(), e))?;

            let this_state_date = (state_id, date);
            if let Some(state_date) = cur_state_date {
                if state_date != this_state_date {
                    state_id_and_date_to_fields.insert(state_date, cur_stats);
                    cur_stats = VaccinationStats::new();
                }
            }
            cur_state_date = Some(this_state_date);

            let dose_number = entry.get("dose_number")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "dose_number".to_owned()))?
                .trim()
                .to_owned();

            let dose_count_str = entry.get("doses_administered_cumulative")
                .ok_or_else(|| FetchingError::MissingField(entry_num, "doses_administered_cumulative".to_owned()))?;
            let dose_count: BigUint = dose_count_str
                .parse()
                .map_err(|e| FetchingError::DoseCountParsing(entry_num, dose_count_str.clone(), e))?;

            let total_dose_count = cur_stats.dose_to_count
                .entry(dose_number)
                .or_insert_with(|| BigUint::from(0u32));
            *total_dose_count += dose_count;
        }

        if cur_stats.dose_to_count.len() > 0 {
            if let Some(cds) = cur_state_date {
                state_id_and_date_to_fields.insert(cds, cur_stats);
            }
        }

        let corona_timestamp = Utc::now();

        Ok(VaccineDatabase {
            cert_database: vaxxed_stats,
            prev_cert_database: prev_vaxxed_stats,
            state_id_and_date_to_fields,
            corona_timestamp,
        })
    }
}
