use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;

use chrono::{DateTime, Local};
use tokio_postgres::NoTls;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RideKey {
    pub rider_username: String,
    pub timestamp: DateTime<Local>,
}
impl RideKey {
    pub fn new(
        rider_username: String,
        timestamp: DateTime<Local>,
    ) -> Self {
        Self {
            rider_username,
            timestamp,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RideValue {
    pub ride_id: i64,
    pub vehicle_number: i64, // actually u32 at time of writing, but natively i64 in the databases
}
impl RideValue {
    pub fn new(
        ride_id: i64,
        vehicle_number: i64,
    ) -> Self {
        Self {
            ride_id,
            vehicle_number,
        }
    }
}


#[tokio::main]
async fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() != 2 {
        eprintln!("Usage: bim_migration_v2_v3 DBCONNSTRING");
        std::process::exit(1);
    }

    let conn_string = args[1].to_str().expect("connection string is not valid UTF-8");
    let (mut db_client, db_conn) = tokio_postgres::connect(conn_string, NoTls).await
        .expect("failed to connect to Postgres server");
    tokio::spawn(async move {
        if let Err(e) = db_conn.await {
            eprintln!("database connection error: {}", e);
        }
    });

    // create table for vehicles
    db_client.execute(
        "
            CREATE TABLE bim.ride_vehicles
            ( ride_id bigint NOT NULL
            , vehicle_number bigint NOT NULL
            , as_part_of_fixed_coupling boolean NOT NULL
            , CONSTRAINT fkey_ride_vehicles_ride_id FOREIGN KEY (ride_id) REFERENCES bim.rides (id) ON DELETE CASCADE
            , CONSTRAINT pkey_ride_vehicles PRIMARY KEY (ride_id, vehicle_number)
            , CONSTRAINT check_ride_vehicles CHECK (vehicle_number >= 0)
            )
        ",
        &[],
    ).await.expect("ride_vehicles creation query failed");

    // collect vehicles
    let rows = db_client.query(
        "
            SELECT
                r.rider_username,
                r.\"timestamp\",
                r.id,
                r.vehicle_number
            FROM bim.rides r
        ",
        &[],
    ).await.expect("failed to obtain current rides");

    let mut ride_map: HashMap<RideKey, Vec<RideValue>> = HashMap::new();
    for row in &rows {
        let rider_username: String = row.get(0);
        let timestamp: DateTime<Local> = row.get(1);
        let ride_id: i64 = row.get(2);
        let vehicle_number: i64 = row.get(3);

        let key = RideKey::new(rider_username, timestamp);
        let value = RideValue::new(ride_id, vehicle_number);

        let values = ride_map
            .entry(key)
            .or_insert_with(|| Vec::new());
        values.push(value);
    }

    // this is where it becomes destructive... start a transaction
    let xact = db_client.transaction()
        .await.expect("failed to create a transaction");
    let insert_vehicle_stmt = xact.prepare(
        "
            INSERT INTO bim.ride_vehicles
            (ride_id, vehicle_number, as_part_of_fixed_coupling)
            VALUES
            ($1, $2, FALSE)
        "
    )
        .await.expect("failed to create insert vehicle statement");
    let delete_ride_stmt = xact.prepare(
        "DELETE FROM bim.rides WHERE id=$1"
    )
        .await.expect("failed to create delete ride statement");

    let mut rides_to_remove: HashSet<i64> = HashSet::new();
    for values in ride_map.values() {
        if values.len() == 0 {
            continue;
        }

        let lowest_id = values.iter()
            .map(|v| v.ride_id)
            .min()
            .unwrap();

        for value in values {
            xact.execute(
                &insert_vehicle_stmt,
                &[&lowest_id, &value.vehicle_number],
            ).await.expect("failed to insert vehicle");

            if value.ride_id != lowest_id {
                rides_to_remove.insert(value.ride_id);
            }
        }
    }

    for id_to_delete in &rides_to_remove {
        xact.execute(
            &delete_ride_stmt,
            &[id_to_delete],
        ).await.expect("failed to delete ride");
    }

    // and we are done
    xact.commit()
        .await.expect("failed to commit transaction");

    // drop check_rides constraint (it only concerns vehicle_number)
    db_client.query("ALTER TABLE bim.rides DROP CONSTRAINT check_rides", &[])
        .await.expect("failed to drop check_rides constraint");

    // drop vehicle_number column
    db_client.query("ALTER TABLE bim.rides DROP COLUMN vehicle_number", &[])
        .await.expect("failed to drop vehicle_number column");
}
