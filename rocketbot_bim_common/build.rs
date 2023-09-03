use std::io::{BufRead, BufReader, Write};
use std::fs::File;

use regex::Regex;


struct AchievementDef {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub order_category: u64,
    pub order_entry: u64,
}


fn main() {
    println!("cargo:rerun-if-changed=../rocketbot_plugin_bim/db/achievements.pgsql");

    // assemble the regexes
    let name_re = Regex::new("^    -- NAME: (.+)$")
        .expect("failed to compile regex");
    let descr_re = Regex::new("^    -- DESCR: (.+)$")
        .expect("failed to compile regex");
    let order_re = Regex::new("^    -- ORDER: ([1-9][0-9]*),([1-9][0-9]*)(?: .+)?$")
        .expect("failed to compile regex");
    let id_re = Regex::new("^    SELECT ([1-9][0-9]*),")
        .expect("failed to compile regex");

    let mut defs = Vec::new();

    {
        // extract achievements from SQL
        let f = File::open("../rocketbot_plugin_bim/db/achievements.pgsql")
            .expect("failed to open ../rocketbot_plugin_bim/db/achievements.pgsql");
        let mut buf_reader = BufReader::new(f);
        let mut line = String::new();

        let mut cur_name = None;
        let mut cur_descr = None;
        let mut cur_order = None;

        loop {
            line.clear();
            match buf_reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {},
                Err(e) => panic!("error while reading ../rocketbot_plugin_bim/db/achievements.pgsql: {}", e),
            }

            let line_no_newline = line.trim_end_matches(['\r', '\n']);

            if let Some(caps) = name_re.captures(line_no_newline) {
                cur_name = Some(caps.get(1).unwrap().as_str().to_owned());
            } else if let Some(caps) = descr_re.captures(line_no_newline) {
                cur_descr = Some(caps.get(1).unwrap().as_str().to_owned());
            } else if let Some(caps) = order_re.captures(line_no_newline) {
                let order_category: u64 = caps.get(1).unwrap().as_str().parse().unwrap();
                let order_entry: u64 = caps.get(2).unwrap().as_str().parse().unwrap();
                cur_order = Some((order_category, order_entry));
            } else if let Some(caps) = id_re.captures(line_no_newline) {
                let id: i64 = caps.get(1).unwrap().as_str().parse().unwrap();
                if let Some(name) = cur_name {
                    if let Some(description) = cur_descr {
                        if let Some((order_category, order_entry)) = cur_order {
                            defs.push(AchievementDef {
                                id,
                                name,
                                description,
                                order_category,
                                order_entry,
                            });
                        }
                    }
                }
                cur_name = None;
                cur_descr = None;
                cur_order = None;
            }
        }
    }

    defs.sort_unstable_by_key(|entry| (entry.order_category, entry.order_entry, entry.id));

    {
        // write achievements to Rust file
        let mut f = File::create("src/achievements/definitions.rs")
            .expect("failed to create src/achievements/definitions.rs");

        writeln!(f, "// This file has been automatically generated from .../rocketbot_plugin_bim/db/achievements.pgsql.").unwrap();
        writeln!(f, "// Manual changes will be lost.").unwrap();
        writeln!(f).unwrap();
        writeln!(f).unwrap();
        writeln!(f, "use crate::achievements::AchievementDef;").unwrap();
        writeln!(f).unwrap();
        writeln!(f).unwrap();
        writeln!(f, "pub static ACHIEVEMENT_DEFINITIONS: [AchievementDef; {}] = [", defs.len()).unwrap();
        for def in defs {
            writeln!(f, "    AchievementDef::new({:?}, {:?}, {:?}),", def.id, def.name, def.description).unwrap();
        }
        writeln!(f, "];").unwrap();
    }
}
