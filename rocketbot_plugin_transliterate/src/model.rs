use regex::Regex;
use rocketbot_interface::serde::serde_regex;
use serde::{Deserialize, Serialize};


#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Language {
    pub abbrev: String,
    pub name: String,
    pub to_lang: Vec<Transformation>,
    pub from_lang: Vec<Transformation>,
}


#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Transformation {
    #[serde(with = "serde_regex")] pub matcher: Regex,
    pub replacements: Vec<Replacement>,
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Replacement {
    #[serde(default = "give_one")] pub weight: u64,
    pub replacement: String,
}


fn give_one() -> u64 { 1 }
