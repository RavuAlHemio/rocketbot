//! Obtain vehicle databases from the SpotLog LocoList.


use std::collections::{BTreeMap, HashMap};
use std::env::args_os;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use ciborium;
use indexmap::IndexSet;
use reqwest;
use rocketbot_bim_common::{VehicleClass, VehicleInfo, VehicleNumber};
use serde::{Deserialize, Serialize};
use serde_json;
use sxd_document;


#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct Config {
    pub pages: Vec<PageInfo>,
    pub output_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct PageInfo {
    pub export_url: String,
    pub timeout_ms: Option<u64>,
    pub web_id_to_export_class: HashMap<String, ExportClass>,
}

#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialEq)]
struct ExportClass {
    pub type_code: String,
    pub vehicle_class: VehicleClass,
    #[serde(default)] pub other_data: BTreeMap<String, String>,
    #[serde(default)] pub include_deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VehicleInfoBuilder {
    number: Option<VehicleNumber>,
    type_code: Option<String>,
    vehicle_class: Option<VehicleClass>,
    in_service_since: Option<String>,
    out_of_service_since: Option<String>,
    manufacturer: Option<String>,
    other_data: BTreeMap<String, String>,
    fixed_coupling: IndexSet<VehicleNumber>,
}
impl VehicleInfoBuilder {
    pub fn new() -> Self {
        Self {
            number: None,
            type_code: None,
            vehicle_class: None,
            in_service_since: None,
            out_of_service_since: None,
            manufacturer: None,
            other_data: BTreeMap::new(),
            fixed_coupling: IndexSet::new(),
        }
    }

    pub fn number(&mut self, number: VehicleNumber) -> &mut Self {
        self.number = Some(number);
        self
    }

    pub fn type_code<T: Into<String>>(&mut self, type_code: T) -> &mut Self {
        self.type_code = Some(type_code.into());
        self
    }

    pub fn vehicle_class(&mut self, vehicle_class: VehicleClass) -> &mut Self {
        self.vehicle_class = Some(vehicle_class);
        self
    }

    pub fn in_service_since<S: Into<String>>(&mut self, since: S) -> &mut Self {
        self.in_service_since = Some(since.into());
        self
    }

    pub fn out_of_service_since<S: Into<String>>(&mut self, since: S) -> &mut Self {
        self.out_of_service_since = Some(since.into());
        self
    }

    pub fn manufacturer<M: Into<String>>(&mut self, manuf: M) -> &mut Self {
        self.manufacturer = Some(manuf.into());
        self
    }

    pub fn other_data<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) -> &mut Self {
        self.other_data.insert(key.into(), value.into());
        self
    }

    pub fn fixed_coupling<C: IntoIterator<Item = VehicleNumber>>(&mut self, coupling: C) -> &mut Self {
        self.fixed_coupling = coupling.into_iter().collect();
        self
    }

    pub fn try_build(self) -> Result<VehicleInfo, Self> {
        let number = match &self.number {
            Some(n) => n.clone(),
            None => return Err(self),
        };
        let vehicle_class = match self.vehicle_class {
            Some(vc) => vc,
            None => return Err(self),
        };
        let type_code = match self.type_code {
            Some(tc) => tc,
            None => return Err(self),
        };
        Ok(VehicleInfo {
            number,
            type_code,
            vehicle_class,
            in_service_since: self.in_service_since,
            out_of_service_since: self.out_of_service_since,
            manufacturer: self.manufacturer,
            other_data: self.other_data,
            fixed_coupling: self.fixed_coupling,
        })
    }
}


async fn obtain_page_bytes(url: &str, timeout: Option<Duration>) -> Vec<u8> {
    if let Some(file_path) = url.strip_prefix("file://") {
        // it's a local file
        let mut f = File::open(file_path)
            .expect("failed to open local page");
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .expect("failed to read local page");
        buf
    } else {
        let mut client_builder = reqwest::Client::builder();
        if let Some(to) = timeout {
            client_builder = client_builder.timeout(to);
        }
        let client = client_builder.build()
            .expect("failed to build client");
        let request = client.get(url)
            .build().expect("failed to build request");
        let response = client.execute(request).await
            .expect("failed to obtain response");
        let response_bytes = response.bytes().await
            .expect("failed to obtain response bytes");
        response_bytes.to_vec()
    }
}


#[tokio::main]
async fn main() {
    // load config
    let config: Config = {
        let config_path = match args_os().nth(1) {
            Some(cp) => PathBuf::from(cp),
            None => PathBuf::from("obtain_bim_sl.json"),
        };
        let f = File::open(config_path)
            .expect("failed to open config file");
        serde_json::from_reader(f)
            .expect("failed to parse config file")
    };

    let mut vehicles: Vec<VehicleInfo> = Vec::new();
    for page in &config.pages {
        eprintln!("fetching {}", page.export_url);

        let timeout = page.timeout_ms.map(|ms| Duration::from_millis(ms));

        let page_bytes = obtain_page_bytes(&page.export_url, timeout).await;
        let page_string = String::from_utf8(page_bytes)
            .expect("failed to decode page as UTF-8");
        let package = sxd_document::parser::parse(&page_string)
            .expect("failed to parse page as XML");
        let document = package.as_document();

        let definition = document.root()
            .children().iter()
            .filter_map(|c| c.element())
            .nth(0).expect("no root element found");
        if definition.name().local_part() != "Definition" {
            panic!("root element is not named \"Definition\"");
        }

        let set = definition.children().iter()
            .filter_map(|c| c.element())
            .filter(|e| e.name().local_part() == "Set")
            .nth(0).expect("no Set child of Definition found");

        let classes = set.children().into_iter()
            .filter_map(|c| c.element())
            .filter(|e| e.name().local_part() == "Class");
        for class in classes {
            // is this class interesting for us?
            let web_id = match class.attribute_value("Webid") {
                Some(wi) => wi,
                None => continue, // classes without Webid are not interesting to us
            };
            let class_def = match page.web_id_to_export_class.get(web_id) {
                Some(cd) => cd,
                None => continue, // not one of the classes we care about
            };

            let builder_opt = class.attribute_value("Builder");
            let introduced: Option<&str> = class.attribute_value("Introduced");
            let withdrawn: Option<&str> = class.attribute_value("Withdrawn");

            let locos = class.children().into_iter()
                .filter_map(|c| c.element())
                .filter(|e| e.name().local_part() == "Loco");
            for loco in locos {
                let formation_str_opt = loco.attribute_value("Form")
                    .and_then(|f| if f.len() == 0 { None } else { Some(f) })
                    .or_else(|| loco.attribute_value("Number"))
                    .and_then(|f| if f.len() == 0 { None } else { Some(f) });
                let formation_str = match formation_str_opt {
                    Some(f) => f,
                    None => continue,
                };
                // formation might be split by "," or ", "
                let formation: IndexSet<VehicleNumber> = formation_str
                    .split(",")
                    .map(|s| if let Some(spaceless) = s.strip_prefix(" ") {
                        spaceless.to_owned().into()
                    } else {
                        s.to_owned().into()
                    })
                    .collect();

                let is_withdrawn = loco.attribute_value("Status")
                    .map(|v| v == "W" || v == "X") // withdrawn or scrapped
                    .unwrap_or(false);
                let is_deleted = loco.attribute_value("Deleted")
                    .map(|v| v == "true")
                    .unwrap_or(false);
                if is_deleted && !class_def.include_deleted {
                    continue;
                }

                // additional attributes
                let mut my_other_data = BTreeMap::new();
                for attrib in loco.attributes() {
                    let name = attrib.name().local_part();
                    if name == "id" || name == "Webid" {
                        // SpotLog LocoList internal ID
                        continue;
                    }
                    if name == "Number" || name == "Form" || name == "form" {
                        // vehicle numbers are processed further above
                        continue;
                    }
                    if name == "Status" {
                        // status is processed above
                        continue;
                    }
                    if name == "Updated" {
                        // SpotLog LocoList internal timestamp
                        continue;
                    }

                    if attrib.value().trim().len() == 0 {
                        // no value, no interest
                        continue;
                    }

                    my_other_data.insert(name.to_owned(), attrib.value().to_owned());
                }

                for veh in &formation {
                    let mut vib = VehicleInfoBuilder::new();
                    vib.number(veh.clone())
                        .type_code(&class_def.type_code)
                        .vehicle_class(class_def.vehicle_class);
                    if let Some(b) = builder_opt {
                        vib.manufacturer(b);
                    }
                    if let Some(i) = introduced {
                        vib.in_service_since(i);
                    }
                    if is_withdrawn {
                        vib.out_of_service_since(withdrawn.unwrap_or("?"));
                    }

                    for (k, v) in &class_def.other_data {
                        vib.other_data(k, v);
                    }
                    // my data overrides class data
                    for (k, v) in &my_other_data {
                        vib.other_data(k, v);
                    }

                    if formation.len() > 1 {
                        vib.fixed_coupling(formation.clone());
                    }

                    let vehicle = vib.try_build()
                        .expect("failed to build vehicle");
                    vehicles.push(vehicle);
                }
            }
        }
    }

    vehicles.sort_unstable_by_key(|v| v.number.clone());

    // clear out duplicates
    let mut i = 1;
    while i < vehicles.len() {
        let left = &vehicles[i-1];
        let right = &vehicles[i];
        if left == right {
            vehicles.remove(i);
            continue;
        }

        if left.number == right.number {
            println!("dupe! {:?} vs. {:?}", left, right);
            // remove the older one (assume it's the one that came first)
            vehicles.remove(i - 1);
        } else {
            i += 1;
        }
    }

    // output
    {
        let f = File::create(config.output_path)
            .expect("failed to open output file");
        ciborium::into_writer(&vehicles, f)
            .expect("failed to write vehicles");
    }
}
