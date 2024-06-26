use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use once_cell::sync::Lazy;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use regex::Regex;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use toml;
use tracing::error;
use unicode_normalization::UnicodeNormalization;


#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ElementData {
    pub elements: Vec<Element>,
}


#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Element {
    pub atomic_number: u32,
    pub symbol: String,
    pub language_to_name: BTreeMap<String, String>,
}


/// The configuration of the elementary plugin.
///
/// The configuration allows a ramp up of response probabilities with some randomness. If the string
/// of found elements has fewer elements than `random_min_elements`, the bot will never answer; if
/// it has at least `random_max_elements`, the bot will always answer; and otherwise, the
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    #[serde(default = "Config::default_element_data_path")]
    pub element_data_path: String,

    #[serde(default = "Config::default_random_min_elements")]
    pub random_min_elements: usize,

    #[serde(default = "Config::default_random_max_elements")]
    pub random_max_elements: usize,

    #[serde(default)]
    pub count_unique_elements: bool,
}
impl Config {
    fn default_element_data_path() -> String { "elements.toml".to_owned() }
    const fn default_random_min_elements() -> usize { 5 }
    const fn default_random_max_elements() -> usize { 10 }
}


#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PreprocessedData {
    pub elements: Vec<Element>,
    pub lowercase_elements_1: HashMap<char, usize>,
    pub lowercase_elements_2: HashMap<(char, char), usize>,
}


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct State<'s> {
    pub element_indexes: Vec<usize>,
    pub remaining_string: &'s str,
}


static WHITESPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    "\\s+"
).expect("failed to compile whitespace regex"));


fn preprocess_data(elements: Vec<Element>) -> PreprocessedData {
    let mut lowercase_elements_1 = HashMap::new();
    let mut lowercase_elements_2 = HashMap::new();
    for (i, element) in elements.iter().enumerate() {
        let lowercase_chars: Vec<char> = element.symbol.to_lowercase().chars().collect();
        if lowercase_chars.len() == 1 {
            lowercase_elements_1.insert(lowercase_chars[0], i);
        } else {
            assert_eq!(lowercase_chars.len(), 2);
            lowercase_elements_2.insert((lowercase_chars[0], lowercase_chars[1]), i);
        }
    }
    PreprocessedData {
        elements,
        lowercase_elements_1,
        lowercase_elements_2,
    }
}


fn find_elements(lowercase_ascii_letters_str: &str, data: &PreprocessedData) -> Option<Vec<usize>> {
    let base_state = State {
        element_indexes: Vec::with_capacity(lowercase_ascii_letters_str.len()),
        remaining_string: lowercase_ascii_letters_str,
    };
    let mut states = vec![base_state];
    while let Some(state) = states.pop() {
        if state.remaining_string.len() == 0 {
            // yay!
            return Some(state.element_indexes);
        }

        // try to match another element
        if let Some(c1) = state.remaining_string.chars().nth(0) {
            if let Some(c1_index) = data.lowercase_elements_1.get(&c1) {
                // it's a single-letter element!
                let mut new_element_indexes = state.element_indexes.clone();
                new_element_indexes.push(*c1_index);
                let new_state = State {
                    element_indexes: new_element_indexes,
                    remaining_string: &state.remaining_string[c1.len_utf8()..],
                };
                states.push(new_state);
            }

            if let Some(c2) = state.remaining_string.chars().nth(1) {
                if let Some(c2_index) = data.lowercase_elements_2.get(&(c1, c2)) {
                    // it's a two-letter element!
                    let mut new_element_indexes = state.element_indexes.clone();
                    new_element_indexes.push(*c2_index);
                    let new_state = State {
                        element_indexes: new_element_indexes,
                        remaining_string: &state.remaining_string[(c1.len_utf8() + c2.len_utf8())..],
                    };
                    states.push(new_state);
                }
            }
        }
    }

    None
}


fn get_element_index(lowercase_symbol: &str, data: &PreprocessedData) -> Option<usize> {
    let char_count = lowercase_symbol.chars().count();
    if char_count == 1 {
        let c1 = lowercase_symbol.chars().nth(0).unwrap();
        data.lowercase_elements_1.get(&c1)
            .map(|i| *i)
    } else if char_count == 2 {
        let c1 = lowercase_symbol.chars().nth(0).unwrap();
        let c2 = lowercase_symbol.chars().nth(1).unwrap();
        data.lowercase_elements_2.get(&(c1, c2))
            .map(|i| *i)
    } else {
        None
    }
}


pub struct ElementaryPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    rng: Mutex<StdRng>,
    preprocessed_data: RwLock<PreprocessedData>,
}
impl ElementaryPlugin {
    async fn channel_command_elements(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let lang_opt = command.options.get("lang")
            .or_else(|| command.options.get("l"))
            .and_then(|cv| cv.as_str());

        let elements: Vec<String> = {
            let data_guard = self.preprocessed_data.read().await;

            WHITESPACE_RE.split(&command.rest.to_lowercase())
                .map(|piece| get_element_index(piece, &*data_guard))
                .map(|i_opt| {
                    if let Some(i) = i_opt {
                        if let Some(lang) = lang_opt {
                            data_guard.elements[i].language_to_name
                                .get(lang)
                                .map(|name| name.to_owned())
                                .unwrap_or_else(|| "?".to_owned())
                        } else {
                            format!("{}", data_guard.elements[i].atomic_number)
                        }
                    } else {
                        "??".to_owned()
                    }
                })
                .collect()
        };

        let response = elements.join(", ");
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for ElementaryPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object: Config = serde_json::from_value(config)
            .expect("failed to parse config");
        let element_data_path = config_object.element_data_path.clone();
        let config_lock = RwLock::new(
            "ElementaryPlugin::config",
            config_object,
        );

        let element_data: ElementData = {
            let element_data_string = std::fs::read_to_string(&element_data_path)
                .expect("failed to read element data");
            toml::from_str(&element_data_string)
                .expect("failed to parse element data")
        };

        let std_rng = StdRng::from_entropy();
        let rng = Mutex::new(
            "ElementaryPlugin::rng",
            std_rng,
        );

        let preprocessed_data = preprocess_data(element_data.elements);
        let preprocessed_data_lock = RwLock::new(
            "ElementaryPlugin::preprocessed_data",
            preprocessed_data,
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "elements",
                "elementary",
                "{cpfx}elements [{lopfx}lang LANG] ELEMENTSYMBOLS",
                "Translates element symbols into their names.",
            )
                .add_option("l", CommandValueType::String)
                .add_option("lang", CommandValueType::String)
                .build()
        ).await;

        ElementaryPlugin {
            interface,
            config: config_lock,
            rng,
            preprocessed_data: preprocessed_data_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "elementary".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let body = match &channel_message.message.raw {
            Some(b) => b,
            None => return,
        };
        if body.len() == 0 {
            return;
        }

        // don't trigger if Serious Mode is active
        let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
        if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
            if serious_mode_until > Local::now().timestamp() {
                return;
            }
        }

        // lowercase and normalize message
        let lowercase_body = body.to_lowercase();
        let normalized_letters_body: String = lowercase_body.nfkd()
            .filter(|c| c.is_lowercase())
            .collect();
        if normalized_letters_body.len() == 0 {
            return;
        }
        if normalized_letters_body.chars().any(|c| !c.is_ascii_lowercase()) {
            // guess we have lowercase but non-ASCII letters in this message; skip it
            return;
        }

        // get randomness values
        let (lower_bound, upper_bound, count_unique) = {
            let config_guard = self.config.read().await;
            (
                config_guard.random_min_elements,
                config_guard.random_max_elements,
                config_guard.count_unique_elements,
            )
        };
        let preprocessed_data = {
            let data_guard = self.preprocessed_data.read().await;
            (*data_guard).clone()
        };

        if let Some(found_element_indexes) = find_elements(&normalized_letters_body, &preprocessed_data) {
            let element_count = if count_unique {
                let unique_indexes: HashSet<&usize> = found_element_indexes.iter()
                    .collect();
                unique_indexes.len()
            } else {
                found_element_indexes.len()
            };

            let output_response = if element_count < lower_bound {
                // too few
                false
            } else if element_count >= upper_bound {
                // enough to be an interesting message
                true
            } else {
                // throw the die

                // lower = 3, upper = 7
                // 0 => false
                // 1 => false
                // 2 => false (also applies if we run it through the RNG formula)
                // 3 =>
                //      (upper_bound - lower_bound) + 1 = 5
                //      (3 - lower_bound) + 1 = 1
                //      1 / 5 = 0.2
                //      rand() < 0.2
                // 4 =>
                //      (upper_bound - lower_bound) + 1 = 5
                //      (4 - lower_bound) + 1 = 2
                //      2 / 5 = 0.4
                //      rand() < 0.4
                // 5 =>
                //      (upper_bound - lower_bound) + 1 = 5
                //      (5 - lower_bound) + 1 = 3
                //      3 / 5 = 0.6
                //      rand() < 0.6
                // 6 =>
                //      (upper_bound - lower_bound) + 1 = 5
                //      (6 - lower_bound) + 1 = 4
                //      4 / 5 = 0.8
                //      rand() < 0.8
                // 7 => true (also applies if we run it through the RNG formula)

                let step_count = (upper_bound - lower_bound) + 1;
                let numerator = (element_count - lower_bound) + 1;
                let cutoff = (numerator as f64) / (step_count as f64);

                let random_value: f64 = {
                    let mut rng_lock = self.rng.lock().await;
                    rng_lock.gen() // 0.0 <= x < 1.0
                };

                random_value < cutoff
            };

            if output_response {
                let mut output_message = format!("Congratulations! Your message can be expressed as a sequence of chemical elements:");
                for element_index in found_element_indexes {
                    output_message.push(' ');
                    output_message.push_str(&preprocessed_data.elements[element_index].symbol);
                }
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &output_message,
                ).await;
            }
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "elements" {
            self.channel_command_elements(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "elements" {
            Some(include_str!("../help/elements.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        let config_object: Config = match serde_json::from_value(new_config) {
            Ok(c) => c,
            Err(e) => {
                error!("failed to parse new elementary config: {}", e);
                return false;
            },
        };

        let element_data: ElementData = {
            let element_data_string = match std::fs::read_to_string(&config_object.element_data_path) {
                Ok(eds) => eds,
                Err(e) => {
                    error!("failed to read new element data from file {:?}: {}", config_object.element_data_path, e);
                    return false;
                },
            };
            match toml::from_str(&element_data_string) {
                Ok(ed) => ed,
                Err(e) => {
                    error!("failed to parse new element data from file {:?}: {}", config_object.element_data_path, e);
                    return false;
                },
            }
        };
        let preprocessed_data = preprocess_data(element_data.elements);

        {
            let mut config_guard = self.config.write().await;
            *config_guard = config_object;
        }
        {
            let mut preprocessed_data_guard = self.preprocessed_data.write().await;
            *preprocessed_data_guard = preprocessed_data;
        }
        true
    }
}


#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use super::{Element, find_elements, preprocess_data};

    fn make_elements(elements: &[&str]) -> Vec<Element> {
        let mut element_vec = Vec::with_capacity(elements.len());
        for (i, elem) in elements.iter().enumerate() {
            let atomic_number = (i + 1).try_into().unwrap();
            let element = Element {
                atomic_number,
                symbol: (*elem).to_owned(),
                language_to_name: BTreeMap::new(),
            };
            element_vec.push(element);
        }
        element_vec
    }

    #[test]
    fn test_find_elements() {
        let test_elements = make_elements(&[
            "H",  "He", "Li", "Be", "B",  "C",  "N",  "O",
            "F",  "Ne", "Na", "Mg", "Al", "Si", "P",  "S",
            "Cl", "Ar", "K",  "Ca", "Sc", "Ti", "V",  "Cr",
            "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge",
            "As", "Se", "Br", "Kr", "Rb", "Sr", "Y",  "Zr",
            "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd",
            "In", "Sn", "Sb", "Te", "I",  "Xe", "Cs", "Ba",
            "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd",
            "Tb", "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf",
            "Ta", "W",  "Re", "Os", "Ir", "Pt", "Au", "Hg",
            "Tl", "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra",
            "Ac", "Th", "Pa", "U",  "Np", "Pu", "Am", "Cm",
            "Bk", "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf",
            "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn",
            "Nh", "Fl", "Mc", "Lv", "Ts", "Og",
        ]);

        let data = preprocess_data(test_elements);
        assert_eq!(find_elements("", &data).unwrap().len(), 0);
        assert_eq!(find_elements("h", &data).unwrap(), [0]);
        assert_eq!(find_elements("hehe", &data).unwrap(), [1, 1]);
        assert_eq!(find_elements("cowpokes", &data).unwrap(), [26, 73, 83, 18, 98]);
        assert!(find_elements("cowpoked", &data).is_none());
    }
}
