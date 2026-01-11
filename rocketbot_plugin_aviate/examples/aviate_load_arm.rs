//! Populates the database from Airline Route Mapper data files.


use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio_postgres::NoTls;


static AIRLINE_IATA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
    "^",
    "[A-Z0-9]{2}",
    "$",
)).expect("failed to compile airline IATA code regexp"));
static AIRPORT_IATA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
    "^",
    "[A-Z0-9]{3}",
    "$",
)).expect("failed to compile airport IATA code regexp"));
static LATLON_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
    "^",
    "-?",
    "[0-9]+",
    "[.]",
    "[0-9]+",
    "$",
)).expect("failed to compile airport IATA code regexp"));


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub db_conn_string: String,
    pub airlines_path: PathBuf,
    pub airports_path: PathBuf,
    pub alliances_path: PathBuf,
    pub equipment_path: PathBuf,
    pub routes_path: PathBuf,
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Airline {
    pub iata_code: String,
    pub name: String,
}
impl Airline {
    pub fn from_csv_cols<S: AsRef<str>>(pieces: &[S]) -> Option<Self> {
        if pieces.len() != 2 {
            let pieces_str: Vec<&str> = pieces
                .into_iter()
                .map(|p| p.as_ref())
                .collect();
            eprintln!("invalid airline line: {:?}", pieces_str);
            return None;
        }
        let iata_code = pieces[0].as_ref().trim().to_owned();
        if !AIRLINE_IATA_RE.is_match(&iata_code) {
            eprintln!("invalid airline IATA code: {:?}", iata_code);
            return None;
        }
        let name = pieces[1].as_ref().trim().to_owned();
        if name.len() == 0 {
            eprintln!("empty airline name for code {:?}", iata_code);
            return None;
        }
        Some(Self {
            iata_code,
            name,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Airport {
    pub iata_code: String,
    pub latitude: String,
    pub longitude: String,
    pub name: String,
}
impl Airport {
    pub fn from_csv_cols<S: AsRef<str>>(pieces: &[S]) -> Option<Self> {
        if pieces.len() != 4 {
            let pieces_str: Vec<&str> = pieces
                .into_iter()
                .map(|p| p.as_ref())
                .collect();
            eprintln!("invalid airport line: {:?}", pieces_str);
            return None;
        }
        let iata_code = pieces[0].as_ref().trim().to_owned();
        let latitude = pieces[1].as_ref().trim().to_owned();
        let longitude = pieces[2].as_ref().trim().to_owned();
        let name = pieces[3].as_ref().trim().to_owned();

        if !AIRPORT_IATA_RE.is_match(&iata_code) {
            eprintln!("invalid airport IATA code: {:?}", iata_code);
            return None;
        }
        if !LATLON_RE.is_match(&latitude) {
            eprintln!("invalid latitude value {:?} for airport with code {:?}", latitude, iata_code);
            return None;
        }
        if !LATLON_RE.is_match(&longitude) {
            eprintln!("invalid longitude value {:?} for airport with code {:?}", longitude, iata_code);
            return None;
        }
        if name.trim().len() == 0 {
            eprintln!("empty name {:?} for airport with code {:?}", name, iata_code);
            return None;
        }

        Some(Self {
            iata_code,
            latitude,
            longitude,
            name,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Alliance {
    pub name: String,
    pub airline_iata_codes: BTreeSet<String>,
}
impl Alliance {
    pub fn from_csv_cols<S: AsRef<str>>(pieces: &[S]) -> Option<Self> {
        if pieces.len() != 2 {
            let pieces_str: Vec<&str> = pieces
                .into_iter()
                .map(|p| p.as_ref())
                .collect();
            eprintln!("invalid alliance line: {:?}", pieces_str);
            return None;
        }
        let name = pieces[0].as_ref().trim().to_owned();
        let airline_iata_codes = pieces[1].as_ref()
            .split(' ')
            .map(|piece| piece.trim())
            .filter(|piece| piece.len() > 0)
            .map(|piece| piece.to_owned())
            .collect();

        Some(Self {
            name,
            airline_iata_codes,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Equipment {
    pub code: String,
    pub name: String,
}
impl Equipment {
    pub fn from_csv_cols<S: AsRef<str>>(pieces: &[S]) -> Option<Self> {
        if pieces.len() != 2 {
            let pieces_str: Vec<&str> = pieces
                .into_iter()
                .map(|p| p.as_ref())
                .collect();
            eprintln!("invalid equipment line: {:?}", pieces_str);
            return None;
        }
        let code = pieces[0].as_ref().trim().to_owned();
        let name = pieces[1].as_ref().trim().to_owned();

        Some(Self {
            code,
            name,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Route {
    pub airline_iata_code: String,
    pub from_airport_iata_code: String,
    pub to_airport_iata_code: String,
    pub codeshare: bool,
    pub stops: u64,
    pub equipment_codes: BTreeSet<String>,
}
impl Route {
    pub fn from_csv_cols<S: AsRef<str>>(pieces: &[S]) -> Option<Self> {
        if pieces.len() != 6 {
            let pieces_str: Vec<&str> = pieces
                .into_iter()
                .map(|p| p.as_ref())
                .collect();
            eprintln!("invalid route line: {:?}", pieces_str);
            return None;
        }
        let airline_iata_code = pieces[0].as_ref().trim().to_owned();
        let from_airport_iata_code = pieces[1].as_ref().trim().to_owned();
        let to_airport_iata_code = pieces[2].as_ref().trim().to_owned();
        let codeshare_str = pieces[3].as_ref().trim();
        let stops_str = pieces[4].as_ref().trim();
        let equipment_codes = pieces[5].as_ref()
            .split(' ')
            .map(|piece| piece.trim())
            .filter(|piece| piece.len() > 0)
            .map(|piece| piece.to_owned())
            .collect();

        let codeshare = match codeshare_str {
            "*" => true,
            "" => false,
            other => {
                eprintln!("invalid codeshare value {:?}", other);
                return None;
            },
        };
        let stops = match stops_str.parse() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("invalid stop count {:?}: {}", stops_str, e);
                return None;
            },
        };

        Some(Self {
            airline_iata_code,
            from_airport_iata_code,
            to_airport_iata_code,
            codeshare,
            stops,
            equipment_codes,
        })
    }
}

fn read_csv(path: &Path) -> Vec<Vec<String>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => panic!("failed to open CSV file {}: {}", path.display(), e),
    };
    let mut reader = BufReader::new(file);
    let mut lines = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {},
            Err(e) => panic!("failed to read line from CSV file {}: {}", path.display(), e),
        }

        if line.starts_with('#') {
            continue;
        }

        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop().unwrap();
        }

        let pieces: Vec<String> = line.split("\t")
            .map(|p| p.to_owned())
            .collect();
        lines.push(pieces);
    }

    lines
}

async fn run() {
    let args: Vec<OsString> = std::env::args_os().collect();
    let config_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("aviate_load_arm.toml")
    };

    // load the config
    let config_string = std::fs::read_to_string(&config_path)
        .expect("failed to read configuration file");
    let config: Config = toml::from_str(&config_string)
        .expect("failed to parse configuration file");

    // connect to the database
    let (mut conn, inner_conn) = tokio_postgres::connect(&config.db_conn_string, NoTls)
        .await.expect("failed to connect to the database");
    tokio::task::spawn(async move {
        if let Err(e) = inner_conn.await {
            eprintln!("Postgres connection failed: {}", e);
        }
    });
    let txn = conn.transaction()
        .await.expect("failed to begin a database transaction");

    // prepare statements
    let insert_airline_stmt = txn.prepare(
        "
            INSERT INTO aviate.airlines
                ( iata_code
                , name
                )
            VALUES
                ( $1
                , $2
                )
        "
    ).await.expect("failed to compile insert-airline statement");
    let insert_alliance_stmt = txn.prepare(
        "
            INSERT INTO aviate.alliances
                ( name
                )
            VALUES
                ( $1
                )
            RETURNING   id
        "
    ).await.expect("failed to compile insert-alliance statement");
    let insert_alliance_airline_stmt = txn.prepare(
        "
            INSERT INTO aviate.alliances_airlines
                ( alliance_id
                , airline_iata_code
                )
            VALUES
                ( $1
                , $2
                )
        "
    ).await.expect("failed to compile insert-alliance-airline statement");
    let insert_equipment_stmt = txn.prepare(
        "
            INSERT INTO aviate.equipment
                ( code
                , description
                )
            VALUES
                ( $1
                , $2
                )
        "
    ).await.expect("failed to compile insert-equipment statement");
    let insert_airport_stmt = txn.prepare(
        "
            INSERT INTO aviate.airports
                ( iata_code
                , latitude
                , longitude
                , name
                )
            VALUES
                ( $1
                , CAST(CAST($2 AS character varying) AS numeric(7, 4))
                , CAST(CAST($3 AS character varying) AS numeric(7, 4))
                , $4
                )
        "
    ).await.expect("failed to compile insert-airport statement");
    let insert_route_stmt = txn.prepare(
        "
            INSERT INTO aviate.routes
                ( airline_iata_code
                , from_airport_iata_code
                , to_airport_iata_code
                , codeshare
                )
            VALUES
                ( $1
                , $2
                , $3
                , $4
                )
            RETURNING   id
        "
    ).await.expect("failed to compile insert-route statement");
    let insert_route_equipment_stmt = txn.prepare(
        "
            INSERT INTO aviate.route_equipment
                ( route_id
                , equipment_code
                )
            VALUES
                ( $1
                , $2
                )
        "
    ).await.expect("failed to compile insert-route-equipment statement");

    // load the CSV files
    eprintln!("inserting airlines");
    let airline_lines = read_csv(&config.airlines_path);
    let mut airline_iata_codes = BTreeSet::new();
    for airline_line in &airline_lines {
        let Some(airline) = Airline::from_csv_cols(&airline_line)
            else { continue };
        let is_unique = airline_iata_codes.insert(airline.iata_code.clone());
        if !is_unique {
            eprintln!("duplicate airline IATA code {:?}", airline.iata_code);
            continue;
        }
        txn.execute(&insert_airline_stmt, &[&airline.iata_code, &airline.name])
            .await.expect("failed to insert airline");
    }
    eprintln!("inserting alliances");
    let alliance_lines = read_csv(&config.alliances_path);
    for alliance_line in &alliance_lines {
        let Some(alliance) = Alliance::from_csv_cols(&alliance_line)
            else { continue };
        let alliance_id_row = txn.query_one(&insert_alliance_stmt, &[&alliance.name])
            .await.expect("failed to insert alliance");
        let alliance_id: i64 = alliance_id_row.get(0);
        for airline in &alliance.airline_iata_codes {
            if !airline_iata_codes.contains(airline) {
                eprintln!("unknown airline {:?} in alliance {:?}", airline, alliance.name);
                continue;
            }
            txn.execute(&insert_alliance_airline_stmt, &[&alliance_id, airline])
                .await.expect("failed to insert airline into alliance");
        }
    }
    eprintln!("inserting equipment");
    let equipment_lines = read_csv(&config.equipment_path);
    let mut equipment_codes = BTreeSet::new();
    for equipment_line in &equipment_lines {
        let Some(equipment) = Equipment::from_csv_cols(equipment_line)
            else { continue };
        let is_unique = equipment_codes.insert(equipment.code.clone());
        if !is_unique {
            eprintln!("duplicate equipment code {:?}", equipment.code);
            continue;
        }
        txn.execute(&insert_equipment_stmt, &[&equipment.code, &equipment.name])
            .await.expect("failed to insert equipment");
    }
    eprintln!("inserting airports");
    let airport_lines = read_csv(&config.airports_path);
    let mut airport_iata_codes = BTreeSet::new();
    for airport_line in &airport_lines {
        let Some(airport) = Airport::from_csv_cols(airport_line)
            else { continue };
        let is_unique = airport_iata_codes.insert(airport.iata_code.clone());
        if !is_unique {
            eprintln!("duplicate airport IATA code {:?}", airport.iata_code);
            continue;
        }
        txn.execute(&insert_airport_stmt, &[&airport.iata_code, &airport.latitude, &airport.longitude, &airport.name])
            .await.expect("failed to insert airport");
    }
    eprintln!("inserting routes");
    let route_lines = read_csv(&config.routes_path);
    let mut route_to_id: BTreeMap<(String, String, String), i64> = BTreeMap::new();
    let mut route_to_equipment: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();
    for route_line in &route_lines {
        let Some(route) = Route::from_csv_cols(route_line)
            else { continue };
        if !airline_iata_codes.contains(&route.airline_iata_code) {
            continue;
        }
        if !airport_iata_codes.contains(&route.from_airport_iata_code) {
            continue;
        }
        if !airport_iata_codes.contains(&route.to_airport_iata_code) {
            continue;
        }
        let known_route_id_opt = route_to_id.get(&(
            route.airline_iata_code.clone(),
            route.from_airport_iata_code.clone(),
            route.to_airport_iata_code.clone(),
        )).map(|r| *r);
        let known_route_id = if let Some(kid) = known_route_id_opt {
            eprintln!(
                "duplicate route {}-{} served by {}; merging",
                route.from_airport_iata_code,
                route.to_airport_iata_code,
                route.airline_iata_code,
            );
            kid
        } else {
            let route_id_row = txn.query_one(
                &insert_route_stmt,
                &[
                    &route.airline_iata_code,
                    &route.from_airport_iata_code,
                    &route.to_airport_iata_code,
                    &route.codeshare,
                ],
            )
                .await.expect("failed to insert route");
            let route_id: i64 = route_id_row.get(0);
            route_to_id.insert(
                (
                    route.airline_iata_code.clone(),
                    route.from_airport_iata_code.clone(),
                    route.to_airport_iata_code.clone(),
                ),
                route_id,
            );
            route_id
        };
        let known_route_equipment = route_to_equipment
            .entry((
                route.airline_iata_code.clone(),
                route.from_airport_iata_code.clone(),
                route.to_airport_iata_code.clone(),
            ))
            .or_insert_with(|| BTreeSet::new());
        for equipment in &route.equipment_codes {
            if known_route_equipment.contains(equipment) {
                eprintln!(
                    "duplicate equipment code {} on route {}-{} served by {}; skipping equipment",
                    equipment,
                    route.from_airport_iata_code,
                    route.to_airport_iata_code,
                    route.airline_iata_code,
                );
                continue;
            }
            if !equipment_codes.contains(equipment) {
                eprintln!(
                    "unknown equipment code {} on route {}-{} served by {}; skipping equipment",
                    equipment,
                    route.from_airport_iata_code,
                    route.to_airport_iata_code,
                    route.airline_iata_code,
                );
                continue;
            }
            known_route_equipment.insert(equipment.clone());
            txn.execute(&insert_route_equipment_stmt, &[&known_route_id, equipment])
                .await.expect("failed to insert equipment into route");
        }
    }

    eprintln!("committing");
    txn.commit()
        .await.expect("committing transaction failed");
}

#[tokio::main]
async fn main() {
    run().await
}
