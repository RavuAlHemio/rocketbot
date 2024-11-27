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
use crate::templating::filters;


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct GroupColumn {
    pub field_name: &'static str,
    pub column_heading: &'static str,
}
impl GroupColumn {
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct FilterColumn {
    pub field_name: &'static str,
    pub field_type: FilterFieldType,
}
impl FilterColumn {
    pub const fn new(
        field_name: &'static str,
        field_type: FilterFieldType,
    ) -> Self {
        Self {
            field_name,
            field_type,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum FilterFieldType {
    String,
    U128,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum FilterFieldValue {
    String(String),
    U128(u128),
}
impl FilterFieldValue {
    pub fn to_sql_string(&self) -> String {
        match self {
            Self::String(s) => {
                let mut ret = String::with_capacity(s.len() + 2);
                ret.push('\'');
                for c in s.chars() {
                    if c == '\'' {
                        // double up
                        ret.push('\'');
                    }
                    ret.push(c);
                }
                ret.push('\'');
                ret
            },
            Self::U128(u) => format!("{}", u),
        }
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Template)]
#[template(path = "bim-drilldown.html")]
struct BimDrilldownTemplate {
    pub known_column_names: Vec<&'static str>,
    pub column_headings: Vec<&'static str>,
    pub rows: Vec<Vec<String>>,
}


static KNOWN_GROUP_TO_COLUMN_INFO: LazyLock<HashMap<&'static str, GroupColumn>> = LazyLock::new(|| {
    let mut ret = HashMap::new();
    ret.insert("company", GroupColumn::new("tbl.company", "company"));
    ret.insert("rider", GroupColumn::new("tbl.rider_username", "rider"));
    ret.insert("timestamp.year", GroupColumn::new("EXTRACT(YEAR FROM tbl.\"timestamp\")", "year"));
    ret.insert("timestamp.month", GroupColumn::new("EXTRACT(MONTH FROM tbl.\"timestamp\")", "month"));
    ret.insert("timestamp.day", GroupColumn::new("EXTRACT(DAY FROM tbl.\"timestamp\")", "day"));
    ret.insert("timestamp.weekday", GroupColumn::new("EXTRACT(DOW FROM tbl.\"timestamp\")", "weekday"));
    ret.insert("timestamp.hour", GroupColumn::new("EXTRACT(HOUR FROM tbl.\"timestamp\")", "hour"));
    ret.insert("timestamp.minute", GroupColumn::new("EXTRACT(MINUTE FROM tbl.\"timestamp\")", "minute"));
    ret.insert("timestamp.second", GroupColumn::new("EXTRACT(SECOND FROM tbl.\"timestamp\")", "second"));
    ret.insert("line", GroupColumn::new("tbl.line", "line"));
    ret.insert("vehicle-type", GroupColumn::new("tbl.vehicle_type", "vehicle type"));
    ret.insert("vehicle-number", GroupColumn::new("tbl.vehicle_number", "vehicle"));
    ret
});

static KNOWN_FILTER_TO_COLUMN_INFO: LazyLock<HashMap<&'static str, FilterColumn>> = LazyLock::new(|| {
    use FilterFieldType as F;
    let mut ret = HashMap::new();
    ret.insert("company", FilterColumn::new("tbl.company", F::String));
    ret.insert("rider", FilterColumn::new("tbl.rider_username", F::String));
    ret.insert("timestamp.year", FilterColumn::new("EXTRACT(YEAR FROM tbl.\"timestamp\")", F::U128));
    ret.insert("timestamp.month", FilterColumn::new("EXTRACT(MONTH FROM tbl.\"timestamp\")", F::U128));
    ret.insert("timestamp.day", FilterColumn::new("EXTRACT(DAY FROM tbl.\"timestamp\")", F::U128));
    ret.insert("timestamp.weekday", FilterColumn::new("EXTRACT(DOW FROM tbl.\"timestamp\")", F::U128));
    ret.insert("timestamp.hour", FilterColumn::new("EXTRACT(HOUR FROM tbl.\"timestamp\")", F::U128));
    ret.insert("timestamp.minute", FilterColumn::new("EXTRACT(MINUTE FROM tbl.\"timestamp\")", F::U128));
    ret.insert("timestamp.second", FilterColumn::new("EXTRACT(SECOND FROM tbl.\"timestamp\")", F::U128));
    ret.insert("line", FilterColumn::new("tbl.line", F::String));
    ret.insert("vehicle-type", FilterColumn::new("tbl.vehicle_type", F::String));
    ret.insert("vehicle-number", FilterColumn::new("tbl.vehicle_number", F::String));
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
    let filter_strings = query_pairs_multi.get("filter")
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

    let mut known_filters = Vec::with_capacity(filter_strings.len());
    {
        let mut seen_filters = HashSet::new();
        for filter_string in filter_strings {
            let (key, value) = match filter_string.split_once('=') {
                Some(kv) => kv,
                None => return return_400(&format!("filter string {:?} does not contain an equals sign", &*filter_string), &query_pairs).await,
            };

            let known_column = match KNOWN_FILTER_TO_COLUMN_INFO.get(key) {
                Some(kc) => *kc,
                None => return return_400(&format!("unknown filter field {:?}", key), &query_pairs).await,
            };
            let is_fresh = seen_filters.insert(key);
            if !is_fresh {
                return return_400(&format!("duplicate filter field {:?}", key), &query_pairs).await;
            }

            let field_value = match known_column.field_type {
                FilterFieldType::String => FilterFieldValue::String(value.to_owned()),
                FilterFieldType::U128 => {
                    let parsed: u128 = match value.parse() {
                        Ok(p) => p,
                        Err(_) => return return_400(&format!("invalid numeric value {:?} for filter field {:?}", value, key), &query_pairs).await,
                    };
                    FilterFieldValue::U128(parsed)
                },
            };
            known_filters.push((known_column, field_value));
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

    let mut where_string = String::new();
    for (column, value) in known_filters {
        if where_string.len() > 0 {
            where_string.push_str(" AND ");
        } else {
            where_string.push_str("WHERE ");
        }
        where_string.push_str(&column.field_name);
        where_string.push_str(" = ");
        where_string.push_str(&value.to_sql_string());
    }

    let query = format!(
        "
            SELECT
                {}
            FROM
                bim.rides_and_ridden_vehicles tbl
            {}
            {} {}
            {} {}
        ",
        select_sql_columns_string,
        where_string,
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

    let mut known_column_names: Vec<&str> = KNOWN_GROUP_TO_COLUMN_INFO
        .keys()
        .map(|kc| *kc)
        .collect();
    known_column_names.sort_unstable();

    let mut column_headings: Vec<&str> = known_columns.iter()
        .map(|kc| kc.column_heading)
        .collect();
    column_headings.push("count");

    let template = BimDrilldownTemplate {
        known_column_names,
        column_headings,
        rows,
    };
    match render_response(&template, &query_pairs, 200, vec![]).await {
        Some(r) => Ok(r),
        None => return_500(),
    }
}
