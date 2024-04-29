use std::collections::{HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use log::error;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;


const ELEMENTS: [&str; 118] = [
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
];


/// The configuration of the elementary plugin.
///
/// The configuration allows a ramp up of response probabilities with some randomness. If the string
/// of found elements has fewer elements than `random_min_elements`, the bot will never answer; if
/// it has at least `random_max_elements`, the bot will always answer; and otherwise, the
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    #[serde(default = "Config::default_random_min_elements")]
    pub random_min_elements: usize,

    #[serde(default = "Config::default_random_max_elements")]
    pub random_max_elements: usize,

    #[serde(default)]
    pub count_unique_elements: bool,
}
impl Config {
    const fn default_random_min_elements() -> usize { 5 }
    const fn default_random_max_elements() -> usize { 10 }
}


#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PreprocessedData {
    pub lowercase_elements_1: HashMap<char, usize>,
    pub lowercase_elements_2: HashMap<(char, char), usize>,
}


#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct State<'s> {
    pub element_indexes: Vec<usize>,
    pub remaining_string: &'s str,
}


fn preprocess_data() -> PreprocessedData {
    let mut lowercase_elements_1 = HashMap::new();
    let mut lowercase_elements_2 = HashMap::new();
    for (i, element) in ELEMENTS.iter().enumerate() {
        let lowercase_chars: Vec<char> = element.to_lowercase().chars().collect();
        if lowercase_chars.len() == 1 {
            lowercase_elements_1.insert(lowercase_chars[0], i);
        } else {
            assert_eq!(lowercase_chars.len(), 2);
            lowercase_elements_2.insert((lowercase_chars[0], lowercase_chars[1]), i);
        }
    }
    PreprocessedData {
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


pub struct ElementaryPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    rng: Mutex<StdRng>,
    preprocessed_data: PreprocessedData,
}
#[async_trait]
impl RocketBotPlugin for ElementaryPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let config_object: Config = serde_json::from_value(config)
            .expect("failed to parse config");
        let config_lock = RwLock::new(
            "ElementaryPlugin::config",
            config_object,
        );

        let std_rng = StdRng::from_entropy();
        let rng = Mutex::new(
            "ElementaryPlugin::rng",
            std_rng,
        );

        let preprocessed_data = preprocess_data();
        ElementaryPlugin {
            interface,
            config: config_lock,
            rng,
            preprocessed_data,
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

        if let Some(found_element_indexes) = find_elements(&normalized_letters_body, &self.preprocessed_data) {
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
                    output_message.push_str(ELEMENTS[element_index]);
                }
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &output_message,
                ).await;
            }
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
        let mut config_guard = self.config.write().await;
        *config_guard = config_object;
        true
    }
}


#[cfg(test)]
mod tests {
    use super::{find_elements, preprocess_data};

    #[test]
    fn test_find_elements() {
        let data = preprocess_data();
        assert_eq!(find_elements("", &data).unwrap().len(), 0);
        assert_eq!(find_elements("h", &data).unwrap(), [0]);
        assert_eq!(find_elements("hehe", &data).unwrap(), [1, 1]);
        assert_eq!(find_elements("cowpokes", &data).unwrap(), [26, 73, 83, 18, 98]);
        assert!(find_elements("cowpoked", &data).is_none());
    }
}
