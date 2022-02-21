use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;

use tokio_postgres::NoTls;


#[tokio::main]
async fn main() {
    // load messages
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() != 2 {
        eprintln!("Usage: renumber_bim_rides DBCONNSTRING");
        std::process::exit(1);
    }

    // connect to database
    let conn_string = args[1].to_str().expect("connection string is not valid UTF-8");
    let (mut db_client, db_conn) = tokio_postgres::connect(conn_string, NoTls).await
        .expect("failed to connect to Postgres server");
    tokio::spawn(async move {
        if let Err(e) = db_conn.await {
            eprintln!("database connection error: {}", e);
        }
    });

    // open a transaction
    {
        let xact = db_client.transaction()
            .await.expect("failed to open database transaction");

        // prepare update statements
        let update_rides_stmt = xact.prepare("UPDATE bim.rides SET id = $1 WHERE id = $2")
            .await.expect("failed to prepare update-rides statement");
        let update_vehicles_stmt = xact.prepare("UPDATE bim.ride_vehicles SET ride_id = $1 WHERE ride_id = $2")
            .await.expect("failed to prepare update-vehicles statement");
        let set_sequence_stmt = xact.prepare("SELECT setval('bim.rides__id', $1, TRUE)")
            .await.expect("failed to prepare set-sequence statement");

        // defer constraints
        xact.execute("SET CONSTRAINTS ALL DEFERRED", &[])
            .await.expect("failed to set constraints to deferred");

        // obtain rows
        let rows = xact.query("SELECT id FROM bim.rides ORDER BY id", &[])
            .await.expect("failed to query rides");

        // calculate old-to-new mappings
        let mut renumber_ids: BTreeMap<i64, i64> = BTreeMap::new();
        let mut last_id: i64 = 0;
        for row in rows {
            let this_id: i64 = row.get(0);
            last_id += 1;
            if this_id != last_id {
                renumber_ids.insert(this_id, last_id);
            }
        }

        // update rows
        for (old_id, new_id) in &renumber_ids {
            xact.execute(&update_rides_stmt, &[&new_id, &old_id])
                .await.expect("failed to update rides");
            xact.execute(&update_vehicles_stmt, &[&new_id, &old_id])
                .await.expect("failed to update vehicles");
        }

        // update sequence
        xact.execute(&set_sequence_stmt, &[&last_id])
            .await.expect("failed to update sequence");

        // commit to it
        xact.commit()
            .await.expect("failed to commit changes");
    }
}
