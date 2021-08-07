use log::{error, warn};
use minidom::Element;
use num_bigint::BigInt;
use reqwest;

use crate::units::{BaseUnit, DerivedUnit, NumberUnits, UnitDatabase};


const CURRENCY_URL: &'static str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const ECB_EUROFXREF_NS_URL: &'static str = "http://www.ecb.int/vocabulary/2002-08-01/eurofxref";


pub(crate) async fn update_currencies(unit_database: &mut UnitDatabase) {
    // EUR, the SI base unit for currency ;-)
    if unit_database.get_base_unit("EUR").is_none() {
        unit_database.register_base_unit(BaseUnit::new(
            "EUR".to_owned(),
        )).expect("failed to register base unit EUR");
    }

    // obtain currency info
    let currency_resp = match reqwest::get(CURRENCY_URL).await {
        Ok(cr) => cr,
        Err(e) => {
            error!("failed to get currency info: {}", e);
            return;
        },
    };
    let currency_bytes = match currency_resp.bytes().await {
        Ok(cb) => cb,
        Err(e) => {
            error!("failed to get currency bytes: {}", e);
            return;
        },
    };
    let currency_vec = currency_bytes.to_vec();
    let currency_str = match String::from_utf8(currency_vec) {
        Ok(cs) => cs,
        Err(e) => {
            error!("failed to decode currency info as UTF-8: {}", e);
            return;
        },
    };
    let currency_root: Element = match currency_str.parse() {
        Ok(cr) => cr,
        Err(e) => {
            error!("failed to parse currency info as XML: {}", e);
            return;
        },
    };

    let outer_cube = match currency_root.get_child("Cube", ECB_EUROFXREF_NS_URL) {
        Some(oc) => oc,
        None => {
            error!("failed to get outer Cube in XML");
            return;
        },
    };
    let time_cube = match outer_cube.get_child("Cube", ECB_EUROFXREF_NS_URL) {
        Some(oc) => oc,
        None => {
            error!("failed to get time Cube in XML");
            return;
        },
    };
    for currency_cube in time_cube.children() {
        let currency = match currency_cube.attr("currency") {
            Some(c) => c,
            None => continue,
        };
        let rate_str = match currency_cube.attr("rate") {
            Some(r) => r,
            None => continue,
        };
        let rate_cur_to_eur: f64 = match rate_str.parse() {
            Ok(r) => r,
            Err(e) => {
                warn!("failed to parse rate {:?} of currency {:?} as f64: {}; skipping", rate_str, currency, e);
                continue;
            },
        };

        if rate_cur_to_eur == 0.0 {
            warn!("currency {:?} has zero exchange rate; skipping", currency);
            continue;
        }

        let rate_eur_to_cur = 1.0 / rate_cur_to_eur;

        let change_result = if unit_database.get_derived_unit(currency).is_some() {
            unit_database.change_derived_unit_factor(currency, rate_eur_to_cur)
        } else {
            let mut eur_parent = NumberUnits::new();
            eur_parent.insert("EUR".to_owned(), BigInt::from(1));
            unit_database.register_derived_unit(DerivedUnit::new(
                currency.to_owned(),
                eur_parent,
                rate_eur_to_cur,
            ))
        };
        if let Err(e) = change_result {
            warn!("failed to register currency {:?}: {}; skipping", currency, e);
        }
    }
}
