use std::collections::HashMap;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::Local;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use unicode_normalization::UnicodeNormalization;


const MIN_LENGTH: usize = 5;
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
    preprocessed_data: PreprocessedData,
}
#[async_trait]
impl RocketBotPlugin for ElementaryPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let preprocessed_data = preprocess_data();
        ElementaryPlugin {
            interface,
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

        if let Some(found_element_indexes) = find_elements(&normalized_letters_body, &self.preprocessed_data) {
            if found_element_indexes.len() >= MIN_LENGTH {
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

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        // not much to update
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
