pub mod hunspell;


use std::path::PathBuf;

use log::error;
use serde::{Deserialize, Serialize};

use crate::hunspell::HunspellDictionary;


pub trait SpellingEngine : Sized {
    fn new(config: serde_json::Value) -> Option<Self>;
    fn is_correct(&self, word: &str) -> bool;
    fn suggest(&self, word: &str) -> Vec<String>;
}


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct HunspellEngineConfig {
    dictionaries: Vec<HunspellDictConfig>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct HunspellDictConfig {
    affix: String,
    dict: String,
    #[serde(default)] key: Option<String>,
    #[serde(default)] additional_dicts: Vec<String>,
}


pub struct HunspellEngine {
    dictionaries: Vec<HunspellDictionary>,
}
impl SpellingEngine for HunspellEngine {
    fn new(config: serde_json::Value) -> Option<Self> {
        let config_object: HunspellEngineConfig = match serde_json::from_value(config) {
            Ok(co) => co,
            Err(e) => {
                error!("failed to parse config: {}", e);
                return None;
            },
        };

        let mut dictionaries = Vec::new();
        for dict_config in &config_object.dictionaries {
            let affix_path = PathBuf::from(&dict_config.affix);
            let dict_path = PathBuf::from(&dict_config.dict);
            let hunspell_res = HunspellDictionary::new(
                &affix_path,
                &dict_path,
                dict_config.key.clone(),
            );
            let mut hunspell = match hunspell_res {
                Ok(h) => h,
                Err(e) => {
                    error!(
                        "failed to initialize Hunspell with affix {:?} and dict {:?}: {}",
                        dict_config.affix, dict_config.dict, e,
                    );
                    return None;
                },
            };

            for add_dict in &dict_config.additional_dicts {
                let add_dict_path = PathBuf::from(&add_dict);
                match hunspell.add_dictionary(&add_dict_path) {
                    Ok(true) => {},
                    Ok(false) => {
                        error!("failed to add Hunspell dictionary {:?}", add_dict);
                        return None;
                    },
                    Err(e) => {
                        error!("failed to add Hunspell dictionary {:?}: {}", add_dict, e);
                        return None;
                    },
                }
            }
            dictionaries.push(hunspell);
        }

        Some(Self {
            dictionaries,
        })
    }

    fn is_correct(&self, word: &str) -> bool {
        for (i, dict) in self.dictionaries.iter().enumerate() {
            match dict.spell(word) {
                Ok(true) => return true,
                Ok(false) => continue, // try the next dictionary
                Err(e) => {
                    error!(
                        "error spelling {:?} with Hunspell dictionary at index {}: {}",
                        word, i, e,
                    );
                    // assume it is spelled correctly
                    return true;
                },
            }
        }

        // all dictionaries said "no"
        false
    }

    fn suggest(&self, word: &str) -> Vec<String> {
        let mut all_suggestions = Vec::new();

        for (i, dict) in self.dictionaries.iter().enumerate() {
            match dict.suggest(word) {
                Ok(mut suggs) => {
                    all_suggestions.append(&mut suggs);
                },
                Err(e) => {
                    error!(
                        "error collecting suggestions for {:?} with Hunspell dictionary at index {}: {}",
                        word, i, e,
                    );
                    // try the next dictionary
                },
            }
        }

        all_suggestions
    }
}
