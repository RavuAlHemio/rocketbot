use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::LazyLock;

use askama::Template;
use http_body_util::Full;
use hyper::{Method, Request, Response};
use hyper::body::{Bytes, Incoming};
use serde::Serialize;
use tracing::error;

use crate::{
    connect_to_db, get_query_pairs, get_query_pairs_multiset, render_response, return_400,
    return_405, return_500,
};


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct KnownColumn {
    pub field_name: &'static str,
    pub column_heading: &'static str,
}
impl KnownColumn {
    pub const fn new(
        field_name: &'static str,
        column_heading: &'static str,
    ) -> Self {
        Self {
            field_name,
            column_heading,
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bim-drilldown.html")]
struct BimDrilldownTemplate {
    pub column_headings: Vec<&'static str>,
    pub rows: Vec<Vec<String>>,
}


static KNOWN_GROUP_TO_COLUMN_INFO: LazyLock<HashMap<&'static str, KnownColumn>> = LazyLock::new(|| {
    let mut ret = HashMap::new();
    ret.insert("company", KnownColumn::new("tbl.company", "company"));
    ret.insert("rider", KnownColumn::new("tbl.rider_username", "rider"));
    ret.insert("timestamp.year", KnownColumn::new("EXTRACT(YEAR FROM tbl.\"timestamp\")", "year"));
    ret.insert("timestamp.month", KnownColumn::new("EXTRACT(MONTH FROM tbl.\"timestamp\")", "month"));
    ret.insert("timestamp.day", KnownColumn::new("EXTRACT(DAY FROM tbl.\"timestamp\")", "day"));
    ret.insert("timestamp.hour", KnownColumn::new("EXTRACT(HOUR FROM tbl.\"timestamp\")", "hour"));
    ret.insert("timestamp.minute", KnownColumn::new("EXTRACT(MINUTE FROM tbl.\"timestamp\")", "minute"));
    ret.insert("timestamp.second", KnownColumn::new("EXTRACT(SECOND FROM tbl.\"timestamp\")", "second"));
    ret.insert("line", KnownColumn::new("tbl.line", "line"));
    ret.insert("vehicle-type", KnownColumn::new("tbl.vehicle_type", "vehicle type"));
    ret.insert("vehicle-number", KnownColumn::new("tbl.vehicle_number", "vehicle"));
    ret
});


pub(crate) async fn handle_bim_drilldown(request: &Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let query_pairs = get_query_pairs(request);

    if request.method() != Method::GET {
        return return_405(&query_pairs).await;
    }

    let query_pairs_multi = get_query_pairs_multiset(request);

    let empty = Vec::with_capacity(0);
    let groups = query_pairs_multi.get("group")
        .unwrap_or(&empty);

    let mut known_columns = Vec::with_capacity(groups.len());
    {
        let mut seen_groups = HashSet::new();
        for group in groups {
            let known_column = match KNOWN_GROUP_TO_COLUMN_INFO.get(&**group) {
                Some(kc) => *kc,
                None => return return_400(&format!("unknown grouping field {:?}", &*group), &query_pairs).await,
            };
            known_columns.push(known_column);

            let is_fresh = seen_groups.insert(group);
            if !is_fresh {
                return return_400(&format!("duplicate grouping field {:?}", &*group), &query_pairs).await;
            }
        }
    }

    let db_conn = match connect_to_db().await {
        Some(c) => c,
        None => return return_500(),
    };

    let mut select_sql_columns_string = String::new();
    let mut sql_columns_string = String::new();
    for known_column in &known_columns {
        if sql_columns_string.len() > 0 {
            select_sql_columns_string.push_str(", ");
            sql_columns_string.push_str(", ");
        }

        select_sql_columns_string.push_str("CAST(");
        select_sql_columns_string.push_str(known_column.field_name);
        select_sql_columns_string.push_str(" AS character varying(256))");

        sql_columns_string.push_str(known_column.field_name);
    }

    if select_sql_columns_string.len() > 0 {
        select_sql_columns_string.push_str(", ");
    }
    select_sql_columns_string.push_str("CAST(COUNT(*) AS character varying(256)) entry_count");

    let query = format!(
        "
            SELECT
                {}
            FROM
                bim.rides_and_ridden_vehicles tbl
            {} {}
            {} {}
        ",
        select_sql_columns_string,
        if sql_columns_string.len() > 0 { "GROUP BY" } else { "" }, sql_columns_string,
        if sql_columns_string.len() > 0 { "ORDER BY" } else { "" }, sql_columns_string,
    );

    let db_rows = match db_conn.query(&query, &[]).await {
        Ok(rs) => rs,
        Err(e) => {
            error!("error querying database: {}", e);
            return return_500();
        },
    };
    let mut rows = Vec::with_capacity(db_rows.len());
    for db_row in db_rows {
        let mut row = Vec::with_capacity(db_row.len());
        for n in 0..db_row.len() {
            let value_opt: Option<String> = db_row.get(n);
            let value = value_opt.unwrap_or_else(|| String::with_capacity(0));
            row.push(value);
        }
        rows.push(row);
    }

    let mut column_headings: Vec<&str> = known_columns.iter()
        .map(|kc| kc.column_heading)
        .collect();
    column_headings.push("count");

    let template = BimDrilldownTemplate {
        column_headings,
        rows,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
