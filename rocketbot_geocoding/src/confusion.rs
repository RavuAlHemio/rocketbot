use rand::{Rng, RngCore, SeedableRng};
use rand::rngs::StdRng;
use regex::Regex;
use rocketbot_interface::ResultExtensions;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;


#[derive(Clone, Debug)]
struct ConfusionConfig {
    pub detect_regex: Regex,
    pub strip_diacritics_first: bool,
    pub replacement: String,
    pub probability: f64,
}
impl ConfusionConfig {
    pub fn new(
        detect_regex: Regex,
        strip_diacritics_first: bool,
        replacement: String,
        probability: f64,
    ) -> Self {
        Self {
            detect_regex,
            strip_diacritics_first,
            replacement,
            probability,
        }
    }
}

pub(crate) struct Confuser {
    confusions: Vec<ConfusionConfig>,
}
impl Confuser {
    pub(crate) fn new(config: &serde_json::Value) -> Result<Self, &'static str> {
        let confusions = if config["confusions"].is_null() {
            Vec::new()
        } else {
            let confusion_list = config["confusions"]
                .as_array().ok_or("confusions is not an array")?;
            let mut confusions = Vec::with_capacity(confusion_list.len());
            for confusion_config in confusion_list {
                let detect_regex_str = confusion_config["detect_regex"]
                    .as_str().ok_or("detect_regex is not a string")?;
                let detect_regex = Regex::new(detect_regex_str)
                    .or_msg("failed to parse detect_regex")?;
                let strip_diacritics_first = confusion_config["strip_diacritics_first"]
                    .as_bool().ok_or("strip_diacritics_first missing or not a bool")?;
                let replacement = confusion_config["replacement"]
                    .as_str().ok_or("replacement missing or not a string")?
                    .to_owned();
                let probability = confusion_config["probability"]
                    .as_f64().ok_or("probability missing or not an f64")?;
                confusions.push(ConfusionConfig::new(
                    detect_regex,
                    strip_diacritics_first,
                    replacement,
                    probability,
                ))
            }
            confusions
        };

        Ok(Self {
            confusions,
        })
    }

    pub(crate) fn confuse_with_rng<R: RngCore>(&self, place: &str, mut rng: R) -> String {
        let stripped_place: String = place.nfd()
            .filter(|c| !is_combining_mark(*c))
            .collect();

        for confusion in &self.confusions {
            let my_place = if confusion.strip_diacritics_first {
                stripped_place.as_str()
            } else {
                place
            };

            if confusion.detect_regex.is_match(my_place) {
                // perform replacement?
                let replace_value: f64 = rng.gen();
                if replace_value >= confusion.probability {
                    // no
                    continue;
                }

                // yes
                return confusion.replacement.clone();
            }
        }

        // no change
        place.to_owned()
    }

    pub(crate) fn confuse(&self, place: &str) -> String {
        let rng = StdRng::from_entropy();
        self.confuse_with_rng(place, rng)
    }
}
